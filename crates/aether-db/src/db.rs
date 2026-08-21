//! AetherDb 主体：记录模型、内存索引（tag 二级索引 + 按 kind 分域 HNSW）、
//! 崩溃恢复重建、自动 compaction。
//!
//! 上层（MemoryStore 适配器）以 `(kind, key)` 为主键读写 JSON payload，
//! 向量检索通过 [`AetherDb::knn`] 完成。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::hnsw::{cosine_distance, Hnsw};
use crate::storage::{encode_segment, fold_segments, DbError, Segment, Storage};

/// 一条存活记录
#[derive(Clone, Debug)]
pub struct Record {
    pub key: String,
    pub tag: String,
    pub payload: Vec<u8>,
    pub vector: Option<Vec<f32>>,
}

/// KNN 命中结果（distance 越小越相似）
#[derive(Clone, Debug)]
pub struct KnnHit {
    pub key: String,
    pub distance: f32,
}

/// 单文件嵌入式向量数据库
pub struct AetherDb {
    storage: Storage,
    dim: usize,
    /// (kind, key) -> 存活记录
    records: HashMap<(u8, String), Record>,
    /// (kind, tag) -> key 集合（二级索引）
    tag_index: HashMap<(u8, String), HashSet<String>>,
    /// 按 kind 分域的向量索引
    hnsw: HashMap<u8, Hnsw>,
    /// (kind, key) -> HNSW 节点 id
    node_of: HashMap<(u8, String), usize>,
    /// 自增向量 id（HNSW 外部 key），与 (kind, key) 双向映射
    next_vec_id: u64,
    id_of: HashMap<(u8, String), u64>,
    key_of_id: HashMap<u64, (u8, String)>,
}

impl AetherDb {
    /// 打开或创建数据库文件，全量扫描重建内存索引
    pub fn open(path: &Path, dim: usize) -> Result<Self, DbError> {
        let (mut storage, segments) = Storage::open(path, dim)?;
        let (live, garbage) = fold_segments(segments);
        storage.garbage = garbage;

        let mut db = Self {
            storage,
            dim,
            records: HashMap::new(),
            tag_index: HashMap::new(),
            hnsw: HashMap::new(),
            node_of: HashMap::new(),
            next_vec_id: 0,
            id_of: HashMap::new(),
            key_of_id: HashMap::new(),
        };
        // 按偏移顺序重建，保证 HNSW 结构确定性
        let mut ordered: Vec<&Segment> = live.values().collect();
        ordered.sort_by_key(|s| s.offset);
        for seg in ordered {
            db.index_record(
                seg.kind,
                Record {
                    key: seg.key.clone(),
                    tag: seg.tag.clone(),
                    payload: seg.payload.clone(),
                    vector: seg.vector.clone(),
                },
            );
        }
        Ok(db)
    }

    /// 记录总数（存活）
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn path(&self) -> &Path {
        &self.storage.path
    }

    /// 把记录放入内存索引（records/tag_index/hnsw），不写盘
    fn index_record(&mut self, kind: u8, rec: Record) {
        let kk = (kind, rec.key.clone());
        self.tag_index
            .entry((kind, rec.tag.clone()))
            .or_default()
            .insert(rec.key.clone());
        if let Some(v) = &rec.vector {
            let id = self.next_vec_id;
            self.next_vec_id += 1;
            let node = self
                .hnsw
                .entry(kind)
                .or_insert_with(|| Hnsw::new(self.dim))
                .insert(id, v);
            self.node_of.insert(kk.clone(), node);
            self.id_of.insert(kk.clone(), id);
            self.key_of_id.insert(id, kk.clone());
        }
        self.records.insert(kk, rec);
    }

    /// 从内存索引中移除记录（并计垃圾字节）
    fn unindex_record(&mut self, kind: u8, key: &str) {
        let kk = (kind, key.to_string());
        if let Some(rec) = self.records.remove(&kk) {
            // 段大小可由内容精确还原（定长头 + 各字段 + CRC）
            let vec_len = rec.vector.as_ref().map(|v| v.len()).unwrap_or(0);
            let old_size = 16 + rec.tag.len() + rec.key.len() + rec.payload.len() + vec_len * 4 + 4;
            self.storage.add_garbage(old_size as u64);
            if let Some(keys) = self.tag_index.get_mut(&(kind, rec.tag.clone())) {
                keys.remove(key);
                if keys.is_empty() {
                    self.tag_index.remove(&(kind, rec.tag));
                }
            }
            if let Some(node) = self.node_of.remove(&kk) {
                if let Some(h) = self.hnsw.get_mut(&kind) {
                    h.mark_deleted(node);
                }
            }
            if let Some(id) = self.id_of.remove(&kk) {
                self.key_of_id.remove(&id);
            }
        }
    }

