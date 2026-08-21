use aether_shared::settings::{AiModelProfile, AiSettings, AppSettings};
use std::sync::{Arc, Mutex};

/// 设置面板字段标识
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsField {
    Provider,
    ApiKey,
    BaseUrl,
    Model,
    /// 自定义模型名称（用户可自由编写，用于区分不同厂商的相同模型）
    DisplayName,
    Temperature,
    MaxTokens,
    MaxInputTokens,
    SystemPrompt,
    /// 停止序列（逗号分隔，开发者参数）
    Stop,
    /// top_logprobs 候选数（0-20，开发者参数）
    TopLogprobs,
    /// 业务侧用户标识（开发者参数）
    UserId,
}

/// 设置面板按钮标识
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsButton {
    Save,
    TestConnection,
    /// 模型编辑视图返回模型列表
    BackToModels,
    /// 立即检查更新
    CheckUpdate,
}

/// 设置标签页类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    /// 通用：主题、字体等
    General,
    /// AI 接口：provider / key / url / model / temperature / max_tokens / system_prompt / 测试连接
    Ai,
    /// 外观：侧边栏、密度等
    Appearance,
    /// 远程：SSH 主机等
    Remote,
    /// 模型管理
    Models,
    /// 策略库管理
    Playbook,
    /// 更新设置
    Update,
}

impl SettingsTab {
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Ai => "AI",
            Self::Appearance => "外观",
            Self::Remote => "远程",
            Self::Models => "模型",
            Self::Playbook => "策略",
            Self::Update => "更新",
        }
    }

    /// 导航中展示的标签页。AI 配置已并入「模型」页（新建/编辑模型时以内嵌表单形式出现），
    /// 因此不再单独列出 AI 项；账户功能当前不需要，也一并移除。
    pub const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::Appearance,
        SettingsTab::Remote,
        SettingsTab::Models,
        SettingsTab::Playbook,
        SettingsTab::Update,
    ];
}

/// 服务商模板按钮
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderTemplateButton {
    DeepSeek,
    Kimi,
    Custom,
}

/// 设置下拉类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsDropdownKind {
    Provider,
    Model,
}

/// 模型按钮类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelButton {
    Add,
    Edit,
    Delete,
    ToggleEnabled,
    /// 能力评测（沙盒评测入口）
    Eval,
}

/// 测试连接轮询结果
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestPollResult {
    /// 测试成功，且有待保存的设置
    SuccessWithPendingSave,
    /// 测试成功，无待保存操作
    Success,
    /// 测试失败，且有待保存的设置
    FailedWithPendingSave,
    /// 测试失败，无待保存操作
    Failed,
    /// 测试尚未完成
    Pending,
}

/// 模型配置项
#[derive(Clone, Debug)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub provider: String,
    pub description: String,
    pub enabled: bool,
    // 多模型：完整配置字段
    pub api_key: String,
    pub base_url: String,
    pub temperature: String,
    pub top_p: String,
    pub max_tokens: String,
    pub max_input_tokens: String,
    pub system_prompt: String,
    /// 深度思考开关（仅 DeepSeek 生效），默认开启
    pub thinking: bool,
    /// 思考强度（"high"/"max"，仅 DeepSeek 思考模式生效）
    pub reasoning_effort: String,
    /// 频率惩罚（-2.0 ~ 2.0）
    pub frequency_penalty: String,
    /// 存在惩罚（-2.0 ~ 2.0）
    pub presence_penalty: String,
    /// 停止序列（逗号分隔，最多 16 个）
    pub stop: String,
    /// 响应格式（"text"/"json_object"）
    pub response_format: String,
    /// 流式用量统计开关
    pub include_usage: bool,
    /// logprobs 调试开关
    pub logprobs: bool,
    /// top_logprobs 候选数（0-20，空=不下发）
    pub top_logprobs: String,
    /// 业务侧用户标识（空=不下发）
    pub user_id: String,
}

impl ModelConfig {
    /// 转换为持久化用的 AiModelProfile
    pub fn to_profile(&self) -> AiModelProfile {
        AiModelProfile {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            base_url: if self.base_url.is_empty() {
                None
            } else {
                Some(self.base_url.clone())
            },
            model: self.name.clone(),
            temperature: self.temperature.trim().parse().ok(),
            top_p: self.top_p.trim().parse().ok(),
            max_tokens: self.max_tokens.trim().parse().ok(),
            max_input_tokens: self.max_input_tokens.trim().parse().ok(),
            system_prompt: if self.system_prompt.is_empty() {
                None
            } else {
                Some(self.system_prompt.clone())
            },
            enabled: self.enabled,
            thinking: Some(self.thinking),
            reasoning_effort: if self.reasoning_effort.is_empty() {
                None
            } else {
                Some(self.reasoning_effort.clone())
            },
            frequency_penalty: self.frequency_penalty.trim().parse().ok(),
            presence_penalty: self.presence_penalty.trim().parse().ok(),
            stop: parse_stop_sequences(&self.stop),
            response_format: if self.response_format == "json_object" {
                Some("json_object".to_string())
            } else {
                None
            },
            include_usage: if self.include_usage { Some(true) } else { None },
            logprobs: if self.logprobs { Some(true) } else { None },
            top_logprobs: if self.logprobs {
                self.top_logprobs.trim().parse().ok()
            } else {
                None
            },
            user_id: if self.user_id.trim().is_empty() {
                None
            } else {
                Some(self.user_id.trim().to_string())
            },
        }
    }

    /// 从持久化的 AiModelProfile 构造
    pub fn from_profile(p: &AiModelProfile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.model.clone(),
            display_name: if p.display_name.is_empty() {
                p.model.clone()
            } else {
                p.display_name.clone()
            },
            provider: p.provider.clone(),
            description: String::new(),
            enabled: p.enabled,
            api_key: p.api_key.clone(),
            base_url: p.base_url.clone().unwrap_or_default(),
            temperature: p
                .temperature
                .map(|t| t.to_string())
                .unwrap_or_else(|| "0.7".to_string()),
            top_p: p
                .top_p
                .map(|t| t.to_string())
                .unwrap_or_else(|| "1.0".to_string()),
            max_tokens: p
                .max_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "8192".to_string()),
            max_input_tokens: p
                .max_input_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "24000".to_string()),
            system_prompt: p.system_prompt.clone().unwrap_or_default(),
            thinking: p.thinking.unwrap_or(true),
            reasoning_effort: p
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "high".to_string()),
            frequency_penalty: p
                .frequency_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string()),
            presence_penalty: p
                .presence_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string()),
            stop: p.stop.as_ref().map(|s| s.join(", ")).unwrap_or_default(),
            response_format: p
                .response_format
                .clone()
                .unwrap_or_else(|| "text".to_string()),
            include_usage: p.include_usage.unwrap_or(false),
            logprobs: p.logprobs.unwrap_or(false),
            top_logprobs: p.top_logprobs.map(|n| n.to_string()).unwrap_or_default(),
            user_id: p.user_id.clone().unwrap_or_default(),
        }
    }
}

/// 把逗号分隔的停止序列文本解析为列表（去空白、去空项，最多 16 个）；空结果返回 None
pub fn parse_stop_sequences(raw: &str) -> Option<Vec<String>> {
    let seqs: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(16)
        .collect();
    if seqs.is_empty() {
        None
    } else {
        Some(seqs)
    }
}

