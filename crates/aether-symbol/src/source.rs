//! 按符号定义行提取其**完整源码块**（整函数/整类），供 AI 精准读取。
//!
//! - C 系语言（Rust/TS/JS/C/C++/Go/Java）：从声明行起做大括号配对，
//!   跳过字符串/字符字面量与注释，避免内容里的花括号误判；
//! - Python：按缩进采集函数/类体（声明行 + 后续更深缩进行与空行）；
//! - 无体的声明（typedef、字段等）只返回声明行本身。

use crate::extract::Lang;

/// 返回 `text` 中 `start_line`（1 起，声明行）处符号的完整源码。
pub fn symbol_source(text: &str, start_line: usize, lang: Lang) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    if start_line == 0 || start_line > lines.len() {
        return None;
    }
    match lang {
        Lang::Python => python_block(&lines, start_line),
        _ => brace_block(&lines, start_line),
    }
}

/// 大括号配对：找到声明行之后第一个 `{`，配对到闭合 `}`。
/// 声明行内如果已闭合（单行函数 `fn f() {}`）也能正确返回。
fn brace_block(lines: &[&str], start_line: usize) -> Option<String> {
    let mut depth = 0i32;
    let mut seen_open = false;
    let mut in_block_comment = false;
    for (i, line) in lines.iter().enumerate().skip(start_line - 1) {
        let mut in_line_comment = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_line_comment {
                continue;
            }
            if in_block_comment {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block_comment = false;
                }
                continue;
            }
            match c {
                '/' => match chars.peek() {
                    Some('/') => {
                        in_line_comment = true;
                        chars.next();
                    }
                    Some('*') => {
                        chars.next();
                        in_block_comment = true;
                    }
                    _ => {}
                },
                // 字符串/字符字面量整体跳过（处理常见转义即可）
                '"' | '\'' | '`' => {
                    let quote = c;
                    while let Some(inner) = chars.next() {
                        if inner == '\\' {
                            chars.next();
                            continue;
                        }
                        if inner == quote {
                            break;
                        }
                        // 单引号内容换行说明不是字符字面量（Rust 生命周期/撇号）
                        if inner == '\n' {
                            break;
                        }
                    }
                }
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => {
                    depth -= 1;
                    if seen_open && depth <= 0 {
                        return Some(lines[start_line - 1..=i].join("\n"));
                    }
                }
                _ => {}
            }
        }
        // 声明到首个 `{` 之间一直没有花括号（如宏结尾、跨行签名后无体）：只给声明行
        if !seen_open && i > start_line - 1 + 30 {
            break;
        }
    }
    // 未找到配对（截断文件等）：返回声明行起最多 80 行
    let end = (start_line - 1 + 80).min(lines.len());
    Some(lines[start_line - 1..end].join("\n"))
}

/// Python 缩进块：声明行 + 后续所有「空行或缩进大于声明行」的行，
/// 尾部空行剔除。
fn python_block(lines: &[&str], start_line: usize) -> Option<String> {
    let def_idx = start_line - 1;
    let def_line = lines[def_idx];
    let def_indent = def_line.len() - def_line.trim_start().len();
    let mut end = def_idx + 1;
    for (i, line) in lines.iter().enumerate().skip(def_idx + 1) {
        let t = line.trim_start();
        if t.is_empty() {
            continue; // 块内空行保留（由 end 追溯决定）
        }
        let indent = line.len() - t.len();
        if indent > def_indent {
            end = i + 1;
        } else {
            break;
        }
    }
    Some(lines[def_idx..end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brace_multiline_fn() {
        let text = "use x;\n\nfn foo() {\n    if a {\n        bar();\n    }\n    baz();\n}\n\nfn after() {}\n";
        let src = symbol_source(text, 3, Lang::Rust).unwrap();
        assert!(src.starts_with("fn foo()"));
        assert!(src.trim_end().ends_with('}'));
        assert!(!src.contains("fn after"));
        assert_eq!(src.lines().count(), 6);
    }

    #[test]
    fn braces_in_strings_and_comments() {
        let text = "fn f() {\n    let s = \"} { {\";\n    // } 注释里的花括号\n    /* { 块注释 } */\n}\n";
        let src = symbol_source(text, 1, Lang::Rust).unwrap();
        assert_eq!(src.lines().count(), 5);
    }

    #[test]
    fn single_line_fn() {
        let text = "fn f() { 1 }\nx\n";
        let src = symbol_source(text, 1, Lang::Rust).unwrap();
        assert_eq!(src, "fn f() { 1 }");
    }

    #[test]
    fn python_indent_block() {
        let text = "import os\n\ndef main():\n    a = 1\n    if a:\n        b = 2\n\n\ndef other():\n    pass\n";
        let src = symbol_source(text, 3, Lang::Python).unwrap();
        assert_eq!(src.lines().count(), 4);
        assert!(src.starts_with("def main"));
        assert!(!src.contains("other"));
    }
}
