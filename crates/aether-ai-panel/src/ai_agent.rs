use std::path::PathBuf;

// ============================================================================
// Agent 工具标记协议（行锚定 + 独特哨兵）
//
// 设计要点：
// 1. 每个标记必须**独占整行**（对该行 `trim_end` 后精确匹配 / 前缀匹配），
//    行内出现同样字样的普通代码不会误触发解析。
// 2. 哨兵带 `AETHER_` 前缀，与 Git 冲突标记（`<<<<<<<` / `=======` / `>>>>>>>`）
//    及常见代码彻底区分，避免文件内容含这些字样时截断解析。
// 3. 不使用"长度前缀"方案：LLM 无法可靠数出字节/字符数，长度前缀反而会因计数
//    错误导致更多截断；行锚定 + 独特哨兵是对模型更友好、解析更确定的方案。
// ============================================================================

/// 文件块起始标记前缀（其后为相对路径）：`<<<<<<< AETHER_FILE <path>`
pub const FILE_HEADER_PREFIX: &str = "<<<<<<< AETHER_FILE";
/// 文件块 search/replace 分隔行：`======= AETHER_SEP`
pub const FILE_SEP: &str = "======= AETHER_SEP";
/// 文件块结束行：`>>>>>>> AETHER_END_FILE`
pub const FILE_FOOTER: &str = ">>>>>>> AETHER_END_FILE";
/// 终端命令块起始行：`<<<<<<< AETHER_RUN`
pub const RUN_HEADER: &str = "<<<<<<< AETHER_RUN";
/// 终端命令块结束行：`>>>>>>> AETHER_END_RUN`
pub const RUN_FOOTER: &str = ">>>>>>> AETHER_END_RUN";
/// 只读工具·读取文件（单行指令，路径为参数）：`<<<<<<< AETHER_READ <path>`
pub const READ_PREFIX: &str = "<<<<<<< AETHER_READ";
/// 只读工具·列出目录（单行指令，路径为参数，可为空/"." 表示工作区根）：`<<<<<<< AETHER_LIST <path>`
pub const LIST_PREFIX: &str = "<<<<<<< AETHER_LIST";
/// 规划器任务清单块起始：`<<<<<<< AETHER_PLAN`
pub const PLAN_HEADER: &str = "<<<<<<< AETHER_PLAN";
/// 规划器任务清单块结束：`>>>>>>> AETHER_END_PLAN`
pub const PLAN_FOOTER: &str = ">>>>>>> AETHER_END_PLAN";
/// 询问卡片块起始：`<<<<<<< AETHER_ASK`（需求不清时向用户提问 + 选项）
pub const ASK_HEADER: &str = "<<<<<<< AETHER_ASK";
/// 询问卡片块结束：`>>>>>>> AETHER_END_ASK`
pub const ASK_FOOTER: &str = ">>>>>>> AETHER_END_ASK";

/// 精准定位标记：`<<<<<<< AETHER_LOCATE <location_type> <params>`
pub const LOCATE_HEADER: &str = "<<<<<<< AETHER_LOCATE";
/// 精准编辑标记：`<<<<<<< AETHER_EDIT <operation_type>`
pub const EDIT_HEADER: &str = "<<<<<<< AETHER_EDIT";
/// 精准编辑结束标记：`>>>>>>> AETHER_END_EDIT`
pub const EDIT_FOOTER: &str = ">>>>>>> AETHER_END_EDIT";

/// 快速判断回复是否包含任一 Agent 工具标记（用于"未打开工作区"等前置校验）。
pub fn has_agent_markers(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_end();
        t.starts_with(FILE_HEADER_PREFIX)
            || t == RUN_HEADER
            || t.starts_with(READ_PREFIX)
            || t.starts_with(LIST_PREFIX)
    })
}

/// 只读探查请求：读取文件 / 列出目录。由编辑器同步执行并将结果回喂给模型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolRequest {
    /// 读取文件内容（参数为工作区相对路径）
    Read(String),
    /// 列出目录条目（参数为工作区相对路径，空串表示根目录）
    List(String),
}

/// 解析单行指令标记，返回其后的参数（前缀后须紧跟空白或行尾）。
fn parse_directive(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest.trim().to_string())
    } else {
        None
    }
}

/// 识别文件头标记行，返回其后的相对路径（新建/整文件替换时可能为空串）。
///
/// 仅当该行以 `FILE_HEADER_PREFIX` 开头、且前缀后紧跟空白或行尾时才识别，
/// 避免 `<<<<<<< AETHER_FILEX` 之类的意外前缀匹配。
fn parse_file_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix(FILE_HEADER_PREFIX)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest.trim().to_string())
    } else {
        None
    }
}

/// AI 建议的单个文件编辑
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiEdit {
    pub path: PathBuf,
    pub search: String,
    pub replace: String,
}

impl AiEdit {
    pub fn new(path: PathBuf, search: String, replace: String) -> Self {
        Self {
            path,
            search,
            replace,
        }
    }

    pub fn is_create_new(&self) -> bool {
        self.search.trim().is_empty()
    }

    pub fn is_delete(&self) -> bool {
        self.replace.trim().is_empty() && !self.search.trim().is_empty()
    }
}

/// 精准定位方式
#[derive(Clone, Debug, PartialEq)]
pub enum PreciseLocation {
    /// 关键词搜索定位：通过文本关键词、函数名、变量名等精确查找
    Keyword {
        keyword: String,
        context_lines: usize,
        /// 大小写不敏感匹配（默认大小写敏感）
        case_insensitive: bool,
        /// 全字匹配：仅命中两侧均非单词字符的出现（默认子串匹配）
        whole_word: bool,
    },
    /// 行号范围定位：指定具体的起始行号和结束行号
    LineRange { start_line: usize, end_line: usize },
    /// 代码片段摘要匹配：通过内容特征匹配定位目标代码
    CodeSnippet {
        snippet: String,
        similarity_threshold: f32,
    },
    /// 函数定义搜索定位：识别 `fn/def/func/function name` 定义并覆盖整个函数体
    Function { name: String },
}

/// 精准编辑操作类型
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOperation {
    /// 替换：替换定位到的代码片段
    Replace { new_content: String },
    /// 插入：在定位到的代码片段前/后插入新内容
    Insert {
        content: String,
        position: InsertPosition,
    },
    /// 删除：删除定位到的代码片段
    Delete,
    /// 重命名：整词替换标识符（函数重命名时同步更新所有调用点，跨文件协同）
    Rename { new_name: String },
}

/// 插入位置
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertPosition {
    Before,
    After,
}

/// 精准编辑：支持多种定位方式和编辑操作
#[derive(Clone, Debug, PartialEq)]
pub struct PreciseEdit {
    pub path: PathBuf,
    pub location: PreciseLocation,
    pub operation: EditOperation,
    /// 上下文感知：读取目标前后若干行作为语境
    pub context_lines: usize,
}

impl PreciseEdit {
    pub fn new(
        path: PathBuf,
        location: PreciseLocation,
        operation: EditOperation,
        context_lines: usize,
    ) -> Self {
        Self {
            path,
            location,
            operation,
            context_lines,
        }
    }
}

