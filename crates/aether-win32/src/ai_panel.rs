use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aether_ai::{AiClient, AiStreamEvent, ChatMessage};
use aether_shared::settings::AiSettings;

use crate::ai_context::{truncate_middle, AiContextAttachment};
use crate::ai_prompt::{build_chat_prompt, AiMode};
use crate::editor::EditorState;

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

/// AI 助手消息
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
    /// "深度思考"内容（DeepSeek reasoner 的 reasoning_content）；None 表示无思考。
    /// 与 content 分离存储，UI 上作为独立的"思考过程"分类展示。
    pub reasoning: Option<String>,
    /// 思考块是否折叠（默认展开，生成完成后自动折叠；用户可点击标题切换）
    #[serde(default)]
    pub reasoning_collapsed: bool,
    /// 思考耗时（毫秒）；None 表示无思考或仍在思考中（旧持久化数据也为 None）
    #[serde(default)]
    pub reasoning_ms: Option<u64>,
    /// 思考开始时刻（Unix 毫秒），仅运行期计时用，不持久化
    #[serde(skip)]
    pub reasoning_started_ms: Option<u64>,
}

impl AiMessage {
    pub fn new(role: AiRole, content: String) -> Self {
        Self {
            role,
            content,
            reasoning: None,
            reasoning_collapsed: false,
            reasoning_ms: None,
            reasoning_started_ms: None,
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
}

/// 流式响应的共享状态
#[derive(Clone, Debug, Default)]
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
    pub stream_state: Arc<Mutex<AiStreamState>>,
    pub should_stop: Arc<AtomicBool>,
    /// 本轮注入过的 playbook 条目 ID（用于反馈归因）
    pub used_bullet_ids: Vec<String>,
    /// 标签休眠：messages 已卸载（完整内容在 SQLite 温数据层），
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
                self.stick_to_bottom = true;
                self.updated_at = now_secs();
            }
            if !partial.is_empty() {
                self.stick_to_bottom = true;
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
                // 生成完成：自动折叠思考块，保持界面整洁
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                    if last.role == AiRole::Assistant && last.reasoning.is_some() {
                        last.reasoning_collapsed = true;
                    }
                }
                if truncated.is_some() {
                    // 被截断：提示用户，后台会话无"继续"按钮，只显示消息
                    self.add_assistant_message(format!(
                        "⚠️ 输出已被截断（原因：{}）。发送\"继续\"以继续生成。",
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
    /// 历史下拉面板整体命中区（渲染时注册，供滚轮路由）
    pub history_panel_region: Option<(f32, f32, f32, f32)>,
    /// 历史记录条目命中区 (history_index, x, y, w, h)
    pub history_item_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 历史索引（懒加载：仅元数据，点击时才读取完整会话）
    pub history: Vec<ConversationMeta>,
    /// 思考块折叠切换命中区 (msg_index, x, y, w, h)（作用于活动会话 messages 索引）
    pub reasoning_toggle_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 热数据持久化存储（三阶段架构：热/温）
    pub hot_data_store: Option<crate::ai_hot_data::HotDataStore>,
    /// 温数据持久化存储（MemoryStore：SQLite + sqlite-vec）
    pub warm_data_store: Option<crate::ai_warm_data::WarmDataStore>,
    /// 历史列表：仅显示当前工作区的会话
    pub history_workspace_only: bool,
    /// Playbook 管理面板是否展开
    pub playbook_open: bool,
    /// Playbook 面板条目缓存（展开时从 SQLite 加载）
    pub playbook_items: Vec<crate::memory_store::PlaybookBullet>,
    /// Playbook 标题栏按钮命中区 (x, y, w, h)
    pub playbook_button_region: Option<(f32, f32, f32, f32)>,
    /// Playbook 条目删除按钮命中区 (item_index, x, y, w, h)
    pub playbook_delete_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 历史面板「仅当前工作区」开关命中区 (x, y, w, h)
    pub history_ws_toggle_region: Option<(f32, f32, f32, f32)>,
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
    /// 历史类型筛选按钮命中区 (HISTORY_TYPE_FILTERS 下标, x, y, w, h)
    pub history_type_filter_regions: Vec<(usize, f32, f32, f32, f32)>,
    /// 「清空全部」按钮命中区
    pub history_clear_all_region: Option<(f32, f32, f32, f32)>,
    /// 详情视图「返回」按钮命中区
    pub history_detail_back_region: Option<(f32, f32, f32, f32)>,
    /// 详情视图「恢复此对话」按钮命中区
    pub history_detail_restore_region: Option<(f32, f32, f32, f32)>,
    /// Agent 自动续跑轮次计数（用户手动发消息时重置；防止工具回环无限迭代）
    pub agent_iter_count: u32,
    /// 最后一次生成是否被截断（达到 max_tokens）
    pub last_truncated: bool,
    /// "继续生成"按钮命中区 (x, y, w, h)
    pub continue_button_region: Option<(f32, f32, f32, f32)>,
    /// 当前多任务编排流水线（None 表示无）
    pub agent_pipeline: Option<AgentPipeline>,
    /// 已展开预览的文件卡片集合，key = (消息下标, 该消息内 File 卡序号)
    pub expanded_file_cards: std::collections::HashSet<(usize, usize)>,
    /// 文件卡片命中区域 (msg_idx, block_seq, x, y, w, h)，渲染每帧更新
    pub file_card_regions: Vec<(usize, usize, f32, f32, f32, f32)>,
    /// "浏览并选择文件夹"按钮命中区 (x, y, w, h)
    pub browse_folder_region: Option<(f32, f32, f32, f32)>,
    /// 底部工具栏"添加文件"按钮命中区 (x, y, w, h)，渲染每帧更新
    pub attach_file_button_region: Option<(f32, f32, f32, f32)>,
}

/// 在后台线程发起一次流式 AI 请求，把事件写入共享 stream_state。
/// `send_message_internal`（普通对话/规划器调度）与 `stream_focused`（逐任务 worker）共用。
fn spawn_ai_stream(
    settings: AiSettings,
    messages: Vec<ChatMessage>,
    stream_state: Arc<Mutex<AiStreamState>>,
    should_stop: Arc<AtomicBool>,
) {
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
                                s.error = Some(format!("请求失败: {}", sanitize_error(&err)));
                                s.done = true;
                            }
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                if let Ok(mut s) = stream_state.lock() {
                    let error_msg = e.safe_display();
                    let hint = if e.is_retryable() {
                        "\n（暂时性错误，系统稍后可能自动重试，或请稍候重试）"
                    } else if e.is_permanent() {
                        "\n（请检查 API 设置，此错误通常无需重试）"
                    } else {
                        ""
                    };
                    s.error = Some(format!("请求失败: {}{}", error_msg, hint));
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
            mode_button_regions: Vec::new(),
            attachment_chip_regions: Vec::new(),
            hover_attachment: None,
            content_height: 0.0,
            code_save_regions: Vec::new(),
            stick_to_bottom: true,
            caret_pos: 0,
            caret_visible: false,
            composition: None,
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
            history_panel_region: None,
            history_item_regions: Vec::new(),
            history: Vec::new(),
            reasoning_toggle_regions: Vec::new(),
            hot_data_store: Self::init_hot_data_store(),
            warm_data_store: Self::init_warm_data_store(),
            history_workspace_only: false,
            playbook_open: false,
            playbook_items: Vec::new(),
            playbook_button_region: None,
            playbook_delete_regions: Vec::new(),
            history_ws_toggle_region: None,
            history_page: 0,
            history_time_filter: HistoryTimeFilter::All,
            history_type_filter: None,
            history_detail_id: None,
            history_detail_conv: None,
            history_delete_regions: Vec::new(),
            history_page_prev_region: None,
            history_page_next_region: None,
            history_time_filter_regions: Vec::new(),
            history_type_filter_regions: Vec::new(),
            history_clear_all_region: None,
            history_detail_back_region: None,
            history_detail_restore_region: None,
            agent_iter_count: 0,
            last_truncated: false,
            continue_button_region: None,
            agent_pipeline: None,
            expanded_file_cards: std::collections::HashSet::new(),
            file_card_regions: Vec::new(),
            browse_folder_region: None,
            attach_file_button_region: None,
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
    fn sync_hot_data(&mut self) {
        // 先 snapshot 到槽位，确保热数据看到的是完整状态
        self.snapshot_active_into_slot();
        if let Some(store) = self.hot_data_store.take() {
            let panel_clone = self.clone_for_sync();
            let mut new_store = store;
            new_store.sync_from_panel(panel_clone);
            self.hot_data_store = Some(new_store);
        }
    }

    /// 为热数据同步克隆必要字段（避免 Clone 整个 AiPanel）
    fn clone_for_sync(&self) -> crate::ai_panel::AiPanel {
        crate::ai_panel::AiPanel {
            visible: self.visible,
            messages: self.messages.clone(),
            input: self.input.clone(),
            is_generating: self.is_generating,
            scroll_y: self.scroll_y,
            hover_apply_button: self.hover_apply_button,
            stream_state: Arc::clone(&self.stream_state),
            input_focused: self.input_focused,
            mode: self.mode,
            model_menu_open: self.model_menu_open,
            attachments: self.attachments.clone(),
            mode_button_regions: self.mode_button_regions.clone(),
            attachment_chip_regions: self.attachment_chip_regions.clone(),
            hover_attachment: self.hover_attachment,
            content_height: self.content_height,
            code_save_regions: self.code_save_regions.clone(),
            stick_to_bottom: self.stick_to_bottom,
            caret_pos: self.caret_pos,
            caret_visible: self.caret_visible,
            composition: self.composition.clone(),
            should_stop: Arc::clone(&self.should_stop),
            conversations: self.conversations.clone(),
            active: self.active,
            tab_regions: self.tab_regions.clone(),
            tab_close_regions: self.tab_close_regions.clone(),
            new_tab_region: self.new_tab_region,
            history_button_region: self.history_button_region,
            hover_tab: self.hover_tab,
            history_open: self.history_open,
            history_anim: self.history_anim,
            history_scroll: self.history_scroll,
            history_max_scroll: self.history_max_scroll,
            history_panel_region: self.history_panel_region,
            history_item_regions: self.history_item_regions.clone(),
            history: self.history.clone(),
            reasoning_toggle_regions: self.reasoning_toggle_regions.clone(),
            hot_data_store: None,
            warm_data_store: None,
            history_workspace_only: self.history_workspace_only,
            playbook_open: self.playbook_open,
            playbook_items: Vec::new(),
            playbook_button_region: None,
            playbook_delete_regions: Vec::new(),
            history_ws_toggle_region: None,
            history_page: self.history_page,
            history_time_filter: self.history_time_filter,
            history_type_filter: self.history_type_filter.clone(),
            history_detail_id: None,
            history_detail_conv: None,
            history_delete_regions: Vec::new(),
            history_page_prev_region: None,
            history_page_next_region: None,
            history_time_filter_regions: Vec::new(),
            history_type_filter_regions: Vec::new(),
            history_clear_all_region: None,
            history_detail_back_region: None,
            history_detail_restore_region: None,
            agent_iter_count: 0,
            last_truncated: false,
            continue_button_region: None,
            agent_pipeline: None,
            expanded_file_cards: std::collections::HashSet::new(),
            file_card_regions: Vec::new(),
            browse_folder_region: None,
            attach_file_button_region: None,
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
    fn restore_latest_conversation(&mut self) {
        let Some(store) = self.warm_data_store.as_ref() else {
            return;
        };
        let Ok(meta_list) = store.load_history_meta() else {
            return;
        };
        let Some(latest) = meta_list.first() else {
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
        slot.stream_state = Arc::clone(&self.stream_state);
        slot.should_stop = Arc::clone(&self.should_stop);
        slot.updated_at = now_secs();
    }

    /// 把某槽位会话加载为活动会话的实时（扁平）状态。
    /// 休眠槽位先从温数据层水合消息体，覆盖所有激活路径（切换/关闭接管/历史恢复）。
    fn load_slot_into_active(&mut self, idx: usize) {
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

    /// 对指定空闲标签发起休眠请求：异步归档进 SQLite，落库成功回执后才卸载消息体。
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
            // 持久化：异步归档进 SQLite（温数据层，含向量索引）
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
        // 否则尝试从 SQLite 加载完整会话，失败则创建占位会话
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

    /// 并发轮询所有会话：活动会话走扁平逻辑，其余走后台 drain。
    /// 返回 `(刚正常完成, 刚中断)` 两个会话下标列表：
    /// 前者应处理 Agent 动作（文件/命令），后者可抢救已接收的文件块。
    pub fn poll_all_background(&mut self) -> (Vec<usize>, Vec<usize>) {
        let mut completed = Vec::new();
        let mut interrupted = Vec::new();
        match self.check_background_result() {
            DrainEdge::Completed => completed.push(self.active),
            DrainEdge::Interrupted => interrupted.push(self.active),
            DrainEdge::Truncated => {} // 截断：UI 显示"继续生成"按钮，不自动处理文件/命令
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
                DrainEdge::Truncated => {} // 后台会话截断：仅显示消息，无按钮
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

    /// 添加助手消息
    pub fn add_assistant_message(&mut self, content: String) {
        self.messages
            .push(AiMessage::new(AiRole::Assistant, content));
        self.stick_to_bottom = true;
        self.sync_hot_data();
    }

    /// 发送消息（AI-H01: 非阻塞 — HTTP 调用在后台线程执行，结果通过 stream_state 流式返回）
    pub fn send_message(&mut self, settings: &AiSettings) -> Result<String, String> {
        self.agent_iter_count = 0;
        self.agent_pipeline = None;
        self.send_message_internal(settings, self.input.clone(), AiMode::Ask, None)
    }

    /// 发送消息，并附带当前编辑器的上下文
    pub fn send_message_with_context(
        &mut self,
        settings: &AiSettings,
        editor: &EditorState,
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
        self.agent_iter_count = 0;
        self.agent_pipeline = None;
        self.send_message_internal(settings, self.input.clone(), mode, Some(context))
    }

    /// Agent 工具结果回喂：把终端命令输出作为上下文再次发起请求，驱动
    /// 「推理 → 执行 → 结果回喂 → 继续推理」循环。受最大轮次限制防止无限回环。
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
        self.send_message_internal(settings, feedback, mode, None)
    }

    /// 逐任务 worker 调用：以给定的 system/user 两条消息发起**聚焦**流式请求，
    /// **不含**累积对话历史（因此上下文窗口占用极小）。生成内容流入新的助手消息，
    /// 完成后由编排器（editor::advance_agent_pipeline）落盘并推进下一任务。
    pub fn stream_focused(&mut self, settings: &AiSettings, system: String, user: String) {
        self.is_generating = true;
        self.should_stop.store(false, Ordering::SeqCst);
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
            },
            ChatMessage {
                role: "user".to_string(),
                content: user,
            },
        ];
        spawn_ai_stream(
            settings.clone(),
            messages,
            Arc::clone(&self.stream_state),
            Arc::clone(&self.should_stop),
        );
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

    /// 命中测试文件卡片，返回 (msg_idx, block_seq)
    pub fn hit_test_file_card(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        self.file_card_regions
            .iter()
            .find(|(_, _, rx, ry, rw, rh)| x >= *rx && x <= rx + rw && y >= *ry && y <= ry + rh)
            .map(|(mi, bi, ..)| (*mi, *bi))
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
        self.input.clear();
        self.caret_pos = 0;
        self.is_generating = true;
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
                _ => ChatMessage {
                    role: "assistant".to_string(),
                    content: m.content.clone(),
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
                self.stick_to_bottom = true;
            }
            if !partial.is_empty() {
                self.stick_to_bottom = true;
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
                // 生成完成：自动折叠思考块，保持界面整洁
                if let Some(last) = self.messages.last_mut() {
                    last.stop_reasoning_timer();
                    if last.role == AiRole::Assistant && last.reasoning.is_some() {
                        last.reasoning_collapsed = true;
                    }
                }
                if truncated.is_some() {
                    // 被截断：显示消息并设置标记，UI 会渲染"继续生成"按钮
                    self.add_assistant_message(format!(
                        "⚠️ 输出已被截断（原因：{}）。点击下方按钮继续生成。",
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
                (
                    AiContextAttachment::LocalFile { path: x, .. },
                    AiContextAttachment::LocalFile { path: y, .. },
                ) => x == y,
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

    /// 读取本地文件并构造为 LocalFile 附件。
    /// 失败（不存在/无权限/疑似二进制/超大）时返回 Err(原因)，由调用方展示状态。
    pub fn read_local_file_attachment(
        path: &std::path::Path,
    ) -> Result<AiContextAttachment, String> {
        const MAX_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB 上限，避免超大文件撑爆上下文
        let meta = std::fs::metadata(path).map_err(|e| format!("无法读取文件信息：{}", e))?;
        if meta.len() > MAX_BYTES {
            return Err(format!(
                "文件过大（{} KB），上限 {} KB",
                meta.len() / 1024,
                MAX_BYTES / 1024
            ));
        }
        let bytes = std::fs::read(path).map_err(|e| format!("读取文件失败：{}", e))?;
        // 简单二进制检测：含 NUL 字节则拒绝（文本文件通常不含）
        if bytes.contains(&0) {
            return Err("疑似二进制文件，无法作为文本上下文".to_string());
        }
        let content = String::from_utf8_lossy(&bytes).to_string();
        Ok(AiContextAttachment::LocalFile {
            path: path.to_string_lossy().to_string(),
            content,
        })
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
        self.history_ws_toggle_region = None;
        self.history_delete_regions.clear();
        self.history_page_prev_region = None;
        self.history_page_next_region = None;
        self.history_time_filter_regions.clear();
        self.history_type_filter_regions.clear();
        self.history_clear_all_region = None;
        self.history_detail_back_region = None;
        self.history_detail_restore_region = None;
        self.history_panel_region = None;
        self.browse_folder_region = None;
        self.attach_file_button_region = None;
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

    /// 立即关闭历史下拉面板（跳过收起动画，用于标签切换/关闭等场景）
    pub fn dismiss_history_dropdown(&mut self) {
        self.history_open = false;
        self.history_anim = 0.0;
        self.history_scroll = 0.0;
    }

    // ===== Playbook 管理面板 =====

    /// 切换 Playbook 管理面板展开/收起（展开时从 SQLite 加载条目）
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

    /// 应用时间 + 类型筛选后的历史下标（对应 self.history 的原始下标）
    pub fn filtered_history_indices(&self) -> Vec<usize> {
        let cutoff = self.history_time_filter.cutoff(now_secs());
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
                time_ok && type_ok
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

    /// 设置时间筛选（回到第一页）
    pub fn set_history_time_filter(&mut self, f: HistoryTimeFilter) {
        self.history_time_filter = f;
        self.history_page = 0;
    }

    /// 设置类型筛选（回到第一页）
    pub fn set_history_type_filter(&mut self, f: Option<String>) {
        self.history_type_filter = f;
        self.history_page = 0;
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

    /// 删除一条历史记录（内存索引 + SQLite 级联删除）
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

/// 解析段落内的轻量 Markdown：标题(`#`/`##`/`###`)、无序列表(`-`/`*`/`+`)、粗体(`**`)。
///
/// 返回 `(清洗后的 UTF-16 文本, 粗体范围, 标题范围[start,len,字号])`，
/// 范围以 UTF-16 code unit 为单位，直接供 `IDWriteTextLayout` 的 range 样式使用。
#[allow(clippy::type_complexity)]
pub fn parse_markdown_segment(text: &str) -> (Vec<u16>, Vec<(u32, u32)>, Vec<(u32, u32, f32)>) {
    let mut clean: Vec<u16> = Vec::new();
    let mut bolds: Vec<(u32, u32)> = Vec::new();
    let mut headings: Vec<(u32, u32, f32)> = Vec::new();

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

        // 行内粗体 **text**
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;
        while i < chars.len() {
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

    (clean, bolds, headings)
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
    fn read_local_file_ok() {
        let mut p = std::env::temp_dir();
        p.push(format!("aether_attach_test_{}.txt", std::process::id()));
        std::fs::write(&p, "hello 上传\nline2").unwrap();
        let att = AiPanel::read_local_file_attachment(&p).expect("应读取成功");
        match att {
            AiContextAttachment::LocalFile { path, content } => {
                assert!(path.ends_with(".txt"));
                assert!(content.contains("hello 上传"));
                assert!(content.contains("line2"));
            }
            _ => panic!("应为 LocalFile 变体"),
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_local_file_rejects_binary() {
        let mut p = std::env::temp_dir();
        p.push(format!("aether_attach_bin_{}.bin", std::process::id()));
        std::fs::write(&p, [0u8, 1, 2, 3, 0, 255]).unwrap();
        let err = AiPanel::read_local_file_attachment(&p).unwrap_err();
        assert!(err.contains("二进制"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn read_local_file_missing_errors() {
        let p = std::path::Path::new("Z:\\definitely\\not\\here_aether_xyz.txt");
        assert!(AiPanel::read_local_file_attachment(p).is_err());
    }

    #[test]
    fn toggle_local_file_dedup_by_path() {
        let mut panel = AiPanel::new();
        let a = AiContextAttachment::LocalFile {
            path: "C:\\x\\a.rs".to_string(),
            content: "one".to_string(),
        };
        // 同路径但内容不同，应视为同一附件（按 path 去重）
        let a2 = AiContextAttachment::LocalFile {
            path: "C:\\x\\a.rs".to_string(),
            content: "two".to_string(),
        };
        panel.toggle_attachment(a);
        assert_eq!(panel.attachments.len(), 1);
        panel.toggle_attachment(a2); // 命中已存在 → 移除
        assert_eq!(panel.attachments.len(), 0);
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
    fn clear_all_history_empties_list_and_resets_page() {
        let now = now_secs();
        let mut p = test_panel();
        p.history = (0..8)
            .map(|i| meta(&format!("c{}", i), now, "Ask"))
            .collect();
        p.history_page = 1;
        p.open_history_detail(0);
        let n = p.clear_all_history().unwrap();
        assert_eq!(n, 8);
        assert!(p.history.is_empty());
        assert_eq!(p.history_page, 0);
        assert!(p.history_detail_id.is_none());
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
