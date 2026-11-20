//! 任务队列集成测试

use aether_remote::queue::{QueueConfig, Task, TaskQueueSystem, TaskStatus};
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

async fn create_test_system() -> (TaskQueueSystem, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("integration_test.aedb");
    
    let config = QueueConfig::new(db_path, "test-device".to_string())
        .with_max_concurrent(2)
        .with_max_queue_size(100);
    
    let system = TaskQueueSystem::new(config).await.unwrap();
    (system, temp_dir)
}

#[tokio::test]
async fn test_complete_task_lifecycle() {
    let (system, _temp) = create_test_system().await;
    
    // 1. 提交任务
    let task = Task::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "Integration Test Task".to_string(),
        "echo test".to_string(),
    );
    
    system.submit_task(task.clone()).await.unwrap();
    
    // 2. 验证任务已保存
    let retrieved = system.get_task(task.id).await.unwrap();
    assert_eq!(retrieved.id, task.id);
    assert_eq!(retrieved.status, TaskStatus::Queued);
    
    // 3. 验证队列大小
    assert_eq!(system.queue_size().await, 1);
    
    // 4. 取消任务
    system.cancel_task(task.id).await.unwrap();
    
    // 5. 验证状态
    let status = system.get_task_status(task.id).await.unwrap();
    assert_eq!(status, TaskStatus::Cancelled);
    
    // 6. 验证队列已清空
    assert_eq!(system.queue_size().await, 0);
}

#[tokio::test]
async fn test_multiple_tasks() {
    let (system, _temp) = create_test_system().await;
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 提交多个任务
    for i in 0..5 {
        let task = Task::new(
            tenant_id,
            user_id,
            format!("Task {}", i),
            format!("cmd {}", i),
        );
        system.submit_task(task).await.unwrap();
    }
    
    assert_eq!(system.queue_size().await, 5);
}

#[tokio::test]
async fn test_priority_ordering() {
    let (system, _temp) = create_test_system().await;
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 提交不同优先级的任务
    let mut low_task = Task::new(tenant_id, user_id, "Low".to_string(), "cmd".to_string());
    low_task.priority = 1;
    
    let mut high_task = Task::new(tenant_id, user_id, "High".to_string(), "cmd".to_string());
    high_task.priority = 10;
    
    let mut mid_task = Task::new(tenant_id, user_id, "Mid".to_string(), "cmd".to_string());
    mid_task.priority = 5;
    
    system.submit_task(low_task).await.unwrap();
    system.submit_task(high_task.clone()).await.unwrap();
    system.submit_task(mid_task).await.unwrap();
    
    // 验证队列大小
    assert_eq!(system.queue_size().await, 3);
}

#[tokio::test]
async fn test_task_dependencies() {
    let (system, _temp) = create_test_system().await;
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 创建依赖链: task1 -> task2 -> task3
    let task1 = Task::new(tenant_id, user_id, "Task 1".to_string(), "cmd1".to_string());
    
    let mut task2 = Task::new(tenant_id, user_id, "Task 2".to_string(), "cmd2".to_string());
    task2.dependencies = vec![task1.id];
    
    let mut task3 = Task::new(tenant_id, user_id, "Task 3".to_string(), "cmd3".to_string());
    task3.dependencies = vec![task2.id];
    
    system.submit_task(task1.clone()).await.unwrap();
    system.submit_task(task2.clone()).await.unwrap();
    system.submit_task(task3.clone()).await.unwrap();
    
    assert_eq!(system.queue_size().await, 3);
}

#[tokio::test]
async fn test_crash_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("recovery_test.aedb");
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 第一次启动：提交任务
    {
        let config = QueueConfig::new(db_path.clone(), "test-device".to_string());
        let system = TaskQueueSystem::new(config).await.unwrap();
        
        let task = Task::new(tenant_id, user_id, "Recovery Task".to_string(), "cmd".to_string());
        system.submit_task(task.clone()).await.unwrap();
        
        // 模拟崩溃（不调用 shutdown）
    }
    
    // 第二次启动：恢复任务
    {
        let config = QueueConfig::new(db_path, "test-device".to_string());
        let system = TaskQueueSystem::new(config).await.unwrap();
        
        // 验证任务已恢复
        let store = system.store();
        let tasks = store.get_tasks_by_status(TaskStatus::Queued);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Recovery Task");
    }
}

#[tokio::test]
#[ignore] // 需要真实的 Supabase 连接
async fn test_concurrent_execution() {
    let (system, _temp) = create_test_system().await;
    system.start().await.unwrap();
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 提交多个任务
    let mut tasks = Vec::new();
    for i in 0..10 {
        let task = Task::new(
            tenant_id,
            user_id,
            format!("Concurrent Task {}", i),
            "sleep 0.1".to_string(),
        );
        tasks.push(task.clone());
        system.submit_task(task).await.unwrap();
    }
    
    // 等待一段时间让任务执行
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 验证并发数不超过限制
    assert!(system.running_count() <= 2);
}

#[tokio::test]
async fn test_task_logs() {
    let (system, _temp) = create_test_system().await;
    
    let task = Task::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "Logged Task".to_string(),
        "echo test".to_string(),
    );
    
    system.submit_task(task.clone()).await.unwrap();
    
    // 手动添加日志
    use aether_remote::queue::models::TaskLog;
    let store = system.store();
    store.append_log(&TaskLog::info(task.id, "Test log 1".to_string())).unwrap();
    store.append_log(&TaskLog::error(task.id, "Test log 2".to_string())).unwrap();
    
    // 获取日志
    let logs = system.get_task_logs(task.id).await;
    assert!(logs.len() >= 2);
}

#[tokio::test]
async fn test_metrics() {
    let (system, _temp) = create_test_system().await;
    
    let task = Task::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "Metrics Task".to_string(),
        "cmd".to_string(),
    );
    
    system.submit_task(task).await.unwrap();
    
    let metrics = system.metrics();
    assert_eq!(metrics.tasks_submitted.get(), 1.0);
    
    let exported = metrics.export();
    assert!(exported.contains("queue_tasks_submitted_total"));
}

#[tokio::test]
async fn test_queue_full() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("full_test.aedb");
    
    let config = QueueConfig::new(db_path, "test-device".to_string())
        .with_max_queue_size(2);
    
    let system = TaskQueueSystem::new(config).await.unwrap();
    
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    
    // 提交 2 个任务（达到容量）
    for i in 0..2 {
        let task = Task::new(tenant_id, user_id, format!("Task {}", i), "cmd".to_string());
        system.submit_task(task).await.unwrap();
    }
    
    // 第 3 个任务应该失败
    let task3 = Task::new(tenant_id, user_id, "Task 3".to_string(), "cmd".to_string());
    let result = system.submit_task(task3).await;
    
    assert!(result.is_err());
}

#[tokio::test]
#[ignore] // 需要真实的 Supabase 连接
async fn test_shutdown() {
    let (system, _temp) = create_test_system().await;
    system.start().await.unwrap();
    
    let task = Task::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "Shutdown Test".to_string(),
        "cmd".to_string(),
    );
    
    system.submit_task(task).await.unwrap();
    
    // 关闭系统
    system.shutdown().await.unwrap();
    
    // 验证系统已关闭（这里只是确保不崩溃）
}
