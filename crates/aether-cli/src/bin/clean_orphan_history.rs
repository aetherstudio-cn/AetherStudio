//! 历史会话记录清理工具
//!
//! 直接操作 AetherDB 数据库（aether_memory.aedb），支持两种模式：
//! - 默认：清理无工作区绑定的历史会话（workspace_hash 为空）
//! - --all：清空全部历史会话记录

use std::io::Write;

use aether_db::kind;
use aether_db::AetherDb;

/// 向量维度（须与 aether-ai-panel 的 EmbeddingModel::DIM 一致）
const EMBEDDING_DIM: usize = 512;

/// 会话 payload 的最小反序列化视图（只取展示字段）
#[derive(serde::Deserialize)]
struct ConvLite {
    #[serde(default)]
    title: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    message_count: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let clear_all = args.iter().any(|a| a == "--all");

    let base_dir = dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Aether")
        .join("conversations");
    let db_path = base_dir.join("aether_memory.aedb");

    if !db_path.exists() {
        eprintln!("数据库不存在: {}", db_path.display());
        std::process::exit(1);
    }

    println!("数据库路径: {}", db_path.display());

    let mut db = match AetherDb::open(&db_path, EMBEDDING_DIM) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("打开数据库失败: {}", e);
            std::process::exit(1);
        }
    };

    // 会话总览（tag = workspace_hash）
    let convs = db.scan(kind::CONVERSATION);
    let total = convs.len();
    println!("总会话数: {}", total);

    {
        let mut dist: Vec<(String, usize)> = Vec::new();
        for r in &convs {
            match dist.iter_mut().find(|(h, _)| *h == r.tag) {
                Some((_, n)) => *n += 1,
                None => dist.push((r.tag.clone(), 1)),
            }
        }
        dist.sort_by(|a, b| b.1.cmp(&a.1));
        println!("\nworkspace_hash 分布:");
        for (hash, count) in dist {
            let display = if hash.is_empty() {
                "(空)".to_string()
            } else {
                hash
            };
            println!("  {}: {} 条", display, count);
        }
    }

    let msg_total = db.scan(kind::MESSAGE).len();
    println!("消息总数: {}", msg_total);

    // 待删目标：--all 为全部会话，否则为无工作区绑定（tag 为空）的会话
    let targets: Vec<(String, String)> = convs
        .iter()
        .filter(|r| clear_all || r.tag.is_empty())
        .map(|r| {
            let lite: ConvLite = serde_json::from_slice(&r.payload).unwrap_or(ConvLite {
                title: String::new(),
                mode: String::new(),
                message_count: 0,
            });
            let display = format!(
                "  - [{}] {} ({} 条消息, 模式: {})",
                r.key, lite.title, lite.message_count, lite.mode
            );
            (r.key.clone(), display)
        })
        .collect();

    if targets.is_empty() {
        if clear_all {
            println!("\n数据库已为空，无需清理。");
        } else {
            println!("没有需要清理的记录。");
        }
        return;
    }

    println!("\n将要删除的{}记录:", if clear_all { "全部" } else { "" });
    for (_, display) in &targets {
        println!("{}", display);
    }

    // 确认删除
    print!(
        "\n确认删除这 {} 条会话吗？此操作不可恢复！(y/N): ",
        targets.len()
    );
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();
    if input != "y" && input != "yes" {
        println!("已取消。");
        return;
    }

    // 级联删除：先删消息（含向量索引）再删会话
    let mut conv_deleted = 0usize;
    let mut msg_deleted = 0usize;
    for (id, _) in &targets {
        match db.delete_by_tag(kind::MESSAGE, id) {
            Ok(n) => msg_deleted += n,
            Err(e) => {
                eprintln!("删除会话 {} 的消息失败: {}", id, e);
                std::process::exit(1);
            }
        }
        if let Err(e) = db.delete(kind::CONVERSATION, id) {
            eprintln!("删除会话 {} 失败: {}", id, e);
            std::process::exit(1);
        }
        conv_deleted += 1;
    }

    // 回收空间
    if let Err(e) = db.force_compact() {
        eprintln!("警告: 空间回收失败（不影响删除结果）: {}", e);
    }

    println!(
        "\n清理完成！共删除 {} 条会话、{} 条消息。",
        conv_deleted, msg_deleted
    );
}
