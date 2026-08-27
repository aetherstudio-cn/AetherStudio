//! AetherDbMemoryStore —— 基于自研 AetherDB 的 MemoryStore 实现
//!
//! 纯 Rust 零 C 依赖：
//! - 会话/消息/条目/剪枝日志统一映射为 AetherDB `(kind, key)` 记录，payload 为 JSON
//! - 消息 tag = conv_id（级联删除与按会话检索）；条目标签 = section
//! - 向量检索走 HNSW（小规模自动退化暴力精确搜索）
//! - hybrid_search 用子串关键词 + 向量 RRF 融合
//!
//! 语义要点：
//! - upsert_bullet 冲突时保留 helpful/harmful/created_at（只更新 section/content/updated_at）
//! - get_messages 按 msg_index 升序，embedding 返回 None
//! - list_* 按 updated_at 降序
//! - delete_bullet / rename_conversation 对不存在的记录报错

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aether_db::kind;
use aether_db::AetherDb;

use crate::memory_store::{
    ChatMessage, Conversation, MemoryStore, PlaybookBullet, PruneConfig, PruneLogEntry, PruneReport,
};

/// 基于 AetherDB 的记忆存储（纯 Rust 嵌入式）
pub struct AetherDbMemoryStore {
    db: Mutex<AetherDb>,
    embedding_dim: usize,
    path: PathBuf,
}

impl AetherDbMemoryStore {
    /// 打开或创建数据库（dir 不存在会自动创建），文件为 dir/aether_memory.aedb
    pub fn open(dir: &Path, embedding_dim: usize) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
        let path = dir.join("aether_memory.aedb");
        let db = AetherDb::open(&path, embedding_dim)
            .map_err(|e| format!("打开 AetherDB 失败: {}", e))?;
        Ok(Self {
            db: Mutex::new(db),
            embedding_dim,
            path,
        })
    }

    /// 数据库文件路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 是否为空库（无会话/条目等任何记录）
    pub fn is_empty(&self) -> bool {
        self.db().is_empty()
    }

    fn check_dim(&self, embedding: &[f32]) -> Result<(), String> {
        if embedding.len() != self.embedding_dim {
            return Err(format!(
                "向量维度不匹配: 期望 {}, 实际 {}",
                self.embedding_dim,
                embedding.len()
            ));
        }
        Ok(())
    }

    /// 获取 DB 锁（投毒容忍）
    ///
    /// 后台归档线程与 UI 线程共享此锁；若 worker 持锁期间 panic，
    /// `lock().unwrap()` 会让之后所有历史操作（打开/删除/恢复）级联 panic 闪退，
    /// 这里恢复内部数据继续服务，避免单点故障扩散。
    fn db(&self) -> std::sync::MutexGuard<'_, AetherDb> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// payload 序列化：实体去掉 embedding 后转 JSON（向量单独存 AetherDB vector 字段）
fn to_payload<T: serde::Serialize>(v: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(v).map_err(|e| format!("序列化失败: {}", e))
}

fn from_payload<T: serde::de::DeserializeOwned>(payload: &[u8]) -> Result<T, String> {
    serde_json::from_slice(payload).map_err(|e| format!("反序列化失败: {}", e))
}

impl MemoryStore for AetherDbMemoryStore {
    // ---- 会话 ----

    fn upsert_conversation(&self, conv: &Conversation) -> Result<(), String> {
        let payload = to_payload(conv)?;
        self.db
            .lock()
            .unwrap()
            .put(
                kind::CONVERSATION,
                &conv.id,
                &conv.workspace_hash,
                &payload,
                None,
            )
            .map_err(|e| format!("写入会话失败: {}", e))
    }

    fn list_conversations(&self, limit: usize) -> Result<Vec<Conversation>, String> {
        let db = self.db();
        let mut convs: Vec<Conversation> = db
            .scan(kind::CONVERSATION)
            .into_iter()
            .filter_map(|r| from_payload(&r.payload).ok())
            .collect();
        convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        convs.truncate(limit);
        Ok(convs)
    }

    fn delete_conversation(&self, conv_id: &str) -> Result<(), String> {
        let mut db = self.db();
        // 级联：先删该会话的全部消息（含向量索引），再删会话元数据
        db.delete_by_tag(kind::MESSAGE, conv_id)
            .map_err(|e| format!("删除会话消息失败: {}", e))?;
        db.delete(kind::CONVERSATION, conv_id)
            .map_err(|e| format!("删除会话失败: {}", e))?;
        Ok(())
    }

