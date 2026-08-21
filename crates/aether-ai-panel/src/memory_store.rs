//! 记忆存储适配层（MemoryStore）
//!
//! 对话持久化 + ACE 上下文工程条目库的统一抽象。
//! 上层（ai_warm_data / ai_agent / reflector）只依赖 [`MemoryStore`] trait，
//! 底层实现可整体替换：
//!
//! - 当前实现：[`AetherDbMemoryStore`](crate::aether_db_store::AetherDbMemoryStore)
//!   （自研 AetherDB：纯 Rust 单文件 + HNSW 向量索引）
//! - 配套 [`JsonlSessionLog`]：VS Code 同款追加式会话日志（热数据）
//!
//! 设计原则（来自 ACE 论文 arXiv:2510.04618）：
//! - 条目化增量更新，禁止整体重写
//! - playbook 条目带 helpful/harmful 计数器，作为"权重"演化信号

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 默认向量维度（bge-small-zh-v1.5）
pub const DEFAULT_EMBEDDING_DIM: usize = 512;

// ============================================================================
// 数据结构
// ============================================================================

/// 会话元数据
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    /// 所属工作区哈希（VS Code workspaceStorage 同款绑定方式）
    pub workspace_hash: String,
    pub mode: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: u32,
}

/// 单条对话消息（Cursor bubble 模式：一条消息一行，带 schema 版本号）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conv_id: String,
    pub msg_index: u32,
    pub role: String,
    pub content: String,
    /// 语义检索向量（写入时可先为空，后台补齐）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub schema_ver: u32,
    pub created_at: u64,
}

/// ACE playbook 条目（权重沉淀的最小单元）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaybookBullet {
    pub id: String,
    /// 分类：tool_use / coding_style / pitfalls / project_facts ...
    pub section: String,
    pub content: String,
    /// “权重”：被引用且任务成功的次数
    pub helpful_count: u32,
    /// 被引用但产生负效果的次数
    pub harmful_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub created_at: u64,
    pub updated_at: u64,
}

// ============================================================================
// 适配器 trait —— 上层只依赖此接口
// ============================================================================

pub trait MemoryStore: Send + Sync {
    // ---- 会话 ----
    fn upsert_conversation(&self, conv: &Conversation) -> Result<(), String>;
    fn list_conversations(&self, limit: usize) -> Result<Vec<Conversation>, String>;
    fn delete_conversation(&self, conv_id: &str) -> Result<(), String>;
    /// 重命名会话标题（仅更新 title，不触碰 updated_at/message_count）
    fn rename_conversation(&self, conv_id: &str, new_title: &str) -> Result<(), String> {
        // 默认实现：读出 -> 改 title -> upsert 回写（注意会刷新 updated_at 由实现决定）
        let all = self.list_conversations(1_000_000)?;
        if let Some(mut c) = all.into_iter().find(|c| c.id == conv_id) {
            c.title = new_title.to_string();
            self.upsert_conversation(&c)?;
            Ok(())
        } else {
            Err(format!("会话不存在: {}", conv_id))
        }
    }
    /// 清空全部会话（级联删除消息与向量索引）；返回删除条数
    fn clear_all_conversations(&self) -> Result<usize, String> {
        let all = self.list_conversations(1_000_000)?;
        let n = all.len();
        for c in all {
            self.delete_conversation(&c.id)?;
        }
        Ok(n)
    }

    /// 清理无工作区绑定的历史会话（workspace_hash 为空）；返回删除条数
    fn clear_orphan_conversations(&self) -> Result<usize, String> {
        let all = self.list_conversations(1_000_000)?;
        let orphans: Vec<_> = all
            .into_iter()
            .filter(|c| c.workspace_hash.is_empty())
            .collect();
        let n = orphans.len();
        for c in orphans {
            self.delete_conversation(&c.id)?;
        }
        Ok(n)
    }

    // ---- 消息 ----
    fn append_message(&self, msg: &ChatMessage) -> Result<(), String>;
    fn get_messages(&self, conv_id: &str) -> Result<Vec<ChatMessage>, String>;

    // ---- playbook 条目（ACE 权重沉淀）----
    fn upsert_bullet(&self, bullet: &PlaybookBullet) -> Result<(), String>;
    /// 条目反馈计数（helpful=true 则 helpful_count+1，否则 harmful_count+1）
    fn bullet_feedback(&self, bullet_id: &str, helpful: bool) -> Result<(), String>;
    fn list_bullets(&self, section: Option<&str>) -> Result<Vec<PlaybookBullet>, String>;

