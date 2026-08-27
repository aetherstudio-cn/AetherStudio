//! 删除撤销栈：记录最近的文件删除操作，支持 Ctrl+Z 从内存/快照/回收站恢复。
//!
//! 策略（按优先级）：
//! - 文件暂存：删除时不关闭已打开的标签页，内容缓存在内存中（标记
//!   `deleted_from_disk`），Ctrl+Z 时优先从标签页缓冲区直接回写磁盘；
//! - 内容快照：删除时将文件内容（≤ 阈值）存入 `DeleteRecord.content`，
//!   标签页已关闭时仍可回写恢复；
//! - 回收站兜底：删除操作走 Windows 回收站（`SHFileOperationW + FOF_ALLOWUNDO`），
//!   无快照时（如目录删除）仍可从系统回收站还原。

use std::path::PathBuf;
use std::time::Instant;

/// 单次删除记录
#[derive(Clone, Debug)]
pub struct DeleteRecord {
    /// 被删除文件/文件夹的原始绝对路径
    pub original_path: PathBuf,
    /// 删除时刻（用于淘汰过期记录）
    pub timestamp: Instant,
    /// 删除时刻的文件内容快照（仅文件且有缓存来源时有值）；
    /// 目录删除或超大文件为 None，此时依赖回收站恢复
    pub content: Option<String>,
}

/// 撤销最近一次删除：弹出栈顶记录，由调用方按内存缓存 → 快照 → 回收站顺序恢复。
/// 返回 Some(记录) 表示有记录可撤销；None 表示栈空。
pub fn pop_last_delete(stack: &mut Vec<DeleteRecord>) -> Option<DeleteRecord> {
    // 淘汰超过 5 分钟的过期记录（回收站仍有，但不再提供快速撤销入口）
    let now = Instant::now();
    stack.retain(|r| now.duration_since(r.timestamp).as_secs() < 300);
    stack.pop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_returns_full_record_with_snapshot() {
        let mut stack = Vec::new();
        stack.push(DeleteRecord {
            original_path: PathBuf::from("D:/ws/a.rs"),
            timestamp: Instant::now(),
            content: Some("fn main() {}".to_string()),
        });
        let record = pop_last_delete(&mut stack).unwrap();
        assert_eq!(record.original_path, PathBuf::from("D:/ws/a.rs"));
        assert_eq!(record.content.as_deref(), Some("fn main() {}"));
        assert!(stack.is_empty());
    }

    #[test]
    fn pop_empty_stack_returns_none() {
        let mut stack: Vec<DeleteRecord> = Vec::new();
        assert!(pop_last_delete(&mut stack).is_none());
    }

    #[test]
    fn pop_evicts_expired_records() {
        let mut stack = Vec::new();
        // 构造一条已过期记录（timestamp 回拨 6 分钟）
        stack.push(DeleteRecord {
            original_path: PathBuf::from("D:/ws/old.rs"),
            timestamp: Instant::now()
                .checked_sub(std::time::Duration::from_secs(360))
                .unwrap(),
            content: None,
        });
        assert!(pop_last_delete(&mut stack).is_none(), "过期记录应被淘汰");
        assert!(stack.is_empty());
    }
}
