use super::common::{skip_block_comment, skip_quoted, skip_whitespace};
use super::{LexemeSpan, Lexer, TokenKind};

/// CSS 词法分析器
///
/// 核心状态机：通过 `in_block` 跟踪当前处于选择器区（`{}` 外）还是声明区（`{}` 内），
/// 同一标识符在不同区域映射为不同 TokenKind（选择器 → Keyword，属性名 → Attribute）。
pub struct CssLexer;

impl CssLexer {
    pub fn new() -> Self {
        Self
    }

    fn lex_next(&self, bytes: &[u8], pos: usize, in_block: bool) -> (LexemeSpan, usize) {
        if pos >= bytes.len() {
            return (LexemeSpan::new(pos, 0, TokenKind::EOF), pos);
        }

        let ch = bytes[pos];

        match ch {
            b' ' | b'\t' | b'\r' => {
                let end = skip_whitespace(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::Whitespace), end)
            }
            b'\n' => (LexemeSpan::new(pos, 1, TokenKind::Newline), pos + 1),

            // 块注释 /* ... */
            b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'*' => {
                let end = skip_block_comment(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::BlockComment),
                    end,
                )
            }

            // @规则：@media, @import, @keyframes, @font-face 等
            b'@' => {
                let end = skip_at_rule(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::Preprocessor),
                    end,
                )
            }

            // 字符串
            b'"' => {
                let end = skip_quoted(bytes, pos, b'"');
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::StringLiteral),
                    end,
                )
            }
            b'\'' => {
                let end = skip_quoted(bytes, pos, b'\'');
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::StringLiteral),
                    end,
                )
            }

            // 十六进制颜色 #fff / #1a3a6b（仅在声明区或选择器区的 #id 之后跟十六进制字符时）
            b'#' => {
                let end = skip_hex_or_id_selector(bytes, pos);
                let text = std::str::from_utf8(&bytes[pos..end]).unwrap_or("");
                let kind = if in_block && is_hex_color(text) {
                    TokenKind::NumberLiteral
                } else {
                    // 选择器区的 #id
                    TokenKind::Keyword
                };
                (LexemeSpan::new(pos, end - pos, kind), end)
            }

            // 数字（含小数、负数、百分比、单位）
            b'0'..=b'9' => {
                let end = skip_css_number(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::NumberLiteral),
                    end,
                )
            }
            b'.' if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit() => {
                // .5 形式的小数
                let end = skip_css_number(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::NumberLiteral),
                    end,
                )
            }
            b'-' if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit() => {
                // 负数
                let end = skip_css_number(bytes, pos);
                (
                    LexemeSpan::new(pos, end - pos, TokenKind::NumberLiteral),
                    end,
                )
            }

            // CSS 变量 --var-name
            b'-' if pos + 1 < bytes.len() && bytes[pos + 1] == b'-' => {
                let end = skip_css_identifier(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::Identifier), end)
            }

            // !important
            b'!' => {
                let end = skip_important(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::Keyword), end)
            }

            // 类选择器 .class-name（在选择器区）
            b'.' if !in_block => {
                let end = skip_css_identifier(bytes, pos);
                (LexemeSpan::new(pos, end - pos, TokenKind::Keyword), end)
            }

            // 标识符：属性名 / 选择器 / 值中的关键字 / 函数名
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let end = skip_css_identifier(bytes, pos);
                let text = std::str::from_utf8(&bytes[pos..end]).unwrap_or("");
                let kind = if in_block {
                    classify_block_identifier(bytes, pos, end, text)
                } else {
                    // 选择器区：元素选择器
                    TokenKind::Keyword
                };
                (LexemeSpan::new(pos, end - pos, kind), end)
            }

            // 函数调用（声明区中标识符后跟 `(`）
            // 上面已处理：classify_block_identifier 会检测 `ident(` 模式

            // 块分隔
            b'{' | b'}' => (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1),

            // 声明区分隔符
            b':' | b';' | b',' => (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1),

            // 组合选择器 / 运算符
            b'>' | b'+' | b'~' => (LexemeSpan::new(pos, 1, TokenKind::Operator), pos + 1),

            // 通用兄弟选择器 / 通配符
            b'*' => (LexemeSpan::new(pos, 1, TokenKind::Operator), pos + 1),

            // 伪类/伪元素双冒号
            // 单冒号已在上面处理

            // 括号
            b'(' | b')' | b'[' | b']' => (LexemeSpan::new(pos, 1, TokenKind::Punctuation), pos + 1),

            // 百分比（独立出现，如 `100%` 中的 `%` 已被 skip_css_number 吞掉）
            b'%' => (LexemeSpan::new(pos, 1, TokenKind::Operator), pos + 1),

            // 其他
            _ => {
                let len = crate::lexer::utf8_char_len(bytes[pos]);
                (LexemeSpan::new(pos, len, TokenKind::Unknown), pos + len)
            }
        }
    }
}

