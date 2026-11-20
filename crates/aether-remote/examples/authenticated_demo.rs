//! 带用户认证的完整演示

use aether_remote::queue::{QueueConfig, SupabaseConfig, Task, TaskQueueSystem};
use std::io::{self, Write};
use std::path::PathBuf;
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== AetherStudio 用户认证演示 ===\n");

    // 1. 创建配置
    let mut config = QueueConfig::new(
        PathBuf::from("data/auth_demo.aedb"),
        "My Windows PC".to_string(),
    );
    
    config.supabase_config = SupabaseConfig::default();

    // 2. 创建任务队列系统
    println!("正在初始化任务队列系统...");
    let system = TaskQueueSystem::new(config).await?;
    println!("✓ 系统初始化完成\n");

    // 3. 检查是否有保存的会话
    if system.session_manager().is_authenticated().await {
        println!("✓ 检测到已保存的会话");
        
        if let Some(session) = system.session_manager().get_session().await {
            println!("  - 用户: {}", session.user.email);
            println!("  - 租户 ID: {:?}", session.user.tenant_id());
        }
        
        println!("\n是否使用此会话？(y/n): ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if input.trim().to_lowercase() != "y" {
            system.logout().await?;
            println!("已清除会话\n");
        }
    }

    // 4. 如果没有会话，提示登录
    if !system.session_manager().is_authenticated().await {
        println!("请先登录或注册\n");
        println!("1. 登录");
        println!("2. 注册");
        print!("请选择 (1/2): ");
        io::stdout().flush()?;
        
        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;
        
        match choice.trim() {
            "1" => {
                // 登录
                print!("\n邮箱: ");
                io::stdout().flush()?;
                let mut email = String::new();
                io::stdin().read_line(&mut email)?;
                let email = email.trim();
                
                print!("密码: ");
                io::stdout().flush()?;
                let mut password = String::new();
                io::stdin().read_line(&mut password)?;
                let password = password.trim();
                
                println!("\n正在登录...");
                match system.login(email, password).await {
                    Ok(_) => println!("✓ 登录成功\n"),
                    Err(e) => {
                        eprintln!("✗ 登录失败: {}\n", e);
                        return Err(e.into());
                    }
                }
            }
            "2" => {
                // 注册
                print!("\n邮箱: ");
                io::stdout().flush()?;
                let mut email = String::new();
                io::stdin().read_line(&mut email)?;
                let email = email.trim();
                
                print!("密码: ");
                io::stdout().flush()?;
                let mut password = String::new();
                io::stdin().read_line(&mut password)?;
                let password = password.trim();
                
                print!("租户 ID (可选，留空创建新租户): ");
                io::stdout().flush()?;
                let mut tenant_id = String::new();
                io::stdin().read_line(&mut tenant_id)?;
                let tenant_id = tenant_id.trim();
                let tenant_id = if tenant_id.is_empty() {
                    None
                } else {
                    Some(tenant_id.to_string())
                };
                
                println!("\n正在注册...");
                match system.signup(email, password, tenant_id).await {
                    Ok(_) => println!("✓ 注册成功\n"),
                    Err(e) => {
                        eprintln!("✗ 注册失败: {}\n", e);
                        return Err(e.into());
                    }
                }
            }
            _ => {
                eprintln!("无效的选择");
                return Ok(());
            }
        }
    }

    // 5. 显示当前用户信息
    if let Some(session) = system.session_manager().get_session().await {
        println!("当前用户:");
        println!("  - ID: {}", session.user.id);
        println!("  - 邮箱: {}", session.user.email);
        println!("  - 租户 ID: {:?}", session.user.tenant_id());
        println!();
    }

    // 6. 启动系统
    println!("正在启动任务队列系统...");
    println!("  - 注册设备...");
    println!("  - 启动心跳...");
    println!("  - 启动 Realtime 监听...");
    
    match system.start().await {
        Ok(_) => println!("✓ 系统已启动\n"),
        Err(e) => {
            eprintln!("✗ 启动失败: {}\n", e);
            return Err(e.into());
        }
    }

    // 7. 获取设备信息
    if let Some(device_id) = system.device_manager().device_id().await {
        println!("设备信息:");
        println!("  - 设备 ID: {}", device_id);
        println!("  - 状态: online");
        println!();
    }

    // 8. 提交示例任务
    println!("提交示例任务...");
    
    let session = system.session_manager().get_session().await.unwrap();
    let tenant_id = Uuid::parse_str(&session.user.tenant_id().unwrap())?;
    let user_id = Uuid::parse_str(&session.user.id)?;

    let task = Task::new(
        tenant_id,
        user_id,
        "测试任务".to_string(),
        if cfg!(windows) {
            "cmd /c echo Hello from AetherStudio".to_string()
        } else {
            "echo Hello from AetherStudio".to_string()
        },
    );
    
    system.submit_task(task.clone()).await?;
    println!("✓ 任务已提交: {}\n", task.title);

    // 9. 等待任务完成
    println!("等待任务执行...");
    for i in 1..=5 {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        if let Some(status) = system.get_task_status(task.id).await {
            println!("  [{}s] 任务状态: {:?}", i, status);
            
            if status == aether_remote::queue::TaskStatus::Completed {
                break;
            }
        }
    }

    // 10. 查看结果
    if let Some(result) = system.get_task_result(task.id).await {
        println!("\n任务结果:");
        println!("  - 状态: {:?}", result.status);
        println!("  - 退出码: {:?}", result.exit_code);
        println!("  - 耗时: {}ms", result.duration_ms);
        if !result.stdout.is_empty() {
            println!("  - 输出: {}", result.stdout);
        }
    }

    // 11. 关闭系统
    println!("\n正在关闭系统...");
    system.shutdown().await?;
    println!("✓ 系统已关闭\n");

    println!("=== 演示完成 ===");
    println!("\n提示: 会话已保存，下次启动时将自动登录");

    Ok(())
}