    // ---- 语义检索（HNSW 向量索引；实现若无向量能力可返回空）----
    fn search_messages(
        &self,
        query_embedding: &[f32],
        conv_id: Option<&str>,
        k: usize,
    ) -> Result<Vec<(ChatMessage, f32)>, String>;
    fn search_bullets(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<(PlaybookBullet, f32)>, String>;

    fn flush(&self) -> Result<(), String>;

    // ---- 内存回收（冰冻态调用；无能力的实现可忽略）----
    /// 释放底层存储的可回收空间（触发 compaction 回收墓碑垃圾）
    fn shrink_memory(&self) -> Result<(), String> {
        Ok(())
    }

    // ---- 混合检索（关键词 + 向量，RRF 融合；默认退化为纯向量）----
    fn hybrid_search_messages(
        &self,
        _query_text: &str,
        query_embedding: &[f32],
        conv_id: Option<&str>,
        k: usize,
    ) -> Result<Vec<(ChatMessage, f32)>, String> {
        self.search_messages(query_embedding, conv_id, k)
    }

    // ---- 会话检索（关键词 + 工作区过滤；默认内存过滤）----
    fn search_conversations(
        &self,
        keyword: &str,
        workspace_hash: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Conversation>, String> {
        let kw = keyword.to_lowercase();
        let all = self.list_conversations(limit.max(500))?;
        Ok(all
            .into_iter()
            .filter(|c| {
                let ws_ok = workspace_hash
                    .map(|h| c.workspace_hash == h)
                    .unwrap_or(true);
                let kw_ok = kw.is_empty() || c.title.to_lowercase().contains(&kw);
                ws_ok && kw_ok
            })
            .take(limit)
            .collect())
    }

    // ---- playbook 管理 ----
    /// 更新条目内容/分类（保留计数器与 ID）
    fn update_bullet(&self, bullet: &PlaybookBullet) -> Result<(), String> {
        self.upsert_bullet(bullet)
    }
    /// 删除条目（含向量索引）
    fn delete_bullet(&self, _bullet_id: &str) -> Result<(), String> {
        Err("该存储实现不支持删除条目".to_string())
    }

    // ---- grow-and-refine 剪枝 ----
    /// 按配置剪枝高 harmful 条目，写审计日志；返回处理报告
    fn prune_bullets(&self, _config: &PruneConfig) -> Result<PruneReport, String> {
        Ok(PruneReport::default())
    }
    /// 查询剪枝审计日志
    fn list_prune_log(&self, _limit: usize) -> Result<Vec<PruneLogEntry>, String> {
        Ok(Vec::new())
    }
}

// ============================================================================
// grow-and-refine 剪枝配置与审计
// ============================================================================

/// 剪枝配置（阈值均可调）
#[derive(Clone, Debug)]
pub struct PruneConfig {
    /// harmful_count 达到该值才考虑剪枝
    pub harmful_threshold: i64,
    /// helpful + harmful 总数达到该值才评估（避免新条目被误删）
    pub min_total_uses: i64,
    /// 试运行：只返回候选，不实际删除、不写日志
    pub dry_run: bool,
}

impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            harmful_threshold: 3,
            min_total_uses: 5,
            dry_run: false,
        }
    }
}

/// 剪枝报告
#[derive(Clone, Debug, Default)]
pub struct PruneReport {
    /// 实际删除的条目数（dry_run 时为候选数）
    pub pruned: usize,
    pub bullet_ids: Vec<String>,
}

/// 剪枝审计日志条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PruneLogEntry {
    pub id: String,
    pub bullet_id: String,
    pub content: String,
    pub helpful_count: i64,
    pub harmful_count: i64,
    pub reason: String,
    pub pruned_at: u64,
}

// ============================================================================
// JSONL 会话日志（VS Code 同款热数据：每会话一个追加式文件）
// ============================================================================

/// VS Code chatSessions 同款：活跃会话的追加式 JSONL 日志。
/// 温数据归档（写入 AetherDB）成功后，对应日志文件可删除。
pub struct JsonlSessionLog {
    path: PathBuf,
    file: Mutex<std::fs::File>,
}

impl JsonlSessionLog {
    /// 打开（或创建）某个会话的日志文件：dir/session_<id>.jsonl
    pub fn open(dir: &Path, session_id: &str) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
        let path = dir.join(format!("session_{}.jsonl", session_id));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("打开会话日志失败: {}", e))?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    /// 追加一条消息（一行 JSON + 立即 flush，崩溃最多丢最后一行）
    pub fn append(&self, msg: &ChatMessage) -> Result<(), String> {
        use std::io::Write;
        let line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        let mut file = self.file.lock().unwrap();
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.flush())
            .map_err(|e| format!("写入会话日志失败: {}", e))
    }

    /// 重放日志，重建全部消息（VS Code mutation-replay 同款思路）
    pub fn read_all(&self) -> Result<Vec<ChatMessage>, String> {
        let content = std::fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(msg) = serde_json::from_str::<ChatMessage>(line) {
                out.push(msg);
            }
            // 损坏行跳过：追加写被中断时最后一行可能不完整
        }
        Ok(out)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ============================================================================
// 工具函数
// ============================================================================

/// 生成短唯一 ID（时间戳 + 进程内原子计数，无外部依赖）
pub fn new_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!(
        "{}-{:x}-{:x}",
        prefix,
        ts,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod jsonl_tests {
    use super::*;

    fn sample_msg(conv_id: &str, idx: u32, content: &str) -> ChatMessage {
        ChatMessage {
            id: new_id("m"),
            conv_id: conv_id.to_string(),
            msg_index: idx,
            role: "user".into(),
            content: content.into(),
            embedding: None,
            schema_ver: 1,
            created_at: 1700000000 + idx as u64,
        }
    }

    #[test]
    fn test_jsonl_session_log() {
        let dir = std::env::temp_dir().join(format!("aether_jsonl_test_{}", new_id("d")));
        let log = JsonlSessionLog::open(&dir, "s1").unwrap();
        log.append(&sample_msg("s1", 0, "第一条")).unwrap();
        log.append(&sample_msg("s1", 1, "第二条")).unwrap();

        let msgs = log.read_all().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].content, "第二条");
        std::fs::remove_dir_all(&dir).ok();
    }
}
