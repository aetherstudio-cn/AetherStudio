//! 任务执行器实现

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::error::{QueueError, Result};
use super::models::Task;

/// 任务执行器（并发池）
pub struct TaskExecutor {
    semaphore: Arc<Semaphore>,
    running_tasks: Arc<DashMap<Uuid, JoinHandle<()>>>,
    max_concurrent: usize,
}

impl TaskExecutor {
    /// 创建新的任务执行器
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            running_tasks: Arc::new(DashMap::new()),
            max_concurrent,
        }
    }

    /// 提交任务执行
    pub async fn submit<F, Fut>(&self, task: Task, executor: F) -> Result<()>
    where
        F: FnOnce(Task) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        // 检查是否已存在相同任务
        if self.running_tasks.contains_key(&task.id) {
            return Err(QueueError::DuplicateTask(task.id.to_string()));
        }

        // 获取信号量许可
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| QueueError::Internal(format!("获取信号量失败: {}", e)))?;

        let task_id = task.id;
        let running_tasks = self.running_tasks.clone();

        // 启动任务
        let handle = tokio::spawn(async move {
            let _permit = permit; // 保持许可直到任务完成

            if let Err(e) = executor(task).await {
                tracing::error!("任务执行失败: {}", e);
            }

            // 任务完成后自动移除
            running_tasks.remove(&task_id);
        });

        self.running_tasks.insert(task_id, handle);
        Ok(())
    }

    /// 取消任务
    pub async fn cancel(&self, task_id: Uuid) -> Result<()> {
        if let Some((_, handle)) = self.running_tasks.remove(&task_id) {
            handle.abort();
            Ok(())
        } else {
            Err(QueueError::TaskNotFound(task_id.to_string()))
        }
    }

    /// 获取运行中任务数
    pub fn running_count(&self) -> usize {
        self.running_tasks.len()
    }

    /// 获取可用槽位数
    pub fn available_slots(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// 是否已满
    pub fn is_full(&self) -> bool {
        self.available_slots() == 0
    }

    /// 检查任务是否在运行
    pub fn is_running(&self, task_id: Uuid) -> bool {
        self.running_tasks.contains_key(&task_id)
    }

    /// 等待所有任务完成
    pub async fn wait_all(&self) {
        while !self.running_tasks.is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    /// 获取最大并发数
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_submit_task() {
        let executor = TaskExecutor::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );

        executor
            .submit(task, move |_task| {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await
            .unwrap();

        // 等待任务完成
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_concurrent_limit() {
        let executor = TaskExecutor::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(AtomicUsize::new(0));
        let max_running = Arc::new(AtomicUsize::new(0));

        for i in 0..5 {
            let task = Task::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                format!("Task {}", i),
                "cmd".to_string(),
            );

            let counter = counter.clone();
            let running = running.clone();
            let max_running = max_running.clone();

            executor
                .submit(task, move |_task| {
                    let counter = counter.clone();
                    let running = running.clone();
                    let max_running = max_running.clone();
                    async move {
                        let current = running.fetch_add(1, Ordering::SeqCst) + 1;
                        max_running.fetch_max(current, Ordering::SeqCst);
                        
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        
                        running.fetch_sub(1, Ordering::SeqCst);
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                })
                .await
                .unwrap();
        }

        executor.wait_all().await;
        
        assert_eq!(counter.load(Ordering::SeqCst), 5);
        assert!(max_running.load(Ordering::SeqCst) <= 2);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let executor = TaskExecutor::new(2);
        let started = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));

        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );
        let task_id = task.id;

        let started_clone = started.clone();
        let completed_clone = completed.clone();

        executor
            .submit(task, move |_task| {
                let started = started_clone.clone();
                let completed = completed_clone.clone();
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    completed.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await
            .unwrap();

        // 等待任务开始
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(started.load(Ordering::SeqCst), 1);

        // 取消任务
        executor.cancel(task_id).await.unwrap();
        
        // 等待一段时间
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(completed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_duplicate_task() {
        let executor = TaskExecutor::new(2);
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );
        let task_clone = task.clone();

        executor
            .submit(task, |_task| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            })
            .await
            .unwrap();

        let result = executor
            .submit(task_clone, |_task| async move { Ok(()) })
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), QueueError::DuplicateTask(_)));
    }

    #[tokio::test]
    async fn test_running_count() {
        let executor = TaskExecutor::new(3);
        
        assert_eq!(executor.running_count(), 0);
        assert_eq!(executor.available_slots(), 3);

        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );

        executor
            .submit(task, |_task| async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(executor.running_count(), 1);
        assert_eq!(executor.available_slots(), 2);

        executor.wait_all().await;
        assert_eq!(executor.running_count(), 0);
    }
}
