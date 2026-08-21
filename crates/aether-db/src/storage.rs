//! AetherDB 存储层：单文件格式、段编解码、崩溃恢复、compaction
//!
//! 文件布局：
//! ```text
//! [4KB Header]  magic / version / dim（计数器不信任，恢复靠全量扫描）
//! [数据段...]   追加式写入；删除/覆盖产生垃圾段（逻辑 Free List）
//! ```
//!
//! 段格式（全部小端）：
//! ```text
//! magic u32 | kind u8 | flags u8 | tag_len u16 | key_len u16
//! payload_len u32 | vec_len u16(元素数)  ……共 16B 定长头
//! tag bytes | key bytes | payload bytes | vector(f32 LE) bytes
//! crc32 u32（覆盖段头到向量尾的全部字节）
//! ```

use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 文件头大小（固定 4KB）
pub const HEADER_SIZE: u64 = 4096;
/// 文件魔数："AEDB"
pub const FILE_MAGIC: u32 = 0x4244_4541;
/// 段魔数
pub const SEG_MAGIC: u32 = 0xAEDB_0101;
/// 当前文件格式版本
pub const FILE_VERSION: u32 = 1;
/// flags bit0：墓碑段（删除标记）
pub const FLAG_TOMBSTONE: u8 = 0x01;
/// 段定长头大小
pub const SEG_HEADER_LEN: usize = 16;

/// 统一错误类型（字符串消息，与上层风格一致）
#[derive(Debug)]
pub struct DbError(pub String);

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError(e.to_string())
    }
}

/// 从文件中扫描出的原始段
#[derive(Clone)]
pub struct Segment {
    pub offset: u64,
    pub size: u64,
    pub kind: u8,
    pub tombstone: bool,
    pub tag: String,
    pub key: String,
    pub payload: Vec<u8>,
    pub vector: Option<Vec<f32>>,
}

/// IEEE CRC32（无外部依赖的位迭代实现；段数量级下性能足够）
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// 编码一个数据段（含尾部 CRC）
pub fn encode_segment(
    kind: u8,
    tombstone: bool,
    tag: &str,
    key: &str,
    payload: &[u8],
    vector: Option<&[f32]>,
) -> Vec<u8> {
    let tag_b = tag.as_bytes();
    let key_b = key.as_bytes();
    let vec_len = vector.map(|v| v.len()).unwrap_or(0);
    let total = SEG_HEADER_LEN + tag_b.len() + key_b.len() + payload.len() + vec_len * 4 + 4;
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&SEG_MAGIC.to_le_bytes());
    buf.push(kind);
    buf.push(if tombstone { FLAG_TOMBSTONE } else { 0 });
    buf.extend_from_slice(&(tag_b.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(key_b.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(vec_len as u16).to_le_bytes());
    buf.extend_from_slice(tag_b);
    buf.extend_from_slice(key_b);
    buf.extend_from_slice(payload);
    if let Some(v) = vector {
        for f in v {
            buf.extend_from_slice(&f.to_le_bytes());
        }
    }
    let crc = crc32(&buf);
    buf.extend_from_slice(&crc.to_le_bytes());
    buf
}

/// 尝试从 data[pos..] 解码一个段；失败返回 None（截断/损坏）
pub fn decode_segment(data: &[u8], pos: usize) -> Option<Segment> {
    let rest = data.get(pos..)?;
    if rest.len() < SEG_HEADER_LEN {
        return None;
    }
    let magic = u32::from_le_bytes(rest[0..4].try_into().ok()?);
    if magic != SEG_MAGIC {
        return None;
    }
    let kind = rest[4];
    let flags = rest[5];
    let tag_len = u16::from_le_bytes(rest[6..8].try_into().ok()?) as usize;
    let key_len = u16::from_le_bytes(rest[8..10].try_into().ok()?) as usize;
    let payload_len = u32::from_le_bytes(rest[10..14].try_into().ok()?) as usize;
    let vec_len = u16::from_le_bytes(rest[14..16].try_into().ok()?) as usize;
    let body_len = tag_len + key_len + payload_len + vec_len * 4;
    let total = SEG_HEADER_LEN + body_len + 4;
    if rest.len() < total {
        return None;
    }
    // CRC 校验（覆盖段头到向量尾）
    let expect_crc = u32::from_le_bytes(rest[total - 4..total].try_into().ok()?);
    if crc32(&rest[..total - 4]) != expect_crc {
        return None;
    }
    let mut p = SEG_HEADER_LEN;
    let tag = String::from_utf8(rest[p..p + tag_len].to_vec()).ok()?;
    p += tag_len;
    let key = String::from_utf8(rest[p..p + key_len].to_vec()).ok()?;
    p += key_len;
    let payload = rest[p..p + payload_len].to_vec();
    p += payload_len;
    let mut vector = None;
    if vec_len > 0 {
        let mut v = Vec::with_capacity(vec_len);
        for i in 0..vec_len {
            let b = rest[p + i * 4..p + i * 4 + 4].try_into().ok()?;
            v.push(f32::from_le_bytes(b));
        }
        vector = Some(v);
    }
    Some(Segment {
        offset: pos as u64,
        size: total as u64,
        kind,
        tombstone: flags & FLAG_TOMBSTONE != 0,
        tag,
        key,
        payload,
        vector,
    })
}

/// 写 4KB 文件头（计数器仅做诊断用途，恢复逻辑不依赖它）
fn write_header(file: &mut std::fs::File, dim: u32) -> std::io::Result<()> {
    let mut h = vec![0u8; HEADER_SIZE as usize];
    h[0..4].copy_from_slice(&FILE_MAGIC.to_le_bytes());
    h[4..8].copy_from_slice(&FILE_VERSION.to_le_bytes());
    h[8..12].copy_from_slice(&dim.to_le_bytes());
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&h)?;
    Ok(())
}