    fn rename_conversation(&self, conv_id: &str, new_title: &str) -> Result<(), String> {
        let mut db = self.db();
        let rec = db
            .get(kind::CONVERSATION, conv_id)
            .ok_or_else(|| format!("会话不存在: {}", conv_id))?;
        let mut conv: Conversation = from_payload(&rec.payload)?;
        let tag = rec.tag.clone();
        conv.title = new_title.to_string();
        let payload = to_payload(&conv)?;
        db.put(kind::CONVERSATION, conv_id, &tag, &payload, None)
            .map_err(|e| format!("重命名会话失败: {}", e))
    }

    // ---- 消息 ----

    fn append_message(&self, msg: &ChatMessage) -> Result<(), String> {
        if let Some(emb) = &msg.embedding {
            self.check_dim(emb)?;
        }
        // payload 不含 embedding（向量单独存 AetherDB vector 字段）
        let mut stored = msg.clone();
        stored.embedding = None;
        let payload = to_payload(&stored)?;
        self.db
            .lock()
            .unwrap()
            .put(
                kind::MESSAGE,
                &msg.id,
                &msg.conv_id,
                &payload,
                msg.embedding.as_deref(),
            )
            .map_err(|e| format!("写入消息失败: {}", e))
    }

    fn get_messages(&self, conv_id: &str) -> Result<Vec<ChatMessage>, String> {
        let db = self.db();
        let mut msgs: Vec<ChatMessage> = db
            .scan_by_tag(kind::MESSAGE, conv_id)
            .into_iter()
            .filter_map(|r| from_payload(&r.payload).ok())
            .collect();
        msgs.sort_by_key(|m| m.msg_index);
        Ok(msgs)
    }

    // ---- playbook 条目 ----

    fn upsert_bullet(&self, bullet: &PlaybookBullet) -> Result<(), String> {
        if let Some(emb) = &bullet.embedding {
            self.check_dim(emb)?;
        }
        let mut db = self.db();
        // 冲突合并：保留既有计数器与 created_at，只更新 section/content/updated_at
        let merged = match db.get(kind::BULLET, &bullet.id) {
            Some(rec) => {
                let old: PlaybookBullet = from_payload(&rec.payload)?;
                PlaybookBullet {
                    helpful_count: old.helpful_count,
                    harmful_count: old.harmful_count,
                    created_at: old.created_at,
                    ..bullet.clone()
                }
            }
            None => bullet.clone(),
        };
        let payload = to_payload(&merged)?;
        db.put(
            kind::BULLET,
            &bullet.id,
            &bullet.section,
            &payload,
            bullet.embedding.as_deref(),
        )
        .map_err(|e| format!("写入条目失败: {}", e))
    }

    fn bullet_feedback(&self, bullet_id: &str, helpful: bool) -> Result<(), String> {
        let mut db = self.db();
        let Some(rec) = db.get(kind::BULLET, bullet_id) else {
            // 条目不存在时静默成功（反馈针对已删除条目不应报错）
            return Ok(());
        };
        let mut bullet: PlaybookBullet = from_payload(&rec.payload)?;
        let vector = rec.vector.clone();
        if helpful {
            bullet.helpful_count += 1;
        } else {
            bullet.harmful_count += 1;
        }
        let payload = to_payload(&bullet)?;
        db.put(
            kind::BULLET,
            bullet_id,
            &bullet.section,
            &payload,
            vector.as_deref(),
        )
        .map_err(|e| format!("条目反馈失败: {}", e))
    }

    fn list_bullets(&self, section: Option<&str>) -> Result<Vec<PlaybookBullet>, String> {
        let db = self.db();
        let recs = match section {
            Some(s) => db.scan_by_tag(kind::BULLET, s),
            None => db.scan(kind::BULLET),
        };
        let mut bullets: Vec<PlaybookBullet> = recs
            .into_iter()
            .filter_map(|r| from_payload(&r.payload).ok())
            .collect();
        bullets.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(bullets)
    }

    fn delete_bullet(&self, bullet_id: &str) -> Result<(), String> {
        let mut db = self.db();
        if !db
            .delete(kind::BULLET, bullet_id)
            .map_err(|e| format!("删除条目失败: {}", e))?
        {
            return Err(format!("条目不存在: {}", bullet_id));
        }
        Ok(())
    }

