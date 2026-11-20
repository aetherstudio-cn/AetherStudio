//! AetherDB 持久化层实现

use aether_db::AetherDb;
use chrono::Utc;
use std::path::Path;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use super::error::{QueueError, Result};
use super::kind;
use super::models::{QueueState, Task, TaskLog, TaskResult, TaskStatus};

/// AetherDB 任务存储
pub struct AetherTaskStore {
    db: Arc<RwLock<AetherDb>>,
}

impl AetherTaskStore {
    /// 打开或创建任务存储
    pub fn open(path: &Path) -> Result<Self> {
        // 任务队列不需要向量索引，dim=0
        let db = AetherDb::open(path, 0)?;
        Ok(Self {
            db: Arc::new(RwLock::new(db)),
        })
    }

    /// 保存任务元数据
    pub fn save_task(&self, task: &Task) -> Result<()> {
        let payload = serde_json::to_vec(task)?;
        
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        
        db.put(
            kind::TASK_META,
            &task.id.to_string(),
            &task.queue_name, // tag = queue_name，便于按队列查询
            &payload,
            None, // 无向量
        )?;
        
        Ok(())
    }

    /// 获取任务
    pub fn get_task(&self, task_id: Uuid) -> Option<Task> {
        let db = self.db.read().ok()?;
        let record = db.get(kind::TASK_META, &task_id.to_string())?;
        serde_json::from_slice(&record.payload).ok()
    }

    /// 按队列查询任务
    pub fn get_tasks_by_queue(&self, queue_name: &str) -> Vec<Task> {
        let db = match self.db.read() {
            Ok(db) => db,
            Err(_) => return Vec::new(),
        };
        
        db.scan_by_tag(kind::TASK_META, queue_name)
            .into_iter()
            .filter_map(|r| serde_json::from_slice(&r.payload).ok())
            .collect()
    }

    /// 按状态查询任务
    pub fn get_tasks_by_status(&self, status: TaskStatus) -> Vec<Task> {
        let db = match self.db.read() {
            Ok(db) => db,
            Err(_) => return Vec::new(),
        };
        
        db.scan(kind::TASK_META)
            .into_iter()
            .filter_map(|r| serde_json::from_slice::<Task>(&r.payload).ok())
            .filter(|t| t.status == status)
            .collect()
    }

    /// 获取所有任务
    pub fn get_all_tasks(&self) -> Vec<Task> {
        let db = match self.db.read() {
            Ok(db) => db,
            Err(_) => return Vec::new(),
        };
        
        db.scan(kind::TASK_META)
            .into_iter()
            .filter_map(|r| serde_json::from_slice(&r.payload).ok())
            .collect()
    }

    /// 保存任务结果
    pub fn save_result(&self, result: &TaskResult) -> Result<()> {
        let payload = serde_json::to_vec(result)?;
        
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        
        db.put(
            kind::TASK_RESULT,
            &result.task_id.to_string(),
            "", // 无 tag
            &payload,
            None,
        )?;
        
        Ok(())
    }

    /// 获取任务结果
    pub fn get_result(&self, task_id: Uuid) -> Option<TaskResult> {
        let db = self.db.read().ok()?;
        let record = db.get(kind::TASK_RESULT, &task_id.to_string())?;
        serde_json::from_slice(&record.payload).ok()
    }

    /// 追加任务日志
    pub fn append_log(&self, log: &TaskLog) -> Result<()> {
        let payload = serde_json::to_vec(log)?;
        
        // 使用复合 key: task_id/timestamp_millis/nanos
        let key = format!(
            "{}/{}/{}",
            log.task_id,
            log.timestamp.timestamp_millis(),
            log.timestamp.timestamp_subsec_nanos()
        );
        
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        
        db.put(
            kind::TASK_LOG,
            &key,
            &log.task_id.to_string(), // tag = task_id，便于查询某个任务的所有日志
            &payload,
            None,
        )?;
        
        Ok(())
    }

    /// 获取任务的所有日志
    pub fn get_task_logs(&self, task_id: Uuid) -> Vec<TaskLog> {
        let db = match self.db.read() {
            Ok(db) => db,
            Err(_) => return Vec::new(),
        };
        
        db.scan_by_tag(kind::TASK_LOG, &task_id.to_string())
            .into_iter()
            .filter_map(|r| serde_json::from_slice(&r.payload).ok())
            .collect()
    }

    /// 保存队列状态快照
    pub fn save_queue_state(&self, state: &QueueState) -> Result<()> {
        let payload = serde_json::to_vec(state)?;
        
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        
        db.put(
            kind::QUEUE_STATE,
            &state.queue_name,
            "",
            &payload,
            None,
        )?;
        
        Ok(())
    }

    /// 获取队列状态快照
    pub fn get_queue_state(&self, queue_name: &str) -> Option<QueueState> {
        let db = self.db.read().ok()?;
        let record = db.get(kind::QUEUE_STATE, queue_name)?;
        serde_json::from_slice(&record.payload).ok()
    }

    /// 删除任务（级联删除结果和日志）
    pub fn delete_task(&self, task_id: Uuid) -> Result<()> {
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        
        // 删除元数据
        db.delete(kind::TASK_META, &task_id.to_string())?;
        
        // 删除结果
        db.delete(kind::TASK_RESULT, &task_id.to_string())?;
        
        // 删除日志（按 tag 级联删除）
        db.delete_by_tag(kind::TASK_LOG, &task_id.to_string())?;
        
        Ok(())
    }

