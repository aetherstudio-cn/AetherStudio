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
        let mouse_x = raw_x / st.dpi_scale;
        let mouse_y = raw_y / st.dpi_scale;
        let layout = st.layout.clone();
        (mouse_x, mouse_y, layout)
    };
    // 早期返回：对话框悬停 / 自定义模式拖拽
    if let Some(r) = omm_early_returns(hwnd, &state, mouse_x, mouse_y, is_dragging, &layout) {
        return r;
    }
    // 文本拖拽选区：前置为最高优先级（仅次于菜单/对话框）。
    // 旧实现放在所有 hover 判定之后的 else 分支，任一 hover 变化都会
    // 提前走 invalidate 分支跳过选区更新，导致拖拽时预选中高亮跟不上鼠标；
    // 拖拽选区期间 hover/tooltip 状态无意义，直接跳过还能省掉逐帧命中开销。
    if is_dragging && state.borrow().is_selecting {
        let mut st = state.borrow_mut();
        let editor_content = layout.editor_content_region(st.show_tab_bar());
        let before = (
            st.content.cursor_line,
            st.content.cursor_col,
            st.content.selection_end,
        );
        st.set_cursor_from_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
        st.update_selection();
        let changed = (
            st.content.cursor_line,
            st.content.cursor_col,
            st.content.selection_end,
        ) != before;
        if changed {
            // 标记编辑区+状态栏脏区：避免无脏区退化为全窗口无裁剪重绘
            st.dirty_tracker.mark_region(
                editor_content.x,
                editor_content.y,
                editor_content.width,
                editor_content.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
            let sb = st.layout.status_bar_region();
            st.dirty_tracker.mark_region(
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
    // 资源管理器空白区域上下文菜单：更新 hover 状态
    if st.context_menus.explorer.is_open {
        let changed = st.context_menus.explorer.update_hover(mouse_x, mouse_y);
        if changed {
            // 标记菜单区域为脏，否则脏矩形渲染器会因“无脏区”跳过整帧重绘 → hover 卡顿
            let mx = st.context_menus.explorer.origin_x;
            let my = st.context_menus.explorer.origin_y;
            let mw = st.context_menus.explorer.menu_width();
            let mh = st.context_menus.explorer.menu_height();
            st.dirty_tracker.mark_region(
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
    if st.context_menus.file_node.is_open {
        let changed = st.context_menus.file_node.update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.context_menus.file_node.origin_x;
            let my = st.context_menus.file_node.origin_y;
            let mw = st.context_menus.file_node.menu_width();
            let mh = st.context_menus.file_node.menu_height();
            st.dirty_tracker.mark_region(
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
    if st.context_menus.tab.visible {
        let changed = st.context_menus.tab.update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.context_menus.tab.x;
            let my = st.context_menus.tab.y;
            let mw = st.context_menus.tab.width;
            let mh = st.context_menus.tab.menu_height();
            st.dirty_tracker.mark_region(
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
    if st.context_menus.activity_bar.visible {
        let changed = st.context_menus.activity_bar.update_hover(mouse_x, mouse_y);
        if changed {
            let mx = st.context_menus.activity_bar.x;
            let my = st.context_menus.activity_bar.y;
            let mw = st.context_menus.activity_bar.width;
            let mh = st.context_menus.activity_bar.menu_height();
            st.dirty_tracker.mark_region(
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
        drop(st);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if st.remote.clone_dialog.visible {
        st.handle_clone_dialog_hover(mouse_x, mouse_y);
        drop(st);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // 长按检测：移动超过容差则取消
    if is_dragging && st.mouse_press.lpress_target.is_some() {
        let dx = mouse_x - st.mouse_press.lpress_x;
        let dy = mouse_y - st.mouse_press.lpress_y;
        if dx.abs() > LP_MOVE_TOLERANCE || dy.abs() > LP_MOVE_TOLERANCE {
            let _ = KillTimer(hwnd, LP_TIMER_ID);
            st.mouse_press.lpress_target = None;
            st.mouse_press.lpress_start = None;
        }
    }
    // 自定义模式下：跟随鼠标更新放置目标
    let activity_dragging = st.activity_bar.customize_mode && st.activity_bar.drag_index.is_some();
    let menu_dragging = st.menu_bar.customize_mode && st.menu_bar.drag_index.is_some();
    if is_dragging && activity_dragging {
        let bar_y = layout.activity_bar_region().y;
        st.activity_bar.drop_index = Some(st.activity_bar.drop_index_at(mouse_y, bar_y));
        drop(st);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if is_dragging && menu_dragging {
        st.menu_bar.drop_index = Some(st.menu_bar.drop_index_at(mouse_x));
        drop(st);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // Task 8.3: 标签拖拽——检测阈值进入拖拽模式，或更新 drop_index
    if is_dragging && st.tab_bar.tab_drag_start.is_some() {
        if st.tab_bar.dragging_tab.is_none() {
            // 判定是否超过 3px 阈值（dx*dx + dy*dy > 9）
            let (sx, sy) = st.tab_bar.tab_drag_start.unwrap();
            let dx = mouse_x - sx as f32;
            let dy = mouse_y - sy as f32;
            if dx * dx + dy * dy > 9.0 {
                // 进入拖拽模式：使用当前 hover_tab 作为拖拽目标
                if let Some(hover) = st.tab_bar.hover_tab {
                    st.tab_bar.dragging_tab = Some(hover);
                    st.tab_bar.tab_drop_index = Some(hover);
                    drop(st);
                    invalidate_window(hwnd);
                    return Some(LRESULT(0));
                }
            }
        } else {
            // 已在拖拽模式：更新 drop_index
            let show_tab_bar = st.show_tab_bar();
            let editor_content = layout.editor_content_region(show_tab_bar);
            let new_drop = st.tab_drop_index_at(mouse_x, editor_content.x);
            let changed = st.tab_bar.tab_drop_index != Some(new_drop);
            st.tab_bar.tab_drop_index = Some(new_drop);
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
    let old_titlebar_hover = st.titlebar_hover_button;
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
        let forward_btn_x = tb.forward_btn_x;
        let back_btn_x = tb.back_btn_x;

        if mouse_x >= minimize_x {
            if mouse_x >= close_x {
                st.titlebar_hover_button = Some(2);
            } else if mouse_x >= maximize_x {
                st.titlebar_hover_button = Some(1);
            } else {
                st.titlebar_hover_button = Some(0);
            }
        } else if mouse_x >= user_btn_x {
            st.titlebar_hover_button = Some(3);
        } else if mouse_x >= settings_btn_x {
            st.titlebar_hover_button = Some(4);
        } else if mouse_x >= right_panel_btn_x {
            st.titlebar_hover_button = Some(5);
        } else if mouse_x >= bottom_panel_btn_x {
            st.titlebar_hover_button = Some(6);
        } else if mouse_x >= left_sidebar_btn_x {
            st.titlebar_hover_button = Some(7);
        } else if mouse_x >= forward_btn_x {
            st.titlebar_hover_button = Some(8);
        } else if mouse_x >= back_btn_x {
            st.titlebar_hover_button = Some(9);
        } else {
            st.titlebar_hover_button = None;
        }
    } else {
        st.titlebar_hover_button = None;
    }
    let titlebar_changed = old_titlebar_hover != st.titlebar_hover_button;
    // 菜单栏悬停
    let old_menu_hover = st.menu_bar.hover_index;
    if titlebar_region.contains(mouse_x, mouse_y) {
        let btn_width = 40.0;
        let minimize_x = titlebar_region.x + titlebar_region.width - btn_width * 3.0;
        if mouse_x < minimize_x {
            st.menu_bar.hover_index =
                st.menu_bar
                    .hit_test(mouse_x, mouse_y - titlebar_region.y, titlebar_region.height);
        } else {
            st.menu_bar.hover_index = None;
        }
    } else {
        st.menu_bar.hover_index = None;
    }
    // 菜单展开状态下：横向移动悬停即切换展开项（符合常规菜单栏行为），
    // 并追踪子菜单项悬停以驱动高亮反馈
    let old_submenu_hover = st.menu_bar.submenu_hover;
    if let Some(active_idx) = st.menu_bar.active_index {
        if !st.menu_bar.customize_mode {
            if let Some(h) = st.menu_bar.hover_index {
                if h != active_idx && h < st.menu_bar.items.len() {
                    st.menu_bar.expand(h);
                }
            }
        }
        let cur_active = st.menu_bar.active_index.unwrap_or(active_idx);
        st.menu_bar.submenu_hover = st
            .menu_bar
            .item_x_positions
            .get(cur_active)
            .and_then(|&sx| {
                let sy = titlebar_region.y + titlebar_region.height;
                st.menu_bar
                    .hit_test_submenu(cur_active, mouse_x, mouse_y, sx, sy)
            });
    } else {
        st.menu_bar.submenu_hover = None;
    }
    titlebar_changed
        || old_menu_hover != st.menu_bar.hover_index
        || old_submenu_hover != st.menu_bar.submenu_hover
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
    st.activity_bar.hover_index = st
        .activity_bar
        .hit_test(mouse_x, mouse_y, activity_region.y);
    // 标签栏悬停
    let editor_content = layout.editor_content_region(st.show_tab_bar());
    let old_hover = st.tab_bar.hover_tab;
    st.update_hover_tab(mouse_x, mouse_y, editor_content.x);
    (old_hover != st.tab_bar.hover_tab, editor_content)
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
    let _old_tree_hover = st.hover_file_node;
    if sidebar_region.contains(mouse_x, mouse_y) {
        if st.sidebar_content == crate::layout::SidebarContent::RemoteManagerPanel {
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
        } else if st.sidebar_content == crate::layout::SidebarContent::SourceControlPanel {
            let old_hover = st.git.hover_button.clone();
            st.update_git_panel_hover(mouse_x - sidebar_region.x, mouse_y - sidebar_region.y);
            old_hover != st.git.hover_button
        } else {
            st.update_file_tree_hover(mouse_x - sidebar_region.x, mouse_y - sidebar_region.y)
        }
    } else {
        let old = st.hover_file_node.take();
        let old_root = std::mem::take(&mut st.hover_file_tree_root);
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
        && st.sidebar_content == crate::layout::SidebarContent::RemoteManagerPanel
    {
        // SSH 管理面板已在上面处理悬停
        false
    } else {
        let mut changed = false;
        if st.settings_panel.hover_tab.is_some() {
            st.settings_panel.hover_tab = None;
            changed = true;
        }
        // 模型管理页悬停检测（仅列表视图）
        if st.settings_panel.active_tab == crate::settings::SettingsTab::Models
            && !st.settings_panel.model_editing
        {
            let editor_region = layout.editor_region();
            if editor_region.contains(mouse_x, mouse_y) {
                // 命中区以绝对坐标注册（原点为 editor_content_region），用绝对 mouse_x/mouse_y 命中测试
                // 检测模型项悬停
                let new_hover_id = st.settings_panel.hit_test_model_item(mouse_x, mouse_y);
                if st.settings_panel.hover_model_id != new_hover_id {
                    st.settings_panel.hover_model_id = new_hover_id.clone();
                    changed = true;
                }
                // 检测模型按钮悬停
                let new_hover_btn = st.settings_panel.hit_test_model_button(mouse_x, mouse_y);
                let (new_btn, new_btn_id) = match new_hover_btn {
                    Some((btn, id)) => (Some(btn), Some(id)),
                    None => (None, None),
                };
                if st.settings_panel.hover_model_button != new_btn {
                    st.settings_panel.hover_model_button = new_btn;
                    changed = true;
                }
                if st.settings_panel.hover_model_button_id != new_btn_id {
                    st.settings_panel.hover_model_button_id = new_btn_id;
                    changed = true;
                }
            } else {
                if st.settings_panel.hover_model_id.is_some() {
                    st.settings_panel.hover_model_id = None;
                    changed = true;
                }
                if st.settings_panel.hover_model_button.is_some() {
                    st.settings_panel.hover_model_button = None;
                    changed = true;
                }
                if st.settings_panel.hover_model_button_id.is_some() {
                    st.settings_panel.hover_model_button_id = None;
                    changed = true;
                }
            }
        }
        // 模型编辑表单（AI 配置）：API 密钥显隐按钮悬停
        if st.settings_panel.active_tab == crate::settings::SettingsTab::Ai
            || (st.settings_panel.active_tab == crate::settings::SettingsTab::Models
                && st.settings_panel.model_editing)
        {
            let new_eye_hover = st.settings_panel.hit_test_api_key_toggle(mouse_x, mouse_y);
            if st.settings_panel.hover_api_key_toggle != new_eye_hover {
                st.settings_panel.hover_api_key_toggle = new_eye_hover;
                changed = true;
            }
        }
        // 温度滑块拖拽：拖拽中根据鼠标 x 实时更新温度
        if st.settings_panel.temp_slider_dragging {
            if is_dragging {
                if st.settings_panel.set_temperature_from_slider_x(mouse_x) {
                    changed = true;
                }
            } else {
                st.settings_panel.temp_slider_dragging = false;
            }
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
        let old_apply_hover = st.ai_panel.hover_apply_button;
        st.ai_panel.hover_apply_button = rel_x >= apply_btn_x
            && rel_x < apply_btn_x + apply_btn_w
            && rel_y >= apply_y
            && rel_y < apply_y + apply_btn_h;
        // 历史条目 / 会话标签悬停（命中区为绝对坐标）
        let old_tab_hover = st.ai_panel.hover_tab;
        st.ai_panel.hover_tab = if st.ai_panel.history_open {
            st.ai_panel
                .history_item_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        } else {
            st.ai_panel
                .tab_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        };
        old_apply_hover != st.ai_panel.hover_apply_button || old_tab_hover != st.ai_panel.hover_tab
    } else {
        let old = st.ai_panel.hover_apply_button;
        let old_tab = st.ai_panel.hover_tab;
        st.ai_panel.hover_apply_button = false;
        st.ai_panel.hover_tab = None;
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
    let old_welcome_hover = st.welcome_hover_action.clone();
    if st.show_welcome() {
        let welcome_x = 0.0;
        let welcome_y = layout.top_offset();
        let welcome_width = st.window_width as f32;
        let welcome_height = st.window_height as f32
            - welcome_y
            - if layout.status_bar_visible {
                layout.status_bar_height
            } else {
                0.0
            };
        st.welcome_hover_action = st.hit_test_welcome_action(
            mouse_x,
            mouse_y,
            welcome_x,
            welcome_y,
            welcome_width,
            welcome_height,
        );
    } else {
        st.welcome_hover_action = None;
    }
    old_welcome_hover != st.welcome_hover_action
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
    let old_hover = st.status_bar.hover_index;
    let new_hover = if layout.status_bar_visible && status_region.contains(mouse_x, mouse_y) {
        let rel_x = mouse_x - status_region.x;
        let rel_y = mouse_y - status_region.y;
        match st.status_bar.hit_test(rel_x, rel_y, status_region.width) {
            Some(idx) => {
                if st
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
    st.status_bar.hover_index = new_hover;
    old_hover != st.status_bar.hover_index
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
    st.hover_sidebar_resize = sidebar_resize_zone;
    // 设置拖拽光标
    if right_panel_resize_zone
        || st.layout.right_panel_resizing
        || sidebar_resize_zone
        || st.layout.sidebar_resizing
    {
        let hcursor = LoadCursorW(None, IDC_SIZEWE).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if bottom_panel_resize_zone || st.layout.bottom_panel_resizing {
        let hcursor = LoadCursorW(None, IDC_SIZENS).unwrap_or_default();
        let _ = SetCursor(hcursor);
    } else if st.welcome_hover_action.is_some() {
        let hcursor = LoadCursorW(None, IDC_HAND).unwrap_or_default();
        let _ = SetCursor(hcursor);
    }
    // 处理拖拽调整
    if is_dragging {
        if st.layout.right_panel_resizing {
            let delta = mouse_x - editor_region.right();
            st.layout.resize_right_panel(-delta);
            drop(st);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        } else if st.layout.sidebar_resizing {
            // 期望宽度 = 鼠标相对侧边栏左缘；不用 region.right() 做增量，
            // 因为收起后 region 宽度归零，增量式无法支持"拖回恢复"
            let sidebar_left = if st.layout.activity_bar_visible {
                st.layout.activity_bar_width
            } else {
                0.0
            };
            st.layout
                .set_sidebar_width_or_collapse(mouse_x - sidebar_left);
            drop(st);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        } else if st.layout.bottom_panel_resizing {
            let delta = mouse_y - bottom_region.y;
            st.layout.resize_bottom_panel(-delta);
            drop(st);
            invalidate_window(hwnd);
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
            st.sidebar_content,
            crate::layout::SidebarContent::FileTree | crate::layout::SidebarContent::RemoteFileTree
        );
    let has_hover_node = st.hover_file_node.is_some() || st.remote.hover_node.is_some();
    let dx = mouse_x - st.hover.last_mouse_x;
    let dy = mouse_y - st.hover.last_mouse_y;
    let moved_beyond_tolerance = dx.abs() > HOVER_MOVE_TOLERANCE || dy.abs() > HOVER_MOVE_TOLERANCE;
    if (moved_beyond_tolerance || !in_sidebar || !has_hover_node) && st.hover.tooltip.is_some() {
        st.hover.tooltip = None;
    }
    if in_sidebar && has_hover_node {
        let _ = SetTimer(hwnd, HOVER_TIMER_ID, HOVER_DELAY_MS, None);
    } else {
        let _ = KillTimer(hwnd, HOVER_TIMER_ID);
    }
    st.hover.last_mouse_x = mouse_x;
    st.hover.last_mouse_y = mouse_y;
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
    _hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> bool {
    use crate::tooltip::{TOOLTIP_DELAY_MS, TOOLTIP_MOVE_TOLERANCE};
    use windows::Win32::System::SystemInformation::GetTickCount64;

    let mut st = state.borrow_mut();
    let (new_key, tooltip_text) = st.compute_tooltip_hover_key();
    let key_changed = new_key != st.tooltip_state.hover_key;

    // 分支 1：hover_key 变化
    if key_changed {
        let was_visible = st.tooltip_state.visible_text.is_some();
        st.tooltip_state.hover_key = new_key.clone();
        st.tooltip_state.anchor = POINT {
            x: mouse_x as i32,
            y: mouse_y as i32,
        };
        st.tooltip_state.timer_start = if new_key.is_some() {
            Some(GetTickCount64())
        } else {
            None
        };
        st.tooltip_state.visible_text = None;
        // 离开元素或切换元素时，若之前有显示，需要重绘清除
        return was_visible;
    }

    // hover_key 相同且为 None：无需任何操作
    if new_key.is_none() {
        return false;
    }

    // 分支 2：检查鼠标移动距离
    let dx = mouse_x - st.tooltip_state.anchor.x as f32;
    let dy = mouse_y - st.tooltip_state.anchor.y as f32;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > TOOLTIP_MOVE_TOLERANCE {
        let was_visible = st.tooltip_state.visible_text.is_some();
        st.tooltip_state.anchor = POINT {
            x: mouse_x as i32,
            y: mouse_y as i32,
        };
        st.tooltip_state.timer_start = Some(GetTickCount64());
        if was_visible {
            st.tooltip_state.visible_text = None;
            return true;
        }
        return false;
    }

    // 分支 3：检查 timer_start 是否到达 500ms
    if let Some(start) = st.tooltip_state.timer_start {
        if st.tooltip_state.visible_text.is_none() {
            let now = GetTickCount64();
            if now - start >= TOOLTIP_DELAY_MS {
                if let Some(text) = tooltip_text {
                    st.tooltip_state.visible_text = Some(text);
                    st.tooltip_state.show_pos = (mouse_x, mouse_y);
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
        let mouse_x = x as f32 / st.dpi_scale;
        let mouse_y = y as f32 / st.dpi_scale;
        let layout = st.layout.clone();

        // 1. 对话框/命令面板打开时返回默认箭头
        if st.remote.ssh_dialog.visible
            || st.remote.clone_dialog.visible
            || st.command_palette.visible
        {
            return CursorType::Arrow;
        }

        // 2. 欢迎页 hover 项 → Hand
        if st.welcome_hover_action.is_some() {
            return CursorType::Hand;
        }

        // 3. 标题栏区域：按钮 hover 或菜单项 hover → Hand
        let titlebar_region = layout.title_bar_region();
        if titlebar_region.contains(mouse_x, mouse_y) {
            if st.titlebar_hover_button.is_some() || st.menu_bar.hover_index.is_some() {
                return CursorType::Hand;
            }
            // 标题栏空白区（拖动区）→ Arrow
            return CursorType::Arrow;
        }

        // 4. 活动栏 hover → Hand
        let activity_region = layout.activity_bar_region();
        if activity_region.contains(mouse_x, mouse_y) && st.activity_bar.hover_index.is_some() {
            return CursorType::Hand;
        }

        // 5. 面板拖拽中：固定 resize 光标（无论当前位置）
        if layout.right_panel_resizing {
            return CursorType::SizeWE;
        }
        if layout.bottom_panel_resizing {
            return CursorType::SizeNS;
        }

        let editor_region = layout.editor_region();
        let editor_content = layout.editor_content_region(st.show_tab_bar());

        // 6. 标签栏 hover → Hand
        let tab_bar_region = layout.tab_bar_region(st.show_tab_bar());
        if tab_bar_region.contains(mouse_x, mouse_y) && st.tab_bar.hover_tab.is_some() {
            return CursorType::Hand;
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
        // 几何与 render_tree_nodes 的输入框绘制保持一致（header_h + 6，高 26，左右 10 边距）
        if layout.sidebar_visible
            && st.sidebar_content == crate::layout::SidebarContent::FileTree
            && st.file_tree_input.is_some()
        {
            let sidebar = layout.sidebar_region();
            let s = st.dpi_scale;
            let input_top = sidebar.y + 28.0 * s + 6.0 * s;
            let input_h = 26.0 * s;
            if mouse_x >= sidebar.x + 10.0 * s
                && mouse_x < sidebar.x + sidebar.width - 10.0 * s
                && mouse_y >= input_top
                && mouse_y < input_top + input_h
            {
                return CursorType::IBeam;
            }
        }

        // 11. AI 面板输入框 → IBeam
        // 几何与 lbd_right_panel_apply_input 的输入框命中检测保持一致
        if layout.right_panel_visible {
            let rp = layout.right_panel_region();
            if rp.contains(mouse_x, mouse_y) {
                let rp_rel_x = mouse_x - rp.x;
                let rp_rel_y = mouse_y - rp.y;
                let margin = 10.0;
                let input_margin = 8.0;
                let input_area_h = 80.0f32;
                let text_input_y = rp.height - input_area_h + 6.0;
                let text_input_h = 36.0f32;
                if rp_rel_y >= text_input_y
                    && rp_rel_y < text_input_y + text_input_h
                    && rp_rel_x >= margin + input_margin
                    && rp_rel_x < rp.width - margin - input_margin
                {
                    return CursorType::IBeam;
                }
            }
        }

        // 12. 编辑器内容区
        if editor_content.contains(mouse_x, mouse_y) {
            // 欢迎页/空占位页：非文本区域 → Arrow（欢迎页可点项已在步骤 2 返回 Hand）
            if st.show_welcome() || st.show_empty_placeholder() {
                return CursorType::Arrow;
            }
            // 设置页：仅文本输入字段 → IBeam（Provider 为下拉选择，保持 Arrow）
            if st.active_tab_is_settings() {
                if st
                    .settings_panel
                    .hit_test_field(mouse_x, mouse_y)
                    .is_some_and(|f| f != crate::settings::SettingsField::Provider)
                {
                    return CursorType::IBeam;
                }
                return CursorType::Arrow;
            }
            // 图片预览：非文本 → Arrow
            if st.content.language == aether_core::lexer::Language::Image {
                return CursorType::Arrow;
            }
            return CursorType::IBeam;
        }

        // 13. 状态栏 → Hand（clickable 分区）
        let status_region = layout.status_bar_region();
        if status_region.contains(mouse_x, mouse_y) {
            let rel_x = mouse_x - status_region.x;
            let rel_y = mouse_y - status_region.y;
            if let Some(idx) = st.status_bar.hit_test(rel_x, rel_y, status_region.width) {
                if st
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