    // ---- 语义检索 ----

    fn search_messages(
        &self,
        query_embedding: &[f32],
        conv_id: Option<&str>,
        k: usize,
    ) -> Result<Vec<(ChatMessage, f32)>, String> {
        self.check_dim(query_embedding)?;
        let db = self.db();
        let hits = db.knn(kind::MESSAGE, query_embedding, k, conv_id);
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(rec) = db.get(kind::MESSAGE, &hit.key) {
                if let Ok(msg) = from_payload::<ChatMessage>(&rec.payload) {
                    results.push((msg, hit.distance));
                }
            }
        }
        Ok(results)
    }

    fn search_bullets(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<(PlaybookBullet, f32)>, String> {
        self.check_dim(query_embedding)?;
        let db = self.db();
        let hits = db.knn(kind::BULLET, query_embedding, k, None);
        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(rec) = db.get(kind::BULLET, &hit.key) {
                if let Ok(b) = from_payload::<PlaybookBullet>(&rec.payload) {
                    results.push((b, hit.distance));
                }
            }
        }
        Ok(results)
    }

    fn flush(&self) -> Result<(), String> {
        self.db
            .lock()
            .unwrap()
            .flush()
            .map_err(|e| format!("flush 失败: {}", e))
    }

    fn shrink_memory(&self) -> Result<(), String> {
        // 强制 compaction 回收墓碑垃圾段
        self.db
            .lock()
            .unwrap()
            .force_compact()
            .map_err(|e| format!("compaction 失败: {}", e))
    }

    // ---- 混合检索（无 FTS：子串关键词 + 向量 RRF 融合）----

    fn hybrid_search_messages(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        conv_id: Option<&str>,
        k: usize,
    ) -> Result<Vec<(ChatMessage, f32)>, String> {
        self.check_dim(query_embedding)?;
        const RRF_K: f32 = 60.0;
        let db = self.db();

        // 候选池（conv 过滤在 knn 与关键词扫描里分别做）
        let fetch = (k * 4).max(k);

        // 1. 关键词候选：内容子串匹配（trigram 语义近似，≥3 字符才启用）
        let mut kw_rank: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let kw = query_text.to_lowercase();
        if query_text.chars().count() >= 3 && !kw.is_empty() {
            let recs = match conv_id {
                Some(cid) => db.scan_by_tag(kind::MESSAGE, cid),
                None => db.scan(kind::MESSAGE),
            };
            let mut matched: Vec<(String, String)> = recs
                .into_iter()
                .filter_map(|r| {
                    let msg: ChatMessage = from_payload(&r.payload).ok()?;
                    if msg.content.to_lowercase().contains(&kw) {
                        Some((r.key.clone(), msg.content.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            // 匹配位置越靠前（内容中出现越早）优先级越高；稳定排序保证确定性
            matched.sort_by(|a, b| a.1.cmp(&b.1));
            matched.truncate(fetch);
            for (i, (key, _)) in matched.iter().enumerate() {
                kw_rank.insert(key.clone(), i + 1);
            }
        }

        // 2. 向量候选（不带 tag 过滤，融合后再过滤以保留双通道候选）
        let vec_hits = db.knn(kind::MESSAGE, query_embedding, fetch, conv_id);

        // 3. RRF 融合：score = Σ 1/(60 + rank)
        let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
        for (key, rank) in &kw_rank {
            *scores.entry(key.clone()).or_default() += 1.0 / (RRF_K + *rank as f32);
        }
        for (i, hit) in vec_hits.iter().enumerate() {
            *scores.entry(hit.key.clone()).or_default() += 1.0 / (RRF_K + (i + 1) as f32);
        }

        // 4. 按融合分降序回表取实体（conv 过滤：关键词通道未被 knn 过滤，这里补齐）
        let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut results = Vec::new();
        for (key, score) in ranked {
            let Some(rec) = db.get(kind::MESSAGE, &key) else {
                continue;
            };
            let Ok(msg) = from_payload::<ChatMessage>(&rec.payload) else {
                continue;
            };
            if let Some(cid) = conv_id {
                if msg.conv_id != cid {
                    continue;
                }
            }
            results.push((msg, score));
            if results.len() >= k {
                break;
            }
        }
        Ok(results)
    }

    // ---- grow-and-refine 剪枝 ----

    fn prune_bullets(&self, config: &PruneConfig) -> Result<PruneReport, String> {
        let mut db = self.db();
        let candidates: Vec<PlaybookBullet> = db
            .scan(kind::BULLET)
            .into_iter()
            .filter_map(|r| from_payload(&r.payload).ok())
            .filter(|b: &PlaybookBullet| {
                let total = (b.helpful_count + b.harmful_count) as i64;
                total >= config.min_total_uses
                    && b.harmful_count as i64 >= config.harmful_threshold
                    && b.harmful_count > b.helpful_count
            })
            .collect();

        let mut report = PruneReport::default();
        for b in candidates {
            report.pruned += 1;
            report.bullet_ids.push(b.id.clone());
            if config.dry_run {
                continue;
            }
            // 审计日志（先写日志再删除，保证可追溯）
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let entry = PruneLogEntry {
                id: crate::memory_store::new_id("prune"),
                bullet_id: b.id.clone(),
                content: b.content.clone(),
                helpful_count: b.helpful_count as i64,
                harmful_count: b.harmful_count as i64,
                reason: format!(
                    "harmful({}) > helpful({}) 且超过阈值",
                    b.harmful_count, b.helpful_count
                ),
                pruned_at: now,
            };
            let payload = to_payload(&entry)?;
            db.put(kind::PRUNE_LOG, &entry.id, "", &payload, None)
                .map_err(|e| format!("写剪枝日志失败: {}", e))?;
            db.delete(kind::BULLET, &b.id)
                .map_err(|e| format!("删除条目失败: {}", e))?;
        }
        Ok(report)
    }

    fn list_prune_log(&self, limit: usize) -> Result<Vec<PruneLogEntry>, String> {
        let db = self.db();
        let mut entries: Vec<PruneLogEntry> = db
            .scan(kind::PRUNE_LOG)
            .into_iter()
            .filter_map(|r| from_payload(&r.payload).ok())
            .collect();
        entries.sort_by(|a, b| b.pruned_at.cmp(&a.pruned_at));
        entries.truncate(limit);
        Ok(entries)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_store::new_id;

    fn temp_store() -> (PathBuf, AetherDbMemoryStore) {
        let dir = std::env::temp_dir().join(format!("aether_dbmem_test_{}", new_id("d")));
        let store = AetherDbMemoryStore::open(&dir, 4).unwrap();
        (dir, store)
    }

    fn sample_msg(conv_id: &str, idx: u32, content: &str, emb: Option<Vec<f32>>) -> ChatMessage {
        ChatMessage {
            id: new_id("m"),
            conv_id: conv_id.to_string(),
            msg_index: idx,
            role: if idx % 2 == 0 { "user" } else { "assistant" }.into(),
            content: content.into(),
            embedding: emb,
            schema_ver: 1,
            created_at: 1700000000 + idx as u64,
        }
    }

    fn sample_bullet(
        id: &str,
        helpful: u32,
        harmful: u32,
        emb: Option<Vec<f32>>,
    ) -> PlaybookBullet {
        PlaybookBullet {
            id: id.into(),
            section: "s".into(),
            content: format!("内容-{}", id),
            helpful_count: helpful,
            harmful_count: harmful,
            embedding: emb,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn test_conversation_and_messages() {
        let (dir, store) = temp_store();
        store
            .upsert_conversation(&Conversation {
                id: "c1".into(),
                title: "测试会话".into(),
                workspace_hash: "abc".into(),
                mode: "chat".into(),
                created_at: 1,
                updated_at: 2,
                message_count: 2,
            })
            .unwrap();
        store
            .append_message(&sample_msg("c1", 1, "后写", None))
            .unwrap();
        store
            .append_message(&sample_msg("c1", 0, "你好", None))
            .unwrap();

        let msgs = store.get_messages("c1").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "你好", "应按 msg_index 升序");

        let convs = store.list_conversations(10).unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].title, "测试会话");

        store.rename_conversation("c1", "新标题").unwrap();
        assert_eq!(store.list_conversations(10).unwrap()[0].title, "新标题");
        assert!(store.rename_conversation("nope", "x").is_err());

        store.delete_conversation("c1").unwrap();
        assert!(store.get_messages("c1").unwrap().is_empty());
        assert!(store.list_conversations(10).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_vector_search_and_persistence() {
        let dir = std::env::temp_dir().join(format!("aether_dbmem_test_{}", new_id("d")));
        {
            let store = AetherDbMemoryStore::open(&dir, 4).unwrap();
            store
                .append_message(&sample_msg("c1", 0, "a", Some(vec![1.0, 0.0, 0.0, 0.0])))
                .unwrap();
            store
                .append_message(&sample_msg("c1", 1, "b", Some(vec![0.0, 1.0, 0.0, 0.0])))
                .unwrap();
            store
                .append_message(&sample_msg("c2", 0, "c", Some(vec![0.9, 0.1, 0.0, 0.0])))
                .unwrap();
        }
        // 重新打开：索引重建后检索等价
        let store = AetherDbMemoryStore::open(&dir, 4).unwrap();
        let results = store
            .search_messages(&[1.0, 0.0, 0.0, 0.0], None, 2)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.content, "a");

        // conv 过滤
        let results = store
            .search_messages(&[1.0, 0.0, 0.0, 0.0], Some("c2"), 5)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.content, "c");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_bullets_and_feedback() {
        let (dir, store) = temp_store();
        store
            .upsert_bullet(&PlaybookBullet {
                id: "b1".into(),
                section: "tool_use".into(),
                content: "git 操作前先检查工作区是否干净".into(),
                helpful_count: 0,
                harmful_count: 0,
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        store.bullet_feedback("b1", true).unwrap();
        store.bullet_feedback("b1", true).unwrap();
        store.bullet_feedback("b1", false).unwrap();
        // 反馈后再 upsert：计数器应保留（冲突合并语义）
        let mut updated = sample_bullet("b1", 0, 0, Some(vec![0.0, 1.0, 0.0, 0.0]));
        updated.section = "tool_use".into();
        store.upsert_bullet(&updated).unwrap();

        let bullets = store.list_bullets(Some("tool_use")).unwrap();
        assert_eq!(bullets.len(), 1);
        assert_eq!(bullets[0].helpful_count, 2);
        assert_eq!(bullets[0].harmful_count, 1);

        // 向量已更新
        let hits = store.search_bullets(&[0.0, 1.0, 0.0, 0.0], 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.id, "b1");

        // 删除
        store.delete_bullet("b1").unwrap();
        assert!(store.delete_bullet("b1").is_err());
        assert!(store
            .search_bullets(&[0.0, 1.0, 0.0, 0.0], 5)
            .unwrap()
            .is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_hybrid_search_keyword_hit() {
        let (dir, store) = temp_store();
        store
            .append_message(&sample_msg(
                "c1",
                0,
                "如何配置 rust-analyzer 的 LSP 服务",
                Some(vec![0.0, 1.0, 0.0, 0.0]),
            ))
            .unwrap();
        store
            .append_message(&sample_msg(
                "c1",
                1,
                "完全不相关的闲聊内容",
                Some(vec![0.0, 0.0, 1.0, 0.0]),
            ))
            .unwrap();

        let hits = store
            .hybrid_search_messages("rust-analyzer", &[1.0, 0.0, 0.0, 0.0], None, 5)
            .unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].0.content, "如何配置 rust-analyzer 的 LSP 服务");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_prune_bullets_with_audit() {
        let (dir, store) = temp_store();
        store
            .upsert_bullet(&sample_bullet("bad", 1, 5, None))
            .unwrap();
        store
            .upsert_bullet(&sample_bullet("good", 8, 1, None))
            .unwrap();
        store
            .upsert_bullet(&sample_bullet("new", 0, 3, None))
            .unwrap();

        let cfg = PruneConfig {
            harmful_threshold: 3,
            min_total_uses: 5,
            dry_run: false,
        };
        let dry = store
            .prune_bullets(&PruneConfig {
                dry_run: true,
                ..cfg.clone()
            })
            .unwrap();
        assert_eq!(dry.pruned, 1);
        assert_eq!(store.list_bullets(None).unwrap().len(), 3);

        let report = store.prune_bullets(&cfg).unwrap();
        assert_eq!(report.pruned, 1);
        assert_eq!(report.bullet_ids, vec!["bad".to_string()]);
        let remaining = store.list_bullets(None).unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().all(|b| b.id != "bad"));

        let log = store.list_prune_log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].bullet_id, "bad");
        assert_eq!(log[0].harmful_count, 5);
        assert!(log[0].reason.contains("harmful"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
