//! `WM_MBUTTONDOWN` 处理：中键点击关闭标签页、图片预览中键拖拽。
//!
//! SubTask 7.1: 当用户在标签栏中某个标签上按下鼠标中键时，关闭该标签。
//! 复用 `EditorState::close_tab` 的 dirty 检查逻辑，与关闭按钮行为一致。
//! 图片预览模式下，中键按下开始拖拽平移。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use super::super::{get_and_set_state, invalidate_window};

/// WM_MBUTTONDOWN：鼠标中键按下事件。
///
/// 仅响应标签栏区域内的中键点击：命中标签则调用 `close_tab(index)`，
/// 由 `close_tab` 内部统一处理 dirty 检查（活动标签走 `close_current_tab_checked`，
/// 非活动标签走 dirty 询问对话框）。
/// 图片预览模式下，中键按下开始拖拽平移。
pub(crate) unsafe fn on_m_button_down(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
    let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
    let Some(state) = get_and_set_state(hwnd) else {
        return LRESULT(0);
    };
    let (mouse_x, mouse_y, layout) = {
        let st = state.borrow();
        (
            raw_x / st.win.dpi_scale,
            raw_y / st.win.dpi_scale,
            st.ui.layout.clone(),
        )
    };
    let mut st = state.borrow_mut();

    // 图片预览模式：中键按下开始拖拽
    if st.editor.content.language == aether_core::lexer::Language::Image {
        let editor = layout.editor_region();
        if editor.contains(mouse_x, mouse_y) {
            st.input.mouse_press.image_dragging = true;
            st.input.mouse_press.image_drag_start = Some((mouse_x, mouse_y));
            st.input.mouse_press.image_drag_offset =
                Some((st.win.image_offset_x, st.win.image_offset_y));
            return LRESULT(0);
        }
    }

    let show_tab_bar = st.show_tab_bar();
    let tab_region = layout.tab_bar_region(show_tab_bar);
    if !show_tab_bar || !tab_region.contains(mouse_x, mouse_y) {
        return LRESULT(0);
    }
    // 命中检测：与 handle_tab_bar_click 一致，应用 tab_scroll_x 偏移
    let rel_x = mouse_x - tab_region.x + st.editor.tab_bar.tab_scroll_x;
    let mut hit_index: Option<usize> = None;
    for layout_entry in &st.editor.tab_bar.tab_layouts {
        if rel_x >= layout_entry.x && rel_x < layout_entry.x + layout_entry.width {
            hit_index = Some(layout_entry.index);
            break;
        }
    }
    let Some(index) = hit_index else {
        return LRESULT(0);
    };
    // SubTask 7.1: 复用 close_tab 的 dirty 检查逻辑（与关闭按钮、Ctrl+W 行为一致）
    st.close_tab(index);
    drop(st);
    invalidate_window(hwnd);
    LRESULT(0)
}