/// 从 AI 回复中解析编辑块（行锚定解析）。
///
/// 支持标记：
/// ```text
/// <<<<<<< AETHER_FILE src/main.rs
/// ...old...（创建新文件或整文件替换时留空）
/// ======= AETHER_SEP
/// ...new...（删除文件时留空）
/// >>>>>>> AETHER_END_FILE
/// ```
pub fn parse_edits(response: &str, default_path: Option<&str>) -> Vec<AiEdit> {
    let lines: Vec<&str> = response.lines().collect();
    let mut edits = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(path) = parse_file_header(lines[i].trim_end()) else {
            i += 1;
            continue;
        };
        // 收集 search 段（header 之后至分隔行）
        i += 1;
        let mut search_lines: Vec<&str> = Vec::new();
        let mut found_sep = false;
        while i < lines.len() {
            if lines[i].trim_end() == FILE_SEP {
                found_sep = true;
                i += 1;
                break;
            }
            search_lines.push(lines[i]);
            i += 1;
        }
        if !found_sep {
            break; // 标记不完整，剩余内容不再解析
        }
        // 收集 replace 段（分隔行之后至结束行）
        let mut replace_lines: Vec<&str> = Vec::new();
        let mut found_footer = false;
        while i < lines.len() {
            if lines[i].trim_end() == FILE_FOOTER {
                found_footer = true;
                i += 1;
                break;
            }
            replace_lines.push(lines[i]);
            i += 1;
        }
        if !found_footer {
            break;
        }
        let path_str = if path.is_empty() {
            default_path.unwrap_or("unknown").to_string()
        } else {
            path
        };
        edits.push(AiEdit::new(
            PathBuf::from(path_str.trim()),
            search_lines.join("\n"),
            replace_lines.join("\n"),
        ));
    }
    edits
}

/// 解析精准定位标记
///
/// 支持格式：
/// ```text
/// <<<<<<< AETHER_LOCATE keyword <keyword> [context_lines] [flags]
/// <<<<<<< AETHER_LOCATE lines <start_line> <end_line>
/// <<<<<<< AETHER_LOCATE snippet <snippet> [similarity_threshold]
/// <<<<<<< AETHER_LOCATE fn <name>
/// ```
/// flags 为逗号分隔选项：`ci`（大小写不敏感）、`word`（全字匹配）。
pub fn parse_precise_location(line: &str) -> Option<PreciseLocation> {
    let trimmed = line.trim_end();
    if !trimmed.starts_with(LOCATE_HEADER) {
        return None;
    }

    let rest = trimmed[LOCATE_HEADER.len()..].trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();

    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "keyword" => {
            if parts.len() < 2 {
                return None;
            }
            let keyword = parts[1].to_string();
            let context_lines = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            let flags = parts.get(3).unwrap_or(&"");
            Some(PreciseLocation::Keyword {
                keyword,
                context_lines,
                case_insensitive: flags.contains("ci"),
                whole_word: flags.contains("word"),
            })
        }
        "fn" => {
            if parts.len() < 2 {
                return None;
            }
            Some(PreciseLocation::Function {
                name: parts[1].to_string(),
            })
        }
        "lines" => {
            if parts.len() < 3 {
                return None;
            }
            let start_line = parts[1].parse().ok()?;
            let end_line = parts[2].parse().ok()?;
            Some(PreciseLocation::LineRange {
                start_line,
                end_line,
            })
        }
        "snippet" => {
            if parts.len() < 2 {
                return None;
            }
            let snippet = parts[1].to_string();
            let similarity_threshold = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.8);
            Some(PreciseLocation::CodeSnippet {
                snippet,
                similarity_threshold,
            })
        }
        _ => None,
    }
}

/// 解析精准编辑操作标记
///
/// 支持格式：
/// ```text
/// <<<<<<< AETHER_EDIT replace
/// <<<<<<< AETHER_EDIT insert before
/// <<<<<<< AETHER_EDIT insert after
/// <<<<<<< AETHER_EDIT delete
/// ```
pub fn parse_edit_operation(line: &str) -> Option<EditOperation> {
    let trimmed = line.trim_end();
    if !trimmed.starts_with(EDIT_HEADER) {
        return None;
    }

    let rest = trimmed[EDIT_HEADER.len()..].trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();

    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "replace" => Some(EditOperation::Replace {
            new_content: String::new(),
        }),
        "insert" => {
            if parts.len() < 2 {
                return None;
            }
            match parts[1] {
                "before" => Some(EditOperation::Insert {
                    content: String::new(),
                    position: InsertPosition::Before,
                }),
                "after" => Some(EditOperation::Insert {
                    content: String::new(),
                    position: InsertPosition::After,
                }),
                _ => None,
            }
        }
        "delete" => Some(EditOperation::Delete),
        "rename" => {
            if parts.len() < 2 {
                return None;
            }
            Some(EditOperation::Rename {
                new_name: parts[1].to_string(),
            })
        }
        _ => None,
    }
}

