use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    /// 旧版单一 AI 配置：仅作为反序列化的迁移载体（load_from 会把它转为模型档案），
    /// 不再写回 settings.json，运行时请一律走 active_ai_settings()。
    #[serde(skip_serializing)]
    pub ai: AiSettings,
    pub ui: UiSettings,
    pub remote: RemoteSettings,
    pub auto_save: AutoSaveSettings,
    /// 多模型配置列表（每个模型独立 provider/key/参数）
    #[serde(default)]
    pub ai_models: Vec<AiModelProfile>,
    /// 当前激活的模型 ID
    #[serde(default)]
    pub active_model_id: Option<String>,
    /// 自动更新设置
    #[serde(default)]
    pub update: UpdateSettings,
}

impl std::fmt::Debug for AppSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppSettings")
            .field("ai", &self.ai)
            .field("ui", &self.ui)
            .field("remote", &self.remote)
            .field("auto_save", &self.auto_save)
            .field("ai_models", &self.ai_models)
            .field("active_model_id", &self.active_model_id)
            .field("update", &self.update)
            .finish()
    }
}

/// 自动保存配置
///
/// 设计原则：组合式触发（防抖空闲 + 失焦 + 周期兜底），原子写入，
/// 内容去重，外部修改检测（mtime 轮询）。大文件智能降级。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AutoSaveSettings {
    /// 总开关
    pub enabled: bool,
    /// 空闲防抖延迟（毫秒）。用户停止输入后等待此时间再保存。
    pub debounce_ms: u32,
    /// 失焦立即保存
    pub focus_loss_save: bool,
    /// 周期强制保存间隔（毫秒）。0 表示关闭周期兜底。
    pub periodic_save_ms: u32,
    /// 大文件阈值（字节）。超过此大小的文件采用更长的防抖、关闭周期保存。
    pub large_file_threshold: u64,
    /// 大文件防抖延迟（毫秒）
    pub large_file_debounce_ms: u32,
}

impl Default for AutoSaveSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 1000,
            focus_loss_save: true,
            periodic_save_ms: 30_000,
            large_file_threshold: 2 * 1024 * 1024,
            large_file_debounce_ms: 5000,
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct AiSettings {
    pub provider: String,
    // C-10: api_key 不再序列化到 settings.json，改为 DPAPI 加密单独存储
    #[serde(skip_serializing, default)]
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub temperature: Option<f32>,
    /// 核采样 top_p（0.0-1.0）：None 表示不下发该参数（用服务端默认 1.0）。
    /// 注：DeepSeek 思考模式下采样参数（temperature/top_p）不生效。
    #[serde(default)]
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    /// 最大输入 Token（上下文窗口预算）：限制发送给模型的历史上下文量。
    /// None 表示用内置默认预算。
    #[serde(default)]
    pub max_input_tokens: Option<u32>,
    pub system_prompt: Option<String>,
    /// 深度思考开关（仅 DeepSeek V4 生效）：Some(true)=思考模式，Some(false)=非思考模式，
    /// None=不下发该参数（用服务端默认，即开启）。
    #[serde(default)]
    pub thinking: Option<bool>,
    /// 思考强度（仅 DeepSeek 思考模式生效）："high"（默认）或 "max"，
    /// None=不下发该参数（服务端默认 high）。
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// 频率惩罚（-2.0 ~ 2.0，默认 0）：抑制逐字重复；思考模式下不生效
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    /// 存在惩罚（-2.0 ~ 2.0，默认 0）：鼓励新话题；思考模式下不生效
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    /// 停止序列（最多 16 个）：生成遇到即停止；None/空=不下发
    #[serde(default)]
    pub stop: Option<Vec<String>>,
    /// 响应格式：Some("json_object")=强制 JSON 输出；None=文本（默认）
    #[serde(default)]
    pub response_format: Option<String>,
    /// 业务侧用户标识（内容安全/缓存隔离/限速调度）；None/空=不下发
    #[serde(default)]
    pub user_id: Option<String>,
    /// 该模型是否支持多模态（图片）输入：用户在模型编辑表单中勾选。
    /// 开启后 AI 输入框允许附加图片，以 OpenAI 兼容 image_url 块随请求发送。
    #[serde(default)]
    pub multimodal: bool,
}

