//! 工作区符号索引：一次遍历构建，内存仅存声明摘要（KB~MB 级），
//! 支持按名称精确/前缀/子串检索，并可回读符号的完整源码。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::extract::{extract_symbols, Lang, Symbol, SymbolKind};
use crate::source::symbol_source;

/// 遍历时跳过的目录名（构建产物/依赖缓存，符号噪音大且量大）
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    ".gradle",
    ".idea",
    ".vscode",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
    ".cargo",
    ".cargo-home",
    "bower_components",
    ".next",
    ".nuxt",
];

/// 单文件大小上限（生成代码/超大文件跳过）
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// 索引符号总量上限
const MAX_TOTAL_SYMBOLS: usize = 60_000;
/// 遍历文件数上限
const MAX_FILES_WALKED: usize = 20_000;

/// 工作区符号索引
#[derive(Default)]
pub struct SymbolIndex {
    root: PathBuf,
    symbols: Vec<Symbol>,
    /// 小写名称 → symbols 下标列表
    by_lower: HashMap<String, Vec<u32>>,
}

impl SymbolIndex {
    /// 遍历工作区构建索引（同步、单遍，无后台进程）
    pub fn build(root: &Path) -> Self {
        let mut idx = SymbolIndex {
            root: root.to_path_buf(),
            ..Default::default()
        };
        let mut files_walked = 0usize;
        idx.walk(root, &mut files_walked);
        for (i, sym) in idx.symbols.iter().enumerate() {
            idx.by_lower
                .entry(sym.name.to_lowercase())
                .or_default()
                .push(i as u32);
        }
        idx
    }

    fn walk(&mut self, dir: &Path, files_walked: &mut usize) {
        if self.symbols.len() >= MAX_TOTAL_SYMBOLS || *files_walked >= MAX_FILES_WALKED {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        // 目录内条目排序，保证跨平台构建结果稳定
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                dirs.push(path);
                continue;
            }
            if !file_type.is_file() || Lang::from_path(&name).is_none() {
                continue;
            }
            *files_walked += 1;
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = relative_path(&self.root, &path);
            let mut syms = extract_symbols(&rel, &text);
            if syms.is_empty() {
                continue;
            }
            for s in syms.iter_mut() {
                s.path = rel.clone();
            }
            self.symbols.append(&mut syms);
            if self.symbols.len() >= MAX_TOTAL_SYMBOLS {
                return;
            }
        }
        for d in dirs {
            self.walk(&d, files_walked);
            if self.symbols.len() >= MAX_TOTAL_SYMBOLS || *files_walked >= MAX_FILES_WALKED {
                return;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// 按名称检索符号：精确 > 前缀 > 子串，大小写不敏感；
    /// 函数/类等重要种类优先展示。
    pub fn find(&self, query: &str, limit: usize) -> Vec<&Symbol> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut exact: Vec<&Symbol> = Vec::new();
        let mut prefix: Vec<&Symbol> = Vec::new();
        let mut substr: Vec<&Symbol> = Vec::new();
        for (name, ids) in &self.by_lower {
            let bucket = if name == &q {
                &mut exact
            } else if name.starts_with(&q) {
                &mut prefix
            } else if name.contains(&q) {
                &mut substr
            } else {
                continue;
            };
            for &id in ids {
                bucket.push(&self.symbols[id as usize]);
            }
        }
        let rank = |s: &&Symbol| match s.kind {
            SymbolKind::Function => 0u8,
            SymbolKind::Struct | SymbolKind::Class | SymbolKind::Enum | SymbolKind::Trait => 1,
            SymbolKind::Interface | SymbolKind::TypeAlias | SymbolKind::Impl => 2,
            SymbolKind::Constant | SymbolKind::Static => 3,
            SymbolKind::Variable | SymbolKind::Module => 4,
        };
        for v in [&mut exact, &mut prefix, &mut substr] {
            v.sort_by_key(|s| (rank(s), s.path.clone(), s.line));
        }
        let mut out = exact;
        out.extend(prefix);
        out.extend(substr);
        out.truncate(limit);
        out
    }

    /// 读取符号的完整源码块（整函数/整类）；失败时返回 None（调用方回退到声明行）
    pub fn source_of(&self, sym: &Symbol) -> Option<String> {
        let path = self.root.join(&sym.path);
        let text = std::fs::read_to_string(&path).ok()?;
        let lang = Lang::from_path(&sym.path)?;
        symbol_source(&text, sym.line, lang)
    }
}

/// 规范化相对路径（`/` 分隔，小写盘符无关紧要——保持原样）
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn build_and_find_in_temp_workspace() {
        let dir = std::env::temp_dir().join(format!("aether_sym_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "src/main.rs",
            "fn process_data(input: &str) -> String {\n    input.to_uppercase()\n}\n\nstruct App;\n",
        );
        write(
            &dir,
            "src/util.rs",
            "pub fn process_all() {}\nconst LIMIT: usize = 1;\n",
        );
        write(&dir, "target/ignore.rs", "fn should_not_index() {}");
        write(&dir, "py/helper.py", "def Process_thing():\n    pass\n");

        let idx = SymbolIndex::build(&dir);
        assert!(idx.len() >= 4);

        let hits = idx.find("process_data", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "src/main.rs");
        assert_eq!(hits[0].kind, SymbolKind::Function);

        // 子串 + 大小写不敏感
        let hits = idx.find("PROCESS", 10);
        assert!(hits.iter().any(|s| s.name == "process_all"));
        assert!(hits.iter().any(|s| s.name == "Process_thing"));

        // 完整源码回读
        let sym = idx.find("process_data", 1).into_iter().next().unwrap();
        let src = idx.source_of(sym).unwrap();
        assert!(src.contains("to_uppercase"));
        assert!(src.starts_with("fn process_data"));

        // target 目录被排除
        assert!(idx.find("should_not_index", 5).is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
