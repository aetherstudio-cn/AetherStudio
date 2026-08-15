use super::*;

/// P2-6: 把字节偏移对齐到字符边界（向下取到下一个字符起点）。
/// 避免 selection_end 落在多字节字符中间导致渲染/截取异常。
fn clamp_to_char_boundary(content: &TabContent, line_idx: usize, byte_pos: usize) -> usize {
    if let Some(line) = content.buffer.get_line(line_idx) {
        let max = line.len();
        if byte_pos >= max {
            return max;
        }
        // 向前微调到字符边界（byte_pos 通常已在边界上，此处做防御性对齐）
        let mut p = byte_pos;
        while p > 0 && !line.is_char_boundary(p) {
            p -= 1;
        }
        p
    } else {
        byte_pos
    }
}

/// 查找所有匹配位置
/// 优化：缓存查询结果，避免查询未变且文本未变时重复全量扫描
pub fn find_all(find_state: &mut FindState, content: &TabContent) {
    find_state.active_index = 0;
    if find_state.query.is_empty() {
        find_state.results.clear();
        find_state.last_query.clear();
        return;
    }
    // 缓存命中：查询和文本版本都未变，跳过搜索
    if find_state.query == find_state.last_query
        && find_state.result_version == content.buffer_version
        && !find_state.results.is_empty()
    {
        // 结果已有效，无需重新搜索
        return;
    }
    // 缓存未命中：清空并重新搜索
    find_state.results.clear();
    let query = find_state.query.clone();
    let total_lines = content.buffer.len_lines();
    for line_idx in 0..total_lines {
        if let Some(line) = content.buffer.get_line(line_idx) {
            let mut start = 0;
            while let Some(pos) = line[start..].find(&query) {
                let abs_pos = start + pos;
                find_state.results.push((line_idx, abs_pos));
                start = abs_pos + query.len();
                if start >= line.len() {
                    break;
                }
            }
        }
    }
    // 更新缓存状态
    find_state.last_query = query;
    find_state.result_version = content.buffer_version;
}

/// 跳转到下一个匹配
pub fn find_next(find_state: &mut FindState, content: &mut TabContent) {
    if find_state.results.is_empty() {
        find_all(find_state, content);
    }
    if !find_state.results.is_empty() {
        find_state.active_index = (find_state.active_index + 1) % find_state.results.len();
        let (line, col) = find_state.results[find_state.active_index];
        // P2-6: 选区末尾对齐到字符边界；cursor_col 置于匹配末尾以符合编辑器约定
        let end_col = clamp_to_char_boundary(content, line, col + find_state.query.len());
        content.cursor_line = line;
        content.cursor_col = end_col;
        // 选中匹配文本
        content.selection_start = Some((line, col));
        content.selection_end = Some((line, end_col));
    }
}

/// 跳转到上一个匹配
pub fn find_prev(find_state: &mut FindState, content: &mut TabContent) {
    if find_state.results.is_empty() {
        find_all(find_state, content);
    }
    if !find_state.results.is_empty() {
        if find_state.active_index == 0 {
            find_state.active_index = find_state.results.len() - 1;
        } else {
            find_state.active_index -= 1;
        }
        let (line, col) = find_state.results[find_state.active_index];
        // P2-6: 选区末尾对齐到字符边界
        let end_col = clamp_to_char_boundary(content, line, col + find_state.query.len());
        content.cursor_line = line;
        content.cursor_col = end_col;
        content.selection_start = Some((line, col));
        content.selection_end = Some((line, end_col));
    }
}

/// 替换当前匹配
pub fn replace_current(find_state: &mut FindState, content: &mut TabContent) -> bool {
    if find_state.results.is_empty() || find_state.active_index >= find_state.results.len() {
        return false;
    }
    let (line, col) = find_state.results[find_state.active_index];
    let pos = content.buffer.line_start_byte(line) + col;
    let end_pos = pos + find_state.query.len();

    let old_text = content.buffer.get_text(pos, end_pos);
    let cursor_before = CursorPosition::new(content.cursor_line, content.cursor_col);

    content.buffer.delete(pos, end_pos);
    content.buffer.insert(pos, &find_state.replace_text);
    content.is_dirty = true;
    content.buffer_version += 1;

    content.cursor_line = line;
    content.cursor_col = col + find_state.replace_text.len();
    let cursor_after = CursorPosition::new(content.cursor_line, content.cursor_col);
    content.history.record_replace(
        pos,
        old_text,
        &find_state.replace_text,
        cursor_before,
        cursor_after,
    );

    // 重新查找
    find_all(find_state, content);
    true
}

