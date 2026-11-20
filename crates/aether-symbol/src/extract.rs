//! 符号提取：按行启发式识别各语言的声明（函数/类/结构体/变量/常量等）。
//!
//! 设计取舍（与 LSP 的差异）：
//! - 不依赖外部语言服务器进程（rust-analyzer 常驻内存常达 GB 级），零依赖、纯 std；
//! - 不做类型推导与作用域分析，只提取**声明行**：名称 + 种类 + 行号 + 签名文本；
//! - 提取结果是"给 AI 的目录"，让 AI 用名字直接跳到定义，而不是全量读文件。

/// 符号种类
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    TypeAlias,
    Constant,
    Static,
    Variable,
    Module,
    Impl,
}

impl SymbolKind {
    /// 面向 AI/用户的简短标签（小写英文单词）
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Constant => "const",
            SymbolKind::Static => "static",
            SymbolKind::Variable => "var",
            SymbolKind::Module => "module",
            SymbolKind::Impl => "impl",
        }
    }
}

/// 一个符号声明：只存定位所需的最小信息，内存占用极小
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    /// 相对工作区根目录的路径（`/` 分隔）
    pub path: String,
    /// 声明所在行（1 起）
    pub line: usize,
    /// 声明行文本（截断至 160 字符），供 AI 直接看懂签名
    pub signature: String,
}

/// 支持的语言（按扩展名映射）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    C,
    Cpp,
    Go,
    Java,
}

impl Lang {
    /// 由文件名扩展名推断语言；不支持返回 None（不入库）
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = path.rsplit('.').next().unwrap_or("");
        Some(match ext {
            "rs" => Lang::Rust,
            "py" | "pyw" => Lang::Python,
            "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "c" | "h" => Lang::C,
            "cpp" | "cxx" | "cc" | "hpp" | "hh" | "hxx" => Lang::Cpp,
            "go" => Lang::Go,
            "java" => Lang::Java,
            _ => return None,
        })
    }
}

/// 提取一个文件中的所有符号声明。
pub fn extract_symbols(rel_path: &str, text: &str) -> Vec<Symbol> {
    let lang = match Lang::from_path(rel_path) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut in_block_comment = false;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim_end();
        let t = line.trim_start();
        let indent = line.len() - t.len();
        // 粗略跳过注释行（块注释按 /* */ 配对跟踪，误判可接受）
        if in_block_comment {
            if let Some(pos) = t.find("*/") {
                in_block_comment = false;
                let rest = t[pos + 2..].trim_start();
                if !rest.is_empty() {
                    extract_line(lang, indent, rest, idx + 1, &mut out);
                }
            }
            continue;
        }
        if t.is_empty()
            || t.starts_with("//")
            || t.starts_with("///")
            || (lang == Lang::Python && t.starts_with('#'))
            || t.starts_with('*')
        {
            continue;
        }
        if t.starts_with("/*") {
            if !t.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        extract_line(lang, indent, t, idx + 1, &mut out);
    }
    out
}

/// 单行声明识别。`indent` 为行缩进（Python 判定模块级变量用），`line_no` 1 起。
fn extract_line(lang: Lang, indent: usize, t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    match lang {
        Lang::Rust => extract_rust(indent, t, line_no, out),
        Lang::Python => extract_python(indent, t, line_no, out),
        Lang::TypeScript | Lang::JavaScript => extract_ts_js(t, line_no, out),
        Lang::C | Lang::Cpp => extract_c_cpp(indent, t, line_no, out),
        Lang::Go => extract_go(t, line_no, out),
        Lang::Java => extract_java(indent, t, line_no, out),
    }
}

