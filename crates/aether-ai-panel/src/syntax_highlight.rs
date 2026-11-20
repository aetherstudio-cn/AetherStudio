//! 代码块语法高亮模块
//!
//! 使用 syntect 实现代码块的语法高亮，支持多种编程语言。

use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// 全局语法集合（懒加载）
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// 全局主题集合（懒加载）
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// 获取语法集合
fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// 获取主题集合
fn get_theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// 获取默认主题（深色主题，适合编辑器）
fn get_default_theme() -> &'static Theme {
    let theme_set = get_theme_set();
    // 使用 InspiredGitHub 主题，它有很好的对比度和可读性
    theme_set
        .themes
        .get("InspiredGitHub")
        .or_else(|| theme_set.themes.get("Solarized (dark)"))
        .or_else(|| theme_set.themes.values().next())
        .expect("No theme available")
}

/// 语法高亮样式范围
#[derive(Debug, Clone)]
pub struct HighlightRange {
    /// 起始位置（UTF-16 code unit）
    pub start: u32,
    /// 长度（UTF-16 code unit）
    pub len: u32,
    /// 前景色 (r, g, b, a)
    pub fg_color: (f32, f32, f32, f32),
    /// 是否粗体
    pub bold: bool,
    /// 是否斜体
    pub italic: bool,
}

/// 从代码围栏行提取语言标识
/// 例如：```rust -> Some("rust"), ```python:main.py -> Some("python")
pub fn extract_language_from_fence(fence_line: &str) -> Option<String> {
    let trimmed = fence_line.trim();
    if !trimmed.starts_with("```") {
        return None;
    }

    let after_fence = trimmed.strip_prefix("```")?.trim();

    // 处理 ```language:filename 或 ```language filename 格式
    let lang = if let Some(colon_pos) = after_fence.find(':') {
        after_fence[..colon_pos].trim()
    } else if let Some(space_pos) = after_fence.find(char::is_whitespace) {
        after_fence[..space_pos].trim()
    } else {
        after_fence
    };

    if lang.is_empty() {
        None
    } else {
        Some(lang.to_lowercase())
    }
}

/// 查找语法定义
fn find_syntax(lang: &str) -> Option<&'static SyntaxReference> {
    let syntax_set = get_syntax_set();

    // 尝试精确匹配
    if let Some(syntax) = syntax_set.find_syntax_by_token(lang) {
        return Some(syntax);
    }

    // 尝试常见别名
    let normalized = match lang {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "jsx" => "javascript",
        "c++" => "cpp",
        "cxx" => "cpp",
        "cc" => "cpp",
        "h" => "c",
        "hpp" => "cpp",
        "sh" => "bash",
        "zsh" => "bash",
        "yml" => "yaml",
        "md" => "markdown",
        "dockerfile" => "docker",
        "makefile" => "make",
        "cmakelists" => "cmake",
        _ => lang,
    };

    syntax_set.find_syntax_by_token(normalized)
}

/// 对代码块进行语法高亮
///
/// 返回清洗后的 UTF-16 文本和样式范围列表
pub fn highlight_code_block(code: &str, lang: Option<&str>) -> (Vec<u16>, Vec<HighlightRange>) {
    let mut clean_text = Vec::new();
    let mut ranges = Vec::new();

    // 如果没有指定语言或找不到语法定义，返回纯文本
    let syntax = match lang.and_then(find_syntax) {
        Some(s) => s,
        None => {
            // 无语法高亮，直接转换为 UTF-16
            for c in code.chars() {
                push_utf16(&mut clean_text, c);
            }
            return (clean_text, ranges);
        }
    };

    let theme = get_default_theme();
    let mut highlighter = HighlightLines::new(syntax, theme);

    // 逐行高亮
    for line in LinesWithEndings::from(code) {
        let line_start = clean_text.len() as u32;

        // 高亮当前行
        let highlighted: Vec<(Style, &str)> = highlighter
            .highlight_line(line, get_syntax_set())
            .unwrap_or_default();

        // 处理每个高亮片段
        for (style, text) in highlighted {
            let segment_start = clean_text.len() as u32;

            // 添加文本
            for c in text.chars() {
                push_utf16(&mut clean_text, c);
            }

            let segment_len = clean_text.len() as u32 - segment_start;

            if segment_len > 0 {
                // 转换颜色
                let fg = style.foreground;
                let fg_color = (
                    fg.r as f32 / 255.0,
                    fg.g as f32 / 255.0,
                    fg.b as f32 / 255.0,
                    fg.a as f32 / 255.0,
                );

                // 检查字体样式
                let bold = style.font_style.contains(FontStyle::BOLD);
                let italic = style.font_style.contains(FontStyle::ITALIC);

                ranges.push(HighlightRange {
                    start: segment_start,
                    len: segment_len,
                    fg_color,
                    bold,
                    italic,
                });
            }
        }

        // 确保行尾换行符被添加
        if clean_text.len() as u32 == line_start && line.ends_with('\n') {
            clean_text.push(b'\n' as u16);
        }
    }

    (clean_text, ranges)
}

/// 将字符转换为 UTF-16 并添加到缓冲区
fn push_utf16(buf: &mut Vec<u16>, c: char) {
    let mut tmp = [0u16; 2];
    for u in c.encode_utf16(&mut tmp) {
        buf.push(*u);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_language_from_fence() {
        assert_eq!(extract_language_from_fence("```rust"), Some("rust".to_string()));
        assert_eq!(
            extract_language_from_fence("```python:main.py"),
            Some("python".to_string())
        );
        assert_eq!(
            extract_language_from_fence("```javascript src/main.js"),
            Some("javascript".to_string())
        );
        assert_eq!(extract_language_from_fence("```"), None);
        assert_eq!(extract_language_from_fence("not a fence"), None);
    }

    #[test]
    fn test_find_syntax() {
        assert!(find_syntax("rust").is_some());
        assert!(find_syntax("python").is_some());
        assert!(find_syntax("javascript").is_some());
        assert!(find_syntax("rs").is_some()); // 别名
        assert!(find_syntax("py").is_some()); // 别名
        assert!(find_syntax("nonexistent_language").is_none());
    }

    #[test]
    fn test_highlight_code_block() {
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let (text, ranges) = highlight_code_block(code, Some("rust"));

        assert!(!text.is_empty());
        assert!(!ranges.is_empty());

        // 验证文本内容
        let text_str = String::from_utf16_lossy(&text);
        assert!(text_str.contains("fn"));
        assert!(text_str.contains("main"));
    }

    #[test]
    fn test_highlight_without_language() {
        let code = "some code";
        let (text, ranges) = highlight_code_block(code, None);

        assert!(!text.is_empty());
        assert!(ranges.is_empty()); // 无语言时无高亮
    }

    #[test]
    fn test_highlight_with_invalid_language() {
        let code = "some code";
        let (text, ranges) = highlight_code_block(code, Some("invalid_lang"));

        assert!(!text.is_empty());
        assert!(ranges.is_empty()); // 无效语言时无高亮
    }
}