    /// 清理已完成任务的旧数据
    pub fn cleanup_old_tasks(&self, days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        
        let old_tasks: Vec<String> = {
            let db = self.db.read().map_err(|e| {
                QueueError::Internal(format!("获取读锁失败: {}", e))
            })?;
            
            db.scan(kind::TASK_META)
                .into_iter()
                .filter_map(|r| {
                    let task: Task = serde_json::from_slice(&r.payload).ok()?;
                    if task.status.is_terminal() && task.updated_at < cutoff {
                        Some(task.id.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };
        
        let count = old_tasks.len();
        for task_id in old_tasks {
            if let Ok(uuid) = Uuid::parse_str(&task_id) {
                let _ = self.delete_task(uuid);
            }
        }
        
        Ok(count)
    }

    /// 强制 compaction（定期维护）
    pub fn compact(&self) -> Result<()> {
        let mut db = self.db.write().map_err(|e| {
            QueueError::Internal(format!("获取写锁失败: {}", e))
        })?;
        db.force_compact()?;
        Ok(())
    }

    /// 获取数据库统计信息
    pub fn stats(&self) -> Result<StoreStats> {
        let db = self.db.read().map_err(|e| {
            QueueError::Internal(format!("获取读锁失败: {}", e))
        })?;
        
        let total_tasks = db.scan(kind::TASK_META).len();
        let total_logs = db.scan(kind::TASK_LOG).len();
        let total_results = db.scan(kind::TASK_RESULT).len();
        
        Ok(StoreStats {
            total_tasks,
            total_logs,
            total_results,
        })
    }
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_tasks: usize,
    pub total_logs: usize,
    pub total_results: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (AetherTaskStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_tasks.aedb");
        let store = AetherTaskStore::open(&db_path).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_save_and_get_task() {
        let (store, _temp) = create_test_store();
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Task".to_string(),
            "echo hello".to_string(),
        );
        
        store.save_task(&task).unwrap();
        
        let retrieved = store.get_task(task.id).unwrap();
        assert_eq!(retrieved.id, task.id);
        assert_eq!(retrieved.title, task.title);
        assert_eq!(retrieved.command, task.command);
    }

    #[test]
    fn test_get_tasks_by_queue() {
        let (store, _temp) = create_test_store();
        
        let mut task1 = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Task 1".to_string(),
            "cmd1".to_string(),
        );
        task1.queue_name = "queue_a".to_string();
        
        let mut task2 = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Task 2".to_string(),
            "cmd2".to_string(),
        );
        task2.queue_name = "queue_b".to_string();
        
        store.save_task(&task1).unwrap();
        store.save_task(&task2).unwrap();
        
        let queue_a_tasks = store.get_tasks_by_queue("queue_a");
        assert_eq!(queue_a_tasks.len(), 1);
        assert_eq!(queue_a_tasks[0].id, task1.id);
    }

    #[test]
    fn test_get_tasks_by_status() {
        let (store, _temp) = create_test_store();
        
        let mut task1 = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Task 1".to_string(),
            "cmd1".to_string(),
        );
        task1.status = TaskStatus::Queued;
        
        let mut task2 = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Task 2".to_string(),
            "cmd2".to_string(),
        );
        task2.status = TaskStatus::Running;
        
        store.save_task(&task1).unwrap();
        store.save_task(&task2).unwrap();
        
        let queued_tasks = store.get_tasks_by_status(TaskStatus::Queued);
        assert_eq!(queued_tasks.len(), 1);
        assert_eq!(queued_tasks[0].id, task1.id);
    }

    #[test]
    fn test_save_and_get_result() {
        let (store, _temp) = create_test_store();
        
        let task_id = Uuid::new_v4();
        let result = TaskResult {
            task_id,
            status: TaskStatus::Completed,
            exit_code: Some(0),
            stdout: "success".to_string(),
            stderr: String::new(),
            error: None,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            duration_ms: 1000,
        };
        
        store.save_result(&result).unwrap();
        
        let retrieved = store.get_result(task_id).unwrap();
        assert_eq!(retrieved.task_id, task_id);
        assert_eq!(retrieved.exit_code, Some(0));
    }

    #[test]
    fn test_append_and_get_logs() {
        let (store, _temp) = create_test_store();
        
        let task_id = Uuid::new_v4();
        let log1 = TaskLog::info(task_id, "Log 1".to_string());
        let log2 = TaskLog::error(task_id, "Log 2".to_string());
        
        store.append_log(&log1).unwrap();
        store.append_log(&log2).unwrap();
        
        let logs = store.get_task_logs(task_id);
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn test_delete_task() {
        let (store, _temp) = create_test_store();
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Task".to_string(),
            "cmd".to_string(),
        );
        
        store.save_task(&task).unwrap();
        assert!(store.get_task(task.id).is_some());
        
        store.delete_task(task.id).unwrap();
        assert!(store.get_task(task.id).is_none());
    }

    #[test]
    fn test_cleanup_old_tasks() {
        let (store, _temp) = create_test_store();
        
        let mut old_task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Old Task".to_string(),
            "cmd".to_string(),
        );
        old_task.status = TaskStatus::Completed;
        old_task.updated_at = Utc::now() - chrono::Duration::days(40);
        
        let new_task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "New Task".to_string(),
            "cmd".to_string(),
        );
        
        store.save_task(&old_task).unwrap();
        store.save_task(&new_task).unwrap();
        
        let cleaned = store.cleanup_old_tasks(30).unwrap();
        assert_eq!(cleaned, 1);
        assert!(store.get_task(old_task.id).is_none());
        assert!(store.get_task(new_task.id).is_some());
    }

    #[test]
    fn test_stats() {
        let (store, _temp) = create_test_store();
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );
        
        store.save_task(&task).unwrap();
        
        let stats = store.stats().unwrap();
        assert_eq!(stats.total_tasks, 1);
    }
}
