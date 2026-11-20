//! 监控指标实现

use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts, Registry};
use std::time::Duration;

/// 队列监控指标
pub struct QueueMetrics {
    registry: Registry,
    
    // 计数器
    pub tasks_submitted: Counter,
    pub tasks_completed: Counter,
    pub tasks_failed: Counter,
    pub tasks_cancelled: Counter,
    pub tasks_retried: Counter,
    
    // 仪表盘
    pub queue_size: Gauge,
    pub running_tasks: Gauge,
    pub available_slots: Gauge,
    
    // 直方图
    pub execution_duration: Histogram,
    pub queue_wait_duration: Histogram,
}

impl QueueMetrics {
    /// 创建新的监控指标
    pub fn new() -> Self {
        let registry = Registry::new();
        
        let tasks_submitted = Counter::with_opts(
            Opts::new("queue_tasks_submitted_total", "Total number of tasks submitted")
        ).unwrap();
        
        let tasks_completed = Counter::with_opts(
            Opts::new("queue_tasks_completed_total", "Total number of tasks completed")
        ).unwrap();
        
        let tasks_failed = Counter::with_opts(
            Opts::new("queue_tasks_failed_total", "Total number of tasks failed")
        ).unwrap();
        
        let tasks_cancelled = Counter::with_opts(
            Opts::new("queue_tasks_cancelled_total", "Total number of tasks cancelled")
        ).unwrap();
        
        let tasks_retried = Counter::with_opts(
            Opts::new("queue_tasks_retried_total", "Total number of tasks retried")
        ).unwrap();
        
        let queue_size = Gauge::with_opts(
            Opts::new("queue_size", "Current queue size")
        ).unwrap();
        
        let running_tasks = Gauge::with_opts(
            Opts::new("queue_running_tasks", "Current number of running tasks")
        ).unwrap();
        
        let available_slots = Gauge::with_opts(
            Opts::new("queue_available_slots", "Current number of available execution slots")
        ).unwrap();
        
        let execution_duration = Histogram::with_opts(
            HistogramOpts::new("queue_execution_duration_seconds", "Task execution duration")
                .buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0])
        ).unwrap();
        
        let queue_wait_duration = Histogram::with_opts(
            HistogramOpts::new("queue_wait_duration_seconds", "Task queue wait duration")
                .buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0])
        ).unwrap();
        
        // 注册指标
        registry.register(Box::new(tasks_submitted.clone())).unwrap();
        registry.register(Box::new(tasks_completed.clone())).unwrap();
        registry.register(Box::new(tasks_failed.clone())).unwrap();
        registry.register(Box::new(tasks_cancelled.clone())).unwrap();
        registry.register(Box::new(tasks_retried.clone())).unwrap();
        registry.register(Box::new(queue_size.clone())).unwrap();
        registry.register(Box::new(running_tasks.clone())).unwrap();
        registry.register(Box::new(available_slots.clone())).unwrap();
        registry.register(Box::new(execution_duration.clone())).unwrap();
        registry.register(Box::new(queue_wait_duration.clone())).unwrap();
        
        Self {
            registry,
            tasks_submitted,
            tasks_completed,
            tasks_failed,
            tasks_cancelled,
            tasks_retried,
            queue_size,
            running_tasks,
            available_slots,
            execution_duration,
            queue_wait_duration,
        }
    }

    /// 记录任务提交
    pub fn record_task_submitted(&self) {
        self.tasks_submitted.inc();
    }

    /// 记录任务完成
    pub fn record_task_completed(&self, duration: Duration) {
        self.tasks_completed.inc();
        self.execution_duration.observe(duration.as_secs_f64());
    }

    /// 记录任务失败
    pub fn record_task_failed(&self) {
        self.tasks_failed.inc();
    }

    /// 记录任务取消
    pub fn record_task_cancelled(&self) {
        self.tasks_cancelled.inc();
    }

    /// 记录任务重试
    pub fn record_task_retried(&self) {
        self.tasks_retried.inc();
    }

    /// 更新队列大小
    pub fn update_queue_size(&self, size: usize) {
        self.queue_size.set(size as f64);
    }

    /// 更新运行中任务数
    pub fn update_running_tasks(&self, count: usize) {
        self.running_tasks.set(count as f64);
    }

    /// 更新可用槽位数
    pub fn update_available_slots(&self, count: usize) {
        self.available_slots.set(count as f64);
    }

    /// 记录队列等待时长
    pub fn record_queue_wait(&self, duration: Duration) {
        self.queue_wait_duration.observe(duration.as_secs_f64());
    }

    /// 获取注册表（用于 HTTP 暴露）
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// 导出指标（Prometheus 格式）
    pub fn export(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for QueueMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = QueueMetrics::new();
        
        assert_eq!(metrics.tasks_submitted.get(), 0.0);
        assert_eq!(metrics.tasks_completed.get(), 0.0);
        assert_eq!(metrics.tasks_failed.get(), 0.0);
    }

    #[test]
    fn test_record_task_submitted() {
        let metrics = QueueMetrics::new();
        
        metrics.record_task_submitted();
        assert_eq!(metrics.tasks_submitted.get(), 1.0);
        
        metrics.record_task_submitted();
        assert_eq!(metrics.tasks_submitted.get(), 2.0);
    }

    #[test]
    fn test_record_task_completed() {
        let metrics = QueueMetrics::new();
        
        let duration = Duration::from_secs(2);
        metrics.record_task_completed(duration);
        
        assert_eq!(metrics.tasks_completed.get(), 1.0);
    }

    #[test]
    fn test_update_queue_size() {
        let metrics = QueueMetrics::new();
        
        metrics.update_queue_size(10);
        assert_eq!(metrics.queue_size.get(), 10.0);
        
        metrics.update_queue_size(5);
        assert_eq!(metrics.queue_size.get(), 5.0);
    }

    #[test]
    fn test_export() {
        let metrics = QueueMetrics::new();
        
        metrics.record_task_submitted();
        metrics.record_task_completed(Duration::from_secs(1));
        
        let exported = metrics.export();
        assert!(exported.contains("queue_tasks_submitted_total"));
        assert!(exported.contains("queue_tasks_completed_total"));
    }
}
