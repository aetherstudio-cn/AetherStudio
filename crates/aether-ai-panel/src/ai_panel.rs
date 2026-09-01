use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aether_ai::{AiClient, AiStreamEvent, ChatImage, ChatMessage};
use aether_shared::settings::AiSettings;

use crate::ai_context::{truncate_middle, AiContextAttachment};
use crate::ai_prompt::{build_chat_prompt, AiMode};

/// 编辑器上下文提供者 trait：解耦 AI 面板对编辑器状态的直接依赖。
/// 由 aether-win32 中的 EditorState 实现，提供 AI 上下文收集所需的只读访问。
pub trait EditorContextProvider {
    /// 根据附件列表收集 AI 上下文
    fn gather_context(&self, attachments: &[AiContextAttachment]) -> String;
}

/// 脱敏错误消息，避免泄漏 API 密钥等敏感信息
/// SEC-C04: 用于 test_connection 路径等所有 UI 错误展示
/// AI-M04: 扩展覆盖 x-api-key、URL 参数、响应体中的密钥
/// H-02: 循环移除所有 Bearer/x-api-key/authorization 出现，而非仅首个
///
/// 注意：当前代码路径已改用 `AiError::safe_display()`，此函数保留供
/// 需要对原始字符串（如日志）做脱敏的场景使用。
#[allow(dead_code)]
pub fn sanitize_error(err: &str) -> String {
    let mut result = err.to_string();

    // H-02: 循环移除所有 "Bearer xxx" 出现（之前仅处理首个，多 Token 时第二个泄露）
    while let Some(pos) = result.find("Bearer ") {
        let start = pos + 7;
        let end = result[start..]
            .find(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
            .map(|p| start + p)
            .unwrap_or(result.len());
        if end > start {
            result.replace_range(start..end, "[REDACTED]");
        } else {
            break;
        }
    }
    // H-02: 循环移除所有 x-api-key 头（支持冒号和等号分隔，大小写不敏感）
    let lower = result.to_lowercase();
    let mut search_from = 0;
    while let Some(rel_pos) = lower[search_from..].find("x-api-key") {
        let pos = search_from + rel_pos;
        // 跳过 "x-api-key" 本身（9 字符）
        let mut value_start = pos + 9;
        // 跳过分隔符（: 或 =）和可选空格
        let rest = &result[value_start..];
        let trimmed_start = rest
            .find(|c: char| c != ':' && c != '=' && c != ' ' && c != '\t')
            .map(|p| value_start + p)
            .unwrap_or(value_start);
        value_start = trimmed_start;
        let end = result[value_start..]
            .find(|c: char| ['\n', '\r'].contains(&c))
            .map(|p| value_start + p)
            .unwrap_or(result.len());
        if end > value_start {
            result.replace_range(value_start..end, "[REDACTED]");
        }
        search_from = pos + 9;
        if search_from >= result.len() {
            break;
        }
    }

    // H-02: 循环移除所有 authorization 头（大小写不敏感）
    let lower = result.to_lowercase();
    let mut search_from = 0;
    while let Some(rel_pos) = lower[search_from..].find("authorization") {
        let pos = search_from + rel_pos;
        let mut value_start = pos + 13; // "authorization" = 13 字符
        let rest = &result[value_start..];
        let trimmed_start = rest
            .find(|c: char| ![':', '=', ' ', '\t'].contains(&c))
            .map(|p| value_start + p)
            .unwrap_or(value_start);
        value_start = trimmed_start;
        let end = result[value_start..]
            .find(|c: char| ['\n', '\r'].contains(&c))
            .map(|p| value_start + p)
            .unwrap_or(result.len());
        if end > value_start {
            result.replace_range(value_start..end, "[REDACTED]");
        }
        search_from = pos + 13;
        if search_from >= result.len() {
            break;
        }
    }

    // 限制长度（H-02: 在 UTF-8 字符边界截断，避免半截 Token 可见）
    if result.len() > 500 {
        let safe_len = result.floor_char_boundary(500);
        result.truncate(safe_len);
        result.push_str("...");
    }
    result
}

/// 诊断流式错误：根据错误消息内容提供详细的诊断信息
fn diagnose_stream_error(err: &str) -> String {
    let err_lower = err.to_lowercase();
    let sanitized = sanitize_error(err);
    
    let mut msg = String::new();
    
    // 判断错误类型
    if err_lower.contains("network") || err_lower.contains("connection") 
        || err_lower.contains("dns") || err_lower.contains("ssl") 
        || err_lower.contains("timeout") || err_lower.contains("timed out") {
        // 网络错误
        msg.push_str("[网络错误] 无法连接到 API 服务器\n\n");
        msg.push_str("🔍 错误详情:\n");
        msg.push_str(&format!("• 错误信息: {}\n\n", sanitized));
        
        msg.push_str("📊 诊断分析:\n");
        if err_lower.contains("dns") {
            msg.push_str("• DNS 解析失败：无法解析 API 服务器域名\n");
            msg.push_str("• 可能原因: DNS 服务器故障、域名错误、网络连接中断\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查网络连接是否正常\n");
            msg.push_str("2. 尝试使用其他 DNS 服务器（如 8.8.8.8）\n");
            msg.push_str("3. 检查 API 基础 URL 配置是否正确\n");
        } else if err_lower.contains("ssl") || err_lower.contains("certificate") {
            msg.push_str("• SSL/TLS 证书验证失败\n");
            msg.push_str("• 可能原因: 证书过期、系统时间不准确、中间人攻击\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查系统时间是否准确\n");
            msg.push_str("2. 更新系统证书库\n");
            msg.push_str("3. 如使用企业代理，可能需要安装企业根证书\n");
        } else if err_lower.contains("timeout") || err_lower.contains("timed out") {
            msg.push_str("• 连接超时：服务器未在预期时间内响应\n");
            msg.push_str("• 可能原因: 网络延迟高、服务器负载高、防火墙拦截\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查网络延迟（ping API 服务器）\n");
            msg.push_str("2. 检查防火墙/代理设置\n");
            msg.push_str("3. 稍后重试或联系 API 提供商\n");
        } else {
            msg.push_str("• 网络连接失败\n");
            msg.push_str("• 可能原因: 网络中断、防火墙拦截、代理配置错误\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查网络连接（尝试访问其他网站）\n");
            msg.push_str("2. 检查防火墙/杀毒软件设置\n");
            msg.push_str("3. 如使用代理，确认代理配置正确\n");
        }
    } else if err_lower.contains("401") || err_lower.contains("unauthorized") {
        // 认证错误
        msg.push_str("[认证错误] API Key 验证失败\n\n");
        msg.push_str("🔍 错误详情:\n");
        msg.push_str(&format!("• 错误信息: {}\n\n", sanitized));
        msg.push_str("📊 诊断分析:\n");
        msg.push_str("• API Key 无效、过期或权限不足\n\n");
        msg.push_str("💡 建议:\n");
        msg.push_str("1. 检查 API Key 是否正确（设置 → AI → API Key）\n");
        msg.push_str("2. 确认 API Key 未过期\n");
        msg.push_str("3. 检查 API Key 是否有访问该模型的权限\n");
    } else if err_lower.contains("429") || err_lower.contains("rate limit") {
        // 速率限制
        msg.push_str("[速率限制] 请求过于频繁\n\n");
        msg.push_str("🔍 错误详情:\n");
        msg.push_str(&format!("• 错误信息: {}\n\n", sanitized));
        msg.push_str("📊 诊断分析:\n");
        msg.push_str("• 已达到 API 请求速率限制（TPM/RPM）\n\n");
        msg.push_str("💡 建议:\n");
        msg.push_str("1. 稍后重试（通常几秒到几分钟后恢复）\n");
        msg.push_str("2. 减少请求频率\n");
        msg.push_str("3. 考虑升级 API 套餐以提高限额\n");
    } else if err_lower.contains("500") || err_lower.contains("502") 
        || err_lower.contains("503") || err_lower.contains("504") {
        // 服务器错误
        msg.push_str("[服务器错误] API 服务器内部错误\n\n");
        msg.push_str("🔍 错误详情:\n");
        msg.push_str(&format!("• 错误信息: {}\n\n", sanitized));
        msg.push_str("📊 诊断分析:\n");
        msg.push_str("• API 服务器遇到内部错误或正在维护\n\n");
        msg.push_str("💡 建议:\n");
        msg.push_str("1. 稍后重试（通常是临时问题）\n");
        msg.push_str("2. 检查 API 服务状态页面\n");
        msg.push_str("3. 如持续失败，联系 API 提供商支持\n");
    } else {
        // 其他 API 错误
        msg.push_str("[API 错误] 请求失败\n\n");
        msg.push_str("🔍 错误详情:\n");
        msg.push_str(&format!("• 错误信息: {}\n\n", sanitized));
        msg.push_str("📊 诊断分析:\n");
        msg.push_str("• API 服务器返回错误响应\n\n");
        msg.push_str("💡 建议:\n");
        msg.push_str("1. 检查请求参数是否正确\n");
        msg.push_str("2. 查看 API 文档确认请求格式\n");
        msg.push_str("3. 联系 API 提供商获取支持\n");
    }
    
    msg
}

/// 诊断 API 错误：根据 AiError 类型提供详细的诊断信息
fn diagnose_api_error(e: &aether_ai::AiError) -> String {
    use aether_ai::AiError;
    
    let mut msg = String::new();
    
    match e {
        AiError::Http(err) => {
            msg.push_str("[HTTP 错误] 网络请求失败\n\n");
            msg.push_str("🔍 错误详情:\n");
            msg.push_str(&format!("• 错误信息: {}\n\n", sanitize_error(err)));
            msg.push_str("📊 诊断分析:\n");
            msg.push_str("• HTTP 请求无法完成\n");
            msg.push_str("• 可能原因: 网络问题、URL 错误、请求格式错误\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查网络连接\n");
            msg.push_str("2. 验证 API 基础 URL 配置\n");
            msg.push_str("3. 检查请求参数格式\n");
        }
        AiError::Parse(err) => {
            msg.push_str("[解析错误] 响应数据格式错误\n\n");
            msg.push_str("🔍 错误详情:\n");
            msg.push_str(&format!("• 错误信息: {}\n\n", sanitize_error(err)));
            msg.push_str("📊 诊断分析:\n");
            msg.push_str("• 无法解析 API 返回的数据\n");
            msg.push_str("• 可能原因: API 返回格式变更、响应数据损坏\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查 API 版本是否兼容\n");
            msg.push_str("2. 查看 API 文档确认响应格式\n");
            msg.push_str("3. 联系 API 提供商报告问题\n");
        }
        AiError::Config(err) => {
            msg.push_str("[配置错误] AI 配置无效\n\n");
            msg.push_str("🔍 错误详情:\n");
            msg.push_str(&format!("• 错误信息: {}\n\n", sanitize_error(err)));
            msg.push_str("📊 诊断分析:\n");
            msg.push_str("• AI 配置参数无效或缺失\n\n");
            msg.push_str("💡 建议:\n");
            msg.push_str("1. 检查设置 → AI 配置\n");
            msg.push_str("2. 确认所有必填字段已填写\n");
            msg.push_str("3. 验证 API Key 和模型名称正确\n");
        }
        AiError::Api { code, message } => {
            let desc = match *code {
                400 => "请求体格式错误",
                401 => "API Key 无效或已过期",
                402 => "账户余额不足",
                403 => "API Key 权限不足",
                404 => "请求的资源不存在",
                422 => "参数错误",
                429 => "请求速率超限",
                500 => "API 服务器内部故障",
                503 => "API 服务器负载过高",
                _ => "API 请求失败",
            };
            
            msg.push_str(&format!("[API 错误 {}] {}\n\n", code, desc));
            msg.push_str("🔍 错误详情:\n");
            msg.push_str(&format!("• HTTP 状态码: {}\n", code));
            msg.push_str(&format!("• 错误信息: {}\n\n", sanitize_error(message)));
            
            msg.push_str("📊 诊断分析:\n");
            match *code {
                400 => {
                    msg.push_str("• 请求参数格式不正确\n");
                    msg.push_str("• 可能原因: 模型名称错误、参数类型不匹配\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 检查模型名称是否正确\n");
                    msg.push_str("2. 验证 temperature/max_tokens 等参数范围\n");
                    msg.push_str("3. 查看 API 文档确认参数格式\n");
                }
                401 => {
                    msg.push_str("• API Key 验证失败\n");
                    msg.push_str("• 可能原因: Key 无效、过期、被撤销\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 重新生成 API Key\n");
                    msg.push_str("2. 检查 Key 是否完整复制（无多余空格）\n");
                    msg.push_str("3. 确认 Key 未过期\n");
                }
                402 => {
                    msg.push_str("• 账户余额不足\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 到 API 提供商官网充值\n");
                    msg.push_str("2. 检查账户余额\n");
                    msg.push_str("3. 考虑使用其他模型或提供商\n");
                }
                403 => {
                    msg.push_str("• API Key 权限不足\n");
                    msg.push_str("• 可能原因: Key 无访问该模型的权限\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 检查 API Key 权限设置\n");
                    msg.push_str("2. 确认账户已开通该模型访问权限\n");
                    msg.push_str("3. 联系 API 提供商升级权限\n");
                }
                404 => {
                    msg.push_str("• 请求的资源不存在\n");
                    msg.push_str("• 可能原因: 模型名称错误、API 端点错误\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 检查模型名称拼写\n");
                    msg.push_str("2. 验证 API 基础 URL 配置\n");
                    msg.push_str("3. 查看 API 文档确认端点地址\n");
                }
                422 => {
                    msg.push_str("• 请求参数验证失败\n");
                    msg.push_str("• 可能原因: 参数值超出允许范围\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 检查 temperature 范围（通常 0.0-2.0）\n");
                    msg.push_str("2. 检查 max_tokens 是否合理\n");
                    msg.push_str("3. 查看 API 文档确认参数约束\n");
                }
                429 => {
                    msg.push_str("• 请求速率超过限制（TPM/RPM）\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 稍后重试（通常几秒到几分钟后恢复）\n");
                    msg.push_str("2. 减少请求频率\n");
                    msg.push_str("3. 考虑升级 API 套餐\n");
                }
                500 | 503 => {
                    msg.push_str("• API 服务器内部错误或负载过高\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 稍后重试（通常是临时问题）\n");
                    msg.push_str("2. 检查 API 服务状态页面\n");
                    msg.push_str("3. 如持续失败，联系 API 提供商\n");
                }
                _ => {
                    msg.push_str("• API 请求失败\n\n");
                    msg.push_str("💡 建议:\n");
                    msg.push_str("1. 检查请求参数\n");
                    msg.push_str("2. 查看 API 文档\n");
                    msg.push_str("3. 联系 API 提供商支持\n");
                }
            }
            
            // 添加重试提示
            if e.is_retryable() {
                msg.push_str("\n♻️ 此错误为暂时性错误，系统稍后可能自动重试\n");
            } else if e.is_permanent() {
                msg.push_str("\n⚠️ 此错误为永久性错误，请检查配置后重试\n");
            }
        }
    }
    
    msg
}

/// AI 助手消息
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
    /// "深度思考"内容（DeepSeek reasoner 的 reasoning_content）；None 表示无思考。
    /// 与 content 分离存储，UI 上作为独立的"思考过程"分类展示。
    pub reasoning: Option<String>,
    /// 思考块是否折叠（默认折叠：思考内容到达即收起，仅显示标题；用户可点击标题展开）
    #[serde(default)]
    pub reasoning_collapsed: bool,
    /// 思考耗时（毫秒）；None 表示无思考或仍在思考中（旧持久化数据也为 None）
    #[serde(default)]
    pub reasoning_ms: Option<u64>,
    /// 思考开始时刻（Unix 毫秒），仅运行期计时用，不持久化
    #[serde(skip)]
    pub reasoning_started_ms: Option<u64>,
    /// 询问卡片（AETHER_ASK）回答：按消息内 ASK 块出现顺序对齐，None = 未回答
    #[serde(default)]
    pub ask_answers: Vec<Option<String>>,
    /// 该消息附带过的图片文件名（仅展示用；图片数据本身不持久化、
    /// 也不随历史重发）。空 = 无图片。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_names: Vec<String>,
}

impl AiMessage {
    pub fn new(role: AiRole, content: String) -> Self {
        Self {
            role,
            content,
            reasoning: None,
            reasoning_collapsed: true,
            reasoning_ms: None,
            reasoning_started_ms: None,
            ask_answers: Vec::new(),
            image_names: Vec::new(),
        }
    }

    /// 思考计时启动：首次收到 reasoning 增量时调用（重复调用无副作用）
    pub fn start_reasoning_timer(&mut self) {
        if self.reasoning_started_ms.is_none() && self.reasoning_ms.is_none() {
            self.reasoning_started_ms = Some(now_millis());
        }
    }

    /// 思考计时结束：回答开始 / 流结束 / 出错时调用，结算耗时（未启动则无操作）
    pub fn stop_reasoning_timer(&mut self) {
        if let Some(start) = self.reasoning_started_ms.take() {
            self.reasoning_ms = Some(now_millis().saturating_sub(start));
        }
    }
}

/// 思考耗时的展示文本（"17s" / "1m05s"），毫秒四舍五入到秒
pub fn format_reasoning_duration(ms: u64) -> String {
    let secs = (ms + 500) / 1000;
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    User,
    Assistant,
    System,
    /// Agent 工具结果回喂：不作为用户消息显示，UI 渲染为简洁的工具卡片
    Tool,
    /// 待确认消息：AI 对用户请求存在歧义时主动发起反问
    PendingConfirmation,
}

/// 流式响应的共享状态
#[derive(Clone, Debug)]
pub struct AiStreamState {
    /// 已累积但尚未被 UI 取走的 token（最终回答）
    pub partial: String,
    /// 已累积但尚未被 UI 取走的"深度思考"内容（DeepSeek reasoning_content 等）
    pub reasoning: String,
    /// 流是否已结束
    pub done: bool,
    /// 流式过程中发生的错误
    pub error: Option<String>,
    /// 输出被截断的原因（如达到 max_tokens 限制），Some 表示被截断
    pub truncated: Option<String>,
    /// 流式请求开始时间（用于超时检测）
    pub start_time: Option<std::time::Instant>,
    /// 是否已收到首个响应（用于区分"无响应超时"和"生成中"）
    pub received_first_response: bool,
}

impl Default for AiStreamState {
    fn default() -> Self {
        Self {
            partial: String::new(),
            reasoning: String::new(),
            done: false,
            error: None,
            truncated: None,
            start_time: None,
            received_first_response: false,
        }
    }
}

/// 扩写动画阶段
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExpandAnimPhase {
    /// 无动画
    None,
    /// 原文渐隐中
    FadeOut,
    /// 流式写入新文本中（新文本渐显）
    Streaming,
}

/// 后台流式轮询的边沿结果
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrainEdge {
    /// 仍在生成中（或本无生成），无结束边沿
    Pending,
    /// 本帧正常完成：调用方应处理 Agent 动作（文件/命令）
    Completed,
    /// 本帧因错误中断：部分内容已落入消息，调用方可抢救已接收的文件块
    Interrupted,
    /// 输出因达到 max_tokens 被截断：用户可点击"继续生成"
    Truncated,
}

/// AI 助手欢迎语（新对话初始系统消息）
pub const AI_WELCOME: &str = "你好！我是 AI 助手，可以帮助你解释代码、重构、修复问题、生成测试等。你可以直接输入问题，或选中代码后使用快捷操作。";

