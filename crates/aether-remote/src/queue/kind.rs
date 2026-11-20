//! AetherDB kind 命名空间定义（任务队列专用）

/// 任务元数据（状态、优先级、依赖等）
pub const TASK_META: u8 = 10;

/// 任务执行结果
pub const TASK_RESULT: u8 = 11;

/// 任务执行日志（追加式）
pub const TASK_LOG: u8 = 12;

/// 队列状态快照（用于恢复）
pub const QUEUE_STATE: u8 = 13;

/// 死信队列
pub const DEAD_LETTER: u8 = 14;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_values() {
        assert_eq!(TASK_META, 10);
        assert_eq!(TASK_RESULT, 11);
        assert_eq!(TASK_LOG, 12);
        assert_eq!(QUEUE_STATE, 13);
        assert_eq!(DEAD_LETTER, 14);
    }
}