/// 解析精准编辑块
///
/// 支持格式：
/// ```text
/// <<<<<<< AETHER_LOCATE keyword <keyword> [context_lines]
/// <<<<<<< AETHER_EDIT replace
/// ...new content...
/// >>>>>>> AETHER_END_EDIT
/// ```
pub fn parse_precise_edits(response: &str) -> Vec<PreciseEdit> {
    let lines: Vec<&str> = response.lines().collect();
    let mut edits = Vec::new();
    let mut i = 0;
    // 上下文路径：精准编辑默认作用于其前面最近的 AETHER_FILE 块目标文件
    let mut last_file_path: Option<String> = None;

    while i < lines.len() {
        // 记录最近的 FILE 头路径（含未闭合块，作为精准编辑的默认目标）
        if let Some(p) = parse_file_header(lines[i].trim_end()) {
            if !p.is_empty() {
                last_file_path = Some(p);
            }
        }
        // 查找定位标记（支持 ` @ <path>` 后缀显式指定目标文件）
        let trimmed = lines[i].trim_end();
        let (loc_line, path_override) = match trimmed.split_once(" @ ") {
            Some((l, p)) if l.starts_with(LOCATE_HEADER) => (l, Some(p.trim().to_string())),
            _ => (trimmed, None),
        };
        let Some(location) = parse_precise_location(loc_line) else {
            i += 1;
            continue;
        };

        i += 1;
        if i >= lines.len() {
            break;
        }

        // 查找编辑操作标记
        let Some(mut operation) = parse_edit_operation(lines[i]) else {
            i += 1;
            continue;
        };

        i += 1;
        if i >= lines.len() {
            break;
        }

        // 收集编辑内容
        let mut content_lines: Vec<&str> = Vec::new();
        let mut found_footer = false;
        while i < lines.len() {
            if lines[i].trim_end() == EDIT_FOOTER {
                found_footer = true;
                i += 1;
                break;
            }
            content_lines.push(lines[i]);
            i += 1;
        }

        if !found_footer {
            break;
        }

        // 填充编辑内容
        let content = content_lines.join("\n");
        match &mut operation {
            EditOperation::Replace { new_content } => *new_content = content,
            EditOperation::Insert {
                content: insert_content,
                ..
            } => *insert_content = content,
            EditOperation::Delete | EditOperation::Rename { .. } => {}
        }

        // 目标路径优先级：LOCATE 行 `@ path` 显式指定 > 最近的 FILE 头 > unknown
        let path = path_override
            .or_else(|| last_file_path.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let context_lines = 5; // 默认上下文行数

        edits.push(PreciseEdit::new(
            PathBuf::from(path),
            location,
            operation,
            context_lines,
        ));
    }

    edits
}

/// 判断字符是否为「单词字符」（字母/数字/下划线/美元符），用于全字匹配边界判定。
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// 全字标识符替换：仅替换两侧均非单词字符的出现（`foo` 不会误伤 `foobar`），
/// 返回 `(新内容, 替换次数)`。用于函数重命名时同步更新调用点。
pub fn rename_whole_word(content: &str, old: &str, new: &str) -> (String, usize) {
    if old.is_empty() || old == new {
        return (content.to_string(), 0);
    }
    let mut count = 0usize;
    let mut out = String::with_capacity(content.len());
    let mut search_start = 0usize;
    while let Some(rel) = content[search_start..].find(old) {
        let abs = search_start + rel;
        let before_ok = content[..abs]
            .chars()
            .next_back()
            .map_or(true, |c| !is_word_char(c));
        let after_ok = content[abs + old.len()..]
            .chars()
            .next()
            .map_or(true, |c| !is_word_char(c));
        out.push_str(&content[search_start..abs]);
        if before_ok && after_ok {
            out.push_str(new);
            count += 1;
        } else {
            out.push_str(old);
        }
        search_start = abs + old.len();
    }
    out.push_str(&content[search_start..]);
    (out, count)
}

/// 判断一行是否为指定名称的函数定义行（覆盖 Rust/Python/Go/JS/TS 等常见语言）。
fn is_function_def_line(line: &str, name: &str) -> bool {
    let t = line.trim_start();
    for kw in ["fn ", "def ", "func ", "function ", "sub "] {
        let Some(after_kw) = t.strip_prefix(kw) else {
            continue;
        };
        // 允许 `pub fn ` / `async fn ` 等前缀：退而检查行内 `kw + name` 组合
        let rest = if after_kw.starts_with(name) {
            after_kw[name.len()..].to_string()
        } else {
            continue;
        };
        if rest.starts_with('(') || rest.starts_with('<') || rest.trim_start().starts_with('(') {
            return true;
        }
        return false;
    }
    // 行内含 `kw name(` 的变体（如 `pub fn foo(`、`export function foo(`）
    for kw in ["fn ", "def ", "func ", "function ", "sub "] {
        if let Some(idx) = t.find(kw) {
            let after = &t[idx + kw.len()..];
            if let Some(rest) = after.strip_prefix(name) {
                if rest.starts_with('(')
                    || rest.starts_with('<')
                    || rest.trim_start().starts_with('(')
                {
                    return true;
                }
            }
        }
    }
    false
}

/// 函数搜索：在文件内容中查找函数定义并返回其行范围（0-based，含两端）。
///
/// 花括号语言（Rust/C/JS/Go…）用花括号配平确定函数体结尾；
/// Python 用缩进回退确定结尾。未找到定义时返回 None。
pub fn find_function_span(content: &str, name: &str) -> Option<(usize, usize)> {
    if name.is_empty() {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let def = lines.iter().position(|l| is_function_def_line(l, name))?;
    let first = lines[def];
    let indent_of = |l: &str| l.len() - l.trim_start().len();
    if first.trim_start().starts_with("def ") || first.trim_start().starts_with("async def ") {
        // Python：函数体到下一条缩进不深于定义行的非空行为止
        let base = indent_of(first);
        let mut end = def;
        for i in def + 1..lines.len() {
            if lines[i].trim().is_empty() {
                continue;
            }
            if indent_of(lines[i]) <= base {
                break;
            }
            end = i;
        }
        return Some((def, end));
    }
    // 花括号语言：从定义行起配平花括号
    let mut balance = 0i32;
    let mut opened = false;
    for i in def..lines.len() {
        for c in lines[i].chars() {
            match c {
                '{' => {
                    balance += 1;
                    opened = true;
                }
                '}' => balance -= 1,
                _ => {}
            }
        }
        if opened && balance <= 0 {
            return Some((def, i));
        }
    }
    Some((def, def))
}

/// 抢救流式中断时未闭合的尾部 FILE 块（缺 `>>>>>>> AETHER_END_FILE` 结束标记）。
///
/// 仅当尾部块是「新建文件」（`AETHER_SEP` 前的 search 段为空）且已有实际内容时，
/// 返回部分内容对应的编辑；修改/删除类块不抢救——截断的 search/replace
/// 应用到现有文件上有破坏风险。已闭合的块不在抢救范围（由 `parse_edits` 处理）。
pub fn parse_trailing_create_block(response: &str) -> Option<AiEdit> {
    let lines: Vec<&str> = response.lines().collect();
    // 定位最后一个文件头
    let header_idx = (0..lines.len())
        .rev()
        .find(|&i| parse_file_header(lines[i].trim_end()).is_some())?;
    let path = parse_file_header(lines[header_idx].trim_end())?;
    if path.is_empty() {
        return None;
    }
    // 已闭合的块由 parse_edits 处理，不在抢救范围
    if lines[header_idx + 1..]
        .iter()
        .any(|l| l.trim_end() == FILE_FOOTER)
    {
        return None;
    }
    // 必须存在分隔行
    let sep_rel = lines[header_idx + 1..]
        .iter()
        .position(|l| l.trim_end() == FILE_SEP)?;
    let sep_idx = header_idx + 1 + sep_rel;
    // search 段必须为空（新建文件语义）
    if lines[header_idx + 1..sep_idx]
        .iter()
        .any(|l| !l.trim().is_empty())
    {
        return None;
    }
    let replace = lines[sep_idx + 1..].join("\n");
    let replace = replace.trim_end();
    if replace.is_empty() {
        return None;
    }
    Some(AiEdit::new(
        PathBuf::from(path.trim()),
        String::new(),
        replace.to_string(),
    ))
}

/// 从 AI 回复中解析待执行的终端命令（行锚定解析）。
///
/// 支持标记：
/// ```text
/// <<<<<<< AETHER_RUN
/// python src/main.py
/// >>>>>>> AETHER_END_RUN
/// ```
/// 每个 RUN 块内可包含一条或多条命令（按行拆分，空行忽略）。
pub fn parse_run_commands(response: &str) -> Vec<String> {
    let lines: Vec<&str> = response.lines().collect();
    let mut commands = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end() != RUN_HEADER {
            i += 1;
            continue;
        }
        i += 1;
        while i < lines.len() {
            if lines[i].trim_end() == RUN_FOOTER {
                i += 1;
                break;
            }
            let t = lines[i].trim();
            if !t.is_empty() {
                commands.push(t.to_string());
            }
            i += 1;
        }
    }
    commands
}

/// 从 AI 回复中解析只读探查请求（READ / LIST）。
///
/// 会跳过 FILE / RUN 块体，避免块内的文件内容/命令被误读为工具请求；
/// READ/LIST 为单行指令，路径写在标记行同一行。
pub fn parse_tool_requests(response: &str) -> Vec<ToolRequest> {
    let lines: Vec<&str> = response.lines().collect();
    let mut reqs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_end();
        // 跳过完整的 FILE 块体
        if parse_file_header(t).is_some() {
            if let Some((_, _, _, next_i)) = scan_file_block(&lines, i) {
                i = next_i;
                continue;
            }
        }
        // 跳过完整的 RUN 块体
        if t == RUN_HEADER {
            if let Some((_, next_i)) = scan_run_block(&lines, i) {
                i = next_i;
                continue;
            }
        }
        // 读取文件：路径非空才有效
        if let Some(p) = parse_directive(t, READ_PREFIX) {
            if !p.is_empty() {
                reqs.push(ToolRequest::Read(p));
            }
            i += 1;
            continue;
        }
        // 列出目录：空路径表示工作区根
        if let Some(p) = parse_directive(t, LIST_PREFIX) {
            reqs.push(ToolRequest::List(p));
            i += 1;
            continue;
        }
        i += 1;
    }
    reqs
}