impl std::fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSettings")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("top_p", &self.top_p)
            .field("max_tokens", &self.max_tokens)
            .field("max_input_tokens", &self.max_input_tokens)
            .field(
                "system_prompt",
                &self.system_prompt.as_deref().map(|_| "[PRESENT]"),
            )
            .field("thinking", &self.thinking)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("frequency_penalty", &self.frequency_penalty)
            .field("presence_penalty", &self.presence_penalty)
            .field("stop", &self.stop)
            .field("response_format", &self.response_format)
            .field("user_id", &self.user_id)
            .field("multimodal", &self.multimodal)
            .finish()
    }
}

/// 单个 AI 模型配置档案（多模型架构）。
/// 运行参数内嵌 AiSettings（flatten 保持 JSON 形状不变），
/// API Key 不序列化到 settings.json，集中加密存储于 api_key.enc。
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct AiModelProfile {
    pub id: String,
    pub display_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 全部运行参数（含 api_key；flatten 内联序列化，api_key 自身 skip）
    #[serde(flatten)]
    pub settings: AiSettings,
}

fn default_true() -> bool {
    true
}

impl AiModelProfile {
    /// 转换为运行时 AiSettings
    pub fn to_ai_settings(&self) -> AiSettings {
        self.settings.clone()
    }
}

impl std::fmt::Debug for AiModelProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiModelProfile")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("enabled", &self.enabled)
            .field("settings", &self.settings)
            .finish()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct UiSettings {
    pub theme: String,
    pub font_size: u32,
    pub sidebar_visible: bool,
    /// 活动栏图标顺序（持久化键列表，空表示使用默认顺序）
    #[serde(default)]
    pub activity_bar_order: Vec<String>,
    /// 菜单栏顶部项顺序（持久化键列表，空表示使用默认顺序）
    #[serde(default)]
    pub menu_bar_order: Vec<String>,
    /// 主窗口左上角 X 坐标（屏幕坐标），None 表示使用默认位置
    #[serde(default)]
    pub window_x: Option<i32>,
    /// 主窗口左上角 Y 坐标（屏幕坐标），None 表示使用默认位置
    #[serde(default)]
    pub window_y: Option<i32>,
    /// 主窗口宽度（像素），None 表示使用默认尺寸
    #[serde(default)]
    pub window_width: Option<u32>,
    /// 主窗口高度（像素），None 表示使用默认尺寸
    #[serde(default)]
    pub window_height: Option<u32>,
    /// 主窗口是否最大化
    #[serde(default)]
    pub window_maximized: bool,
    /// 上次打开的工作区路径，None 表示未打开任何工作区
    #[serde(default)]
    pub last_workspace: Option<PathBuf>,
    /// 最大化时是否显示 Windows 任务栏（默认 true）
    #[serde(default = "default_true")]
    pub show_taskbar_when_maximized: bool,
    /// 各工作区退出时仍打开的 AI 对话标签页（工作区哈希 → 标签页快照）
    #[serde(default)]
    pub ai_open_tabs: std::collections::HashMap<String, AiOpenTabsSnapshot>,
    /// 编辑器模式：开发者模式（默认）或智能体模式
    /// 智能体模式下 AI 对话面板在左侧为主体，文件编辑区移至右侧，设置以弹窗打开
    #[serde(default)]
    pub editor_mode: EditorMode,
}

/// 编辑器模式（持久化到 settings.json）
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorMode {
    /// 开发者模式：传统 IDE 布局，AI 面板在右侧
    #[default]
    Developer,
    /// 智能体模式：AI 对话为主体在左侧，编辑器在右侧
    Agent,
}

impl EditorMode {
    /// 是否为智能体模式
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent)
    }
}

/// AI 面板打开标签页的快照（持久化到 settings.json）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AiOpenTabsSnapshot {
    /// 打开的对话 ID 列表（按标签页顺序）
    pub conversation_ids: Vec<String>,
    /// 活动标签页在 conversation_ids 中的索引
    pub active: usize,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: String::new(),
            font_size: 0,
            sidebar_visible: false,
            activity_bar_order: Vec::new(),
            menu_bar_order: Vec::new(),
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_maximized: false,
            last_workspace: None,
            show_taskbar_when_maximized: true,
            ai_open_tabs: std::collections::HashMap::new(),
            editor_mode: EditorMode::default(),
        }
    }
}

