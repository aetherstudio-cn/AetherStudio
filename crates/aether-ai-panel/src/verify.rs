//! 长任务验证门与检查点回滚（免训练 harness 层）。
//!
//! 对标 Dependable Long-Horizon Agents（arXiv 2611.13254）：多数失败在出错前
//! 轨迹里已有反证却未被处理，且约三分之一失败仅靠恢复状态即可救回。本模块提供：
//! - **验证门**：编辑落盘后重读环境校验是否真正生效（而非信任写入返回值）；
//! - **检查点回滚**：任务前快照原始内容，验证失败时恢复，避免半应用/损坏状态。
//!
//! 纯逻辑、无 IO，便于单测；实际读写由调用方（win32 编排器）执行。

/// 文件检查点：任务执行前的原始内容快照。
/// `original = None` 表示文件当时不存在（新建），回滚时应删除。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCheckpoint {
    pub path: String,
    pub original: Option<String>,
}

/// 回滚动作（由检查点推导）：恢复原始内容，或删除新建的文件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RollbackAction {
    Restore(String),
    Delete,
}

impl FileCheckpoint {
    /// 推导回滚动作：原有内容则恢复，原本不存在则删除。
    pub fn rollback_action(&self) -> RollbackAction {
        match &self.original {
            Some(content) => RollbackAction::Restore(content.clone()),
            None => RollbackAction::Delete,
        }
    }
}

/// 验证门：校验编辑是否真正生效（对照落盘后的实际内容 `after`）。
///
/// - 新建（search 为空）：after 非空即生效；
/// - 删除（replace 为空）：由调用方校验文件不存在，本函数视为通过；
/// - 修改：after 必须包含 replace 内容（替换确实落盘）。
pub fn verify_edit_applied(search: &str, replace: &str, after: &str) -> bool {
    if search.trim().is_empty() {
        // 新建文件：内容非空
        return !after.trim().is_empty();
    }
    if replace.trim().is_empty() {
        // 删除：文件存在性由调用方校验
        return true;
    }
    // 修改：替换内容必须出现在落盘结果中
    after.contains(replace)
}

/// 批量校验：所有编辑都生效才返回 true。
pub fn verify_edits_applied(edits: &[(String, String, String)]) -> bool {
    edits
        .iter()
        .all(|(search, replace, after)| verify_edit_applied(search, replace, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_verified_when_nonempty() {
        assert!(verify_edit_applied("", "fn main() {}", "fn main() {}"));
        assert!(!verify_edit_applied("", "x", "   "));
        assert!(!verify_edit_applied("", "x", ""));
    }

    #[test]
    fn modify_verified_when_replace_present() {
        let after = "line1\nNEW_CONTENT\nline3";
        assert!(verify_edit_applied("OLD_CONTENT", "NEW_CONTENT", after));
        assert!(!verify_edit_applied(
            "OLD_CONTENT",
            "NEW_CONTENT",
            "line1\nOLD_CONTENT\nline3"
        ));
    }

    #[test]
    fn delete_passes_content_check() {
        assert!(verify_edit_applied("old", "", ""));
    }

    #[test]
    fn batch_requires_all_applied() {
        let ok = vec![
            ("a".to_string(), "b".to_string(), "xbx".to_string()),
            ("".to_string(), "c".to_string(), "c".to_string()),
        ];
        assert!(verify_edits_applied(&ok));
        let bad = vec![
            ("a".to_string(), "b".to_string(), "xbx".to_string()),
            ("a".to_string(), "b".to_string(), "xax".to_string()),
        ];
        assert!(!verify_edits_applied(&bad));
    }

    #[test]
    fn checkpoint_restore_existing() {
        let cp = FileCheckpoint {
            path: "a.rs".to_string(),
            original: Some("old".to_string()),
        };
        assert_eq!(
            cp.rollback_action(),
            RollbackAction::Restore("old".to_string())
        );
    }

    #[test]
    fn checkpoint_delete_new_file() {
        let cp = FileCheckpoint {
            path: "new.rs".to_string(),
            original: None,
        };
        assert_eq!(cp.rollback_action(), RollbackAction::Delete);
    }
}