impl Lexer for CssLexer {
    fn lex_full(&self, text: &str) -> Vec<LexemeSpan> {
        let mut tokens = Vec::with_capacity(text.len() / 4 + 1);
        let bytes = text.as_bytes();
        let mut pos = 0;
        let mut in_block = false;
        // 嵌套深度（@media 等 @规则内可嵌套块）
        let mut depth: u32 = 0;

        while pos < bytes.len() {
            let (token, new_pos) = self.lex_next(bytes, pos, in_block);
            match token.kind {
                TokenKind::Punctuation => {
                    if bytes[pos] == b'{' {
                        depth += 1;
                        in_block = true;
                    } else if bytes[pos] == b'}' {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            in_block = false;
                        }
                    }
                }
                _ => {}
            }
            tokens.push(token);
            pos = new_pos;
        }

        tokens
    }
}

impl Default for CssLexer {
    fn default() -> Self {
        Self::new()
    }
}

/// 跳过 @规则名称：@media, @import, @keyframes 等
fn skip_at_rule(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos + 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
        i += 1;
    }
    i
}

/// 跳过 # 后的十六进制颜色或 ID 选择器
fn skip_hex_or_id_selector(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos + 1;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    i
}

/// 判断 # 后的文本是否是十六进制颜色值
fn is_hex_color(text: &str) -> bool {
    let hex = &text[1..]; // 去掉 '#'
    !hex.is_empty() && hex.len() <= 8 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// 跳过 CSS 数字：整数、小数、百分比、带单位
fn skip_css_number(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    // 可选负号
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    // 整数部分
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // 小数部分
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    // 百分比
    if i < bytes.len() && bytes[i] == b'%' {
        i += 1;
        return i;
    }
    // 单位（px, em, rem, vh, vw, s, ms, deg, fr, ch, ex, cm, mm, in, pt, pc 等）
    if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
    }
    i
}

/// 跳过 CSS 标识符（允许字母、数字、连字符、下划线）
fn skip_css_identifier(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    // 跳过前导的 . # - 等前缀字符
    if i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b'#' || bytes[i] == b'-') {
        i += 1;
        // CSS 变量 -- 前缀
        if i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        }
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    i
}

/// 跳过 !important
fn skip_important(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos + 1;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    i
}

/// 声明区中标识符的分类：
/// - 后跟 `(` → 函数调用（Function）
/// - 后跟 `:` → 属性名（Attribute）
/// - 常见 CSS 值关键字 → Keyword
/// - 其他 → Identifier（值）
fn classify_block_identifier(bytes: &[u8], _start: usize, end: usize, text: &str) -> TokenKind {
    // 检查是否紧跟 `(`（函数调用）
    let mut i = end;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'(' {
        return TokenKind::Function;
    }

    // 检查是否紧跟 `:`（属性名）
    if i < bytes.len() && bytes[i] == b':' {
        // 排除伪类选择器（如 :hover, ::before）——它们在选择器区，不会在声明区出现
        return TokenKind::Attribute;
    }

    // 常见 CSS 值关键字
    if is_css_keyword(text) {
        return TokenKind::Keyword;
    }

    TokenKind::Identifier
}