/// 当前 Unix 秒级时间戳（对话创建/更新时间）
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 当前 Unix 毫秒级时间戳（思考耗时计时）
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 生成对话 ID（时间戳毫秒 + 计数，保证唯一）
pub fn gen_conversation_id() -> String {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("conv-{}-{}", ms, n)
}

/// 待发送的图片附件（AI 输入框中附加，随下一条用户消息以
/// OpenAI 兼容 `image_url` base64 data URL 块发送）。
///
/// 仅运行期驻留：发送后清空，不持久化（历史消息只保留 `AiMessage::image_names`
/// 文件名用于展示），避免 base64 数据撑爆会话存储。
#[derive(Clone, Debug)]
pub struct AiImageAttachment {
    /// 原始文件名（用于 chip 展示与历史消息标注）
    pub filename: String,
    /// MIME 类型（按文件魔数检测：image/jpeg | image/png | image/gif | image/webp）
    pub mime: String,
    /// 图片内容的 base64 编码
    pub data_b64: String,
}

/// 单个 AI 对话会话（多标签页 + 历史记录的基本单元）。
///
/// 活动会话的实时状态保存在 `AiPanel` 的扁平字段中（沿用旧逻辑，避免大面积改动）；
/// 非活动会话以本结构存放于 `AiPanel::conversations`，可在后台并发流式生成。
#[derive(Clone, Debug)]
pub struct AiConversation {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<AiMessage>,
    pub input: String,
    pub caret_pos: usize,
    pub composition: Option<String>,
    pub is_generating: bool,
    pub scroll_y: f32,
    pub content_height: f32,
    pub stick_to_bottom: bool,
    pub mode: AiMode,
    pub attachments: Vec<AiContextAttachment>,
    /// 待发送图片附件（发送后即清空；会话切换时随槽位保存/恢复）
    pub pending_images: Vec<AiImageAttachment>,
    pub stream_state: Arc<Mutex<AiStreamState>>,
    pub should_stop: Arc<AtomicBool>,
    /// 本轮注入过的 playbook 条目 ID（用于反馈归因）
    pub used_bullet_ids: Vec<String>,
    /// 标签休眠：messages 已卸载（完整内容在 AetherDB 温数据层），
    /// 轻量现场（草稿/滚动/模式等）仍驻留内存；激活时同步水合
    pub hibernated: bool,
    /// 休眠两阶段握手：发起归档时记录当时 updated_at，
    /// 归档成功回执且 updated_at 未变才真正卸载，防快照后新增消息丢失
    pub hibernate_pending_at: Option<u64>,
    /// 休眠前的消息数（供关闭时写历史元数据，免读库）
    pub hibernated_msg_count: usize,
}

impl AiConversation {
    pub fn new(id: String, title: String) -> Self {
        let now = now_secs();
        Self {
            id,
            title,
            created_at: now,
            updated_at: now,
            messages: vec![AiMessage::new(AiRole::System, AI_WELCOME.to_string())],
            input: String::new(),
            caret_pos: 0,
            composition: None,
            is_generating: false,
            scroll_y: 0.0,
            content_height: 0.0,
            stick_to_bottom: true,
            mode: AiMode::Agent,
            attachments: Vec::new(),
            pending_images: Vec::new(),
            stream_state: Arc::new(Mutex::new(AiStreamState::default())),
            should_stop: Arc::new(AtomicBool::new(false)),
            used_bullet_ids: Vec::new(),
            hibernated: false,
            hibernate_pending_at: None,
            hibernated_msg_count: 0,
        }
    }

    /// 是否具备归档价值（含用户消息的真实对话，与归档/反思的判定一致）
    pub fn is_archivable(&self) -> bool {
        self.messages.len() > 1 && self.messages.iter().any(|m| m.role == AiRole::User)
    }

    /// 卸载消息体进入休眠态（调用方负责确认已安全落库）
    fn enter_hibernation(&mut self) {
        self.hibernated_msg_count = self.messages.len();
        self.messages = Vec::new();
        self.hibernated = true;
        self.hibernate_pending_at = None;
    }

    /// 用温数据层读回的消息体退出休眠态
    fn wake_with_messages(&mut self, messages: Vec<AiMessage>) {
        self.messages = messages;
        self.hibernated = false;
        self.hibernate_pending_at = None;
        self.hibernated_msg_count = 0;
    }

    fn add_assistant_message(&mut self, content: String) {
        self.messages
            .push(AiMessage::new(AiRole::Assistant, content));
        self.stick_to_bottom = true;
        self.updated_at = now_secs();
    }

    /// 最后一条助手消息文本
    pub fn last_assistant_text(&self) -> Option<String> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::Assistant)
            .map(|m| m.content.clone())
    }

    /// 首条用户消息（用于自动生成标题）
    pub fn first_user_text(&self) -> Option<String> {
        self.messages
            .iter()
            .find(|m| m.role == AiRole::User)
            .map(|m| m.content.clone())
    }

    /// 后台（非活动）会话的流式轮询：把新 token 追加到消息，返回结束边沿。
    /// 与 `AiPanel::check_background_result` 逻辑一致，但作用于本会话，支持并发。
    pub fn drain_background(&mut self) -> DrainEdge {
        if !self.is_generating {
            return DrainEdge::Pending;
        }
        let delta = if let Ok(mut s) = self.stream_state.lock() {
            let partial = std::mem::take(&mut s.partial);
            let reasoning = std::mem::take(&mut s.reasoning);
            let done = s.done;
            let error = s.error.take();
            let truncated = s.truncated.take();
            if done {
                s.done = false;
            }
            Some((partial, reasoning, done, error, truncated))
        } else {
            None
        };
        let mut edge = DrainEdge::Pending;
        if let Some((partial, reasoning, done, error, truncated)) = delta {
            // 深度思考通常先于回答到达：确保有一条助手消息承载 reasoning
            if !reasoning.is_empty() {
                if !matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
                    self.messages
                        .push(AiMessage::new(AiRole::Assistant, String::new()));
                }
                if let Some(last) = self.messages.last_mut() {
                    last.start_reasoning_timer();
                    last.reasoning
                        .get_or_insert_with(String::new)
                        .push_str(&reasoning);
                }
                // 不强制吸附底部：尊重用户手动滚动选择
                self.updated_at = now_secs();
            }
            if !partial.is_empty() {
                // 不强制吸附底部：尊重用户手动滚动选择
                if !matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
                    self.messages
                        .push(AiMessage::new(AiRole::Assistant, String::new()));
                }
                if let Some(last) = self.messages.last_mut() {
                    // 回答开始到达 = 思考阶段结束，结算思考耗时
                    last.stop_reasoning_timer();
                    last.content.push_str(&partial);
                }
                self.updated_at = now_secs();
            }
            if let Some(err) = error {
                // 出错中断：结算已进行的思考耗时，避免显示永远的"思考中"
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                }
                self.add_assistant_message(err);
                self.is_generating = false;
                // 中断边沿：已接收的部分内容（含完整文件块）仍值得抢救
                return DrainEdge::Interrupted;
            }
            if done {
                self.is_generating = false;
                // 生成完成：恢复思考块折叠（默认即折叠，用户流式中手动展开时在此收起）
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                    // 修复：当 content 为空但 reasoning 非空时（DeepSeek 思考模式简单问答场景），
                    // 将 reasoning 内容作为正常回答显示，而非放在"深度思考"区域
                    if last.role == AiRole::Assistant
                        && last.content.trim().is_empty()
                        && last
                            .reasoning
                            .as_ref()
                            .is_some_and(|r| !r.trim().is_empty())
                    {
                        last.content = last.reasoning.take().unwrap_or_default();
                        last.reasoning = None;
                        last.reasoning_collapsed = false;
                        last.reasoning_ms = None;
                        last.reasoning_started_ms = None;
                    } else if last.role == AiRole::Assistant && last.reasoning.is_some() {
                        last.reasoning_collapsed = true;
                    }
                }
                if truncated.is_some() {
                    // 被截断：提示用户，后台会话无"继续"按钮，只显示消息
                    self.add_assistant_message(format!(
                        "[警告] 输出已被截断（原因：{}）。发送\"继续\"以继续生成。",
                        truncated.unwrap_or_else(|| "max_tokens".to_string())
                    ));
                    edge = DrainEdge::Truncated;
                } else {
                    edge = DrainEdge::Completed;
                }
            }
        }
        edge
    }
}

/// 历史记录轻量元数据（懒加载：列表只用元数据，点击时才读完整会话）
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
    pub message_count: usize,
    pub preview: String,
    /// 会话模式（"Ask" / "Agent"；旧数据可能为空串）
    #[serde(default)]
    pub mode: String,
}

/// 历史记录时间筛选
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HistoryTimeFilter {
    /// 全部
    All,
    /// 最近 24 小时
    Today,
    /// 最近 7 天
    Week,
    /// 最近 30 天
    Month,
}

impl HistoryTimeFilter {
    pub const ALL: [HistoryTimeFilter; 4] = [
        HistoryTimeFilter::All,
        HistoryTimeFilter::Today,
        HistoryTimeFilter::Week,
        HistoryTimeFilter::Month,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HistoryTimeFilter::All => "全部",
            HistoryTimeFilter::Today => "今天",
            HistoryTimeFilter::Week => "本周",
            HistoryTimeFilter::Month => "本月",
        }
    }

    /// 时间下限（Unix 秒）；None 表示不限
    pub fn cutoff(self, now: u64) -> Option<u64> {
        match self {
            HistoryTimeFilter::All => None,
            HistoryTimeFilter::Today => Some(now.saturating_sub(24 * 3600)),
            HistoryTimeFilter::Week => Some(now.saturating_sub(7 * 24 * 3600)),
            HistoryTimeFilter::Month => Some(now.saturating_sub(30 * 24 * 3600)),
        }
    }
}

/// 历史列表可选的类型筛选项（None = 全部）
pub const HISTORY_TYPE_FILTERS: [Option<&str>; 3] = [None, Some("Ask"), Some("Agent")];

/// 历史列表每页条数
pub const HISTORY_PAGE_SIZE: usize = 6;

/// 相对时间显示（历史列表用）
pub fn relative_time(updated_at: u64, now: u64) -> String {
    let d = now.saturating_sub(updated_at);
    if d < 60 {
        "刚刚".to_string()
    } else if d < 3600 {
        format!("{} 分钟前", d / 60)
    } else if d < 86400 {
        format!("{} 小时前", d / 3600)
    } else if d < 7 * 86400 {
        format!("{} 天前", d / 86400)
    } else if d < 30 * 86400 {
        format!("{} 周前", d / (7 * 86400))
    } else {
        format!("{} 个月前", d / (30 * 86400))
    }
}

/// 多任务编排流水线状态（CoT 任务分解：规划器 → 逐任务独立 AI 调用）。
///
/// 由规划器产出 tasks 后创建；每个 FILE 任务发起一次聚焦 worker 调用生成，
/// RUN 任务直接执行。cursor 指向当前任务，created_files 供后续 worker 参考已建文件。
#[derive(Clone, Debug)]
pub struct AgentPipeline {
    pub goal: String,
    pub tasks: Vec<crate::ai_agent::PlannedTask>,
    pub cursor: usize,
    pub created_files: Vec<String>,
    /// 未成功写入的文件任务（worker 输出未闭合/应用失败），收尾时如实汇报
    pub failed_files: Vec<String>,
    /// 当前任务的续写次数（防止无限续写循环）
    pub resume_count: usize,
}

/// Tab 键无障碍导航的焦点动作（文件卡展开 / 文件链接打开）
#[derive(Clone, Debug, PartialEq)]
pub enum TabFocusAction {
    /// 切换文件卡片展开/折叠 (msg_idx, block_seq)
    ToggleCard(usize, usize),
    /// 打开文件链接（相对路径）
    OpenFile(String),
}

/// AI 助手面板状态
#[derive(Debug)]
pub struct AiPanel {
    /// 是否可见
    pub visible: bool,
    /// 聊天历史
    pub messages: Vec<AiMessage>,
    /// 当前输入
    pub input: String,
    /// 是否正在生成回复
    pub is_generating: bool,
    /// 滚动偏移
    pub scroll_y: f32,
    /// Apply 按钮悬停状态
    pub hover_apply_button: bool,
    /// AI-H01: 后台线程流式状态，UI 渲染时轮询此字段
    pub stream_state: Arc<Mutex<AiStreamState>>,
    /// C-10: 输入框是否聚焦。仅当聚焦时才拦截键盘输入，避免面板可见即劫持编辑器
    pub input_focused: bool,
    /// 当前 AI 模式（Ask / Agent）
    pub mode: AiMode,
    /// 底部工具栏"当前模型"下拉是否展开（在对话框内切换当前使用的模型）
    pub model_menu_open: bool,
    /// 已附加的上下文项
    pub attachments: Vec<AiContextAttachment>,
    /// 待发送的图片附件（活动会话；多模态模型下经输入框图片按钮附加）
    pub pending_images: Vec<AiImageAttachment>,
    /// 图片 chip 命中区域 (index, x, y, w, h)，渲染每帧更新（面板相对坐标）；点击移除该图片
    pub image_chip_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 悬停的图片 chip 下标
    pub hover_image_chip: Option<usize>,
    /// 输入框工具栏"附加图片"按钮命中区 (x, y, w, h)（面板相对坐标）
    pub image_button_region: Option<(f32, f32, f32, f32)>,
    /// 模式切换按钮命中区域 (mode, x, y, w, h)
    pub mode_button_regions: Vec<(AiMode, f32, f32, f32, f32)>,
    /// 附件 chip 命中区域 (index, x, y, w, h)
    pub attachment_chip_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 悬停的附件 chip 索引
    pub hover_attachment: Option<usize>,
    /// 上一帧渲染的消息内容总高度（用于滚动条与自动滚底）
    pub content_height: f32,
    /// 代码块保存按钮区域 (msg_index, seg_index, x, y, w, h, suggested_filename)
    pub code_save_regions: Vec<(usize, usize, f32, f32, f32, f32, String)>,
    /// 是否吸附底部：新消息/流式到达时自动滚动到底部
    pub stick_to_bottom: bool,
    /// 输入框光标位置（字符索引，0 = 开头）
    pub caret_pos: usize,
    /// 输入框光标可见状态（闪烁，由 CARET_TIMER 切换）
    pub caret_visible: bool,
    /// IME 合成串（中文输入法预编辑文本），渲染时显示在 input 之后
    pub composition: Option<String>,
    /// 输入框计算后的高度（根据内容动态调整）
    pub input_computed_height: f32,
    /// 停止生成标志：后台流式线程在下一次循环检查时退出
    pub should_stop: Arc<AtomicBool>,
    /// 全部对话会话（多标签页）。活动会话的实时数据在上面的扁平字段中；
    /// conversations[active] 作为槽位，其 id/title/时间戳为权威值，切换时回写消息等数据。
    pub conversations: Vec<AiConversation>,
    /// 当前活动会话下标
    pub active: usize,
    /// 对话标签命中区 (conv_index, x, y, w, h)
    pub tab_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 标签关闭按钮命中区 (conv_index, x, y, w, h)
    pub tab_close_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// "新建对话"按钮命中区
    pub new_tab_region: Option<(f32, f32, f32, f32)>,
    /// "历史记录"按钮命中区
    pub history_button_region: Option<(f32, f32, f32, f32)>,
    /// 悬停的标签下标
    pub hover_tab: Option<usize>,
    /// 是否展开历史记录列表
    pub history_open: bool,
    /// 历史下拉面板展开动画进度（0.0 = 收起，1.0 = 完全展开）
    pub history_anim: f32,
    /// 历史下拉面板内部滚动偏移（内容超高时滚动）
    pub history_scroll: f32,
    /// 历史下拉面板最大滚动量（渲染时计算，供滚轮钳制）
    pub history_max_scroll: f32,
    /// 历史记录条目命中区 (history_index, x, y, w, h)
    pub history_item_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 历史索引（懒加载：仅元数据，点击时才读取完整会话）
    pub history: Vec<ConversationMeta>,
    /// 思考块折叠切换命中区 (msg_index, x, y, w, h)（作用于活动会话 messages 索引）
    pub reasoning_toggle_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 热数据持久化存储（三阶段架构：热/温）
    pub hot_data_store: Option<crate::ai_hot_data::HotDataStore>,
    /// 温数据持久化存储（MemoryStore：AetherDB 纯 Rust 存储）
    pub warm_data_store: Option<crate::ai_warm_data::WarmDataStore>,
    /// 历史列表：仅显示当前工作区的会话
    pub history_workspace_only: bool,
    /// Playbook 管理面板是否展开
    pub playbook_open: bool,
    /// Playbook 面板条目缓存（展开时从 AetherDB 加载）
    pub playbook_items: Vec<crate::memory_store::PlaybookBullet>,
    /// Playbook 标题栏按钮命中区 (x, y, w, h)
    pub playbook_button_region: Option<(f32, f32, f32, f32)>,
    /// Playbook 条目删除按钮命中区 (item_index, x, y, w, h)
    pub playbook_delete_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 历史列表当前页码（0 起）
    pub history_page: usize,
    /// 历史时间筛选
    pub history_time_filter: HistoryTimeFilter,
    /// 历史类型筛选（None=全部，否则匹配 mode 字符串，如 "Ask"/"Agent"）
    pub history_type_filter: Option<String>,
    /// 历史详情视图：当前查看的会话 id（Some 时历史面板显示详情而非列表）
    pub history_detail_id: Option<String>,
    /// 历史详情缓存的完整会话（懒加载）
    pub history_detail_conv: Option<AiConversation>,
    /// 历史条目删除按钮命中区 (history_index, x, y, w, h)
    pub history_delete_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 历史分页「上一页」命中区
    pub history_page_prev_region: Option<(f32, f32, f32, f32)>,
    /// 历史分页「下一页」命中区
    pub history_page_next_region: Option<(f32, f32, f32, f32)>,
    /// 历史时间筛选按钮命中区 (HistoryTimeFilter::ALL 下标, x, y, w, h)
    pub history_time_filter_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 「清空全部」按钮命中区
    pub history_clear_all_region: Option<(f32, f32, f32, f32)>,
    // ===== 历史浮窗（可拖动独立窗口）=====
    /// 浮窗左上角位置（窗口客户区逻辑像素）；None = 未拖动过（渲染时默认居中）
    pub history_win_pos: Option<(f32, f32)>,
    /// 拖动中：鼠标相对浮窗左上角的偏移（按下标题栏时记录，抬起清空）
    pub history_win_drag: Option<(f32, f32)>,
    /// 浮窗尺寸（宽, 高），固定初始值，后续可扩展拖拽缩放
    pub history_win_size: (f32, f32),
    /// 浮窗整体命中区（渲染时注册，供点击外部关闭与滚轮路由）
    pub history_win_region: Option<(f32, f32, f32, f32)>,
    /// 浮窗标题栏命中区（拖动区）
    pub history_win_titlebar_region: Option<(f32, f32, f32, f32)>,
    /// 浮窗关闭按钮命中区
    pub history_win_close_region: Option<(f32, f32, f32, f32)>,
    /// 搜索框文本（按标题实时过滤）
    pub history_search: String,
    /// 搜索框是否聚焦（键盘路由）
    pub history_search_focused: bool,
    /// 搜索框光标位置（字节索引）
    pub history_search_caret: usize,
    /// 搜索框命中区
    pub history_search_region: Option<(f32, f32, f32, f32)>,
    /// 正在编辑标题的会话 id（Some 时该条目渲染为输入框）
    pub history_editing_id: Option<String>,
    /// 编辑中的标题文本缓冲
    pub history_editing_text: String,
    /// 编辑光标（字节索引）
    pub history_editing_caret: usize,
    /// 双击检测：上次点击的条目 id + 时间戳
    pub history_last_click: Option<(String, std::time::Instant)>,
    /// Agent 自动续跑轮次计数（用户手动发消息时重置；防止工具回环无限迭代）
    pub agent_iter_count: u32,
    /// 最后一次生成是否被截断（达到 max_tokens）
    pub last_truncated: bool,
    /// "继续生成"按钮命中区 (x, y, w, h)
    pub continue_button_region: Option<(f32, f32, f32, f32)>,
    /// "重试"按钮命中区 (x, y, w, h)
    pub retry_button_region: Option<(f32, f32, f32, f32)>,
    /// 当前多任务编排流水线（None 表示无）
    pub agent_pipeline: Option<AgentPipeline>,
    /// 已展开预览的文件卡片集合，key = (消息下标, 该消息内 File 卡序号)
    pub expanded_file_cards: std::collections::HashSet<(usize, usize)>,
    /// 文件卡片命中区域 (msg_idx, block_seq, x, y, w, h)，渲染每帧更新
    pub file_card_regions: Vec<(usize, usize, f32, f32, f32, f32)>,
    /// 修改前历史快照：(相对路径, 旧内容, 新内容)，供差异可视化；上限 32 条
    pub diff_snapshots: Vec<(String, String, String)>,
    /// 差异预览缓存：key = (消息下标, File 卡序号)，快照更新时清空
    pub diff_preview_cache: std::collections::HashMap<(usize, usize), Vec<crate::diff::DiffLine>>,
    /// 对话内可点击文件名命中区 (x, y, w, h, 路径)，渲染每帧更新
    pub file_link_regions: Vec<(f32, f32, f32, f32, String)>,
    /// Tab 键无障碍导航焦点区（文件卡/文件链接），渲染每帧重建
    pub tab_focus_regions: Vec<(f32, f32, f32, f32, TabFocusAction)>,
    /// 当前 Tab 焦点下标（None = 未聚焦）
    pub tab_focus_index: Option<usize>,
    /// "浏览并选择文件夹"按钮命中区 (x, y, w, h)
    pub browse_folder_region: Option<(f32, f32, f32, f32)>,
    /// 是否正在进行问题扩写（扩写结果写回输入框而非聊天区）
    pub is_expanding: bool,
    /// 扩写动画阶段
    pub expand_anim_phase: ExpandAnimPhase,
    /// 扩写动画进度（0.0 ~ 1.0），用于渐隐/渐显透明度计算
    pub expand_anim_progress: f32,
    /// 扩写前的原始文本（渐隐阶段显示用）
    pub expand_original_text: String,
    /// 扩写动画起始时间戳（毫秒），用于计算进度
    pub expand_anim_start_ms: u64,
    /// 智能体模式左侧边栏：新会话按钮命中区 (x, y, w, h)
    pub agent_new_chat_region: Option<(f32, f32, f32, f32)>,
    /// 智能体模式左侧边栏：对话标签页命中区 (conv_index, x, y, w, h)
    pub agent_tab_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 智能体模式左侧边栏：对话标签页关闭按钮命中区 (conv_index, x, y, w, h)
    pub agent_tab_close_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 智能体模式左侧边栏：文件树区域 y 偏移量（相对于侧边栏顶部）
    pub agent_file_tree_offset_y: f32,
    /// 在途请求是否启用深度思考（思考模式首包延迟显著更高，超时看门狗阈值相应放宽）
    pub in_flight_thinking: bool,
    /// 询问卡片选项命中区 (msg_idx, ask_seq, opt_idx, x, y, w, h)，渲染每帧更新
    pub ask_option_regions: Vec<(usize, usize, usize, f32, f32, f32, f32)>,
    /// 询问卡片“自定义回答”命中区 (msg_idx, ask_seq, x, y, w, h)，渲染每帧更新
    pub ask_custom_regions: Vec<(usize, usize, f32, f32, f32, f32)>,
    /// 当前等待自定义输入的询问卡片 (msg_idx, ask_seq)：用户点“自定义回答”后，
    /// 输入框下一条消息将作为该问题的自定义回答而非普通提问
    pub pending_ask_custom: Option<(usize, usize)>,
}

