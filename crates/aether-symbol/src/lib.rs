//! aether-symbol：面向 AI 的轻量工作区符号索引组件。
//!
//! 动机：传统 LSP（rust-analyzer 等）常驻内存动辄 GB 级，且能力面向"交互式编辑"，
//! AI 智能体真正需要的只是「按名字找到函数/变量/类的定义位置并读到完整源码」。
//! 本组件用零依赖的行级声明提取 + 括号配对源码回读实现这一能力：
//!
//! - [`extract`]：按语言启发式提取符号声明（名称/种类/行号/签名），不做大面积解析；
//! - [`source`]：从声明行回读整函数/整类源码（大括号配对 / Python 缩进块）；
//! - [`index`]：遍历工作区构建名称索引（精确/前缀/子串检索），内存仅存声明摘要。
//!
//! 典型用法（AI 工具执行侧）：
//! ```ignore
//! let idx = SymbolIndex::build(workspace_root);
//! for sym in idx.find("process_data", 20) {
//!     // sym.kind / sym.path / sym.line / sym.signature
//!     let full = idx.source_of(sym); // 整函数源码，回喂给模型
//! }
//! ```

pub mod extract;
pub mod index;
pub mod source;

pub use extract::{extract_symbols, Lang, Symbol, SymbolKind};
pub use index::SymbolIndex;
pub use source::symbol_source;