/// SSH 服务器配置（持久化到 settings.json）
/// 密码/passphrase 不持久化（安全考虑），连接时由用户输入
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct SshServerConfig {
    /// 服务器显示名称
    pub name: String,
    /// 主机地址（IP 或域名）
    pub host: String,
    /// SSH 端口
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// 登录用户名
    pub username: String,
    /// 认证方法（默认 Agent；未知值反序列化为 Fallback，语义等同 Agent）
    #[serde(default)]
    pub auth_type: SshAuthType,
    /// 密钥文件路径（auth_type == Key 时使用）
    #[serde(default)]
    pub key_path: String,
}

/// SSH 认证方法（持久化到 settings.json）
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthType {
    /// 密码认证（shell out 模式不支持，仅兼容旧配置保留）
    Password,
    /// 密钥文件认证
    Key,
    /// ssh-agent 认证（默认）
    #[default]
    Agent,
    /// 未知认证方式（旧配置兼容兑底，行为等同 Agent；各消费点应与 Agent 同样处理）
    #[serde(other)]
    Fallback,
}

impl SshAuthType {
    /// 是否为密钥认证
    pub fn is_key(&self) -> bool {
        matches!(self, Self::Key)
    }

    /// 是否为密码认证
    pub fn is_password(&self) -> bool {
        matches!(self, Self::Password)
    }
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct RemoteSettings {
    /// 已保存的 SSH 服务器配置列表
    #[serde(default)]
    pub ssh_servers: Vec<SshServerConfig>,
}

impl AppSettings {
    /// 返回当前激活模型的 AI 设置副本（供 AI 面板/运行时调用）。
    /// 优先级：激活模型 → 首个启用模型 → 回退旧的单一 ai。
    pub fn active_ai_settings(&self) -> AiSettings {
        if let Some(id) = &self.active_model_id {
            if let Some(m) = self.ai_models.iter().find(|m| &m.id == id && m.enabled) {
                return m.to_ai_settings();
            }
        }
        if let Some(m) = self.ai_models.iter().find(|m| m.enabled) {
            return m.to_ai_settings();
        }
        self.ai.clone()
    }

    /// 返回当前激活模型的显示名称（供 AI 面板显示）。
    /// 优先级：display_name → model → "未配置模型"。
    pub fn active_model_display_name(&self) -> String {
        if let Some(id) = &self.active_model_id {
            if let Some(m) = self.ai_models.iter().find(|m| &m.id == id && m.enabled) {
                if !m.display_name.is_empty() {
                    return m.display_name.clone();
                }
                if !m.settings.model.is_empty() {
                    return m.settings.model.clone();
                }
            }
        }
        if let Some(m) = self.ai_models.iter().find(|m| m.enabled) {
            if !m.display_name.is_empty() {
                return m.display_name.clone();
            }
            if !m.settings.model.is_empty() {
                return m.settings.model.clone();
            }
        }
        "未配置模型".to_string()
    }

    pub fn settings_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        let aether_dir = config_dir.join("Aether");
        if let Err(e) = std::fs::create_dir_all(&aether_dir) {
            eprintln!("警告: 无法创建配置目录 {}: {}", aether_dir.display(), e);
        }
        aether_dir.join("settings.json")
    }