/// 存储层：持有文件句柄与空间统计
pub struct Storage {
    pub path: PathBuf,
    file: std::fs::File,
    /// 数据区末尾偏移（= 文件长度）
    pub data_end: u64,
    /// 垃圾字节数（死段累计，compaction 触发依据）
    pub garbage: u64,
}

impl Storage {
    /// 打开或创建文件，mmap 全量扫描重建段列表（崩溃恢复：尾部损坏段直接截断）
    pub fn open(path: &Path, dim: usize) -> Result<(Self, Vec<Segment>), DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        let file_len = file.metadata()?.len();

        let mut segments = Vec::new();
        let mut data_end = HEADER_SIZE;
        if file_len >= HEADER_SIZE {
            // mmap 只读映射加速恢复扫描（文件小则退化为直接读）
            let data = if file_len > HEADER_SIZE {
                let map = unsafe { memmap2::MmapOptions::new().map(&file) }
                    .map_err(|e| DbError(format!("mmap 映射失败: {}", e)))?;
                map.to_vec()
            } else {
                Vec::new()
            };
            // Header 校验：magic 与 dim 必须匹配（计数器不信任）
            if data.len() >= 12 {
                let magic = u32::from_le_bytes(data[0..4].try_into().unwrap());
                let stored_dim = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
                if magic != FILE_MAGIC {
                    return Err(DbError(format!(
                        "不是 AetherDB 文件（magic 不匹配）: {}",
                        path.display()
                    )));
                }
                if stored_dim != dim {
                    return Err(DbError(format!(
                        "向量维度不匹配: 文件 {} 维，运行时期望 {} 维",
                        stored_dim, dim
                    )));
                }
            }
            // 全量扫描：连续有效段保留，首个损坏/截断段起丢弃（尾部写中断的恢复策略）
            let mut pos = HEADER_SIZE as usize;
            while pos < data.len() {
                match decode_segment(&data, pos) {
                    Some(seg) => {
                        pos += seg.size as usize;
                        segments.push(seg);
                    }
                    None => break,
                }
            }
            data_end = pos as u64;
            // 尾部有损坏字节：截断文件到最后一个有效段（下次写入即覆盖）
            if data_end < file_len {
                file.set_len(data_end)?;
            }
        } else if file_len == 0 {
            write_header(&mut file, dim as u32)?;
            file.flush()?;
        } else {
            return Err(DbError("文件损坏：长度不足以容纳文件头".to_string()));
        }

