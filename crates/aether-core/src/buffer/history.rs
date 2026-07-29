use std::collections::VecDeque;
use std::time::Instant;

use super::piece_table::PieceTable;

/// 基于差分记录（delta）的高效 Undo/Redo
///
/// P0-B: 不再保存整张 piece 表的快照（旧实现每次按键克隆完整 `Vec<Piece>`，
/// 长会话下内存 O(N²) 增长），只记录编辑本身：
/// - 撤销插入 = 删除该范围（add_buffer 只追加不删除，插入天然可逆）
/// - 撤销删除 = 重新插入被删文本
///
/// 每条记录从 KB 级降到几十字节，按键路径零 piece 克隆。
#[derive(Clone, Debug)]
pub struct History {
    /// 撤销步骤栈：每步为一条或一组记录（组内按时间顺序）
    /// CORE-M02: 使用 VecDeque，O(1) 淘汰
    undos: VecDeque<Vec<EditRecord>>,
    redos: VecDeque<Vec<EditRecord>>,
    /// 合并窗口（连续输入合并为一个undo组）
    merge_state: MergeState,
    /// 最大步骤数（默认10000）
    max_records: usize,
    /// REQ-P0-02: 撤销组模式进行中（begin_group..end_group）
    grouping: bool,
    /// 当前组是否已创建步骤（组内首条记录建步，后续追加）
    group_open: bool,
}

/// 编辑差分：记录一次编辑的全部可逆信息
#[derive(Clone, Debug, PartialEq)]
pub enum EditDelta {
    /// 在 pos 处插入了 text
    Insert { pos: usize, text: String },
    /// 在 pos 处删除了 text
    Delete { pos: usize, text: String },
    /// 在 pos 处将 old_text 替换为 new_text
    Replace {
        pos: usize,
        old_text: String,
        new_text: String,
    },
}

impl EditDelta {
    /// 撤销：对缓冲区应用本编辑的逆操作
    pub fn apply_inverse(&self, buffer: &mut PieceTable) {
        match self {
            EditDelta::Insert { pos, text } => buffer.delete(*pos, *pos + text.len()),
            EditDelta::Delete { pos, text } => buffer.insert(*pos, text),
            EditDelta::Replace {
                pos,
                old_text,
                new_text,
            } => {
                buffer.delete(*pos, *pos + new_text.len());
                buffer.insert(*pos, old_text);
            }
        }
    }

    /// 重做：对缓冲区重新应用本编辑
    pub fn apply_forward(&self, buffer: &mut PieceTable) {
        match self {
            EditDelta::Insert { pos, text } => buffer.insert(*pos, text),
            EditDelta::Delete { pos, text } => buffer.delete(*pos, *pos + text.len()),
            EditDelta::Replace {
                pos,
                old_text,
                new_text,
            } => {
                buffer.delete(*pos, *pos + old_text.len());
                buffer.insert(*pos, new_text);
            }
        }
    }
}

/// 单次编辑记录（极轻量：差分 + 光标位置）
#[derive(Clone, Debug)]
pub struct EditRecord {
    delta: EditDelta,
    pub cursor_before: CursorPosition,
    pub cursor_after: CursorPosition,
    /// 编辑时间戳
    timestamp: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

impl CursorPosition {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// 合并状态
#[derive(Clone, Copy, Debug, PartialEq)]
enum MergeState {
    Idle,
    Inserting { last_time: Instant, last_pos: usize },
    Deleting { last_time: Instant, last_pos: usize },
}

/// 连续同位置输入/删除的合并时间窗口（毫秒）
const MERGE_WINDOW_MS: u128 = 500;

impl History {
    pub fn new() -> Self {
        Self {
            undos: VecDeque::new(),
            redos: VecDeque::new(),
            merge_state: MergeState::Idle,
            max_records: 10000,
            grouping: false,
            group_open: false,
        }
    }

    /// REQ-P0-02: 开始撤销组
    /// 组内所有 record_*() 调用归入同一撤销步骤且互不合并，
    /// undo() 会一次性撤销整个组。
    pub fn begin_group(&mut self) {
        self.grouping = true;
        self.group_open = false;
        self.merge_state = MergeState::Idle;
    }

    /// REQ-P0-02: 结束撤销组
    pub fn end_group(&mut self) {
        self.grouping = false;
        self.group_open = false;
        self.merge_state = MergeState::Idle;
    }