    /// C-10: 加密后的 API 密钥存储路径
    pub fn api_key_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        let aether_dir = config_dir.join("Aether");
        let _ = std::fs::create_dir_all(&aether_dir);
        aether_dir.join("api_key.enc")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::settings_path(), &Self::api_key_path())
    }

    fn load_from(settings_path: &std::path::Path, api_key_path: &std::path::Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(settings_path) {
            match serde_json::from_str::<AppSettings>(&content) {
                Ok(mut settings) => {
                    // C-10 + 多模型：解密 API Key 存储。
                    // 新格式为 JSON map（model_id -> key）；旧格式为单个密钥（JSON 字符串或裸文本）。
                    let mut legacy_single_key: Option<String> = None;
                    if let Ok(encrypted) = std::fs::read(api_key_path) {
                        if let Ok(decrypted) = decrypt_api_key(&encrypted) {
                            if let Ok(map) = serde_json::from_str::<
                                std::collections::BTreeMap<String, String>,
                            >(&decrypted)
                            {
                                for m in settings.ai_models.iter_mut() {
                                    if let Some(k) = map.get(&m.id) {
                                        m.settings.api_key = k.clone();
                                    }
                                }
                            } else if let Ok(single) = serde_json::from_str::<String>(&decrypted) {
                                legacy_single_key = Some(single);
                            } else {
                                // 裸文本旧格式（解密成功即为本程序写入的数据）
                                legacy_single_key = Some(decrypted);
                            }
                        }
                    }

                    // 迁移：旧版单一 ai 配置（尚无 ai_models）→ 生成默认模型档案。
                    // 仅当磁盘 JSON 确实携带 "ai" 对象时迁移（新保存已不再写出该字段）。
                    if settings.ai_models.is_empty()
                        && serde_json::from_str::<serde_json::Value>(&content)
                            .map(|v| v.get("ai").is_some_and(|a| a.is_object()))
                            .unwrap_or(false)
                    {
                        let mut migrated = std::mem::take(&mut settings.ai);
                        if migrated.model.is_empty() {
                            migrated.model = "deepseek-v4-pro".to_string();
                        }
                        if let Some(k) = legacy_single_key.take() {
                            migrated.api_key = k;
                        }
                        let display = migrated.model.clone();
                        settings.ai_models.push(AiModelProfile {
                            id: "legacy-ai".to_string(),
                            display_name: display,
                            enabled: true,
                            settings: migrated,
                        });
                        settings.active_model_id = Some("legacy-ai".to_string());
                    }

                    return settings;
                }
                Err(e) => {
                    // M-13: JSON 损坏时记录警告并备份原文件，避免用户在不知情下丢失设置
                    eprintln!("[M-13] 警告: settings.json 解析失败，回退到默认设置: {}", e);
                    let backup = settings_path.with_extension("json.corrupt");
                    if std::fs::rename(settings_path, &backup).is_ok() {
                        eprintln!("[M-13] 已将损坏的配置备份到 {}", backup.display());
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let settings_path = Self::settings_path();
        // 常规保存不携带 last_workspace：以磁盘现值为准。
        // 各窗口持有创建时加载的整份内存副本，若任由 save() 写入该字段，
        // 任何窗口的一次普通保存（切换模型、调整布局等）都会用陈旧值
        // 覆写“最后打开的文件夹”，导致下次启动恢复错误的工作区。
        // last_workspace 仅由 persist_last_workspace() 读改写维护。
        let mut to_save = self.clone();
        if let Some(disk_lw) = Self::read_disk_last_workspace(&settings_path) {
            to_save.ui.last_workspace = disk_lw;
        }
        to_save.save_to(&settings_path, &Self::api_key_path())
    }

    /// 读取磁盘 settings.json 中的 last_workspace 字段。
    /// 外层 None = 文件不存在/解析失败（保留调用方自身值）；
    /// 内层 Option = 磁盘上的实际值（含显式 null）。
    fn read_disk_last_workspace(settings_path: &std::path::Path) -> Option<Option<PathBuf>> {
        let content = std::fs::read_to_string(settings_path).ok()?;
        let parsed = serde_json::from_str::<AppSettings>(&content).ok()?;
        Some(parsed.ui.last_workspace)
    }

    /// 单独持久化“最后打开的工作区”：读盘 → 改字段 → 写盘。
    /// 与 save() 分离，避免各窗口的整份内存副本互相覆写该字段。
    /// 仅由 open_folder（传 Some）与 close_workspace（传 None）调用。
    pub fn persist_last_workspace(workspace: Option<&std::path::Path>) -> std::io::Result<()> {
        let mut disk = Self::load();
        disk.ui.last_workspace = workspace.map(|p| p.to_path_buf());
        disk.save_to(&Self::settings_path(), &Self::api_key_path())
    }

    fn save_to(
        &self,
        path: &std::path::Path,
        api_key_path: &std::path::Path,
    ) -> std::io::Result<()> {
        // C-10: settings.json 不写入明文 api_key（各 api_key 均 skip_serializing），
        // 密钥改为单独 DPAPI 加密存储；旧 ai 字段整体 skip_serializing，不再写回。
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // P1-3: 原子写入——临时文件 + fsync + rename
        // 写入过程中崩溃只会留下临时文件，不会损坏 settings.json。
        // rename 在同卷上是原子操作（Windows MoveFileEx / POSIX rename）。
        let mut tmp_path = path.to_path_buf();
        let mut suffix = std::ffi::OsString::from(".tmp.");
        suffix.push(std::process::id().to_string());
        suffix.push(".");
        suffix.push(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos().to_string())
                .unwrap_or_else(|_| "0".to_string()),
        );
        tmp_path.set_extension(suffix);

        #[cfg(windows)]
        {
            use std::io::Write;
            use std::os::windows::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .share_mode(0)
                .open(&tmp_path)?;
            let mut writer = std::io::BufWriter::new(file);
            writer.write_all(content.as_bytes())?;
            writer.flush()?;
            // fsync 确保数据落盘后再 rename
            writer.get_ref().sync_all()?;
        }
        #[cfg(not(windows))]
        {
            std::fs::write(&tmp_path, &content)?;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
            // fsync 确保数据落盘后再 rename
            std::fs::File::open(&tmp_path)?.sync_all()?;
        }

        // 原子 rename：旧文件被整体替换，要么成功要么原文件不变
        std::fs::rename(&tmp_path, path).inspect_err(|_e| {
            // rename 失败时清理临时文件，避免残留
            let _ = std::fs::remove_file(&tmp_path);
        })?;

        // C-10 + 多模型：所有模型的 API Key 集中为 JSON map，整体 DPAPI 加密存储
        let mut key_map: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for m in &self.ai_models {
            if !m.settings.api_key.is_empty() {
                key_map.insert(m.id.clone(), m.settings.api_key.clone());
            }
        }
        if key_map.is_empty() {
            let _ = std::fs::remove_file(api_key_path);
        } else if let Ok(json) = serde_json::to_string(&key_map) {
            if let Ok(encrypted) = encrypt_api_key(&json) {
                let _ = std::fs::write(api_key_path, encrypted);
            }
        }

        Ok(())
    }
}

/// C-10: 使用 Windows DPAPI 加密 API 密钥
#[cfg(windows)]
fn encrypt_api_key(api_key: &str) -> std::io::Result<Vec<u8>> {
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_LOCAL_MACHINE, CRYPT_INTEGER_BLOB,
    };

    let bytes = api_key.as_bytes();
    let data_in = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &data_in,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_LOCAL_MACHINE,
            &mut data_out,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let result = slice.to_vec();
        windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
            data_out.pbData as *mut _,
        ));
        Ok(result)
    }
}

