//! 编辑器核心：文本编辑、光标、选择、查找替换、自动保存、事件系统
#![allow(clippy::new_without_default)]

pub mod dirty_rect;
pub mod events;
pub mod focus_manager;
pub mod inline_completion;
pub mod undo_delete;
