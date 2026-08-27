//! `WM_MOUSEMOVE` 处理函数及辅助函数。
//!
//! 从 `window.rs` 拆分而来，保持原有逻辑不变。
//! 原函数 380 行，拆分为调度器 + 多个辅助函数。

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::cursor::CursorType;
use crate::editor::EditorState;

use super::super::{
    get_and_set_state, invalidate_window, EDITOR_STATE, HOVER_DELAY_MS, HOVER_MOVE_TOLERANCE,
    HOVER_TIMER_ID, LP_MOVE_TOLERANCE, LP_TIMER_ID,
};

/// 面板拖拽同步重绘节流（~120fps）。
///
/// 高回报率鼠标下每条 WM_MOUSEMOVE 都同步重绘会压垮管线产生卡顿感，
/// 故节流至 8ms 最小帧间隔；被跳过的帧由后续 WM_PAINT 合并补齐。
/// 返回 true 表示本帧应调用 UpdateWindow 立即重绘。
fn panel_drag_should_sync_paint() -> bool {
    const MIN_FRAME: std::time::Duration = std::time::Duration::from_millis(8);
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let now = std::time::Instant::now();
    let mut last = LAST.lock().unwrap();
    match *last {
        Some(t) if now.duration_since(t) < MIN_FRAME => false,
        _ => {
            *last = Some(now);
            true
        }
    }
}

