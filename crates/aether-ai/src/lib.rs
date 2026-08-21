use aether_shared::settings::AiSettings;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc;
use url::Url;

// H-01: SSRF DNS 重绑定限制说明
//
// 当前实现对 DNS 解析返回的所有 IP 做私有地址校验（resolve_and_lock），
// 能阻断「域名始终解析到内网 IP」的静态攻击。
//
// 但由于 ureq + rustls 不支持在保持 TLS 主机名校验的前提下固定连接 IP，
// DNS 重绑定攻击（验证时返回公网 IP，连接时返回 169.254.169.254）仍有残余风险。
// 彻底修复需要自定义 TLS connector + IP pinning，属于架构级改造，暂不实施。
// 此处保留 DNS 校验作为纵深防御层，并移除从未使用的 verified_ips 死代码。

#[derive(Clone, PartialEq, Eq)]
pub enum AiProvider {
    DeepSeek,
    Kimi,
    Custom,
}

impl std::fmt::Debug for AiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeepSeek => write!(f, "DeepSeek"),
            Self::Kimi => write!(f, "Kimi"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

impl AiProvider {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "deepseek" => Self::DeepSeek,
            "kimi" | "moonshot" => Self::Kimi,
            _ => Self::Custom,
        }
    }

    pub fn default_base_url(&self) -> &'static str {
        match self {
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::Kimi => "https://api.moonshot.cn/v1",
            Self::Custom => "",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek-v4-pro",
            Self::Kimi => "moonshot-v1-8k",
            Self::Custom => "",
        }
    }

    /// 该服务商的预置模型清单（真实模型名）。Custom 无预置，返回空切片。
    /// 作为 UI 模型下拉的唯一数据源，避免核心层与 UI 清单漂移。
    pub fn preset_models(&self) -> &'static [&'static str] {
        match self {
            Self::DeepSeek => &["deepseek-v4-pro", "deepseek-v4-flash"],
            Self::Kimi => &[
                "moonshot-v1-8k",
                "moonshot-v1-32k",
                "moonshot-v1-128k",
                "kimi-latest",
            ],
            Self::Custom => &[],
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::Kimi => "kimi",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug)]
pub enum AiError {
    Http(String),
    Parse(String),
    Config(String),
    /// H-21: message 已截断至 200 字符，但仍可能含敏感信息，
    /// 展示给用户时应使用 `safe_display()` 而非 `Display`。
    Api {
        code: u16,
        message: String,
    },
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Http(e) => write!(f, "HTTP error: {}", e),
            AiError::Parse(e) => write!(f, "Parse error: {}", e),
            AiError::Config(e) => write!(f, "Config error: {}", e),
            AiError::Api { code, message } => write!(f, "API error {}: {}", code, message),
        }
    }
}

impl std::error::Error for AiError {}

impl AiError {
    /// H-18 / H-21: 返回对用户安全的错误描述，不包含原始 API 响应体。
    ///
    /// `Display` 实现包含完整（已截断）的 API 响应体，可能含 API Key 等敏感信息，
    /// 仅供日志使用。展示给用户时应调用此方法，仅返回 HTTP 状态码和通用描述。
    ///
    /// 错误码对应解决建议：
    /// - 400 格式错误 → 请求体参数不符合 API 要求，请检查模型名/参数
    /// - 401 认证失败 → API Key 错误或已过期，请到设置页重新填写
    /// - 402 余额不足 → 账户余额不足，请到提供商官网充值
    /// - 422 参数错误 → 请求体参数格式错误（如 temperature 超出范围）
    /// - 429 速率上限 → 请求过于频繁，请稍后重试
    /// - 500 服务器故障 → API 提供商内部错误，请稍后重试
    /// - 503 服务器繁忙 → 服务器负载过高，请稍后重试
    pub fn safe_display(&self) -> String {
        match self {
            AiError::Http(_) => "网络请求失败，请检查网络连接".to_string(),
            AiError::Parse(_) => "API 响应解析失败".to_string(),
            AiError::Config(e) => e.clone(),
            AiError::Api { code, .. } => {
                let desc = match *code {
                    400 => "请求体格式错误，请检查模型名和参数设置",
                    401 => "API Key 无效或已过期，请到设置页重新填写",
                    402 => "账户余额不足，请到提供商官网充值",
                    403 => "API Key 权限不足，请检查模型访问权限",
                    404 => "请求的资源不存在，请检查 Base URL 和模型名",
                    422 => "参数错误，请检查 temperature/max_tokens 等参数范围",
                    429 => "请求速率超限（TPM/RPM 达到上限），请稍后重试",
                    500 => "API 服务器内部故障，请稍后重试",
                    503 => "API 服务器负载过高（服务器繁忙），请稍后重试",
                    _ => "API 请求失败",
                };
                format!("HTTP {}: {}", code, desc)
            }
        }
    }

    /// 返回是否可自动重试的错误（暂时性错误）
    /// 用于 429/503 的自动重试策略（指数退避）
    pub fn is_retryable(&self) -> bool {
        match self {
            AiError::Api { code, .. } => matches!(*code, 429 | 500 | 503),
            _ => false,
        }
    }

    /// 返回是否为永久错误（无需重试，提示用户检查配置）
    pub fn is_permanent(&self) -> bool {
        match self {
            AiError::Api { code, .. } => matches!(*code, 400 | 401 | 402 | 403 | 404 | 422),
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub temperature: Option<f32>,
    /// 核采样 top_p（0.0-1.0），None 表示不下发该参数
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
    /// 深度思考开关（仅 DeepSeek 生效），None 表示不下发该参数
    pub thinking: Option<bool>,
    /// 思考强度（仅 DeepSeek 思考模式生效）："high"/"max"，None 表示不下发
    pub reasoning_effort: Option<String>,
    /// 频率惩罚（-2.0 ~ 2.0），思考模式下不生效，None 表示不下发
    pub frequency_penalty: Option<f32>,
    /// 存在惩罚（-2.0 ~ 2.0），思考模式下不生效，None 表示不下发
    pub presence_penalty: Option<f32>,
    /// 停止序列（最多 16 个），None/空表示不下发
    pub stop: Option<Vec<String>>,
    /// 响应格式：Some("json_object")=强制 JSON 输出
    pub response_format: Option<String>,
    /// 流式用量统计：Some(true) 时下发 stream_options.include_usage
    pub include_usage: Option<bool>,
    /// 返回输出 token 对数概率（调试用）
    pub logprobs: Option<bool>,
    /// 每位置候选 token 数（0-20，需 logprobs 开启）
    pub top_logprobs: Option<u32>,
    /// 业务侧用户标识，None/空表示不下发
    pub user_id: Option<String>,
}

impl std::fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiConfig")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("top_p", &self.top_p)
            .field("max_tokens", &self.max_tokens)
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
            .field("include_usage", &self.include_usage)
            .field("logprobs", &self.logprobs)
            .field("top_logprobs", &self.top_logprobs)
            .field("user_id", &self.user_id)
            .finish()
    }
}

