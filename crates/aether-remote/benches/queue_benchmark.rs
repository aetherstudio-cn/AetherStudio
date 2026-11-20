//! 任务队列性能测试

use aether_remote::queue::{QueueConfig, Task, TaskQueueSystem};
use std::time::Instant;
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== 任务队列性能测试 ===\n");

    // 测试 1: 任务提交吞吐量
    test_submit_throughput().await?;
    
    // 测试 2: 任务查询性能
    test_query_performance().await?;
    
    // 测试 3: 并发执行性能
    test_concurrent_execution().await?;
    
    // 测试 4: 崩溃恢复性能
    test_recovery_performance().await?;

    println!("\n=== 性能测试完成 ===");
    Ok(())
}

async fn test_submit_throughput() -> anyhow::Result<()> {
    println!("测试 1: 任务提交吞吐量");
    
    let temp_dir = TempDir::new()?;
    let config = QueueConfig::new(
        temp_dir.path().join("bench_submit.aedb"),
        "bench-device".to_string(),
    );
    
    let system = TaskQueueSystem::new(config).await?;
    
    let task_count = 1000;
    let start = Instant::now();
    
    for i in 0..task_count {
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            format!("Task {}", i),
            "echo test".to_string(),
        );
        system.submit_task(task).await?;
    }
    
    let duration = start.elapsed();
    let throughput = task_count as f64 / duration.as_secs_f64();
    
    println!("  提交 {} 个任务耗时: {:?}", task_count, duration);
    println!("  吞吐量: {:.2} 任务/秒\n", throughput);
    
    Ok(())
}

async fn test_query_performance() -> anyhow::Result<()> {
    println!("测试 2: 任务查询性能");
    
    let temp_dir = TempDir::new()?;
    let config = QueueConfig::new(
        temp_dir.path().join("bench_query.aedb"),
        "bench-device".to_string(),
    );
    
    let system = TaskQueueSystem::new(config).await?;
    
    // 先提交一些任务
    let mut task_ids = Vec::new();
    for i in 0..100 {
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            format!("Task {}", i),
            "cmd".to_string(),
        );
        task_ids.push(task.id);
        system.submit_task(task).await?;
    }
    
    // 测试查询性能
    let query_count = 1000;
    let start = Instant::now();
    
    for _ in 0..query_count {
        for id in &task_ids {
            let _ = system.get_task(*id).await;
        }
    }
    
    let duration = start.elapsed();
    let qps = (query_count * task_ids.len()) as f64 / duration.as_secs_f64();
    
    println!("  查询 {} 次耗时: {:?}", query_count * task_ids.len(), duration);
    println!("  QPS: {:.2}\n", qps);
    
    Ok(())
}

async fn test_concurrent_execution() -> anyhow::Result<()> {
    println!("测试 3: 并发执行性能");
    
    let temp_dir = TempDir::new()?;
    let config = QueueConfig::new(
        temp_dir.path().join("bench_concurrent.aedb"),
        "bench-device".to_string(),
    )
    .with_max_concurrent(4);
    
    let system = TaskQueueSystem::new(config).await?;
    system.start().await?;
    
    let task_count = 100;
    let start = Instant::now();
    
    for i in 0..task_count {
        let task = Task::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            format!("Concurrent Task {}", i),
            "sleep 0.01".to_string(),
        );
        system.submit_task(task).await?;
    }
    
    // 等待所有任务完成
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    let duration = start.elapsed();
    let throughput = task_count as f64 / duration.as_secs_f64();
    
    println!("  执行 {} 个任务耗时: {:?}", task_count, duration);
    println!("  吞吐量: {:.2} 任务/秒\n", throughput);
    
    system.shutdown().await?;
    
    Ok(())
}

async fn test_recovery_performance() -> anyhow::Result<()> {
    println!("测试 4: 崩溃恢复性能");
    
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("bench_recovery.aedb");
    
    // 第一次启动：提交任务
    {
        let config = QueueConfig::new(db_path.clone(), "bench-device".to_string());
        let system = TaskQueueSystem::new(config).await?;
        
        for i in 0..1000 {
            let task = Task::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                format!("Recovery Task {}", i),
                "cmd".to_string(),
            );
            system.submit_task(task).await?;
        }
    }
    
    // 第二次启动：测试恢复时间
    let start = Instant::now();
    {
        let config = QueueConfig::new(db_path, "bench-device".to_string());
        let system = TaskQueueSystem::new(config).await?;
        
        let queue_size = system.queue_size().await;
        println!("  恢复 {} 个任务", queue_size);
    }
    let duration = start.elapsed();
    
    println!("  恢复耗时: {:?}\n", duration);
    
    Ok(())
}
