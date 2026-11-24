//! ACE Reflector / Curator — 对话反思与策略条目沉淀
//!
//! 实现 ACE（arXiv:2510.04618）的核心循环：
//! - Generator：即现有 agent 对话循环（不在此模块）
//! - Reflector：归档后对会话做一次 LLM 反思，蒸馏出可复用策略（[`build_reflect_prompt`]）
//! - Curator：**纯代码确定性合并**（[`curate_bullets`]），向量去重，禁止 LLM 整体改写，
//!   从根源避免 context collapse
//!
//! 条目带 helpful/harmful 计数器，作为权重演化信号；
//! 注入上下文时用 [`playbook_context`] 按语义相关性检索。

use serde::Deserialize;

use crate::ai_panel::AiConversation;
use crate::memory_store::{BulletFilter, MemoryScope, MemoryStore, PlaybookBullet};

/// 向量去重阈值（L2 距离；归一化向量下约等于余弦相似度 > 0.9）
const DEDUP_MAX_DISTANCE: f32 = 0.45;

/// 反思产物：一条新策略
#[derive(Clone, Debug, Deserialize)]
pub struct NewBullet {
    /// 分类：tool_use / coding_style / pitfalls / project_facts / workflow
    pub section: String,
    pub content: String,
    /// 适用范围："global"（通用经验）/ "project"（仅当前项目）；缺省按项目
    #[serde(default)]
    pub scope: Option<String>,
}

impl NewBullet {
    fn resolved_scope(&self) -> MemoryScope {
        match self.scope.as_deref() {
            Some("global") => MemoryScope::Global,
            _ => MemoryScope::Project,
        }
    }
}

// ============================================================================
// Reflector：会话 → 策略候选（LLM 调用在 ai_warm_data 的后台线程中执行）
// ============================================================================

/// 构造反思 prompt（ACE Reflector 角色）
///
/// 输入会话轨迹，要求 LLM 输出 JSON 数组形式的策略条目。
/// 只提取可复用经验，不复述对话内容。
pub fn build_reflect_prompt(conv: &AiConversation) -> String {
    // 截取最近若干轮，控制 token 开销
    let mut transcript = String::new();
    for msg in conv.messages.iter().rev().take(12).rev() {
        let role = format!("{:?}", msg.role);
        let content: String = msg.content.chars().take(600).collect();
        transcript.push_str(&format!("【{}】{}\n\n", role, content));
    }

    format!(
        "你是一个 AI 助手的「反思器」。下面是一段刚结束的对话轨迹。\n\
         请从中提炼**可跨会话复用的经验策略**，供助手以后参考。\n\n\
         要求：\n\
         - 只提炼有长期价值的策略：工具使用技巧、该项目的事实/约定、踩过的坑、有效的协作流程\n\
         - 不要复述对话内容，不要记录一次性的问答\n\
         - 每条策略是一句具体、可执行的陈述（不超过 80 字）\n\
         - 判断适用范围：与具体项目无关的通用经验 scope 填 \"global\"；\n\
           依赖当前项目事实/约定/路径的经验 scope 填 \"project\"\n\
         - 如果没有值得沉淀的内容，返回空数组 []\n\n\
         严格输出 JSON 数组（不要输出其他内容）：\n\
         [{{\"section\": \"tool_use|coding_style|pitfalls|project_facts|workflow\", \"content\": \"...\", \"scope\": \"global|project\"}}]\n\n\
         对话轨迹：\n{}",
        transcript
    )
}

/// 解析 LLM 反思输出为策略列表（容错：容忍 markdown 代码围栏和前后杂文本）
pub fn parse_bullets(response: &str) -> Vec<NewBullet> {
    // 提取第一个 '[' 到最后一个 ']' 之间的 JSON
    let start = match response.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match response.rfind(']') {
        Some(i) if i > start => i,
        _ => return Vec::new(),
    };
    let json_str = &response[start..=end];

    serde_json::from_str::<Vec<NewBullet>>(json_str)
        .unwrap_or_default()
        .into_iter()
        .filter(|b| !b.content.trim().is_empty())
        .collect()
}

// ============================================================================
// Curator：确定性合并（无 LLM，向量去重 + 计数器更新）
// ============================================================================

/// 沉淀策略：决定哪些作用域允许写入
#[derive(Clone, Copy, Debug)]
pub struct CuratePolicy<'a> {
    /// 是否允许写入全局记忆
    pub allow_global: bool,
    /// 当前项目（哈希, 名称）；None 表示未打开工作区或项目记忆被关闭
    pub project: Option<(&'a str, &'a str)>,
}

impl<'a> CuratePolicy<'a> {
    pub fn disabled(&self) -> bool {
        !self.allow_global && self.project.is_none()
    }