    /// 写入/覆盖一条记录（追加段 + 更新索引；旧版本计为垃圾）
    pub fn put(
        &mut self,
        kind: u8,
        key: &str,
        tag: &str,
        payload: &[u8],
        vector: Option<&[f32]>,
    ) -> Result<(), DbError> {
        if let Some(v) = vector {
            if v.len() != self.dim {
                return Err(DbError(format!(
                    "向量维度不匹配: 传入 {} 维，数据库 {} 维",
                    v.len(),
                    self.dim
                )));
            }
        }
        self.unindex_record(kind, key);
        let buf = encode_segment(kind, false, tag, key, payload, vector);
        self.storage.append(&buf)?;
        self.index_record(
            kind,
            Record {
                key: key.to_string(),
                tag: tag.to_string(),
                payload: payload.to_vec(),
                vector: vector.map(|v| v.to_vec()),
            },
        );
        self.maybe_compact()?;
        Ok(())
    }

    /// 删除一条记录（追加墓碑段）。返回是否存在
    pub fn delete(&mut self, kind: u8, key: &str) -> Result<bool, DbError> {
        if !self.records.contains_key(&(kind, key.to_string())) {
            return Ok(false);
        }
        self.unindex_record(kind, key);
        let buf = encode_segment(kind, true, "", key, &[], None);
        self.storage.append(&buf)?;
        self.maybe_compact()?;
        Ok(true)
    }

    /// 删除某 kind 下所有 tag 匹配的记录（级联删除）
    pub fn delete_by_tag(&mut self, kind: u8, tag: &str) -> Result<usize, DbError> {
        let keys: Vec<String> = self
            .tag_index
            .get(&(kind, tag.to_string()))
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        let n = keys.len();
        for key in keys {
            self.delete(kind, &key)?;
        }
        Ok(n)
    }

    /// 读取单条记录
    pub fn get(&self, kind: u8, key: &str) -> Option<&Record> {
        self.records.get(&(kind, key.to_string()))
    }