/// 切换查找面板
pub fn toggle_find(find_state: &mut FindState, content: &TabContent) {
    find_state.visible = !find_state.visible;
    if !find_state.visible {
        find_state.replace_visible = false;
        find_state.focus = FindReplaceFocus::None;
    } else {
        find_state.focus = FindReplaceFocus::FindQuery;
    }
    if find_state.visible && !find_state.query.is_empty() {
        find_all(find_state, content);
    }
}

/// 切换替换面板
pub fn toggle_replace(find_state: &mut FindState, content: &TabContent) {
    find_state.replace_visible = !find_state.replace_visible;
    find_state.visible = find_state.replace_visible || find_state.visible;
    if !find_state.visible {
        find_state.focus = FindReplaceFocus::None;
    } else {
        find_state.focus = if find_state.replace_visible {
            FindReplaceFocus::FindQuery
        } else {
            FindReplaceFocus::None
        };
    }
    if find_state.visible && !find_state.query.is_empty() {
        find_all(find_state, content);
    }
}

/// 关闭查找替换面板
pub fn close_find_replace(find_state: &mut FindState) {
    find_state.visible = false;
    find_state.replace_visible = false;
    find_state.focus = FindReplaceFocus::None;
}

impl FindState {
    /// 查找所有匹配位置
    /// 优化：缓存查询结果，避免查询未变且文本未变时重复全量扫描
    pub fn find_all(&mut self, content: &TabContent) {
        find_all(self, content)
    }

    /// 跳转到下一个匹配
    pub fn find_next(&mut self, content: &mut TabContent) {
        find_next(self, content)
    }

    /// 跳转到上一个匹配
    pub fn find_prev(&mut self, content: &mut TabContent) {
        find_prev(self, content)
    }

    /// 替换当前匹配
    pub fn replace_current(&mut self, content: &mut TabContent) -> bool {
        replace_current(self, content)
    }

    /// 切换查找面板
    pub fn toggle_find(&mut self, content: &TabContent) {
        toggle_find(self, content)
    }

    /// 切换替换面板
    pub fn toggle_replace(&mut self, content: &TabContent) {
        toggle_replace(self, content)
    }

    /// 关闭查找替换面板
    pub fn close_find_replace(&mut self) {
        close_find_replace(self)
    }
}

/// 替换所有匹配
/// REQ-P0-02: 使用 begin_group/end_group 包裹，记录撤销历史
pub fn replace_all(state: &mut EditorState) -> usize {
    if state.editor.find.query.is_empty() || state.editor.find.query == state.editor.find.replace_text {
        return 0;
    }
    state.editor.find.find_all(&state.editor.content);
    let count = state.editor.find.results.len();
    if count == 0 {
        return 0;
    }

    // REQ-P1-04: 转换为全局字节偏移，避免替换文本含换行符时行号偏移
    let query_len = state.editor.find.query.len();
    let replace_text = state.editor.find.replace_text.clone();
    let mut global_offsets: Vec<usize> = state
        .editor.find
        .results
        .iter()
        .map(|(line, col)| state.line_byte_start(*line) + *col)
        .collect();
    // 降序排序：从文件末尾向前替换，前面的位置不受影响
    global_offsets.sort_by(|a, b| b.cmp(a));

    // REQ-P0-02: 记录替换前的光标位置，用于撤销后恢复
    let cursor_before = CursorPosition::new(state.editor.content.cursor_line, state.editor.content.cursor_col);

    // REQ-P0-02: 开始撤销组，所有替换作为一个原子撤销单元
    state.editor.content.history.begin_group();

    for pos in global_offsets {
        let end_pos = pos + query_len;

        // REQ-P0-02: 捕获被替换的原文本，供差分撤销使用
        let old_text = state.editor.content.buffer.get_text(pos, end_pos);

        state.editor.content.buffer.delete(pos, end_pos);
        state.editor.content.buffer.insert(pos, &replace_text);

        // REQ-P0-02: 记录每次替换的编辑历史
        state.editor.content.history.record_replace(
            pos,
            old_text,
            &replace_text,
            cursor_before,
            cursor_before,
        );
    }

    // REQ-P0-02: 结束撤销组
    state.editor.content.history.end_group();

    state.editor.content.is_dirty = true;
    if let Some(tab) = state.editor.tab_bar.tabs.get_mut(state.editor.tab_bar.active_tab) {
        tab.mark_dirty();
    }
    state.editor.content.buffer_version += 1;
    state.editor.find.results.clear();
    state.editor.find.active_index = 0;
    state.ui.status_message = format!("已替换 {} 处", count);
    state.emit_edit_events();
    count
}

impl EditorState {
    /// 替换所有匹配
    /// REQ-P0-02: 使用 begin_group/end_group 包裹，记录撤销历史
    pub fn replace_all(&mut self) -> usize {
        replace_all(self)
    }
}
