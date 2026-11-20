//! 任务队列系统（整合所有模块）

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::auth::SupabaseAuthClient;
use super::command_executor::CommandExecutor;
use super::config::QueueConfig;
use super::dependency::DependencyGraph;
use super::device::DeviceManager;
use super::error::{QueueError, Result};
use super::executor::TaskExecutor;
use super::metrics::QueueMetrics;
use super::models::{QueueState, Task, TaskLog, TaskResult, TaskStatus};
use super::persistence::AetherTaskStore;
use super::priority_queue::PriorityTaskQueue;
use super::realtime::HybridTaskListener;
use super::session::SessionManager;
use super::supabase::{Device, SupabaseRestClient};

/// 任务队列系统
pub struct TaskQueueSystem {
    /// AetherDB 持久化存储
    store: Arc<AetherTaskStore>,
    
    /// 内存优先级队列
    memory_queue: Arc<RwLock<PriorityTaskQueue>>,
    
    /// 任务执行器
    executor: Arc<TaskExecutor>,
    
    /// 命令执行器
    command_executor: Arc<CommandExecutor>,
    
    /// 依赖图
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    
    /// 设备管理器
    device_manager: Arc<DeviceManager>,
    
    /// 会话管理器
    session_manager: Arc<SessionManager>,
    
    /// 监控指标
    metrics: Arc<QueueMetrics>,
    
    /// 配置
    config: QueueConfig,
}

impl TaskQueueSystem {
    /// 创建新的任务队列系统
    pub async fn new(config: QueueConfig) -> Result<Self> {
        // 1. 打开 AetherDB
        let store = Arc::new(AetherTaskStore::open(&config.db_path)?);
        
        // 2. 初始化内存队列
        let memory_queue = Arc::new(RwLock::new(PriorityTaskQueue::new(config.max_queue_size)));
        
        // 3. 恢复未完成任务
        Self::recover_from_db(&store, &memory_queue).await?;
        
        // 4. 初始化执行器
        let executor = Arc::new(TaskExecutor::new(config.max_concurrent));
        
        // 5. 初始化 Auth 客户端和会话管理器
        let auth_client = Arc::new(SupabaseAuthClient::new(config.supabase_config.clone()));
        let session_file = config.db_path.parent()
            .map(|p| p.join("session.json"))
            .unwrap_or_else(|| PathBuf::from("session.json"));
        let session_manager = Arc::new(SessionManager::new(auth_client, session_file));
        
        // 6. 尝试恢复会话
        let access_token = match session_manager.restore_session().await? {
            Some(session) => {
                tracing::info!("会话已恢复: {}", session.user.email);
                Some(session.access_token)
            }
            None => {
                tracing::warn!("没有有效的会话，需要先登录");
                None
            }
        };
        
        // 7. 初始化命令执行器（使用用户 Token）
        let mut rest_client = SupabaseRestClient::new(config.supabase_config.clone());
        if let Some(token) = access_token {
            rest_client = rest_client.with_access_token(token);
        }
        let rest_client = Arc::new(rest_client);
        
        let command_executor = Arc::new(
            CommandExecutor::new(store.clone())
                .with_rest_client(rest_client.clone())
        );
        
        // 8. 初始化依赖图
        let dependency_graph = Arc::new(RwLock::new(DependencyGraph::new()));
        
        // 9. 初始化设备管理器（使用用户 Token）
        let device_manager = Arc::new(DeviceManager::new(config.supabase_config.clone()));
        
        // 10. 初始化监控
        let metrics = Arc::new(QueueMetrics::new());
        
        Ok(Self {
            store,
            memory_queue,
            executor,
            command_executor,
            dependency_graph,
            device_manager,
            session_manager,
            metrics,
            config,
        })
    }

    /// 从 AetherDB 恢复未完成任务
    async fn recover_from_db(
        store: &Arc<AetherTaskStore>,
        memory_queue: &Arc<RwLock<PriorityTaskQueue>>,
    ) -> Result<()> {
        let pending_tasks = store.get_tasks_by_status(TaskStatus::Queued);
        let running_tasks = store.get_tasks_by_status(TaskStatus::Running);
        
        let mut queue = memory_queue.write().await;
        
        // 恢复排队任务
        for task in pending_tasks {
            if let Err(e) = queue.push(task) {
                tracing::error!("恢复任务失败: {}", e);
            }
        }
        
        // 恢复运行中任务（标记为失败，等待重试）
        for mut task in running_tasks {
            task.status = TaskStatus::Queued;
            task.retry_count += 1;
            
            if let Err(e) = store.save_task(&task) {
                tracing::error!("更新任务状态失败: {}", e);
                continue;
            }
            
            if let Err(e) = queue.push(task) {
                tracing::error!("恢复任务失败: {}", e);
            }
        }
        
        tracing::info!("从 AetherDB 恢复 {} 个任务", queue.len());
        Ok(())
    }

