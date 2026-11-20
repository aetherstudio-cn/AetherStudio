//! 任务队列系统演示

use aether_remote::queue::{QueueConfig, Task, TaskQueueSystem};
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== 任务队列系统演示 ===\n");

    // 1. 创建配置
    let config = QueueConfig::new(
        PathBuf::from("data/demo_tasks.aedb"),
        "demo-device".to_string(),
    )
    .with_max_concurrent(4)
    .with_max_queue_size(100);

    // 2. 创建任务队列系统
    println!("正在初始化任务队列系统...");
    let system = TaskQueueSystem::new(config).await?;
    println!("任务队列系统初始化完成\n");

    // 3. 启动系统
    println!("正在启动任务队列系统...");
    system.start().await?;
    println!("任务队列系统已启动\n");

    // 4. 提交一些示例任务
    println!("提交示例任务...");

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    // 任务 1: 简单任务
    let task1 = Task::new(
        tenant_id,
        user_id,
        "任务 1: 打印 Hello".to_string(),
        "echo Hello World".to_string(),
    );
    system.submit_task(task1.clone()).await?;
    println!("  - 已提交任务: {}", task1.title);

    // 任务 2: 高优先级任务
    let mut task2 = Task::new(
        tenant_id,
        user_id,
        "任务 2: 高优先级".to_string(),
        "echo High Priority".to_string(),
    );
    task2.priority = 10;
    system.submit_task(task2.clone()).await?;
    println!(
        "  - 已提交任务: {} (优先级: {})",
        task2.title, task2.priority
    );

    // 任务 3: 带依赖的任务
    let mut task3 = Task::new(
        tenant_id,
        user_id,
        "任务 3: 依赖任务 1".to_string(),
        "echo Dependent Task".to_string(),
    );
    task3.dependencies = vec![task1.id];
    system.submit_task(task3.clone()).await?;
    println!("  - 已提交任务: {} (依赖: {})", task3.title, task1.id);

    // 任务 4: 多个依赖
    let mut task4 = Task::new(
        tenant_id,
        user_id,
        "任务 4: 依赖任务 2 和 3".to_string(),
        "echo Multi Dependencies".to_string(),
    );
    task4.dependencies = vec![task2.id, task3.id];
    system.submit_task(task4.clone()).await?;
    println!(
        "  - 已提交任务: {} (依赖: {} 个任务)",
        task4.title,
        task4.dependencies.len()
    );

    println!("\n队列状态:");
    println!("  - 队列大小: {}", system.queue_size().await);
    println!("  - 运行中任务: {}", system.running_count());

    // 5. 等待一段时间让任务执行
    println!("\n等待任务执行...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 6. 查询任务状态
    println!("\n任务状态:");
    let task1_id = task1.id;
    for (i, task) in [task1, task2, task3, task4].iter().enumerate() {
        if let Some(status) = system.get_task_status(task.id).await {
            println!("  - 任务 {}: {:?}", i + 1, status);
        }
    }

    // 7. 获取任务日志
    println!("\n任务日志示例:");
    let logs = system.get_task_logs(task1_id).await;
    for log in logs.iter().take(3) {
        println!("  - [{:?}] {}", log.level, log.message);
    }

    // 8. 导出监控指标
    println!("\n监控指标:");
    let metrics = system.metrics();
    let exported = metrics.export();
    for line in exported.lines().take(10) {
        if !line.starts_with('#') {
            println!("  {}", line);
        }
    }

    // 9. 关闭系统
    println!("\n正在关闭任务队列系统...");
    system.shutdown().await?;
    println!("任务队列系统已关闭");

    println!("\n=== 演示完成 ===");
    Ok(())
}