/// 规划器任务类型：文件生成（需独立 AI 调用）或命令执行（直接跑）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannedTaskKind {
    File,
    Run,
}

/// 规划器产出的单个任务
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedTask {
    pub kind: PlannedTaskKind,
    /// FILE 任务为相对路径；RUN 任务为要执行的命令
    pub target: String,
    /// 任务职责描述（FILE 用；RUN 通常为空）
    pub description: String,
}

/// 解析规划器输出的 `AETHER_PLAN` 清单块。
///
/// 返回 `(整体目标, 任务列表)`；未找到清单块或无有效任务时返回 `None`，
/// 调用方据此回退到普通单次流程（聊天/单文件）。最多解析 20 个任务。
pub fn parse_plan(response: &str) -> Option<(String, Vec<PlannedTask>)> {
    const MAX_TASKS: usize = 20;
    let lines: Vec<&str> = response.lines().collect();
    let start = lines.iter().position(|l| l.trim_end() == PLAN_HEADER)?;
    let end_rel = lines[start + 1..]
        .iter()
        .position(|l| l.trim_end() == PLAN_FOOTER)?;
    let body = &lines[start + 1..start + 1 + end_rel];
    let mut goal = String::new();
    let mut tasks: Vec<PlannedTask> = Vec::new();
    for line in body {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let mut it = t.splitn(2, char::is_whitespace);
        let kw = it.next().unwrap_or("");
        let rest = it.next().unwrap_or("").trim();
        match kw {
            "GOAL" => goal = rest.to_string(),
            "FILE" => {
                if tasks.len() >= MAX_TASKS || rest.is_empty() {
                    continue;
                }
                let (path, desc) = match rest.split_once(char::is_whitespace) {
                    Some((p, d)) => (p.trim().to_string(), d.trim().to_string()),
                    None => (rest.to_string(), String::new()),
                };
                if !path.is_empty() {
                    tasks.push(PlannedTask {
                        kind: PlannedTaskKind::File,
                        target: path,
                        description: desc,
                    });
                }
            }
            "RUN" => {
                if tasks.len() >= MAX_TASKS || rest.is_empty() {
                    continue;
                }
                tasks.push(PlannedTask {
                    kind: PlannedTaskKind::Run,
                    target: rest.to_string(),
                    description: String::new(),
                });
            }
            _ => {}
        }
    }
    if tasks.is_empty() {
        return None;
    }
    Some((goal, tasks))
}

/// 文件操作类型（用于面板清晰展示 AI 执行了什么）
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOpKind {
    Create,
    Modify,
    Delete,
}

/// 面板展示用的有序块：普通文本 / 文件操作 / 运行命令。
///
/// 目的：把 AI 回复里的标记原文转成清晰的操作提示，让用户直观看到
/// "新建 / 修改 / 删除了哪个文件、运行了什么命令"，而不是一堆标记行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentDisplayBlock {
    Text(String),
    File {
        kind: FileOpKind,
        path: String,
        /// 文件块的 replace 段完整内容（供卡片展开预览）
        content: String,
        /// 文件块的 search 段内容（旧内容片段，供 +/- 行数统计）
        old: String,
    },
    Run {
        cmd: String,
    },
    Read {
        path: String,
    },
    List {
        path: String,
    },
    /// 未闭合的文件块（流式截断）——通知用户该文件未落盘
    Incomplete {
        path: String,
    },
    /// 询问卡片：需求不清时 AI 向用户提问，附 2~4 个选项（用户也可自定义回答）
    Ask {
        question: String,
        options: Vec<String>,
    },
}

/// 把累积的文本行刷入块列表（去除首尾空行，纯空白不产生块）。
fn push_text_block(blocks: &mut Vec<AgentDisplayBlock>, lines: &[&str]) {
    if lines.is_empty() {
        return;
    }
    let joined = lines.join("\n");
    let t = joined.trim_matches('\n');
    if !t.trim().is_empty() {
        blocks.push(AgentDisplayBlock::Text(t.to_string()));
    }
}

/// 从 `start`（FILE 头行）起扫描一个完整文件块，返回 `(操作类型, search段内容, replace段内容, 块结束后的下一行下标)`；
/// 块不完整（缺分隔行或结束行）时返回 None。
fn scan_file_block(lines: &[&str], start: usize) -> Option<(FileOpKind, String, String, usize)> {
    let mut i = start + 1;
    let mut search_empty = true;
    let search_start = i;
    while i < lines.len() && lines[i].trim_end() != FILE_SEP {
        if !lines[i].trim().is_empty() {
            search_empty = false;
        }
        i += 1;
    }
    if i >= lines.len() {
        return None; // 无分隔行
    }
    let search = lines[search_start..i].join("\n");
    i += 1; // 跳过分隔行
    let replace_start = i;
    let mut replace_empty = true;
    while i < lines.len() && lines[i].trim_end() != FILE_FOOTER {
        if !lines[i].trim().is_empty() {
            replace_empty = false;
        }
        i += 1;
    }
    if i >= lines.len() {
        return None; // 无结束行
    }
    let content = lines[replace_start..i].join("\n");
    i += 1; // 跳过结束行
    let kind = if search_empty {
        FileOpKind::Create
    } else if replace_empty {
        FileOpKind::Delete
    } else {
        FileOpKind::Modify
    };
    Some((kind, search, content, i))
}

/// 从 `start`（RUN 头行）起扫描一个完整命令块，返回 `(命令列表, 块结束后的下一行下标)`；
/// 块不完整（缺结束行）时返回 None。
fn scan_run_block(lines: &[&str], start: usize) -> Option<(Vec<String>, usize)> {
    let mut i = start + 1;
    let mut cmds = Vec::new();
    while i < lines.len() && lines[i].trim_end() != RUN_FOOTER {
        let t = lines[i].trim();
        if !t.is_empty() {
            cmds.push(t.to_string());
        }
        i += 1;
    }
    if i >= lines.len() {
        return None; // 无结束行
    }
    i += 1; // 跳过结束行
    Some((cmds, i))
}

