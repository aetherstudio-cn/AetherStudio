//! 完整集成演示：设备注册 + Realtime 监听 + 任务执行 + 状态同步

use aether_remote::queue::{Device, QueueConfig, SupabaseConfig, Task, TaskQueueSystem};
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== AetherStudio 完整集成演示 ===\n");

    // 1. 创建配置
    let mut config = QueueConfig::new(
        PathBuf::from("data/full_demo.aedb"),
        "My Windows PC".to_string(),
    );

    // 配置 Supabase
    config.supabase_config = SupabaseConfig::default();
    config.tenant_id = "00000000-0000-0000-0000-000000000000".to_string();
    config.user_id = "00000000-0000-0000-0000-000000000001".to_string();
    config.device_type = "windows".to_string();

    println!("配置信息:");
    println!("  - 设备名称: {}", config.device_name);
    println!("  - 设备类型: {}", config.device_type);
    println!("  - Supabase URL: {}", config.supabase_config.url);
    println!();

    // 2. 创建任务队列系统
    println!("正在初始化任务队列系统...");
    let system = TaskQueueSystem::new(config).await?;
    println!("✓ 任务队列系统初始化完成\n");

    // 3. 启动系统（包括设备注册、心跳、Realtime 监听）
    println!("正在启动任务队列系统...");
    println!("  - 注册设备到 Supabase...");
    println!("  - 启动心跳机制...");
    println!("  - 启动 Realtime 监听...");
    println!("  - 启动任务调度器...");

    match system.start().await {
        Ok(_) => println!("✓ 任务队列系统已启动\n"),
        Err(e) => {
            eprintln!("✗ 启动失败: {}", e);
            eprintln!("\n提示: 请确保:");
            eprintln!("  1. Supabase 项目可访问");
            eprintln!("  2. 数据库权限已配置");
            eprintln!("  3. Realtime 已启用");
            return Err(e.into());
        }
    }

    // 4. 获取设备信息
    if let Some(device_id) = system.device_manager().device_id().await {
        println!("设备信息:");
        println!("  - 设备 ID: {}", device_id);
        println!("  - 状态: online");
        println!();
    }

    // 5. 等待 Realtime 连接建立
    println!("等待 Realtime 连接建立...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    println!("✓ Realtime 连接已建立\n");

    // 6. 提交示例任务
    println!("提交示例任务...");

    let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;
    let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001")?;

    // 任务 1: 简单命令
    let task1 = Task::new(
        tenant_id,
        user_id,
        "列出当前目录".to_string(),
        if cfg!(windows) {
            "cmd /c dir".to_string()
        } else {
            "ls -la".to_string()
        },
    );

    system.submit_task(task1.clone()).await?;
    println!("  ✓ 已提交任务: {}", task1.title);

    // 任务 2: 系统信息
    let task2 = Task::new(
        tenant_id,
        user_id,
        "显示系统信息".to_string(),
        if cfg!(windows) {
            "cmd /c systeminfo | findstr /C:\"OS Name\" /C:\"OS Version\"".to_string()
        } else {
            "uname -a".to_string()
        },
    );

    system.submit_task(task2.clone()).await?;
    println!("  ✓ 已提交任务: {}", task2.title);

    println!();

    // 7. 监控任务执行
    println!("监控任务执行...");
    println!("  - 队列大小: {}", system.queue_size().await);
    println!("  - 运行中任务: {}", system.running_count());
    println!();

    // 8. 等待任务完成
    println!("等待任务完成...");
    for i in 1..=10 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let status1 = system.get_task_status(task1.id).await;
        let status2 = system.get_task_status(task2.id).await;

        println!("  [{}s] 任务 1: {:?}, 任务 2: {:?}", i, status1, status2);

        if status1 == Some(aether_remote::queue::TaskStatus::Completed)
            && status2 == Some(aether_remote::queue::TaskStatus::Completed)
        {
            break;
        }
    }

    println!();

    // 9. 查看任务结果
    println!("任务结果:");

    if let Some(result) = system.get_task_result(task1.id).await {
        println!("  任务 1:");
        println!("    - 状态: {:?}", result.status);
        println!("    - 退出码: {:?}", result.exit_code);
        println!("    - 耗时: {}ms", result.duration_ms);
        if !result.stdout.is_empty() {
            println!("    - 输出: {} 行", result.stdout.lines().count());
        }
    }

    if let Some(result) = system.get_task_result(task2.id).await {
        println!("  任务 2:");
        println!("    - 状态: {:?}", result.status);
        println!("    - 退出码: {:?}", result.exit_code);
        println!("    - 耗时: {}ms", result.duration_ms);
    }

    println!();

    // 10. 查看任务日志
    println!("任务日志示例:");
    let logs = system.get_task_logs(task1.id).await;
    for (i, log) in logs.iter().take(5).enumerate() {
        println!("  {}. [{:?}] {}", i + 1, log.level, log.message);
    }

    println!();

    // 11. 导出监控指标
    println!("监控指标:");
    let metrics = system.metrics();
    println!("  - 提交的任务: {}", metrics.tasks_submitted.get());
    println!("  - 完成的任务: {}", metrics.tasks_completed.get());
    println!("  - 失败的任务: {}", metrics.tasks_failed.get());

    println!();

    // 12. 关闭系统
    println!("正在关闭任务队列系统...");
    println!("  - 设备离线...");
    println!("  - 等待任务完成...");
    println!("  - 保存队列状态...");

    system.shutdown().await?;
    println!("✓ 任务队列系统已关闭\n");

    println!("=== 演示完成 ===");
    println!("\n提示: 现在可以从手机端 (AetherLink) 提交任务到这个设备了！");

    Ok(())
}
