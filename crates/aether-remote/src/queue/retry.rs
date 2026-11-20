//! 重试策略实现

use std::time::Duration;

use super::error::{ErrorType, QueueError};

/// 重试策略
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub retryable_errors: Vec<ErrorType>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            retryable_errors: vec![ErrorType::Network, ErrorType::Timeout, ErrorType::RateLimit],
        }
    }
}

impl RetryPolicy {
    /// 创建新的重试策略
    pub fn new(max_attempts: usize) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    /// 判断是否应该重试
    pub fn should_retry(&self, error: &QueueError, attempt: usize) -> bool {
        if attempt >= self.max_attempts {
            return false;
        }

        self.retryable_errors
            .iter()
            .any(|t| error.error_type() == *t)
    }

    /// 计算下次重试延迟
    pub fn next_delay(&self, attempt: usize) -> Duration {
        let delay_secs =
            self.initial_delay.as_secs_f64() * self.backoff_multiplier.powi(attempt as i32);

        let capped_delay = delay_secs.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(capped_delay)
    }

    /// 添加可重试的错误类型
    pub fn add_retryable_error(mut self, error_type: ErrorType) -> Self {
        if !self.retryable_errors.contains(&error_type) {
            self.retryable_errors.push(error_type);
        }
        self
    }

    /// 设置初始延迟
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// 设置最大延迟
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// 设置退避倍数
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_delay, Duration::from_secs(1));
        assert_eq!(policy.max_delay, Duration::from_secs(60));
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy::default();

        let network_err = QueueError::NetworkError("connection failed".to_string());
        assert!(policy.should_retry(&network_err, 0));
        assert!(policy.should_retry(&network_err, 1));
        assert!(policy.should_retry(&network_err, 2));
        assert!(!policy.should_retry(&network_err, 3));

        let invalid_err = QueueError::InvalidTask("bad task".to_string());
        assert!(!policy.should_retry(&invalid_err, 0));
    }

    #[test]
    fn test_next_delay() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.next_delay(0), Duration::from_secs(1));
        assert_eq!(policy.next_delay(1), Duration::from_secs(2));
        assert_eq!(policy.next_delay(2), Duration::from_secs(4));
        assert_eq!(policy.next_delay(3), Duration::from_secs(8));

        // 测试最大延迟限制
        assert_eq!(policy.next_delay(10), Duration::from_secs(60));
    }

    #[test]
    fn test_custom_policy() {
        let policy = RetryPolicy::new(5)
            .with_initial_delay(Duration::from_millis(500))
            .with_max_delay(Duration::from_secs(30))
            .with_backoff_multiplier(3.0);

        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_delay, Duration::from_millis(500));
        assert_eq!(policy.max_delay, Duration::from_secs(30));
        assert_eq!(policy.backoff_multiplier, 3.0);

        assert_eq!(policy.next_delay(0), Duration::from_millis(500));
        assert_eq!(policy.next_delay(1), Duration::from_millis(1500));
        assert_eq!(policy.next_delay(2), Duration::from_millis(4500));
    }

    #[test]
    fn test_add_retryable_error() {
        let policy = RetryPolicy::default().add_retryable_error(ErrorType::Execution);

        let exec_err = QueueError::ExecutionError("command failed".to_string());
        assert!(policy.should_retry(&exec_err, 0));
    }

    #[test]
    fn test_max_delay_cap() {
        let policy = RetryPolicy::new(10)
            .with_initial_delay(Duration::from_secs(10))
            .with_max_delay(Duration::from_secs(15))
            .with_backoff_multiplier(2.0);

        assert_eq!(policy.next_delay(0), Duration::from_secs(10));
        assert_eq!(policy.next_delay(1), Duration::from_secs(15)); // capped
        assert_eq!(policy.next_delay(2), Duration::from_secs(15)); // capped
    }
}
