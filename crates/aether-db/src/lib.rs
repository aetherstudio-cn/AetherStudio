//! AetherDB — 自研轻量级嵌入式向量数据库（纯 Rust，零 C 依赖）
//!
//! 设计目标（纯 Rust 零外部依赖的嵌入式向量数据库）：
//! - 单文件持久化：4KB Header + 追加式数据段，段级 CRC32 校验，崩溃安全
//! - HNSW 近似最近邻向量索引（内存常驻，打开时全量重建；小规模自动退化暴力精确搜索）
//! - 标量过滤：kind 命名空间 + tag 二级索引（会话ID/分类等精确匹配）
//! - 向量相似度与标量条件联合检索（knn + tag 过滤）
//! - 空间回收：删除/覆盖产生垃圾段，达阈值后后台 compaction（重写+原子替换）
//!
//! 记录模型：`(kind, key) -> { tag, payload, vector? }`
//! - kind：业务命名空间（如 会话/消息/条目）
//! - key：主键（同 kind 内唯一，put 覆盖写）
//! - tag：二级索引字段（如消息的 conv_id、条目的 section）
//! - payload：业务自描述的 JSON 字节（AetherDB 不解析）
//! - vector：可选 f32 向量，进 HNSW 索引

mod db;
mod hnsw;
mod storage;

pub use db::{AetherDb, KnnHit, Record};
pub use storage::DbError;

/// 便捷别名：所有公开 API 统一用字符串错误，与上层 MemoryStore 风格一致
pub type Result<T> = std::result::Result<T, DbError>;

/// 记录命名空间（业务侧自定义常量，AetherDB 不解释语义）
pub mod kind {
    /// AI 会话元数据
    pub const CONVERSATION: u8 = 1;
    /// 对话消息
    pub const MESSAGE: u8 = 2;
    /// ACE playbook 条目
    pub const BULLET: u8 = 3;
    /// 剪枝审计日志
    pub const PRUNE_LOG: u8 = 4;
}
