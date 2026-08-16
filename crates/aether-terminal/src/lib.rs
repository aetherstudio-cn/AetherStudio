//! 终端面板：ConPTY、输出缓冲、Agent 命令回环
#![allow(clippy::new_without_default)]

pub mod conpty;
pub mod terminal;

pub use terminal::TerminalPanel;