        file.seek(SeekFrom::Start(data_end))?;
        Ok((
            Self {
                path: path.to_path_buf(),
                file,
                data_end,
                garbage: 0,
            },
            segments,
        ))
    }

    /// 追加一个段并落盘；返回写入偏移与大小
    pub fn append(&mut self, seg: &[u8]) -> Result<(u64, u64), DbError> {
        let off = self.data_end;
        self.file.write_all(seg)?;
        self.file.flush()?;
        self.data_end += seg.len() as u64;
        Ok((off, seg.len() as u64))
    }

    /// 累加垃圾字节；达到阈值触发 compaction（重写存活段 + 原子替换）
    pub fn add_garbage(&mut self, bytes: u64) {
        self.garbage += bytes;
    }

    /// 是否需要压缩：垃圾超过 1MB 且占比超 1/3
    pub fn needs_compaction(&self) -> bool {
        let live = self.data_end.saturating_sub(HEADER_SIZE);
        self.garbage > 1024 * 1024 && self.garbage * 3 > live
    }

    /// compaction：用存活段重写临时文件，fsync 后原子替换原文件
    pub fn compact(&mut self, live_segments: &[Vec<u8>]) -> Result<(), DbError> {
        let tmp_path = self.path.with_extension("aedb.tmp");
        {
            let mut tmp = std::fs::File::create(&tmp_path)?;
            // 维度从原文件头读出，保持一致
            let mut head = [0u8; 12];
            use std::io::Read;
            self.file.seek(SeekFrom::Start(0))?;
            self.file.read_exact(&mut head)?;
            let dim = u32::from_le_bytes(head[8..12].try_into().unwrap());
            write_header(&mut tmp, dim)?;
            for seg in live_segments {
                tmp.write_all(seg)?;
            }
            tmp.flush()?;
            tmp.sync_all()?;
        }
        std::fs::rename(&tmp_path, &self.path)
            .map_err(|e| DbError(format!("compaction 替换文件失败: {}", e)))?;
        // 重新打开文件句柄
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)?;
        let new_end = file.metadata()?.len();
        file.seek(SeekFrom::Start(new_end))?;
        self.file = file;
        self.data_end = new_end;
        self.garbage = 0;
        Ok(())
    }

    /// flush 文件（段追加时已 flush，此处为显式同步点）
    pub fn flush(&mut self) -> Result<(), DbError> {
        self.file.flush()?;
        Ok(())
    }
}

/// 按 (kind, key) 折叠段序列：后写覆盖先写，墓碑表示删除；返回存活记录与垃圾字节
pub fn fold_segments(segments: Vec<Segment>) -> (HashMap<(u8, String), Segment>, u64) {
    let mut live: HashMap<(u8, String), Segment> = HashMap::new();
    let mut garbage = 0u64;
    for seg in segments {
        let k = (seg.kind, seg.key.clone());
        match live.remove(&k) {
            Some(old) => {
                // 旧版本成为垃圾
                garbage += old.size;
            }
            None => {}
        }
        if seg.tombstone {
            // 墓碑本身也占空间，计入垃圾（不保留）
            garbage += seg.size;
            continue;
        }
        live.insert(k, seg);
    }
    (live, garbage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_roundtrip() {
        let payload = b"{\"hello\": 1}";
        let vec = vec![0.1f32, -0.5, 1.0];
        let buf = encode_segment(2, false, "conv-1", "msg-42", payload, Some(&vec));
        let seg = decode_segment(&buf, 0).expect("解码失败");
        assert_eq!(seg.kind, 2);
        assert_eq!(seg.tag, "conv-1");
        assert_eq!(seg.key, "msg-42");
        assert_eq!(seg.payload, payload);
        assert_eq!(seg.vector, Some(vec));
        assert!(!seg.tombstone);
    }

    #[test]
    fn test_segment_crc_detects_corruption() {
        let mut buf = encode_segment(1, false, "", "k", b"data", None);
        let mid = buf.len() / 2;
        buf[mid] ^= 0xFF; // 翻转一位
        assert!(decode_segment(&buf, 0).is_none(), "损坏段应被 CRC 拒绝");
    }

    #[test]
    fn test_truncated_segment_rejected() {
        let buf = encode_segment(1, false, "", "k", b"data", None);
        assert!(decode_segment(&buf[..buf.len() - 3], 0).is_none());
    }

    #[test]
    fn test_fold_overwrite_and_tombstone() {
        let s1 = Segment {
            offset: 0,
            size: 10,
            kind: 1,
            tombstone: false,
            tag: "".into(),
            key: "a".into(),
            payload: b"v1".to_vec(),
            vector: None,
        };
        let mut s2 = s1.clone();
        s2.payload = b"v2".to_vec();
        s2.offset = 10;
        let mut t = s1.clone();
        t.tombstone = true;
        t.offset = 20;

        let (live, garbage) = fold_segments(vec![s1.clone(), s2.clone()]);
        assert_eq!(live.len(), 1);
        assert_eq!(live[&(1, "a".to_string())].payload, b"v2");
        assert_eq!(garbage, 10);

        let (live, garbage) = fold_segments(vec![s1, s2, t]);
        assert!(live.is_empty(), "墓碑后记录应删除");
        assert_eq!(garbage, 30);
    }
}
