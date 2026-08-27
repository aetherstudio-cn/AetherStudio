//! 文本嵌入模块（ONNX Runtime）
//!
//! 使用 sentence-transformers 模型将文本编码为稠密向量，
//! 供 MemoryStore 的向量语义检索使用。
//!
//! 全局单例懒加载：调用 [`init_embedding_model`] 后，
//! [`embed_text`] 使用真实模型；未初始化时回退到 n-gram 哈希向量
//! （保证维度一致、检索链路可用，但语义质量有限）。

use std::sync::Mutex;

use ort::value::Tensor;

// ============================================================================
// 嵌入模型管理器（ONNX Runtime）
// ============================================================================

/// 嵌入模型管理器（单例，懒加载）
pub struct EmbeddingModel {
    /// ONNX 会话
    session: ort::session::Session,
    /// Tokenizer
    tokenizer: tokenizers::Tokenizer,
}

/// 从推理输出提取嵌入向量：优先 pooler_output；否则对 last_hidden_state 做均值池化
/// （BGE 系列 ONNX 导出通常只有 last_hidden_state）
fn extract_embedding(outputs: &ort::session::SessionOutputs<'_>) -> Result<Vec<f32>, String> {
    // 优先 pooler_output；注意 ort 的 Index 访问在输出缺失时会 panic，必须用 get()
    if let Some(v) = outputs.get("pooler_output") {
        if let Ok((_shape, data)) = v.try_extract_tensor::<f32>() {
            return Ok(data.to_vec());
        }
    }
    let lhs = outputs
        .get("last_hidden_state")
        .ok_or_else(|| "模型既无 pooler_output 也无 last_hidden_state 输出".to_string())?;
    let (shape, data) = lhs
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("提取输出失败: {}", e))?;
    let dims = shape.len();
    if dims != 3 {
        return Err(format!("last_hidden_state 维度异常: {:?}", shape));
    }
    let (seq, hidden) = (shape[1] as usize, shape[2] as usize);
    let mut pooled = vec![0.0f32; hidden];
    for t in 0..seq {
        for h in 0..hidden {
            pooled[h] += data[t * hidden + h];
        }
    }
    for h in &mut pooled {
        *h /= seq as f32;
    }
    Ok(pooled)
}

impl EmbeddingModel {
    /// 模型维度（bge-small-zh-v1.5 为 512 维）
    pub const DIM: usize = 512;

    /// 从模型文件加载
    pub fn from_files(model_path: &str, tokenizer_path: &str) -> Result<Self, String> {
        let session = ort::session::Session::builder()
            .map_err(|e| format!("ONNX 会话构建失败: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| format!("加载 ONNX 模型失败: {}", e))?;

        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("加载 Tokenizer 失败: {}", e))?;

        Ok(Self { session, tokenizer })
    }

    /// 将文本编码为向量
    pub fn encode(&mut self, text: &str) -> Result<Vec<f32>, String> {
        // 1. Tokenize
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenize 失败: {}", e))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();

        let seq_len = input_ids.len();

        // 2. 构建输入张量
        let ids_shape = vec![1i64, seq_len as i64];
        let input_ids_tensor = Tensor::from_array((ids_shape.clone(), input_ids.clone()))
            .map_err(|e| format!("构建 input_ids 张量失败: {}", e))?;
        let attention_mask_tensor = Tensor::from_array((ids_shape.clone(), attention_mask.clone()))
            .map_err(|e| format!("构建 attention_mask 张量失败: {}", e))?;

        // 3. 运行推理（命名输入）
        //    多数 Bert 风格导出要求 token_type_ids；部分导出不含该输入，失败时重试不带它。
        //    注意：SessionOutputs 借用 session，必须在独立作用域内提取为自有 Vec 后再重试
        let mut extracted: Option<Vec<f32>> = None;
        {
            let type_ids_tensor = Tensor::from_array((ids_shape.clone(), type_ids))
                .map_err(|e| format!("构建 token_type_ids 张量失败: {}", e))?;
            match self.session.run(ort::inputs! {
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => type_ids_tensor
            }) {
                Ok(outputs) => extracted = Some(extract_embedding(&outputs)?),
                Err(_) => {}
            }
        }
        let mut vec: Vec<f32> = match extracted {
            Some(v) => v,
            None => {
                let retry_input_ids = Tensor::from_array((ids_shape.clone(), input_ids))
                    .map_err(|e| format!("构建 input_ids 张量失败: {}", e))?;
                let retry_mask = Tensor::from_array((ids_shape, attention_mask))
                    .map_err(|e| format!("构建 attention_mask 张量失败: {}", e))?;
                let outputs = self
                    .session
                    .run(ort::inputs! {
                        "input_ids" => retry_input_ids,
                        "attention_mask" => retry_mask
                    })
                    .map_err(|e| format!("ONNX 推理失败: {}", e))?;
                extract_embedding(&outputs)?
            }
        };

        // 4. L2 归一化（BGE 模型要求归一化后用内积/余弦检索）
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vec {
                *x /= norm;
            }
        }

        Ok(vec)
    }