    /// 启动任务队列系统
    pub async fn start(&self) -> Result<()> {
        // 1. 检查是否已登录
        if !self.session_manager.is_authenticated().await {
            return Err(QueueError::Internal("未登录，请先调用 login()".to_string()));
        }
        
        // 2. 注册设备
        self.register_device().await?;
        
        // 3. 启动心跳
        self.device_manager.start_heartbeat().await?;
        
        // 4. 启动 Realtime 监听
        self.start_realtime_listener().await?;
        
        // 5. 启动任务调度循环
        self.start_scheduler().await?;
        
        // 6. 启动定期维护任务
        self.start_maintenance().await?;
        
        tracing::info!("任务队列系统已启动");
        Ok(())
    }

    /// 用户登录
    pub async fn login(&self, email: &str, password: &str) -> Result<()> {
        let session = self.session_manager.login(email, password).await?;
        tracing::info!("用户登录成功: {}", session.user.email);
        
        // 更新 REST 客户端的 Token
        // TODO: 需要重新创建 command_executor 和 device_manager
        
        Ok(())
    }

    /// 用户注册
    pub async fn signup(&self, email: &str, password: &str, tenant_id: Option<String>) -> Result<()> {
        let session = self.session_manager.signup(email, password, tenant_id).await?;
        tracing::info!("用户注册成功: {}", session.user.email);
        Ok(())
    }

    /// 用户登出
    pub async fn logout(&self) -> Result<()> {
        self.session_manager.logout().await?;
        tracing::info!("用户已登出");
        Ok(())
    }

    /// 注册设备
    async fn register_device(&self) -> Result<()> {
        // 从会话获取用户信息
        let session = self.session_manager.get_session().await
            .ok_or_else(|| QueueError::Internal("未登录".to_string()))?;
        
        let tenant_id = session.user.tenant_id()
            .unwrap_or_else(|| self.config.tenant_id.clone());
        
        let device = Device::new(
            tenant_id,
            session.user.id.clone(),
            self.config.device_name.clone(),
            self.config.device_type.clone(),
        );
        
        let device_id = self.device_manager.register(device).await?;
        tracing::info!("设备已注册: {}", device_id);
        
        Ok(())
    }