/// AI 设置面板状态
#[derive(Clone, Debug)]
pub struct SettingsPanel {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// 自定义模型名称（用户可自由编写，用于区分不同厂商的相同模型）
    pub display_name: String,
    pub temperature: String,
    pub top_p: String,
    pub max_tokens: String,
    pub max_input_tokens: String,
    pub system_prompt: String,
    pub active_field: Option<SettingsField>,
    pub hover_button: Option<SettingsButton>,
    pub test_status: String,
    pub is_testing: bool,
    /// 后台测试连接结果（Some(Ok)=成功，Some(Err)=失败，None=未完成）
    pub test_result: Arc<Mutex<Option<Result<String, String>>>>,
    /// 测试通过后是否自动保存设置
    pub pending_save: bool,
    /// 从 /models 拉取到的模型 ID 列表（对应 fetched_models_key 的配置）
    pub fetched_models: Vec<String>,
    /// fetched_models 对应的配置指纹（provider|base|key），用于判断是否需重新拉取
    pub fetched_models_key: String,
    /// 是否正在后台拉取模型列表
    pub is_fetching_models: bool,
    /// 后台拉取模型结果槽位（Some(Ok)=成功，Some(Err)=失败，None=未完成）
    pub models_fetch_result: Arc<Mutex<Option<Result<Vec<String>, String>>>>,
    /// 拉取状态提示（供模型下拉展示，如"获取中…"/"获取失败"）
    pub models_fetch_status: String,
    // Cached layout for hit testing
    pub field_regions: Vec<(SettingsField, f32, f32, f32, f32)>,
    pub button_regions: Vec<(SettingsButton, f32, f32, f32, f32)>,
    /// 标签页：当前激活
    pub active_tab: SettingsTab,
    /// 标签页：鼠标悬停
    pub hover_tab: Option<SettingsTab>,
    /// 标签页命中区域缓存 (tab, x, y, w, h)
    pub tab_regions: Vec<(SettingsTab, f32, f32, f32, f32)>,
    // 导航栏
    pub nav_width: f32,
    pub hover_nav_resize: bool,
    pub nav_resizing: bool,
    // 模型管理
    pub models: Vec<ModelConfig>,
    pub selected_model_id: Option<String>,
    pub hover_model_id: Option<String>,
    pub active_model_id: Option<String>,
    /// 「模型」页当前是否处于新建/编辑模型的内嵌表单视图（false=模型列表，true=AI 配置表单）
    pub model_editing: bool,
    pub hover_model_button: Option<ModelButton>,
    /// 悬停按钮对应的模型ID（用于区分不同模型项上的按钮）
    pub hover_model_button_id: Option<String>,
    // 模型按钮/项命中区域
    model_button_regions: Vec<(ModelButton, f32, f32, f32, f32)>,
    model_item_regions: Vec<(String, f32, f32, f32, f32)>,
    // 下拉框
    pub open_dropdown: Option<SettingsDropdownKind>,
    pub dropdown_trigger_regions: Vec<(SettingsDropdownKind, f32, f32, f32, f32)>,
    pub dropdown_item_regions: Vec<(SettingsDropdownKind, usize, f32, f32, f32, f32)>,
    pub hover_dropdown: Option<SettingsDropdownKind>,
    pub hover_dropdown_index: Option<usize>,
    // 滚动：scroll_offset 为当前偏移，content_height 为最大可滚动距离（总内容高 - 可视高）
    pub scroll_offset: f32,
    pub content_height: f32,
    // API 密钥显隐
    pub show_api_key: bool,
    pub hover_api_key_toggle: bool,
    pub api_key_toggle_region: Option<(f32, f32, f32, f32)>,
    // 温度滑块轨道命中区
    pub temp_slider_region: Option<(f32, f32, f32, f32)>,
    /// 温度滑块是否处于拖拽中
    pub temp_slider_dragging: bool,
    // Top-p 滑块轨道命中区
    pub top_p_slider_region: Option<(f32, f32, f32, f32)>,
    /// Top-p 滑块是否处于拖拽中
    pub top_p_slider_dragging: bool,
    /// 深度思考开关状态（仅 DeepSeek 显示），默认开启
    pub thinking: bool,
    /// 深度思考开关命中区
    pub thinking_toggle_region: Option<(f32, f32, f32, f32)>,
    /// 深度思考开关悬停态
    pub hover_thinking_toggle: bool,
    /// 思考强度（"high"/"max"）
    pub reasoning_effort: String,
    /// 思考强度分段按钮命中区 (强度值, x, y, w, h)
    pub effort_regions: Vec<(&'static str, f32, f32, f32, f32)>,
    /// 思考强度分段悬停态（命中的强度值）
    pub hover_effort: Option<&'static str>,
    // ---- 开发者参数 ----
    /// 开发者参数区是否展开
    pub dev_params_expanded: bool,
    /// 开发者参数区标题命中区
    pub dev_params_toggle_region: Option<(f32, f32, f32, f32)>,
    /// 频率惩罚（-2.0 ~ 2.0，字符串编辑态）
    pub frequency_penalty: String,
    /// 频率惩罚滑块轨道命中区
    pub freq_slider_region: Option<(f32, f32, f32, f32)>,
    /// 频率惩罚滑块拖拽中
    pub freq_slider_dragging: bool,
    /// 存在惩罚（-2.0 ~ 2.0，字符串编辑态）
    pub presence_penalty: String,
    /// 存在惩罚滑块轨道命中区
    pub pres_slider_region: Option<(f32, f32, f32, f32)>,
    /// 存在惩罚滑块拖拽中
    pub pres_slider_dragging: bool,
    /// 停止序列（逗号分隔）
    pub stop: String,
    /// 响应格式（"text"/"json_object"）
    pub response_format: String,
    /// 响应格式分段命中区 (格式值, x, y, w, h)
    pub response_format_regions: Vec<(&'static str, f32, f32, f32, f32)>,
    /// 响应格式分段悬停态
    pub hover_response_format: Option<&'static str>,
    /// 流式用量统计开关
    pub include_usage: bool,
    /// 流式用量统计开关命中区
    pub include_usage_toggle_region: Option<(f32, f32, f32, f32)>,
    /// logprobs 调试开关
    pub logprobs: bool,
    /// logprobs 开关命中区
    pub logprobs_toggle_region: Option<(f32, f32, f32, f32)>,
    /// top_logprobs 候选数（0-20，字符串编辑态，空=不下发）
    pub top_logprobs: String,
    /// 业务侧用户标识（空=不下发）
    pub user_id: String,
    /// 打开设置面板时的 AI 配置快照，用于"未保存更改"检测
    pub baseline_ai: Option<AiSettings>,
    /// 外观页：最大化时显示任务栏开关命中区
    pub taskbar_toggle_region: Option<(f32, f32, f32, f32)>,
    /// 通用页：默认启动模式行命中区（点击切换 开发者/智能体，持久化后重启生效）
    pub default_mode_toggle_region: Option<(f32, f32, f32, f32)>,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            provider: "deepseek".to_string(),
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-v4-pro".to_string(),
            display_name: String::new(),
            temperature: "0.7".to_string(),
            top_p: "1.0".to_string(),
            max_tokens: "8192".to_string(),
            max_input_tokens: "24000".to_string(),
            system_prompt: String::new(),
            active_field: None,
            hover_button: None,
            test_status: String::new(),
            is_testing: false,
            pending_save: false,
            test_result: Arc::new(Mutex::new(None)),
            fetched_models: Vec::new(),
            fetched_models_key: String::new(),
            is_fetching_models: false,
            models_fetch_result: Arc::new(Mutex::new(None)),
            models_fetch_status: String::new(),
            field_regions: Vec::new(),
            button_regions: Vec::new(),
            active_tab: SettingsTab::Models,
            hover_tab: None,
            tab_regions: Vec::new(),
            nav_width: 120.0,
            hover_nav_resize: false,
            nav_resizing: false,
            models: Vec::new(),
            selected_model_id: None,
            hover_model_id: None,
            active_model_id: None,
            model_editing: false,
            hover_model_button: None,
            hover_model_button_id: None,
            model_button_regions: Vec::new(),
            model_item_regions: Vec::new(),
            open_dropdown: None,
            dropdown_trigger_regions: Vec::new(),
            dropdown_item_regions: Vec::new(),
            hover_dropdown: None,
            hover_dropdown_index: None,
            scroll_offset: 0.0,
            content_height: 0.0,
            show_api_key: false,
            hover_api_key_toggle: false,
            api_key_toggle_region: None,
            temp_slider_region: None,
            temp_slider_dragging: false,
            top_p_slider_region: None,
            top_p_slider_dragging: false,
            thinking: true,
            thinking_toggle_region: None,
            hover_thinking_toggle: false,
            reasoning_effort: "high".to_string(),
            effort_regions: Vec::new(),
            hover_effort: None,
            dev_params_expanded: false,
            dev_params_toggle_region: None,
            frequency_penalty: "0.0".to_string(),
            freq_slider_region: None,
            freq_slider_dragging: false,
            presence_penalty: "0.0".to_string(),
            pres_slider_region: None,
            pres_slider_dragging: false,
            stop: String::new(),
            response_format: "text".to_string(),
            response_format_regions: Vec::new(),
            hover_response_format: None,
            include_usage: false,
            include_usage_toggle_region: None,
            logprobs: false,
            logprobs_toggle_region: None,
            top_logprobs: String::new(),
            user_id: String::new(),
            baseline_ai: None,
            taskbar_toggle_region: None,
            default_mode_toggle_region: None,
        }
    }

    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            provider: settings.ai.provider.clone(),
            api_key: settings.ai.api_key.clone(),
            base_url: settings.ai.base_url.clone().unwrap_or_default(),
            model: settings.ai.model.clone(),
            display_name: String::new(),
            temperature: settings
                .ai
                .temperature
                .map(|t| t.to_string())
                .unwrap_or_else(|| "0.7".to_string()),
            top_p: settings
                .ai
                .top_p
                .map(|t| t.to_string())
                .unwrap_or_else(|| "1.0".to_string()),
            max_tokens: settings
                .ai
                .max_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "8192".to_string()),
            max_input_tokens: settings
                .ai
                .max_input_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "24000".to_string()),
            system_prompt: settings.ai.system_prompt.clone().unwrap_or_default(),
            active_field: None,
            hover_button: None,
            test_status: String::new(),
            is_testing: false,
            pending_save: false,
            test_result: Arc::new(Mutex::new(None)),
            fetched_models: Vec::new(),
            fetched_models_key: String::new(),
            is_fetching_models: false,
            models_fetch_result: Arc::new(Mutex::new(None)),
            models_fetch_status: String::new(),
            field_regions: Vec::new(),
            button_regions: Vec::new(),
            active_tab: SettingsTab::Models,
            hover_tab: None,
            tab_regions: Vec::new(),
            nav_width: 120.0,
            hover_nav_resize: false,
            nav_resizing: false,
            models: Vec::new(),
            selected_model_id: None,
            hover_model_id: None,
            active_model_id: None,
            model_editing: false,
            hover_model_button: None,
            hover_model_button_id: None,
            model_button_regions: Vec::new(),
            model_item_regions: Vec::new(),
            open_dropdown: None,
            dropdown_trigger_regions: Vec::new(),
            dropdown_item_regions: Vec::new(),
            hover_dropdown: None,
            hover_dropdown_index: None,
            scroll_offset: 0.0,
            content_height: 0.0,
            show_api_key: false,
            hover_api_key_toggle: false,
            api_key_toggle_region: None,
            temp_slider_region: None,
            temp_slider_dragging: false,
            top_p_slider_region: None,
            top_p_slider_dragging: false,
            thinking: settings.ai.thinking.unwrap_or(true),
            thinking_toggle_region: None,
            hover_thinking_toggle: false,
            reasoning_effort: settings
                .ai
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "high".to_string()),
            effort_regions: Vec::new(),
            hover_effort: None,
            dev_params_expanded: false,
            dev_params_toggle_region: None,
            frequency_penalty: settings
                .ai
                .frequency_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string()),
            freq_slider_region: None,
            freq_slider_dragging: false,
            presence_penalty: settings
                .ai
                .presence_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string()),
            pres_slider_region: None,
            pres_slider_dragging: false,
            stop: settings
                .ai
                .stop
                .as_ref()
                .map(|s| s.join(", "))
                .unwrap_or_default(),
            response_format: settings
                .ai
                .response_format
                .clone()
                .unwrap_or_else(|| "text".to_string()),
            response_format_regions: Vec::new(),
            hover_response_format: None,
            include_usage: settings.ai.include_usage.unwrap_or(false),
            include_usage_toggle_region: None,
            logprobs: settings.ai.logprobs.unwrap_or(false),
            logprobs_toggle_region: None,
            top_logprobs: settings
                .ai
                .top_logprobs
                .map(|n| n.to_string())
                .unwrap_or_default(),
            user_id: settings.ai.user_id.clone().unwrap_or_default(),
            baseline_ai: None,
            taskbar_toggle_region: None,
            default_mode_toggle_region: None,
        }
    }

    pub fn to_ai_settings(&self) -> AiSettings {
        AiSettings {
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            base_url: if self.base_url.is_empty() {
                None
            } else {
                Some(self.base_url.clone())
            },
            model: self.model.clone(),
            temperature: self.temperature.parse().ok(),
            top_p: self.top_p.parse().ok(),
            max_tokens: self.max_tokens.parse().ok(),
            max_input_tokens: self.max_input_tokens.parse().ok(),
            system_prompt: if self.system_prompt.is_empty() {
                None
            } else {
                Some(self.system_prompt.clone())
            },
            thinking: Some(self.thinking),
            reasoning_effort: if self.reasoning_effort.is_empty() {
                None
            } else {
                Some(self.reasoning_effort.clone())
            },
            frequency_penalty: self.frequency_penalty.trim().parse().ok(),
            presence_penalty: self.presence_penalty.trim().parse().ok(),
            stop: parse_stop_sequences(&self.stop),
            response_format: if self.response_format == "json_object" {
                Some("json_object".to_string())
            } else {
                None
            },
            include_usage: if self.include_usage { Some(true) } else { None },
            logprobs: if self.logprobs { Some(true) } else { None },
            top_logprobs: if self.logprobs {
                self.top_logprobs.trim().parse().ok()
            } else {
                None
            },
            user_id: if self.user_id.trim().is_empty() {
                None
            } else {
                Some(self.user_id.trim().to_string())
            },
        }
    }

    pub fn apply_settings(&mut self, settings: &AppSettings) {
        // 打开设置面板时默认展示模型列表（非编辑态）
        self.model_editing = false;
        // 加载模型列表（多模型架构）
        self.models = settings
            .ai_models
            .iter()
            .map(ModelConfig::from_profile)
            .collect();
        self.active_model_id = settings
            .active_model_id
            .clone()
            .filter(|id| self.models.iter().any(|m| &m.id == id))
            .or_else(|| self.models.first().map(|m| m.id.clone()));
        // 加载激活模型（或回退旧单一配置）到 AI 页字段
        self.load_active_model_fields(&settings.ai);
        // 记录打开时的快照，作为未保存更改检测的基准
        self.baseline_ai = Some(self.to_ai_settings());
    }

    /// 把激活模型的配置加载到 AI 页编辑字段；无模型时回退到传入的 fallback 配置
    pub fn load_active_model_fields(&mut self, fallback_ai: &AiSettings) {
        let found = self
            .active_model_id
            .as_ref()
            .and_then(|id| self.models.iter().find(|m| &m.id == id))
            .cloned();
        if let Some(m) = found {
            self.provider = m.provider;
            self.api_key = m.api_key;
            self.base_url = m.base_url;
            self.model = m.name;
            self.display_name = m.display_name;
            self.temperature = m.temperature;
            self.top_p = m.top_p;
            self.max_tokens = m.max_tokens;
            self.max_input_tokens = m.max_input_tokens;
            self.system_prompt = m.system_prompt;
            self.thinking = m.thinking;
            self.reasoning_effort = m.reasoning_effort;
            self.frequency_penalty = m.frequency_penalty;
            self.presence_penalty = m.presence_penalty;
            self.stop = m.stop;
            self.response_format = m.response_format;
            self.include_usage = m.include_usage;
            self.logprobs = m.logprobs;
            self.top_logprobs = m.top_logprobs;
            self.user_id = m.user_id;
        } else {
            self.provider = fallback_ai.provider.clone();
            self.api_key = fallback_ai.api_key.clone();
            self.base_url = fallback_ai.base_url.clone().unwrap_or_default();
            self.model = fallback_ai.model.clone();
            self.display_name = String::new();
            self.temperature = fallback_ai
                .temperature
                .map(|t| t.to_string())
                .unwrap_or_else(|| "0.7".to_string());
            self.top_p = fallback_ai
                .top_p
                .map(|t| t.to_string())
                .unwrap_or_else(|| "1.0".to_string());
            self.max_tokens = fallback_ai
                .max_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "8192".to_string());
            self.max_input_tokens = fallback_ai
                .max_input_tokens
                .map(|m| m.to_string())
                .unwrap_or_else(|| "24000".to_string());
            self.system_prompt = fallback_ai.system_prompt.clone().unwrap_or_default();
            self.thinking = fallback_ai.thinking.unwrap_or(true);
            self.reasoning_effort = fallback_ai
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "high".to_string());
            self.frequency_penalty = fallback_ai
                .frequency_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string());
            self.presence_penalty = fallback_ai
                .presence_penalty
                .map(|v| format!("{:.1}", v))
                .unwrap_or_else(|| "0.0".to_string());
            self.stop = fallback_ai
                .stop
                .as_ref()
                .map(|s| s.join(", "))
                .unwrap_or_default();
            self.response_format = fallback_ai
                .response_format
                .clone()
                .unwrap_or_else(|| "text".to_string());
            self.include_usage = fallback_ai.include_usage.unwrap_or(false);
            self.logprobs = fallback_ai.logprobs.unwrap_or(false);
            self.top_logprobs = fallback_ai
                .top_logprobs
                .map(|n| n.to_string())
                .unwrap_or_default();
            self.user_id = fallback_ai.user_id.clone().unwrap_or_default();
        }
    }

    /// 把当前 AI 页编辑字段写回激活模型档案（无激活模型时用当前字段自动创建）
    pub fn store_fields_to_active_model(&mut self) {
        let has_active = self
            .active_model_id
            .as_ref()
            .map(|id| self.models.iter().any(|m| &m.id == id))
            .unwrap_or(false);
        if !has_active {
            // 未填写密钥且未填写模型名则不创建（避免「新建」后未填写就误建空模型；
            // 只有填了有效信息并点击「保存」才应产生一条模型）
            if self.api_key.trim().is_empty() && self.model.trim().is_empty() {
                return;
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let new_id = format!("model-{}", stamp);
            self.models.push(ModelConfig {
                id: new_id.clone(),
                name: String::new(),
                display_name: String::new(),
                provider: "deepseek".to_string(),
                description: String::new(),
                enabled: true,
                api_key: String::new(),
                base_url: String::new(),
                temperature: "0.7".to_string(),
                top_p: "1.0".to_string(),
                max_tokens: "8192".to_string(),
                max_input_tokens: "24000".to_string(),
                system_prompt: String::new(),
                thinking: true,
                reasoning_effort: "high".to_string(),
                frequency_penalty: "0.0".to_string(),
                presence_penalty: "0.0".to_string(),
                stop: String::new(),
                response_format: "text".to_string(),
                include_usage: false,
                logprobs: false,
                top_logprobs: String::new(),
                user_id: String::new(),
            });
            self.active_model_id = Some(new_id);
        }
        let provider = self.provider.clone();
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();
        let display_name = self.display_name.clone();
        let temperature = self.temperature.clone();
        let top_p = self.top_p.clone();
        let max_tokens = self.max_tokens.clone();
        let max_input_tokens = self.max_input_tokens.clone();
        let system_prompt = self.system_prompt.clone();
        let thinking = self.thinking;
        let reasoning_effort = self.reasoning_effort.clone();
        let frequency_penalty = self.frequency_penalty.clone();
        let presence_penalty = self.presence_penalty.clone();
        let stop = self.stop.clone();
        let response_format = self.response_format.clone();
        let include_usage = self.include_usage;
        let logprobs = self.logprobs;
        let top_logprobs = self.top_logprobs.clone();
        let user_id = self.user_id.clone();
        if let Some(id) = self.active_model_id.clone() {
            if let Some(m) = self.models.iter_mut().find(|m| m.id == id) {
                m.provider = provider;
                m.api_key = api_key;
                m.base_url = base_url;
                m.name = model.clone();
                m.temperature = temperature;
                m.top_p = top_p;
                m.max_tokens = max_tokens;
                m.max_input_tokens = max_input_tokens;
                m.system_prompt = system_prompt;
                m.thinking = thinking;
                m.reasoning_effort = reasoning_effort;
                m.frequency_penalty = frequency_penalty;
                m.presence_penalty = presence_penalty;
                m.stop = stop;
                m.response_format = response_format;
                m.include_usage = include_usage;
                m.logprobs = logprobs;
                m.top_logprobs = top_logprobs;
                m.user_id = user_id;
                // 用户自定义名称优先；为空时回退为模型 ID
                if !display_name.is_empty() {
                    m.display_name = display_name;
                } else if m.display_name.is_empty() && !model.is_empty() {
                    m.display_name = model;
                }
            }
        }
    }

    /// 切换激活模型：先保存当前编辑，再加载目标模型字段
    pub fn set_active_model(&mut self, id: &str, fallback_ai: &AiSettings) {
        self.store_fields_to_active_model();
        self.active_model_id = Some(id.to_string());
        self.load_active_model_fields(fallback_ai);
    }

    /// 新建一个模型档案并设为激活，返回其 id
    pub fn create_new_model(&mut self) -> String {
        self.store_fields_to_active_model();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let id = format!("model-{}", stamp);
        self.models.push(ModelConfig {
            id: id.clone(),
            name: String::new(),
            display_name: "新模型".to_string(),
            provider: "deepseek".to_string(),
            description: String::new(),
            enabled: true,
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            temperature: "0.7".to_string(),
            top_p: "1.0".to_string(),
            max_tokens: "8192".to_string(),
            max_input_tokens: "24000".to_string(),
            system_prompt: String::new(),
            thinking: true,
            reasoning_effort: "high".to_string(),
            frequency_penalty: "0.0".to_string(),
            presence_penalty: "0.0".to_string(),
            stop: String::new(),
            response_format: "text".to_string(),
            include_usage: false,
            logprobs: false,
            top_logprobs: String::new(),
            user_id: String::new(),
        });
        self.active_model_id = Some(id.clone());
        self.provider = "deepseek".to_string();
        self.api_key = String::new();
        self.base_url = "https://api.deepseek.com/v1".to_string();
        self.model = String::new();
        self.temperature = "0.7".to_string();
        self.top_p = "1.0".to_string();
        self.max_tokens = "8192".to_string();
        self.max_input_tokens = "24000".to_string();
        self.system_prompt = String::new();
        self.thinking = true;
        self.reasoning_effort = "high".to_string();
        self.reset_dev_params();
        id
    }

    /// 开始「新建模型」草稿：把编辑字段重置为默认值，但**不**加入模型列表、**不**持久化。
    /// 只有用户点击「保存」（且连接验证通过）后，才会真正创建并保存该模型。
    /// 这样「新建」本身不会产生一条模型，避免误建空模型。
    pub fn begin_new_model_draft(&mut self) {
        self.active_model_id = None;
        self.provider = "deepseek".to_string();
        self.api_key = String::new();
        self.base_url = "https://api.deepseek.com/v1".to_string();
        self.model = String::new();
        self.temperature = "0.7".to_string();
        self.top_p = "1.0".to_string();
        self.max_tokens = "8192".to_string();
        self.max_input_tokens = "24000".to_string();
        self.system_prompt = String::new();
        self.thinking = true;
        self.reasoning_effort = "high".to_string();
        self.reset_dev_params();
        self.test_status.clear();
        self.active_field = None;
    }

    /// 把开发者参数重置为默认值（新建模型/草稿时用）
    fn reset_dev_params(&mut self) {
        self.frequency_penalty = "0.0".to_string();
        self.presence_penalty = "0.0".to_string();
        self.stop = String::new();
        self.response_format = "text".to_string();
        self.include_usage = false;
        self.logprobs = false;
        self.top_logprobs = String::new();
        self.user_id = String::new();
    }

    /// 把模型列表与激活选择同步回 AppSettings（供持久化）
    pub fn sync_to_app_settings(&self, app: &mut AppSettings) {
        app.ai_models = self.models.iter().map(|m| m.to_profile()).collect();
        app.active_model_id = self.active_model_id.clone();
    }

    /// 当前激活/编辑模型的展示名（用于 AI 页指示）
    pub fn active_model_display(&self) -> String {
        if let Some(id) = &self.active_model_id {
            if let Some(m) = self.models.iter().find(|m| &m.id == id) {
                let name = if m.display_name.is_empty() {
                    &m.name
                } else {
                    &m.display_name
                };
                if !name.is_empty() {
                    return name.clone();
                }
            }
        }
        "新模型".to_string()
    }

    /// 内嵌表单是否处于「添加模型」模式（草稿态，未关联已有模型）；
    /// false 表示编辑已有模型（active_model_id 指向列表中的模型）
    pub fn is_adding_model(&self) -> bool {
        self.active_model_id.is_none()
    }

    pub fn clear_regions(&mut self) {
        self.field_regions.clear();
        self.button_regions.clear();
        self.tab_regions.clear();
        self.model_button_regions.clear();
        self.model_item_regions.clear();
        self.dropdown_trigger_regions.clear();
        self.dropdown_item_regions.clear();
        self.thinking_toggle_region = None;
        self.effort_regions.clear();
        self.temp_slider_region = None;
        self.top_p_slider_region = None;
        self.dev_params_toggle_region = None;
        self.freq_slider_region = None;
        self.pres_slider_region = None;
        self.response_format_regions.clear();
        self.include_usage_toggle_region = None;
        self.logprobs_toggle_region = None;
    }

    pub fn add_field_region(&mut self, field: SettingsField, x: f32, y: f32, w: f32, h: f32) {
        self.field_regions.push((field, x, y, w, h));
    }

    pub fn add_button_region(&mut self, button: SettingsButton, x: f32, y: f32, w: f32, h: f32) {
        self.button_regions.push((button, x, y, w, h));
    }

    pub fn add_tab_region(&mut self, tab: SettingsTab, x: f32, y: f32, w: f32, h: f32) {
        self.tab_regions.push((tab, x, y, w, h));
        // 同步注册语义命中区，供测试框架/辅助点击按名称定位（如 settings_tab:general）
        let name = match tab {
            SettingsTab::General => "general",
            SettingsTab::Ai => "ai",
            SettingsTab::Appearance => "appearance",
            SettingsTab::Remote => "remote",
            SettingsTab::Models => "models",
            SettingsTab::Playbook => "playbook",
            SettingsTab::Update => "update",
        };
        crate::hit_test::register_hit_region(
            format!("settings_tab:{}", name),
            x,
            y,
            w,
            h,
        );
    }

    /// 命中检测：标签页
    pub fn hit_test_tab(&self, x: f32, y: f32) -> Option<SettingsTab> {
        for (tab, tx, ty, tw, th) in &self.tab_regions {
            if x >= *tx && x < tx + tw && y >= *ty && y < ty + th {
                return Some(*tab);
            }
        }
        None
    }

    pub fn hit_test_field(&self, x: f32, y: f32) -> Option<SettingsField> {
        for (field, fx, fy, fw, fh) in &self.field_regions {
            if x >= *fx && x < fx + fw && y >= *fy && y < fy + fh {
                return Some(*field);
            }
        }
        None
    }

    pub fn hit_test_button(&self, x: f32, y: f32) -> Option<SettingsButton> {
        for (button, bx, by, bw, bh) in &self.button_regions {
            if x >= *bx && x < bx + bw && y >= *by && y < by + bh {
                return Some(*button);
            }
        }
        None
    }

    pub fn input_char(&mut self, ch: char) {
        if let Some(field) = self.active_field {
            match field {
                SettingsField::Provider => self.provider.push(ch),
                SettingsField::ApiKey => self.api_key.push(ch),
                SettingsField::BaseUrl => self.base_url.push(ch),
                SettingsField::Model => self.model.push(ch),
                SettingsField::DisplayName => self.display_name.push(ch),
                SettingsField::Temperature => self.temperature.push(ch),
                SettingsField::MaxTokens => self.max_tokens.push(ch),
                SettingsField::MaxInputTokens => self.max_input_tokens.push(ch),
                SettingsField::SystemPrompt => self.system_prompt.push(ch),
                SettingsField::Stop => self.stop.push(ch),
                SettingsField::TopLogprobs => self.top_logprobs.push(ch),
                SettingsField::UserId => self.user_id.push(ch),
            }
        }
    }

    /// 粘贴文本到当前活动字段
    pub fn paste_text(&mut self, text: &str) {
        if let Some(field) = self.active_field {
            match field {
                SettingsField::Provider => self.provider.push_str(text),
                SettingsField::ApiKey => self.api_key.push_str(text),
                SettingsField::BaseUrl => self.base_url.push_str(text),
                SettingsField::Model => self.model.push_str(text),
                SettingsField::DisplayName => self.display_name.push_str(text),
                SettingsField::Temperature => self.temperature.push_str(text),
                SettingsField::MaxTokens => self.max_tokens.push_str(text),
                SettingsField::MaxInputTokens => self.max_input_tokens.push_str(text),
                SettingsField::SystemPrompt => self.system_prompt.push_str(text),
                SettingsField::Stop => self.stop.push_str(text),
                SettingsField::TopLogprobs => self.top_logprobs.push_str(text),
                SettingsField::UserId => self.user_id.push_str(text),
            }
        }
    }

    /// 退格
    pub fn backspace(&mut self) {
        if let Some(field) = self.active_field {
            match field {
                SettingsField::Provider => {
                    self.provider.pop();
                }
                SettingsField::ApiKey => {
                    self.api_key.pop();
                }
                SettingsField::BaseUrl => {
                    self.base_url.pop();
                }
                SettingsField::Model => {
                    self.model.pop();
                }
                SettingsField::DisplayName => {
                    self.display_name.pop();
                }
                SettingsField::Temperature => {
                    self.temperature.pop();
                }
                SettingsField::MaxTokens => {
                    self.max_tokens.pop();
                }
                SettingsField::MaxInputTokens => {
                    self.max_input_tokens.pop();
                }
                SettingsField::SystemPrompt => {
                    self.system_prompt.pop();
                }
                SettingsField::Stop => {
                    self.stop.pop();
                }
                SettingsField::TopLogprobs => {
                    self.top_logprobs.pop();
                }
                SettingsField::UserId => {
                    self.user_id.pop();
                }
            }
        }
    }

    /// UI-M05: Delete 键清除活动字段（区别于 Backspace 删除末尾字符）
    pub fn delete_forward(&mut self) {
        if let Some(field) = self.active_field {
            match field {
                SettingsField::Provider => self.provider.clear(),
                SettingsField::ApiKey => self.api_key.clear(),
                SettingsField::BaseUrl => self.base_url.clear(),
                SettingsField::Model => self.model.clear(),
                SettingsField::DisplayName => self.display_name.clear(),
                SettingsField::Temperature => self.temperature.clear(),
                SettingsField::MaxTokens => self.max_tokens.clear(),
                SettingsField::MaxInputTokens => self.max_input_tokens.clear(),
                SettingsField::SystemPrompt => self.system_prompt.clear(),
                SettingsField::Stop => self.stop.clear(),
                SettingsField::TopLogprobs => self.top_logprobs.clear(),
                SettingsField::UserId => self.user_id.clear(),
            }
        }
    }

    /// 当前可用键盘聚焦的字段序列。
    /// 下拉框（Provider / Model）由鼠标操作，不参与 Tab 循环；
    /// BaseUrl 仅在自定义服务商模式下可编辑。
    fn focusable_fields(&self) -> Vec<SettingsField> {
        let mut fields = vec![SettingsField::ApiKey];
        if self.provider == "custom" {
            fields.push(SettingsField::BaseUrl);
        }
        // 自定义名称在模型下拉框之后，参与 Tab 循环
        fields.push(SettingsField::DisplayName);
        // 温度已改为滑块交互，不参与键盘 Tab 循环
        fields.push(SettingsField::MaxInputTokens);
        fields.push(SettingsField::MaxTokens);
        fields.push(SettingsField::SystemPrompt);
        // 开发者参数区展开时，其文本字段加入 Tab 循环
        if self.dev_params_expanded {
            fields.push(SettingsField::Stop);
            if self.logprobs {
                fields.push(SettingsField::TopLogprobs);
            }
            fields.push(SettingsField::UserId);
        }
        fields
    }

    pub fn next_field(&mut self) {
        let fields = self.focusable_fields();
        self.active_field = match self.active_field {
            None => fields.first().copied(),
            Some(cur) => match fields.iter().position(|f| *f == cur) {
                Some(i) if i + 1 < fields.len() => Some(fields[i + 1]),
                _ => None,
            },
        };
    }

    pub fn prev_field(&mut self) {
        let fields = self.focusable_fields();
        self.active_field = match self.active_field {
            None => fields.last().copied(),
            Some(cur) => match fields.iter().position(|f| *f == cur) {
                Some(i) if i > 0 => Some(fields[i - 1]),
                _ => None,
            },
        };
    }

    /// 掩码 API 密钥用于显示：隐藏态下全部字符都以圆点遮盖，不泄露任何明文字符。
    pub fn masked_api_key(&self) -> String {
        "•".repeat(self.api_key.chars().count())
    }

    /// 显示用的 API 密钥文本：显隐开关打开时明文，否则掩码
    pub fn display_api_key(&self) -> String {
        if self.show_api_key {
            self.api_key.clone()
        } else {
            self.masked_api_key()
        }
    }

    /// 切换 API 密钥显隐
    pub fn toggle_api_key_visibility(&mut self) {
        self.show_api_key = !self.show_api_key;
    }

    /// 温度是否合法（0.0-2.0）
    pub fn temperature_valid(&self) -> bool {
        matches!(self.temperature.trim().parse::<f32>(), Ok(v) if (0.0..=2.0).contains(&v))
    }

    /// Top-p 是否合法（0.0-1.0）
    pub fn top_p_valid(&self) -> bool {
        matches!(self.top_p.trim().parse::<f32>(), Ok(v) if (0.0..=1.0).contains(&v))
    }

    /// 频率惩罚是否合法（-2.0 ~ 2.0）
    pub fn frequency_penalty_valid(&self) -> bool {
        matches!(self.frequency_penalty.trim().parse::<f32>(), Ok(v) if (-2.0..=2.0).contains(&v))
    }

    /// 存在惩罚是否合法（-2.0 ~ 2.0）
    pub fn presence_penalty_valid(&self) -> bool {
        matches!(self.presence_penalty.trim().parse::<f32>(), Ok(v) if (-2.0..=2.0).contains(&v))
    }

    /// top_logprobs 是否合法（空=不下发，或 0-20 的整数）
    pub fn top_logprobs_valid(&self) -> bool {
        let t = self.top_logprobs.trim();
        t.is_empty() || matches!(t.parse::<u32>(), Ok(v) if v <= 20)
    }

    /// Max Tokens 是否合法（1..=1_000_000 的正整数）
    pub fn max_tokens_valid(&self) -> bool {
        matches!(self.max_tokens.trim().parse::<u32>(), Ok(v) if (1..=1_000_000).contains(&v))
    }

    /// 最大输入 Token 是否合法（1..=2_000_000 的正整数，容纳大上下文模型）
    pub fn max_input_tokens_valid(&self) -> bool {
        matches!(self.max_input_tokens.trim().parse::<u32>(), Ok(v) if (1..=2_000_000).contains(&v))
    }

    /// 滚动内容（delta>0 向下），并夹紧到有效范围
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_offset = (self.scroll_offset + delta).clamp(0.0, self.content_height.max(0.0));
    }

    /// 夹紧滚动偏移（内容高度变化后调用）
    pub fn clamp_scroll(&mut self) {
        self.scroll_offset = self.scroll_offset.clamp(0.0, self.content_height.max(0.0));
    }

    /// 命中：API 密钥显隐按钮
    pub fn hit_test_api_key_toggle(&self, x: f32, y: f32) -> bool {
        if let Some((rx, ry, rw, rh)) = self.api_key_toggle_region {
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        } else {
            false
        }
    }

    /// 命中：深度思考开关
    pub fn hit_test_thinking_toggle(&self, x: f32, y: f32) -> bool {
        if let Some((rx, ry, rw, rh)) = self.thinking_toggle_region {
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        } else {
            false
        }
    }

    /// 切换深度思考开关
    pub fn toggle_thinking(&mut self) {
        self.thinking = !self.thinking;
    }

    /// 命中：温度滑块轨道，返回点击位置对应的温度（0.0-2.0，步进 0.1）
    pub fn hit_test_temp_slider(&self, x: f32, y: f32) -> Option<f32> {
        if let Some((rx, ry, rw, rh)) = self.temp_slider_region {
            if x >= rx - 4.0 && x <= rx + rw + 4.0 && y >= ry - 8.0 && y <= ry + rh + 8.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                let val = (ratio * 2.0 * 10.0).round() / 10.0;
                return Some(val);
            }
        }
        None
    }

    /// 拖拽中根据鼠标 x 更新温度（忽略 y，超出轨道两端自动夹紧）。返回是否有变化。
    pub fn set_temperature_from_slider_x(&mut self, x: f32) -> bool {
        if let Some((rx, _ry, rw, _rh)) = self.temp_slider_region {
            if rw > 0.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                let val = (ratio * 2.0 * 10.0).round() / 10.0;
                let new_str = format!("{:.1}", val);
                if self.temperature != new_str {
                    self.temperature = new_str;
                    return true;
                }
            }
        }
        false
    }

    /// 命中：Top-p 滑块轨道，返回点击位置对应的 top_p（0.0-1.0，步进 0.05）
    pub fn hit_test_top_p_slider(&self, x: f32, y: f32) -> Option<f32> {
        if let Some((rx, ry, rw, rh)) = self.top_p_slider_region {
            if x >= rx - 4.0 && x <= rx + rw + 4.0 && y >= ry - 8.0 && y <= ry + rh + 8.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                let val = (ratio * 20.0).round() / 20.0;
                return Some(val);
            }
        }
        None
    }

    /// 拖拽中根据鼠标 x 更新 top_p（忽略 y，超出轨道两端自动夹紧）。返回是否有变化。
    pub fn set_top_p_from_slider_x(&mut self, x: f32) -> bool {
        if let Some((rx, _ry, rw, _rh)) = self.top_p_slider_region {
            if rw > 0.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                let val = (ratio * 20.0).round() / 20.0;
                let new_str = format!("{:.2}", val);
                if self.top_p != new_str {
                    self.top_p = new_str;
                    return true;
                }
            }
        }
        false
    }

    /// 命中：思考强度分段按钮，返回命中的强度值（"high"/"max"）
    pub fn hit_test_effort(&self, x: f32, y: f32) -> Option<&'static str> {
        for (effort, ex, ey, ew, eh) in &self.effort_regions {
            if x >= *ex && x < ex + ew && y >= *ey && y < ey + eh {
                return Some(effort);
            }
        }
        None
    }

    /// 命中：响应格式分段按钮，返回命中的格式值（"text"/"json_object"）
    pub fn hit_test_response_format(&self, x: f32, y: f32) -> Option<&'static str> {
        for (fmt, fx, fy, fw, fh) in &self.response_format_regions {
            if x >= *fx && x < fx + fw && y >= *fy && y < fy + fh {
                return Some(fmt);
            }
        }
        None
    }

    /// 命中：开发者参数区标题（展开/折叠）
    pub fn hit_test_dev_params_toggle(&self, x: f32, y: f32) -> bool {
        if let Some((rx, ry, rw, rh)) = self.dev_params_toggle_region {
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        } else {
            false
        }
    }

    /// 命中：流式用量统计开关
    pub fn hit_test_include_usage_toggle(&self, x: f32, y: f32) -> bool {
        if let Some((rx, ry, rw, rh)) = self.include_usage_toggle_region {
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        } else {
            false
        }
    }

    /// 命中：logprobs 调试开关
    pub fn hit_test_logprobs_toggle(&self, x: f32, y: f32) -> bool {
        if let Some((rx, ry, rw, rh)) = self.logprobs_toggle_region {
            x >= rx && x < rx + rw && y >= ry && y < ry + rh
        } else {
            false
        }
    }

    /// 命中：频率惩罚滑块轨道，返回点击位置对应的值（-2.0 ~ 2.0，步进 0.1）
    pub fn hit_test_freq_slider(&self, x: f32, y: f32) -> Option<f32> {
        Self::hit_test_penalty_slider(self.freq_slider_region, x, y)
    }

    /// 命中：存在惩罚滑块轨道，返回点击位置对应的值（-2.0 ~ 2.0，步进 0.1）
    pub fn hit_test_pres_slider(&self, x: f32, y: f32) -> Option<f32> {
        Self::hit_test_penalty_slider(self.pres_slider_region, x, y)
    }

    /// 惩罚滑块公共命中逻辑：轨道周边扩展区内映射到 -2.0 ~ 2.0（步进 0.1）
    fn hit_test_penalty_slider(
        region: Option<(f32, f32, f32, f32)>,
        x: f32,
        y: f32,
    ) -> Option<f32> {
        if let Some((rx, ry, rw, rh)) = region {
            if x >= rx - 4.0 && x <= rx + rw + 4.0 && y >= ry - 8.0 && y <= ry + rh + 8.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                let val = ((ratio * 4.0 - 2.0) * 10.0).round() / 10.0;
                return Some(val);
            }
        }
        None
    }

    /// 拖拽中根据鼠标 x 更新频率惩罚。返回是否有变化。
    pub fn set_freq_from_slider_x(&mut self, x: f32) -> bool {
        if let Some(val) = Self::penalty_from_slider_x(self.freq_slider_region, x) {
            let new_str = format!("{:.1}", val);
            if self.frequency_penalty != new_str {
                self.frequency_penalty = new_str;
                return true;
            }
        }
        false
    }

    /// 拖拽中根据鼠标 x 更新存在惩罚。返回是否有变化。
    pub fn set_pres_from_slider_x(&mut self, x: f32) -> bool {
        if let Some(val) = Self::penalty_from_slider_x(self.pres_slider_region, x) {
            let new_str = format!("{:.1}", val);
            if self.presence_penalty != new_str {
                self.presence_penalty = new_str;
                return true;
            }
        }
        false
    }

    /// 惩罚滑块拖拽公共逻辑：忽略 y，超出轨道两端自动夹紧
    fn penalty_from_slider_x(region: Option<(f32, f32, f32, f32)>, x: f32) -> Option<f32> {
        if let Some((rx, _ry, rw, _rh)) = region {
            if rw > 0.0 {
                let ratio = ((x - rx) / rw).clamp(0.0, 1.0);
                return Some(((ratio * 4.0 - 2.0) * 10.0).round() / 10.0);
            }
        }
        None
    }

    /// 采样参数（temperature/top_p）是否被思考模式禁用：
    /// DeepSeek 思考模式下采样参数不生效（官方文档）
    pub fn sampling_disabled_by_thinking(&self) -> bool {
        self.provider == "deepseek" && self.thinking
    }

    /// 当前 AI 配置相对打开时的快照是否有未保存更改
    pub fn is_dirty(&self) -> bool {
        match &self.baseline_ai {
            Some(baseline) => self.to_ai_settings() != *baseline,
            None => false,
        }
    }

    /// 标记当前配置为已保存（更新基准快照）
    pub fn mark_saved(&mut self) {
        self.baseline_ai = Some(self.to_ai_settings());
    }

    /// 若当前配置（provider|base|key）尚未拉取过模型列表，则后台异步拉取一次。
    /// 用于打开模型下拉时"自动搜索"厂商当前可用模型；失败/无 key 时静默回退预置清单。
    pub fn ensure_models_fetched(&mut self) {
        if self.is_fetching_models {
            return;
        }
        if self.api_key.trim().is_empty() {
            return; // 无密钥无法查询，静默用预置
        }
        let key = self.models_cache_key();
        // 已拉取过（成功且列表非空，或已尝试过同一配置）则不重复请求
        if self.fetched_models_key == key {
            return;
        }
        let ai = self.to_ai_settings();
        self.is_fetching_models = true;
        self.fetched_models_key = key; // 标记"已针对此配置发起过"，避免重复请求
        self.models_fetch_status = "正在获取模型列表…".to_string();
        if let Ok(mut slot) = self.models_fetch_result.lock() {
            *slot = None;
        }
        let result = Arc::clone(&self.models_fetch_result);
        std::thread::spawn(move || {
            let client = aether_ai::AiClient::new(&ai);
            let r = client.list_models_safe();
            if let Ok(mut slot) = result.lock() {
                *slot = Some(r);
            }
        });
    }

    /// 轮询后台模型拉取结果（渲染循环每帧调用）。返回是否发生了状态变化（用于触发重绘）。
    pub fn poll_models_fetch(&mut self) -> bool {
        if !self.is_fetching_models {
            return false;
        }
        let outcome = self
            .models_fetch_result
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        match outcome {
            Some(Ok(models)) => {
                self.is_fetching_models = false;
                if models.is_empty() {
                    self.models_fetch_status = "该服务商未返回模型".to_string();
                } else {
                    self.fetched_models = models;
                    self.models_fetch_status.clear();
                }
                true
            }
            Some(Err(e)) => {
                self.is_fetching_models = false;
                self.models_fetch_status = format!("获取模型失败：{}", e);
                // 保留 fetched_models_key 以免立即重试；用户改配置后 key 变化会重新拉取
                true
            }
            None => false,
        }
    }

    // 模型管理方法

    pub fn provider_display_label(&self) -> String {
        match self.provider.as_str() {
            "kimi" => "Kimi".to_string(),
            "deepseek" => "DeepSeek".to_string(),
            _ => self.provider.clone(),
        }
    }

    pub fn provider_dropdown_options() -> Vec<(&'static str, &'static str)> {
        vec![
            ("deepseek", "DeepSeek"),
            ("kimi", "Kimi"),
            ("custom", "自定义"),
        ]
    }

    pub fn model_dropdown_options(&self) -> Vec<(String, String)> {
        // 优先使用从 /models 拉取到的实时模型清单（仅当对应当前配置指纹时）；
        // 未拉取/失败时回退到核心层 preset_models；Custom 无预置则回退为当前已填模型。
        if !self.fetched_models.is_empty() && self.fetched_models_key == self.models_cache_key() {
            return self
                .fetched_models
                .iter()
                .map(|m| (m.clone(), m.clone()))
                .collect();
        }
        let presets = aether_ai::AiProvider::from_str(&self.provider).preset_models();
        if presets.is_empty() {
            vec![(self.model.clone(), self.model.clone())]
        } else {
            presets
                .iter()
                .map(|m| (m.to_string(), m.to_string()))
                .collect()
        }
    }

    /// 当前配置指纹（provider|base|key），用于判断已拉取的模型清单是否仍适用。
    pub fn models_cache_key(&self) -> String {
        format!("{}|{}|{}", self.provider, self.base_url, self.api_key)
    }

    pub fn poll_test_result(&mut self) -> TestPollResult {
        if !self.is_testing {
            return TestPollResult::Pending;
        }
        let outcome = self
            .test_result
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        match outcome {
            Some(Ok(reply)) => {
                let snippet: String = reply.chars().take(60).collect();
                self.test_status = format!("✓ 连接成功：{}", snippet.trim());
                self.is_testing = false;
                if self.pending_save {
                    self.pending_save = false;
                    TestPollResult::SuccessWithPendingSave
                } else {
                    TestPollResult::Success
                }
            }
            Some(Err(e)) => {
                self.test_status = format!("✗ 连接失败：{}", e);
                self.is_testing = false;
                if self.pending_save {
                    self.pending_save = false;
                    TestPollResult::FailedWithPendingSave
                } else {
                    TestPollResult::Failed
                }
            }
            None => TestPollResult::Pending,
        }
    }

    /// 启动后台测试连接（非阻塞，HTTP 请求在后台线程执行）
    pub fn start_test_connection(&mut self, ai: AiSettings) {
        if self.is_testing {
            return;
        }
        if ai.api_key.trim().is_empty() {
            self.test_status = "✗ 请先填写 API 密钥".to_string();
            return;
        }
        self.is_testing = true;
        self.test_status = "正在测试连接…".to_string();
        if let Ok(mut slot) = self.test_result.lock() {
            *slot = None;
        }
        let result = Arc::clone(&self.test_result);
        std::thread::spawn(move || {
            let client = aether_ai::AiClient::new(&ai);
            let r = client.test_connection_safe();
            if let Ok(mut slot) = result.lock() {
                *slot = Some(r);
            }
        });
    }

    /// 命中检测：下拉触发区
    pub fn hit_test_dropdown_trigger(&self, x: f32, y: f32) -> Option<SettingsDropdownKind> {
        for (kind, tx, ty, tw, th) in &self.dropdown_trigger_regions {
            if x >= *tx && x < tx + tw && y >= *ty && y < ty + th {
                return Some(*kind);
            }
        }
        None
    }

    /// 命中检测：下拉选项（返回类型与索引）
    pub fn hit_test_dropdown_item(&self, x: f32, y: f32) -> Option<(SettingsDropdownKind, usize)> {
        for (kind, idx, ix, iy, iw, ih) in &self.dropdown_item_regions {
            if x >= *ix && x < ix + iw && y >= *iy && y < iy + ih {
                return Some((*kind, *idx));
            }
        }
        None
    }

    /// 按下拉索引选择服务商，并将模型重置为该服务商的第一个预置模型
    pub fn select_provider_by_index(&mut self, idx: usize) {
        let opts = Self::provider_dropdown_options();
        if let Some((id, _)) = opts.get(idx) {
            self.provider = id.to_string();
            // 根据选择的厂商自动设置 base_url
            match *id {
                "deepseek" => {
                    self.base_url = "https://api.deepseek.com/v1".to_string();
                }
                "kimi" => {
                    self.base_url = "https://api.moonshot.cn/v1".to_string();
                }
                _ => {
                    self.base_url = String::new();
                }
            }
            if let Some((mid, _)) = self.model_dropdown_options().first() {
                self.model = mid.clone();
            }
        }
    }

    /// 按下拉索引选择模型
    pub fn select_model_by_index(&mut self, idx: usize) {
        let opts = self.model_dropdown_options();
        if let Some((mid, _)) = opts.get(idx) {
            self.model = mid.clone();
        }
    }

    pub fn add_model_button_region(&mut self, button: ModelButton, x: f32, y: f32, w: f32, h: f32) {
        self.model_button_regions.push((button, x, y, w, h));
    }

    pub fn add_model_item_region(&mut self, id: String, x: f32, y: f32, w: f32, h: f32) {
        self.model_item_regions.push((id, x, y, w, h));
    }

    /// 命中检测：模型项（返回模型ID）
    pub fn hit_test_model_item(&self, x: f32, y: f32) -> Option<String> {
        for (id, ix, iy, iw, ih) in &self.model_item_regions {
            if x >= *ix && x < ix + iw && y >= *iy && y < iy + ih {
                return Some(id.clone());
            }
        }
        None
    }

    /// 命中检测：模型按钮（返回按钮类型和模型ID）
    pub fn hit_test_model_button(&self, x: f32, y: f32) -> Option<(ModelButton, String)> {
        for (button, bx, by, bw, bh) in &self.model_button_regions {
            if x >= *bx && x < bx + bw && y >= *by && y < by + bh {
                // 找到对应的模型项ID
                if let Some(model_id) = self.hit_test_model_item(x, y) {
                    return Some((*button, model_id));
                }
                // 如果是添加按钮，没有对应的模型项
                if *button == ModelButton::Add {
                    return Some((*button, String::new()));
                }
            }
        }
        None
    }

    /// 切换模型启用状态
    pub fn toggle_model_enabled(&mut self, model_id: &str) {
        if let Some(model) = self.models.iter_mut().find(|m| m.id == model_id) {
            model.enabled = !model.enabled;
        }
    }

    /// 删除模型
    pub fn delete_model(&mut self, model_id: &str) {
        self.models.retain(|m| m.id != model_id);
        if self.active_model_id.as_deref() == Some(model_id) {
            self.active_model_id = None;
        }
        if self.selected_model_id.as_deref() == Some(model_id) {
            self.selected_model_id = None;
        }
    }

    pub fn current_provider_button(&self) -> Option<ProviderTemplateButton> {
        match self.provider.as_str() {
            "deepseek" => Some(ProviderTemplateButton::DeepSeek),
            "kimi" => Some(ProviderTemplateButton::Kimi),
            _ => None,
        }
    }
}
