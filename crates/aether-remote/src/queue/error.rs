//! 任务队列错误类型定义

use thiserror::Error;

/// 任务队列错误
#[derive(Debug, Error)]
pub enum QueueError {
    #[error("任务验证失败: {0}")]
    InvalidTask(String),

    #[error("队列已满: 当前容量 {0}")]
    QueueFull(usize),

    #[error("任务不存在: {0}")]
    TaskNotFound(String),

    #[error("重复任务: {0}")]
    DuplicateTask(String),

    #[error("依赖循环: {0}")]
    CircularDependency(String),

    #[error("AetherDB 错误: {0}")]
    DbError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("执行错误: {0}")]
    ExecutionError(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("网络错误: {0}")]
    NetworkError(String),

    #[error("限流: {0}")]
    RateLimit(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<aether_db::DbError> for QueueError {
    fn from(e: aether_db::DbError) -> Self {
        QueueError::DbError(e.to_string())
    }
}

impl From<serde_json::Error> for QueueError {
    fn from(e: serde_json::Error) -> Self {
        QueueError::SerializationError(e.to_string())
    }
}

impl From<std::io::Error> for QueueError {
    fn from(e: std::io::Error) -> Self {
        QueueError::Internal(e.to_string())
    }
}

/// 错误类型分类（用于重试策略）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    Network,
    Timeout,
    RateLimit,
    Execution,
    Serialization,
    Database,
    Unknown,
}

impl QueueError {
    /// 获取错误类型
    pub fn error_type(&self) -> ErrorType {
        match self {
            QueueError::NetworkError(_) => ErrorType::Network,
            QueueError::Timeout(_) => ErrorType::Timeout,
            QueueError::RateLimit(_) => ErrorType::RateLimit,
            QueueError::ExecutionError(_) => ErrorType::Execution,
            QueueError::SerializationError(_) => ErrorType::Serialization,
            QueueError::DbError(_) => ErrorType::Database,
            _ => ErrorType::Unknown,
        }
    }

    /// 判断是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.error_type(),
            ErrorType::Network | ErrorType::Timeout | ErrorType::RateLimit
        )
    }
}

pub type Result<T> = std::result::Result<T, QueueError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_type_classification() {
        let err = QueueError::NetworkError("connection failed".to_string());
        assert_eq!(err.error_type(), ErrorType::Network);
        assert!(err.is_retryable());

        let err = QueueError::InvalidTask("bad task".to_string());
        assert_eq!(err.error_type(), ErrorType::Unknown);
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_conversion() {
        let db_err = aether_db::DbError("db error".to_string());
        let queue_err: QueueError = db_err.into();
        assert!(matches!(queue_err, QueueError::DbError(_)));
    }
}