    /// 批量编码
    pub fn encode_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.encode(text)?);
        }
        Ok(results)
    }
}

// ============================================================================
// 全局单例
// ============================================================================

/// 全局嵌入模型实例（懒加载，Mutex 包装以支持可变借用）
static EMBEDDING_MODEL: Mutex<Option<EmbeddingModel>> = Mutex::new(None);

/// 初始化全局嵌入模型
pub fn init_embedding_model(model_path: &str, tokenizer_path: &str) -> Result<(), String> {
    let mut guard = EMBEDDING_MODEL
        .lock()
        .map_err(|e| format!("锁获取失败: {}", e))?;
    *guard = Some(EmbeddingModel::from_files(model_path, tokenizer_path)?);
    Ok(())
}

/// 全局模型是否已初始化
pub fn embedding_model_ready() -> bool {
    EMBEDDING_MODEL.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// 约定的用户模型目录：%CONFIG%/Aether/models/bge-small-zh-v1.5/
/// 内含 model.onnx + tokenizer.json（手动下载放这里）
pub fn default_model_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Aether")
        .join("models")
        .join("bge-small-zh-v1.5")
}

/// 安装包自带的模型目录：exe 同级 resources/models/bge-small-zh-v1.5/
/// （NSIS 可选组件 SecModel 的安装位置）
fn bundled_model_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|exe| {
        exe.parent().map(|dir| {
            dir.join("resources")
                .join("models")
                .join("bge-small-zh-v1.5")
        })
    })
}

/// 模型查找候选目录：安装包自带（优先）→ 用户配置目录
fn model_candidate_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(bundled) = bundled_model_dir() {
        dirs.push(bundled);
    }
    dirs.push(default_model_dir());
    dirs
}

/// 启动时尝试初始化默认模型。
/// 依次查找安装包自带目录与用户配置目录；均不存在时不视为错误
/// （回退 n-gram 哈希），仅打印下载指引。返回 true 表示真实模型已加载。
pub fn try_init_default_model() -> bool {
    // 逐个候选目录尝试：文件齐全才加载，加载失败继续试下一个
    for dir in model_candidate_dirs() {
        let model_path = dir.join("model.onnx");
        let tokenizer_path = dir.join("tokenizer.json");
        if !model_path.exists() || !tokenizer_path.exists() {
            continue;
        }
        match init_embedding_model(
            &model_path.to_string_lossy(),
            &tokenizer_path.to_string_lossy(),
        ) {
            Ok(()) => {
                eprintln!("[Embedding] 嵌入模型已加载: {}", dir.display());
                return true;
            }
            Err(e) => {
                eprintln!("[Embedding] 模型加载失败（{}）: {}", dir.display(), e);
            }
        }
    }

    eprintln!(
        "[Embedding] 未找到嵌入模型，语义检索将使用 n-gram 回退。\n\
         [Embedding] 可重新运行安装程序勾选“Semantic Search Model”组件，\n\
         [Embedding] 或手动下载 bge-small-zh-v1.5 的 ONNX 版本，放置 model.onnx 与 tokenizer.json 到 {}。\n\
         [Embedding] 参考: https://huggingface.co/BAAI/bge-small-zh-v1.5",
        default_model_dir().display()
    );
    false
}

/// 文本 → 向量（优先 ONNX 模型，未初始化时回退 n-gram 哈希）
pub fn embed_text(text: &str) -> Vec<f32> {
    // 尝试使用 ONNX 嵌入模型
    if let Ok(mut guard) = EMBEDDING_MODEL.lock() {
        if let Some(ref mut model) = *guard {
            if let Ok(embedding) = model.encode(text) {
                let mut v = vec![0.0f32; EmbeddingModel::DIM];
                let len = embedding.len().min(EmbeddingModel::DIM);
                v[..len].copy_from_slice(&embedding[..len]);
                return v;
            }
        }
    }

    // 回退：字符 n-gram 哈希向量（保证维度一致，链路可用）
    let mut v = vec![0.0f32; EmbeddingModel::DIM];
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(2) {
        let hash =
            ((bytes[i] as u32) * 31 + (bytes[i + 1] as u32) * 17 + (bytes[i + 2] as u32)) as usize;
        let idx = hash % EmbeddingModel::DIM;
        v[idx] += 1.0;
    }
    // 归一化
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_text_fallback_dim() {
        // 未初始化模型时走 n-gram 回退，维度仍应为 DIM
        let v = embed_text("你好，世界");
        assert_eq!(v.len(), EmbeddingModel::DIM);
        // 归一化向量模长应约为 1
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}