/// WM_MOUSEMOVE：鼠标移动事件调度器。
pub(crate) unsafe fn on_mouse_move(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> LRESULT {
    let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
    let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
    let is_dragging = wparam.0 & 0x0001 != 0; // MK_LBUTTON
    let Some(state) = get_and_set_state(hwnd) else {
        return LRESULT(0);
    };
    let (mouse_x, mouse_y, layout) = {
        let st = state.borrow_mut();
        let mouse_x = raw_x / st.win.dpi_scale;
        let mouse_y = raw_y / st.win.dpi_scale;
        let layout = st.ui.layout.clone();
        (mouse_x, mouse_y, layout)
    };
    // 早期返回：对话框悬停 / 自定义模式拖拽
    if let Some(r) = omm_early_returns(hwnd, &state, mouse_x, mouse_y, is_dragging, &layout) {
        return r;
    }

    // 图片预览拖拽：中键拖拽平移（优先级最高，跳过其他 hover 检测）
    let is_mbutton_dragging = wparam.0 & 0x0010 != 0; // MK_MBUTTON
    if is_mbutton_dragging {
        let mut st = state.borrow_mut();
        if st.input.mouse_press.image_dragging {
            if let (Some((start_x, start_y)), Some((orig_offset_x, orig_offset_y))) = (
                st.input.mouse_press.image_drag_start,
                st.input.mouse_press.image_drag_offset,
            ) {
                let dx = mouse_x - start_x;
                let dy = mouse_y - start_y;
                st.win.image_offset_x = orig_offset_x + dx;
                st.win.image_offset_y = orig_offset_y + dy;
                invalidate_window(hwnd);
                return LRESULT(0);
            }
        }
    }
    // 文本拖拽选区：前置为最高优先级（仅次于菜单/对话框）。
    // 旧实现放在所有 hover 判定之后的 else 分支，任一 hover 变化都会
    // 提前走 invalidate 分支跳过选区更新，导致拖拽时预选中高亮跟不上鼠标；
    // 拖拽选区期间 hover/tooltip 状态无意义，直接跳过还能省掉逐帧命中开销。
    if is_dragging {
        // 历史浮窗拖动：最高优先级（浮窗覆盖所有内容，拖动中跳过其他拖拽/选区）
        {
            let mut st = state.borrow_mut();
            if let Some((off_x, off_y)) = st.ai.ai_panel.history_win_drag {
                let (win_w, win_h) = st.ai.ai_panel.history_win_size;
                // 新位置 = 鼠标 - 偏移，钳制在窗口客户区内
                let max_x = (st.win.window_width as f32 - win_w).max(0.0);
                let max_y = (st.win.window_height as f32 - win_h).max(0.0);
                let nx = (mouse_x - off_x).clamp(0.0, max_x);
                let ny = (mouse_y - off_y).clamp(0.0, max_y);
                st.ai.ai_panel.history_win_pos = Some((nx, ny));
                drop(st);
                // 拖动中设置握紧手形光标
                let hcursor = LoadCursorW(None, IDC_SIZEALL).unwrap_or_default();
                let _ = SetCursor(hcursor);
                invalidate_window(hwnd);
                return LRESULT(0);
            }
        }
        // 面板拖拽前置：跳过全部 hover 检测（拖拽中 hover 无意义），
        // 直接处理分割线/拐角调整并提前返回，避免热路径上 7 个 hover 命中的逐帧开销。
        let panel_dragging = {
            let st = state.borrow();
            st.ui.layout.right_panel_resizing
                || st.ui.layout.bottom_panel_resizing
                || st.ui.layout.sidebar_resizing
                || st.ui.layout.corner_left_resizing
                || st.ui.layout.corner_right_resizing
        };
        if panel_dragging {
            if let Some(r) = omm_resize_drag(hwnd, &state, mouse_x, mouse_y, is_dragging, &layout) {
                return r;
            }
            return LRESULT(0);
        }

        let mut st = state.borrow_mut();
        let editor_content = layout.editor_content_region(st.show_tab_bar());

        // 滚动条：拖拽优先（SetCapture 期间鼠标可能在窗口外）；未拖拽时更新悬停高亮
        // 经典模式与智能体模式共用统一入口（区域由 active_code_editor_region 裁定）
        match crate::editor::scrollbar::handle_move(&mut st, mouse_x, mouse_y) {
            crate::editor::scrollbar::MoveOutcome::Dragged => {
                invalidate_window(hwnd);
                return LRESULT(0);
            }
            crate::editor::scrollbar::MoveOutcome::HoverChanged => invalidate_window(hwnd),
            crate::editor::scrollbar::MoveOutcome::Ignored => {}
        }

        // 如果尚未进入选区模式，检查鼠标是否移动了足够距离来启动选区
        if !st.editor.is_selecting {
            // 记录鼠标按下位置（在 WM_LBUTTONDOWN 时设置）
            // 仅当按下位置在编辑区内时才允许启动选区，
            // 防止从菜单栏/侧边栏/AI面板拖入编辑区时误触选区
            if let Some((press_x, press_y)) = st.input.mouse_press.lbutton_down_pos {
                if editor_content.contains(press_x, press_y) {
                    let dx = mouse_x - press_x;
                    let dy = mouse_y - press_y;
                    // 超过 3px 阈值才启动选区（避免单击时的微小抖动）
                    if dx * dx + dy * dy > 9.0 {
                        st.start_selection();
                    }
                }
            }
        }

        if st.editor.is_selecting {
            let before = (
                st.editor.content.cursor_line,
                st.editor.content.cursor_col,
                st.editor.content.selection_end,
            );
            st.set_cursor_from_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
            st.update_selection();
            let changed = (
                st.editor.content.cursor_line,
                st.editor.content.cursor_col,
                st.editor.content.selection_end,
            ) != before;
            if changed {
                // 标记编辑区+状态栏脏区：避免无脏区退化为全窗口无裁剪重绘
                st.win.dirty_tracker.mark_region(
                    editor_content.x,
                    editor_content.y,
                    editor_content.width,
                    editor_content.height,
                    crate::dirty_rect::DirtyRegionType::EditorContent,
                );
                let sb = st.ui.layout.status_bar_region();
                st.win.dirty_tracker.mark_region(
                    sb.x,
                    sb.y,
                    sb.width,
                    sb.height,
                    crate::dirty_rect::DirtyRegionType::StatusBar,
                );
            }
            drop(st);
            // 光标未跨过字符边界时跳过重绘，避免鼠标微动刷帧
            if changed {
                invalidate_window(hwnd);
                // WM_PAINT 优先级低于 WM_MOUSEMOVE，快速拖拽时会被消息洪流饿死，
                // 导致选区/光标视觉滞后——UpdateWindow 绕过队列立即重绘
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return LRESULT(0);
        }
        drop(st);
    }
    // 文件树拖拽：按下候选节点后处理阈值判定与放置目标/浮标更新。
    // 进入拖拽后独占本次消息（跳过 hover/tooltip 更新，避免高亮叠加）。
    if is_dragging
        && state
            .borrow()
            .input
            .mouse_press
            .file_tree_drag_node
            .is_some()
    {
        let mut st = state.borrow_mut();
        let changed = st.file_drag_update(mouse_x, mouse_y);
        let dragging_now = st.input.mouse_press.file_tree_dragging;
        drop(st);
        if dragging_now {
            // 拖出侧边栏 → 转交系统 OLE 拖放（CF_HDROP + CF_UNICODETEXT），
            // Explorer 收文件对象，输入框收绝对路径文本。内部拖拽状态
            // 必须先清空并释放 RefCell 借用，再进入 DoDragDrop 模态循环。
            if !layout.sidebar_region().contains(mouse_x, mouse_y) {
                let paths = {
                    let st = state.borrow();
                    st.file_drag_external_source_path().map(|p| vec![p])
                };
                if let Some(paths) = paths {
                    state.borrow_mut().file_drag_abort_internal();
                    // 先擦掉侧边栏内的浮标/高亮，再进入阻塞式模态循环
                    invalidate_window(hwnd);
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
                    let _ = crate::file_drag_drop::start_ole_file_drag(paths);
                    return LRESULT(0);
                }
            }
            if changed {
                invalidate_window(hwnd);
                // 同文本拖拽选区思路绕过消息队列立即重绘，但高回报率鼠标
                // 下每条 WM_MOUSEMOVE 都同步重绘会压垮管线产生卡顿感，
                // 故节流至 ~120fps；被跳过的帧由后续 WM_PAINT 合并补齐
                let mut st = state.borrow_mut();
                if st.file_drag_should_sync_paint() {
                    drop(st);
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
                }
            }
            return LRESULT(0);
        }
    }
    // 悬停状态更新（每个辅助函数返回是否有变化）
    let titlebar_changed = omm_titlebar_menu_hover(&state, mouse_x, mouse_y, &layout);
    let (tab_changed, _editor_content) = omm_activity_tab_hover(&state, mouse_x, mouse_y, &layout);
    let tree_changed = omm_file_tree_hover(&state, mouse_x, mouse_y, &layout);
    let settings_changed = omm_settings_hover(&state, mouse_x, mouse_y, is_dragging, &layout);
    let ai_changed = omm_ai_hover(&state, mouse_x, mouse_y, &layout);
    let welcome_changed = omm_welcome_hover(&state, mouse_x, mouse_y, &layout);
    let status_bar_changed = omm_status_bar_hover(&state, mouse_x, mouse_y, &layout);
    let any_hover_changed = titlebar_changed
        || tab_changed
        || tree_changed
        || settings_changed
        || ai_changed
        || welcome_changed
        || status_bar_changed;
    // 拖拽光标 + 面板拖拽调整
    if let Some(r) = omm_resize_drag(hwnd, &state, mouse_x, mouse_y, is_dragging, &layout) {
        return r;
    }
    // Hover tooltip 防抖
    omm_hover_tooltip(hwnd, &state, mouse_x, mouse_y, &layout);
    // UI Tooltip 状态更新（500ms 延迟显示、4px 移动容差）
    let tooltip_changed = omm_tooltip_state(hwnd, &state, mouse_x, mouse_y);
    // 最终失效判定（文本拖拽选区已在前置分支处理并提前返回）
    if any_hover_changed || tooltip_changed {
        // 标记 hover 相关脏区域，避免全窗口重绘
        let mut st = state.borrow_mut();
        if titlebar_changed {
            // 标题栏 hover 变化：标记标题栏区域脏，确保窗控按钮 hover 高亮即时刷新
            let tb = layout.title_bar_region();
            st.win.dirty_tracker.mark_region(
                tb.x,
                tb.y,
                tb.width,
                tb.height,
                crate::dirty_rect::DirtyRegionType::TitleBar,
            );
        }
        if tab_changed {
            // 活动栏 + 标签栏区域
            let ar = layout.activity_bar_region();
            st.win.dirty_tracker.mark_region(
                ar.x,
                ar.y,
                ar.width,
                ar.height,
                crate::dirty_rect::DirtyRegionType::ActivityBar,
            );
            let tr = st.effective_tab_bar_region();
            st.win.dirty_tracker.mark_region(
                tr.x,
                tr.y,
                tr.width,
                tr.height,
                crate::dirty_rect::DirtyRegionType::TabBar,
            );
        }
        if tree_changed || settings_changed {
            let sr = layout.sidebar_region();
            st.win.dirty_tracker.mark_region(
                sr.x,
                sr.y,
                sr.width,
                sr.height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
        }
        if ai_changed {
            // AI 面板悬停状态变化（智能体模式中间列 / 经典模式右面板）
            st.mark_ai_panel_dirty();
        }
        if welcome_changed {
            st.win.dirty_tracker.mark_full_window();
        }
        if status_bar_changed {
            let sb = layout.status_bar_region();
            st.win.dirty_tracker.mark_region(
                sb.x,
                sb.y,
                sb.width,
                sb.height,
                crate::dirty_rect::DirtyRegionType::StatusBar,
            );
        }
        if tooltip_changed {
            // tooltip 显示/隐藏需要全窗口重绘（位置不固定）
            st.win.dirty_tracker.mark_full_window();
        }
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// 早期返回：对话框悬停 + 长按取消 + 自定义模式拖拽。
unsafe fn omm_early_returns(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    is_dragging: bool,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();
    // 菜单栏下拉菜单展开时：拦截所有鼠标移动，防止 hover 穿透到被覆盖的
    // 文件树/编辑器等下层组件。仅更新子菜单项的 hover 高亮和菜单栏横向切换。
    if st.ui.menu_bar.active_index.is_some() && !st.ui.menu_bar.customize_mode {
        let titlebar_region = layout.title_bar_region();
        let old_menu_hover = st.ui.menu_bar.hover_index;
        let old_submenu_hover = st.ui.menu_bar.submenu_hover;
        // 菜单栏横向 hover 切换展开项
        if titlebar_region.contains(mouse_x, mouse_y) {
            let btn_width = 40.0;
            let minimize_x = titlebar_region.x + titlebar_region.width - btn_width * 3.0;
            if mouse_x < minimize_x {
                st.ui.menu_bar.hover_index = st.ui.menu_bar.hit_test(
                    mouse_x,
                    mouse_y - titlebar_region.y,
                    titlebar_region.height,
                );
            } else {
                st.ui.menu_bar.hover_index = None;
            }
        } else {
            st.ui.menu_bar.hover_index = None;
        }
        // 横向 hover 切换展开项
        if let Some(h) = st.ui.menu_bar.hover_index {
            if Some(h) != st.ui.menu_bar.active_index && h < st.ui.menu_bar.items.len() {
                st.ui.menu_bar.expand(h);
                // 旧子菜单区域需一并重绘清除，脏矩形增量渲染下否则残影叠加
                st.win.dirty_tracker.mark_full_window();
            }
        }
        // 子菜单项 hover 追踪
        let cur_active = st.ui.menu_bar.active_index.unwrap_or(0);
        st.ui.menu_bar.submenu_hover =
            st.ui
                .menu_bar
                .item_x_positions
                .get(cur_active)
                .and_then(|&sx| {
                    let sy = titlebar_region.y + titlebar_region.height;
                    st.ui
                        .menu_bar
                        .hit_test_submenu(cur_active, mouse_x, mouse_y, sx, sy)
                });
        let changed = old_menu_hover != st.ui.menu_bar.hover_index
            || old_submenu_hover != st.ui.menu_bar.submenu_hover;
        if changed {
            st.win.dirty_tracker.mark_region(
                titlebar_region.x,
                titlebar_region.y,
                titlebar_region.width,
                titlebar_region.height,
                crate::dirty_rect::DirtyRegionType::TitleBar,
            );
            // 子菜单区域也需要标记脏（覆盖在侧边栏/编辑器上方）
            if let Some(active_idx) = st.ui.menu_bar.active_index {
                if let Some(&sx) = st.ui.menu_bar.item_x_positions.get(active_idx) {
                    let sy = titlebar_region.y + titlebar_region.height;
                    let item = &st.ui.menu_bar.items[active_idx];
                    let sw = if item.submenu_width > 0.0 {
                        item.submenu_width
                    } else {
                        200.0
                    };
                    let sh = item
                        .items
                        .iter()
                        .map(|mi| if mi.label == "-" { 8.0 } else { 26.0 })
                        .sum::<f32>()
                        + 16.0; // 8px top + 8px bottom padding
                    st.win.dirty_tracker.mark_region(
                        sx,
                        sy,
                        sw,
                        sh,
                        crate::dirty_rect::DirtyRegionType::Dialog,
                    );
                }
            }
        }
        drop(st);
        if changed {
            invalidate_window(hwnd);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        }
        return Some(LRESULT(0));
    }
    // 资源管理器空白区域上下文菜单：更新 hover 状态
    if st.ui.context_menus.explorer.is_open {
        let changed = st.ui.context_menus.explorer.update_hover(mouse_x, mouse_y);
        if changed {
            // 标记菜单区域为脏，否则脏矩形渲染器会因“无脏区”跳过整帧重绘 → hover 卡顿
            let mx = st.ui.context_menus.explorer.origin_x;
            let my = st.ui.context_menus.explorer.origin_y;
            let mw = st.ui.context_menus.explorer.menu_width();
            let mh = st.ui.context_menus.explorer.menu_height();
            st.win.dirty_tracker.mark_region(
                mx,
                my,
                mw,
                mh,
                crate::dirty_rect::DirtyRegionType::Dialog,
            );
        }
        drop(st);
        if changed {
            invalidate_window(hwnd);
            // 强制即时重绘：WM_PAINT 优先级低于 WM_MOUSEMOVE，快速移动鼠标时 WM_PAINT
            // 会被洪流饿死——UpdateWindow 绕过队列优先级直接派发 WM_PAINT，消除菜单卡顿。
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        }
        return Some(LRESULT(0));
    }
    // 文件节点右键上下文菜单：更新 hover 状态
    if st.ui.context_menus.file_node.is_open {
        let changed = st.ui.context_menus.file_node.update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.ui.context_menus.file_node.origin_x;
            let my = st.ui.context_menus.file_node.origin_y;
            let mw = st.ui.context_menus.file_node.menu_width();
            let mh = st.ui.context_menus.file_node.menu_height();
            st.win.dirty_tracker.mark_region(
                mx,
                my,
                mw,
                mh,
                crate::dirty_rect::DirtyRegionType::Dialog,
            );
        }
        drop(st);
        if changed {
            invalidate_window(hwnd);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        }
        return Some(LRESULT(0));
    }
    // 标签右键上下文菜单：更新 hover 状态
    if st.ui.context_menus.tab.visible {
        let changed = st.ui.context_menus.tab.update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.ui.context_menus.tab.x;
            let my = st.ui.context_menus.tab.y;
            let mw = st.ui.context_menus.tab.width;
            let mh = st.ui.context_menus.tab.menu_height();
            st.win.dirty_tracker.mark_region(
                mx,
                my,
                mw,
                mh,
                crate::dirty_rect::DirtyRegionType::Dialog,
            );
        }
        drop(st);
        if changed {
            invalidate_window(hwnd);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        }
        return Some(LRESULT(0));
    }
    // 活动栏右键上下文菜单：更新 hover 状态
    if st.ui.context_menus.activity_bar.visible {
        let changed = st
            .ui
            .context_menus
            .activity_bar
            .update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.ui.context_menus.activity_bar.x;
            let my = st.ui.context_menus.activity_bar.y;
            let mw = st.ui.context_menus.activity_bar.width;
            let mh = st.ui.context_menus.activity_bar.menu_height();
            st.win.dirty_tracker.mark_region(
                mx,
                my,
                mw,
                mh,
                crate::dirty_rect::DirtyRegionType::Dialog,
            );
        }
        drop(st);
        if changed {
            invalidate_window(hwnd);
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
        }
        return Some(LRESULT(0));
    }
    // 对话框悬停处理
    if st.remote.ssh_dialog.visible {
        st.handle_ssh_dialog_hover(mouse_x, mouse_y);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if st.remote.clone_dialog.visible {
        st.handle_clone_dialog_hover(mouse_x, mouse_y);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // 长按检测：移动超过容差则取消
    if is_dragging && st.input.mouse_press.lpress_target.is_some() {
        let dx = mouse_x - st.input.mouse_press.lpress_x;
        let dy = mouse_y - st.input.mouse_press.lpress_y;
        if dx.abs() > LP_MOVE_TOLERANCE || dy.abs() > LP_MOVE_TOLERANCE {
            let _ = KillTimer(hwnd, LP_TIMER_ID);
            st.input.mouse_press.lpress_target = None;
            st.input.mouse_press.lpress_start = None;
        }
    }
    // 自定义模式下：跟随鼠标更新放置目标
    let activity_dragging =
        st.ui.activity_bar.customize_mode && st.ui.activity_bar.drag_index.is_some();
    let menu_dragging = st.ui.menu_bar.customize_mode && st.ui.menu_bar.drag_index.is_some();
    if is_dragging && activity_dragging {
        let bar_y = layout.activity_bar_region().y;
        st.ui.activity_bar.drop_index = Some(st.ui.activity_bar.drop_index_at(mouse_y, bar_y));
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if is_dragging && menu_dragging {
        st.ui.menu_bar.drop_index = Some(st.ui.menu_bar.drop_index_at(mouse_x));
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // Task 8.3: 标签拖拽——检测阈值进入拖拽模式，或更新 drop_index
    if is_dragging && st.editor.tab_bar.tab_drag_start.is_some() {
        if st.editor.tab_bar.dragging_tab.is_none() {
            // 判定是否超过 3px 阈值（dx*dx + dy*dy > 9）
            let (sx, sy) = st.editor.tab_bar.tab_drag_start.unwrap();
            let dx = mouse_x - sx as f32;
            let dy = mouse_y - sy as f32;
            if dx * dx + dy * dy > 9.0 {
                // 进入拖拽模式：使用当前 hover_tab 作为拖拽目标
                if let Some(hover) = st.editor.tab_bar.hover_tab {
                    st.editor.tab_bar.dragging_tab = Some(hover);
                    st.editor.tab_bar.tab_drop_index = Some(hover);
                    invalidate_window(hwnd);
                    return Some(LRESULT(0));
                }
            }
        } else {
            // 已在拖拽模式：更新 drop_index
            let show_tab_bar = st.show_tab_bar();
            let editor_content = layout.editor_content_region(show_tab_bar);
            let new_drop = st.tab_drop_index_at(mouse_x, editor_content.x);
            let changed = st.editor.tab_bar.tab_drop_index != Some(new_drop);
            st.editor.tab_bar.tab_drop_index = Some(new_drop);
            drop(st);
            if changed {
                invalidate_window(hwnd);
            }
            return Some(LRESULT(0));
        }
    }
    None
}

/// 标题栏 + 菜单栏悬停更新。返回是否有变化。
unsafe fn omm_titlebar_menu_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    let titlebar_region = layout.title_bar_region();
    // 标题栏按钮悬停（与 render_title_bar 布局保持一致）
    let old_titlebar_hover = st.win.titlebar_hover_button;
    if titlebar_region.contains(mouse_x, mouse_y) {
        // 与 render_title_bar 共用单一布局源，消除手写副本间的尺寸漂移
        let tb = crate::layout::TitlebarButtons::compute(titlebar_region.x, titlebar_region.width);
        let close_x = tb.close_x;
        let maximize_x = tb.maximize_x;
        let minimize_x = tb.minimize_x;
        let user_btn_x = tb.user_btn_x;
        let settings_btn_x = tb.settings_btn_x;
        let right_panel_btn_x = tb.right_panel_btn_x;
        let bottom_panel_btn_x = tb.bottom_panel_btn_x;
        let left_sidebar_btn_x = tb.left_sidebar_btn_x;
        let mode_btn_x = tb.mode_btn_x;
        let mode_btn_width = tb.mode_btn_width;

        // 右侧工具按钮组悬停检测（从右往左）
        if mouse_x >= minimize_x {
            if mouse_x >= close_x {
                st.win.titlebar_hover_button = Some(2);
            } else if mouse_x >= maximize_x {
                st.win.titlebar_hover_button = Some(1);
            } else {
                st.win.titlebar_hover_button = Some(0);
            }
        } else if mouse_x >= user_btn_x {
            st.win.titlebar_hover_button = Some(3);
        } else if mouse_x >= settings_btn_x {
            st.win.titlebar_hover_button = Some(4);
        } else if mouse_x >= right_panel_btn_x {
            st.win.titlebar_hover_button = Some(5);
        } else if mouse_x >= bottom_panel_btn_x {
            st.win.titlebar_hover_button = Some(6);
        } else if mouse_x >= left_sidebar_btn_x {
            st.win.titlebar_hover_button = Some(7);
        } else if mouse_x >= mode_btn_x && mouse_x < mode_btn_x + mode_btn_width {
            st.win.titlebar_hover_button = Some(10);
        } else {
            // 左侧箭头按钮（位置动态计算，从渲染帧缓存读取）
            let back_x = st.win.titlebar_back_btn_x;
            let fwd_x = st.win.titlebar_forward_btn_x;
            let btn_size = tb.tool_btn_size;
            if mouse_x >= back_x && mouse_x < back_x + btn_size {
                st.win.titlebar_hover_button = Some(9);
            } else if mouse_x >= fwd_x && mouse_x < fwd_x + btn_size {
                st.win.titlebar_hover_button = Some(8);
            } else {
                st.win.titlebar_hover_button = None;
            }
        }
    } else {
        st.win.titlebar_hover_button = None;
    }
    let titlebar_changed = old_titlebar_hover != st.win.titlebar_hover_button;
    // 菜单栏悬停
    let old_menu_hover = st.ui.menu_bar.hover_index;
    if titlebar_region.contains(mouse_x, mouse_y) {
        let btn_width = 40.0;
        let minimize_x = titlebar_region.x + titlebar_region.width - btn_width * 3.0;
        if mouse_x < minimize_x {
            st.ui.menu_bar.hover_index = st.ui.menu_bar.hit_test(
                mouse_x,
                mouse_y - titlebar_region.y,
                titlebar_region.height,
            );
        } else {
            st.ui.menu_bar.hover_index = None;
        }
    } else {
        st.ui.menu_bar.hover_index = None;
    }
    // 菜单展开状态下：横向移动悬停即切换展开项（符合常规菜单栏行为），
    // 并追踪子菜单项悬停以驱动高亮反馈
    let old_submenu_hover = st.ui.menu_bar.submenu_hover;
    if let Some(active_idx) = st.ui.menu_bar.active_index {
        if !st.ui.menu_bar.customize_mode {
            if let Some(h) = st.ui.menu_bar.hover_index {
                if h != active_idx && h < st.ui.menu_bar.items.len() {
                    st.ui.menu_bar.expand(h);
                    // 旧子菜单区域需一并重绘清除，脏矩形增量渲染下否则残影叠加
                    st.win.dirty_tracker.mark_full_window();
                }
            }
        }
        let cur_active = st.ui.menu_bar.active_index.unwrap_or(active_idx);
        st.ui.menu_bar.submenu_hover =
            st.ui
                .menu_bar
                .item_x_positions
                .get(cur_active)
                .and_then(|&sx| {
                    let sy = titlebar_region.y + titlebar_region.height;
                    st.ui
                        .menu_bar
                        .hit_test_submenu(cur_active, mouse_x, mouse_y, sx, sy)
                });
    } else {
        st.ui.menu_bar.submenu_hover = None;
    }
    titlebar_changed
        || old_menu_hover != st.ui.menu_bar.hover_index
        || old_submenu_hover != st.ui.menu_bar.submenu_hover
}

/// 活动栏 + 标签栏悬停更新。返回 (是否有变化, editor_content)。
unsafe fn omm_activity_tab_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> (bool, crate::layout::Region) {
    let mut st = state.borrow_mut();
    // 活动栏悬停
    let activity_region = layout.activity_bar_region();
    let old_activity_hover = st.ui.activity_bar.hover_index;
    st.ui.activity_bar.hover_index =
        st.ui
            .activity_bar
            .hit_test(mouse_x, mouse_y, activity_region.y);
    let activity_changed = old_activity_hover != st.ui.activity_bar.hover_index;
    // 标签栏悬停（智能体模式下标签栏在右侧面板顶部，使用实际区域）
    let editor_content = layout.editor_content_region(st.show_tab_bar());
    let tab_region = st.effective_tab_bar_region();
    let old_hover = st.editor.tab_bar.hover_tab;
    st.update_hover_tab(mouse_x, mouse_y, tab_region.x, tab_region.y);
    let tab_changed = old_hover != st.editor.tab_bar.hover_tab;
    (activity_changed || tab_changed, editor_content)
}

/// 文件树 / SSH 管理面板 / 源代码管理面板悬停更新。返回是否有变化。
unsafe fn omm_file_tree_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    let sidebar_region = layout.sidebar_region();
    let _old_tree_hover = st.fs.hover_file_node;
    if sidebar_region.contains(mouse_x, mouse_y) {
        if st.ui.sidebar_content == crate::layout::SidebarContent::RemoteManagerPanel {
            // SSH 管理面板悬停检测
            let old_hover = st.remote.ssh_manager_panel.hover;
            let old_action = st.remote.ssh_manager_panel.hover_action;
            let mut new_hover_action = None;
            let btn_rects = st.remote.ssh_manager_panel.item_btn_rects.clone();
            for &(idx, action, ref rect) in &btn_rects {
                if rect.contains(mouse_x, mouse_y) {
                    new_hover_action = Some((idx, action));
                    break;
                }
            }
            st.remote.ssh_manager_panel.hover_action = new_hover_action;
            if new_hover_action.is_none() {
                if let Some(ref rect) = st.remote.ssh_manager_panel.add_btn_rect {
                    if rect.contains(mouse_x, mouse_y) {
                        st.remote.ssh_manager_panel.hover_action = Some((997, 0));
                    }
                }
            }
            if new_hover_action.is_none() && st.remote.ssh_manager_panel.editing {
                if let Some(ref rect) = st.remote.ssh_manager_panel.save_btn_rect {
                    if rect.contains(mouse_x, mouse_y) {
                        st.remote.ssh_manager_panel.hover_action = Some((998, 0));
                    }
                }
                if st.remote.ssh_manager_panel.hover_action.is_none() {
                    if let Some(ref rect) = st.remote.ssh_manager_panel.cancel_btn_rect {
                        if rect.contains(mouse_x, mouse_y) {
                            st.remote.ssh_manager_panel.hover_action = Some((998, 1));
                        }
                    }
                }
            }
            st.remote.ssh_manager_panel.hover = None;
            old_hover != st.remote.ssh_manager_panel.hover
                || old_action != st.remote.ssh_manager_panel.hover_action
        } else if st.ui.sidebar_content == crate::layout::SidebarContent::SourceControlPanel {
            let old_hover = st.ui.git.hover_button.clone();
            st.update_git_panel_hover(mouse_x - sidebar_region.x, mouse_y - sidebar_region.y);
            old_hover != st.ui.git.hover_button
        } else {
            // 智能体模式下，文件树区域被下移（上面有新会话按钮和对话标签页），
            // 使用渲染时记录的偏移量调整坐标
            let offset_y = if st.editor_mode.is_agent() {
                st.ai.ai_panel.agent_file_tree_offset_y
            } else {
                0.0
            };
            st.update_file_tree_hover(
                mouse_x - sidebar_region.x,
                mouse_y - sidebar_region.y - offset_y,
            )
        }
    } else {
        let old = st.fs.hover_file_node.take();
        let old_root = std::mem::take(&mut st.fs.hover_file_tree_root);
        old.is_some() || old_root
    }
}

/// 设置面板悬停更新。返回是否有变化。
unsafe fn omm_settings_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    is_dragging: bool,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    let sidebar_region = layout.sidebar_region();
    if sidebar_region.contains(mouse_x, mouse_y)
        && st.ui.sidebar_content == crate::layout::SidebarContent::RemoteManagerPanel
    {
        // SSH 管理面板已在上面处理悬停
        false
    } else {
        let mut changed = false;
        if st.ui.settings_panel.hover_tab.is_some() {
            st.ui.settings_panel.hover_tab = None;
            changed = true;
        }
        // 模型管理页悬停检测（仅列表视图）
        if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Models
            && !st.ui.settings_panel.model_editing
        {
            // 命中区以绝对坐标注册；门槛用设置页实际渲染区域（经典=编辑区，智能体=右面板内容区）
            let settings_region = st.settings_page_region(layout);
            if settings_region.contains(mouse_x, mouse_y) {
                // 用绝对 mouse_x/mouse_y 命中测试，与标签栏 / 编辑表单保持一致
                // 检测模型项悬停
                let new_hover_id = st.ui.settings_panel.hit_test_model_item(mouse_x, mouse_y);
                if st.ui.settings_panel.hover_model_id != new_hover_id {
                    st.ui.settings_panel.hover_model_id = new_hover_id.clone();
                    changed = true;
                }
                // 检测模型按钮悬停
                let new_hover_btn = st.ui.settings_panel.hit_test_model_button(mouse_x, mouse_y);
                let (new_btn, new_btn_id) = match new_hover_btn {
                    Some((btn, id)) => (Some(btn), Some(id)),
                    None => (None, None),
                };
                if st.ui.settings_panel.hover_model_button != new_btn {
                    st.ui.settings_panel.hover_model_button = new_btn;
                    changed = true;
                }
                if st.ui.settings_panel.hover_model_button_id != new_btn_id {
                    st.ui.settings_panel.hover_model_button_id = new_btn_id;
                    changed = true;
                }
            } else {
                if st.ui.settings_panel.hover_model_id.is_some() {
                    st.ui.settings_panel.hover_model_id = None;
                    changed = true;
                }
                if st.ui.settings_panel.hover_model_button.is_some() {
                    st.ui.settings_panel.hover_model_button = None;
                    changed = true;
                }
                if st.ui.settings_panel.hover_model_button_id.is_some() {
                    st.ui.settings_panel.hover_model_button_id = None;
                    changed = true;
                }
            }
        }
        // 模型编辑表单（AI 配置）：API 密钥显隐按钮悬停
        if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Ai
            || (st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Models
                && st.ui.settings_panel.model_editing)
        {
            let new_eye_hover = st
                .ui
                .settings_panel
                .hit_test_api_key_toggle(mouse_x, mouse_y);
            if st.ui.settings_panel.hover_api_key_toggle != new_eye_hover {
                st.ui.settings_panel.hover_api_key_toggle = new_eye_hover;
                changed = true;
            }
        }
        // 温度滑块拖拽：拖拽中根据鼠标 x 实时更新温度
        if st.ui.settings_panel.temp_slider_dragging {
            if is_dragging {
                if st.ui.settings_panel.set_temperature_from_slider_x(mouse_x) {
                    changed = true;
                }
            } else {
                st.ui.settings_panel.temp_slider_dragging = false;
            }
        }
        // Top-p 滑块拖拽：拖拽中根据鼠标 x 实时更新 top_p
        if st.ui.settings_panel.top_p_slider_dragging {
            if is_dragging {
                if st.ui.settings_panel.set_top_p_from_slider_x(mouse_x) {
                    changed = true;
                }
            } else {
                st.ui.settings_panel.top_p_slider_dragging = false;
            }
        }
        // 频率惩罚滑块拖拽
        if st.ui.settings_panel.freq_slider_dragging {
            if is_dragging {
                if st.ui.settings_panel.set_freq_from_slider_x(mouse_x) {
                    changed = true;
                }
            } else {
                st.ui.settings_panel.freq_slider_dragging = false;
            }
        }
        // 存在惩罚滑块拖拽
        if st.ui.settings_panel.pres_slider_dragging {
            if is_dragging {
                if st.ui.settings_panel.set_pres_from_slider_x(mouse_x) {
                    changed = true;
                }
            } else {
                st.ui.settings_panel.pres_slider_dragging = false;
            }
        }
        // 思考强度分段悬停态
        let new_effort_hover = st.ui.settings_panel.hit_test_effort(mouse_x, mouse_y);
        if st.ui.settings_panel.hover_effort != new_effort_hover {
            st.ui.settings_panel.hover_effort = new_effort_hover;
            changed = true;
        }
        // 响应格式分段悬停态
        let new_fmt_hover = st
            .ui
            .settings_panel
            .hit_test_response_format(mouse_x, mouse_y);
        if st.ui.settings_panel.hover_response_format != new_fmt_hover {
            st.ui.settings_panel.hover_response_format = new_fmt_hover;
            changed = true;
        }
        changed
    }
}

/// AI 面板悬停更新。返回是否有变化。
unsafe fn omm_ai_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    // 历史浮窗打开时：hover 基于浮窗实际区域（浮窗可拖出/居中于整个窗口，
    // 不受右面板区域限制），直接用浮窗条目命中区更新 hover_tab。
    if st.ai.ai_panel.history_open {
        let in_win = st
            .ai
            .ai_panel
            .history_win_region
            .map(|(px, py, pw, ph)| {
                mouse_x >= px && mouse_x < px + pw && mouse_y >= py && mouse_y < py + ph
            })
            .unwrap_or(false);
        let old_tab_hover = st.ai.ai_panel.hover_tab;
        st.ai.ai_panel.hover_tab = if in_win {
            st.ai
                .ai_panel
                .history_item_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        } else {
            None
        };
        // 浮窗打开期间不处理 Apply 按钮 hover（浮窗覆盖于面板之上）
        let old_apply_hover = st.ai.ai_panel.hover_apply_button;
        st.ai.ai_panel.hover_apply_button = false;
        return old_tab_hover != st.ai.ai_panel.hover_tab
            || old_apply_hover != st.ai.ai_panel.hover_apply_button;
    }

    // 智能体模式：处理左侧边栏的对话标签页悬浮
    if st.editor_mode.is_agent() {
        let sidebar_region = layout.sidebar_region();
        if sidebar_region.contains(mouse_x, mouse_y) {
            // 检查对话标签页悬浮
            let old_tab_hover = st.ai.ai_panel.hover_tab;
            st.ai.ai_panel.hover_tab = st
                .ai
                .ai_panel
                .agent_tab_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i);
            return old_tab_hover != st.ai.ai_panel.hover_tab;
        } else {
            // 鼠标不在侧边栏，清除悬浮状态
            let old_tab_hover = st.ai.ai_panel.hover_tab;
            st.ai.ai_panel.hover_tab = None;
            return old_tab_hover.is_some();
        }
    }

    // 开发者模式：右侧面板是 AI 面板
    let right_panel_region = layout.right_panel_region();
    if layout.right_panel_visible && right_panel_region.contains(mouse_x, mouse_y) {
        // Apply 按钮悬停
        let rel_x = mouse_x - right_panel_region.x;
        let rel_y = mouse_y - right_panel_region.y;
        let margin = 10.0;
        let apply_y = right_panel_region.height - 76.0;
        let apply_btn_w = 80.0;
        let apply_btn_h = 24.0;
        let apply_btn_x = right_panel_region.width - margin - apply_btn_w;
        let old_apply_hover = st.ai.ai_panel.hover_apply_button;
        st.ai.ai_panel.hover_apply_button = rel_x >= apply_btn_x
            && rel_x < apply_btn_x + apply_btn_w
            && rel_y >= apply_y
            && rel_y < apply_y + apply_btn_h;
        // 历史条目 / 会话标签悬停（命中区为绝对坐标）
        let old_tab_hover = st.ai.ai_panel.hover_tab;
        st.ai.ai_panel.hover_tab = if st.ai.ai_panel.history_open {
            st.ai
                .ai_panel
                .history_item_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        } else {
            st.ai
                .ai_panel
                .tab_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        };
        old_apply_hover != st.ai.ai_panel.hover_apply_button
            || old_tab_hover != st.ai.ai_panel.hover_tab
    } else {
        let old = st.ai.ai_panel.hover_apply_button;
        let old_tab = st.ai.ai_panel.hover_tab;
        st.ai.ai_panel.hover_apply_button = false;
        st.ai.ai_panel.hover_tab = None;
        old || old_tab.is_some()
    }
}

/// 欢迎页悬停更新。返回是否有变化。
unsafe fn omm_welcome_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    let old_welcome_hover = st.ui.welcome_hover_action.clone();
    if st.show_welcome() {
        let welcome_x = 0.0;
        let welcome_y = layout.top_offset();
        let welcome_width = st.win.window_width as f32;
        let welcome_height = st.win.window_height as f32
            - welcome_y
            - if layout.status_bar_visible {
                layout.status_bar_height
            } else {
                0.0
            };
        st.ui.welcome_hover_action = st.hit_test_welcome_action(
            mouse_x,
            mouse_y,
            welcome_x,
            welcome_y,
            welcome_width,
            welcome_height,
        );
    } else {
        st.ui.welcome_hover_action = None;
    }
    old_welcome_hover != st.ui.welcome_hover_action
}