/// 在后台线程发起一次流式 AI 请求，把事件写入共享 stream_state。
/// `send_message_internal`（普通对话/规划器调度）与 `stream_focused`（逐任务 worker）共用。
fn spawn_ai_stream(
    settings: AiSettings,
    messages: Vec<ChatMessage>,
    stream_state: Arc<Mutex<AiStreamState>>,
    should_stop: Arc<AtomicBool>,
) {
    // 记录请求开始时间
    if let Ok(mut s) = stream_state.lock() {
        s.start_time = Some(std::time::Instant::now());
        s.received_first_response = false;
    }

    std::thread::spawn(move || {
        let client = AiClient::new(&settings);
        match client.chat_completion_stream(&messages) {
            Ok(rx) => {
                while let Ok(event) = rx.recv() {
                    if should_stop.load(Ordering::SeqCst) {
                        if let Ok(mut s) = stream_state.lock() {
                            s.done = true;
                        }
                        break;
                    }

                    // 标记已收到首个响应
                    if let Ok(mut s) = stream_state.lock() {
                        if !s.received_first_response {
                            s.received_first_response = true;
                        }
                    }

                    match event {
                        AiStreamEvent::Token(token) => {
                            if let Ok(mut s) = stream_state.lock() {
                                s.partial.push_str(&token);
                            }
                        }
                        AiStreamEvent::Reasoning(r) => {
                            if let Ok(mut s) = stream_state.lock() {
                                s.reasoning.push_str(&r);
                            }
                        }
                        AiStreamEvent::Done => {
                            if let Ok(mut s) = stream_state.lock() {
                                s.done = true;
                            }
                            break;
                        }
                        AiStreamEvent::Truncated(reason) => {
                            if let Ok(mut s) = stream_state.lock() {
                                s.truncated = Some(reason);
                            }
                            // 不 break：继续接收 Done（SSE 会先发 finish_reason 再发 [DONE]）
                        }
                        AiStreamEvent::Error(err) => {
                            if let Ok(mut s) = stream_state.lock() {
                                // 使用详细错误诊断
                                let error_msg = diagnose_stream_error(&err);
                                s.error = Some(error_msg);
                                s.done = true;
                            }
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                if let Ok(mut s) = stream_state.lock() {
                    // 使用详细错误诊断
                    let error_msg = diagnose_api_error(&e);
                    s.error = Some(error_msg);
                    s.done = true;
                }
            }
        }
    });
}

impl AiPanel {
    pub fn new() -> Self {
        let mut panel = Self {
            visible: false,
            messages: vec![AiMessage::new(AiRole::System, AI_WELCOME.to_string())],
            input: String::new(),
            is_generating: false,
            scroll_y: 0.0,
            hover_apply_button: false,
            stream_state: Arc::new(Mutex::new(AiStreamState::default())),
            input_focused: false,
            mode: AiMode::Agent,
            model_menu_open: false,
            attachments: Vec::new(),
            pending_images: Vec::new(),
            image_chip_regions: Vec::new(),
            hover_image_chip: None,
            image_button_region: None,
            mode_button_regions: Vec::new(),
            attachment_chip_regions: Vec::new(),
            hover_attachment: None,
            content_height: 0.0,
            code_save_regions: Vec::new(),
            stick_to_bottom: true,
            caret_pos: 0,
            caret_visible: false,
            composition: None,
            input_computed_height: 36.0,
            should_stop: Arc::new(AtomicBool::new(false)),
            conversations: vec![AiConversation::new(
                gen_conversation_id(),
                "新对话".to_string(),
            )],
            active: 0,
            tab_regions: Vec::new(),
            tab_close_regions: Vec::new(),
            new_tab_region: None,
            history_button_region: None,
            hover_tab: None,
            history_open: false,
            history_anim: 0.0,
            history_scroll: 0.0,
            history_max_scroll: 0.0,
            history_item_regions: Vec::new(),
            history: Vec::new(),
            reasoning_toggle_regions: Vec::new(),
            hot_data_store: Self::init_hot_data_store(),
            warm_data_store: Self::init_warm_data_store(),
            history_workspace_only: true,
            playbook_open: false,
            playbook_items: Vec::new(),
            playbook_button_region: None,
            playbook_delete_regions: Vec::new(),
            history_page: 0,
            history_time_filter: HistoryTimeFilter::All,
            history_type_filter: None,
            history_detail_id: None,
            history_detail_conv: None,
            history_delete_regions: Vec::new(),
            history_page_prev_region: None,
            history_page_next_region: None,
            history_time_filter_regions: Vec::new(),
            history_clear_all_region: None,
            history_win_pos: None,
            history_win_drag: None,
            history_win_size: (480.0, 420.0),
            history_win_region: None,
            history_win_titlebar_region: None,
            history_win_close_region: None,
            history_search: String::new(),
            history_search_focused: false,
            history_search_caret: 0,
            history_search_region: None,
            history_editing_id: None,
            history_editing_text: String::new(),
            history_editing_caret: 0,
            history_last_click: None,
            agent_iter_count: 0,
            last_truncated: false,
            continue_button_region: None,
            retry_button_region: None,
            agent_pipeline: None,
            expanded_file_cards: std::collections::HashSet::new(),
            file_card_regions: Vec::new(),
            diff_snapshots: Vec::new(),
            diff_preview_cache: std::collections::HashMap::new(),
            file_link_regions: Vec::new(),
            tab_focus_regions: Vec::new(),
            tab_focus_index: None,
            browse_folder_region: None,
            is_expanding: false,
            expand_anim_phase: ExpandAnimPhase::None,
            expand_anim_progress: 0.0,
            expand_original_text: String::new(),
            expand_anim_start_ms: 0,
            agent_new_chat_region: None,
            agent_tab_regions: Vec::new(),
            agent_tab_close_regions: Vec::new(),
            agent_file_tree_offset_y: 0.0,
            in_flight_thinking: false,
            ask_option_regions: Vec::new(),
            ask_custom_regions: Vec::new(),
            pending_ask_custom: None,
        };
        panel.restore_latest_conversation();
        panel
    }

    /// 初始化热数据存储
    fn init_hot_data_store() -> Option<crate::ai_hot_data::HotDataStore> {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Aether")
            .join("conversations");
        match crate::ai_hot_data::HotDataStore::new(base_dir) {
            Ok(store) => Some(store),
            Err(e) => {
                eprintln!("[AiPanel] 热数据存储初始化失败: {}", e);
                None
            }
        }
    }

    /// 初始化温数据存储
    fn init_warm_data_store() -> Option<crate::ai_warm_data::WarmDataStore> {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Aether")
            .join("conversations");
        match crate::ai_warm_data::WarmDataStore::new(base_dir) {
            Ok(store) => Some(store),
            Err(e) => {
                eprintln!("[AiPanel] 温数据存储初始化失败: {}", e);
                None
            }
        }
    }

    /// 同步当前状态到热数据存储
    /// P1-B: 只借用会话列表传入，不再克隆整个 AiPanel（原 clone_for_sync
    /// 每次同步深拷贝全部会话消息体，长对话下单次即 MB 级分配）
    fn sync_hot_data(&mut self) {
        // 先 snapshot 到槽位，确保热数据看到的是完整状态
        self.snapshot_active_into_slot();
        if let Some(mut store) = self.hot_data_store.take() {
            store.sync_from_panel(&self.conversations);
            self.hot_data_store = Some(store);
        }
    }

    /// 触发温数据归档（空闲时调用）
    pub fn trigger_warm_archive(&mut self) {
        // 1. 先收割归档结果：后台线程异步完成，结果在后续调用中才就绪
        let results = self
            .warm_data_store
            .as_ref()
            .map(|s| s.poll_results())
            .unwrap_or_default();
        for result in results {
            match result {
                crate::ai_warm_data::ArchiveResult::Success { conv_id } => {
                    if let Some(hot_store) = self.hot_data_store.as_mut() {
                        hot_store.clear_dirty(&conv_id);
                    }
                    // 休眠两阶段握手：落库确认后才卸载对应标签的消息体
                    self.finalize_hibernation(&conv_id);
                    if let Some(warm_store) = self.warm_data_store.as_ref() {
                        warm_store.request_remove_hot_log(conv_id);
                    }
                }
                crate::ai_warm_data::ArchiveResult::Failed { conv_id, error } => {
                    eprintln!("[AiPanel] 归档失败 {}: {}", conv_id, error);
                }
            }
        }

        // 2. 空闲且有脏会话时发起新一轮归档
        if let Some(hot_store) = self.hot_data_store.as_mut() {
            if hot_store.should_warm_archive() {
                let dirty_sessions: Vec<crate::ai_panel::AiConversation> = hot_store
                    .dirty_sessions()
                    .iter()
                    .map(|c| (*c).clone())
                    .collect();
                if let Some(warm_store) = self.warm_data_store.as_ref() {
                    warm_store.request_archive_all(dirty_sessions, true);
                }
            }
        }
    }

    /// 应用退出前调用：同步归档所有有效会话并关闭归档线程。
    /// 与空闲归档的区别：不限于脏会话（覆盖聊完不足 30 秒就退出的场景），
    /// 且 shutdown() 会等待后台线程把队列写完，保证数据真正落盘。
    /// 跳过 LLM 反思，避免退出被网络请求阻塞。
    pub fn archive_all_on_exit(&mut self) {
        self.snapshot_active_into_slot();
        let sessions: Vec<AiConversation> = self
            .conversations
            .iter()
            .filter(|c| c.messages.len() > 1 && c.messages.iter().any(|m| m.role == AiRole::User))
            .cloned()
            .collect();
        if let Some(warm_store) = self.warm_data_store.as_mut() {
            if !sessions.is_empty() {
                warm_store.request_archive_all(sessions, false);
            }
            warm_store.shutdown();
        }
    }

    /// 启动时恢复最近一次的会话（若数据库中有归档），否则保持新建的"新对话"
    /// 仅恢复当前工作区绑定的会话；无工作区（启动未打开文件夹）时不恢复，
    /// 避免全局最新会话串到无工作区的空白页。
    fn restore_latest_conversation(&mut self) {
        let Some(store) = self.warm_data_store.as_ref() else {
            return;
        };
        // 无工作区上下文时不恢复任何会话，防止串工作区
        let ws_hash = store.current_workspace_hash();
        if ws_hash.is_empty() {
            return;
        }
        // 按当前工作区过滤：仅恢复属于本工作区的会话
        let Ok(convs) = store.search_conversations("", true, 1) else {
            return;
        };
        let Some(latest) = convs.first() else {
            return;
        };
        if let Ok(conv) = store.load_conversation(&latest.id) {
            self.conversations[0] = conv;
            self.load_slot_into_active(0);
        }
    }

    // ===== 多会话（标签页 / 并发 / 历史）=====

    /// 标签标题（活动会话取槽位标题，槽位标题在 sync_active_title 中维护）
    pub fn conv_title(&self, i: usize) -> &str {
        self.conversations
            .get(i)
            .map(|c| c.title.as_str())
            .unwrap_or("")
    }

    /// 某会话是否正在生成（活动会话读扁平字段，其余读槽位）
    pub fn conv_is_generating(&self, i: usize) -> bool {
        if i == self.active {
            self.is_generating
        } else {
            self.conversations
                .get(i)
                .map(|c| c.is_generating)
                .unwrap_or(false)
        }
    }

    /// 将活动会话的实时（扁平）状态回写到 conversations[active] 槽位。
    /// 切换/关闭/保存前调用，保证槽位数据最新。
    pub fn snapshot_active_into_slot(&mut self) {
        if self.active >= self.conversations.len() {
            return;
        }
        let slot = &mut self.conversations[self.active];
        slot.messages = self.messages.clone();
        slot.input = self.input.clone();
        slot.caret_pos = self.caret_pos;
        slot.composition = self.composition.clone();
        slot.is_generating = self.is_generating;
        slot.scroll_y = self.scroll_y;
        slot.content_height = self.content_height;
        slot.stick_to_bottom = self.stick_to_bottom;
        slot.mode = self.mode;
        slot.attachments = self.attachments.clone();
        slot.pending_images = self.pending_images.clone();
        slot.stream_state = Arc::clone(&self.stream_state);
        slot.should_stop = Arc::clone(&self.should_stop);
        slot.updated_at = now_secs();
    }

    /// 把某槽位会话加载为活动会话的实时（扁平）状态。
    /// 休眠槽位先从温数据层水合消息体，覆盖所有激活路径（切换/关闭接管/历史恢复）。
    pub fn load_slot_into_active(&mut self, idx: usize) {
        if idx >= self.conversations.len() {
            return;
        }
        self.wake_conversation(idx);
        // 挪移大字段而非克隆：活动会话现场以扁平字段为唯一权威副本，
        // 槽位在下一次 snapshot_active_into_slot 时回填，消除双份驻留。
        // 所有读活动槽位重字段的路径（热同步/退出归档/关闭）均先 snapshot。
        let slot = &mut self.conversations[idx];
        self.messages = std::mem::take(&mut slot.messages);
        self.input = std::mem::take(&mut slot.input);
        self.caret_pos = slot.caret_pos;
        self.composition = slot.composition.take();
        self.is_generating = slot.is_generating;
        self.scroll_y = slot.scroll_y;
        self.content_height = slot.content_height;
        self.stick_to_bottom = slot.stick_to_bottom;
        self.mode = slot.mode;
        self.attachments = std::mem::take(&mut slot.attachments);
        self.pending_images = std::mem::take(&mut slot.pending_images);
        self.image_chip_regions.clear();
        self.hover_image_chip = None;
        self.stream_state = Arc::clone(&slot.stream_state);
        self.should_stop = Arc::clone(&slot.should_stop);
        self.active = idx;
    }

    /// 切换到指定会话标签
    pub fn switch_to(&mut self, idx: usize) {
        if idx == self.active || idx >= self.conversations.len() {
            return;
        }
        self.snapshot_active_into_slot();
        let prev = self.active;
        self.load_slot_into_active(idx);
        // 刚切走的旧标签：空闲且可归档时发起休眠（两阶段，落库成功才真正卸载）
        self.request_hibernate(prev);
        self.model_menu_open = false;
        self.dismiss_history_dropdown();
    }

    /// 对指定空闲标签发起休眠请求：异步归档进 AetherDB，落库成功回执后才卸载消息体。
    /// 生成中、已休眠、活动标签、无归档价值的会话跳过。
    fn request_hibernate(&mut self, idx: usize) {
        if idx == self.active || idx >= self.conversations.len() {
            return;
        }
        let conv = &self.conversations[idx];
        if conv.hibernated || conv.is_generating || conv.hibernate_pending_at.is_some() {
            return;
        }
        if !conv.is_archivable() {
            return; // 空对话无需落库，直接常驻内存（成本极低）
        }
        let Some(store) = self.warm_data_store.as_ref() else {
            return; // 无温数据层则不休眠，避免消息无处可存
        };
        let snapshot = conv.updated_at;
        store.request_archive(conv.id.clone(), conv.clone());
        self.conversations[idx].hibernate_pending_at = Some(snapshot);
    }

    /// 唤醒指定休眠标签：从温数据层读回消息体。读失败则退化为保留存根（避免 panic）。
    fn wake_conversation(&mut self, idx: usize) {
        if idx >= self.conversations.len() || !self.conversations[idx].hibernated {
            return;
        }
        let id = self.conversations[idx].id.clone();
        let loaded = self
            .warm_data_store
            .as_ref()
            .and_then(|s| s.load_conversation(&id).ok())
            .map(|c| c.messages);
        match loaded {
            Some(messages) => self.conversations[idx].wake_with_messages(messages),
            None => {
                // 温数据缺失（极端情况）：以欢迎语兜底，保证 UI 可用
                self.conversations[idx].wake_with_messages(vec![AiMessage::new(
                    AiRole::System,
                    AI_WELCOME.to_string(),
                )]);
            }
        }
    }

    /// 收割休眠归档回执：对已落库且 updated_at 未变的 pending 标签真正卸载消息体。
    /// 在归档结果轮询（trigger_warm_archive）中调用，保证「落盘确认 → 卸载」的原子顺序。
    fn finalize_hibernation(&mut self, archived_id: &str) {
        let active = self.active;
        for (i, conv) in self.conversations.iter_mut().enumerate() {
            if conv.id != archived_id {
                continue;
            }
            // 回执到达前用户已切回该标签：放弃本轮并清 pending，
            // 否则残留的 pending 会永久屏蔽该标签后续的休眠发起
            if i == active {
                conv.hibernate_pending_at = None;
                continue;
            }
            // 两阶段握手：仅当 pending 且期间无新消息（updated_at 未变）才卸载
            if let Some(pending_at) = conv.hibernate_pending_at {
                if pending_at == conv.updated_at && !conv.is_generating {
                    conv.enter_hibernation();
                } else {
                    // 快照后又有新活动：放弃本轮休眠，等下次切走重新发起
                    conv.hibernate_pending_at = None;
                }
            }
        }
    }

    /// 新建一个空对话并激活
    pub fn new_conversation(&mut self) {
        self.snapshot_active_into_slot();
        let prev = self.active;
        let conv = AiConversation::new(gen_conversation_id(), "新对话".to_string());
        self.conversations.push(conv);
        let idx = self.conversations.len() - 1;
        self.load_slot_into_active(idx);
        self.request_hibernate(prev);
        self.input_focused = true;
        self.model_menu_open = false;
        self.dismiss_history_dropdown();
    }

    /// 关闭指定会话标签（正在生成的后台线程会被请求停止）
    /// 关闭前将会话归档到历史记录（内存中，Phase 2 再持久化到磁盘）。
    pub fn close_conversation(&mut self, idx: usize) {
        if idx >= self.conversations.len() {
            return;
        }
        // 关闭活动标签：先把扁平现场回填槽位，保证下面读到的是最新消息
        if idx == self.active {
            self.snapshot_active_into_slot();
        }
        self.conversations[idx]
            .should_stop
            .store(true, Ordering::SeqCst);
        // 归档到历史（仅非空对话）；休眠标签已落库，只补内存历史元数据不重复归档
        let conv = &self.conversations[idx];
        let msg_count = conv.messages.len();
        let has_user_msg = conv.messages.iter().any(|m| m.role == AiRole::User);
        if conv.hibernated {
            let meta = ConversationMeta {
                id: conv.id.clone(),
                title: conv.title.clone(),
                updated_at: conv.updated_at,
                message_count: conv.hibernated_msg_count,
                preview: String::new(),
                mode: format!("{:?}", conv.mode),
            };
            self.upsert_history_meta(meta);
        } else if has_user_msg && msg_count > 1 {
            let preview = conv
                .messages
                .iter()
                .rev()
                .find(|m| m.role == AiRole::Assistant)
                .map(|m| {
                    let s = m.content.trim();
                    if s.len() > 60 {
                        format!("{}…", &s[..s.floor_char_boundary(60)])
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or_default();
            let meta = ConversationMeta {
                id: conv.id.clone(),
                title: conv.title.clone(),
                updated_at: conv.updated_at,
                message_count: msg_count,
                preview,
                mode: format!("{:?}", conv.mode),
            };
            // 持久化：异步归档进 AetherDB（温数据层，含向量索引）
            if let Some(warm_store) = self.warm_data_store.as_ref() {
                warm_store.request_archive(conv.id.clone(), conv.clone());
            }
            self.upsert_history_meta(meta);
        }
        if idx == self.active {
            self.conversations.remove(idx);
            if self.conversations.is_empty() {
                self.conversations.push(AiConversation::new(
                    gen_conversation_id(),
                    "新对话".to_string(),
                ));
                self.load_slot_into_active(0);
            } else {
                let new_active = idx.min(self.conversations.len() - 1);
                self.load_slot_into_active(new_active);
            }
        } else {
            self.conversations.remove(idx);
            if idx < self.active {
                self.active -= 1;
            }
        }
        self.model_menu_open = false;
        self.dismiss_history_dropdown();
    }

    /// 历史元数据去重插入（同 id 替换旧记录），并限制内存条数
    fn upsert_history_meta(&mut self, meta: ConversationMeta) {
        if let Some(pos) = self.history.iter().position(|h| h.id == meta.id) {
            self.history.remove(pos);
        }
        self.history.insert(0, meta);
        const MAX_HISTORY: usize = 50;
        if self.history.len() > MAX_HISTORY {
            self.history.truncate(MAX_HISTORY);
        }
    }

    /// 从历史记录中恢复指定会话为新的活动标签页
    pub fn restore_from_history(&mut self, hist_idx: usize) {
        if hist_idx >= self.history.len() {
            return;
        }
        let (id, title, updated_at) = {
            let meta = &self.history[hist_idx];
            (meta.id.clone(), meta.title.clone(), meta.updated_at)
        };
        // 若该会话仍在 conversations 中（未真正关闭），直接切换
        if let Some(pos) = self.conversations.iter().position(|c| c.id == id) {
            self.switch_to(pos);
            self.dismiss_history_dropdown();
            return;
        }
        // 否则尝试从 AetherDB 加载完整会话，失败则创建占位会话
        self.snapshot_active_into_slot();
        let prev = self.active;
        let conv = self
            .warm_data_store
            .as_ref()
            .and_then(|store| store.load_conversation(&id).ok())
            .unwrap_or_else(|| {
                let mut c = AiConversation::new(id, title);
                c.updated_at = updated_at;
                c
            });
        self.conversations.push(conv);
        let new_idx = self.conversations.len() - 1;
        self.load_slot_into_active(new_idx);
        self.request_hibernate(prev);
        self.dismiss_history_dropdown();
    }

    /// 用首条用户消息自动生成活动会话标题（仍为默认标题时）
    pub fn sync_active_title(&mut self) {
        if self.active >= self.conversations.len() {
            return;
        }
        if self.conversations[self.active].title == "新对话" {
            if let Some(u) = self
                .messages
                .iter()
                .find(|m| m.role == AiRole::User)
                .map(|m| m.content.clone())
            {
                let t: String = u.trim().chars().take(18).collect();
                if !t.is_empty() {
                    self.conversations[self.active].title = t;
                }
            }
        }
    }

    /// 请求发起时记录在途请求是否启用深度思考（供超时看门狗放宽阈值）
    fn note_in_flight_thinking(&mut self, settings: &AiSettings) {
        self.in_flight_thinking =
            aether_ai::AiConfig::from_settings(settings).deepseek_thinking_active();
    }

    /// 检测当前会话是否超时（首包阈值内无任何响应）
    /// 返回 true 表示已超时且尚未收到任何响应
    ///
    /// 思考模式（如 DeepSeek v4 + reasoning_effort=max）的 prefill+思考启动
    /// 首包延迟常超过 30s，阈值放宽到 180s；非思考模式保持 30s。
    pub fn check_timeout(&self) -> bool {
        if !self.is_generating {
            return false;
        }
        let limit_secs = if self.in_flight_thinking { 180 } else { 30 };

        if let Ok(s) = self.stream_state.lock() {
            if let Some(start_time) = s.start_time {
                // 超过阈值且未收到任何响应
                if start_time.elapsed().as_secs() > limit_secs && !s.received_first_response {
                    return true;
                }
            }
        }

        false
    }

    /// 处理超时情况：添加详细的超时诊断消息
    pub fn handle_timeout(&mut self) {
        if !self.check_timeout() {
            return;
        }

        // 收集诊断信息
        let (elapsed_secs, thinking_mode, limit_secs) = {
            if let Ok(s) = self.stream_state.lock() {
                let elapsed = s.start_time
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                let thinking = self.in_flight_thinking;
                let limit = if thinking { 180 } else { 30 };
                (elapsed, thinking, limit)
            } else {
                (0, false, 30)
            }
        };

        // 停止当前生成
        self.stop_generation();

        // 构建详细的超时诊断消息
        let mut msg = String::from("[超时] AI 响应超时\n\n");
        
        // 基本信息
        msg.push_str(&format!("⏱️  等待时长: {} 秒\n", elapsed_secs));
        msg.push_str(&format!("🎯 超时阈值: {} 秒\n", limit_secs));
        msg.push_str(&format!("🧠 思考模式: {}\n\n", if thinking_mode { "开启" } else { "关闭" }));

        // 诊断分析
        msg.push_str("📊 诊断分析:\n");
        
        if elapsed_secs < 5 {
            msg.push_str("• 请求几乎立即超时，可能是网络连接完全中断\n");
            msg.push_str("• 检查: 网络连接、防火墙设置、代理配置\n");
        } else if elapsed_secs < limit_secs / 2 {
            msg.push_str("• 请求在超时阈值的一半内失败，可能是 DNS 解析或 TCP 连接问题\n");
            msg.push_str("• 检查: DNS 设置、API 服务器地址是否正确\n");
        } else if elapsed_secs >= limit_secs {
            if thinking_mode {
                msg.push_str("• 思考模式下超时，可能是模型正在深度推理\n");
                msg.push_str("• 检查: 问题复杂度、是否可以通过简化问题来加速\n");
            } else {
                msg.push_str("• 非思考模式下超时，可能是 API 服务器响应缓慢\n");
                msg.push_str("• 检查: API 服务状态、网络延迟\n");
            }
        }

        // 针对性建议
        msg.push_str("\n💡 针对性建议:\n");
        
        if elapsed_secs < 5 {
            msg.push_str("1. 立即检查网络连接（尝试访问其他网站）\n");
            msg.push_str("2. 检查防火墙/杀毒软件是否拦截了请求\n");
            msg.push_str("3. 如使用代理，确认代理配置正确\n");
        } else if thinking_mode {
            msg.push_str("1. 尝试关闭思考模式（设置 → AI → 思考模式）\n");
            msg.push_str("2. 简化问题描述，减少上下文长度\n");
            msg.push_str("3. 检查 API 配额是否充足\n");
        } else {
            msg.push_str("1. 稍后重试（可能是临时网络波动）\n");
            msg.push_str("2. 检查 API 服务状态页面\n");
            msg.push_str("3. 尝试切换到其他模型或提供商\n");
        }

        // 技术细节
        msg.push_str("\n🔧 技术细节:\n");
        msg.push_str(&format!("• 错误类型: 首包响应超时（{}秒内未收到任何数据）\n", limit_secs));
        msg.push_str("• 建议操作: 点击重试按钮或重新发送消息\n");

        self.add_assistant_message(msg);
    }

    /// 并发轮询所有会话：活动会话走扁平逻辑，其余走后台 drain。
    /// 返回 `(刚正常完成, 刚中断)` 两个会话下标列表：
    /// 前者应处理 Agent 动作（文件/命令），后者可抢救已接收的文件块。
    ///
    /// 特殊处理：当 worker 流水线运行中时，截断（Truncated）也视为完成，
    /// 以便 `advance_agent_pipeline` 检测截断并自动续写补全文件。
    pub fn poll_all_background(&mut self) -> (Vec<usize>, Vec<usize>) {
        let mut completed = Vec::new();
        let mut interrupted = Vec::new();
        // worker 流水线运行中：截断也视为完成（由 advance_agent_pipeline 自动续写）
        let pipeline_active = self.agent_pipeline.is_some();
        match self.check_background_result() {
            DrainEdge::Completed => completed.push(self.active),
            DrainEdge::Interrupted => interrupted.push(self.active),
            DrainEdge::Truncated => {
                if pipeline_active {
                    completed.push(self.active);
                }
                // 非流水线模式：UI 显示"继续生成"按钮，不自动处理文件/命令
            }
            DrainEdge::Pending => {}
        }
        let active = self.active;
        for i in 0..self.conversations.len() {
            if i == active {
                continue;
            }
            match self.conversations[i].drain_background() {
                DrainEdge::Completed => completed.push(i),
                DrainEdge::Interrupted => interrupted.push(i),
                DrainEdge::Truncated => {
                    if pipeline_active {
                        completed.push(i);
                    }
                    // 非流水线模式：后台会话截断仅显示消息，无按钮
                }
                DrainEdge::Pending => {}
            }
        }
        (completed, interrupted)
    }

    /// 是否存在任一会话正在生成（用于维持定时重绘）
    pub fn any_generating(&self) -> bool {
        self.is_generating
            || self
                .conversations
                .iter()
                .enumerate()
                .any(|(i, c)| i != self.active && c.is_generating)
    }

    /// 指定会话的模式（活动会话读扁平，其余读槽位）
    pub fn mode_of(&self, conv_idx: usize) -> AiMode {
        if conv_idx == self.active {
            self.mode
        } else {
            self.conversations
                .get(conv_idx)
                .map(|c| c.mode)
                .unwrap_or(self.mode)
        }
    }

    /// 指定会话的最后一条助手消息文本
    pub fn last_assistant_text_of(&self, conv_idx: usize) -> Option<String> {
        if conv_idx == self.active {
            self.last_assistant_text()
        } else {
            self.conversations
                .get(conv_idx)
                .and_then(|c| c.last_assistant_text())
        }
    }

    /// 指定会话中从后往前第一条满足谓词的助手消息文本。
    /// 用于生成中断后的抢救：跳过末尾的错误提示消息，定位携带文件块的内容消息。
    pub fn last_assistant_text_matching_of(
        &self,
        conv_idx: usize,
        pred: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let msgs = if conv_idx == self.active {
            &self.messages
        } else {
            &self.conversations.get(conv_idx)?.messages
        };
        msgs.iter()
            .rev()
            .find(|m| m.role == AiRole::Assistant && pred(&m.content))
            .map(|m| m.content.clone())
    }

    /// 向指定会话追加一条助手消息（用于会话作用域的 Agent 动作反馈）
    pub fn add_assistant_message_to(&mut self, conv_idx: usize, content: String) {
        if conv_idx == self.active {
            self.add_assistant_message(content);
        } else if let Some(c) = self.conversations.get_mut(conv_idx) {
            c.messages.push(AiMessage::new(AiRole::Assistant, content));
            c.stick_to_bottom = true;
            c.updated_at = now_secs();
        }
    }

    /// 添加用户消息
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(AiMessage::new(AiRole::User, content));
        self.stick_to_bottom = true;
        self.sync_hot_data();
    }

    // ===== 待发送图片（多模态）=====

    /// 附加一张待发送图片（随下一条用户消息发送；是否允许由发送时的
    /// 多模态门禁统一校验）
    pub fn add_pending_image(&mut self, attachment: AiImageAttachment) {
        self.pending_images.push(attachment);
    }

    /// 移除指定下标的待发送图片（点击输入框图片 chip）
    pub fn remove_pending_image(&mut self, index: usize) {
        if index < self.pending_images.len() {
            self.pending_images.remove(index);
            self.hover_image_chip = None;
        }
    }

    /// 清空全部待发送图片（切换模型关闭多模态等场景调用）
    pub fn clear_pending_images(&mut self) {
        self.pending_images.clear();
        self.hover_image_chip = None;
    }

    /// 命中测试：返回点击位置覆盖的图片 chip 下标（渲染帧注册的区域）
    pub fn hit_test_image_chip(&self, px: f32, py: f32) -> Option<usize> {
        self.image_chip_regions
            .iter()
            .find(|(_, x, y, w, h)| px >= *x && px <= *x + *w && py >= *y && py <= *y + *h)
            .map(|(i, _, _, _, _)| *i)
    }

    /// 待发送图片 chips 行高度：有待发图片时占一行，否则为 0
    pub const IMAGE_CHIPS_ROW_H: f32 = 32.0;
    pub fn image_chips_row_height(&self) -> f32 {
        if self.pending_images.is_empty() {
            0.0
        } else {
            Self::IMAGE_CHIPS_ROW_H
        }
    }

    /// 输入区域总高度 = 文本输入高度 + 固定工具栏与间距(44) + 图片 chips 行。
    /// 所有依赖输入区几何的位置（裁剪/命中/IME 定位）必须经此方法计算，
    /// 避免硬编码 `input_computed_height + 44.0` 与 chips 行漂移。
    pub fn input_area_height(&self) -> f32 {
        self.input_computed_height + 44.0 + self.image_chips_row_height()
    }

    /// 添加助手消息
    pub fn add_assistant_message(&mut self, content: String) {
        self.messages
            .push(AiMessage::new(AiRole::Assistant, content));
        self.stick_to_bottom = true;
        self.sync_hot_data();
    }

    /// 添加工具结果消息（Agent 自续回喂）：不作为用户气泡显示，
    /// UI 渲染为简洁的工具卡片，语义上属于 agent 内部步骤。
    pub fn add_tool_message(&mut self, content: String) {
        self.messages.push(AiMessage::new(AiRole::Tool, content));
        self.stick_to_bottom = true;
        self.sync_hot_data();
    }

    /// 添加待确认消息：当 AI 对用户请求存在歧义时主动发起反问
    ///
    /// # 参数
    /// * `questions` - 需要用户澄清的具体问题点列表
    /// * `options` - 可选的快捷选项按钮列表（如适用）
    pub fn add_pending_confirmation_message(
        &mut self,
        questions: Vec<String>,
        options: Option<Vec<String>>,
    ) {
        let mut content = String::from("[待确认] 需要您澄清以下问题：\n\n");

        for (i, question) in questions.iter().enumerate() {
            content.push_str(&format!("{}. {}\n", i + 1, question));
        }

        if let Some(opts) = options {
            if !opts.is_empty() {
                content.push_str("\n快捷选项：\n");
                for (i, option) in opts.iter().enumerate() {
                    content.push_str(&format!("  {}. {}\n", i + 1, option));
                }
            }
        }

        self.messages
            .push(AiMessage::new(AiRole::PendingConfirmation, content));
        self.stick_to_bottom = true;
        self.sync_hot_data();
    }

    /// 发送消息（AI-H01: 非阻塞 — HTTP 调用在后台线程执行，结果通过 stream_state 流式返回）
    ///
    /// 智能判断模式：根据用户输入内容自动判断是简单问答还是执行任务
    pub fn send_message(&mut self, settings: &AiSettings) -> Result<String, String> {
        self.agent_iter_count = 0;
        self.agent_pipeline = None;

        // 智能判断模式：根据用户输入内容自动判断是简单问答还是执行任务
        let mode = self.detect_mode_from_input(&self.input);
        self.send_message_internal(settings, self.input.clone(), mode, None)
    }

    /// 根据用户输入内容智能判断模式
    ///
    /// 判断逻辑：
    /// 1. 如果输入包含文件操作关键词（如"创建文件"、"修改代码"、"删除文件"等），则使用 Agent 模式
    /// 2. 如果输入包含命令执行关键词（如"运行命令"、"执行脚本"等），则使用 Agent 模式
    /// 3. 如果输入包含多步骤任务关键词（如"步骤"、"计划"、"任务"等），则使用 Agent 模式
    /// 4. 否则使用 Ask 模式（简单问答）
    fn detect_mode_from_input(&self, input: &str) -> AiMode {
        let input_lower = input.to_lowercase();

        // 文件操作关键词
        let file_keywords = [
            "创建文件",
            "新建文件",
            "修改文件",
            "删除文件",
            "重写文件",
            "创建代码",
            "修改代码",
            "删除代码",
            "重构代码",
            "生成文件",
            "保存文件",
            "写入文件",
            "create file",
            "modify file",
            "delete file",
            "write file",
            "generate code",
            "refactor code",
            "update file",
        ];

        // 命令执行关键词
        let command_keywords = [
            "运行命令",
            "执行命令",
            "运行脚本",
            "执行脚本",
            "运行程序",
            "执行程序",
            "启动服务",
            "停止服务",
            "run command",
            "execute command",
            "run script",
            "execute script",
            "start service",
            "stop service",
            "run program",
        ];

        // 多步骤任务关键词
        let task_keywords = [
            "步骤",
            "计划",
            "任务",
            "流程",
            "阶段",
            "第一步",
            "第二步",
            "第三步",
            "首先",
            "然后",
            "最后",
            "step",
            "plan",
            "task",
            "process",
            "stage",
            "first",
            "then",
            "next",
            "finally",
        ];

        // 检查是否包含文件操作关键词
        for keyword in &file_keywords {
            if input_lower.contains(keyword) {
                return AiMode::Agent;
            }
        }

        // 检查是否包含命令执行关键词
        for keyword in &command_keywords {
            if input_lower.contains(keyword) {
                return AiMode::Agent;
            }
        }

        // 检查是否包含多步骤任务关键词
        for keyword in &task_keywords {
            if input_lower.contains(keyword) {
                return AiMode::Agent;
            }
        }

        // 默认为简单问答模式
        AiMode::Ask
    }

    /// 发送消息，并附带当前编辑器的上下文
    pub fn send_message_with_context(
        &mut self,
        settings: &AiSettings,
        editor: &dyn EditorContextProvider,
        mode: AiMode,
    ) -> Result<String, String> {
        self.agent_iter_count = 0;
        self.agent_pipeline = None;
        let context = editor.gather_context(&self.attachments);
        self.send_message_internal(settings, self.input.clone(), mode, Some(context))
    }

    /// 发送消息，使用已经准备好的上下文字符串
    pub fn send_message_with_prepared_context(
        &mut self,
        settings: &AiSettings,
        context: String,
        mode: AiMode,
    ) -> Result<String, String> {
        // 询问卡片自定义回答：输入框文本作为待回答问题的答案而非普通提问。
        // 若目标卡片已失效（消息被裁剪/会话切换），则降级为普通发送。
        if let Some((mi, seq)) = self.pending_ask_custom.take() {
            if !self.ask_blocks_of(mi).is_empty() {
                let text = self.input.trim().to_string();
                if text.is_empty() {
                    return Err("请先输入自定义回答再发送".to_string());
                }
                let all_done = self.answer_ask(mi, seq, text);
                if !all_done {
                    return Ok("已记录回答，请继续回答其余问题".to_string());
                }
                return self.send_ask_reply(settings, mi, mode, context);
            }
        }
        self.agent_iter_count = 0;
        self.agent_pipeline = None;
        self.send_message_internal(settings, self.input.clone(), mode, Some(context))
    }

    /// Agent 工具结果回喂：把终端命令输出作为上下文再次发起请求，驱动
    /// 「推理 → 执行 → 结果回喂 → 继续推理」循环。受最大轮次限制防止无限回环。
    ///
    /// 与 `send_message_internal` 的区别：工具结果以 `AiRole::Tool` 消息记录，
    /// 不作为用户气泡显示，避免用户困惑。
    pub fn continue_agent_with_tool_result(
        &mut self,
        settings: &AiSettings,
        feedback: String,
        mode: AiMode,
    ) -> Result<String, String> {
        const MAX_AGENT_ITERATIONS: u32 = 5;
        if self.agent_iter_count >= MAX_AGENT_ITERATIONS {
            return Err(format!("已达最大自动执行轮次（{}）", MAX_AGENT_ITERATIONS));
        }
        self.agent_iter_count += 1;

        if feedback.is_empty() {
            return Err("工具结果为空".to_string());
        }
        if self.is_generating {
            return Err("正在等待上一次回复，请稍后再试".to_string());
        }

        // 工具结果以 Tool 角色记录（不显示为用户气泡）
        self.add_tool_message(feedback.clone());
        self.is_generating = true;
        self.note_in_flight_thinking(settings);
        self.should_stop.store(false, Ordering::SeqCst);
        if let Ok(mut s) = self.stream_state.lock() {
            *s = AiStreamState::default();
        }

        let settings = settings.clone();
        let context = String::new();
        let mut messages = build_chat_prompt(&settings, &context, mode);
        let input_budget = settings
            .max_input_tokens
            .map(|v| v as usize)
            .unwrap_or(24000);
        messages.extend(Self::history_to_chat_messages(&self.messages, input_budget));
        let stream_state = Arc::clone(&self.stream_state);
        let should_stop = Arc::clone(&self.should_stop);

        spawn_ai_stream(settings, messages, stream_state, should_stop);

        Ok("Agent 续跑已提交".to_string())
    }

    /// 逐任务 worker 调用：以给定的 system/user 两条消息发起**聚焦**流式请求，
    /// **不含**累积对话历史（因此上下文窗口占用极小）。生成内容流入新的助手消息，
    /// 完成后由编排器（editor::advance_agent_pipeline）落盘并推进下一任务。
    pub fn stream_focused(&mut self, settings: &AiSettings, system: String, user: String) {
        self.is_generating = true;
        self.note_in_flight_thinking(settings);
        self.should_stop.store(false, Ordering::SeqCst);
        // 重置截断标记：worker 每次调用都是独立生成，不应继承上次的截断状态
        self.last_truncated = false;
        if let Ok(mut s) = self.stream_state.lock() {
            *s = AiStreamState::default();
        }
        // worker 输出必须流入独立的新助手消息：若续写在上一条消息（如执行计划清单）末尾，
        // 首个 FILE 标记会粘在清单行后失去行锚定，导致既不渲染卡片也无法解析落盘。
        self.add_assistant_message(String::new());
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
                images: Vec::new(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user,
                images: Vec::new(),
            },
        ];
        spawn_ai_stream(
            settings.clone(),
            messages,
            Arc::clone(&self.stream_state),
            Arc::clone(&self.should_stop),
        );
    }

    /// 问题扩写：将当前输入框文本发送给 AI 进行扩写，
    /// 扩写结果流式写回输入框（不作为聊天消息）。
    /// 动画流程：原文渐隐 → 流式写入新文本（渐显）。
    pub fn expand_input(&mut self, settings: &AiSettings) -> Result<String, String> {
        let user_input = self.input.trim().to_string();
        if user_input.is_empty() {
            return Err("请先输入问题，再使用扩写功能".to_string());
        }
        if self.is_generating {
            return Err("正在等待上一次回复，请稍后再试".to_string());
        }

        self.is_generating = true;
        self.is_expanding = true;
        self.note_in_flight_thinking(settings);
        self.should_stop.store(false, Ordering::SeqCst);
        if let Ok(mut s) = self.stream_state.lock() {
            *s = AiStreamState::default();
        }

        // 保存原文用于渐隐动画，然后清空输入框
        self.expand_original_text = user_input.clone();
        self.expand_anim_phase = ExpandAnimPhase::FadeOut;
        self.expand_anim_progress = 0.0;
        self.expand_anim_start_ms = now_millis();
        self.input.clear();
        self.caret_pos = 0;
        // 锁定输入框焦点，防止扩写期间用户编辑
        self.input_focused = false;

        let system = "你是一个问题扩写助手。用户会给你一个问题或请求，你需要将其扩写为更详细、更完整、更清晰的版本。\
            扩写后的文本应该：\n\
            1. 保持原始意图不变\n\
            2. 补充必要的上下文和细节\n\
            3. 使问题更加具体和明确\n\
            4. 直接输出扩写后的文本，不要添加任何解释、标记或前缀"
            .to_string();
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
                images: Vec::new(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_input,
                images: Vec::new(),
            },
        ];
        spawn_ai_stream(
            settings.clone(),
            messages,
            Arc::clone(&self.stream_state),
            Arc::clone(&self.should_stop),
        );

        Ok("正在扩写问题...".to_string())
    }

    /// 用给定内容替换最后一条助手消息（无则追加）；用于把规划器原始清单块替换为可读的执行计划。
    pub fn rewrite_last_assistant(&mut self, content: String) {
        if let Some(last) = self.messages.last_mut() {
            if last.role == AiRole::Assistant {
                last.content = content;
                last.reasoning = None;
                last.reasoning_collapsed = false;
                self.stick_to_bottom = true;
                self.sync_hot_data();
                return;
            }
        }
        self.add_assistant_message(content);
    }

    /// 切换文件卡片的展开/折叠状态（P0 可展开预览）
    pub fn toggle_file_card_expand(&mut self, msg_idx: usize, block_seq: usize) {
        let key = (msg_idx, block_seq);
        if !self.expanded_file_cards.remove(&key) {
            self.expanded_file_cards.insert(key);
        }
    }

    /// 记录文件修改前/后快照（供差异可视化）。单条内容超 20 万字符截断，总量超 32 条淘汰最旧。
    pub fn record_diff_snapshot(&mut self, path: &str, old: String, new: String) {
        const MAX_CHARS: usize = 200_000;
        let trunc = |s: String| {
            if s.chars().count() <= MAX_CHARS {
                s
            } else {
                s.chars().take(MAX_CHARS).collect()
            }
        };
        self.diff_snapshots.retain(|(p, _, _)| p != path);
        self.diff_snapshots
            .push((path.to_string(), trunc(old), trunc(new)));
        if self.diff_snapshots.len() > 32 {
            let extra = self.diff_snapshots.len() - 32;
            self.diff_snapshots.drain(0..extra);
        }
        // 快照变化使所有缓存的差异预览失效
        self.diff_preview_cache.clear();
    }

    /// 查询指定路径的最新快照 (旧内容, 新内容)
    pub fn diff_snapshot(&self, path: &str) -> Option<(String, String)> {
        self.diff_snapshots
            .iter()
            .rev()
            .find(|(p, _, _)| p == path)
            .map(|(_, o, n)| (o.clone(), n.clone()))
    }

    /// 获取文件卡片的差异预览行（带缓存）：无快照时返回 None（渲染回退为原文预览）
    pub fn diff_preview_for(
        &mut self,
        key: (usize, usize),
        path: &str,
    ) -> Option<Vec<crate::diff::DiffLine>> {
        if let Some(c) = self.diff_preview_cache.get(&key) {
            return Some(c.clone());
        }
        let (old, new) = self.diff_snapshot(path)?;
        let diff = crate::diff::line_diff(&old, &new);
        self.diff_preview_cache.insert(key, diff.clone());
        Some(diff)
    }

    /// 命中测试对话内可点击文件名，返回路径
    pub fn hit_test_file_link(&self, x: f32, y: f32) -> Option<String> {
        self.file_link_regions
            .iter()
            .rev()
            .find(|(rx, ry, rw, rh, _)| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh)
            .map(|(_, _, _, _, p)| p.clone())
    }

    /// Tab 焦点移动到下一/上一可交互元素，返回焦点动作克隆
    pub fn tab_focus_advance(&mut self, reverse: bool) -> Option<TabFocusAction> {
        let n = self.tab_focus_regions.len();
        if n == 0 {
            self.tab_focus_index = None;
            return None;
        }
        let next = match self.tab_focus_index {
            None if reverse => n - 1,
            None => 0,
            Some(i) if reverse => i.saturating_add(n - 1) % n,
            Some(i) => (i + 1) % n,
        };
        self.tab_focus_index = Some(next);
        self.tab_focus_regions
            .get(next)
            .map(|(_, _, _, _, a)| a.clone())
    }

    /// 当前 Tab 焦点对应的动作（Enter/Space 激活用）
    pub fn tab_focus_action(&self) -> Option<TabFocusAction> {
        self.tab_focus_index
            .and_then(|i| self.tab_focus_regions.get(i))
            .map(|(_, _, _, _, a)| a.clone())
    }

    /// 清除 Tab 焦点（Esc / 面板隐藏时）
    pub fn clear_tab_focus(&mut self) {
        self.tab_focus_index = None;
    }

    /// 命中测试文件卡片，返回 (msg_idx, block_seq)
    pub fn hit_test_file_card(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        self.file_card_regions
            .iter()
            .find(|(_, _, rx, ry, rw, rh)| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh)
            .map(|(mi, bi, ..)| (*mi, *bi))
    }

    /// 提取指定消息内的全部询问块（ASK），按展示顺序返回（问题, 选项）列表
    pub fn ask_blocks_of(&self, msg_idx: usize) -> Vec<(String, Vec<String>)> {
        self.messages
            .get(msg_idx)
            .map(|m| {
                crate::ai_agent::parse_display_blocks(&m.content)
                    .into_iter()
                    .filter_map(|b| match b {
                        crate::ai_agent::AgentDisplayBlock::Ask { question, options } => {
                            Some((question, options))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 获取询问卡片指定选项的文本（下标越界返回 None）
    pub fn ask_option_text(
        &self,
        msg_idx: usize,
        ask_seq: usize,
        opt_idx: usize,
    ) -> Option<String> {
        self.ask_blocks_of(msg_idx)
            .into_iter()
            .nth(ask_seq)
            .and_then(|(_, options)| options.into_iter().nth(opt_idx))
    }

    /// 记录询问卡片的回答并清空输入框；返回该消息内全部问题是否已回答完毕
    pub fn answer_ask(&mut self, msg_idx: usize, ask_seq: usize, answer: String) -> bool {
        let answer = answer.trim().to_string();
        if answer.is_empty() {
            return false;
        }
        let total = self.ask_blocks_of(msg_idx).len();
        if total == 0 || ask_seq >= total {
            return false;
        }
        let Some(msg) = self.messages.get_mut(msg_idx) else {
            return false;
        };
        if msg.ask_answers.len() < total {
            msg.ask_answers.resize(total, None);
        }
        msg.ask_answers[ask_seq] = Some(answer);
        let all_done = msg.ask_answers.iter().all(|a| a.is_some());
        self.input.clear();
        self.caret_pos = 0;
        self.pending_ask_custom = None;
        self.sync_hot_data();
        all_done
    }

    /// 汇总指定消息内全部问题的回答，生成回传给模型的用户消息文本
    pub fn build_ask_reply(&self, msg_idx: usize) -> Option<String> {
        let msg = self.messages.get(msg_idx)?;
        let blocks = self.ask_blocks_of(msg_idx);
        if blocks.is_empty() {
            return None;
        }
        let mut out = String::from("【用户对你提问的回答】\n");
        for (i, (question, _)) in blocks.iter().enumerate() {
            let answer = msg
                .ask_answers
                .get(i)
                .and_then(|a| a.as_deref())
                .unwrap_or("未回答");
            out.push_str(&format!("Q: {}\nA: {}\n", question, answer));
        }
        out.push_str("请根据以上回答继续执行。");
        Some(out)
    }

    /// 把指定消息的汇总回答作为用户消息发送并继续生成（走普通发送管线）
    pub fn send_ask_reply(
        &mut self,
        settings: &AiSettings,
        msg_idx: usize,
        mode: AiMode,
        context: String,
    ) -> Result<String, String> {
        let reply = self
            .build_ask_reply(msg_idx)
            .ok_or_else(|| "未找到可发送的回答".to_string())?;
        self.agent_iter_count = 0;
        self.agent_pipeline = None;
        self.send_message_internal(settings, reply, mode, Some(context))
    }

    fn send_message_internal(
        &mut self,
        settings: &AiSettings,
        user_input: String,
        mode: AiMode,
        context: Option<String>,
    ) -> Result<String, String> {
        if user_input.is_empty() {
            return Err("输入为空".to_string());
        }

        // H-17: 限制并发线程数 — 正在生成时拒绝新请求，防止无限制 spawn 线程
        if self.is_generating {
            return Err("正在等待上一次回复，请稍后再试".to_string());
        }

        // 多模态门禁：附带图片但当前模型未声明支持多模态时拒绝发送，
        // 避免非视觉模型返回 400（"This model does not support image"）。
        // 所有发送路径（发送按钮/Enter/重试/扩写回复）均经本函数，一处覆盖。
        if !self.pending_images.is_empty() && !settings.multimodal {
            return Err(
                "当前模型不支持图片输入：请在模型设置中勾选「多模态（图片）」，或移除已附加的图片"
                    .to_string(),
            );
        }
        // 取出待发图片（发送即清空；门禁通过后不再有早退路径）
        let pending_images = std::mem::take(&mut self.pending_images);

        // 用户手动发送消息（或点击"继续"）时，重置截断标记
        self.last_truncated = false;

        // 限制输入长度（M-03）
        const MAX_INPUT_LEN: usize = 10000;
        let user_input = if user_input.len() > MAX_INPUT_LEN {
            let safe_len = user_input.floor_char_boundary(MAX_INPUT_LEN);
            user_input[..safe_len].to_string()
        } else {
            user_input
        };

        self.add_user_message(user_input.clone());
        // 历史消息只保留图片文件名用于展示（数据不持久化、不随历史重发）
        if !pending_images.is_empty() {
            if let Some(last) = self.messages.last_mut() {
                last.image_names = pending_images.iter().map(|p| p.filename.clone()).collect();
            }
        }
        self.input.clear();
        self.caret_pos = 0;
        self.is_generating = true;
        self.note_in_flight_thinking(settings);
        self.should_stop.store(false, Ordering::SeqCst);
        // 重置流式状态
        if let Ok(mut s) = self.stream_state.lock() {
            *s = AiStreamState::default();
        }

        // 限制消息历史长度（M-05: 滑动窗口，保留最近 40 条非系统消息 + 系统消息）。
        // 显示历史的上界；实际发送给模型的历史再按 token 预算二次窗口切片，见
        // history_to_chat_messages，兼顾上下文连续性与性能。
        const MAX_HISTORY: usize = 40;
        if self.messages.len() > MAX_HISTORY + 1 {
            let system_msgs: Vec<AiMessage> = self
                .messages
                .iter()
                .filter(|m| m.role == AiRole::System)
                .cloned()
                .collect();
            let non_system: Vec<AiMessage> = self
                .messages
                .iter()
                .filter(|m| m.role != AiRole::System)
                .cloned()
                .collect();
            let recent_start = non_system.len().saturating_sub(MAX_HISTORY);
            let recent: Vec<AiMessage> = non_system.into_iter().skip(recent_start).collect();
            self.messages = system_msgs;
            self.messages.extend(recent);
        }

        let settings = settings.clone();
        let context = context.unwrap_or_default();
        // 系统前缀（system/Agent 能力/模式/上下文）+ 经窗口切片的会话历史（含本轮输入），
        // 保证同一轮对话上下文连续；历史来自本会话的 self.messages，天然与其它标签页隔离。
        let mut messages = build_chat_prompt(&settings, &context, mode);
        // ACE playbook：注入已沉淀的经验策略，并记录条目 ID 供反馈归因
        let mut used_bullet_ids: Vec<String> = Vec::new();
        if let Some(warm) = self.warm_data_store.as_ref() {
            if let Ok(hits) = warm.search_playbook(&user_input, 5) {
                if !hits.is_empty() {
                    used_bullet_ids = hits.iter().map(|(b, _)| b.id.clone()).collect();
                    messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: crate::reflector::format_bullets(&hits),
                        images: Vec::new(),
                    });
                }
            }
        }
        // 记录到活动会话槽位（接受/拒绝编辑时回填 helpful/harmful）
        if !used_bullet_ids.is_empty() {
            if let Some(slot) = self.conversations.get_mut(self.active) {
                slot.used_bullet_ids = used_bullet_ids;
            }
        }
        // 最大输入 Token（上下文预算）：按激活模型配置切片历史，None 时用内置默认
        let input_budget = settings
            .max_input_tokens
            .map(|v| v as usize)
            .unwrap_or(24000);
        messages.extend(Self::history_to_chat_messages(&self.messages, input_budget));
        // 图片只附到本轮最新的 user 消息（OpenAI 兼容 image_url 块；
        // history_to_chat_messages 保证最新消息必在切片内）。历史消息不重发图片，
        // 控制多轮对话的请求体大小与 token 成本。
        if !pending_images.is_empty() {
            let images: Vec<ChatImage> = pending_images
                .iter()
                .map(|p| ChatImage {
                    mime: p.mime.clone(),
                    data_b64: p.data_b64.clone(),
                })
                .collect();
            if let Some(last_user) = messages.iter_mut().rev().find(|m| m.role == "user") {
                last_user.images = images;
            }
        }
        let stream_state = Arc::clone(&self.stream_state);
        let should_stop = Arc::clone(&self.should_stop);

        spawn_ai_stream(settings, messages, stream_state, should_stop);

        Ok("请求已提交".to_string())
    }

    /// 估算文本 token 数（保守上界：按字符数计，CJK≈1 token/字，英文会高估但更安全）
    fn estimate_tokens(s: &str) -> usize {
        s.chars().count()
    }

    /// 将本会话消息转换为发送给模型的历史，应用"窗口切片"：
    /// - 跳过用于展示的 System 欢迎语（真正的 system 由 build_chat_prompt 注入）；
    /// - 从最近往前累加，受最大消息数与 token 预算双重限制，避免上下文过长影响性能；
    /// - 始终至少包含最后一条（当前用户输入）。
    ///
    /// 历史取自各会话自身的 messages，因此不同标签页/对话轮次天然隔离、互不串扰。
    fn history_to_chat_messages(
        messages: &[AiMessage],
        max_input_tokens: usize,
    ) -> Vec<ChatMessage> {
        const MAX_MSGS: usize = 40;
        // 下限保护：预算过小会几乎丢光上下文，至少保留约 2000 字符
        let budget = max_input_tokens.max(2000);
        let eligible: Vec<&AiMessage> = messages
            .iter()
            .filter(|m| m.role != AiRole::System)
            .collect();
        let mut selected: Vec<&AiMessage> = Vec::new();
        let mut tokens = 0usize;
        for m in eligible.iter().rev() {
            let t = Self::estimate_tokens(&m.content);
            if !selected.is_empty() && (selected.len() >= MAX_MSGS || tokens + t > budget) {
                break;
            }
            tokens += t;
            selected.push(m);
        }
        selected.reverse();
        selected
            .into_iter()
            .map(|m| match m.role {
                AiRole::User => ChatMessage::user(m.content.clone()),
                AiRole::Tool => ChatMessage::user(m.content.clone()),
                _ => ChatMessage {
                    role: "assistant".to_string(),
                    content: m.content.clone(),
                    images: Vec::new(),
                },
            })
            .collect()
    }

    /// 输入字符（在光标位置插入）
    pub fn input_char(&mut self, ch: char) {
        if self.caret_pos > self.input.len() {
            self.caret_pos = self.input.len();
        }
        self.input.insert(self.caret_pos, ch);
        self.caret_pos += ch.len_utf8();
    }

    /// 在光标位置插入字符串（用于 IME 提交等一次性多字符输入）
    pub fn insert_str(&mut self, s: &str) {
        if self.caret_pos > self.input.len() {
            self.caret_pos = self.input.len();
        }
        self.input.insert_str(self.caret_pos, s);
        self.caret_pos += s.len();
    }

    /// 退格（删除光标前一个字符）
    pub fn backspace(&mut self) {
        if self.caret_pos > 0 {
            let prev_pos = self.prev_char_boundary();
            self.input.drain(prev_pos..self.caret_pos);
            self.caret_pos = prev_pos;
        }
    }

    /// 删除（删除光标后一个字符）
    pub fn delete(&mut self) {
        if self.caret_pos < self.input.len() {
            let next_pos = self.next_char_boundary();
            self.input.drain(self.caret_pos..next_pos);
        }
    }

    /// 粘贴文本到光标位置
    pub fn paste_text(&mut self, text: &str) {
        self.input.insert_str(self.caret_pos, text);
        self.caret_pos += text.len();
    }

    /// 重试上一次请求：使用最后一条用户消息重新发送
    pub fn retry_last_request(&mut self, settings: &AiSettings) -> Result<String, String> {
        if self.is_generating {
            return Err("正在等待上一次回复，请稍后再试".to_string());
        }

        // 查找最后一条用户消息
        let last_user_msg = self
            .messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::User)
            .map(|m| m.content.clone());

        if let Some(input) = last_user_msg {
            // 移除最后一条助手消息（错误消息）
            if matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
                self.messages.pop();
            }

            // 重新发送
            self.input = input;
            self.send_message(settings)
        } else {
            Err("没有找到可重试的请求".to_string())
        }
    }

    /// 光标左移
    pub fn move_caret_left(&mut self) {
        if self.caret_pos > 0 {
            self.caret_pos = self.prev_char_boundary();
        }
    }

    /// 光标右移
    pub fn move_caret_right(&mut self) {
        if self.caret_pos < self.input.len() {
            self.caret_pos = self.next_char_boundary();
        }
    }

    /// 光标移到行首
    pub fn move_caret_home(&mut self) {
        self.caret_pos = 0;
    }

    /// 光标移到行尾
    pub fn move_caret_end(&mut self) {
        self.caret_pos = self.input.len();
    }

    /// 获取前一个字符边界（UTF-8）
    fn prev_char_boundary(&self) -> usize {
        let mut pos = self.caret_pos;
        while pos > 0 {
            pos -= 1;
            if self.input.is_char_boundary(pos) {
                return pos;
            }
        }
        0
    }

    /// 获取后一个字符边界（UTF-8）
    fn next_char_boundary(&self) -> usize {
        let mut pos = self.caret_pos + 1;
        while pos < self.input.len() {
            if self.input.is_char_boundary(pos) {
                return pos;
            }
            pos += 1;
        }
        self.input.len()
    }

    /// 清除输入
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.caret_pos = 0;
    }

    /// 停止当前生成（后台线程在下一次循环检查时退出）
    pub fn stop_generation(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.is_generating = false;
        // 手动停止后 drain 不再走 done 分支，思考计时在此结算
        if let Some(last) = self.messages.last_mut() {
            last.stop_reasoning_timer();
        }
        if let Ok(mut s) = self.stream_state.lock() {
            s.done = true;
        }
    }

    /// 重新生成：移除末尾助手消息，用最近一条用户消息重新发送
    pub fn regenerate(&mut self, settings: &AiSettings) {
        if self.is_generating {
            return;
        }
        while matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
            self.messages.pop();
        }
        let last_user = self
            .messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::User)
            .map(|m| m.content.clone());
        if let Some(input) = last_user {
            if matches!(self.messages.last(), Some(m) if m.role == AiRole::User) {
                self.messages.pop();
            }
            self.input = input;
            let _ = self.send_message(settings);
        }
    }

    /// 清除所有对话
    pub fn clear_history(&mut self) {
        self.messages.clear();
        self.messages.push(AiMessage::new(
            AiRole::System,
            "你好！我是 AI 助手，可以帮助你解释代码、重构、修复问题、生成测试等。".to_string(),
        ));
        if let Ok(mut s) = self.stream_state.lock() {
            *s = AiStreamState::default();
        }
        self.is_generating = false;
    }

    /// AI-H01: 轮询后台线程结果，应在渲染循环中调用
    ///
    /// 返回结束边沿：`Completed` 表示本帧正常完成（处理 Agent 动作：文件/命令）；
    /// `Interrupted` 表示本帧因错误中断（可抢救已接收的文件块）；
    /// `Truncated` 表示输出因 max_tokens 被截断（用户可点击"继续生成"）。
    pub fn check_background_result(&mut self) -> DrainEdge {
        if !self.is_generating {
            return DrainEdge::Pending;
        }
        let delta = {
            if let Ok(mut s) = self.stream_state.lock() {
                let partial = std::mem::take(&mut s.partial);
                let reasoning = std::mem::take(&mut s.reasoning);
                let done = s.done;
                let error = s.error.take();
                let truncated = s.truncated.take();
                if done {
                    s.done = false;
                }
                Some((partial, reasoning, done, error, truncated))
            } else {
                None
            }
        };
        let mut edge = DrainEdge::Pending;
        if let Some((partial, reasoning, done, error, truncated)) = delta {
            // ===== 扩写模式：流式结果写回输入框 =====
            if self.is_expanding {
                // 更新动画进度
                let elapsed = now_millis().saturating_sub(self.expand_anim_start_ms);
                match self.expand_anim_phase {
                    ExpandAnimPhase::FadeOut => {
                        // 渐隐阶段：300ms 内完成
                        const FADE_OUT_MS: u64 = 300;
                        self.expand_anim_progress = (elapsed as f32 / FADE_OUT_MS as f32).min(1.0);
                        if self.expand_anim_progress >= 1.0 {
                            // 渐隐完成，进入流式写入阶段
                            self.expand_anim_phase = ExpandAnimPhase::Streaming;
                            self.expand_anim_progress = 0.0;
                            self.expand_anim_start_ms = now_millis();
                        }
                    }
                    ExpandAnimPhase::Streaming => {
                        // 流式写入阶段：新文本随 token 到达逐渐显示
                        // 进度基于已接收文本长度（简单线性增长）
                        self.expand_anim_progress = 1.0; // 流式阶段直接显示
                    }
                    ExpandAnimPhase::None => {}
                }

                if !partial.is_empty() {
                    // 首个 token 到达时，确保已进入流式阶段
                    if self.expand_anim_phase == ExpandAnimPhase::FadeOut {
                        self.expand_anim_phase = ExpandAnimPhase::Streaming;
                        self.expand_anim_progress = 1.0;
                    }
                    self.input.push_str(&partial);
                    self.caret_pos = self.input.len();
                }
                if let Some(err) = error {
                    self.is_generating = false;
                    self.is_expanding = false;
                    self.expand_anim_phase = ExpandAnimPhase::None;
                    self.expand_anim_progress = 0.0;
                    self.expand_original_text.clear();
                    if self.input.is_empty() {
                        self.input = err;
                        self.caret_pos = self.input.len();
                    }
                    return DrainEdge::Interrupted;
                }
                if done {
                    self.is_generating = false;
                    self.is_expanding = false;
                    self.expand_anim_phase = ExpandAnimPhase::None;
                    self.expand_anim_progress = 0.0;
                    self.expand_original_text.clear();
                    let trimmed = self.input.trim().to_string();
                    self.input = trimmed;
                    self.caret_pos = self.input.len();
                    edge = DrainEdge::Completed;
                }
                return edge;
            }

            // ===== 正常对话模式 =====
            // 深度思考（DeepSeek reasoning_content）先于回答到达：单独承载于助手消息的 reasoning
            if !reasoning.is_empty() {
                if !matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
                    self.messages
                        .push(AiMessage::new(AiRole::Assistant, String::new()));
                }
                if let Some(last) = self.messages.last_mut() {
                    last.start_reasoning_timer();
                    last.reasoning
                        .get_or_insert_with(String::new)
                        .push_str(&reasoning);
                }
                // 不强制吸附底部：尊重用户手动滚动选择
            }
            if !partial.is_empty() {
                // 不强制吸附底部：尊重用户手动滚动选择
                if !matches!(self.messages.last(), Some(m) if m.role == AiRole::Assistant) {
                    self.messages
                        .push(AiMessage::new(AiRole::Assistant, String::new()));
                }
                if let Some(last) = self.messages.last_mut() {
                    // 回答开始到达 = 思考阶段结束，结算思考耗时
                    last.stop_reasoning_timer();
                    last.content.push_str(&partial);
                }
            }
            if let Some(err) = error {
                // 出错中断：结算已进行的思考耗时，避免显示永远的"思考中"
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                }
                self.add_assistant_message(err);
                self.is_generating = false;
                // 同步热数据（消息已最终确定），并上报中断边沿供抢救文件块
                self.sync_hot_data();
                return DrainEdge::Interrupted;
            }
            if done {
                self.is_generating = false;
                // 生成完成：恢复思考块折叠（默认即折叠，用户流式中手动展开时在此收起）
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                    // 修复：当 content 为空但 reasoning 非空时（DeepSeek 思考模式简单问答场景），
                    // 将 reasoning 内容作为正常回答显示，而非放在"深度思考"区域
                    if last.role == AiRole::Assistant
                        && last.content.trim().is_empty()
                        && last
                            .reasoning
                            .as_ref()
                            .is_some_and(|r| !r.trim().is_empty())
                    {
                        last.content = last.reasoning.take().unwrap_or_default();
                        last.reasoning = None;
                        last.reasoning_collapsed = false;
                        last.reasoning_ms = None;
                        last.reasoning_started_ms = None;
                    } else if last.role == AiRole::Assistant && last.reasoning.is_some() {
                        last.reasoning_collapsed = true;
                    }
                }
                if truncated.is_some() {
                    // 被截断：显示消息并设置标记，UI 会渲染"继续生成"按钮
                    self.add_assistant_message(format!(
                        "[警告] 输出已被截断（原因：{}）。点击下方按钮继续生成。",
                        truncated.unwrap_or_else(|| "max_tokens".to_string())
                    ));
                    self.last_truncated = true;
                    edge = DrainEdge::Truncated;
                } else {
                    edge = DrainEdge::Completed;
                    self.last_truncated = false;
                }
                // 同步热数据（生成完成，消息已最终确定）
                self.sync_hot_data();
            }
        }
        edge
    }

    /// 继续被截断的输出：把当前最后一条助手消息作为上下文，重新发起请求"继续生成"
    pub fn continue_truncated_generation(&mut self, settings: &AiSettings) -> Result<(), String> {
        if self.is_generating {
            return Err("正在生成中，无法继续".to_string());
        }
        // 移除最后一条"被截断"提示消息（如果有）
        if let Some(last) = self.messages.last() {
            if last.role == AiRole::Assistant && last.content.contains("输出已被截断") {
                self.messages.pop();
            }
        }
        // 重置截断标记，发送明确的"续写"指令（而非裸"继续"），避免模型在 Agent 模式下
        // 重新规划/列目录/读文件，而是直接接着上次断点补全。
        self.last_truncated = false;
        let resume_prompt = "请从上次被截断的位置继续输出剩余内容：直接接着写，\
不要重复已经输出的部分，不要重新开始或重新解释，也不要重新列目录/读取文件。\
如果上次正在写某个文件，请继续用同一个 FILE 标记补全剩余内容，直到文件完整。";
        self.send_message_internal(settings, resume_prompt.to_string(), self.mode, None)?;
        Ok(())
    }

    /// 从最后一条助手消息中提取代码块
    pub fn extract_last_code_block(&self) -> Option<String> {
        for msg in self.messages.iter().rev() {
            if msg.role == AiRole::Assistant {
                return Self::extract_code_blocks(&msg.content);
            }
        }
        None
    }

    /// 提取所有代码块（```...``` 之间的内容）
    fn extract_code_blocks(text: &str) -> Option<String> {
        let mut result = String::new();
        let mut in_code = false;
        let mut code_content = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_code {
                    if !code_content.is_empty() {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str(&code_content);
                    }
                    code_content.clear();
                    in_code = false;
                } else {
                    in_code = true;
                }
            } else if in_code {
                if !code_content.is_empty() {
                    code_content.push('\n');
                }
                code_content.push_str(line);
            }
        }

        // AI-L01: 未闭合代码围栏时，将累积内容也加入结果
        if in_code && !code_content.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&code_content);
        }

        if !result.is_empty() {
            Some(result)
        } else {
            None
        }
    }

    /// 从代码围栏行提取建议的文件名
    /// 例如 ```python:main.py 或 ```rust src/main.rs
    pub fn extract_filename_from_fence(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with("```") {
            return None;
        }
        let after_fence = trimmed.strip_prefix("```")?.trim();
        // 检查是否包含冒号或空格分隔的文件名
        if let Some(colon_pos) = after_fence.find(':') {
            let filename = after_fence[colon_pos + 1..].trim();
            if !filename.is_empty() && !filename.contains(' ') {
                return Some(filename.to_string());
            }
        }
        // 检查格式：语言 文件名（如 "python main.py"）
        let parts: Vec<&str> = after_fence.split_whitespace().collect();
        if parts.len() >= 2 {
            // 第二部分看起来像文件名（包含 . 或 /）
            let candidate = parts[1];
            if candidate.contains('.') || candidate.contains('/') || candidate.contains("\\") {
                return Some(candidate.to_string());
            }
        }
        None
    }

    /// 获取最后一条助手消息的纯文本（去掉代码块标记）
    pub fn last_assistant_text(&self) -> Option<String> {
        for msg in self.messages.iter().rev() {
            if msg.role == AiRole::Assistant {
                return Some(msg.content.clone());
            }
        }
        None
    }

    /// 切换附件：已存在则移除，否则添加
    pub fn toggle_attachment(&mut self, attachment: AiContextAttachment) {
        let pos = self
            .attachments
            .iter()
            .position(|a| match (a, &attachment) {
                (AiContextAttachment::CurrentFile, AiContextAttachment::CurrentFile) => true,
                (AiContextAttachment::Selection, AiContextAttachment::Selection) => true,
                (AiContextAttachment::OpenFiles, AiContextAttachment::OpenFiles) => true,
                (AiContextAttachment::Diagnostics, AiContextAttachment::Diagnostics) => true,
                (AiContextAttachment::FileTree, AiContextAttachment::FileTree) => true,
                (AiContextAttachment::CustomText(x), AiContextAttachment::CustomText(y)) => x == y,
                _ => false,
            });
        if let Some(idx) = pos {
            self.attachments.remove(idx);
        } else {
            self.attachments.push(attachment);
        }
    }

    /// 清除所有上下文附件
    pub fn clear_attachments(&mut self) {
        self.attachments.clear();
    }

    /// 可通过工具栏切换的 5 种上下文附件（不含 CustomText）
    pub fn toggleable_attachments() -> [AiContextAttachment; 5] {
        [
            AiContextAttachment::CurrentFile,
            AiContextAttachment::Selection,
            AiContextAttachment::OpenFiles,
            AiContextAttachment::Diagnostics,
            AiContextAttachment::FileTree,
        ]
    }

    /// 判断某类附件是否已附加（按变体判断，忽略 CustomText 内部内容）
    pub fn has_attachment(&self, att: &AiContextAttachment) -> bool {
        self.attachments
            .iter()
            .any(|a| std::mem::discriminant(a) == std::mem::discriminant(att))
    }

    /// 当前已附加的上下文文本摘要（用于 UI 展示）
    pub fn attachment_summary(&self) -> String {
        if self.attachments.is_empty() {
            return String::new();
        }
        let labels: Vec<String> = self.attachments.iter().map(|a| a.short_label()).collect();
        format!("上下文: {}", labels.join(" "))
    }

    /// 限制并格式化自定义文本附件
    pub fn prepare_custom_text(text: &str) -> AiContextAttachment {
        AiContextAttachment::CustomText(truncate_middle(text, 2000))
    }

    /// 命中测试：模式切换按钮
    pub fn hit_test_mode_button(&self, px: f32, py: f32) -> Option<AiMode> {
        for (mode, x, y, w, h) in &self.mode_button_regions {
            if px >= *x && px <= *x + *w && py >= *y && py <= *y + *h {
                return Some(*mode);
            }
        }
        None
    }

    /// 命中测试：附件 chip（返回索引）
    pub fn hit_test_attachment(&self, px: f32, py: f32) -> Option<usize> {
        for (idx, x, y, w, h) in &self.attachment_chip_regions {
            if px >= *x && px <= *x + *w && py >= *y && py <= *y + *h {
                return Some(*idx);
            }
        }
        None
    }

    /// 清除所有命中区域（每帧渲染前调用）
    pub fn clear_hit_regions(&mut self) {
        self.mode_button_regions.clear();
        self.attachment_chip_regions.clear();
        self.code_save_regions.clear();
        self.tab_regions.clear();
        self.tab_close_regions.clear();
        self.new_tab_region = None;
        self.history_button_region = None;
        self.history_item_regions.clear();
        self.reasoning_toggle_regions.clear();
        self.playbook_button_region = None;
        self.playbook_delete_regions.clear();
        self.history_delete_regions.clear();
        self.history_page_prev_region = None;
        self.history_page_next_region = None;
        self.history_time_filter_regions.clear();
        self.history_clear_all_region = None;
        self.history_win_region = None;
        self.history_win_titlebar_region = None;
        self.history_win_close_region = None;
        self.history_search_region = None;
        self.browse_folder_region = None;
        self.retry_button_region = None;
    }

    /// 历史下拉面板动画步进：向目标状态（展开 1.0 / 收起 0.0）推进。
    /// 返回 true 表示动画尚未结束（调用方应继续重绘并保留定时器）。
    pub fn tick_history_anim(&mut self) -> bool {
        const STEP: f32 = 0.14; // 16ms 定时器下约 7 帧完成
        let target = if self.history_open { 1.0 } else { 0.0 };
        if (self.history_anim - target).abs() < f32::EPSILON {
            return false;
        }
        if self.history_anim < target {
            self.history_anim = (self.history_anim + STEP).min(target);
        } else {
            self.history_anim = (self.history_anim - STEP).max(target);
        }
        (self.history_anim - target).abs() > f32::EPSILON
    }

    /// 立即关闭历史浮窗（跳过收起动画，用于标签切换/关闭/恢复会话等场景）。
    /// 委托给 close_history_window，彻底清理拖动/搜索焦点/编辑态。
    pub fn dismiss_history_dropdown(&mut self) {
        self.close_history_window();
    }

    // ===== Playbook 管理面板 =====

    /// 切换 Playbook 管理面板展开/收起（展开时从 AetherDB 加载条目）
    pub fn toggle_playbook_panel(&mut self) {
        self.playbook_open = !self.playbook_open;
        if self.playbook_open {
            self.reload_playbook();
        }
    }

    /// 重新加载 Playbook 条目缓存
    pub fn reload_playbook(&mut self) {
        if let Some(warm) = self.warm_data_store.as_ref() {
            self.playbook_items = warm.list_playbook(None).unwrap_or_default();
        }
    }

    /// 删除指定下标的 Playbook 条目（调用方需先弹确认）
    pub fn delete_playbook_item(&mut self, idx: usize) -> Result<(), String> {
        let id = self
            .playbook_items
            .get(idx)
            .map(|b| b.id.clone())
            .ok_or_else(|| "条目不存在".to_string())?;
        if let Some(warm) = self.warm_data_store.as_ref() {
            warm.delete_bullet(&id)?;
        }
        self.reload_playbook();
        Ok(())
    }

    /// 切换历史列表的工作区过滤
    pub fn toggle_history_workspace_only(&mut self) {
        self.history_workspace_only = !self.history_workspace_only;
        self.history_page = 0;
    }

    // ===== 历史记录：筛选 / 分页 / 详情 / 清除 =====

    /// 应用时间 + 类型 + 搜索关键词筛选后的历史下标（对应 self.history 的原始下标）
    pub fn filtered_history_indices(&self) -> Vec<usize> {
        let cutoff = self.history_time_filter.cutoff(now_secs());
        let kw = self.history_search.trim().to_lowercase();
        self.history
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                let time_ok = cutoff.map(|c| m.updated_at >= c).unwrap_or(true);
                let type_ok = self
                    .history_type_filter
                    .as_ref()
                    .map(|t| m.mode.eq_ignore_ascii_case(t))
                    .unwrap_or(true);
                let kw_ok = kw.is_empty() || m.title.to_lowercase().contains(&kw);
                time_ok && type_ok && kw_ok
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// 筛选后的总页数（至少 1 页）
    pub fn history_page_count(&self) -> usize {
        let n = self.filtered_history_indices().len();
        (n + HISTORY_PAGE_SIZE - 1) / HISTORY_PAGE_SIZE.max(1)
    }

    /// 把当前页码收敛到合法范围（筛选/删除/刷新后调用）
    pub fn clamp_history_page(&mut self) {
        let pc = self.history_page_count().max(1);
        if self.history_page >= pc {
            self.history_page = pc - 1;
        }
    }

    /// 当前页显示的历史下标
    pub fn history_page_indices(&self) -> Vec<usize> {
        let start = self.history_page * HISTORY_PAGE_SIZE;
        self.filtered_history_indices()
            .into_iter()
            .skip(start)
            .take(HISTORY_PAGE_SIZE)
            .collect()
    }

    /// 设置时间筛选（回到第一页；取消进行中的标题编辑，避免编辑条目被过滤后状态卡住）
    pub fn set_history_time_filter(&mut self, f: HistoryTimeFilter) {
        self.history_time_filter = f;
        self.history_page = 0;
        if self.history_editing_id.is_some() {
            self.cancel_history_edit();
        }
    }

    /// 设置类型筛选（回到第一页）
    pub fn set_history_type_filter(&mut self, f: Option<String>) {
        self.history_type_filter = f;
        self.history_page = 0;
    }

    // ===== 历史浮窗：搜索 / 标题编辑 / 拖动 =====

    /// 打开历史浮窗（居中默认位置，重置搜索与编辑态）
    pub fn open_history_window(&mut self) {
        self.history_open = true;
        self.history_anim = 1.0;
        self.history_scroll = 0.0;
        self.history_win_drag = None;
        self.history_editing_id = None;
        self.history_last_click = None;
        // 保留搜索词与位置，用户再次打开时延续上次状态
    }

    /// 关闭历史浮窗（清理拖动/搜索焦点/编辑态）
    pub fn close_history_window(&mut self) {
        self.history_open = false;
        self.history_anim = 0.0;
        self.history_win_drag = None;
        self.history_search_focused = false;
        self.history_editing_id = None;
        self.history_last_click = None;
        self.close_history_detail();
    }

    /// 搜索框输入一个字符（在光标处插入）
    pub fn history_search_input_char(&mut self, ch: char) {
        if self.history_search_caret > self.history_search.len() {
            self.history_search_caret = self.history_search.len();
        }
        self.history_search.insert(self.history_search_caret, ch);
        self.history_search_caret += ch.len_utf8();
        self.history_page = 0;
    }

    /// 搜索框退格（删除光标前一个字符）
    pub fn history_search_backspace(&mut self) {
        if self.history_search_caret == 0 {
            return;
        }
        // 找到前一个字符边界
        let prev = self.history_search[..self.history_search_caret]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.history_search.remove(prev);
        self.history_search_caret = prev;
        self.history_page = 0;
    }

    /// 搜索框光标左移
    pub fn history_search_move_left(&mut self) {
        if self.history_search_caret > 0 {
            self.history_search_caret = self.history_search[..self.history_search_caret]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// 搜索框光标右移
    pub fn history_search_move_right(&mut self) {
        if self.history_search_caret < self.history_search.len() {
            self.history_search_caret = self.history_search[self.history_search_caret..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.history_search_caret + i)
                .unwrap_or(self.history_search.len());
        }
    }

    /// 进入标题编辑态（双击条目触发）
    pub fn begin_history_edit(&mut self, hist_idx: usize) {
        if let Some(m) = self.history.get(hist_idx) {
            self.history_editing_id = Some(m.id.clone());
            self.history_editing_text = m.title.clone();
            self.history_editing_caret = self.history_editing_text.len();
            self.history_search_focused = false;
        }
    }

    /// 取消标题编辑（Esc）
    pub fn cancel_history_edit(&mut self) {
        self.history_editing_id = None;
        self.history_editing_text.clear();
        self.history_editing_caret = 0;
    }

    /// 编辑态输入一个字符
    pub fn history_edit_input_char(&mut self, ch: char) {
        if self.history_editing_caret > self.history_editing_text.len() {
            self.history_editing_caret = self.history_editing_text.len();
        }
        self.history_editing_text
            .insert(self.history_editing_caret, ch);
        self.history_editing_caret += ch.len_utf8();
    }

    /// 编辑态退格
    pub fn history_edit_backspace(&mut self) {
        if self.history_editing_caret == 0 {
            return;
        }
        let prev = self.history_editing_text[..self.history_editing_caret]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.history_editing_text.remove(prev);
        self.history_editing_caret = prev;
    }

    /// 提交标题编辑（回车）：持久化到 AetherDB 并同步内存
    pub fn commit_history_edit(&mut self) -> Result<(), String> {
        let id = match self.history_editing_id.take() {
            Some(id) => id,
            None => return Ok(()),
        };
        let new_title = self.history_editing_text.trim().to_string();
        self.history_editing_text.clear();
        self.history_editing_caret = 0;
        if new_title.is_empty() {
            return Err("标题不能为空".to_string());
        }
        // 1. 持久化
        if let Some(warm) = self.warm_data_store.as_ref() {
            warm.rename_conversation(&id, &new_title)?;
        }
        // 2. 同步内存 history
        if let Some(m) = self.history.iter_mut().find(|m| m.id == id) {
            m.title = new_title.clone();
        }
        // 3. 同步 conversations 中的活动标签
        if let Some(c) = self.conversations.iter_mut().find(|c| c.id == id) {
            c.title = new_title;
        }
        Ok(())
    }

    /// 双击检测：判断本次点击是否构成对同一条目的双击（<500ms）
    /// 返回 true 表示是双击，调用方应进入编辑态
    pub fn history_click_or_double(&mut self, conv_id: &str) -> bool {
        const DOUBLE_CLICK_MS: u128 = 500;
        let now = std::time::Instant::now();
        let is_double = match &self.history_last_click {
            Some((last_id, last_t))
                if last_id == conv_id
                    && now.duration_since(*last_t).as_millis() < DOUBLE_CLICK_MS =>
            {
                true
            }
            _ => false,
        };
        self.history_last_click = if is_double {
            None // 双击后重置，避免三击误判
        } else {
            Some((conv_id.to_string(), now))
        };
        is_double
    }

    /// 下一页（到末页为止）
    pub fn history_next_page(&mut self) {
        if self.history_page + 1 < self.history_page_count() {
            self.history_page += 1;
        }
    }

    /// 上一页
    pub fn history_prev_page(&mut self) {
        self.history_page = self.history_page.saturating_sub(1);
    }

    /// 删除一条历史记录（内存索引 + AetherDB 级联删除）
    pub fn delete_history_item(&mut self, hist_idx: usize) -> Result<(), String> {
        let meta = self
            .history
            .get(hist_idx)
            .cloned()
            .ok_or_else(|| "历史记录不存在".to_string())?;
        if let Some(warm) = self.warm_data_store.as_ref() {
            warm.delete_conversation(&meta.id)?;
        }
        self.history.remove(hist_idx);
        // 取消同 id 标签的在途休眠握手：DB 记录已删，若回执到达后仍卸载消息体，
        // 下次唤醒会因读不到记录退化为欢迎语占位，导致对话内容丢失
        for conv in self.conversations.iter_mut() {
            if conv.id == meta.id {
                conv.hibernate_pending_at = None;
            }
        }
        // 清理被删条目的残留编辑态，避免提交时报"会话不存在"
        if self.history_editing_id.as_deref() == Some(meta.id.as_str()) {
            self.cancel_history_edit();
        }
        if self.history_detail_id.as_deref() == Some(meta.id.as_str()) {
            self.close_history_detail();
        }
        self.clamp_history_page();
        Ok(())
    }

    /// 清空全部历史记录；返回删除条数
    pub fn clear_all_history(&mut self) -> Result<usize, String> {
        let n = if let Some(warm) = self.warm_data_store.as_ref() {
            warm.clear_all_conversations()?
        } else {
            self.history.len()
        };
        self.history.clear();
        self.history_page = 0;
        self.close_history_detail();
        self.cancel_history_edit();
        // 清空后所有在途休眠握手均失去落点：清 pending，防卸载后唤醒读不到记录丢消息
        for conv in self.conversations.iter_mut() {
            conv.hibernate_pending_at = None;
        }
        // 重置筛选状态，避免清空后筛选条件残留导致困惑
        self.history_time_filter = HistoryTimeFilter::All;
        self.history_type_filter = None;
        Ok(n)
    }

    /// 清理无工作区绑定的历史记录（旧版本未隔离工作区的数据）；返回删除条数
    pub fn clear_orphan_history(&mut self) -> Result<usize, String> {
        let n = if let Some(warm) = self.warm_data_store.as_ref() {
            warm.clear_orphan_conversations()?
        } else {
            // 无温数据存储时，从内存 history 中移除无 workspace_hash 的条目
            let before = self.history.len();
            self.history.retain(|m| !m.id.is_empty());
            before - self.history.len()
        };
        // 同步刷新内存中的历史列表（移除已删除的条目）
        if let Some(warm) = self.warm_data_store.as_ref() {
            let ws_only = self.history_workspace_only;
            if let Ok(convs) = warm.search_conversations("", ws_only, 500) {
                self.history = convs
                    .into_iter()
                    .map(|c| ConversationMeta {
                        id: c.id,
                        title: c.title,
                        updated_at: c.updated_at,
                        message_count: c.message_count as usize,
                        preview: String::new(),
                        mode: c.mode,
                    })
                    .collect();
            }
        }
        self.clamp_history_page();
        Ok(n)
    }

    /// 打开历史详情视图（懒加载完整会话消息）
    pub fn open_history_detail(&mut self, hist_idx: usize) {
        let Some(meta) = self.history.get(hist_idx) else {
            return;
        };
        let id = meta.id.clone();
        self.history_detail_conv = self
            .warm_data_store
            .as_ref()
            .and_then(|s| s.load_conversation(&id).ok());
        self.history_detail_id = Some(id);
    }

    /// 关闭历史详情视图，返回列表
    pub fn close_history_detail(&mut self) {
        self.history_detail_id = None;
        self.history_detail_conv = None;
    }

    /// 从详情视图恢复该会话为活动标签页
    pub fn restore_history_detail(&mut self) {
        if let Some(id) = self.history_detail_id.clone() {
            self.close_history_detail();
            if let Some(idx) = self.history.iter().position(|m| m.id == id) {
                self.restore_from_history(idx);
            }
        }
    }
}

/// 解析段落内的轻量 Markdown：标题(`#`/`##`/`###`)、无序列表(`-`/`*`/`+`)、粗体(`**`)、行内代码(`` ` ``)。
///
/// 返回 `(清洗后的 UTF-16 文本, 粗体范围, 标题范围[start,len,字号], 行内代码范围)`，
/// 范围以 UTF-16 code unit 为单位，直接供 `IDWriteTextLayout` 的 range 样式使用。
#[allow(clippy::type_complexity)]
pub fn parse_markdown_segment(
    text: &str,
) -> (
    Vec<u16>,
    Vec<(u32, u32)>,
    Vec<(u32, u32, f32)>,
    Vec<(u32, u32)>,
) {
    let mut clean: Vec<u16> = Vec::new();
    let mut bolds: Vec<(u32, u32)> = Vec::new();
    let mut headings: Vec<(u32, u32, f32)> = Vec::new();
    let mut codes: Vec<(u32, u32)> = Vec::new();

    for (li, line) in text.lines().enumerate() {
        if li > 0 {
            clean.push(b'\n' as u16);
        }
        let line_start = clean.len() as u32;

        // 行首标题标记
        let trimmed = line.trim_start();
        let (mut content, heading_size): (&str, Option<f32>) =
            if let Some(rest) = trimmed.strip_prefix("### ") {
                (rest, Some(13.5))
            } else if let Some(rest) = trimmed.strip_prefix("## ") {
                (rest, Some(15.0))
            } else if let Some(rest) = trimmed.strip_prefix("# ") {
                (rest, Some(17.0))
            } else {
                (line, None)
            };

        // 行首无序列表标记（非标题时），替换为圆点
        if heading_size.is_none() {
            let t = content.trim_start();
            if let Some(rest) = t
                .strip_prefix("- ")
                .or_else(|| t.strip_prefix("* "))
                .or_else(|| t.strip_prefix("+ "))
            {
                clean.push(0x2022); // •
                clean.push(b' ' as u16);
                content = rest;
            }
        }

        // 行内粗体 **text** 与行内代码 `code`
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            // 行内代码：`...`（去掉反引号，记录范围供链接化/样式使用）
            if chars[i] == '`' {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    let end = i + 1 + end;
                    if end > i + 1 {
                        let c_start = clean.len() as u32;
                        for &c in &chars[i + 1..end] {
                            push_utf16(&mut clean, c);
                        }
                        let c_len = clean.len() as u32 - c_start;
                        if c_len > 0 {
                            codes.push((c_start, c_len));
                        }
                    }
                    i = end + 1;
                    continue;
                }
            }
            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                if let Some(end) = find_double_star(&chars, i + 2) {
                    let b_start = clean.len() as u32;
                    for &c in &chars[i + 2..end] {
                        push_utf16(&mut clean, c);
                    }
                    let b_len = clean.len() as u32 - b_start;
                    if b_len > 0 {
                        bolds.push((b_start, b_len));
                    }
                    i = end + 2;
                    continue;
                }
            }
            push_utf16(&mut clean, chars[i]);
            i += 1;
        }

        if let Some(size) = heading_size {
            let line_len = clean.len() as u32 - line_start;
            if line_len > 0 {
                headings.push((line_start, line_len, size));
            }
        }
    }

    (clean, bolds, headings, codes)
}