/// 从 `rest` 中取出紧随关键词后的标识符名（去除泛型/参数部分）
fn ident_after(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(rest.len());
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// `fn foo(` / `struct Foo<` 等：定位关键词并取名字（Rust 声明可带属性/可见性前缀）
fn kw_name(t: &str, kw: &str) -> Option<String> {
    // 要求关键词作为完整单词出现，其后是空白或 < ( : 等
    let mut search = t;
    while let Some(pos) = search.find(kw) {
        let before_ok =
            pos == 0 || !search[..pos].ends_with(|c: char| c.is_alphanumeric() || c == '_');
        let rest = &search[pos + kw.len()..];
        let after_ok = rest.starts_with(' ') || rest.starts_with('\t');
        if before_ok && after_ok {
            if let Some(name) = ident_after(rest) {
                return Some(name);
            }
        }
        search = &search[pos + kw.len()..];
    }
    None
}

fn signature(t: &str) -> String {
    let s = t.trim();
    if s.chars().count() > 160 {
        let cut: String = s.chars().take(157).collect();
        format!("{}...", cut)
    } else {
        s.to_string()
    }
}

fn push(
    out: &mut Vec<Symbol>,
    kind: SymbolKind,
    name: String,
    line_no: usize,
    t: &str,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    out.push(Symbol {
        kind,
        name,
        path: String::new(), // 由索引层统一填充
        line: line_no,
        signature: signature(t),
    });
}

/// 每文件符号上限，防止生成物/超大文件撑爆索引
const MAX_SYMBOLS_PER_FILE: usize = 2000;

fn extract_rust(indent: usize, t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    if indent > 0 {
        // 仅索引顶层/impl 块内的声明，函数体局部变量噪音大且无检索价值
        return;
    }
    // 去除属性与可见性前缀后统一匹配
    let body = strip_rust_prefixes(t);
    if let Some(name) = kw_name(body, "fn") {
        push(
            out,
            SymbolKind::Function,
            name,
            line_no,
            t,
            MAX_SYMBOLS_PER_FILE,
        );
        return;
    }
    for (kw, kind) in [
        ("struct", SymbolKind::Struct),
        ("enum", SymbolKind::Enum),
        ("trait", SymbolKind::Trait),
        ("type", SymbolKind::TypeAlias),
        ("mod", SymbolKind::Module),
    ] {
        if let Some(name) = kw_name(body, kw) {
            push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            return;
        }
    }
    if let Some(rest) = body.strip_prefix("impl") {
        // impl Foo / impl<T> Trait for Foo —— 记录目标类型名供检索
        let seg = rest.split('{').next().unwrap_or("").trim();
        let name = seg
            .rsplit(char::is_whitespace)
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(seg)
            .to_string();
        push(
            out,
            SymbolKind::Impl,
            format!("impl {}", name),
            line_no,
            t,
            MAX_SYMBOLS_PER_FILE,
        );
        return;
    }
    if let Some(rest) = body.strip_prefix("const ") {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::Constant,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
    } else if let Some(rest) = body
        .strip_prefix("static ")
        .or_else(|| body.strip_prefix("pub static "))
    {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::Static,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
    }
}

/// 去掉 `#[...]`、`#!`、`pub`、`pub(...)`、`unsafe`、`async`、`extern "..."` 等前缀
fn strip_rust_prefixes(t: &str) -> &str {
    let mut s = t.trim_start();
    loop {
        let next = if let Some(rest) = s.strip_prefix("#[") {
            rest.split_once(']')
                .map(|(_, r)| r.trim_start())
                .unwrap_or("")
        } else if let Some(rest) = s.strip_prefix("pub") {
            let rest = rest.trim_start();
            if rest.starts_with('(') {
                match rest.split_once(')') {
                    Some((_, r)) => r.trim_start(),
                    None => return "",
                }
            } else {
                rest
            }
        } else if let Some(rest) = s.strip_prefix("unsafe ") {
            rest.trim_start()
        } else if let Some(rest) = s.strip_prefix("async ") {
            rest.trim_start()
        } else if let Some(rest) = s.strip_prefix("extern ") {
            rest.split_once(' ')
                .map(|(_, r)| r.trim_start())
                .unwrap_or("")
        } else {
            break;
        };
        s = next;
    }
    s
}

fn extract_python(indent: usize, t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    let body = t.strip_prefix("@").map(|_| "").unwrap_or(t); // 装饰器行跳过
    if body.is_empty() {
        return;
    }
    if let Some(rest) = body.strip_prefix("def ") {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::Function,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    if let Some(rest) = body.strip_prefix("class ") {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::Class,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    // 模块级赋值：全大写视为常量，其余视为变量
    if indent == 0 {
        if let Some((lhs, _)) = body.split_once('=') {
            let lhs = lhs.trim();
            if lhs.ends_with(char::is_whitespace) {
                return;
            }
            let valid = !lhs.is_empty()
                && lhs.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !lhs.starts_with(char::is_numeric);
            if valid {
                let kind = if lhs.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                };
                push(out, kind, lhs.to_string(), line_no, t, MAX_SYMBOLS_PER_FILE);
            }
        }
    }
}

fn extract_ts_js(t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    let body = strip_ts_prefixes(t);
    if let Some(rest) = body.strip_prefix("function ") {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::Function,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    for (kw, kind) in [
        ("class", SymbolKind::Class),
        ("interface", SymbolKind::Interface),
    ] {
        if let Some(name) = kw_name(body, kw) {
            push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            return;
        }
    }
    if let Some(rest) = body.strip_prefix("type ") {
        if let Some(name) = ident_after(rest) {
            push(
                out,
                SymbolKind::TypeAlias,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    // const/let/var NAME = ... ：箭头函数记为 Function，否则记为变量/常量
    for kw in ["const ", "let ", "var "] {
        if let Some(rest) = body.strip_prefix(kw) {
            if let Some(name) = ident_after(rest) {
                let kind = if let Some(eq_pos) = rest.find('=') {
                    let rhs = &rest[eq_pos + 1..];
                    if rhs.contains("=>") || rhs.trim_start().starts_with("function") {
                        SymbolKind::Function
                    } else if kw == "const " {
                        SymbolKind::Constant
                    } else {
                        SymbolKind::Variable
                    }
                } else {
                    SymbolKind::Variable
                };
                push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            }
            return;
        }
    }
}

/// 去掉 `export` / `export default` / `export ` 前缀与 `async`
fn strip_ts_prefixes(t: &str) -> &str {
    let mut s = t.trim_start();
    loop {
        let next = if let Some(rest) = s.strip_prefix("export") {
            let rest = rest.trim_start();
            if let Some(r2) = rest.strip_prefix("default") {
                r2.trim_start()
            } else {
                rest
            }
        } else if let Some(rest) = s.strip_prefix("declare ") {
            rest.trim_start()
        } else if let Some(rest) = s.strip_prefix("async ") {
            rest.trim_start()
        } else if let Some(rest) = s.strip_prefix("public ") {
            rest.trim_start()
        } else if let Some(rest) = s.strip_prefix("static ") {
            rest.trim_start()
        } else {
            break;
        };
        s = next;
    }
    s
}

/// C 控制流/杂项关键词，函数定义启发式需排除
const C_NON_FUNC: &[&str] = &[
    "if", "for", "while", "switch", "return", "do", "else", "sizeof", "typedef", "case", "delete",
    "new", "throw", "catch", "defined",
];

fn extract_c_cpp(indent: usize, t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    for (kw, kind) in [
        ("struct", SymbolKind::Struct),
        ("class", SymbolKind::Class),
        ("enum", SymbolKind::Enum),
        ("union", SymbolKind::Struct),
    ] {
        if let Some(name) = kw_name(t, kw) {
            push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            return;
        }
    }
    if let Some(name) = kw_name(t, "namespace") {
        push(
            out,
            SymbolKind::Module,
            name,
            line_no,
            t,
            MAX_SYMBOLS_PER_FILE,
        );
        return;
    }
    // 函数定义启发式：顶格、含 ( ) 、不以分号结尾、首词非控制关键词
    if indent == 0 && !t.ends_with(';') && !t.ends_with(',') {
        if let Some(lp) = t.find('(') {
            if t[lp..].contains(')') {
                let first_word: String = t
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let tail = t[lp + 1..].trim_start();
                if !first_word.is_empty()
                    && !C_NON_FUNC.contains(&first_word.as_str())
                    && !tail.starts_with(';')
                {
                    // 函数名是左括号前最后一个标识符（返回类型可能含 * & 空格）
                    let before = t[..lp].trim_end();
                    let name: String = before
                        .chars()
                        .rev()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '~')
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    if !name.is_empty() && !name.chars().next().unwrap().is_numeric() {
                        push(
                            out,
                            SymbolKind::Function,
                            name,
                            line_no,
                            t,
                            MAX_SYMBOLS_PER_FILE,
                        );
                    }
                }
            }
        }
    }
}

fn extract_go(t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    if let Some(rest) = t.strip_prefix("func ") {
        // func Name( 或 func (recv T) Name(
        let rest = rest.trim_start();
        let name = if rest.starts_with('(') {
            match rest.find(')') {
                Some(rp) => ident_after(&rest[rp + 1..]),
                None => None,
            }
        } else {
            ident_after(rest)
        };
        if let Some(name) = name {
            push(
                out,
                SymbolKind::Function,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    if let Some(rest) = t.strip_prefix("type ") {
        if let Some(name) = ident_after(rest) {
            let kind = if rest.contains("interface") {
                SymbolKind::Interface
            } else {
                SymbolKind::Struct
            };
            push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
        }
        return;
    }
    for (kw, kind) in [
        ("var ", SymbolKind::Variable),
        ("const ", SymbolKind::Constant),
    ] {
        if let Some(rest) = t.strip_prefix(kw) {
            if let Some(name) = ident_after(rest) {
                push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            }
        }
    }
}

fn extract_java(indent: usize, t: &str, line_no: usize, out: &mut Vec<Symbol>) {
    for (kw, kind) in [
        ("class", SymbolKind::Class),
        ("interface", SymbolKind::Interface),
        ("enum", SymbolKind::Enum),
    ] {
        if let Some(name) = kw_name(t, kw) {
            push(out, kind, name, line_no, t, MAX_SYMBOLS_PER_FILE);
            return;
        }
    }
    if t.starts_with("package ") {
        if let Some(name) = kw_name(t, "package") {
            push(
                out,
                SymbolKind::Module,
                name,
                line_no,
                t,
                MAX_SYMBOLS_PER_FILE,
            );
        }
        return;
    }
    // 方法启发式：非顶格也可，含 ( 、不以分号/逗号/花括号结尾、排除控制关键词
    let body = strip_ts_prefixes(t); // public/static/final 等前缀共用剥离逻辑不适用 final，单独处理
    let body = body.strip_prefix("final ").unwrap_or(body);
    if body.contains('(') && !t.ends_with(';') && !t.ends_with(',') && !t.ends_with('{') {
        let first_word: String = body
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '<' || *c == '>')
            .collect();
        if !C_NON_FUNC.contains(&first_word.as_str()) && !first_word.is_empty() {
            let before = body.split('(').next().unwrap_or("").trim_end();
            let name: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let is_kw_like = ["public", "private", "protected", "static", "new", "return"]
                .contains(&name.as_str());
            if !name.is_empty() && !is_kw_like && indent <= 12 {
                push(
                    out,
                    SymbolKind::Function,
                    name,
                    line_no,
                    t,
                    MAX_SYMBOLS_PER_FILE,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(syms: &[Symbol]) -> Vec<String> {
        syms.iter().map(|s| s.name.clone()).collect()
    }

    #[test]
    fn rust_top_level_decls() {
        let src = "use std::fs;\n#[derive(Clone)]\npub struct Foo {\n}\n\npub(crate) fn bar(x: i32) -> i32 {\n    let inner = 1;\n    x + inner\n}\n\ntrait Baz {}\nenum E { A }\nconst MAX: usize = 3;\nstatic NAME: &str = \"a\";\nmod inner;\nimpl Foo {\n    fn method() {}\n}\n";
        let syms = extract_symbols("a.rs", src);
        let n = names(&syms);
        assert!(n.contains(&"Foo".to_string()));
        assert!(n.contains(&"bar".to_string()));
        assert!(n.contains(&"Baz".to_string()));
        assert!(n.contains(&"E".to_string()));
        assert!(n.contains(&"MAX".to_string()));
        assert!(n.contains(&"NAME".to_string()));
        assert!(n.contains(&"inner".to_string()));
        // 函数体局部变量不索引
        assert!(!n.contains(&"x".to_string()));
    }

    #[test]
    fn python_decls() {
        let src = "import os\nCONST = 1\nname = \"x\"\n\nclass Point:\n    def __init__(self):\n        self.v = 0\n\ndef main():\n    local = 2\n";
        let syms = extract_symbols("a.py", src);
        let n = names(&syms);
        assert!(n.contains(&"CONST".to_string()));
        assert!(n.contains(&"Point".to_string()));
        assert!(n.contains(&"__init__".to_string()));
        assert!(n.contains(&"main".to_string()));
        assert!(n.contains(&"name".to_string()));
    }

    #[test]
    fn ts_js_decls() {
        let src = "export function greet(name: string) {\n}\nconst handler = (e) => {\n}\nclass App extends Base {\n}\ninterface Config {\n}\nlet count = 0\nexport default App;\n";
        let syms = extract_symbols("a.ts", src);
        let n = names(&syms);
        assert!(n.contains(&"greet".to_string()));
        assert!(n.contains(&"handler".to_string()));
        assert!(n.contains(&"App".to_string()));
        assert!(n.contains(&"Config".to_string()));
        assert!(n.contains(&"count".to_string()));
    }

    #[test]
    fn c_func_heuristic() {
        let src = "#include <stdio.h>\n\nint add(int a, int b) {\n    return a + b;\n}\n\nvoid foo(void);\n\nstruct Node {\n    int v;\n};\n\nif (x) {\n}\n";
        let syms = extract_symbols("a.c", src);
        let n = names(&syms);
        assert!(n.contains(&"add".to_string()));
        assert!(n.contains(&"Node".to_string()));
        assert!(!n.contains(&"if".to_string()));
        // 原型声明（以分号结尾）不收录为定义
        assert!(!n.contains(&"foo".to_string()));
    }

    #[test]
    fn go_decls() {
        let src = "package main\n\nfunc main() {\n}\n\nfunc (s *Server) Serve(addr string) error {\n}\n\ntype Server struct {\n}\n\ntype Reader interface {\n}\n\nvar Global int\nconst Limit = 10\n";
        let syms = extract_symbols("a.go", src);
        let n = names(&syms);
        assert!(n.contains(&"main".to_string()));
        assert!(n.contains(&"Serve".to_string()));
        assert!(n.contains(&"Server".to_string()));
        assert!(n.contains(&"Reader".to_string()));
        assert!(n.contains(&"Global".to_string()));
        assert!(n.contains(&"Limit".to_string()));
    }

    #[test]
    fn unsupported_ext_ignored() {
        assert!(extract_symbols("a.bin", "fn foo()").is_empty());
    }
}
