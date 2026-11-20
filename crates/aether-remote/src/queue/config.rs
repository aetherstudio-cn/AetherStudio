//! 任务队列配置

use std::path::PathBuf;
use std::time::Duration;

use super::retry::RetryPolicy;
use super::supabase::SupabaseConfig;

/// 任务队列配置
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// AetherDB 文件路径
    pub db_path: PathBuf,
    
    /// 最大并发任务数
    pub max_concurrent: usize,
    
    /// 内存队列最大容量
    pub max_queue_size: usize,
    
    /// 任务保留天数
    pub retention_days: i64,
    
    /// 重试策略
    pub retry_policy: RetryPolicy,
    
    /// Supabase 配置
    pub supabase_config: SupabaseConfig,
    
    /// 租户 ID
    pub tenant_id: String,
    
    /// 用户 ID
    pub user_id: String,
    
    /// 设备名称
    pub device_name: String,
    
    /// 设备类型
    pub device_type: String,
    
    /// Realtime 重连间隔
    pub realtime_reconnect_interval: Duration,
    
    /// 轮询间隔（降级模式）
    pub polling_interval: Duration,
    
    /// 维护任务间隔
    pub maintenance_interval: Duration,
    
    /// 心跳间隔
    pub heartbeat_interval: Duration,
    
    /// 是否启用监控
    pub enable_metrics: bool,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("data/tasks.aedb"),
            max_concurrent: 4,
            max_queue_size: 1000,
            retention_days: 30,
            retry_policy: RetryPolicy::default(),
            supabase_config: SupabaseConfig::default(),
            tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            device_name: "AetherStudio PC".to_string(),
            device_type: std::env::consts::OS.to_string(),
            realtime_reconnect_interval: Duration::from_secs(5),
            polling_interval: Duration::from_secs(30),
            maintenance_interval: Duration::from_secs(3600), // 1 小时
            heartbeat_interval: Duration::from_secs(30),
            enable_metrics: true,
        }
    }
}

impl QueueConfig {
    /// 创建新配置
    pub fn new(db_path: PathBuf, device_name: String) -> Self {
        Self {
            db_path,
            device_name,
            ..Default::default()
        }
    }

    /// 设置最大并发数
    pub fn with_max_concurrent(mut self, max_concurrent: usize) -> Self {
        self.max_concurrent = max_concurrent;
        self
    }

    /// 设置队列容量
    pub fn with_max_queue_size(mut self, max_queue_size: usize) -> Self {
        self.max_queue_size = max_queue_size;
        self
    }

    /// 设置保留天数
    pub fn with_retention_days(mut self, days: i64) -> Self {
        self.retention_days = days;
        self
    }

    /// 设置重试策略
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// 设置 Realtime 重连间隔
    pub fn with_realtime_reconnect_interval(mut self, interval: Duration) -> Self {
        self.realtime_reconnect_interval = interval;
        self
    }

    /// 设置轮询间隔
    pub fn with_polling_interval(mut self, interval: Duration) -> Self {
        self.polling_interval = interval;
        self
    }

    /// 设置维护间隔
    pub fn with_maintenance_interval(mut self, interval: Duration) -> Self {
        self.maintenance_interval = interval;
        self
    }

    /// 设置是否启用监控
    pub fn with_enable_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QueueConfig::default();
        
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.device_name, "AetherStudio PC");
        assert!(config.enable_metrics);
    }

    #[test]
    fn test_custom_config() {
        let config = QueueConfig::new(
            PathBuf::from("/tmp/test.aedb"),
            "device-1".to_string(),
        )
        .with_max_concurrent(8)
        .with_max_queue_size(2000)
        .with_retention_days(60)
        .with_enable_metrics(false);
        
        assert_eq!(config.max_concurrent, 8);
        assert_eq!(config.max_queue_size, 2000);
        assert_eq!(config.retention_days, 60);
        assert_eq!(config.device_name, "device-1");
        assert!(!config.enable_metrics);
    }

    #[test]
    fn test_builder_pattern() {
        let config = QueueConfig::default()
            .with_max_concurrent(16)
            .with_polling_interval(Duration::from_secs(60));
        
        assert_eq!(config.max_concurrent, 16);
        assert_eq!(config.polling_interval, Duration::from_secs(60));
    }
}