/// 从 `start`（ASK 头行）起扫描一个完整询问块，返回 `(问题, 选项列表, 块结束后的下一行下标)`；
/// 块不完整（缺结束行）或问题/选项为空时返回 None。
///
/// 格式：问题行在前（1~多行），选项行以 `- ` 或 `* ` 开头；选项行之后的非选项行忽略。
fn scan_ask_block(lines: &[&str], start: usize) -> Option<(String, Vec<String>, usize)> {
    let mut i = start + 1;
    let mut question_lines: Vec<&str> = Vec::new();
    let mut options: Vec<String> = Vec::new();
    while i < lines.len() && lines[i].trim_end() != ASK_FOOTER {
        let t = lines[i].trim();
        if let Some(opt) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let opt = opt.trim();
            if !opt.is_empty() {
                options.push(opt.to_string());
            }
        } else if options.is_empty() && !t.is_empty() {
            question_lines.push(lines[i]);
        }
        i += 1;
    }
    if i >= lines.len() {
        return None; // 无结束行
    }
    i += 1; // 跳过结束行
    let question = question_lines.join("\n").trim().to_string();
    if question.is_empty() || options.is_empty() {
        return None;
    }
    Some((question, options, i))
}

/// 判断是否为独占一行的协议标记行（<<<<<<< / >>>>>>> / ======= + AETHER_ 前缀）。
/// 这类行若未被识别为合法块，属于内部协议泄漏，不应展示给用户。
fn is_protocol_marker_line(t: &str) -> bool {
    t.starts_with("<<<<<<< AETHER_")
        || t.starts_with(">>>>>>> AETHER_")
        || t.starts_with("======= AETHER_")
}