/// 判断行内代码 span 是否为「类文件路径」token（供对话内文件名链接化）。
///
/// 规则：无空白、含 `.`、至少一个字母、不含 `://`、扩展名为 1~8 位字母数字。
pub fn is_path_like_token(s: &str) -> bool {
    if s.len() < 3 || s.len() > 260 {
        return false;
    }
    if s.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if s.contains("://") {
        return false;
    }
    if !s.contains('.') || !s.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    let ext = s.rsplit('.').next().unwrap_or("");
    !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_alphanumeric())
}

fn push_utf16(buf: &mut Vec<u16>, c: char) {
    let mut tmp = [0u16; 2];
    for u in c.encode_utf16(&mut tmp) {
        buf.push(*u);
    }
}

fn find_double_star(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: AiRole, content: &str) -> AiMessage {
        AiMessage::new(role, content.to_string())
    }

    #[test]
    fn history_keeps_order_and_maps_roles() {
        let history = vec![
            msg(AiRole::System, "欢迎语（应被跳过）"),
            msg(AiRole::User, "你好"),
            msg(AiRole::Assistant, "你好！我是助手"),
            msg(AiRole::User, "我刚刚问了什么"),
        ];
        let out = AiPanel::history_to_chat_messages(&history, 24000);
        // System 欢迎语被跳过，其余按序映射
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content, "你好");
        assert_eq!(out[1].role, "assistant");
        assert_eq!(out[2].role, "user");
        assert_eq!(out[2].content, "我刚刚问了什么");
    }

    #[test]
    fn history_window_always_includes_last_even_if_huge() {
        // 单条超预算也必须包含（保证当前输入不被丢弃）
        let big = "字".repeat(30_000);
        let history = vec![msg(AiRole::User, &big)];
        let out = AiPanel::history_to_chat_messages(&history, 24000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn history_window_drops_oldest_when_over_budget() {
        // 构造多条大消息（每条约 3000 字符 × 20 对，远超 24000 预算），
        // 超出 token 预算时应丢弃较早的，保留最近的。
        let mut history = Vec::new();
        for i in 0..20 {
            history.push(msg(AiRole::User, &"x".repeat(3000)));
            history.push(msg(AiRole::Assistant, &format!("回复{}", i)));
        }
        let out = AiPanel::history_to_chat_messages(&history, 24000);
        assert!(!out.is_empty());
        // 不超过消息数上限 MAX_MSGS；超预算应丢弃最早的消息、保留最近的
        assert!(out.len() <= 40);
        assert!(
            !out.iter().any(|m| m.content == "回复0"),
            "最早的消息应被丢弃"
        );
        assert_eq!(out.last().unwrap().content, "回复19", "应保留最近的消息");
    }

    #[test]
    fn empty_history_yields_empty() {
        let out = AiPanel::history_to_chat_messages(&[], 24000);
        assert!(out.is_empty());
    }

    // ===== 历史记录列表：筛选 / 分页 / 详情 / 清除 =====

    fn test_panel() -> AiPanel {
        let mut p = AiPanel::new();
        // 测试不触碰真实持久化层
        p.warm_data_store = None;
        p.hot_data_store = None;
        p
    }

    fn meta(id: &str, updated_at: u64, mode: &str) -> ConversationMeta {
        ConversationMeta {
            id: id.to_string(),
            title: format!("会话{}", id),
            updated_at,
            message_count: 3,
            preview: String::new(),
            mode: mode.to_string(),
        }
    }

    #[test]
    fn history_time_filter_keeps_recent_only() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![
            meta("a", now, "Ask"),
            meta("b", now.saturating_sub(3 * 86400), "Agent"),
            meta("c", now.saturating_sub(40 * 86400), "Ask"),
        ];
        p.set_history_time_filter(HistoryTimeFilter::Today);
        assert_eq!(p.filtered_history_indices(), vec![0]);
        p.set_history_time_filter(HistoryTimeFilter::Week);
        assert_eq!(p.filtered_history_indices(), vec![0, 1]);
        p.set_history_time_filter(HistoryTimeFilter::All);
        assert_eq!(p.filtered_history_indices(), vec![0, 1, 2]);
    }

    #[test]
    fn history_type_filter_matches_mode_case_insensitive() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask"), meta("b", now, "Agent")];
        p.set_history_type_filter(Some("ask".to_string()));
        assert_eq!(p.filtered_history_indices(), vec![0]);
        p.set_history_type_filter(Some("Agent".to_string()));
        assert_eq!(p.filtered_history_indices(), vec![1]);
        p.set_history_type_filter(None);
        assert_eq!(p.filtered_history_indices(), vec![0, 1]);
    }

    #[test]
    fn history_filter_resets_page() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = (0..13)
            .map(|i| meta(&format!("c{}", i), now, "Ask"))
            .collect();
        p.history_page = 2;
        p.set_history_time_filter(HistoryTimeFilter::Week);
        assert_eq!(p.history_page, 0);
        p.history_page = 1;
        p.set_history_type_filter(Some("Agent".to_string()));
        assert_eq!(p.history_page, 0);
    }

    #[test]
    fn history_pagination_pages_and_bounds() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = (0..13)
            .map(|i| meta(&format!("c{}", i), now, "Ask"))
            .collect();
        assert_eq!(p.history_page_count(), 3); // 6 + 6 + 1
        assert_eq!(p.history_page_indices().len(), HISTORY_PAGE_SIZE);
        p.history_next_page();
        assert_eq!(p.history_page, 1);
        p.history_next_page();
        assert_eq!(p.history_page, 2);
        assert_eq!(p.history_page_indices().len(), 1);
        // 末页后继续 next 不变
        p.history_next_page();
        assert_eq!(p.history_page, 2);
        p.history_prev_page();
        assert_eq!(p.history_page, 1);
        // 首页 prev 保持 0
        p.history_prev_page();
        p.history_prev_page();
        assert_eq!(p.history_page, 0);
        // 筛选后页数收缩时 clamp 收敛页码
        p.history_page = 2;
        p.clamp_history_page();
        assert_eq!(p.history_page, 2);
        p.set_history_time_filter(HistoryTimeFilter::Today);
        p.clamp_history_page();
        assert_eq!(p.history_page, 0);
    }

    #[test]
    fn delete_history_item_removes_entry_and_closes_detail() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask"), meta("b", now, "Agent")];
        p.open_history_detail(0);
        assert_eq!(p.history_detail_id.as_deref(), Some("a"));
        p.delete_history_item(0).unwrap();
        assert_eq!(p.history.len(), 1);
        assert_eq!(p.history[0].id, "b");
        // 详情指向被删会话时自动关闭
        assert!(p.history_detail_id.is_none());
        // 越界删除报错
        assert!(p.delete_history_item(5).is_err());
    }

    #[test]
    fn delete_history_item_cancels_pending_hibernate_and_editing() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask")];
        // 打开标签中存在同 id 会话且休眠握手在途
        let mut c = archivable_conv("a");
        c.hibernate_pending_at = Some(c.updated_at);
        p.conversations.push(c);
        // 正在编辑即将被删除的条目
        p.begin_history_edit(0);
        assert_eq!(p.history_editing_id.as_deref(), Some("a"));

        p.delete_history_item(0).unwrap();

        // 同 id 标签的休眠握手必须取消：否则回执到达后卸载消息体，
        // 唤醒时 DB 记录已删，退化为欢迎语占位导致对话丢失
        assert!(p.conversations[1].hibernate_pending_at.is_none());
        // 编辑态同步清理
        assert!(p.history_editing_id.is_none());
    }

    #[test]
    fn clear_all_history_cancels_all_pending_hibernations() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask"), meta("b", now, "Agent")];
        for id in ["a", "b"] {
            let mut c = archivable_conv(id);
            c.hibernate_pending_at = Some(c.updated_at);
            p.conversations.push(c);
        }
        p.begin_history_edit(0);

        p.clear_all_history().unwrap();

        assert!(p.history.is_empty());
        assert!(p.history_editing_id.is_none());
        assert!(
            p.conversations
                .iter()
                .all(|c| c.hibernate_pending_at.is_none()),
            "清空后不得残留任何在途休眠握手"
        );
    }

    #[test]
    fn clear_all_history_empties_list_and_resets_page() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = (0..8)
            .map(|i| meta(&format!("c{}", i), now, "Ask"))
            .collect();
        p.history_page = 1;
        p.history_time_filter = HistoryTimeFilter::Week;
        p.history_type_filter = Some("Agent".to_string());
        p.open_history_detail(0);
        let n = p.clear_all_history().unwrap();
        assert_eq!(n, 8);
        assert!(p.history.is_empty());
        assert_eq!(p.history_page, 0);
        assert!(p.history_detail_id.is_none());
        // 筛选状态应重置为默认值
        assert_eq!(p.history_time_filter, HistoryTimeFilter::All);
        assert!(p.history_type_filter.is_none());
    }

    #[test]
    fn history_detail_open_and_restore() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask")];
        // 无 warm store：详情打开但消息为空（会话仍在 conversations 中可直接恢复）
        p.open_history_detail(0);
        assert_eq!(p.history_detail_id.as_deref(), Some("a"));
        assert!(p.history_detail_conv.is_none());
        p.close_history_detail();
        assert!(p.history_detail_id.is_none());
        // 越界打开为 no-op
        p.open_history_detail(9);
        assert!(p.history_detail_id.is_none());
    }

    #[test]
    fn relative_time_buckets() {
        let now = 1_000_000_000u64;
        assert_eq!(relative_time(now, now), "刚刚");
        assert_eq!(relative_time(now - 120, now), "2 分钟前");
        assert_eq!(relative_time(now - 7200, now), "2 小时前");
        assert_eq!(relative_time(now - 2 * 86400, now), "2 天前");
        assert_eq!(relative_time(now - 14 * 86400, now), "2 周前");
        assert_eq!(relative_time(now - 60 * 86400, now), "2 个月前");
        // 未来时间戳不回绕
        assert_eq!(relative_time(now + 100, now), "刚刚");
    }

    // ===== 历史浮窗：搜索过滤 / 标题编辑 / 双击检测 =====

    #[test]
    fn history_search_filters_by_title_case_insensitive() {
        let now = now_secs();
        let mut p = test_panel();
        let mut m1 = meta("a", now, "Ask");
        m1.title = "Rust 生命周期问题".to_string();
        let mut m2 = meta("b", now, "Ask");
        m2.title = "Python 装饰器".to_string();
        let mut m3 = meta("c", now, "Ask");
        m3.title = "RUST 所有权".to_string();
        p.history = vec![m1, m2, m3];
        // 空搜索：全部
        assert_eq!(p.filtered_history_indices(), vec![0, 1, 2]);
        // 关键词 "rust" 命中 0 和 2（大小写不敏感）
        p.history_search = "rust".to_string();
        assert_eq!(p.filtered_history_indices(), vec![0, 2]);
        // 无命中
        p.history_search = "java".to_string();
        assert!(p.filtered_history_indices().is_empty());
        // 清空恢复
        p.history_search.clear();
        assert_eq!(p.filtered_history_indices(), vec![0, 1, 2]);
    }

    #[test]
    fn history_search_input_and_backspace() {
        let mut p = test_panel();
        p.history_search_input_char('r');
        p.history_search_input_char('u');
        p.history_search_input_char('s');
        assert_eq!(p.history_search, "rus");
        assert_eq!(p.history_search_caret, 3);
        p.history_search_backspace();
        assert_eq!(p.history_search, "ru");
        assert_eq!(p.history_search_caret, 2);
        // 中文按字符删除
        p.history_search_input_char('生');
        assert_eq!(p.history_search, "ru生");
        p.history_search_backspace();
        assert_eq!(p.history_search, "ru");
    }

    #[test]
    fn history_search_caret_move_respects_char_boundary() {
        let mut p = test_panel();
        p.history_search = "a生b".to_string();
        p.history_search_caret = p.history_search.len();
        p.history_search_move_left(); // 移到 'b' 前
        assert_eq!(&p.history_search[p.history_search_caret..], "b");
        p.history_search_move_left(); // 移到 '生' 前
        assert_eq!(&p.history_search[p.history_search_caret..], "生b");
        p.history_search_move_right(); // 移到 'b' 前
        assert_eq!(&p.history_search[p.history_search_caret..], "b");
    }

    #[test]
    fn history_edit_commit_updates_memory_and_conversations() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask")];
        // 活动标签中有一个同 id 会话
        p.conversations
            .push(AiConversation::new("a".into(), "旧标题".into()));
        p.begin_history_edit(0);
        assert_eq!(p.history_editing_id.as_deref(), Some("a"));
        assert_eq!(p.history_editing_text, "会话a");
        // 清空并输入新标题
        p.history_editing_text = "新标题".to_string();
        p.history_editing_caret = p.history_editing_text.len();
        p.commit_history_edit().unwrap();
        assert_eq!(p.history[0].title, "新标题");
        assert_eq!(p.conversations[1].title, "新标题");
        assert!(p.history_editing_id.is_none());
    }

    #[test]
    fn history_edit_commit_rejects_empty_title() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask")];
        p.begin_history_edit(0);
        p.history_editing_text = "   ".to_string();
        assert!(p.commit_history_edit().is_err());
        // 原标题不变
        assert_eq!(p.history[0].title, "会话a");
    }

    #[test]
    fn history_edit_cancel_discards_changes() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = vec![meta("a", now, "Ask")];
        p.begin_history_edit(0);
        p.history_editing_text = "被丢弃".to_string();
        p.cancel_history_edit();
        assert!(p.history_editing_id.is_none());
        assert_eq!(p.history[0].title, "会话a");
    }

    #[test]
    fn history_double_click_detection() {
        let mut p = test_panel();
        // 第一次点击：不是双击
        assert!(!p.history_click_or_double("a"));
        // 紧接着同一条目：是双击
        assert!(p.history_click_or_double("a"));
        // 双击后重置，再点不是双击
        assert!(!p.history_click_or_double("a"));
        // 不同条目：不是双击
        assert!(!p.history_click_or_double("b"));
    }

    #[test]
    fn history_window_open_close_resets_state() {
        let mut p = test_panel();
        p.history_search_focused = true;
        p.history_editing_id = Some("x".to_string());
        p.history_win_drag = Some((1.0, 2.0));
        p.close_history_window();
        assert!(!p.history_open);
        assert!(!p.history_search_focused);
        assert!(p.history_editing_id.is_none());
        assert!(p.history_win_drag.is_none());
        p.open_history_window();
        assert!(p.history_open);
        assert!(p.history_editing_id.is_none());
    }

    // ===== 流式中断：drain_background 中断边沿（文件块抢救的前提）=====

    #[test]
    fn drain_background_error_yields_interrupted_and_keeps_partial() {
        let mut conv = AiConversation::new("c1".into(), "t".into());
        conv.is_generating = true;
        {
            let mut s = conv.stream_state.lock().unwrap();
            s.partial =
                "前言\n<<<<<<< AETHER_FILE index.html\n======= AETHER_SEP\n<html>".to_string();
            s.error = Some("请求失败: 连接断开".to_string());
        }
        let edge = conv.drain_background();
        assert_eq!(edge, DrainEdge::Interrupted);
        assert!(!conv.is_generating);
        // 部分内容已落入消息，错误提示追加在后（抢救时按谓词跳过它）
        let last = conv.messages.last().unwrap();
        assert!(last.content.contains("请求失败"));
        let content_msg = conv
            .messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::Assistant && m.content.contains("<<<<<<< AETHER_FILE"));
        assert!(content_msg.is_some(), "部分内容应保留在消息中供抢救");
    }

    #[test]
    fn drain_background_done_yields_completed() {
        let mut conv = AiConversation::new("c1".into(), "t".into());
        conv.is_generating = true;
        {
            let mut s = conv.stream_state.lock().unwrap();
            s.partial = "完成内容".to_string();
            s.done = true;
        }
        assert_eq!(conv.drain_background(), DrainEdge::Completed);
        assert!(!conv.is_generating);
    }

    #[test]
    fn drain_background_pending_when_idle() {
        let mut conv = AiConversation::new("c1".into(), "t".into());
        assert_eq!(conv.drain_background(), DrainEdge::Pending);
    }

    // ===== 标签休眠：两阶段卸载 / 唤醒 / 关闭补元数据 =====

    /// 造一个含用户消息的可归档会话
    fn archivable_conv(id: &str) -> AiConversation {
        let mut c = AiConversation::new(id.to_string(), format!("会话{}", id));
        c.messages.push(AiMessage::new(AiRole::User, "问题".into()));
        c.messages
            .push(AiMessage::new(AiRole::Assistant, "回答".into()));
        c
    }

    #[test]
    fn hibernate_finalize_two_phase_unloads_messages() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h1"));
        // 模拟已发起归档：pending 快照 = 当前 updated_at
        let at = p.conversations[1].updated_at;
        p.conversations[1].hibernate_pending_at = Some(at);
        let msg_count = p.conversations[1].messages.len();
        p.finalize_hibernation("h1");
        let c = &p.conversations[1];
        assert!(c.hibernated, "落库确认后应进入休眠态");
        assert!(c.messages.is_empty(), "消息体应卸载");
        assert_eq!(c.hibernated_msg_count, msg_count);
        assert_eq!(c.hibernate_pending_at, None);
    }

    #[test]
    fn hibernate_finalize_aborts_when_updated_after_snapshot() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h2"));
        // 快照早于当前 updated_at（归档后又有新消息）
        let at = p.conversations[1].updated_at;
        p.conversations[1].hibernate_pending_at = Some(at.saturating_sub(10));
        p.finalize_hibernation("h2");
        let c = &p.conversations[1];
        assert!(!c.hibernated, "快照过期不得卸载，否则丢新消息");
        assert!(!c.messages.is_empty());
        assert_eq!(c.hibernate_pending_at, None, "应清 pending 供下次重新发起");
    }

    #[test]
    fn hibernate_finalize_on_active_only_clears_pending() {
        let mut p = test_panel();
        // 活动标签（下标 0）收到回执：不卸载，只清 pending
        let at = p.conversations[0].updated_at;
        p.conversations[0].hibernate_pending_at = Some(at);
        let id = p.conversations[0].id.clone();
        p.finalize_hibernation(&id);
        assert!(!p.conversations[0].hibernated);
        assert_eq!(p.conversations[0].hibernate_pending_at, None);
    }

    #[test]
    fn request_hibernate_skips_generating_and_without_store() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h3"));
        p.conversations[1].is_generating = true;
        p.request_hibernate(1);
        assert_eq!(
            p.conversations[1].hibernate_pending_at, None,
            "生成中的会话不得发起休眠"
        );
        p.conversations[1].is_generating = false;
        p.request_hibernate(1);
        assert_eq!(
            p.conversations[1].hibernate_pending_at, None,
            "无温数据层时不得发起休眠（消息无处可存）"
        );
    }

    #[test]
    fn wake_without_store_falls_back_to_welcome() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h4"));
        p.conversations[1].enter_hibernation();
        p.switch_to(1);
        assert!(!p.conversations[1].hibernated, "切入必须退出休眠态");
        assert!(!p.messages.is_empty(), "水合失败应以欢迎语兜底，不得空白");
    }

    #[test]
    fn close_hibernated_conv_inserts_meta_without_messages() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h5"));
        let msg_count = p.conversations[1].messages.len();
        p.conversations[1].enter_hibernation();
        p.close_conversation(1);
        assert_eq!(p.conversations.len(), 1);
        let meta = p.history.first().expect("休眠标签关闭后应补历史元数据");
        assert_eq!(meta.id, "h5");
        assert_eq!(meta.message_count, msg_count, "应使用休眠前记录的消息数");
    }

    #[test]
    fn load_slot_takes_heavy_fields_out_of_slot() {
        let mut p = test_panel();
        p.conversations.push(archivable_conv("h6"));
        p.switch_to(1);
        // 挪移而非克隆：激活后槽位重字段应已清空（单份驻留）
        assert!(p.conversations[1].messages.is_empty());
        assert!(!p.messages.is_empty(), "扁平现场持有唯一副本");
        // 切回时 snapshot 回填，数据不丢
        p.switch_to(0);
        assert!(!p.conversations[1].messages.is_empty(), "切走应回填槽位");
    }

    #[test]
    fn matching_text_skips_trailing_error_message() {
        let mut p = test_panel();
        // 隔离：清空 new() 从磁盘恢复的真实会话消息，谓词断言只作用于本测试数据
        p.messages.clear();
        p.messages.push(AiMessage::new(
            AiRole::Assistant,
            "<<<<<<< AETHER_FILE a.txt".into(),
        ));
        p.messages.push(AiMessage::new(
            AiRole::Assistant,
            "请求失败: 连接断开".into(),
        ));
        let hit =
            p.last_assistant_text_matching_of(p.active, |t| t.contains("<<<<<<< AETHER_FILE"));
        assert_eq!(hit.as_deref(), Some("<<<<<<< AETHER_FILE a.txt"));
        // 谓词不匹配时返回 None
        assert!(p
            .last_assistant_text_matching_of(p.active, |t| t.contains("不存在"))
            .is_none());
    }
}