/// SubTask 10.1: 状态栏分区悬停更新。返回是否有变化。
///
/// 当鼠标位于状态栏区域内时，调用 `hit_test` 检测命中的分区：
/// - 若命中且分区 `clickable` 为 true，设置 `hover_index = Some(idx)`
/// - 否则 `hover_index = None`
///
/// `hover_index` 变化时返回 true，触发 `invalidate_window` 重绘以显示 hover 高亮。
unsafe fn omm_status_bar_hover(
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> bool {
    let mut st = state.borrow_mut();
    let status_region = layout.status_bar_region();
    let old_hover = st.ui.status_bar.hover_index;
    let new_hover = if layout.status_bar_visible && status_region.contains(mouse_x, mouse_y) {
        let rel_x = mouse_x - status_region.x;
        let rel_y = mouse_y - status_region.y;
        match st.ui.status_bar.hit_test(rel_x, rel_y, status_region.width) {
            Some(idx) => {
                if st
                    .ui
                    .status_bar
                    .sections
                    .get(idx)
                    .is_some_and(|sec| sec.clickable)
                {
                    Some(idx)
                } else {
                    None
                }
            }
            None => None,
        }
    } else {
        None
    };
    st.ui.status_bar.hover_index = new_hover;
    old_hover != st.ui.status_bar.hover_index
}

/// 拖拽光标设置 + 面板拖拽调整。返回 Some 表示已处理（需提前返回）。
unsafe fn omm_resize_drag(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    is_dragging: bool,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();
    let editor_region = layout.editor_region();
    // 拐角手柄 hover（优先于单线）：左下 = 侧边栏×底部面板，右下 = 右面板×底部面板
    let corner_left_hit = layout
        .corner_left_handle()
        .map(|r| r.contains(mouse_x, mouse_y))
        .unwrap_or(false);
    let corner_right_hit = layout
        .corner_right_handle()
        .map(|r| r.contains(mouse_x, mouse_y))
        .unwrap_or(false);
    let right_panel_resize_zone = layout.right_panel_visible
        && (mouse_x >= editor_region.right() - 4.0 && mouse_x <= editor_region.right() + 4.0)
        && mouse_y >= editor_region.y
        && mouse_y < editor_region.y + editor_region.height;
    let bottom_region = layout.bottom_panel_region();
    let bottom_panel_resize_zone = layout.bottom_panel_visible
        && (mouse_y >= bottom_region.y - 4.0 && mouse_y <= bottom_region.y + 4.0)
        && mouse_x >= bottom_region.x
        && mouse_x < bottom_region.x + bottom_region.width;
    // 侧边栏右侧调整区域（显示时）或活动栏右缘拖回把手（隐藏时）
    let sidebar_region = layout.sidebar_region();
    let sidebar_resize_zone = if layout.sidebar_visible {
        // 正常状态：侧边栏右边缘 ±4px
        (mouse_x >= sidebar_region.right() - 4.0 && mouse_x <= sidebar_region.right() + 4.0)
            && mouse_y >= sidebar_region.y
            && mouse_y < sidebar_region.y + sidebar_region.height
    } else {
        // 收起状态：活动栏右缘保留 4px 拖回把手（VS Code 行为）
        let edge = if layout.activity_bar_visible {
            layout.activity_bar_width
        } else {
            0.0
        };
        (mouse_x >= edge - 2.0 && mouse_x <= edge + 4.0)
            && mouse_y >= sidebar_region.y
            && mouse_y < sidebar_region.y + sidebar_region.height
    };
    // 更新 hover 状态
    st.ui.hover_sidebar_resize = sidebar_resize_zone;
    // 设置拖拽光标（拐角优先，斜向光标）
    if corner_left_hit || st.ui.layout.corner_left_resizing {
        let hcursor = LoadCursorW(None, IDC_SIZENWSE).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if corner_right_hit || st.ui.layout.corner_right_resizing {
        let hcursor = LoadCursorW(None, IDC_SIZENESW).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if right_panel_resize_zone
        || st.ui.layout.right_panel_resizing
        || sidebar_resize_zone
        || st.ui.layout.sidebar_resizing
    {
        let hcursor = LoadCursorW(None, IDC_SIZEWE).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if bottom_panel_resize_zone || st.ui.layout.bottom_panel_resizing {
        let hcursor = LoadCursorW(None, IDC_SIZENS).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if st.ui.welcome_hover_action.is_some() {
        let hcursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
        let _ = SetCursor(hcursor);
    }
    // 处理拖拽调整（拐角优先：同时调整两条分割线）
    if is_dragging {
        if st.ui.layout.corner_left_resizing {
            // 左下拐角：水平调侧边栏宽度（绝对值，与单线一致）+ 垂直调底部面板高度（增量）
            let sidebar_left = if st.ui.layout.activity_bar_visible {
                st.ui.layout.activity_bar_width
            } else {
                0.0
            };
            st.ui
                .layout
                .set_sidebar_width_or_collapse(mouse_x - sidebar_left);
            let delta_y = mouse_y - bottom_region.y;
            st.ui.layout.resize_bottom_panel(-delta_y);
            drop(st);
            invalidate_window(hwnd);
            // 拖拽中 WM_PAINT 被消息洪流饿死，节流 UpdateWindow 立即重绘
            if panel_drag_should_sync_paint() {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return Some(LRESULT(0));
        } else if st.ui.layout.corner_right_resizing {
            // 右下拐角：水平调右面板宽度（增量）+ 垂直调底部面板高度（增量）
            let delta_x = mouse_x - editor_region.right();
            st.ui.layout.resize_right_panel(-delta_x);
            let delta_y = mouse_y - bottom_region.y;
            st.ui.layout.resize_bottom_panel(-delta_y);
            drop(st);
            invalidate_window(hwnd);
            if panel_drag_should_sync_paint() {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return Some(LRESULT(0));
        } else if st.ui.layout.right_panel_resizing {
            let delta = mouse_x - editor_region.right();
            st.ui.layout.resize_right_panel(-delta);
            drop(st);
            invalidate_window(hwnd);
            if panel_drag_should_sync_paint() {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return Some(LRESULT(0));
        } else if st.ui.layout.sidebar_resizing {
            // 期望宽度 = 鼠标相对侧边栏左缘；不用 region.right() 做增量，
            // 因为收起后 region 宽度归零，增量式无法支持"拖回恢复"
            let sidebar_left = if st.ui.layout.activity_bar_visible {
                st.ui.layout.activity_bar_width
            } else {
                0.0
            };
            st.ui
                .layout
                .set_sidebar_width_or_collapse(mouse_x - sidebar_left);
            drop(st);
            invalidate_window(hwnd);
            if panel_drag_should_sync_paint() {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return Some(LRESULT(0));
        } else if st.ui.layout.bottom_panel_resizing {
            let delta = mouse_y - bottom_region.y;
            st.ui.layout.resize_bottom_panel(-delta);
            drop(st);
            invalidate_window(hwnd);
            if panel_drag_should_sync_paint() {
                let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
            }
            return Some(LRESULT(0));
        }
    }
    None
}

/// P3.4: Hover tooltip 防抖逻辑。
unsafe fn omm_hover_tooltip(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) {
    let mut st = state.borrow_mut();
    let sidebar_region = layout.sidebar_region();
    let in_sidebar = sidebar_region.contains(mouse_x, mouse_y)
        && matches!(
            st.ui.sidebar_content,
            crate::layout::SidebarContent::FileTree | crate::layout::SidebarContent::RemoteFileTree
        );
    let has_hover_node = st.fs.hover_file_node.is_some() || st.remote.hover_node.is_some();
    let dx = mouse_x - st.input.hover.last_mouse_x;
    let dy = mouse_y - st.input.hover.last_mouse_y;
    let moved_beyond_tolerance = dx.abs() > HOVER_MOVE_TOLERANCE || dy.abs() > HOVER_MOVE_TOLERANCE;
    if (moved_beyond_tolerance || !in_sidebar || !has_hover_node)
        && st.input.hover.tooltip.is_some()
    {
        st.input.hover.tooltip = None;
    }
    if in_sidebar && has_hover_node {
        let _ = SetTimer(hwnd, HOVER_TIMER_ID, HOVER_DELAY_MS, None);
    } else {
        let _ = KillTimer(hwnd, HOVER_TIMER_ID);
    }
    st.input.hover.last_mouse_x = mouse_x;
    st.input.hover.last_mouse_y = mouse_y;
}

/// UI Tooltip 状态更新：500ms 延迟显示、4px 移动容差。
///
/// 返回 true 表示 tooltip 可见性发生变化，需要 invalidate。
///
/// 状态机：
/// 1. hover_key 变化（含进入/离开元素）：更新 hover_key/anchor/timer_start，清空 visible_text
/// 2. hover_key 相同且鼠标移动 > 4px：重置 anchor/timer_start，清空 visible_text
/// 3. hover_key 相同且静止 ≥ 500ms：设置 visible_text + show_pos
unsafe fn omm_tooltip_state(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> bool {
    use crate::tooltip::{TOOLTIP_DELAY_MS, TOOLTIP_MOVE_TOLERANCE};
    use windows::Win32::System::SystemInformation::GetTickCount64;

    let mut st = state.borrow_mut();
    let (new_key, tooltip_text) = st.compute_tooltip_hover_key();
    let key_changed = new_key != st.ui.tooltip_state.hover_key;

    // 分支 1：hover_key 变化
    if key_changed {
        let was_visible = st.ui.tooltip_state.visible_text.is_some();
        st.ui.tooltip_state.hover_key = new_key.clone();
        st.ui.tooltip_state.anchor = POINT {
            x: mouse_x as i32,
            y: mouse_y as i32,
        };
        st.ui.tooltip_state.timer_start = if new_key.is_some() {
            Some(GetTickCount64())
        } else {
            None
        };
        st.ui.tooltip_state.visible_text = None;
        // 启动/取消 tooltip 定时器
        if new_key.is_some() {
            let _ = SetTimer(
                hwnd,
                super::super::TOOLTIP_TIMER_ID,
                TOOLTIP_DELAY_MS as u32,
                None,
            );
        } else {
            let _ = KillTimer(hwnd, super::super::TOOLTIP_TIMER_ID);
        }
        // 离开元素或切换元素时，若之前有显示，需要重绘清除
        return was_visible;
    }

    // hover_key 相同且为 None：无需任何操作
    if new_key.is_none() {
        return false;
    }

    // 分支 2：检查鼠标移动距离
    let dx = mouse_x - st.ui.tooltip_state.anchor.x as f32;
    let dy = mouse_y - st.ui.tooltip_state.anchor.y as f32;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > TOOLTIP_MOVE_TOLERANCE {
        let was_visible = st.ui.tooltip_state.visible_text.is_some();
        st.ui.tooltip_state.anchor = POINT {
            x: mouse_x as i32,
            y: mouse_y as i32,
        };
        st.ui.tooltip_state.timer_start = Some(GetTickCount64());
        // 移动超限时重置定时器
        let _ = KillTimer(hwnd, super::super::TOOLTIP_TIMER_ID);
        let _ = SetTimer(
            hwnd,
            super::super::TOOLTIP_TIMER_ID,
            TOOLTIP_DELAY_MS as u32,
            None,
        );
        if was_visible {
            st.ui.tooltip_state.visible_text = None;
            return true;
        }
        return false;
    }

    // 分支 3：检查 timer_start 是否到达 500ms
    if let Some(start) = st.ui.tooltip_state.timer_start {
        if st.ui.tooltip_state.visible_text.is_none() {
            let now = GetTickCount64();
            if now - start >= TOOLTIP_DELAY_MS {
                if let Some(text) = tooltip_text {
                    st.ui.tooltip_state.visible_text = Some(text);
                    st.ui.tooltip_state.show_pos = (mouse_x, mouse_y);
                    return true;
                }
            }
        }
    }

    false
}

/// WM_SETCURSOR 调用：根据鼠标位置和当前 hover 状态返回 CursorType。
///
/// 输入 `x`/`y` 为客户端物理像素坐标（来自 `ScreenToClient`）。
/// 内部转换为逻辑像素后与布局区域比对。**只读访问状态**，不修改任何字段。
///
/// 检查顺序：
/// 1. 对话框/命令面板打开 → Arrow
/// 2. 欢迎页 hover 项 → Hand
/// 3. 标题栏按钮/菜单项 hover → Hand
/// 4. 活动栏 hover → Hand
/// 5. 面板拖拽中 → 固定 SizeWE/SizeNS
/// 6. 标签栏 hover → Hand
/// 7. 侧边栏分隔条 → SizeWE
/// 8. 右侧面板分隔条 → SizeWE
/// 9. 底部面板分隔条 → SizeNS
/// 10. 文件树内联输入框（新建/重命名时）→ IBeam
/// 11. AI 面板输入框 → IBeam
/// 12. 编辑器内容区：欢迎页/空占位页 → Arrow；设置页仅输入字段 → IBeam；其余 → IBeam
/// 13. 状态栏 clickable 分区 → Hand
/// 14. 默认 → Arrow
pub(crate) unsafe fn compute_cursor_for_pos(_hwnd: HWND, x: i32, y: i32) -> CursorType {
    EDITOR_STATE.with(|s| {
        let s = s.borrow();
        let Some(state) = s.as_ref() else {
            return CursorType::Arrow;
        };
        let st = state.borrow();

        // 转换为逻辑像素
        let mouse_x = x as f32 / st.win.dpi_scale;
        let mouse_y = y as f32 / st.win.dpi_scale;
        let layout = st.ui.layout.clone();

        // 1. 对话框/命令面板打开时返回默认箭头
        if st.remote.ssh_dialog.visible
            || st.remote.clone_dialog.visible
            || st.ui.command_palette.visible
        {
            return CursorType::Arrow;
        }

        // 1a. 菜单栏下拉菜单展开时：子菜单覆盖区域 → Arrow（不穿透到下层 UI）
        if st.ui.menu_bar.active_index.is_some() && !st.ui.menu_bar.customize_mode {
            let titlebar_region = layout.title_bar_region();
            // 标题栏内：菜单项 hover → Hand
            if titlebar_region.contains(mouse_x, mouse_y) {
                if st.win.titlebar_hover_button.is_some() || st.ui.menu_bar.hover_index.is_some() {
                    return CursorType::Hand;
                }
                return CursorType::Arrow;
            }
            // 子菜单覆盖区域 → Arrow（阻止穿透到文件树/编辑器等）
            if let Some(active_idx) = st.ui.menu_bar.active_index {
                if let Some(&sx) = st.ui.menu_bar.item_x_positions.get(active_idx) {
                    let sy = titlebar_region.y + titlebar_region.height;
                    let item = &st.ui.menu_bar.items[active_idx];
                    let sw = if item.submenu_width > 0.0 {
                        item.submenu_width
                    } else {
                        200.0
                    };
                    let sh: f32 = item
                        .items
                        .iter()
                        .map(|mi| if mi.label == "-" { 8.0 } else { 26.0 })
                        .sum::<f32>()
                        + 16.0;
                    if mouse_x >= sx && mouse_x < sx + sw && mouse_y >= sy && mouse_y < sy + sh {
                        return CursorType::Arrow;
                    }
                }
            }
        }

        // 1b. 历史浮窗：拖动中 → Grabbing；悬停标题栏 → Grab；悬停关闭按钮 → Hand
        if st.ai.ai_panel.history_open {
            // 拖动中（按住标题栏拖拽）→ 握紧手形
            if st.ai.ai_panel.history_win_drag.is_some() {
                return CursorType::Grabbing;
            }
            // 悬停标题栏（可拖动区域）→ 张开手形
            if let Some((tx, ty, tw, th)) = st.ai.ai_panel.history_win_titlebar_region {
                if mouse_x >= tx && mouse_x < tx + tw && mouse_y >= ty && mouse_y < ty + th {
                    return CursorType::Grab;
                }
            }
            // 悬停关闭按钮 → Hand
            if let Some((cx, cy, cw, ch)) = st.ai.ai_panel.history_win_close_region {
                if mouse_x >= cx && mouse_x < cx + cw && mouse_y >= cy && mouse_y < cy + ch {
                    return CursorType::Hand;
                }
            }
            // 悬停浮窗其他区域 → Arrow（浮窗覆盖下层 UI，不穿透）
            if let Some((wx, wy, ww, wh)) = st.ai.ai_panel.history_win_region {
                if mouse_x >= wx && mouse_x < wx + ww && mouse_y >= wy && mouse_y < wy + wh {
                    return CursorType::Arrow;
                }
            }
        }

        // 2. 欢迎页 hover 项 → Hand
        if st.ui.welcome_hover_action.is_some() {
            return CursorType::Hand;
        }

        // 3. 标题栏区域：按钮 hover 或菜单项 hover → Hand
        let titlebar_region = layout.title_bar_region();
        if titlebar_region.contains(mouse_x, mouse_y) {
            if st.win.titlebar_hover_button.is_some() || st.ui.menu_bar.hover_index.is_some() {
                return CursorType::Hand;
            }
            // 标题栏空白区（拖动区）→ Arrow
            return CursorType::Arrow;
        }

        // 4. 活动栏 hover → Hand
        let activity_region = layout.activity_bar_region();
        if activity_region.contains(mouse_x, mouse_y) && st.ui.activity_bar.hover_index.is_some() {
            return CursorType::Hand;
        }

        // 5. 面板拖拽中：固定 resize 光标（无论当前位置）
        if layout.corner_left_resizing {
            return CursorType::SizeNWSE;
        }
        if layout.corner_right_resizing {
            return CursorType::SizeNESW;
        }
        if layout.right_panel_resizing {
            return CursorType::SizeWE;
        }
        if layout.bottom_panel_resizing {
            return CursorType::SizeNS;
        }

        let editor_region = layout.editor_region();
        let editor_content = layout.editor_content_region(st.show_tab_bar());

        // 6. 标签栏 hover → Hand（智能体模式下标签栏在右侧面板顶部）
        let tab_bar_region = st.effective_tab_bar_region();
        if tab_bar_region.contains(mouse_x, mouse_y) && st.editor.tab_bar.hover_tab.is_some() {
            return CursorType::Hand;
        }

        // 6b. 拐角手柄 hover（优先于单线分隔条）→ 斜向光标
        if let Some(r) = layout.corner_left_handle() {
            if r.contains(mouse_x, mouse_y) {
                return CursorType::SizeNWSE;
            }
        }
        if let Some(r) = layout.corner_right_handle() {
            if r.contains(mouse_x, mouse_y) {
                return CursorType::SizeNESW;
            }
        }

        // 7. 侧边栏分隔条（sidebar 右边缘 4px 容差）
        if layout.sidebar_visible {
            let sidebar_right = layout.sidebar_region().right();
            if (mouse_x - sidebar_right).abs() <= 4.0
                && mouse_y >= editor_region.y
                && mouse_y < editor_region.y + editor_region.height
            {
                return CursorType::SizeWE;
            }
        }

        // 8. 右侧面板分隔条（right_panel 左边缘 4px 容差）
        if layout.right_panel_visible {
            let right_panel_left = layout.right_panel_region().x;
            if (mouse_x - right_panel_left).abs() <= 4.0
                && mouse_y >= editor_region.y
                && mouse_y < editor_region.y + editor_region.height
            {
                return CursorType::SizeWE;
            }
        }

        // 9. 底部面板分隔条（bottom_panel 顶部 4px 容差）
        if layout.bottom_panel_visible {
            let bottom_panel_top = layout.bottom_panel_region().y;
            if (mouse_y - bottom_panel_top).abs() <= 4.0
                && mouse_x >= editor_region.x
                && mouse_x < editor_region.x + editor_region.width
            {
                return CursorType::SizeNS;
            }
        }

        // 10. 文件树内联输入框（新建文件/文件夹/重命名时显示）→ IBeam
        // 几何与 file_tree_input_row_geom（树内联行）保持一致
        if layout.sidebar_visible
            && st.ui.sidebar_content == crate::layout::SidebarContent::FileTree
            && st.fs.file_tree_input.is_some()
        {
            let sidebar = layout.sidebar_region();
            let s = st.win.dpi_scale;
            if let Some((top_rel, _, text_left_rel)) = st.file_tree_input_row_geom() {
                let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * s;
                if mouse_x >= sidebar.x + text_left_rel - 3.0 * s
                    && mouse_x < sidebar.x + sidebar.width - 6.0 * s
                    && mouse_y >= sidebar.y + top_rel
                    && mouse_y < sidebar.y + top_rel + row_h
                {
                    return CursorType::IBeam;
                }
            }
        }

        // 11. AI 面板输入框 → IBeam
        // 几何与 lbd_right_panel_apply_input 的输入框命中检测保持一致
        // 智能体模式：AI 面板渲染在中间编辑器内容区；开发者模式：右侧面板
        // 智能体模式标签栏在右侧面板，中间区域不预留标签栏高度（与渲染一致）
        let ai_region = if st.editor_mode.is_agent() {
            Some(layout.editor_content_region(false))
        } else if layout.right_panel_visible {
            Some(layout.right_panel_region())
        } else {
            None
        };
        if let Some(rp) = ai_region {
            if rp.contains(mouse_x, mouse_y) {
                let rp_rel_x = mouse_x - rp.x;
                let rp_rel_y = mouse_y - rp.y;
                let margin = 10.0;
                let input_margin = 8.0;
                // 使用动态计算的输入框高度
                let input_area_h = st.ai.ai_panel.input_computed_height + 44.0f32;
                let text_input_y = rp.height - input_area_h + 6.0;
                let text_input_h = st.ai.ai_panel.input_computed_height;
                if rp_rel_y >= text_input_y
                    && rp_rel_y < text_input_y + text_input_h
                    && rp_rel_x >= margin + input_margin
                    && rp_rel_x < rp.width - margin - input_margin
                {
                    return CursorType::IBeam;
                }
            }
        }

        // 11b. 新标签页（NTP）：快捷搜索框 → IBeam，其余快捷按钮 → Hand
        // 几何与渲染/点击共用 new_tab_page_region + new_tab_page_geom_in
        if st.ntp_active() {
            let search_box = crate::render::new_tab_page_geom_in(st.new_tab_page_region(&layout));
            let btn_height = crate::render::NEW_TAB_BTN_HEIGHT;
            let btn_gap = crate::render::NEW_TAB_BTN_GAP;
            for i in 0..crate::render::NEW_TAB_BTN_COUNT {
                let btn_y = search_box.y + i as f32 * (btn_height + btn_gap);
                let rect =
                    crate::layout::Region::new(search_box.x, btn_y, search_box.width, btn_height);
                if rect.contains(mouse_x, mouse_y) {
                    return if i == 0 {
                        CursorType::IBeam
                    } else {
                        CursorType::Hand
                    };
                }
            }
        }

        // 11c. 终端区域：内层标签栏（终端/问题）→ Hand，终端内容区 → IBeam
        // 区域与 render_bottom_panel / terminal_view_region 几何一致
        let term_region = if st.editor_mode.is_agent() && st.active_tab_is_terminal() {
            Some(st.terminal_view_region(&layout))
        } else if !st.editor_mode.is_agent()
            && layout.bottom_panel_visible
            && st.terminal.bottom_panel_tab == crate::editor::BottomPanelTab::Terminal
        {
            Some(layout.bottom_panel_region())
        } else {
            None
        };
        if let Some(tr) = term_region {
            if tr.contains(mouse_x, mouse_y) {
                return if mouse_y < tr.y + crate::render::TERMINAL_CONTENT_TOP {
                    CursorType::Hand
                } else {
                    CursorType::IBeam
                };
            }
        }

        // 12. 编辑器内容区
        if editor_content.contains(mouse_x, mouse_y) {
            // 欢迎页/新标签页：非文本区域 → Arrow（可点项已在步骤 2/11b 返回 Hand）
            if st.show_welcome() || st.ntp_active() {
                return CursorType::Arrow;
            }
            // 设置页：仅文本输入字段 → IBeam（Provider 为下拉选择，保持 Arrow）
            if st.active_tab_is_settings() {
                if st
                    .ui
                    .settings_panel
                    .hit_test_field(mouse_x, mouse_y)
                    .is_some_and(|f| f != crate::settings::SettingsField::Provider)
                {
                    return CursorType::IBeam;
                }
                return CursorType::Arrow;
            }
            // 沙盒评测页：仅文本输入字段 → IBeam，其余为 Arrow
            if st.active_tab_is_sandbox_eval() {
                let regions = &st.ui.sandbox_eval.regions;
                let in_field = regions
                    .topic_field
                    .is_some_and(|r| crate::sandbox_eval::rect_hit(&r, mouse_x, mouse_y))
                    || regions
                        .custom_count_field
                        .is_some_and(|r| crate::sandbox_eval::rect_hit(&r, mouse_x, mouse_y));
                if in_field {
                    return CursorType::IBeam;
                }
                return CursorType::Arrow;
            }
            // 图片预览：非文本 → Arrow
            if st.editor.content.language == aether_core::lexer::Language::Image {
                return CursorType::Arrow;
            }
            // Markdown 切换按钮 hover → Hand
            if st.editor.content.language == aether_core::lexer::Language::Markdown {
                if let Some(btn) = &st.editor.markdown_toggle_btn {
                    if btn.contains(mouse_x, mouse_y) {
                        return CursorType::Hand;
                    }
                }
                // Markdown 预览模式：非编辑区 → Arrow
                if st.editor.markdown_preview {
                    return CursorType::Arrow;
                }
            }
            return CursorType::IBeam;
        }

        // 13. 状态栏 → Hand（clickable 分区）
        let status_region = layout.status_bar_region();
        if status_region.contains(mouse_x, mouse_y) {
            let rel_x = mouse_x - status_region.x;
            let rel_y = mouse_y - status_region.y;
            if let Some(idx) = st.ui.status_bar.hit_test(rel_x, rel_y, status_region.width) {
                if st
                    .ui
                    .status_bar
                    .sections
                    .get(idx)
                    .is_some_and(|sec| sec.clickable)
                {
                    return CursorType::Hand;
                }
            }
            return CursorType::Arrow;
        }

        // 14. 默认 → Arrow
        CursorType::Arrow
    })
}