/// C-10: 非 Windows 平台回退为 UTF-8 字节（项目主要面向 Windows，此处仅保证编译）
#[cfg(not(windows))]
fn encrypt_api_key(api_key: &str) -> std::io::Result<Vec<u8>> {
    Ok(api_key.as_bytes().to_vec())
}

/// C-10: 使用 Windows DPAPI 解密 API 密钥
#[cfg(windows)]
fn decrypt_api_key(data: &[u8]) -> std::io::Result<String> {
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_LOCAL_MACHINE, CRYPT_INTEGER_BLOB,
    };

    let data_in = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut data_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &data_in,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_LOCAL_MACHINE,
            &mut data_out,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        let slice = std::slice::from_raw_parts(data_out.pbData, data_out.cbData as usize);
        let result = String::from_utf8(slice.to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
            data_out.pbData as *mut _,
        ));
        Ok(result)
    }
}

#[cfg(not(windows))]
fn decrypt_api_key(data: &[u8]) -> std::io::Result<String> {
    String::from_utf8(data.to_vec())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai: AiSettings {
                provider: "deepseek".to_string(),
                api_key: String::new(),
                base_url: None,
                model: "deepseek-v4-pro".to_string(),
                temperature: Some(0.7),
                top_p: None,
                max_tokens: Some(8192),
                max_input_tokens: None,
                system_prompt: None,
                thinking: None,
                reasoning_effort: None,
                frequency_penalty: None,
                presence_penalty: None,
                stop: None,
                response_format: None,
                user_id: None,
                multimodal: false,
            },
            ui: UiSettings::default(),
            remote: RemoteSettings::default(),
            auto_save: AutoSaveSettings::default(),
            ai_models: Vec::new(),
            active_model_id: None,
            update: UpdateSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_test_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|_| "0".to_string());
        dir.push(format!("{}-{}-{}", prefix, std::process::id(), stamp));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_api_key_encryption_roundtrip() {
        // C-10: 验证 DPAPI 加密/解密往返正确
        let key = "sk-test-12345";
        let encrypted = encrypt_api_key(key).expect("加密失败");
        assert_ne!(encrypted, key.as_bytes());
        let decrypted = decrypt_api_key(&encrypted).expect("解密失败");
        assert_eq!(decrypted, key);
    }

    #[test]
    fn test_decrypt_invalid_data_fails() {
        // 非 Windows 平台：解密失败路径表现为非法 UTF-8
        #[cfg(not(windows))]
        {
            let invalid = vec![0xFF, 0xFE, 0xFD];
            assert!(decrypt_api_key(&invalid).is_err());
        }
        // Windows 平台：随机字节无法被 DPAPI 解密
        #[cfg(windows)]
        {
            let invalid = vec![0xDE, 0xAD, 0xBE, 0xEF];
            assert!(decrypt_api_key(&invalid).is_err());
        }
    }

    #[test]
    fn test_settings_save_does_not_include_plaintext_api_key() {
        // C-10: 验证 settings.json 序列化中不包含明文 api_key
        let mut settings = AppSettings::default();
        settings.ai.api_key = "secret-key".to_string();
        let json = serde_json::to_string(&settings).expect("序列化失败");
        assert!(
            !json.contains("secret-key"),
            "api_key 不应以明文出现在 JSON 中"
        );
    }

    #[test]
    fn test_default_settings_values() {
        let s = AppSettings::default();
        assert_eq!(s.ai.provider, "deepseek");
        assert_eq!(s.ai.model, "deepseek-v4-pro");
        assert_eq!(s.ai.base_url, None);
        assert_eq!(s.ai.temperature, Some(0.7));
        assert_eq!(s.ai.max_tokens, Some(8192));
        assert!(s.ai.api_key.is_empty());
        assert_eq!(s.ui.theme, String::new());
        assert_eq!(s.ui.font_size, 0);
        assert!(!s.ui.sidebar_visible);
        assert!(s.remote.ssh_servers.is_empty());
    }

    #[test]
    fn test_settings_deserialize_defaults() {
        let json = r#"{}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai.provider, "deepseek");
        assert_eq!(s.ui.theme, String::new());
        assert!(!s.ui.sidebar_visible);
    }

    #[test]
    fn test_settings_deserialize_different_provider() {
        let json = r#"{
            "ai": {
                "provider": "kimi",
                "model": "moonshot-v1-32k",
                "base_url": "https://api.moonshot.cn/v1",
                "temperature": 0.5,
                "max_tokens": 4096
            }
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai.provider, "kimi");
        assert_eq!(s.ai.model, "moonshot-v1-32k");
        assert_eq!(s.ai.base_url.as_deref(), Some("https://api.moonshot.cn/v1"));
        assert_eq!(s.ai.temperature, Some(0.5));
        assert_eq!(s.ai.max_tokens, Some(4096));
    }

    #[test]
    fn test_ssh_server_config_defaults() {
        let cfg: SshServerConfig =
            serde_json::from_str(r#"{"name":"x","host":"h","username":"u"}"#).unwrap();
        assert_eq!(cfg.port, 22);
        assert_eq!(cfg.auth_type, SshAuthType::Agent);
        assert!(cfg.key_path.is_empty());
        // 未知认证方式回退为 Fallback（行为等同 Agent）
        let cfg: SshServerConfig =
            serde_json::from_str(r#"{"name":"x","host":"h","username":"u","auth_type":"gssapi"}"#)
                .unwrap();
        assert_eq!(cfg.auth_type, SshAuthType::Fallback);
        assert!(!cfg.auth_type.is_key());
        assert!(!cfg.auth_type.is_password());
    }

    #[test]
    fn test_settings_save_and_load_roundtrip() {
        let dir = temp_test_dir("aether-settings-rt");
        let settings_path = dir.join("settings.json");
        let api_key_path = dir.join("api_key.enc");

        let mut s = AppSettings::default();
        s.ai_models.push(AiModelProfile {
            id: "test-model".to_string(),
            display_name: "custom-model".to_string(),
            enabled: true,
            settings: AiSettings {
                provider: "custom".to_string(),
                api_key: "sk-roundtrip".to_string(),
                ..Default::default()
            },
        });
        s.active_model_id = Some("test-model".to_string());
        s.ui.font_size = 16;
        s.ui.theme = "dark".to_string();
        s.remote.ssh_servers.push(SshServerConfig {
            name: "home".to_string(),
            host: "192.168.1.1".to_string(),
            port: 2222,
            username: "u".to_string(),
            auth_type: SshAuthType::Key,
            key_path: "C:\\key".to_string(),
        });

        s.save_to(&settings_path, &api_key_path).unwrap();

        // settings.json 中不应包含明文 api_key
        let json = std::fs::read_to_string(&settings_path).unwrap();
        assert!(!json.contains("sk-roundtrip"));
        assert!(json.contains("custom-model"));
        assert!(json.contains("192.168.1.1"));

        // 加密文件应存在且可解密
        assert!(api_key_path.exists());
        let loaded = AppSettings::load_from(&settings_path, &api_key_path);
        assert_eq!(loaded.ai_models[0].settings.api_key, "sk-roundtrip");
        assert_eq!(loaded.ai_models[0].settings.provider, "custom");
        assert_eq!(loaded.ui.font_size, 16);
        assert_eq!(loaded.remote.ssh_servers.len(), 1);
        assert_eq!(loaded.remote.ssh_servers[0].auth_type, SshAuthType::Key);

        // 清理
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_settings_load_missing_returns_default() {
        let dir = temp_test_dir("aether-settings-missing");
        let settings_path = dir.join("nope.json");
        let api_key_path = dir.join("nope.enc");
        let loaded = AppSettings::load_from(&settings_path, &api_key_path);
        assert_eq!(loaded.ai.provider, "deepseek");
        assert!(loaded.ai.api_key.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_settings_load_corrupted_returns_default() {
        let dir = temp_test_dir("aether-settings-corrupt");
        let settings_path = dir.join("settings.json");
        let api_key_path = dir.join("api_key.enc");
        std::fs::write(&settings_path, "this is not json").unwrap();

        let loaded = AppSettings::load_from(&settings_path, &api_key_path);
        assert_eq!(loaded.ai.provider, "deepseek");
        assert!(loaded.ai.api_key.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_settings_empty_api_key_removes_encrypted_file() {
        let dir = temp_test_dir("aether-settings-nokey");
        let settings_path = dir.join("settings.json");
        let api_key_path = dir.join("api_key.enc");

        let mut s = AppSettings::default();
        s.ai_models.push(AiModelProfile {
            id: "m1".to_string(),
            display_name: String::new(),
            enabled: true,
            settings: AiSettings {
                api_key: "temp-key".to_string(),
                ..Default::default()
            },
        });
        s.save_to(&settings_path, &api_key_path).unwrap();
        assert!(api_key_path.exists());

        s.ai_models[0].settings.api_key.clear();
        s.save_to(&settings_path, &api_key_path).unwrap();
        assert!(!api_key_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_settings_debug_redacts_api_key() {
        let mut s = AppSettings::default();
        s.ai.api_key = "super-secret".to_string();
        let debug = format!("{:?}", s);
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn test_legacy_ai_migrates_to_model_profile() {
        // 旧版配置（只有单一 ai、无 ai_models）加载后应迁移为一条模型档案，
        // 且旧格式加密文件（单个密钥）能注入迁移后的档案。
        let dir = temp_test_dir("aether-settings-legacy");
        let settings_path = dir.join("settings.json");
        let api_key_path = dir.join("api_key.enc");
        std::fs::write(
            &settings_path,
            r#"{"ai":{"provider":"kimi","model":"moonshot-v1-32k","base_url":"https://api.moonshot.cn/v1","temperature":0.5,"max_tokens":4096}}"#,
        )
        .unwrap();
        // 旧格式：加密内容为单个密钥的 JSON 字符串
        let encrypted =
            encrypt_api_key(&serde_json::to_string("sk-legacy-key").unwrap()).expect("加密失败");
        std::fs::write(&api_key_path, encrypted).unwrap();

        let loaded = AppSettings::load_from(&settings_path, &api_key_path);
        assert_eq!(loaded.ai_models.len(), 1);
        let m = &loaded.ai_models[0];
        assert_eq!(m.id, "legacy-ai");
        assert_eq!(m.settings.provider, "kimi");
        assert_eq!(m.settings.model, "moonshot-v1-32k");
        assert_eq!(m.settings.api_key, "sk-legacy-key");
        assert_eq!(loaded.active_model_id.as_deref(), Some("legacy-ai"));
        // 迁移后旧 ai 被腾空，不再参与运行时逻辑
        assert!(loaded.ai.model.is_empty());

        // 再次保存后 settings.json 不应再包含 "ai" 字段
        loaded
            .save_to(&dir.join("settings2.json"), &dir.join("api_key2.enc"))
            .unwrap();
        let json = std::fs::read_to_string(dir.join("settings2.json")).unwrap();
        assert!(!json.contains("\"ai\""));
        assert!(json.contains("moonshot-v1-32k"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_no_legacy_ai_no_migration() {
        // 新格式配置（无 "ai" 键、无 ai_models）不应触发迁移
        let dir = temp_test_dir("aether-settings-nomigrate");
        let settings_path = dir.join("settings.json");
        let api_key_path = dir.join("api_key.enc");
        std::fs::write(&settings_path, r#"{"ui":{"font_size":14}}"#).unwrap();

        let loaded = AppSettings::load_from(&settings_path, &api_key_path);
        assert!(loaded.ai_models.is_empty());
        assert!(loaded.active_model_id.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_settings_ui_window_fields_default() {
        let ui: UiSettings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(ui.window_x, None);
        assert_eq!(ui.window_y, None);
        assert_eq!(ui.window_width, None);
        assert_eq!(ui.window_height, None);
        assert!(!ui.window_maximized);
        assert!(ui.activity_bar_order.is_empty());
        assert!(ui.menu_bar_order.is_empty());
        assert!(ui.show_taskbar_when_maximized);
    }

    #[test]
    fn test_settings_ui_window_fields_roundtrip() {
        let json = r#"{
            "theme": "light",
            "font_size": 14,
            "sidebar_visible": false,
            "window_x": 100,
            "window_y": 200,
            "window_width": 1280,
            "window_height": 720,
            "window_maximized": true,
            "activity_bar_order": ["files", "search"],
            "menu_bar_order": ["file", "edit"],
            "last_workspace": "C:\\\\proj"
        }"#;
        let ui: UiSettings = serde_json::from_str(json).unwrap();
        assert_eq!(ui.theme, "light");
        assert_eq!(ui.font_size, 14);
        assert!(!ui.sidebar_visible);
        assert_eq!(ui.window_x, Some(100));
        assert_eq!(ui.window_y, Some(200));
        assert_eq!(ui.window_width, Some(1280));
        assert_eq!(ui.window_height, Some(720));
        assert!(ui.window_maximized);
        assert_eq!(ui.activity_bar_order, vec!["files", "search"]);
        assert_eq!(ui.menu_bar_order, vec!["file", "edit"]);
        assert_eq!(ui.last_workspace, Some(PathBuf::from("C:\\proj")));
    }
}

/// 自动更新策略
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    /// 自动下载并静默安装（默认）
    AutoInstall,
    /// 仅通知用户有新版，不自动下载
    NotifyOnly,
    /// 关闭自动更新
    Disabled,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self::AutoInstall
    }
}

/// 自动更新设置（持久化到 settings.json）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct UpdateSettings {
    /// 更新策略
    pub policy: UpdatePolicy,
    /// 抑制天数：0=每次启动检查，1/7/30=N天内不再提醒
    pub suppress_days: u32,
    /// 上次检查时间（Unix 时间戳秒）
    pub last_check_ts: u64,
    /// 上次用户点击"稍后提醒"的时间戳
    pub last_suppressed_ts: u64,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            policy: UpdatePolicy::AutoInstall,
            suppress_days: 0,
            last_check_ts: 0,
            last_suppressed_ts: 0,
        }
    }
}