/// 将 AI 回复按出现顺序解析为"文本 + 操作"块，隐藏原始标记，供面板渲染操作卡片。
///
/// 解析失败/标记不完整时，剩余内容作为普通文本返回（不丢内容）。
pub fn parse_display_blocks(response: &str) -> Vec<AgentDisplayBlock> {
    let lines: Vec<&str> = response.lines().collect();
    let mut blocks: Vec<AgentDisplayBlock> = Vec::new();
    let mut text_buf: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_end();
        // 文件块
        if let Some(path) = parse_file_header(trimmed) {
            if let Some((kind, search, content, next_i)) = scan_file_block(&lines, i) {
                push_text_block(&mut blocks, &text_buf);
                text_buf.clear();
                blocks.push(AgentDisplayBlock::File {
                    kind,
                    path: path.trim().to_string(),
                    content,
                    old: search,
                });
                i = next_i;
                continue;
            }
        }
        // 命令块
        if trimmed == RUN_HEADER {
            if let Some((cmds, next_i)) = scan_run_block(&lines, i) {
                push_text_block(&mut blocks, &text_buf);
                text_buf.clear();
                for c in cmds {
                    blocks.push(AgentDisplayBlock::Run { cmd: c });
                }
                i = next_i;
                continue;
            }
        }
        // 只读探查（单行指令）
        if let Some(p) = parse_directive(trimmed, READ_PREFIX) {
            if !p.is_empty() {
                push_text_block(&mut blocks, &text_buf);
                text_buf.clear();
                blocks.push(AgentDisplayBlock::Read { path: p });
                i += 1;
                continue;
            }
        }
        if let Some(p) = parse_directive(trimmed, LIST_PREFIX) {
            push_text_block(&mut blocks, &text_buf);
            text_buf.clear();
            blocks.push(AgentDisplayBlock::List {
                path: if p.is_empty() { ".".to_string() } else { p },
            });
            i += 1;
            continue;
        }
        // 询问卡片块（需求不清时向用户提问 + 选项）
        if trimmed == ASK_HEADER {
            if let Some((question, options, next_i)) = scan_ask_block(&lines, i) {
                push_text_block(&mut blocks, &text_buf);
                text_buf.clear();
                blocks.push(AgentDisplayBlock::Ask { question, options });
                i = next_i;
                continue;
            }
        }
        // 精准编辑块（含内容行）：内部协议，整体不展示
        //（执行结果由编辑器状态消息「✓ 已精准修改」呈现）。
        if trimmed.starts_with(EDIT_HEADER) {
            i += 1;
            while i < lines.len() && lines[i].trim_end() != EDIT_FOOTER {
                i += 1;
            }
            if i < lines.len() {
                i += 1; // 跳过结束行；未闭合则已到末尾
            }
            continue;
        }
        // 规划块（含内容行）：内部协议，整体不展示
        if trimmed.starts_with(PLAN_HEADER) {
            i += 1;
            while i < lines.len() && lines[i].trim_end() != PLAN_FOOTER {
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            continue;
        }
        // 精准定位单行指令：内部协议，不展示
        if trimmed.starts_with(LOCATE_HEADER) {
            i += 1;
            continue;
        }
        // 泄漏的协议标记行（如模型违规输出的 >>>>>>> AETHER_END_LIST）不进入展示文本；
        // FILE 头保留，供尾部截断 Incomplete 检测使用。
        if is_protocol_marker_line(trimmed) && parse_file_header(trimmed).is_none() {
            i += 1;
            continue;
        }
        text_buf.push(lines[i]);
        i += 1;
    }
    // 尾部截断检测：若 text_buf 包含未闭合 FILE 头，转为 Incomplete 提示（P1）
    let incomplete_pos = text_buf
        .iter()
        .rposition(|l| parse_file_header(l.trim_end()).is_some());
    if let Some(pos) = incomplete_pos {
        push_text_block(&mut blocks, &text_buf[..pos]);
        let path = parse_file_header(text_buf[pos].trim_end()).unwrap_or_default();
        if !path.is_empty() {
            blocks.push(AgentDisplayBlock::Incomplete { path });
        } else {
            push_text_block(&mut blocks, &text_buf[pos..]);
        }
    } else {
        push_text_block(&mut blocks, &text_buf);
    }
    // 纯探查回合（仅 READ/LIST 卡片，无文件/命令操作）：开场白属于内部叙述，
    // 整体过滤，只向用户展示工具卡片。
    let probe_only = blocks.iter().any(|b| {
        matches!(
            b,
            AgentDisplayBlock::Read { .. } | AgentDisplayBlock::List { .. }
        )
    }) && !blocks.iter().any(|b| {
        matches!(
            b,
            AgentDisplayBlock::File { .. }
                | AgentDisplayBlock::Run { .. }
                | AgentDisplayBlock::Incomplete { .. }
        )
    });
    if probe_only {
        blocks.retain(|b| !matches!(b, AgentDisplayBlock::Text(_)));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_edit() {
        let text = "下面修改 main.rs：\n\
<<<<<<< AETHER_FILE src/main.rs\n\
fn old() {}\n\
======= AETHER_SEP\n\
fn new() {}\n\
>>>>>>> AETHER_END_FILE\n";
        let edits = parse_edits(text, None);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].path, PathBuf::from("src/main.rs"));
        assert!(edits[0].search.contains("fn old()"));
        assert!(edits[0].replace.contains("fn new()"));
    }

    #[test]
    fn test_parse_create_new_file() {
        let text = "<<<<<<< AETHER_FILE src/lib.rs\n\
======= AETHER_SEP\n\
pub fn hello() {}\n\
>>>>>>> AETHER_END_FILE\n";
        let edits = parse_edits(text, None);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].is_create_new());
    }

    #[test]
    fn test_parse_delete_file() {
        let text = "<<<<<<< AETHER_FILE old.txt\n\
some content\n\
======= AETHER_SEP\n\
>>>>>>> AETHER_END_FILE\n";
        let edits = parse_edits(text, None);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].is_delete());
    }

    #[test]
    fn test_parse_no_markers() {
        let edits = parse_edits("普通回答，没有编辑", None);
        assert!(edits.is_empty());
    }

    #[test]
    fn test_parse_multiple_edits() {
        let text = "<<<<<<< AETHER_FILE a.rs\n\
======= AETHER_SEP\n\
a\n\
>>>>>>> AETHER_END_FILE\n\
中间说明\n\
<<<<<<< AETHER_FILE b.rs\n\
======= AETHER_SEP\n\
b\n\
>>>>>>> AETHER_END_FILE\n";
        let edits = parse_edits(text, None);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].path, PathBuf::from("a.rs"));
        assert_eq!(edits[1].path, PathBuf::from("b.rs"));
    }

    #[test]
    fn test_content_with_git_conflict_markers_not_truncated() {
        // 关键鲁棒性：文件内容含 Git 冲突标记（裸 ======= / >>>>>>>）不应截断解析
        let text = "<<<<<<< AETHER_FILE conflict.txt\n\
======= AETHER_SEP\n\
<<<<<<< HEAD\n\
line from head\n\
=======\n\
line from branch\n\
>>>>>>> feature\n\
>>>>>>> AETHER_END_FILE\n";
        let edits = parse_edits(text, None);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].is_create_new());
        // 冲突标记原样保留在新内容里
        assert!(edits[0].replace.contains("<<<<<<< HEAD"));
        assert!(edits[0].replace.contains("=======\n") || edits[0].replace.contains("======="));
        assert!(edits[0].replace.contains(">>>>>>> feature"));
    }

    #[test]
    fn test_inline_marker_text_not_triggered() {
        // 标记字样出现在行内（非独占整行）不应触发解析
        let text = "这行提到 <<<<<<< AETHER_FILE 只是说明，不是真标记";
        let edits = parse_edits(text, None);
        assert!(edits.is_empty());
    }

    #[test]
    fn test_parse_run_commands_single() {
        let text = "我将运行脚本：\n\
<<<<<<< AETHER_RUN\n\
python src/main.py\n\
>>>>>>> AETHER_END_RUN\n";
        let cmds = parse_run_commands(text);
        assert_eq!(cmds, vec!["python src/main.py".to_string()]);
    }

    #[test]
    fn test_parse_run_commands_multi() {
        let text = "<<<<<<< AETHER_RUN\n\
cargo build\n\
cargo test\n\
>>>>>>> AETHER_END_RUN";
        let cmds = parse_run_commands(text);
        assert_eq!(
            cmds,
            vec!["cargo build".to_string(), "cargo test".to_string()]
        );
    }

    #[test]
    fn test_parse_run_commands_none() {
        let cmds = parse_run_commands("没有命令");
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_trailing_create_block_unclosed() {
        // 流式中断：新建网页文件的尾部块未闭合 → 抢救部分内容
        let text = "好的，我来创建网页：\n\
<<<<<<< AETHER_FILE index.html\n\
======= AETHER_SEP\n\
<!DOCTYPE html>\n\
<html>\n\
<head><title>Demo</title></head>\n\
<body>\n\
  <h1>Hello";
        let edit = parse_trailing_create_block(text).expect("应抢救未闭合的新建块");
        assert_eq!(edit.path, PathBuf::from("index.html"));
        assert!(edit.is_create_new());
        assert!(edit.replace.contains("<!DOCTYPE html>"));
        assert!(edit.replace.ends_with("<h1>Hello"));
    }

    #[test]
    fn test_trailing_create_block_modify_not_salvaged() {
        // 修改类块（search 非空）未闭合 → 不抢救，避免截断内容破坏现有文件
        let text = "<<<<<<< AETHER_FILE src/main.rs\n\
fn old() {}\n\
======= AETHER_SEP\n\
fn new() {}\n\
fn extra()";
        assert!(parse_trailing_create_block(text).is_none());
    }

    #[test]
    fn test_trailing_create_block_closed_not_salvaged() {
        // 已闭合的块由 parse_edits 处理，不在抢救范围
        let text = "<<<<<<< AETHER_FILE a.txt\n\
======= AETHER_SEP\n\
content\n\
>>>>>>> AETHER_END_FILE";
        assert!(parse_trailing_create_block(text).is_none());
    }

    #[test]
    fn test_trailing_create_block_no_marker() {
        assert!(parse_trailing_create_block("普通回答，没有编辑").is_none());
    }

    #[test]
    fn test_trailing_create_block_empty_content() {
        // 头部完整但还没有实际内容 → 不抢救
        let text = "<<<<<<< AETHER_FILE a.txt\n======= AETHER_SEP\n";
        assert!(parse_trailing_create_block(text).is_none());
    }

    #[test]
    fn test_display_blocks_mixed() {
        let text = "先说明\n\
<<<<<<< AETHER_FILE a.rs\n\
======= AETHER_SEP\n\
code\n\
>>>>>>> AETHER_END_FILE\n\
再运行\n\
<<<<<<< AETHER_RUN\n\
cargo test\n\
>>>>>>> AETHER_END_RUN\n\
收尾";
        let blocks = parse_display_blocks(text);
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0], AgentDisplayBlock::Text("先说明".to_string()));
        assert!(matches!(
            blocks[1],
            AgentDisplayBlock::File {
                kind: FileOpKind::Create,
                ..
            }
        ));
        assert_eq!(blocks[2], AgentDisplayBlock::Text("再运行".to_string()));
        assert_eq!(
            blocks[3],
            AgentDisplayBlock::Run {
                cmd: "cargo test".to_string()
            }
        );
        assert_eq!(blocks[4], AgentDisplayBlock::Text("收尾".to_string()));
    }

    #[test]
    fn test_display_blocks_incomplete_produces_warning() {
        // 标记不完整（缺结束行）→ 产生 Incomplete 警告卡片（P1），不再作为裸文本显示
        let text = "<<<<<<< AETHER_FILE a.rs\n======= AETHER_SEP\ncode without end";
        let blocks = parse_display_blocks(text);
        assert!(blocks
            .iter()
            .any(|b| matches!(b, AgentDisplayBlock::Incomplete { path } if path == "a.rs")));
        assert!(!blocks
            .iter()
            .any(|b| matches!(b, AgentDisplayBlock::File { .. })));
    }

    #[test]
    fn test_display_blocks_file_content_captured() {
        // 闭合 FILE 块的 replace 段内容应被完整捕获（P0 可展开预览基础）
        let text =
            "<<<<<<< AETHER_FILE a.rs\n======= AETHER_SEP\nfn main() {}\n>>>>>>> AETHER_END_FILE";
        let blocks = parse_display_blocks(text);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AgentDisplayBlock::File {
                kind,
                path,
                content,
                ..
            } => {
                assert_eq!(*kind, FileOpKind::Create);
                assert_eq!(path, "a.rs");
                assert_eq!(content, "fn main() {}");
            }
            _ => panic!("expected File block"),
        }
    }

    #[test]
    fn test_has_agent_markers() {
        assert!(has_agent_markers("<<<<<<< AETHER_FILE a.rs\n"));
        assert!(has_agent_markers("前言\n<<<<<<< AETHER_RUN\n"));
        assert!(has_agent_markers("<<<<<<< AETHER_READ src/main.rs\n"));
        assert!(has_agent_markers("<<<<<<< AETHER_LIST src\n"));
        assert!(!has_agent_markers("普通文本，行内提到 AETHER_FILE 字样"));
    }

    #[test]
    fn test_parse_tool_requests_read_and_list() {
        let text = "我先看看：\n\
<<<<<<< AETHER_READ src/main.rs\n\
<<<<<<< AETHER_LIST src\n\
<<<<<<< AETHER_LIST\n";
        let reqs = parse_tool_requests(text);
        assert_eq!(
            reqs,
            vec![
                ToolRequest::Read("src/main.rs".to_string()),
                ToolRequest::List("src".to_string()),
                ToolRequest::List("".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_tool_requests_read_empty_path_ignored() {
        // READ 无路径无效（列目录允许空=根，读文件必须有路径）
        let reqs = parse_tool_requests("<<<<<<< AETHER_READ\n");
        assert!(reqs.is_empty());
    }

    #[test]
    fn test_parse_tool_requests_skips_file_block_body() {
        // FILE 块体内出现 READ/LIST 字样不应被误读为工具请求
        let text = "<<<<<<< AETHER_FILE note.txt\n\
======= AETHER_SEP\n\
<<<<<<< AETHER_READ inside-content\n\
<<<<<<< AETHER_LIST inside-content\n\
>>>>>>> AETHER_END_FILE\n\
<<<<<<< AETHER_READ real.rs\n";
        let reqs = parse_tool_requests(text);
        assert_eq!(reqs, vec![ToolRequest::Read("real.rs".to_string())]);
    }

    #[test]
    fn test_display_blocks_read_list() {
        let text = "看下代码\n\
<<<<<<< AETHER_READ a.rs\n\
<<<<<<< AETHER_LIST\n";
        let blocks = parse_display_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[0],
            AgentDisplayBlock::Read {
                path: "a.rs".to_string()
            }
        );
        assert_eq!(
            blocks[1],
            AgentDisplayBlock::List {
                path: ".".to_string()
            }
        );
    }

    #[test]
    fn test_display_blocks_probe_hides_narration_and_leaked_marker() {
        // 纯探查回合：模型违规输出的结束标记与开场白均不展示，只剩工具卡片
        let text = [
            "我先看看当前工作区里有哪些文件。",
            LIST_PREFIX,
            ">>>>>>> AETHER_END_LIST",
        ]
        .join("\n");
        let blocks = parse_display_blocks(&text);
        assert_eq!(
            blocks,
            vec![AgentDisplayBlock::List {
                path: ".".to_string()
            }]
        );
    }

    #[test]
    fn test_display_blocks_hides_locate_edit_and_plan_blocks() {
        // 精准定位/精准编辑/规划块均为内部协议，内容行不展示，仅保留普通文本
        let text = [
            "开始修改。".to_string(),
            format!("{} keyword fn_main", LOCATE_HEADER),
            format!("{} replace", EDIT_HEADER),
            "fn main() {}".to_string(),
            EDIT_FOOTER.to_string(),
            PLAN_HEADER.to_string(),
            "TASK a——b".to_string(),
            PLAN_FOOTER.to_string(),
            "完成。".to_string(),
        ]
        .join("\n");
        let blocks = parse_display_blocks(&text);
        assert_eq!(blocks.len(), 1);
        // 隐藏块不割裂文本：前后叙述合并为一段
        assert_eq!(
            blocks[0],
            AgentDisplayBlock::Text("开始修改。\n完成。".to_string())
        );
    }

    #[test]
    fn test_display_blocks_mixed_turn_keeps_text_strips_leaked_marker() {
        // 含文件操作的回合：文本保留，但泄漏标记行仍被过滤
        let text = [
            "下面修改文件：",
            ">>>>>>> AETHER_END_LIST",
            FILE_HEADER_PREFIX,
            FILE_SEP,
            "内容",
            FILE_FOOTER,
        ]
        .join("\n");
        let blocks = parse_display_blocks(&text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[0],
            AgentDisplayBlock::Text("下面修改文件：".to_string())
        );
        assert!(matches!(blocks[1], AgentDisplayBlock::File { .. }));
    }

    #[test]
    fn test_parse_plan_basic() {
        let text = "好的，我来规划：\n\
<<<<<<< AETHER_PLAN\n\
GOAL 做一个产品卡片单页\n\
FILE index.html 首页结构与卡片区\n\
FILE styles.css 全站样式与响应式\n\
RUN python -m http.server 8000\n\
>>>>>>> AETHER_END_PLAN\n";
        let (goal, tasks) = parse_plan(text).expect("应解析出计划");
        assert_eq!(goal, "做一个产品卡片单页");
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].kind, PlannedTaskKind::File);
        assert_eq!(tasks[0].target, "index.html");
        assert_eq!(tasks[0].description, "首页结构与卡片区");
        assert_eq!(tasks[1].target, "styles.css");
        assert_eq!(tasks[2].kind, PlannedTaskKind::Run);
        assert_eq!(tasks[2].target, "python -m http.server 8000");
    }

    #[test]
    fn test_parse_plan_no_block_returns_none() {
        // 无计划块（普通回答/单文件）→ None，回退到普通流程
        assert!(parse_plan("普通回答，没有计划").is_none());
        assert!(parse_plan("<<<<<<< AETHER_FILE a.rs\ncontent").is_none());
    }

    #[test]
    fn test_parse_plan_empty_block_returns_none() {
        // 有块但无有效任务 → None
        let text = "<<<<<<< AETHER_PLAN\nGOAL 只有目标没有任务\n>>>>>>> AETHER_END_PLAN";
        assert!(parse_plan(text).is_none());
    }

    #[test]
    fn test_parse_plan_file_without_description() {
        let text = "<<<<<<< AETHER_PLAN\nFILE main.rs\n>>>>>>> AETHER_END_PLAN";
        let (_, tasks) = parse_plan(text).expect("应解析");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].target, "main.rs");
        assert_eq!(tasks[0].description, "");
    }

    #[test]
    fn test_parse_plan_caps_at_20_tasks() {
        let mut s = String::from("<<<<<<< AETHER_PLAN\n");
        for i in 0..30 {
            s.push_str(&format!("FILE f{}.txt 第{}个\n", i, i));
        }
        s.push_str(">>>>>>> AETHER_END_PLAN\n");
        let (_, tasks) = parse_plan(&s).expect("应解析");
        assert_eq!(tasks.len(), 20);
    }

    #[test]
    fn test_display_blocks_ask_card() {
        // 完整询问块：解析为 Ask 卡片（问题 + 选项），前后文本保留
        let text = [
            "先确认一下需求：".to_string(),
            ASK_HEADER.to_string(),
            "你希望主题用哪种风格？".to_string(),
            "- 深色".to_string(),
            "* 浅色".to_string(),
            "- 跟随系统".to_string(),
            ASK_FOOTER.to_string(),
            "选好后我立刻开工。".to_string(),
        ]
        .join("\n");
        let blocks = parse_display_blocks(&text);
        assert_eq!(blocks.len(), 3);
        assert_eq!(
            blocks[0],
            AgentDisplayBlock::Text("先确认一下需求：".to_string())
        );
        match &blocks[1] {
            AgentDisplayBlock::Ask { question, options } => {
                assert_eq!(question, "你希望主题用哪种风格？");
                assert_eq!(
                    options,
                    &vec![
                        "深色".to_string(),
                        "浅色".to_string(),
                        "跟随系统".to_string()
                    ]
                );
            }
            other => panic!("应为 Ask 块，实际 {:?}", other),
        }
        assert_eq!(
            blocks[2],
            AgentDisplayBlock::Text("选好后我立刻开工。".to_string())
        );
    }

    #[test]
    fn test_display_blocks_ask_multiple_keeps_order() {
        // 引导式顺序提问：同一回复多个询问块按序解析，供 UI 逐个展示
        let text = [
            ASK_HEADER.to_string(),
            "页面用途是什么？".to_string(),
            "- 个人博客".to_string(),
            "- 产品展示".to_string(),
            ASK_FOOTER.to_string(),
            ASK_HEADER.to_string(),
            "需要哪种配色？".to_string(),
            "- 深色".to_string(),
            "- 浅色".to_string(),
            ASK_FOOTER.to_string(),
        ]
        .join("\n");
        let blocks = parse_display_blocks(&text);
        assert_eq!(blocks.len(), 2);
        match (&blocks[0], &blocks[1]) {
            (
                AgentDisplayBlock::Ask { question: q1, .. },
                AgentDisplayBlock::Ask { question: q2, .. },
            ) => {
                assert_eq!(q1, "页面用途是什么？");
                assert_eq!(q2, "需要哪种配色？");
            }
            other => panic!("应为两个 Ask 块，实际 {:?}", other),
        }
    }

    #[test]
    fn test_display_blocks_ask_incomplete_yields_no_card() {
        // 未闭合（流式中间态）：不产生 Ask 卡片，头行被协议标记规则过滤
        let text = "<<<<<<< AETHER_ASK\n你希望哪种风格？\n- 深色";
        let blocks = parse_display_blocks(text);
        assert!(!blocks
            .iter()
            .any(|b| matches!(b, AgentDisplayBlock::Ask { .. })));
    }

    #[test]
    fn test_display_blocks_ask_missing_question_or_options_rejected() {
        // 缺问题或缺选项的块不产生卡片（整体被协议行过滤规则隐藏）
        let no_question = "<<<<<<< AETHER_ASK\n- 只有选项\n>>>>>>> AETHER_END_ASK";
        let blocks = parse_display_blocks(no_question);
        assert!(!blocks
            .iter()
            .any(|b| matches!(b, AgentDisplayBlock::Ask { .. })));

        let no_options = "<<<<<<< AETHER_ASK\n只有问题\n>>>>>>> AETHER_END_ASK";
        let blocks = parse_display_blocks(no_options);
        assert!(!blocks
            .iter()
            .any(|b| matches!(b, AgentDisplayBlock::Ask { .. })));
    }

    #[test]
    fn test_locate_fn_and_keyword_flags() {
        let loc = parse_precise_location("<<<<<<< AETHER_LOCATE fn compute");
        assert_eq!(
            loc,
            Some(PreciseLocation::Function {
                name: "compute".to_string()
            })
        );
        let loc = parse_precise_location("<<<<<<< AETHER_LOCATE keyword foo 3 ci,word");
        match loc {
            Some(PreciseLocation::Keyword {
                keyword,
                context_lines,
                case_insensitive,
                whole_word,
            }) => {
                assert_eq!(keyword, "foo");
                assert_eq!(context_lines, 3);
                assert!(case_insensitive);
                assert!(whole_word);
            }
            other => panic!("应为 Keyword，实际 {:?}", other),
        }
        // 默认大小写敏感、子串匹配
        let loc = parse_precise_location("<<<<<<< AETHER_LOCATE keyword bar");
        match loc {
            Some(PreciseLocation::Keyword {
                case_insensitive,
                whole_word,
                ..
            }) => {
                assert!(!case_insensitive);
                assert!(!whole_word);
            }
            other => panic!("应为 Keyword，实际 {:?}", other),
        }
    }

    #[test]
    fn test_edit_rename_parsed() {
        let op = parse_edit_operation("<<<<<<< AETHER_EDIT rename new_name");
        assert_eq!(
            op,
            Some(EditOperation::Rename {
                new_name: "new_name".to_string()
            })
        );
        assert_eq!(parse_edit_operation("<<<<<<< AETHER_EDIT rename"), None);
    }

    #[test]
    fn test_precise_edit_path_from_file_header_and_override() {
        let text = "<<<<<<< AETHER_FILE src/a.rs\n======= AETHER_SEP\nx\n>>>>>>> AETHER_END_FILE\n\
<<<<<<< AETHER_LOCATE fn foo\n\
<<<<<<< AETHER_EDIT delete\n\
>>>>>>> AETHER_END_EDIT\n\
<<<<<<< AETHER_LOCATE fn bar @ src/b.rs\n\
<<<<<<< AETHER_EDIT delete\n\
>>>>>>> AETHER_END_EDIT\n";
        let edits = parse_precise_edits(text);
        assert_eq!(edits.len(), 2);
        // 第一个继承最近的 FILE 头路径
        assert_eq!(edits[0].path, PathBuf::from("src/a.rs"));
        // 第二个被 @ 显式覆盖
        assert_eq!(edits[1].path, PathBuf::from("src/b.rs"));
        assert_eq!(
            edits[1].location,
            PreciseLocation::Function {
                name: "bar".to_string()
            }
        );
    }

    #[test]
    fn test_rename_whole_word_boundaries() {
        let src = "foo(1); foobar(2); x.foo; let foo = foo;";
        let (out, n) = rename_whole_word(src, "foo", "bar");
        assert_eq!(n, 4);
        assert_eq!(out, "bar(1); foobar(2); x.bar; let bar = bar;");
        // 同名替换不计次
        let (_, n) = rename_whole_word(src, "foo", "foo");
        assert_eq!(n, 0);
    }

    #[test]
    fn test_find_function_span_rust_braces() {
        let src = "fn a() {}\n\nfn target(x: i32) -> i32 {\n    if x > 0 {\n        return 1;\n    }\n    2\n}\n\nfn b() {}";
        let span = find_function_span(src, "target");
        assert_eq!(span, Some((2, 7)));
        // 不会误命中调用点或前缀同名函数
        assert!(find_function_span(src, "targe").is_none());
    }

    #[test]
    fn test_find_function_span_python_indent() {
        let src = "def a():\n    pass\n\ndef target(x):\n    if x:\n        return 1\n    return 2\n\ndef b():\n    pass";
        let span = find_function_span(src, "target");
        assert_eq!(span, Some((3, 6)));
    }
}