impl AiConfig {
    pub fn from_settings(settings: &AiSettings) -> Self {
        let provider = AiProvider::from_str(&settings.provider);
        let base_url = settings.base_url.clone().or_else(|| {
            let default = provider.default_base_url();
            if default.is_empty() {
                None
            } else {
                Some(default.to_string())
            }
        });
        let model = if settings.model.is_empty() {
            provider.default_model().to_string()
        } else {
            settings.model.clone()
        };
        Self {
            provider,
            api_key: settings.api_key.clone(),
            base_url,
            model,
            temperature: settings.temperature,
            top_p: settings.top_p,
            max_tokens: settings.max_tokens,
            system_prompt: settings.system_prompt.clone(),
            thinking: settings.thinking,
            reasoning_effort: settings.reasoning_effort.clone(),
            frequency_penalty: settings.frequency_penalty,
            presence_penalty: settings.presence_penalty,
            stop: settings.stop.clone(),
            response_format: settings.response_format.clone(),
            include_usage: settings.include_usage,
            logprobs: settings.logprobs,
            top_logprobs: settings.top_logprobs,
            user_id: settings.user_id.clone(),
        }
    }

    /// 是否处于 DeepSeek 思考模式（thinking 默认开启，None 视为开启）。
    /// 思考模式下采样参数（temperature/top_p 等）不生效，不应下发。
    pub fn deepseek_thinking_active(&self) -> bool {
        matches!(self.provider, AiProvider::DeepSeek) && self.thinking.unwrap_or(true)
    }
}

pub struct AiClient {
    config: AiConfig,
    http: ureq::Agent,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// AI 流式响应事件
#[derive(Clone, Debug)]
pub enum AiStreamEvent {
    /// 一个新的文本 token（最终回答内容）
    Token(String),
    /// 一个新的"深度思考"token（如 DeepSeek reasoner 的 reasoning_content）
    Reasoning(String),
    /// 流结束（正常完成，finish_reason = "stop"）
    Done,
    /// 输出被截断（达到 max_tokens 限制，finish_reason = "length" / "max_tokens"）
    Truncated(String),
    /// 流式过程中出现错误
    Error(String),
}

/// 已解析并校验的公网端点
#[derive(Debug, PartialEq, Eq)]
struct ResolvedEndpoint {
    host: String,
    port: u16,
}

impl AiClient {
    pub fn new(config: &AiSettings) -> Self {
        let config = AiConfig::from_settings(config);
        // SEC-C02: 禁用自动重定向，防止 SSRF 通过 302 跳转到内网地址
        // 超时策略：流式生成总时长不可预估，禁止设置整体 timeout（会把长生成中途掐断）；
        // 改为分离配置——连接阶段 15s + 读空闲 300s（DeepSeek 会持续发 keep-alive，正常流不会触发）。
        let http = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(300))
            .redirects(0)
            .build();
        Self { config, http }
    }

    pub fn test_connection(&self) -> Result<String, AiError> {
        // 轻量探活：极短提示 + 极小输出 + 显式关闭思考模式。
        // DeepSeek V4 思考模式默认开启，若不关闭，测试请求也会先跑完整思维链
        // 才返回，导致验证连接长达数十秒。
        self.complete_openai_compatible("Hi", 8, true)
    }

    /// H-18: test_connection 的安全版本，错误信息经过脱敏处理，
    /// 可直接用于 UI 展示。调用方无需再单独 sanitize。
    pub fn test_connection_safe(&self) -> Result<String, String> {
        self.test_connection().map_err(|e| e.safe_display())
    }

    /// 拉取该服务商当前可用的模型 ID 列表（OpenAI 兼容 `GET {base_url}/models`）。
    ///
    /// DeepSeek/Kimi/自定义（OpenAI 兼容）均支持；返回 data[].id 列表（保持接口原始顺序）。
    pub fn list_models(&self) -> Result<Vec<String>, AiError> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.deepseek.com/v1");
        Self::validate_https(base_url)?;
        Self::validate_not_private_ip(base_url)?;

        if self.config.api_key.is_empty() {
            return Err(AiError::Config("API Key 未设置".to_string()));
        }

        Self::validate_tcp_connect_target(base_url)?;
        let url = format!("{}/models", base_url);

