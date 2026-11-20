//! 任务队列数据模型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 任务定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Option<Uuid>,
    pub title: String,
    pub command: String,
    pub status: TaskStatus,
    pub priority: i32,
    pub queue_name: String,
    pub dependencies: Vec<Uuid>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Task {
    /// 创建新任务
    pub fn new(tenant_id: Uuid, user_id: Uuid, title: String, command: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            device_id: None,
            title,
            command,
            status: TaskStatus::Draft,
            priority: 0,
            queue_name: "default".to_string(),
            dependencies: Vec::new(),
            retry_count: 0,
            max_retries: 3,
            timeout_seconds: 3600,
            scheduled_at: None,
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        }
    }

    /// 检查任务是否就绪（所有依赖已完成）
    pub fn is_ready(&self, completed_tasks: &[Uuid]) -> bool {
        self.dependencies
            .iter()
            .all(|dep| completed_tasks.contains(dep))
    }

    /// 检查任务是否可重试
    pub fn can_retry(&self) -> bool {
        self.status == TaskStatus::Failed && self.retry_count < self.max_retries
    }
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::DeadLetter
        )
    }
}

/// 任务执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub status: TaskStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
}

/// 任务执行日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLog {
    pub task_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

impl TaskLog {
    pub fn new(task_id: Uuid, level: LogLevel, message: String) -> Self {
        Self {
            task_id,
            timestamp: Utc::now(),
            level,
            message,
        }
    }

    pub fn info(task_id: Uuid, message: String) -> Self {
        Self::new(task_id, LogLevel::Info, message)
    }

    pub fn error(task_id: Uuid, message: String) -> Self {
        Self::new(task_id, LogLevel::Error, message)
    }
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// 队列状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueState {
    pub queue_name: String,
    pub pending_count: usize,
    pub running_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub snapshot_at: DateTime<Utc>,
}

impl QueueState {
    pub fn new(queue_name: String) -> Self {
        Self {
            queue_name,
            pending_count: 0,
            running_count: 0,
            completed_count: 0,
            failed_count: 0,
            snapshot_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let task = Task::new(
            tenant_id,
            user_id,
            "Test Task".to_string(),
            "echo hello".to_string(),
        );

        assert_eq!(task.status, TaskStatus::Draft);
        assert_eq!(task.priority, 0);
        assert_eq!(task.queue_name, "default");
        assert_eq!(task.retry_count, 0);
        assert_eq!(task.max_retries, 3);
    }

    #[test]
    fn test_task_is_ready() {
        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );

        let dep1 = Uuid::new_v4();
        let dep2 = Uuid::new_v4();
        task.dependencies = vec![dep1, dep2];

        assert!(!task.is_ready(&[]));
        assert!(!task.is_ready(&[dep1]));
        assert!(task.is_ready(&[dep1, dep2]));
    }

    #[test]
    fn test_task_can_retry() {
        let mut task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );

        task.status = TaskStatus::Failed;
        task.retry_count = 2;
        task.max_retries = 3;
        assert!(task.can_retry());

        task.retry_count = 3;
        assert!(!task.can_retry());
    }

    #[test]
    fn test_task_status_is_terminal() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(TaskStatus::DeadLetter.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(!TaskStatus::Queued.is_terminal());
    }
}