    /// 遍历某 kind 下全部记录
    pub fn scan(&self, kind: u8) -> Vec<&Record> {
        self.records
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, r)| r)
            .collect()
    }

    /// 遍历某 kind 下 tag 匹配的记录
    pub fn scan_by_tag(&self, kind: u8, tag: &str) -> Vec<&Record> {
        self.tag_index
            .get(&(kind, tag.to_string()))
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| self.records.get(&(kind, k.clone())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 遍历某 kind 下 key 以 prefix 开头的记录（复合键前缀检索，如 "conv-1/"）
    pub fn scan_prefix(&self, kind: u8, prefix: &str) -> Vec<&Record> {
        self.records
            .iter()
            .filter(|((k, key), _)| *k == kind && key.starts_with(prefix))
            .map(|(_, r)| r)
            .collect()
    }

    /// KNN 检索。tag_filter 非 None 时只返回该 tag 下的记录。
    ///
    /// 小集合（含 tag 过滤后 ≤4096 条）走暴力精确搜索；大集合走 HNSW
    /// 过量取 k*4 后过滤。
    pub fn knn(&self, kind: u8, query: &[f32], k: usize, tag_filter: Option<&str>) -> Vec<KnnHit> {
        if k == 0 {
            return Vec::new();
        }
        // tag 过滤且集合较小：对集合内向量暴力精确搜索
        if let Some(tag) = tag_filter {
            let recs = self.scan_by_tag(kind, tag);
            if recs.len() <= 4096 {
                let mut hits: Vec<KnnHit> = recs
                    .iter()
                    .filter_map(|r| {
                        r.vector.as_ref().map(|v| KnnHit {
                            key: r.key.clone(),
                            distance: cosine_distance(query, v),
                        })
                    })
                    .collect();
                hits.sort_by(|a, b| {
                    a.distance
                        .partial_cmp(&b.distance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                hits.truncate(k);
                return hits;
            }
        }
        let Some(h) = self.hnsw.get(&kind) else {
            return Vec::new();
        };
        let fetch = if tag_filter.is_some() { k * 4 } else { k };
        let ef = (fetch * 2).max(64);
        let raw = h.search(query, fetch, ef);
        let mut hits: Vec<KnnHit> = raw
            .into_iter()
            .filter_map(|(id, d)| {
                let kk = self.key_of_id.get(&id)?;
                let rec = self.records.get(kk)?;
                if let Some(tag) = tag_filter {
                    if rec.tag != tag {
                        return None;
                    }
                }
                Some(KnnHit {
                    key: rec.key.clone(),
                    distance: d,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);
        hits
    }

    /// 显式同步（追加写已 flush，此为上层层级同步点）
    pub fn flush(&mut self) -> Result<(), DbError> {
        self.storage.flush()
    }

    /// 垃圾达阈值时 compaction：用当前存活记录重写文件（索引无需变化）
    fn maybe_compact(&mut self) -> Result<(), DbError> {
        if !self.storage.needs_compaction() {
            return Ok(());
        }
        self.force_compact()
    }

    /// 强制 compaction（测试与维护工具可用）
    pub fn force_compact(&mut self) -> Result<(), DbError> {
        let mut segs: Vec<Vec<u8>> = Vec::with_capacity(self.records.len());
        let mut ordered: Vec<(&u8, &Record)> =
            self.records.iter().map(|((k, _), r)| (k, r)).collect();
        ordered.sort_by(|a, b| a.1.key.cmp(&b.1.key));
        for (kind, rec) in ordered {
            segs.push(encode_segment(
                *kind,
                false,
                &rec.tag,
                &rec.key,
                &rec.payload,
                rec.vector.as_deref(),
            ));
        }
        self.storage.compact(&segs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "aether_db_test_{}_{}.aedb",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn test_put_get_delete() {
        let p = tmp_path("basic");
        {
            let mut db = AetherDb::open(&p, 4).unwrap();
            db.put(1, "k1", "t1", b"{\"a\":1}", Some(&[1.0, 0.0, 0.0, 0.0]))
                .unwrap();
            let r = db.get(1, "k1").unwrap();
            assert_eq!(r.payload, b"{\"a\":1}");
            assert_eq!(r.tag, "t1");

            // 覆盖写
            db.put(1, "k1", "t2", b"{\"a\":2}", Some(&[0.0, 1.0, 0.0, 0.0]))
                .unwrap();
            let r = db.get(1, "k1").unwrap();
            assert_eq!(r.payload, b"{\"a\":2}");
            assert_eq!(r.tag, "t2");
            assert_eq!(db.scan_by_tag(1, "t1").len(), 0, "旧 tag 索引应清除");

            assert!(db.delete(1, "k1").unwrap());
            assert!(db.get(1, "k1").is_none());
            assert!(!db.delete(1, "k1").unwrap());
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_persistence_across_reopen() {
        let p = tmp_path("reopen");
        {
            let mut db = AetherDb::open(&p, 3).unwrap();
            db.put(1, "a", "g1", b"pa", Some(&[1.0, 0.0, 0.0])).unwrap();
            db.put(1, "b", "g1", b"pb", Some(&[0.0, 1.0, 0.0])).unwrap();
            db.put(2, "c", "g2", b"pc", None).unwrap();
            db.delete(1, "b").unwrap();
        }
        // 重新打开：墓碑生效，其余恢复
        let db = AetherDb::open(&p, 3).unwrap();
        assert_eq!(db.len(), 2);
        assert!(db.get(1, "b").is_none());
        assert_eq!(db.get(1, "a").unwrap().payload, b"pa");
        assert_eq!(db.scan(2).len(), 1);
        let hits = db.knn(1, &[0.9, 0.1, 0.0], 5, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "a");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_knn_with_tag_filter() {
        let p = tmp_path("knn_tag");
        let mut db = AetherDb::open(&p, 3).unwrap();
        // 两个会话各两条消息
        db.put(2, "m1", "convA", b"", Some(&[1.0, 0.0, 0.0]))
            .unwrap();
        db.put(2, "m2", "convA", b"", Some(&[0.0, 1.0, 0.0]))
            .unwrap();
        db.put(2, "m3", "convB", b"", Some(&[0.99, 0.01, 0.0]))
            .unwrap();
        db.put(2, "m4", "convB", b"", Some(&[0.0, 0.0, 1.0]))
            .unwrap();

        let hits = db.knn(2, &[1.0, 0.0, 0.0], 10, Some("convA"));
        assert_eq!(hits.len(), 2, "过滤后只剩 convA");
        assert_eq!(hits[0].key, "m1");

        let hits = db.knn(2, &[1.0, 0.0, 0.0], 1, None);
        assert_eq!(hits[0].key, "m1", "全局最近应是 m1");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_delete_by_tag_cascade() {
        let p = tmp_path("cascade");
        let mut db = AetherDb::open(&p, 2).unwrap();
        db.put(2, "m1", "conv1", b"x", Some(&[1.0, 0.0])).unwrap();
        db.put(2, "m2", "conv1", b"y", Some(&[0.0, 1.0])).unwrap();
        db.put(2, "m3", "conv2", b"z", Some(&[1.0, 1.0])).unwrap();

        assert_eq!(db.delete_by_tag(2, "conv1").unwrap(), 2);
        assert_eq!(db.len(), 1);
        assert!(db.knn(2, &[1.0, 0.0], 5, Some("conv1")).is_empty());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_compaction_preserves_data() {
        let p = tmp_path("compact");
        {
            let mut db = AetherDb::open(&p, 2).unwrap();
            // 反复覆盖制造垃圾
            for i in 0..50 {
                let payload = format!("{{\"v\":{}}}", i);
                db.put(1, "hot", "t", payload.as_bytes(), Some(&[i as f32, 1.0]))
                    .unwrap();
            }
            db.put(1, "keep", "t", b"stay", None).unwrap();
            db.force_compact().unwrap();
            assert_eq!(db.get(1, "hot").unwrap().payload, b"{\"v\":49}");
        }
        // compaction 后重新打开仍完整
        let db = AetherDb::open(&p, 2).unwrap();
        assert_eq!(db.len(), 2);
        assert_eq!(db.get(1, "hot").unwrap().payload, b"{\"v\":49}");
        assert_eq!(db.get(1, "keep").unwrap().payload, b"stay");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn test_dim_mismatch_rejected() {
        let p = tmp_path("dim");
        let mut db = AetherDb::open(&p, 4).unwrap();
        assert!(db.put(1, "k", "", b"", Some(&[1.0, 0.0])).is_err());
        let _ = std::fs::remove_file(&p);
    }
}