        let response = self
            .http
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.config.api_key))
            .call()
            .map_err(|e| AiError::Http(e.to_string()))?;

        let status = response.status();
        if status != 200 {
            let text = Self::read_limited_response(response)?;
            return Err(AiError::Api {
                code: status,
                message: Self::truncate_error_message(&text),
            });
        }

        let text = Self::read_limited_response(response)?;
        Self::parse_model_ids(&text)
    }

    /// 从 `/models` 响应文本解析出模型 ID 列表（`data[].id`）。
    fn parse_model_ids(text: &str) -> Result<Vec<String>, AiError> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| AiError::Parse(e.to_string()))?;
        let arr = json["data"]
            .as_array()
            .ok_or_else(|| AiError::Parse("模型列表响应缺少 data 数组".to_string()))?;
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        Ok(ids)
    }

    /// list_models 的安全版本，错误信息脱敏，可直接用于 UI 展示。
    pub fn list_models_safe(&self) -> Result<Vec<String>, String> {
        self.list_models().map_err(|e| e.safe_display())
    }

    fn validate_https(url: &str) -> Result<(), AiError> {
        if !url.starts_with("https://") {
            return Err(AiError::Config(format!(
                "API base URL 必须使用 HTTPS: {}",
                url
            )));
        }
        Ok(())
    }

    /// 校验 URL 不属于私有/保留 IP 范围（SSRF 防护）
    /// SEC-C05: 使用 url::Url 进行严格 URL 解析，防止 userinfo/IPv6 绕过
    /// SEC-C03: 在发起 HTTP 请求前进行二次 DNS 校验（TOCTOU 防护）
    /// AI-H04: 扩展云元数据黑名单
    fn validate_not_private_ip(url_str: &str) -> Result<(), AiError> {
        // SEC-C05: 使用 url::Url 进行严格的 URL 解析
        let parsed =
            Url::parse(url_str).map_err(|e| AiError::Config(format!("无效的 URL 格式: {}", e)))?;

        let host_str = parsed
            .host_str()
            .ok_or_else(|| AiError::Config("URL 缺少主机名".to_string()))?;

        let port = parsed.port().unwrap_or(443);

        // 检查是否为 IP 地址（包括 IPv6）
        if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
            Self::check_ip_private(ip, host_str)?;
        }

        // SEC-C03: DNS TOCTOU 防护 — 对主机名做一次 DNS 解析并校验所有 IP
        // 后续在发起 HTTP 请求前还会做二次校验
        if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(host_str, port)) {
            for addr in addrs {
                let ip = addr.ip();
                Self::check_ip_private(ip, host_str)?;
            }
        }

        // AI-H04: 阻止常见云元数据端点（扩展黑名单）
        let blocked_hosts_lower = host_str.to_lowercase();
        let blocked = [
            // AWS
            "169.254.169.254",
            "fd00:ec2::254",
            // GCP
            "metadata.google.internal",
            "metadata.google",
            // Azure
            "metadata.azure.internal",
            "169.254.169.253",
            // 阿里云
            "100.100.100.200",
            // 腾讯云
            "metadata.tencentyun.com",
        ];
        for blocked_host in &blocked {
            if blocked_hosts_lower == *blocked_host {
                return Err(AiError::Config(format!("禁止访问元数据端点: {}", host_str)));
            }
        }
        Ok(())
    }

    /// 检查单个 IP 是否为私有/保留地址
    fn check_ip_private(ip: std::net::IpAddr, host_str: &str) -> Result<(), AiError> {
        let is_private = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_link_local()
                    || v4.is_loopback()
                    || v4.is_multicast()
                    || v4.is_broadcast()
                    || v4.is_documentation()
            }
            std::net::IpAddr::V6(v6) => {
                if let Some(v4) = v6.to_ipv4_mapped() {
                    v4.is_private()
                        || v4.is_link_local()
                        || v4.is_loopback()
                        || v4.is_multicast()
                        || v4.is_broadcast()
                        || v4.is_documentation()
                } else {
                    v6.is_loopback() || v6.is_multicast() || v6.is_unspecified()
                }
            }
        };
        if is_private || ip.is_unspecified() || ip.is_loopback() {
            return Err(AiError::Config(format!(
                "禁止访问私有/本地地址: {} (解析自 {})",
                ip, host_str
            )));
        }
        Ok(())
    }

    /// SEC-A01: 解析并校验 DNS 结果，阻断「域名始终解析到内网 IP」的攻击。
    ///
    /// H-01: 此方法对 DNS 解析返回的所有 IP 做私有地址校验，但不固定连接 IP。
    /// DNS 重绑定攻击（验证后 DNS 返回不同 IP）的彻底防护需要自定义 TLS connector，
    /// 属于架构级改造，暂不实施。当前校验作为纵深防御层保留。
    fn resolve_and_lock(url_str: &str) -> Result<ResolvedEndpoint, AiError> {
        let parsed =
            Url::parse(url_str).map_err(|e| AiError::Config(format!("无效的 URL: {}", e)))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| AiError::Config("URL 缺少主机名".to_string()))?;
        let port = parsed.port().unwrap_or(443);

        // 如果已经是 IP 地址，直接校验
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            Self::check_ip_private(ip, host)?;
            return Ok(ResolvedEndpoint {
                host: host.to_string(),
                port,
            });
        }

        // DNS 解析并校验所有 IP
        let addrs: Vec<std::net::SocketAddr> =
            std::net::ToSocketAddrs::to_socket_addrs(&(host, port))
                .map_err(|e| AiError::Config(format!("DNS 解析失败: {}", e)))?
                .collect();
        if addrs.is_empty() {
            return Err(AiError::Config(format!("DNS 解析无结果: {}", host)));
        }
        for addr in &addrs {
            Self::check_ip_private(addr.ip(), host)?;
        }
        Ok(ResolvedEndpoint {
            host: host.to_string(),
            port,
        })
    }

    /// SEC-C03: TOCTOU 二次 DNS 校验 — 在发起 HTTP 请求前调用
    /// 验证 DNS 解析结果未在两次查询间被篡改为私有地址
    fn validate_tcp_connect_target(url_str: &str) -> Result<(), AiError> {
        Self::resolve_and_lock(url_str).map(|_| ())
    }

    /// 安全读取响应体，限制最大 10MB
    fn read_limited_response(response: ureq::Response) -> Result<String, AiError> {
        let mut reader = response.into_reader();
        let mut buf = Vec::with_capacity(4096);
        let max_size = 10 * 1024 * 1024; // 10MB
        let mut total = 0usize;
        let mut chunk = [0u8; 4096];

        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    total += n;
                    if total > max_size {
                        return Err(AiError::Http("响应体超过 10MB 限制".to_string()));
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                Err(e) => return Err(AiError::Http(format!("读取响应失败: {}", e))),
            }
        }
        String::from_utf8(buf).map_err(|e| AiError::Parse(format!("UTF-8 解码失败: {}", e)))
    }

    /// H-21: 截断错误消息至 200 字符（在 UTF-8 字符边界上截断），防止大量响应体传入 UI
    fn truncate_error_message(text: &str) -> String {
        const MAX_ERR_LEN: usize = 200;
        if text.len() <= MAX_ERR_LEN {
            return text.to_string();
        }
        let safe_len = text.floor_char_boundary(MAX_ERR_LEN);
        let mut truncated = text[..safe_len].to_string();
        truncated.push_str("...(已截断)");
        truncated
    }

    pub fn complete(&self, prompt: &str) -> Result<String, AiError> {
        // DeepSeek / Kimi / Custom 均为 OpenAI 兼容接口，统一走同一路径
        self.complete_openai_compatible(prompt, 100, false)
    }

    pub fn chat_completion(&self, messages: &[ChatMessage]) -> Result<String, AiError> {
        self.chat_openai_compatible(messages)
    }

    fn complete_openai_compatible(
        &self,
        prompt: &str,
        max_tokens: u32,
        disable_thinking: bool,
    ) -> Result<String, AiError> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.deepseek.com/v1");
        Self::validate_https(base_url)?;
        Self::validate_not_private_ip(base_url)?;

        // AI-M01: 空 API Key 前置检查
        if self.config.api_key.is_empty() {
            return Err(AiError::Config("API Key 未设置".to_string()));
        }

        // SEC-C03: TOCTOU 二次 DNS 校验，仅在请求前做 SSRF 校验，
        // 不再用解析到的 IP 直连，以保留 TLS 主机名证书校验
        Self::validate_tcp_connect_target(base_url)?;
        // 始终使用原始 base_url（含域名），TLS 证书验证才能匹配域名
        let url = format!("{}/chat/completions", base_url);

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
        });
        // 探活请求：DeepSeek 显式关闭思考模式（否则默认思考链会显著拖慢响应）
        if disable_thinking && matches!(self.config.provider, AiProvider::DeepSeek) {
            body["thinking"] = serde_json::json!({ "type": "disabled" });
        }

        let response = self
            .http
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.config.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| AiError::Http(e.to_string()))?;

        let status = response.status();
        if status != 200 {
            let text = Self::read_limited_response(response)?;
            // H-21: 截断 API 错误响应体至 200 字符，防止大量数据（可能含敏感信息）传入 UI
            return Err(AiError::Api {
                code: status,
                message: Self::truncate_error_message(&text),
            });
        }

        let text = Self::read_limited_response(response)?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| AiError::Parse(e.to_string()))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AiError::Parse("Unexpected API response structure".to_string()))?
            .to_string();

        Ok(content)
    }

    fn chat_openai_compatible(&self, messages: &[ChatMessage]) -> Result<String, AiError> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.deepseek.com/v1");
        Self::validate_https(base_url)?;
        Self::validate_not_private_ip(base_url)?;

        // AI-M01: 空 API Key 前置检查
        if self.config.api_key.is_empty() {
            return Err(AiError::Config("API Key 未设置".to_string()));
        }

        // SEC-C03: TOCTOU 二次 DNS 校验，仅在请求前做 SSRF 校验，
        // 不再用解析到的 IP 直连，以保留 TLS 主机名证书校验
        Self::validate_tcp_connect_target(base_url)?;
        // 始终使用原始 base_url（含域名），TLS 证书验证才能匹配域名
        let url = format!("{}/chat/completions", base_url);

        let msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": self.config.model,
            "messages": msgs,
            "max_tokens": 2048,
        });

        let response = self
            .http
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.config.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| AiError::Http(e.to_string()))?;

        let status = response.status();
        if status != 200 {
            let text = Self::read_limited_response(response)?;
            return Err(AiError::Api {
                code: status,
                // H-21: 截断 API 错误响应体至 200 字符
                message: Self::truncate_error_message(&text),
            });
        }

        let text = Self::read_limited_response(response)?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| AiError::Parse(e.to_string()))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AiError::Parse("Unexpected API response structure".to_string()))?
            .to_string();

        Ok(content)
    }

    /// 流式聊天补全。
    ///
    /// 返回一个 Receiver，后台线程会在每次收到 token 时发送 `AiStreamEvent::Token`，
    /// 流结束时发送 `AiStreamEvent::Done`，出错时发送 `AiStreamEvent::Error`。
    pub fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
    ) -> Result<mpsc::Receiver<AiStreamEvent>, AiError> {
        // DeepSeek / Kimi / Custom 均走 OpenAI 兼容的 SSE 流式接口
        self.stream_openai_compatible(messages)
    }

    /// 为 DeepSeek 请求体注入 thinking 参数（深度思考开关）。
    ///
    /// DeepSeek V4 用 `thinking: {"type":"enabled"|"disabled"}` 控制思考/非思考模式，
    /// 是 DeepSeek 专属参数；其它服务商不下发。None 表示不下发（用服务端默认=开启）。
    fn apply_thinking_param(&self, body: &mut serde_json::Value) {
        if !matches!(self.config.provider, AiProvider::DeepSeek) {
            return;
        }
        if let Some(enabled) = self.config.thinking {
            let mode = if enabled { "enabled" } else { "disabled" };
            body["thinking"] = serde_json::json!({ "type": mode });
        }
    }

    fn stream_openai_compatible(
        &self,
        messages: &[ChatMessage],
    ) -> Result<mpsc::Receiver<AiStreamEvent>, AiError> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.deepseek.com/v1");
        Self::validate_https(base_url)?;
        Self::validate_not_private_ip(base_url)?;
        Self::validate_tcp_connect_target(base_url)?;

        if self.config.api_key.is_empty() {
            return Err(AiError::Config("API Key 未设置".to_string()));
        }

        let url = format!("{}/chat/completions", base_url);
        // system 消息由调用方在消息列表中构建（见 build_chat_prompt，固定为第一条），
        // 此处不再从 config.system_prompt 重复注入，避免同一提示词发送两遍。
        let body_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": body_messages,
            "stream": true,
        });
        // 厂商差异：DeepSeek 思考模式下采样类参数（temperature/top_p/惩罚项）不生效，不下发；
        // 非思考模式与其它厂商按配置正常发送
        let thinking_active = self.config.deepseek_thinking_active();
        if !thinking_active {
            if let Some(t) = self.config.temperature {
                body["temperature"] = serde_json::json!(t);
            }
            if let Some(p) = self.config.top_p {
                body["top_p"] = serde_json::json!(p);
            }
            if let Some(fp) = self.config.frequency_penalty {
                body["frequency_penalty"] = serde_json::json!(fp);
            }
            if let Some(pp) = self.config.presence_penalty {
                body["presence_penalty"] = serde_json::json!(pp);
            }
        }
        // 厂商差异：DeepSeek V4 用 thinking 参数控制深度思考（其它服务商不下发）
        self.apply_thinking_param(&mut body);
        // DeepSeek 思考模式下可选下发思考强度 reasoning_effort（high/max）
        if thinking_active {
            if let Some(effort) = self.config.reasoning_effort.as_deref() {
                if effort == "high" || effort == "max" {
                    body["reasoning_effort"] = serde_json::json!(effort);
                }
            }
        }
        if let Some(m) = self.config.max_tokens {
            body["max_tokens"] = serde_json::json!(m);
        }
        // 停止序列（最多 16 个，空列表不下发）
        if let Some(stop) = &self.config.stop {
            if !stop.is_empty() {
                let capped: Vec<&String> = stop.iter().take(16).collect();
                body["stop"] = serde_json::json!(capped);
            }
        }
        // JSON 输出模式（仅 json_object 时下发，文本模式用服务端默认）
        if self.config.response_format.as_deref() == Some("json_object") {
            body["response_format"] = serde_json::json!({ "type": "json_object" });
        }
        // 流式用量统计：末尾 chunk 附带 token 用量
        if self.config.include_usage == Some(true) {
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        // logprobs 调试参数（top_logprobs 需 logprobs 开启，上限 20）
        if self.config.logprobs == Some(true) {
            body["logprobs"] = serde_json::json!(true);
            if let Some(n) = self.config.top_logprobs {
                body["top_logprobs"] = serde_json::json!(n.min(20));
            }
        }
        // 业务侧用户标识（空字符串不下发）
        if let Some(uid) = self.config.user_id.as_deref() {
            if !uid.is_empty() {
                body["user_id"] = serde_json::json!(uid);
            }
        }

        let response = self
            .http
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.config.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| AiError::Http(e.to_string()))?;

        Self::stream_response(response)
    }

    fn stream_response(response: ureq::Response) -> Result<mpsc::Receiver<AiStreamEvent>, AiError> {
        let status = response.status();
        if status != 200 {
            let text = Self::read_limited_response(response)?;
            return Err(AiError::Api {
                code: status,
                message: text,
            });
        }

        let (tx, rx) = mpsc::channel::<AiStreamEvent>();
        std::thread::spawn(move || {
            let reader = response.into_reader();
            let mut buf = BufReader::new(reader);
            let mut data_buf = String::new();
            let mut line = String::new();

            loop {
                line.clear();
                match buf.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        let _ = tx.send(AiStreamEvent::Error(format!("读取流失败: {}", e)));
                        break;
                    }
                }

                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    if !data_buf.is_empty() {
                        if data_buf.trim() == "[DONE]" {
                            let _ = tx.send(AiStreamEvent::Done);
                            break;
                        } else {
                            match serde_json::from_str::<serde_json::Value>(&data_buf) {
                                Ok(json) => {
                                    if json.get("error").is_some() {
                                        let _ = tx.send(AiStreamEvent::Error(format!(
                                            "API error: {}",
                                            json["error"]
                                        )));
                                        break;
                                    }
                                    if let Some(reasoning) = Self::extract_stream_reasoning(&json) {
                                        if !reasoning.is_empty() {
                                            let _ = tx.send(AiStreamEvent::Reasoning(
                                                reasoning.to_string(),
                                            ));
                                        }
                                    }
                                    if let Some(token) = Self::extract_stream_token(&json) {
                                        if !token.is_empty() {
                                            let _ = tx.send(AiStreamEvent::Token(token));
                                        }
                                    }
                                    // 检查 finish_reason：如果是 length/max_tokens 说明被截断了
                                    if let Some(finish_reason) = json
                                        .pointer("/choices/0/finish_reason")
                                        .and_then(|v| v.as_str().map(|s| s.to_lowercase()))
                                    {
                                        if finish_reason == "length"
                                            || finish_reason == "max_tokens"
                                        {
                                            let _ =
                                                tx.send(AiStreamEvent::Truncated(finish_reason));
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(AiStreamEvent::Error(format!(
                                        "解析 SSE JSON 失败: {}",
                                        e
                                    )));
                                }
                            }
                        }
                        data_buf.clear();
                    }
                    continue;
                }

                if let Some(data) = trimmed.strip_prefix("data:") {
                    data_buf.push_str(data.trim_start());
                }
            }
        });

        Ok(rx)
    }

    fn extract_stream_token(json: &serde_json::Value) -> Option<String> {
        // OpenAI / OpenAI-compatible: choices[0].delta.content
        if let Some(content) = json
            .pointer("/choices/0/delta/content")
            .and_then(|v| v.as_str())
        {
            return Some(content.to_string());
        }
        // Anthropic content_block_delta: delta.text（仅 text_delta 带此字段，
        // thinking_delta/signature_delta 不带，天然与思维链分流）
        if let Some(text) = json.pointer("/delta/text").and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }
        None
    }

    /// 从 SSE JSON 分片提取"深度思考"内容（思维链），与最终回答 token 分流。
    ///
    /// 兼容两种主流格式：
    /// - OpenAI / DeepSeek reasoner：`choices[0].delta.reasoning_content`
    /// - Anthropic 扩展思考：`content_block_delta` 中 `delta.type == "thinking_delta"`
    ///   时取 `delta.thinking`（`signature_delta` 等其它块不含 thinking，返回 None）
    fn extract_stream_reasoning(json: &serde_json::Value) -> Option<String> {
        // OpenAI / DeepSeek：choices[0].delta.reasoning_content
        if let Some(reasoning) = json
            .pointer("/choices/0/delta/reasoning_content")
            .and_then(|v| v.as_str())
        {
            return Some(reasoning.to_string());
        }
        // Anthropic 扩展思考：仅当 delta.type 为 thinking_delta 时取 delta.thinking
        if json.pointer("/delta/type").and_then(|v| v.as_str()) == Some("thinking_delta") {
            if let Some(thinking) = json.pointer("/delta/thinking").and_then(|v| v.as_str()) {
                return Some(thinking.to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== AiProvider ====================

    #[test]
    fn provider_from_str_deepseek_variants() {
        for s in ["deepseek", "DeepSeek", "DEEPSEEK"] {
            assert_eq!(
                AiProvider::from_str(s),
                AiProvider::DeepSeek,
                "failed for {}",
                s
            );
        }
    }

    #[test]
    fn provider_from_str_kimi_variants() {
        for s in ["kimi", "moonshot", "KIMI", "Moonshot"] {
            assert_eq!(
                AiProvider::from_str(s),
                AiProvider::Kimi,
                "failed for {}",
                s
            );
        }
    }

    #[test]
    fn provider_from_str_custom_and_unknown() {
        // 已移除的旧服务商（openai/claude/azure）以及未知串一律回退为 Custom
        for s in [
            "custom",
            "foo",
            "",
            "llama",
            "unknown",
            "openai",
            "claude",
            "anthropic",
            "azure",
        ] {
            assert_eq!(
                AiProvider::from_str(s),
                AiProvider::Custom,
                "failed for {:?}",
                s
            );
        }
    }

    #[test]
    fn provider_default_base_url() {
        assert_eq!(
            AiProvider::DeepSeek.default_base_url(),
            "https://api.deepseek.com/v1"
        );
        assert_eq!(
            AiProvider::Kimi.default_base_url(),
            "https://api.moonshot.cn/v1"
        );
        assert_eq!(AiProvider::Custom.default_base_url(), "");
    }

    #[test]
    fn provider_default_model() {
        assert_eq!(AiProvider::DeepSeek.default_model(), "deepseek-v4-pro");
        assert_eq!(AiProvider::Kimi.default_model(), "moonshot-v1-8k");
        assert_eq!(AiProvider::Custom.default_model(), "");
    }

    #[test]
    fn provider_preset_models() {
        // DeepSeek/Kimi 提供真实模型清单；Custom 无预置
        assert_eq!(
            AiProvider::DeepSeek.preset_models(),
            &["deepseek-v4-pro", "deepseek-v4-flash"]
        );
        assert!(AiProvider::Kimi.preset_models().contains(&"moonshot-v1-8k"));
        assert!(AiProvider::Kimi.preset_models().contains(&"kimi-latest"));
        assert!(AiProvider::Custom.preset_models().is_empty());
        // 每个预置模型的第一项即该服务商的默认模型
        assert_eq!(
            AiProvider::DeepSeek.preset_models().first(),
            Some(&AiProvider::DeepSeek.default_model())
        );
    }

    #[test]
    fn provider_as_str() {
        assert_eq!(AiProvider::DeepSeek.as_str(), "deepseek");
        assert_eq!(AiProvider::Kimi.as_str(), "kimi");
        assert_eq!(AiProvider::Custom.as_str(), "custom");
    }

    #[test]
    fn provider_debug_and_clone_eq() {
        let p = AiProvider::Kimi;
        assert_eq!(format!("{:?}", p), "Kimi");
        assert_eq!(p.clone(), p);
    }

    // ==================== AiError ====================

    #[test]
    fn ai_error_display() {
        assert_eq!(
            format!("{}", AiError::Http("timeout".to_string())),
            "HTTP error: timeout"
        );
        assert_eq!(
            format!("{}", AiError::Parse("bad json".to_string())),
            "Parse error: bad json"
        );
        assert_eq!(
            format!("{}", AiError::Config("missing key".to_string())),
            "Config error: missing key"
        );
        assert_eq!(
            format!(
                "{}",
                AiError::Api {
                    code: 500,
                    message: "boom".to_string()
                }
            ),
            "API error 500: boom"
        );
    }

    // ==================== AiConfig ====================

    fn settings_with(
        provider: &str,
        api_key: &str,
        base_url: Option<&str>,
        model: &str,
    ) -> AiSettings {
        AiSettings {
            provider: provider.to_string(),
            api_key: api_key.to_string(),
            base_url: base_url.map(|s| s.to_string()),
            model: model.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn config_from_settings_defaults() {
        let settings = settings_with("deepseek", "key", None, "");
        let config = AiConfig::from_settings(&settings);
        assert_eq!(config.provider, AiProvider::DeepSeek);
        assert_eq!(config.api_key, "key");
        assert_eq!(
            config.base_url,
            Some("https://api.deepseek.com/v1".to_string())
        );
        assert_eq!(config.model, "deepseek-v4-pro");
    }

    #[test]
    fn config_from_settings_custom_base_url_and_model() {
        // 显式 base_url 应覆盖服务商默认值
        let settings = settings_with("kimi", "secret", Some("https://example.com/v1"), "model-x");
        let config = AiConfig::from_settings(&settings);
        assert_eq!(config.provider, AiProvider::Kimi);
        assert_eq!(config.base_url, Some("https://example.com/v1".to_string()));
        assert_eq!(config.model, "model-x");
    }

    #[test]
    fn config_from_settings_empty_base_url_for_custom_provider() {
        // Custom provider has empty default base_url, so result should be None.
        let settings = settings_with("custom", "key", None, "");
        let config = AiConfig::from_settings(&settings);
        assert_eq!(config.provider, AiProvider::Custom);
        assert_eq!(config.base_url, None);
        assert_eq!(config.model, "");
    }

    #[test]
    fn config_from_settings_explicit_empty_base_url() {
        let settings = settings_with("deepseek", "key", Some(""), "");
        let config = AiConfig::from_settings(&settings);
        // An explicitly empty base_url is preserved as Some("") rather than falling back.
        assert_eq!(config.base_url, Some("".to_string()));
    }

    #[test]
    fn config_debug_hides_api_key_and_shows_system_prompt_presence() {
        let config = AiConfig {
            provider: AiProvider::DeepSeek,
            api_key: "super-secret".to_string(),
            base_url: Some("https://api.deepseek.com/v1".to_string()),
            model: "deepseek-v4-pro".to_string(),
            temperature: Some(0.7),
            top_p: None,
            max_tokens: Some(100),
            system_prompt: Some("you are helpful".to_string()),
            thinking: None,
            reasoning_effort: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            response_format: None,
            include_usage: None,
            logprobs: None,
            top_logprobs: None,
            user_id: None,
        };
        let out = format!("{:?}", config);
        assert!(!out.contains("super-secret"), "api_key leaked in Debug");
        assert!(out.contains("[REDACTED]"), "api_key not marked redacted");
        assert!(
            out.contains("[PRESENT]"),
            "system_prompt presence not indicated"
        );
        assert!(out.contains("deepseek-v4-pro"));
    }

    // ==================== ChatMessage ====================

    #[test]
    fn chat_message_user_and_assistant() {
        let u = ChatMessage::user("hello");
        assert_eq!(u.role, "user");
        assert_eq!(u.content, "hello");

        let a = ChatMessage::assistant(String::from("hi there"));
        assert_eq!(a.role, "assistant");
        assert_eq!(a.content, "hi there");
    }

    // ==================== AiClient ====================

    #[test]
    fn client_new_preserves_config() {
        let settings = AiSettings {
            provider: "kimi".to_string(),
            api_key: "mk".to_string(),
            base_url: Some("https://api.moonshot.cn/v1".to_string()),
            model: "moonshot-v1-8k".to_string(),
            temperature: Some(0.5),
            max_tokens: Some(512),
            system_prompt: Some("sys".to_string()),
            thinking: Some(true),
            ..Default::default()
        };
        let client = AiClient::new(&settings);
        assert_eq!(client.config.provider, AiProvider::Kimi);
        assert_eq!(client.config.api_key, "mk");
        assert_eq!(
            client.config.base_url,
            Some("https://api.moonshot.cn/v1".to_string())
        );
        assert_eq!(client.config.model, "moonshot-v1-8k");
        assert_eq!(client.config.temperature, Some(0.5));
        assert_eq!(client.config.max_tokens, Some(512));
        assert_eq!(client.config.system_prompt, Some("sys".to_string()));
        assert_eq!(client.config.thinking, Some(true));
    }

    #[test]
    fn parse_model_ids_deepseek_example() {
        // DeepSeek /models 官方示例响应
        let text = r#"{"object":"list","data":[
            {"id":"deepseek-v4-flash","object":"model","owned_by":"deepseek"},
            {"id":"deepseek-v4-pro","object":"model","owned_by":"deepseek"}
        ]}"#;
        let ids = AiClient::parse_model_ids(text).unwrap();
        assert_eq!(ids, vec!["deepseek-v4-flash", "deepseek-v4-pro"]);
    }

    #[test]
    fn parse_model_ids_empty_and_bad() {
        // 空 data → 空列表
        assert!(AiClient::parse_model_ids(r#"{"object":"list","data":[]}"#)
            .unwrap()
            .is_empty());
        // 缺 data 数组 → 解析错误
        assert!(AiClient::parse_model_ids(r#"{"object":"list"}"#).is_err());
        // 非法 JSON → 解析错误
        assert!(AiClient::parse_model_ids("not json").is_err());
    }

    #[test]
    fn validate_https_rejects_http_and_accepts_https() {
        assert!(AiClient::validate_https("http://api.openai.com").is_err());
        assert!(AiClient::validate_https("https://").is_ok());
        assert!(AiClient::validate_https("https://api.openai.com/v1").is_ok());
        assert!(AiClient::validate_https("ftp://api.openai.com").is_err());
        assert!(AiClient::validate_https("").is_err());
    }

    #[test]
    fn check_ip_private_ipv4() {
        assert!(AiClient::check_ip_private("10.0.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("172.16.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("192.168.1.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("127.0.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("169.254.1.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("224.0.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("192.0.2.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("255.255.255.255".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("0.0.0.0".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("1.1.1.1".parse().unwrap(), "h").is_ok());
        assert!(AiClient::check_ip_private("8.8.8.8".parse().unwrap(), "h").is_ok());
    }

    #[test]
    fn check_ip_private_ipv6() {
        assert!(AiClient::check_ip_private("::1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("::".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("ff02::1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("::ffff:10.0.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("::ffff:127.0.0.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("::ffff:192.168.1.1".parse().unwrap(), "h").is_err());
        assert!(AiClient::check_ip_private("2001:4860:4860::8888".parse().unwrap(), "h").is_ok());
    }

    #[test]
    fn validate_not_private_ip_public_ip_passes() {
        assert!(AiClient::validate_not_private_ip("https://1.1.1.1").is_ok());
    }

    #[test]
    fn validate_not_private_ip_public_domain_passes() {
        // example.com is not blocked; if DNS is unavailable the function still returns Ok.
        assert!(AiClient::validate_not_private_ip("https://example.com").is_ok());
    }

    #[test]
    fn validate_not_private_ip_rejects_private_and_local() {
        assert!(AiClient::validate_not_private_ip("https://192.168.1.1").is_err());
        assert!(AiClient::validate_not_private_ip("https://127.0.0.1").is_err());
        assert!(AiClient::validate_not_private_ip("https://10.0.0.1").is_err());
        assert!(AiClient::validate_not_private_ip("https://[::1]").is_err());
        assert!(AiClient::validate_not_private_ip("https://[::ffff:192.168.1.1]").is_err());
    }

    #[test]
    fn validate_not_private_ip_rejects_metadata_endpoints() {
        assert!(AiClient::validate_not_private_ip("https://169.254.169.254").is_err());
        assert!(AiClient::validate_not_private_ip("https://metadata.google.internal").is_err());
        assert!(AiClient::validate_not_private_ip("https://metadata.google").is_err());
        assert!(AiClient::validate_not_private_ip("https://metadata.azure.internal").is_err());
        assert!(AiClient::validate_not_private_ip("https://100.100.100.200").is_err());
        assert!(AiClient::validate_not_private_ip("https://metadata.tencentyun.com").is_err());
    }

    #[test]
    fn validate_not_private_ip_rejects_bad_urls() {
        assert!(AiClient::validate_not_private_ip("not a url").is_err());
        assert!(AiClient::validate_not_private_ip("https://").is_err());
    }

    #[test]
    fn resolve_and_lock_public_ip_ok() {
        let ep = AiClient::resolve_and_lock("https://1.1.1.1").unwrap();
        assert_eq!(ep.host, "1.1.1.1");
        assert_eq!(ep.port, 443);
    }

    #[test]
    fn resolve_and_lock_custom_port() {
        let ep = AiClient::resolve_and_lock("https://8.8.8.8:8443").unwrap();
        assert_eq!(ep.host, "8.8.8.8");
        assert_eq!(ep.port, 8443);
    }

    #[test]
    fn resolve_and_lock_rejects_private_ip() {
        assert!(AiClient::resolve_and_lock("https://192.168.1.1").is_err());
        assert!(AiClient::resolve_and_lock("https://127.0.0.1:8080").is_err());
    }

    #[test]
    fn resolve_and_lock_rejects_bad_url() {
        assert!(AiClient::resolve_and_lock("not a url").is_err());
        assert!(AiClient::resolve_and_lock("https://").is_err());
    }

    #[test]
    fn validate_tcp_connect_target_matches_resolve_and_lock() {
        assert!(AiClient::validate_tcp_connect_target("https://1.1.1.1").is_ok());
        assert!(AiClient::validate_tcp_connect_target("https://127.0.0.1").is_err());
        assert!(AiClient::validate_tcp_connect_target("https://[::1]").is_err());
    }

    #[test]
    fn read_limited_response_empty() {
        let resp = ureq::Response::new(200, "OK", "").unwrap();
        let text = AiClient::read_limited_response(resp).unwrap();
        assert_eq!(text, "");
    }

    #[test]
    fn read_limited_response_normal_body() {
        let body = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
        let resp = ureq::Response::new(200, "OK", body).unwrap();
        let text = AiClient::read_limited_response(resp).unwrap();
        assert_eq!(text, body);
    }

    fn client_with_empty_key(provider: AiProvider, base_url: &str) -> AiClient {
        let settings = AiSettings {
            provider: provider.as_str().to_string(),
            api_key: "".to_string(),
            base_url: Some(base_url.to_string()),
            model: "model".to_string(),
            ..Default::default()
        };
        AiClient::new(&settings)
    }

    fn thinking_client(provider: &str, thinking: Option<bool>) -> AiClient {
        AiClient::new(&AiSettings {
            provider: provider.to_string(),
            api_key: "k".to_string(),
            base_url: Some("https://1.1.1.1".to_string()),
            model: "m".to_string(),
            thinking,
            ..Default::default()
        })
    }

    #[test]
    fn deepseek_thinking_param_applied() {
        // DeepSeek + thinking=Some(false) → 下发 {"type":"disabled"}
        let mut body = serde_json::json!({});
        thinking_client("deepseek", Some(false)).apply_thinking_param(&mut body);
        assert_eq!(body["thinking"], serde_json::json!({"type": "disabled"}));

        // DeepSeek + thinking=Some(true) → {"type":"enabled"}
        let mut body = serde_json::json!({});
        thinking_client("deepseek", Some(true)).apply_thinking_param(&mut body);
        assert_eq!(body["thinking"], serde_json::json!({"type": "enabled"}));

        // DeepSeek + thinking=None → 不下发（用服务端默认）
        let mut body = serde_json::json!({});
        thinking_client("deepseek", None).apply_thinking_param(&mut body);
        assert!(body.get("thinking").is_none());

        // 非 DeepSeek（kimi）即使设了 thinking 也不下发（DeepSeek 专属参数）
        let mut body = serde_json::json!({});
        thinking_client("kimi", Some(true)).apply_thinking_param(&mut body);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn deepseek_thinking_active_flag() {
        // DeepSeek + thinking=None（服务端默认开启）/ Some(true) → 思考模式，采样参数不下发
        assert!(thinking_client("deepseek", None)
            .config
            .deepseek_thinking_active());
        assert!(thinking_client("deepseek", Some(true))
            .config
            .deepseek_thinking_active());
        // DeepSeek + thinking=Some(false) → 非思考模式，采样参数正常下发
        assert!(!thinking_client("deepseek", Some(false))
            .config
            .deepseek_thinking_active());
        // 非 DeepSeek 厂商永不视为思考模式
        assert!(!thinking_client("kimi", Some(true))
            .config
            .deepseek_thinking_active());
    }

    #[test]
    fn complete_rejects_empty_api_key_deepseek() {
        let client = client_with_empty_key(AiProvider::DeepSeek, "https://1.1.1.1");
        let err = client.complete("prompt").unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    #[test]
    fn complete_rejects_empty_api_key_kimi() {
        let client = client_with_empty_key(AiProvider::Kimi, "https://1.1.1.1");
        let err = client.complete("prompt").unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    #[test]
    fn chat_completion_rejects_empty_api_key_kimi() {
        let client = client_with_empty_key(AiProvider::Kimi, "https://1.1.1.1");
        let err = client
            .chat_completion(&[ChatMessage::user("hi")])
            .unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    #[test]
    fn chat_completion_rejects_empty_api_key_deepseek() {
        let client = client_with_empty_key(AiProvider::DeepSeek, "https://1.1.1.1");
        let err = client
            .chat_completion(&[ChatMessage::user("hi")])
            .unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    #[test]
    fn chat_completion_stream_rejects_empty_api_key_deepseek() {
        let client = client_with_empty_key(AiProvider::DeepSeek, "https://1.1.1.1");
        let err = client
            .chat_completion_stream(&[ChatMessage::user("hi")])
            .unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    #[test]
    fn chat_completion_stream_rejects_empty_api_key_custom() {
        let client = client_with_empty_key(AiProvider::Custom, "https://1.1.1.1");
        let err = client
            .chat_completion_stream(&[ChatMessage::user("hi")])
            .unwrap_err();
        match err {
            AiError::Config(msg) => assert_eq!(msg, "API Key 未设置"),
            other => panic!("expected Config error, got {:?}", other),
        }
    }

    // ==================== extract_stream_token ====================

    #[test]
    fn extract_stream_token_openai() {
        let json = serde_json::json!({
            "choices": [{"delta": {"content": "hello"}}]
        });
        assert_eq!(
            AiClient::extract_stream_token(&json),
            Some("hello".to_string())
        );
    }

    #[test]
    fn extract_stream_token_openai_empty_content() {
        let json = serde_json::json!({
            "choices": [{"delta": {"content": ""}}]
        });
        assert_eq!(AiClient::extract_stream_token(&json), Some("".to_string()));
    }

    #[test]
    fn extract_stream_token_openai_null_content() {
        let json = serde_json::json!({
            "choices": [{"delta": {"content": null}}]
        });
        assert_eq!(AiClient::extract_stream_token(&json), None);
    }

    #[test]
    fn extract_stream_token_anthropic() {
        let json = serde_json::json!({
            "delta": {"text": "world"}
        });
        assert_eq!(
            AiClient::extract_stream_token(&json),
            Some("world".to_string())
        );
    }

    #[test]
    fn extract_stream_token_unrelated() {
        let json = serde_json::json!({"foo": "bar"});
        assert_eq!(AiClient::extract_stream_token(&json), None);
    }

    // ==================== extract_stream_reasoning ====================

    #[test]
    fn extract_stream_reasoning_openai_deepseek() {
        let json = serde_json::json!({
            "choices": [{"delta": {"reasoning_content": "让我想想"}}]
        });
        assert_eq!(
            AiClient::extract_stream_reasoning(&json),
            Some("让我想想".to_string())
        );
    }

    #[test]
    fn extract_stream_reasoning_anthropic_thinking_delta() {
        let json = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "thinking_delta", "thinking": "先分析问题"}
        });
        assert_eq!(
            AiClient::extract_stream_reasoning(&json),
            Some("先分析问题".to_string())
        );
    }

    #[test]
    fn extract_stream_reasoning_anthropic_text_delta_is_none() {
        // 普通回答分片（text_delta）不应被识别为思维链
        let json = serde_json::json!({
            "delta": {"type": "text_delta", "text": "答案"}
        });
        assert_eq!(AiClient::extract_stream_reasoning(&json), None);
    }

    #[test]
    fn extract_stream_reasoning_anthropic_signature_delta_is_none() {
        // 思考签名块不含 thinking 文本，应忽略
        let json = serde_json::json!({
            "delta": {"type": "signature_delta", "signature": "abc"}
        });
        assert_eq!(AiClient::extract_stream_reasoning(&json), None);
    }

    #[test]
    fn extract_stream_reasoning_unrelated_is_none() {
        let json = serde_json::json!({"foo": "bar"});
        assert_eq!(AiClient::extract_stream_reasoning(&json), None);
    }

    #[test]
    fn thinking_delta_not_treated_as_answer_token() {
        let json = serde_json::json!({
            "delta": {"type": "thinking_delta", "thinking": "思考中"}
        });
        assert_eq!(AiClient::extract_stream_token(&json), None);
        assert_eq!(
            AiClient::extract_stream_reasoning(&json),
            Some("思考中".to_string())
        );
    }

    // ==================== AiStreamEvent ====================

    #[test]
    fn ai_stream_event_clone_and_debug() {
        let e = AiStreamEvent::Token("tok".to_string());
        assert_eq!(format!("{:?}", e.clone()), format!("{:?}", e));

        let done = AiStreamEvent::Done;
        match done.clone() {
            AiStreamEvent::Done => {}
            _ => panic!("clone of Done should be Done"),
        }

        let err = AiStreamEvent::Error("oops".to_string());
        match err.clone() {
            AiStreamEvent::Error(msg) => assert_eq!(msg, "oops"),
            _ => panic!("clone of Error should be Error"),
        }

        let dbg = format!("{:?}", AiStreamEvent::Token("x".to_string()));
        assert!(dbg.contains("Token") && dbg.contains("x"));
    }
}
