use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokenizers::Tokenizer;

/// DeepSeek 模型定价（每 1K tokens 的价格，单位：人民币元）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelPricing {
    /// 输入 token 价格（每 1K tokens）
    pub input_price_per_1k: f64,
    /// 输出 token 价格（每 1K tokens）
    pub output_price_per_1k: f64,
    /// 缓存命中输入 token 价格（每 1K tokens）
    pub cached_input_price_per_1k: f64,
}

impl ModelPricing {
    /// DeepSeek V4 Pro 定价
    pub fn deepseek_v4_pro() -> Self {
        Self {
            input_price_per_1k: 0.002,         // 0.002 元/1K tokens
            output_price_per_1k: 0.006,        // 0.006 元/1K tokens
            cached_input_price_per_1k: 0.0005, // 0.0005 元/1K tokens
        }
    }

    /// DeepSeek V4 Flash 定价
    pub fn deepseek_v4_flash() -> Self {
        Self {
            input_price_per_1k: 0.001,          // 0.001 元/1K tokens
            output_price_per_1k: 0.003,         // 0.003 元/1K tokens
            cached_input_price_per_1k: 0.00025, // 0.00025 元/1K tokens
        }
    }

    /// 根据模型名获取定价
    pub fn from_model_name(model: &str) -> Self {
        match model {
            "deepseek-v4-pro" => Self::deepseek_v4_pro(),
            "deepseek-v4-flash" => Self::deepseek_v4_flash(),
            _ => Self::deepseek_v4_pro(), // 默认使用 Pro 定价
        }
    }
}

/// Token 用量统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 输入 token 数量
    pub input_tokens: usize,
    /// 输出 token 数量
    pub output_tokens: usize,
    /// 缓存命中输入 token 数量
    pub cached_input_tokens: usize,
    /// 总 token 数量
    pub total_tokens: usize,
}

impl TokenUsage {
    pub fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 0,
        }
    }

    /// 计算费用（单位：人民币元）
    pub fn calculate_cost(&self, pricing: &ModelPricing) -> f64 {
        let input_cost = (self.input_tokens as f64 / 1000.0) * pricing.input_price_per_1k;
        let output_cost = (self.output_tokens as f64 / 1000.0) * pricing.output_price_per_1k;
        let cached_input_cost =
            (self.cached_input_tokens as f64 / 1000.0) * pricing.cached_input_price_per_1k;

        input_cost + output_cost + cached_input_cost
    }
}

/// DeepSeek Tokenizer 封装
pub struct DeepSeekTokenizer {
    tokenizer: Tokenizer,
}

impl DeepSeekTokenizer {
    /// 从离线包加载 tokenizer
    pub fn from_offline_package() -> Result<Self, Box<dyn std::error::Error>> {
        // 获取 tokenizer 文件路径
        let tokenizer_path = Self::get_tokenizer_path()?;

        // 加载 tokenizer
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("加载 tokenizer 失败: {}", e))?;

        Ok(Self { tokenizer })
    }

    /// 获取 tokenizer 文件路径
    fn get_tokenizer_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        // 首先尝试从当前目录加载
        let current_dir = std::env::current_dir()?;
        let tokenizer_path = current_dir
            .join("deepseek_v3_tokenizer")
            .join("tokenizer.json");

        if tokenizer_path.exists() {
            return Ok(tokenizer_path);
        }

        // 然后尝试从用户配置目录加载
        let config_dir = dirs::config_dir()
            .ok_or("无法获取用户配置目录")?
            .join("Aether")
            .join("deepseek_v3_tokenizer")
            .join("tokenizer.json");

        if config_dir.exists() {
            return Ok(config_dir);
        }

        // 最后尝试从可执行文件目录加载
        let exe_dir = std::env::current_exe()?
            .parent()
            .ok_or("无法获取可执行文件目录")?
            .join("deepseek_v3_tokenizer")
            .join("tokenizer.json");

        if exe_dir.exists() {
            return Ok(exe_dir);
        }

        Err("未找到 DeepSeek tokenizer 文件".into())
    }

    /// 计算文本的 token 数量
    pub fn count_tokens(&self, text: &str) -> Result<usize, Box<dyn std::error::Error>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| format!("Token 编码失败: {}", e))?;
        Ok(encoding.get_ids().len())
    }

    /// 计算消息列表的 token 数量
    pub fn count_messages_tokens(
        &self,
        messages: &[crate::ChatMessage],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut total_tokens = 0;

        for message in messages {
            // 根据角色添加特殊标记
            let role_prefix = match message.role.as_str() {
                "system" => "<｜begin▁of▁sentence｜>",
                "user" => "<｜User｜>",
                "assistant" => "<｜Assistant｜>",
                _ => "",
            };

            let text = format!("{}{}", role_prefix, message.content);
            total_tokens += self.count_tokens(&text)?;
        }

        Ok(total_tokens)
    }

    /// 估算上下文缓存命中情况
    pub fn estimate_cache_hit(&self, current_input: &str, previous_inputs: &[String]) -> usize {
        // 简化实现：计算当前输入与之前输入的最长公共前缀
        let mut max_prefix_len = 0;

        for prev_input in previous_inputs {
            let prefix_len = Self::longest_common_prefix(current_input, prev_input);
            max_prefix_len = max_prefix_len.max(prefix_len);
        }

        // 将字符长度转换为 token 数量（粗略估算）
        max_prefix_len / 4 // 假设平均每个 token 约 4 个字符
    }

    /// 计算两个字符串的最长公共前缀长度
    fn longest_common_prefix(s1: &str, s2: &str) -> usize {
        let mut len = 0;
        let min_len = s1.len().min(s2.len());

        while len < min_len && s1.as_bytes()[len] == s2.as_bytes()[len] {
            len += 1;
        }

        len
    }
}

/// 全局 tokenizer 实例（懒加载）
static TOKENIZER: std::sync::OnceLock<Result<DeepSeekTokenizer, String>> =
    std::sync::OnceLock::new();

/// 获取全局 tokenizer 实例
pub fn get_tokenizer() -> Result<&'static DeepSeekTokenizer, Box<dyn std::error::Error>> {
    let result = TOKENIZER.get_or_init(|| {
        DeepSeekTokenizer::from_offline_package()
            .map_err(|e| format!("初始化 DeepSeek tokenizer 失败: {}", e))
    });

    match result {
        Ok(tokenizer) => Ok(tokenizer),
        Err(e) => Err(e.clone().into()),
    }
}

/// 计算文本的 token 数量（全局函数）
pub fn count_tokens(text: &str) -> Result<usize, Box<dyn std::error::Error>> {
    get_tokenizer()?.count_tokens(text)
}

/// 计算消息列表的 token 数量（全局函数）
pub fn count_messages_tokens(
    messages: &[crate::ChatMessage],
) -> Result<usize, Box<dyn std::error::Error>> {
    get_tokenizer()?.count_messages_tokens(messages)
}
