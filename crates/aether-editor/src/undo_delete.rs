//! 删除撤销栈：记录最近的文件删除操作，支持 Ctrl+Z 从回收站恢复。
//!
//! 策略：
//! - 删除操作走 Windows 回收站（`SHFileOperationW + FOF_ALLOWUNDO`），
//!   文件实际在回收站中仍可被系统恢复。
//! - 应用级记录 `original_path`，Ctrl+Z 时通过 Shell 命名空间
//!   定位回收站中的对应项并还原到原位。
//! - 简化实现：第一期不做 Shell Namespace 枚举，而是记录路径后
//!   由用户手动去回收站恢复。Ctrl+Z 仅提示用户"已移至回收站，
//!   请右键回收站还原"。后续可升级为自动还原。

use std::path::PathBuf;
use std::time::Instant;

/// 单次删除记录
#[derive(Clone, Debug)]
pub struct DeleteRecord {
    /// 被删除文件/文件夹的原始绝对路径
    pub original_path: PathBuf,
    /// 删除时刻（用于淘汰过期记录）
    pub timestamp: Instant,
}

/// 撤销最近一次删除：弹出栈顶记录，提示用户可从回收站恢复。
/// 返回 Some(路径) 表示有记录可撤销；None 表示栈空。
pub fn pop_last_delete(stack: &mut Vec<DeleteRecord>) -> Option<PathBuf> {
    // 淘汰超过 5 分钟的过期记录（回收站仍有，但不再提供快速撤销入口）
    let now = Instant::now();
    stack.retain(|r| now.duration_since(r.timestamp).as_secs() < 300);
    stack.pop().map(|r| r.original_path)
}
