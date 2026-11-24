//! 长任务上下文压缩（免训练 harness 层）。
//!
//! 对标 Cat/SWE-Compressor 的三段式工作区 `C = (Q, M, I_k)`：
//! - Q 固定锚点：system prompt + 用户目标（由 `build_chat_prompt` 注入，永不压缩）；
//! - M 长期记忆：被近期窗口淘汰的历史压缩成的结构化摘要（本模块产出）；
//! - I_k 近期高保真：最近若干步原文（由 `ai_panel` 的窗口选择保留）。
//!
//! P1 采用**免训练抽取式压缩**：从淘汰历史中保留决策/目标/文件路径/错误/约束等
//! 关键信号，丢弃冗余叙述，使早期上下文不被静默丢弃。后续可升级为模型摘要式
//! （Acon 的对比反馈优化准则），接口保持不变。

use crate::ai_panel::{AiMessage, AiRole};

/// 长期记忆默认字符上限（超出保留较近部分，防止记忆本身无限膨胀）。
pub const MEMORY_CAP: usize = 6000;

/// 单条消息抽取的要点上限（避免单条长消息塞满记忆）。
const PER_MSG_POINTS: usize = 4;

/// 关键信号标记：命中这些词的行优先保留（决策/目标/错误/约束/产物）。
const KEY_MARKERS: &[&str] = &[
    "目标",
    "任务",
    "决定",
    "决策",
    "采用",
    "方案",
    "修复",
    "解决",
    "完成",
    "已改",
    "新增",
    "删除",
    "错误",
    "失败",
    "报错",
    "异常",
    "约束",
    "必须",
    "注意",
    "不要",
    "禁止",
    "TODO",
    "FIXME",
    "结论",
    "结果",
    "根因",
    "原因",
    "验证",
    "测试通过",
    "编译",
];

/// 判断一行是否携带关键信号（关键词 / 文件路径 / 错误标记）。
fn is_signal_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    if KEY_MARKERS.iter().any(|m| t.contains(m)) {
        return true;
    }
    // 文件路径（含分隔符且带扩展名）——保留改动落点
    if (t.contains('/') || t.contains('\\')) && t.contains('.') && !t.contains(' ') {
        return true;
    }
    // 常见错误/诊断标记
    if t.contains("error[") || t.contains("error:") || t.contains("panic") || t.contains("✗") {
        return true;
    }
    false
}

/// 截取字符安全的短摘要（按字符边界，避免半截 UTF-8）。
fn short(s: &str, max_chars: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    let cut = t
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(t.len());
    format!("{}…", &t[..cut])
}

/// 从单条消息抽取要点：优先关键信号行，不足时补首行作 gist。
fn extract_points(msg: &AiMessage) -> Vec<String> {
    let mut points: Vec<String> = Vec::new();
    for line in msg.content.lines() {
        if is_signal_line(line) {
            let p = short(line, 160);
            if !p.is_empty() && !points.contains(&p) {
                points.push(p);
            }
            if points.len() >= PER_MSG_POINTS {
                break;
            }
        }
    }
    // 无信号行时保留首行 gist（用户意图/工具结果概述），避免整条丢失
    if points.is_empty() {
        if let Some(first) = msg.content.lines().find(|l| !l.trim().is_empty()) {
            let p = short(first, 120);
            if !p.is_empty() {
                points.push(p);
            }
        }
    }
    points
}

/// 角色标签（记忆中标注来源，便于模型理解）。
fn role_tag(role: &AiRole) -> &'static str {
    match role {
        AiRole::User => "用户",
        AiRole::Assistant => "助手",
        AiRole::Tool => "工具",
        _ => "系统",
    }
}

/// 把被淘汰的历史 `dropped` 压缩进长期记忆，与 `existing` 合并并限长到 `cap`。
///
/// 抽取式：保留每条消息的关键信号行（决策/目标/路径/错误/约束），去重、加角色标签。
/// 合并后若超 `cap` 字符，保留较近（尾部）内容——近期记忆比远期更相关。
pub fn compress_to_memory(dropped: &[AiMessage], existing: &str, cap: usize) -> String {
    let mut fresh: Vec<String> = Vec::new();
    for msg in dropped {
        if matches!(msg.role, AiRole::System) {
            continue;
        }
        for p in extract_points(msg) {
            let entry = format!("[{}] {}", role_tag(&msg.role), p);
            if !fresh.contains(&entry) {
                fresh.push(entry);
            }
        }
    }
    if fresh.is_empty() {
        return bound_memory(existing, cap);
    }
    let merged = if existing.trim().is_empty() {
        fresh.join("\n")
    } else {
        format!("{}\n{}", existing.trim_end(), fresh.join("\n"))
    };
    bound_memory(&merged, cap)
}