/// 常见 CSS 值关键字
fn is_css_keyword(text: &str) -> bool {
    matches!(
        text,
        "auto"
            | "none"
            | "inherit"
            | "initial"
            | "unset"
            | "revert"
            | "solid"
            | "dashed"
            | "dotted"
            | "double"
            | "hidden"
            | "visible"
            | "absolute"
            | "relative"
            | "fixed"
            | "sticky"
            | "static"
            | "flex"
            | "grid"
            | "block"
            | "inline"
            | "inline-block"
            | "inline-flex"
            | "inline-grid"
            | "center"
            | "left"
            | "right"
            | "top"
            | "bottom"
            | "middle"
            | "baseline"
            | "bold"
            | "normal"
            | "italic"
            | "oblique"
            | "underline"
            | "overline"
            | "line-through"
            | "uppercase"
            | "lowercase"
            | "capitalize"
            | "transparent"
            | "currentColor"
            | "important"
            | "default"
            | "wrap"
            | "nowrap"
            | "wrap-reverse"
            | "row"
            | "column"
            | "row-reverse"
            | "column-reverse"
            | "space-between"
            | "space-around"
            | "space-evenly"
            | "flex-start"
            | "flex-end"
            | "stretch"
            | "cover"
            | "contain"
            | "repeat"
            | "no-repeat"
            | "repeat-x"
            | "repeat-y"
            | "round"
            | "space"
            | "scroll"
            | "local"
            | "ease"
            | "linear"
            | "ease-in"
            | "ease-out"
            | "ease-in-out"
            | "infinite"
            | "alternate"
            | "forwards"
            | "backwards"
            | "both"
            | "running"
            | "paused"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<TokenKind> {
        CssLexer::new()
            .lex_full(text)
            .into_iter()
            .map(|s| s.kind)
            .collect()
    }

    #[test]
    fn test_css_empty() {
        assert!(CssLexer::new().lex_full("").is_empty());
    }

    #[test]
    fn test_css_block_comment() {
        let tokens = CssLexer::new().lex_full("/* comment */");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::BlockComment);
    }

    #[test]
    fn test_css_element_selector() {
        let ks = kinds("body { }");
        assert!(ks.contains(&TokenKind::Keyword)); // body
        assert!(ks.contains(&TokenKind::Punctuation)); // { }
    }

    #[test]
    fn test_css_class_selector() {
        let ks = kinds(".container { }");
        assert!(ks.contains(&TokenKind::Keyword)); // .container
    }

    #[test]
    fn test_css_id_selector() {
        let ks = kinds("#main { }");
        assert!(ks.contains(&TokenKind::Keyword)); // #main
    }

    #[test]
    fn test_css_property_and_value() {
        let tokens = CssLexer::new().lex_full("a { color: red; }");
        let ks: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert!(ks.contains(&TokenKind::Attribute)); // color
        assert!(ks.contains(&TokenKind::Punctuation)); // :
        assert!(ks.contains(&TokenKind::Keyword)); // red（CSS 关键字）
    }

    #[test]
    fn test_css_number_with_unit() {
        let ks = kinds("a { margin: 10px; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // 10px
    }

    #[test]
    fn test_css_percentage() {
        let ks = kinds("a { width: 100%; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // 100%
    }

    #[test]
    fn test_css_hex_color() {
        let ks = kinds("a { color: #fff; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // #fff
    }

    #[test]
    fn test_css_hex_color_long() {
        let ks = kinds("a { color: #1a3a6b; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // #1a3a6b
    }

    #[test]
    fn test_css_at_rule() {
        let ks = kinds("@media (max-width: 768px) { }");
        assert!(ks.contains(&TokenKind::Preprocessor)); // @media
    }

    #[test]
    fn test_css_string_value() {
        let ks = kinds(r#"a { font-family: 'Microsoft YaHei'; }"#);
        assert!(ks.contains(&TokenKind::StringLiteral));
    }

    #[test]
    fn test_css_important() {
        let ks = kinds("a { color: red !important; }");
        assert!(ks.contains(&TokenKind::Keyword)); // !important
    }

    #[test]
    fn test_css_variable() {
        let ks = kinds("a { color: var(--primary); }");
        assert!(ks.contains(&TokenKind::Identifier)); // --primary
    }

    #[test]
    fn test_css_function_call() {
        let ks = kinds("a { background: rgba(0, 0, 0, 0.5); }");
        assert!(ks.contains(&TokenKind::Function)); // rgba
    }

    #[test]
    fn test_css_nested_block() {
        let ks = kinds("@media (max-width: 768px) { .container { display: none; } }");
        assert!(ks.contains(&TokenKind::Preprocessor)); // @media
        assert!(ks.contains(&TokenKind::Keyword)); // .container
        assert!(ks.contains(&TokenKind::Attribute)); // display
    }

    #[test]
    fn test_css_combinator() {
        let ks = kinds("div > span { }");
        assert!(ks.contains(&TokenKind::Operator)); // >
    }

    #[test]
    fn test_css_multiple_selectors() {
        let ks = kinds("h1, h2, h3 { }");
        assert!(ks.contains(&TokenKind::Keyword)); // h1, h2, h3
        assert!(ks.contains(&TokenKind::Punctuation)); // ,
    }

    #[test]
    fn test_css_zero_value() {
        let ks = kinds("a { margin: 0; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // 0
    }

    #[test]
    fn test_css_negative_value() {
        let ks = kinds("a { margin: -10px; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // -10px
    }

    #[test]
    fn test_css_decimal_value() {
        let ks = kinds("a { opacity: 0.5; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // 0.5
    }

    #[test]
    fn test_css_dot_decimal() {
        let ks = kinds("a { opacity: .5; }");
        assert!(ks.contains(&TokenKind::NumberLiteral)); // .5
    }
}