    /// 记录一次插入操作（在编辑完成后调用）
    /// 连续快速同位置输入（500ms 窗口）会合并为一条记录
    pub fn record_insert(
        &mut self,
        pos: usize,
        text: &str,
        cursor_before: CursorPosition,
        cursor_after: CursorPosition,
    ) {
        let now = Instant::now();
        if !self.grouping {
            if let MergeState::Inserting {
                last_time,
                last_pos,
            } = self.merge_state
            {
                if now.duration_since(last_time).as_millis() < MERGE_WINDOW_MS && pos == last_pos {
                    // 合并进上一条 Insert 记录：扩展文本、更新光标与时间戳
                    if let Some(step) = self.undos.back_mut() {
                        if step.len() == 1 {
                            if let Some(rec) = step.last_mut() {
                                if let EditDelta::Insert { text: prev, .. } = &mut rec.delta {
                                    prev.push_str(text);
                                    rec.cursor_after = cursor_after;
                                    rec.timestamp = now;
                                    self.merge_state = MergeState::Inserting {
                                        last_time: now,
                                        last_pos: pos + text.len(),
                                    };
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.push_record(
            EditRecord {
                delta: EditDelta::Insert {
                    pos,
                    text: text.to_string(),
                },
                cursor_before,
                cursor_after,
                timestamp: now,
            },
            now,
        );
        if !self.grouping {
            // H-15: 使用实际字节长度，正确处理多字节 UTF-8 字符的连续合并
            self.merge_state = MergeState::Inserting {
                last_time: now,
                last_pos: pos + text.len(),
            };
        }
    }

    /// 记录一次删除操作（在编辑完成后调用，传入被删除的文本）
    /// 连续快速同位置前向删除（Delete 键）会合并为一条记录
    pub fn record_delete(
        &mut self,
        pos: usize,
        deleted_text: String,
        cursor_before: CursorPosition,
        cursor_after: CursorPosition,
    ) {
        let now = Instant::now();
        if !self.grouping {
            if let MergeState::Deleting {
                last_time,
                last_pos,
            } = self.merge_state
            {
                if now.duration_since(last_time).as_millis() < MERGE_WINDOW_MS && pos == last_pos {
                    if let Some(step) = self.undos.back_mut() {
                        if step.len() == 1 {
                            if let Some(rec) = step.last_mut() {
                                if let EditDelta::Delete { text: prev, .. } = &mut rec.delta {
                                    prev.push_str(&deleted_text);
                                    rec.cursor_after = cursor_after;
                                    rec.timestamp = now;
                                    self.merge_state = MergeState::Deleting {
                                        last_time: now,
                                        last_pos: pos,
                                    };
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.push_record(
            EditRecord {
                delta: EditDelta::Delete {
                    pos,
                    text: deleted_text,
                },
                cursor_before,
                cursor_after,
                timestamp: now,
            },
            now,
        );
        if !self.grouping {
            self.merge_state = MergeState::Deleting {
                last_time: now,
                last_pos: pos,
            };
        }
    }

    /// 记录一次替换操作（delete + insert 的原子组合，不参与合并）
    pub fn record_replace(
        &mut self,
        pos: usize,
        old_text: String,
        new_text: &str,
        cursor_before: CursorPosition,
        cursor_after: CursorPosition,
    ) {
        let now = Instant::now();
        self.push_record(
            EditRecord {
                delta: EditDelta::Replace {
                    pos,
                    old_text,
                    new_text: new_text.to_string(),
                },
                cursor_before,
                cursor_after,
                timestamp: now,
            },
            now,
        );
        if !self.grouping {
            self.merge_state = MergeState::Idle;
        }
    }

    /// 将记录放入 undo 栈：组模式追加到当前组步骤，否则新建步骤
    fn push_record(&mut self, record: EditRecord, _now: Instant) {
        if self.grouping && self.group_open {
            if let Some(step) = self.undos.back_mut() {
                step.push(record);
                self.redos.clear();
                return;
            }
        }
        self.undos.push_back(vec![record]);
        if self.grouping {
            self.group_open = true;
        }
        self.redos.clear();

        // 限制步骤数量 — CORE-M02: O(1) pop_front
        while self.undos.len() > self.max_records {
            self.undos.pop_front();
        }
    }

    /// 撤销一步（单条记录或整个撤销组）
    /// 返回：需按序应用 `apply_inverse` 的差分列表（组内已逆序）+ 撤销后光标位置
    pub fn undo(&mut self) -> Option<(Vec<EditDelta>, CursorPosition)> {
        let step = self.undos.pop_back()?;
        let cursor = step[0].cursor_before;
        let deltas: Vec<EditDelta> = step.iter().rev().map(|r| r.delta.clone()).collect();
        self.redos.push_back(step);
        self.merge_state = MergeState::Idle;
        Some((deltas, cursor))
    }

    /// 重做一步
    /// 返回：需按序应用 `apply_forward` 的差分列表（时间顺序）+ 重做后光标位置
    pub fn redo(&mut self) -> Option<(Vec<EditDelta>, CursorPosition)> {
        let step = self.redos.pop_back()?;
        let cursor = step.last()?.cursor_after;
        let deltas: Vec<EditDelta> = step.iter().map(|r| r.delta.clone()).collect();
        self.undos.push_back(step);
        self.merge_state = MergeState::Idle;
        Some((deltas, cursor))
    }

    pub fn can_undo(&self) -> bool {
        !self.undos.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redos.is_empty()
    }

    pub fn clear(&mut self) {
        self.undos.clear();
        self.redos.clear();
        self.merge_state = MergeState::Idle;
        self.grouping = false;
        self.group_open = false;
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cur(line: usize, col: usize) -> CursorPosition {
        CursorPosition::new(line, col)
    }

    /// 对 buffer 应用一次 undo 步骤
    fn apply_undo(history: &mut History, buffer: &mut PieceTable) -> Option<CursorPosition> {
        let (deltas, cursor) = history.undo()?;
        for d in &deltas {
            d.apply_inverse(buffer);
        }
        Some(cursor)
    }

    /// 对 buffer 应用一次 redo 步骤
    fn apply_redo(history: &mut History, buffer: &mut PieceTable) -> Option<CursorPosition> {
        let (deltas, cursor) = history.redo()?;
        for d in &deltas {
            d.apply_forward(buffer);
        }
        Some(cursor)
    }

    #[test]
    fn test_undo_redo_roundtrip_insert() {
        let mut buffer = PieceTable::from_string("hello".to_string());
        let mut history = History::new();

        buffer.insert(5, " world");
        history.record_insert(5, " world", cur(0, 5), cur(0, 11));
        assert_eq!(buffer.get_all_text(), "hello world");

        let cursor = apply_undo(&mut history, &mut buffer).unwrap();
        assert_eq!(buffer.get_all_text(), "hello");
        assert_eq!(cursor, cur(0, 5));
        assert!(history.can_redo());

        let cursor = apply_redo(&mut history, &mut buffer).unwrap();
        assert_eq!(buffer.get_all_text(), "hello world");
        assert_eq!(cursor, cur(0, 11));
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_undo_redo_roundtrip_delete() {
        let mut buffer = PieceTable::from_string("hello world".to_string());
        let mut history = History::new();

        let deleted = buffer.get_text(5, 11);
        buffer.delete(5, 11);
        history.record_delete(5, deleted, cur(0, 11), cur(0, 5));
        assert_eq!(buffer.get_all_text(), "hello");

        apply_undo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "hello world");

        apply_redo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "hello");
    }

    #[test]
    fn test_undo_redo_roundtrip_replace() {
        let mut buffer = PieceTable::from_string("foo bar foo".to_string());
        let mut history = History::new();

        let old = buffer.get_text(4, 7);
        buffer.delete(4, 7);
        buffer.insert(4, "bazzz");
        history.record_replace(4, old, "bazzz", cur(0, 4), cur(0, 9));
        assert_eq!(buffer.get_all_text(), "foo bazzz foo");

        apply_undo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "foo bar foo");

        apply_redo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "foo bazzz foo");
    }

    #[test]
    fn test_merge_inserts() {
        let mut history = History::new();
        // 快速连续同位置插入应合并为一条
        history.record_insert(0, "a", cur(0, 0), cur(0, 1));
        history.record_insert(1, "b", cur(0, 1), cur(0, 2));
        assert_eq!(history.undos.len(), 1);

        // 合并后的记录一次 undo 应删除全部两个字符
        let mut buffer = PieceTable::from_string("ab".to_string());
        let cursor = apply_undo(&mut history, &mut buffer).unwrap();
        assert_eq!(buffer.get_all_text(), "");
        assert_eq!(cursor, cur(0, 0));
    }

    #[test]
    fn test_merge_deletes_forward() {
        let mut history = History::new();
        let mut buffer = PieceTable::from_string("abcde".to_string());
        // Delete 键在同一位置连续前向删除
        let d1 = buffer.get_text(1, 2);
        buffer.delete(1, 2);
        history.record_delete(1, d1, cur(0, 1), cur(0, 1));
        let d2 = buffer.get_text(1, 2);
        buffer.delete(1, 2);
        history.record_delete(1, d2, cur(0, 1), cur(0, 1));
        assert_eq!(history.undos.len(), 1);
        assert_eq!(buffer.get_all_text(), "ade");

        apply_undo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "abcde");
    }

    #[test]
    fn test_backspace_not_merged() {
        let mut history = History::new();
        // 退格位置递减，不满足同位置合并条件
        history.record_delete(4, "e".to_string(), cur(0, 5), cur(0, 4));
        history.record_delete(3, "d".to_string(), cur(0, 4), cur(0, 3));
        assert_eq!(history.undos.len(), 2);
    }

    #[test]
    fn test_replace_not_merged() {
        let mut history = History::new();
        history.record_replace(0, "a".to_string(), "x", cur(0, 0), cur(0, 1));
        history.record_replace(1, "b".to_string(), "y", cur(0, 1), cur(0, 2));
        assert_eq!(history.undos.len(), 2);
    }

    #[test]
    fn test_new_record_clears_redo() {
        let mut history = History::new();
        history.record_insert(0, "a", cur(0, 0), cur(0, 1));
        let _ = history.undo();
        assert!(history.can_redo());
        history.record_insert(0, "b", cur(0, 0), cur(0, 1));
        assert!(!history.can_redo());
    }

    #[test]
    fn test_history_clear() {
        let mut history = History::new();
        history.record_insert(0, "a", cur(0, 0), cur(0, 1));
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_undos_limit() {
        let mut history = History::new();
        for i in 0..10010 {
            history.record_replace(0, "a".to_string(), "b", cur(0, i), cur(0, i + 1));
        }
        assert_eq!(history.undos.len(), 10000);
    }

    #[test]
    fn test_group_undo_redo() {
        let mut buffer = PieceTable::from_string("aaa bbb aaa".to_string());
        let mut history = History::new();

        // 模拟 replace_all："aaa" -> "xx"（从后往前替换）
        history.begin_group();
        for &pos in &[8usize, 0usize] {
            let old = buffer.get_text(pos, pos + 3);
            buffer.delete(pos, pos + 3);
            buffer.insert(pos, "xx");
            history.record_replace(pos, old, "xx", cur(0, 0), cur(0, 0));
        }
        history.end_group();
        assert_eq!(buffer.get_all_text(), "xx bbb xx");
        // 组内多条记录合成一个撤销步骤
        assert_eq!(history.undos.len(), 1);

        // 一次 undo 撤销整个组
        apply_undo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "aaa bbb aaa");
        assert!(!history.can_undo());

        // 一次 redo 恢复整个组
        apply_redo(&mut history, &mut buffer);
        assert_eq!(buffer.get_all_text(), "xx bbb xx");
    }

    #[test]
    fn test_group_records_not_merged() {
        let mut history = History::new();
        history.begin_group();
        history.record_insert(0, "a", cur(0, 0), cur(0, 1));
        history.record_insert(1, "b", cur(0, 1), cur(0, 2));
        history.record_insert(2, "c", cur(0, 2), cur(0, 3));
        history.end_group();
        // 单步骤内 3 条独立记录（未合并）
        assert_eq!(history.undos.len(), 1);
        assert_eq!(history.undos[0].len(), 3);
    }

    #[test]
    fn test_group_cursor_positions() {
        let mut history = History::new();
        history.begin_group();
        history.record_insert(0, "a", cur(0, 0), cur(0, 1));
        history.record_insert(1, "b", cur(0, 1), cur(0, 2));
        history.end_group();

        let (_, cursor) = history.undo().unwrap();
        // 撤销组回到组首记录的 cursor_before
        assert_eq!(cursor, cur(0, 0));
        let (_, cursor) = history.redo().unwrap();
        // 重做组回到组尾记录的 cursor_after
        assert_eq!(cursor, cur(0, 2));
    }
}