/// 限长：超 cap 时按行保留尾部（较近记忆），不切断单行 UTF-8。
fn bound_memory(memory: &str, cap: usize) -> String {
    if memory.chars().count() <= cap {
        return memory.to_string();
    }
    let lines: Vec<&str> = memory.lines().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut total = 0usize;
    for line in lines.iter().rev() {
        let w = line.chars().count() + 1;
        if total + w > cap && !kept.is_empty() {
            break;
        }
        total += w;
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: AiRole, content: &str) -> AiMessage {
        AiMessage::new(role, content.to_string())
    }

    #[test]
    fn keeps_signal_lines_drops_filler() {
        let dropped = vec![msg(
            AiRole::Assistant,
            "好的我来看看。\n决定采用三段式上下文方案。\n这里是一些无关的寒暄叙述。\n修复了 conpty.rs 的闪窗问题。",
        )];
        let mem = compress_to_memory(&dropped, "", MEMORY_CAP);
        assert!(
            mem.contains("决定采用三段式上下文方案"),
            "决策行应保留: {mem}"
        );
        assert!(mem.contains("修复了 conpty.rs"), "修复行应保留: {mem}");
        assert!(!mem.contains("无关的寒暄"), "冗余叙述应丢弃: {mem}");
    }

    #[test]
    fn keeps_file_paths_and_errors() {
        let dropped = vec![msg(
            AiRole::Tool,
            "运行结果如下\nerror[E0308]: mismatched types\ncrates/aether-win32/src/conpty.rs\n普通输出行",
        )];
        let mem = compress_to_memory(&dropped, "", MEMORY_CAP);
        assert!(mem.contains("error[E0308]"), "错误行应保留: {mem}");
        assert!(mem.contains("conpty.rs"), "文件路径应保留: {mem}");
    }

    #[test]
    fn empty_dropped_preserves_existing() {
        let mem = compress_to_memory(&[], "已有记忆", MEMORY_CAP);
        assert_eq!(mem, "已有记忆");
    }

    #[test]
    fn no_signal_keeps_gist() {
        let dropped = vec![msg(AiRole::User, "帮我优化一下终端启动的速度问题")];
        let mem = compress_to_memory(&dropped, "", MEMORY_CAP);
        assert!(mem.contains("终端启动"), "无信号行时保留首行 gist: {mem}");
    }

    #[test]
    fn merges_existing_and_new() {
        let dropped = vec![msg(AiRole::Assistant, "决定改用 mmap 读取大文件")];
        let mem = compress_to_memory(&dropped, "旧记忆条目", MEMORY_CAP);
        assert!(mem.contains("旧记忆条目"));
        assert!(mem.contains("mmap"));
    }

    #[test]
    fn bounds_to_cap_keeps_recent() {
        // 构造超长既有记忆 + 新条目，验证限长保留尾部（较近）
        let existing = (0..2000)
            .map(|i| format!("旧条目{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let dropped = vec![msg(AiRole::Assistant, "最新决策：采用子目标驱动续跑")];
        let mem = compress_to_memory(&dropped, &existing, 500);
        assert!(
            mem.chars().count() <= 500,
            "记忆应限长: {}",
            mem.chars().count()
        );
        assert!(mem.contains("最新决策"), "较近的新条目应保留: {mem}");
    }

    #[test]
    fn dedups_repeated_points() {
        let dropped = vec![
            msg(AiRole::Assistant, "修复了闪窗问题"),
            msg(AiRole::Assistant, "修复了闪窗问题"),
        ];
        let mem = compress_to_memory(&dropped, "", MEMORY_CAP);
        assert_eq!(
            mem.matches("修复了闪窗问题").count(),
            1,
            "重复要点应去重: {mem}"
        );
    }

    #[test]
    fn skips_system_messages() {
        let dropped = vec![msg(AiRole::System, "系统欢迎语不应进入记忆")];
        let mem = compress_to_memory(&dropped, "", MEMORY_CAP);
        assert!(mem.is_empty(), "System 消息应跳过: {mem}");
    }
}