    /// 启动 Realtime 监听器
    async fn start_realtime_listener(&self) -> Result<()> {
        let device_id = self.device_manager.device_id().await
            .ok_or_else(|| QueueError::Internal("设备未注册".to_string()))?;
        
        let listener = HybridTaskListener::new(
            self.config.supabase_config.clone(),
            device_id,
        );
        let mut rx = listener.start().await?;
        
        let store = self.store.clone();
        let queue = self.memory_queue.clone();
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                tracing::info!("收到新任务: {}", task.id);
                
                // 保存到 AetherDB
                if let Err(e) = store.save_task(&task) {
                    tracing::error!("保存任务失败: {}", e);
                    continue;
                }
                
                // 加入内存队列
                let mut q = queue.write().await;
                if let Err(e) = q.push(task) {
                    tracing::error!("任务入队失败: {}", e);
                    continue;
                }
                
                metrics.record_task_submitted();
                metrics.update_queue_size(q.len());
            }
        });
        
        Ok(())
    }

    /// 启动任务调度器
    async fn start_scheduler(&self) -> Result<()> {
        let store = self.store.clone();
        let queue = self.memory_queue.clone();
        let executor = self.executor.clone();
        let graph = self.dependency_graph.clone();
        let metrics = self.metrics.clone();
        let retry_policy = self.config.retry_policy.clone();
        let command_executor = self.command_executor.clone();
        
        tokio::spawn(async move {
            loop {
                // 获取就绪任务
                let ready_tasks = {
                    let g = graph.read().await;
                    g.get_ready_tasks()
                };
                
                // 从队列中取出任务
                let mut q = queue.write().await;
                for task_id in ready_tasks {
                    if let Some(task) = q.pop_by_id(task_id) {
                        // 更新状态为运行中
                        let mut task = task;
                        task.status = TaskStatus::Running;
                        task.updated_at = chrono::Utc::now();
                        
                        if let Err(e) = store.save_task(&task) {
                            tracing::error!("更新任务状态失败: {}", e);
                            continue;
                        }
                        
                        // 记录开始日志
                        let _ = store.append_log(&TaskLog::info(
                            task.id,
                            format!("任务开始执行: {}", task.title),
                        ));
                        
                        // 提交到执行器
                        let store_clone = store.clone();
                        let metrics_clone = metrics.clone();
                        let graph_clone = graph.clone();
                        let retry_policy_clone = retry_policy.clone();
                        let command_executor_clone = command_executor.clone();
                        
                        let execute_fn = move |task: Task| {
                            let store = store_clone.clone();
                            let metrics = metrics_clone.clone();
                            let graph = graph_clone.clone();
                            let retry_policy = retry_policy_clone.clone();
                            let command_executor = command_executor_clone.clone();
                            
                            async move {
                                let start = Instant::now();
                                
                                match command_executor.execute(task.clone()).await {
                                    Ok(result) => {
                                        let duration = start.elapsed();
                                        metrics.record_task_completed(duration);
                                        
                                        // 保存结果
                                        if let Err(e) = store.save_result(&result) {
                                            tracing::error!("保存任务结果失败: {}", e);
                                        }
                                        
                                        // 更新任务状态
                                        let mut completed_task = task.clone();
                                        completed_task.status = TaskStatus::Completed;
                                        completed_task.updated_at = chrono::Utc::now();
                                        
                                        if let Err(e) = store.save_task(&completed_task) {
                                            tracing::error!("更新任务状态失败: {}", e);
                                        }
                                        
                                        // 从依赖图移除
                                        let mut g = graph.write().await;
                                        g.remove_task(task.id);
                                        
                                        Ok(())
                                    }
                                    Err(e) => {
                                        tracing::error!("任务执行失败: {}", e);
                                        metrics.record_task_failed();
                                        
                                        // 记录错误日志
                                        let _ = store.append_log(&TaskLog::error(
                                            task.id,
                                            format!("任务执行失败: {}", e),
                                        ));
                                        
                                        // 重试逻辑
                                        if retry_policy.should_retry(&e, task.retry_count as usize) {
                                            let mut retry_task = task.clone();
                                            retry_task.status = TaskStatus::Queued;
                                            retry_task.retry_count += 1;
                                            retry_task.updated_at = chrono::Utc::now();
                                            
                                            if let Err(e) = store.save_task(&retry_task) {
                                                tracing::error!("保存重试任务失败: {}", e);
                                            }
                                            
                                            metrics.record_task_retried();
                                        } else {
                                            // 移入死信队列
                                            let mut dead_task = task.clone();
                                            dead_task.status = TaskStatus::DeadLetter;
                                            dead_task.updated_at = chrono::Utc::now();
                                            
                                            if let Err(e) = store.save_task(&dead_task) {
                                                tracing::error!("保存死信任务失败: {}", e);
                                            }
                                        }
                                        
                                        Err(e)
                                    }
                                }
                            }
                        };
                        
                        if let Err(e) = executor.submit(task, execute_fn).await {
                            tracing::error!("提交任务到执行器失败: {}", e);
                        }
                    }
                }
                
                // 更新指标
                metrics.update_queue_size(q.len());
                metrics.update_running_tasks(executor.running_count());
                metrics.update_available_slots(executor.available_slots());
                
                drop(q); // 释放写锁
                
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        
        Ok(())
    }

    /// 启动定期维护任务
    async fn start_maintenance(&self) -> Result<()> {
        let store = self.store.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.maintenance_interval);
            
            loop {
                interval.tick().await;
                
                // 1. 清理旧任务
                match store.cleanup_old_tasks(config.retention_days) {
                    Ok(count) => tracing::info!("清理了 {} 个旧任务", count),
                    Err(e) => tracing::error!("清理旧任务失败: {}", e),
                }
                
                // 2. AetherDB compaction
                match store.compact() {
                    Ok(_) => tracing::info!("AetherDB compaction 完成"),
                    Err(e) => tracing::error!("AetherDB compaction 失败: {}", e),
                }
                
                // 3. 保存队列状态快照
                let state = QueueState::new("default".to_string());
                
                if let Err(e) = store.save_queue_state(&state) {
                    tracing::error!("保存队列状态失败: {}", e);
                }
            }
        });
        
        Ok(())
    }

    /// 提交任务
    pub async fn submit_task(&self, mut task: Task) -> Result<()> {
        // 1. 验证任务
        self.validate_task(&task)?;
        
        // 2. 设置状态为排队
        task.status = TaskStatus::Queued;
        task.updated_at = chrono::Utc::now();
        
        // 3. 保存到 AetherDB
        self.store.save_task(&task)?;
        
        // 4. 添加到依赖图
        {
            let mut graph = self.dependency_graph.write().await;
            graph.add_task(&task)?;
        }
        
        // 5. 加入内存队列
        {
            let mut queue = self.memory_queue.write().await;
            queue.push(task)?;
            self.metrics.update_queue_size(queue.len());
        }
        
        self.metrics.record_task_submitted();
        Ok(())
    }

    /// 取消任务
    pub async fn cancel_task(&self, task_id: Uuid) -> Result<()> {
        // 1. 尝试从执行器取消（如果任务正在运行）
        let _ = self.executor.cancel(task_id).await; // 忽略错误，任务可能不在执行器中
        
        // 2. 更新状态
        if let Some(mut task) = self.store.get_task(task_id) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = chrono::Utc::now();
            self.store.save_task(&task)?;
        } else {
            return Err(QueueError::TaskNotFound(task_id.to_string()));
        }
        
        // 3. 从依赖图移除
        {
            let mut graph = self.dependency_graph.write().await;
            graph.remove_task(task_id);
        }
        
        // 4. 从内存队列移除
        {
            let mut queue = self.memory_queue.write().await;
            queue.pop_by_id(task_id);
            self.metrics.update_queue_size(queue.len());
        }
        
        self.metrics.record_task_cancelled();
        Ok(())
    }

    /// 获取任务状态
    pub async fn get_task_status(&self, task_id: Uuid) -> Option<TaskStatus> {
        self.store.get_task(task_id).map(|t| t.status)
    }

    /// 获取任务
    pub async fn get_task(&self, task_id: Uuid) -> Option<Task> {
        self.store.get_task(task_id)
    }

    /// 获取任务结果
    pub async fn get_task_result(&self, task_id: Uuid) -> Option<TaskResult> {
        self.store.get_result(task_id)
    }

    /// 获取任务日志
    pub async fn get_task_logs(&self, task_id: Uuid) -> Vec<TaskLog> {
        self.store.get_task_logs(task_id)
    }

    /// 获取队列大小
    pub async fn queue_size(&self) -> usize {
        let queue = self.memory_queue.read().await;
        queue.len()
    }

    /// 获取运行中任务数
    pub fn running_count(&self) -> usize {
        self.executor.running_count()
    }

    /// 获取监控指标
    pub fn metrics(&self) -> Arc<QueueMetrics> {
        self.metrics.clone()
    }

    /// 获取存储层（用于测试）
    pub fn store(&self) -> Arc<AetherTaskStore> {
        self.store.clone()
    }

    /// 获取设备管理器
    pub fn device_manager(&self) -> Arc<DeviceManager> {
        self.device_manager.clone()
    }

    /// 获取会话管理器
    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }

    /// 验证任务
    fn validate_task(&self, task: &Task) -> Result<()> {
        if task.title.is_empty() {
            return Err(QueueError::InvalidTask("任务标题不能为空".to_string()));
        }
        
        if task.command.is_empty() {
            return Err(QueueError::InvalidTask("任务命令不能为空".to_string()));
        }
        
        Ok(())
    }

    /// 关闭系统
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("正在关闭任务队列系统...");
        
        // 1. 设备离线
        if let Err(e) = self.device_manager.go_offline().await {
            tracing::error!("设备离线失败: {}", e);
        }
        
        // 2. 等待所有任务完成
        self.executor.wait_all().await;
        
        // 3. 保存队列状态
        let state = QueueState::new("default".to_string());
        self.store.save_queue_state(&state)?;
        
        tracing::info!("任务队列系统已关闭");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_system() -> (TaskQueueSystem, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_tasks.aedb");
        
        let config = QueueConfig::new(db_path, "test-device".to_string())
            .with_max_concurrent(2)
            .with_max_queue_size(10);
        
        let system = TaskQueueSystem::new(config).await.unwrap();
        (system, temp_dir)
    }

    #[tokio::test]
    async fn test_submit_task() {
        let (system, _temp) = create_test_system().await;
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Task".to_string(),
            "echo hello".to_string(),
        );
        
        system.submit_task(task.clone()).await.unwrap();
        
        let retrieved = system.get_task(task.id).await.unwrap();
        assert_eq!(retrieved.id, task.id);
        assert_eq!(retrieved.status, TaskStatus::Queued);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let (system, _temp) = create_test_system().await;
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test Task".to_string(),
            "echo hello".to_string(),
        );
        
        system.submit_task(task.clone()).await.unwrap();
        system.cancel_task(task.id).await.unwrap();
        
        let status = system.get_task_status(task.id).await.unwrap();
        assert_eq!(status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_queue_size() {
        let (system, _temp) = create_test_system().await;
        
        assert_eq!(system.queue_size().await, 0);
        
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Test".to_string(),
            "cmd".to_string(),
        );
        
        system.submit_task(task).await.unwrap();
        assert_eq!(system.queue_size().await, 1);
    }
}