    fn resolve(&self, wanted: MemoryScope) -> Option<(MemoryScope, String, String)> {
        match wanted {
            MemoryScope::Project => match self.project {
                Some((h, n)) if !h.is_empty() => {
                    Some((MemoryScope::Project, h.to_string(), n.to_string()))
                }
                _ if self.allow_global => Some((MemoryScope::Global, String::new(), String::new())),
                _ => None,
            },
            MemoryScope::Global => {
                if self.allow_global {
                    Some((MemoryScope::Global, String::new(), String::new()))
                } else {
                    self.project
                        .filter(|(h, _)| !h.is_empty())
                        .map(|(h, n)| (MemoryScope::Project, h.to_string(), n.to_string()))
                }
            }
        }
    }
}

/// 将策略候选合并进 playbook（确定性操作）
///
/// - 作用域由 LLM 判定并经 [`CuratePolicy`] 收敛：`global` 进全局记忆，其余归入当前项目记忆
/// - 去重只在同一作用域内进行（同项目的相似条目强化计数，跨项目互不干扰）
/// - 记忆行为开关关闭的作用域不会被写入（项目记忆关闭时全局条目仍按全局处理）
///
/// 返回新插入的条目数。
pub fn curate_bullets(
    store: &dyn MemoryStore,
    bullets: Vec<NewBullet>,
    policy: CuratePolicy<'_>,
) -> Result<usize, String> {
    if policy.disabled() {
        return Ok(0);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut inserted = 0;
    for bullet in bullets {
        let Some((scope, ws_hash, ws_name)) = policy.resolve(bullet.resolved_scope()) else {
            continue;
        };
        let embedding = crate::embedding::embed_text(&bullet.content);

        // 向量查重：同一作用域内最近邻距离小于阈值视为同一条策略
        let filter = BulletFilter::exact(scope, ws_hash.clone());
        let nearest = store.search_bullets(&embedding, &filter, 1)?;
        if let Some((existing, distance)) = nearest.first() {
            if *distance < DEDUP_MAX_DISTANCE {
                store.bullet_feedback(&existing.id, true)?;
                continue;
            }
        }

        store.upsert_bullet(&PlaybookBullet {
            id: crate::memory_store::new_id("b"),
            section: bullet.section,
            content: bullet.content,
            helpful_count: 0,
            harmful_count: 0,
            scope,
            workspace_hash: ws_hash,
            workspace_name: ws_name,
            embedding: Some(embedding),
            created_at: now,
            updated_at: now,
        })?;
        inserted += 1;
    }
    Ok(inserted)
}

/// 完整反思-沉淀流程（在后台线程调用）
///
/// `policy` 携带工作区归属与记忆开关；返回新沉淀的条目数。
/// LLM 调用失败会向上传播，由调用方记录日志。
pub fn reflect_and_curate(
    store: &dyn MemoryStore,
    client: &aether_ai::AiClient,
    conv: &AiConversation,
    policy: CuratePolicy<'_>,
) -> Result<usize, String> {
    if policy.disabled() {
        return Ok(0);
    }
    let prompt = build_reflect_prompt(conv);
    let response = client
        .complete(&prompt)
        .map_err(|e| format!("反思 LLM 调用失败: {}", e))?;
    let bullets = parse_bullets(&response);
    if bullets.is_empty() {
        return Ok(0);
    }
    curate_bullets(store, bullets, policy)
}

// ============================================================================
// 检索注入：组装 playbook 上下文（供系统提示注入）
// ============================================================================

/// 按查询语义检索 playbook 条目，格式化为可注入系统提示的文本
///
/// `filter` 决定可见作用域（全局记忆开关 + 当前项目记忆）。
pub fn playbook_context(
    store: &dyn MemoryStore,
    query: &str,
    filter: &BulletFilter,
    k: usize,
) -> Result<String, String> {
    let embedding = crate::embedding::embed_text(query);
    let hits = store.search_bullets(&embedding, filter, k)?;
    Ok(format_bullets(&hits))
}

/// 将检索到的条目格式化为系统提示文本（空列表返回空串）
pub fn format_bullets(hits: &[(PlaybookBullet, f32)]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from("## 已沉淀的经验策略（按相关性排序）\n");
    for (bullet, _distance) in hits {
        out.push_str(&format!(
            "- [{}] {}（有效 {} 次 / 无效 {} 次）\n",
            bullet.section, bullet.content, bullet.helpful_count, bullet.harmful_count
        ));
    }
    out
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aether_db_store::AetherDbMemoryStore;
    use crate::ai_panel::{AiConversation, AiMessage, AiRole};

    #[test]
    fn test_parse_bullets_clean_json() {
        let resp = r#"[{"section": "tool_use", "content": "git 操作前先检查工作区状态"}]"#;
        let bullets = parse_bullets(resp);
        assert_eq!(bullets.len(), 1);
        assert_eq!(bullets[0].section, "tool_use");
    }

    #[test]
    fn test_parse_bullets_with_markdown_fence() {
        let resp = "以下是提炼的策略：\n```json\n[{\"section\": \"pitfalls\", \"content\": \"不要在锁内做 IO\"}]\n```\n希望有帮助";
        let bullets = parse_bullets(resp);
        assert_eq!(bullets.len(), 1);
        assert_eq!(bullets[0].content, "不要在锁内做 IO");
    }

    #[test]
    fn test_parse_bullets_empty_and_garbage() {
        assert!(parse_bullets("[]").is_empty());
        assert!(parse_bullets("没有值得沉淀的内容").is_empty());
        assert!(parse_bullets("{not json}").is_empty());
        // 空内容条目被过滤
        assert!(parse_bullets(r#"[{"section": "x", "content": "  "}]"#).is_empty());
    }

    #[test]
    fn test_build_reflect_prompt_contains_transcript() {
        let mut conv = AiConversation::new("c1".into(), "t".into());
        conv.messages
            .push(AiMessage::new(AiRole::User, "如何配置 LSP？".into()));
        let prompt = build_reflect_prompt(&conv);
        assert!(prompt.contains("如何配置 LSP？"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_curate_bullets_dedup() {
        let dir = std::env::temp_dir().join(format!(
            "aether_reflect_test_{}",
            crate::memory_store::new_id("d")
        ));
        let store = AetherDbMemoryStore::open(&dir, crate::embedding::EmbeddingModel::DIM).unwrap();
        let ws = CuratePolicy {
            allow_global: true,
            project: Some(("ws-1", "demo")),
        };

        // 第一次：插入新条目（缺省 project → 归属当前工作区）
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "tool_use".into(),
                content: "git 操作前先检查工作区状态".into(),
                scope: None,
            }],
            ws,
        )
        .unwrap();
        assert_eq!(n, 1);

        // 第二次：同一工作区的完全相同条目应被去重，转为 helpful+1
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "tool_use".into(),
                content: "git 操作前先检查工作区状态".into(),
                scope: None,
            }],
            ws,
        )
        .unwrap();
        assert_eq!(n, 0);

        let bullets = store.list_bullets(None).unwrap();
        assert_eq!(bullets.len(), 1);
        assert_eq!(bullets[0].helpful_count, 1);
        assert_eq!(bullets[0].scope, MemoryScope::Project);
        assert_eq!(bullets[0].workspace_hash, "ws-1");

        // 另一个工作区相同内容：视为独立记忆，不串项目
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "tool_use".into(),
                content: "git 操作前先检查工作区状态".into(),
                scope: None,
            }],
            CuratePolicy {
                allow_global: true,
                project: Some(("ws-2", "other")),
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.list_bullets(None).unwrap().len(), 2);

        // 显式 global 的通用经验进全局记忆
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "coding_style".into(),
                content: "提交信息使用中文描述".into(),
                scope: Some("global".into()),
            }],
            ws,
        )
        .unwrap();
        assert_eq!(n, 1);
        let global = store
            .list_bullets_filtered(None, &BulletFilter::exact(MemoryScope::Global, ""))
            .unwrap();
        assert_eq!(global.len(), 1);

        // 关闭全局记忆 → global 候选降级进当前项目
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "coding_style".into(),
                content: "注释统一使用中文".into(),
                scope: Some("global".into()),
            }],
            CuratePolicy {
                allow_global: false,
                project: Some(("ws-1", "demo")),
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            store
                .list_bullets_filtered(None, &BulletFilter::exact(MemoryScope::Global, ""))
                .unwrap()
                .len(),
            1,
            "关闭全局记忆后不得新增全局条目"
        );

        // 两个开关都关闭 → 不沉淀任何条目
        let n = curate_bullets(
            &store,
            vec![NewBullet {
                section: "coding_style".into(),
                content: "这条不应入库".into(),
                scope: None,
            }],
            CuratePolicy {
                allow_global: false,
                project: None,
            },
        )
        .unwrap();
        assert_eq!(n, 0);

        // playbook_context 按可见性过滤：ws-1 视角看不到 ws-2 的项目记忆
        let filter = BulletFilter::visible(true, Some("ws-1".into()));
        let ctx = playbook_context(&store, "git 工作区", &filter, 5).unwrap();
        assert!(ctx.contains("git 操作前先检查工作区状态"));
        let no_global = store
            .list_bullets_filtered(None, &BulletFilter::visible(false, Some("ws-1".into())))
            .unwrap();
        assert!(!no_global.is_empty());
        assert!(
            no_global.iter().all(|b| b.scope == MemoryScope::Project),
            "关闭全局记忆后可见列表不得包含全局条目"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
