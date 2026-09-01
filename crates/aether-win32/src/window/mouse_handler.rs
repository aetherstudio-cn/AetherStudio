//! 鼠标事件处理模块。
//!
//! 从 `window.rs` 拆分而来，保持原有逻辑不变。
//! 包含小型鼠标处理函数；大型函数（`on_l_button_down`、`on_mouse_move`）
//! 拆分到子模块中以控制单文件行数。

mod l_button_down;
mod m_button_down;
mod mouse_move;
mod r_button_down;

pub(crate) use l_button_down::on_l_button_down;
pub(crate) use m_button_down::on_m_button_down;
pub(crate) use mouse_move::{compute_cursor_for_pos, on_mouse_move};
pub(crate) use r_button_down::{on_r_button_down, on_r_button_up};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{get_and_set_state, invalidate_window, EDITOR_STATE, LP_TIMER_ID};

/// WM_MBUTTONUP：鼠标中键释放事件
pub(crate) unsafe fn on_m_button_up(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            // 结束图片拖拽
            if st.input.mouse_press.image_dragging {
                st.input.mouse_press.image_dragging = false;
                st.input.mouse_press.image_drag_start = None;
                st.input.mouse_press.image_drag_offset = None;
            }
        }
    });
    LRESULT(0)
}

/// WM_LBUTTONUP
pub(crate) unsafe fn on_l_button_up(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let _ = KillTimer(hwnd, LP_TIMER_ID);
    let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
    let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            // 结束滚动条拖拽并释放鼠标捕获
            if st.editor.scrollbar_drag.is_some() {
                st.editor.scrollbar_drag = None;
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            }
            st.end_selection();
            // 结束面板拖拽
            st.ui.layout.right_panel_resizing = false;
            st.ui.layout.bottom_panel_resizing = false;
            // 拐角手柄拖拽结束（右下拐角仅复位；左下拐角含侧边栏，需收起判断）
            st.ui.layout.corner_right_resizing = false;
            let corner_left_was = st.ui.layout.corner_left_resizing;
            st.ui.layout.corner_left_resizing = false;
            // 侧边栏拖拽结束：当前宽度低于阈值且仍可见 → 启动平滑收起动画（而非立即跳变）
            if st.ui.layout.sidebar_resizing || corner_left_was {
                st.ui.layout.sidebar_resizing = false;
                let collapse_threshold = crate::layout::MIN_SIDEBAR_WIDTH * 0.5;
                if st.ui.layout.sidebar_visible && st.ui.layout.sidebar_width < collapse_threshold {
                    st.ui.layout.sidebar_anim = Some(crate::layout::SidebarAnim::new(
                        st.ui.layout.sidebar_width,
                        0.0,
                    ));
                }
            }
            st.ui.settings_panel.temp_slider_dragging = false;
            st.ui.settings_panel.top_p_slider_dragging = false;
            st.ui.settings_panel.freq_slider_dragging = false;
            st.ui.settings_panel.pres_slider_dragging = false;
            // 历史浮窗拖动结束
            st.ai.ai_panel.history_win_drag = None;
            // 长按检测状态清理
            st.input.mouse_press.lbutton_down = false;
            st.input.mouse_press.lbutton_down_pos = None;
            st.input.mouse_press.lpress_target = None;
            st.input.mouse_press.lpress_start = None;
            // 文件树拖拽：拖拽中则以释放位置执行移动，否则仅清理按下候选
            let file_drag_handled = {
                let dpi_scale = st.win.dpi_scale;
                st.file_drag_finish(raw_x / dpi_scale, raw_y / dpi_scale)
            };
            // 自定义模式下：完成拖拽重排 + 持久化
            let persist_activity =
                st.ui.activity_bar.customize_mode && st.ui.activity_bar.drag_index.is_some();
            let persist_menu = st.ui.menu_bar.customize_mode && st.ui.menu_bar.drag_index.is_some();
            if persist_activity {
                st.ui.activity_bar.reorder();
                st.ui.app_settings.ui.activity_bar_order = st.ui.activity_bar.order_keys();
                let _ = st.ui.app_settings.save();
                st.ui.status_message = "活动栏顺序已保存".to_string();
            }
            if persist_menu {
                st.ui.menu_bar.reorder();
                st.ui.app_settings.ui.menu_bar_order = st.ui.menu_bar.order_keys();
                let _ = st.ui.app_settings.save();
                st.ui.status_message = "菜单栏顺序已保存".to_string();
            }
            // Task 8.4: 标签拖拽重排或延迟切换
            let tab_handled = if let (Some(drag_idx), Some(drop_idx)) = (
                st.editor.tab_bar.dragging_tab,
                st.editor.tab_bar.tab_drop_index,
            ) {
                if drag_idx < st.editor.tab_bar.tabs.len()
                    && drop_idx <= st.editor.tab_bar.tabs.len()
                    && drag_idx != drop_idx
                {
                    st.reorder_tabs(drag_idx, drop_idx);
                    st.ui.status_message = "标签已重排".to_string();
                }
                st.editor.tab_bar.dragging_tab = None;
                st.editor.tab_bar.tab_drop_index = None;
                st.editor.tab_bar.tab_drag_start = None;
                true
            } else if st.editor.tab_bar.tab_drag_start.is_some() {
                // 未进入拖拽模式 → 视为普通点击切换标签
                st.editor.tab_bar.tab_drag_start = None;
                let dpi_scale = st.win.dpi_scale;
                let mouse_x = raw_x / dpi_scale;
                let mouse_y = raw_y / dpi_scale;
                // 智能体模式下标签栏在右侧面板顶部，使用实际区域
                let tab_region = st.effective_tab_bar_region();
                if let Some(tab_idx) =
                    st.tab_body_hit_test(mouse_x, mouse_y, tab_region.x, tab_region.y)
                {
                    st.switch_tab(tab_idx);
                }
                true
            } else {
                false
            };
            // 仅在用户实际开始拖拽时才重绘
            if persist_activity || persist_menu || tab_handled || file_drag_handled {
                invalidate_window(hwnd);
            }
        }
    });
    LRESULT(0)
}

/// WM_LBUTTONDBLCLK
pub(crate) unsafe fn on_l_button_dblclk(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // P2-5: 双击选词
    let raw_x = (lparam.0 & 0xFFFF) as i16 as f32;
    let raw_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
    if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        // 仅在非对话框、非命令面板、非欢迎页时处理编辑器区域双击
        // （settings_panel 在侧边栏，editor_region.contains 已排除）
        if st.remote.ssh_dialog.visible
            || st.remote.clone_dialog.visible
            || st.ui.command_palette.visible
            || st.show_welcome()
        {
            return LRESULT(0);
        }
        let mouse_x = raw_x / st.win.dpi_scale;
        let mouse_y = raw_y / st.win.dpi_scale;
        let layout = st.ui.layout.clone();
        let show_tab_bar = st.show_tab_bar();
        let editor_content = layout.editor_content_region(show_tab_bar);
        let editor_region = crate::layout::Region::new(
            editor_content.x,
            editor_content.y,
            editor_content.width,
            editor_content.height,
        );
        if editor_region.contains(mouse_x, mouse_y) {
            st.select_word_at_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
            invalidate_window(hwnd);
        }
    }
    LRESULT(0)
}

/// WM_MOUSEWHEEL
pub(crate) unsafe fn on_mouse_wheel(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32;
    // H-18: 提取光标屏幕坐标并转换为客户端坐标
    let screen_x = (lparam.0 & 0xFFFF) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    let mut client_point = windows::Win32::Foundation::POINT {
        x: screen_x,
        y: screen_y,
    };
    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut client_point);
    // P0-3: Shift + 滚轮 → 横向滚动
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut state = state.borrow_mut();
            // UI-C01: ScreenToClient 返回物理像素，需转换为逻辑像素
            let dpi_scale = state.win.dpi_scale;
            let cursor_x = client_point.x as f32 / dpi_scale;
            let cursor_y = client_point.y as f32 / dpi_scale;

            // 图片预览：Ctrl+滚轮缩放
            if state.editor.content.language == aether_core::lexer::Language::Image && ctrl {
                let editor = state.ui.layout.editor_region();
                if cursor_x >= editor.x
                    && cursor_x < editor.x + editor.width
                    && cursor_y >= editor.y
                    && cursor_y < editor.y + editor.height
                {
                    // 缩放因子：每 120 单位滚轮 = 10% 缩放
                    let zoom_delta = delta / 120.0 * 0.1;
                    state.win.image_zoom = (state.win.image_zoom + zoom_delta).clamp(0.1, 10.0);
                    invalidate_window(hwnd);
                    return;
                }
            }

            // SubTask 7.5: 光标在标签栏区域时 → 横向滚动标签栏（平滑滚动）
            // 智能体模式下标签栏在右侧面板顶部，使用实际区域
            let show_tab_bar = state.show_tab_bar();
            let tab_region = state.effective_tab_bar_region();
            if show_tab_bar && tab_region.contains(cursor_x, cursor_y) {
                if state.scroll_tab_bar(delta, tab_region.width) {
                    // 只标记标签栏区域为脏，避免全窗口重绘
                    state.win.dirty_tracker.mark_region(
                        tab_region.x,
                        tab_region.y,
                        tab_region.width,
                        tab_region.height,
                        crate::dirty_rect::DirtyRegionType::TabBar,
                    );
                    invalidate_window(hwnd);
                }
                return;
            }

            // P0-3: Shift+滚轮 或 光标在编辑器区域内时 → 横向滚动
            if shift {
                let editor = state.ui.layout.editor_region();
                if cursor_x >= editor.x
                    && cursor_x < editor.x + editor.width
                    && cursor_y >= editor.y
                    && cursor_y < editor.y + editor.height
                {
                    // Shift+滚轮向右滚动查看右侧内容
                    let char_width = state.win.text_renderer.char_width();
                    state.scroll_horizontal(-delta * char_width);
                    invalidate_window(hwnd);
                    return;
                }
            }

            // 检查光标是否在底部终端面板区域内
            if state.ui.layout.bottom_panel_visible {
                let bottom = state.ui.layout.bottom_panel_region();
                if bottom.contains(cursor_x, cursor_y) {
                    // 向上滚动(delta>0)查看更早输出，向下滚动回到最新
                    let lines = ((delta.abs() / 120.0).ceil() as usize).max(1);
                    if delta > 0.0 {
                        state.terminal.terminal_panel.scroll_up(lines * 3);
                    } else {
                        state.terminal.terminal_panel.scroll_down(lines * 3);
                    }
                    // 只标脏底部面板局部重绘；依赖 WM_PAINT 全窗口兕底会导致滚轮卡顿
                    state.win.dirty_tracker.mark_region(
                        bottom.x,
                        bottom.y,
                        bottom.width,
                        bottom.height,
                        crate::dirty_rect::DirtyRegionType::BottomPanel,
                    );
                    invalidate_window(hwnd);
                    return;
                }
            }
            // 历史浮窗：光标在浮窗内 → 滚动浮窗列表（全局最顶层，优先于其他滚动）
            if state.ai.ai_panel.history_open {
                if let Some((px, py, pw, ph)) = state.ai.ai_panel.history_win_region {
                    if cursor_x >= px && cursor_x < px + pw && cursor_y >= py && cursor_y < py + ph
                    {
                        let scroll_amount = delta * 2.0;
                        state.ai.ai_panel.history_scroll = (state.ai.ai_panel.history_scroll
                            - scroll_amount)
                            .clamp(0.0, state.ai.ai_panel.history_max_scroll.max(0.0));
                        invalidate_window(hwnd);
                        return;
                    }
                }
            }
            // 检查光标是否在右侧 AI 面板区域内（仅经典模式；
            // 智能体模式下右面板是文件编辑器，由下方 active_code_editor_region 分支处理）
            if !state.editor_mode.is_agent() && state.ui.layout.right_panel_visible {
                let right_panel = state.ui.layout.right_panel_region();
                if right_panel.contains(cursor_x, cursor_y) {
                    let chat_top = 52.0f32;
                    // 使用动态计算的输入框高度
                    let input_area_h = state.ai.ai_panel.input_area_height();
                    let chat_bottom = right_panel.height - input_area_h;
                    // 只有当光标在聊天消息区域（非输入框）时才滚动
                    if cursor_y >= chat_top && cursor_y < chat_bottom {
                        let scroll_amount = delta * 2.0; // 每滚轮单位滚动 2 像素
                        state.ai.ai_panel.scroll_y = (state.ai.ai_panel.scroll_y - scroll_amount)
                            .clamp(0.0, state.ai.ai_panel.content_height.max(0.0));
                        state.ai.ai_panel.stick_to_bottom = false; // 用户手动滚动时取消吸附底部
                        invalidate_window(hwnd);
                        return;
                    }
                }
            }

            // 设置页：光标在设置页区域内（经典=编辑器内容区，智能体=右面板内容区）→ 滚动设置内容
            if state.active_tab_is_settings() {
                let page = state.full_page_region(&state.ui.layout);
                if page.contains(cursor_x, cursor_y) {
                    // delta>0（上滚）减小偏移查看上方内容
                    state.ui.settings_panel.scroll_by(-delta * 0.5);
                    invalidate_window(hwnd);
                    return;
                }
            }

            // 沙盒评测页：光标在页面区域内（经典=编辑器内容区，智能体=右面板内容区）→ 滚动页面内容
            if state.active_tab_is_sandbox_eval() {
                let page = state.full_page_region(&state.ui.layout);
                if page.contains(cursor_x, cursor_y) {
                    let max_scroll = (state.ui.sandbox_eval.content_height
                        - state.ui.sandbox_eval.view_height)
                        .max(0.0);
                    state.ui.sandbox_eval.scroll_y =
                        (state.ui.sandbox_eval.scroll_y - delta * 0.5).clamp(0.0, max_scroll);
                    invalidate_window(hwnd);
                    return;
                }
            }

            // 智能体模式：中间区域是 AI 对话面板 → 滚动对话（与经典右面板 AI 滚动同口径）
            if state.editor_mode.is_agent() {
                let center = state.ui.layout.editor_content_region(false);
                if center.contains(cursor_x, cursor_y) {
                    let scroll_amount = delta * 2.0;
                    state.ai.ai_panel.scroll_y = (state.ai.ai_panel.scroll_y - scroll_amount)
                        .clamp(0.0, state.ai.ai_panel.content_height.max(0.0));
                    state.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                    return;
                }
            }

            // 代码编辑器区域（经典中间区 / 智能体右侧标签面板）→ 编辑器滚动
            if let Some(region) = state.active_code_editor_region() {
                if region.contains(cursor_x, cursor_y) {
                    if shift {
                        let char_width = state.win.text_renderer.char_width();
                        state.scroll_horizontal(-delta * char_width);
                    } else {
                        state.scroll(-delta);
                    }
                    invalidate_window(hwnd);
                    return;
                }
            }

            // 检查光标是否在侧边栏区域内
            let sidebar = state.ui.layout.sidebar_region();
            if state.ui.layout.sidebar_visible
                && cursor_x >= sidebar.x
                && cursor_x < sidebar.x + sidebar.width
                && cursor_y >= sidebar.y
                && cursor_y < sidebar.y + sidebar.height
            {
                state.scroll_sidebar(-delta);
            } else {
                state.scroll(-delta);
            }
            invalidate_window(hwnd);
        }
    });
    LRESULT(0)
}

/// WM_MOUSEHWHEEL
pub(crate) unsafe fn on_mouse_hwheel(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // P0-3: 横向滚轮（触控板水平滚动 / 鼠标侧键）
    let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as f32;
    let screen_x = (lparam.0 & 0xFFFF) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    let mut client_point = windows::Win32::Foundation::POINT {
        x: screen_x,
        y: screen_y,
    };
    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut client_point);
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut state = state.borrow_mut();
            let dpi_scale = state.win.dpi_scale;
            let cursor_x = client_point.x as f32 / dpi_scale;
            let cursor_y = client_point.y as f32 / dpi_scale;
            let editor = state.ui.layout.editor_region();
            // 仅在编辑器区域内响应横向滚轮
            if cursor_x >= editor.x
                && cursor_x < editor.x + editor.width
                && cursor_y >= editor.y
                && cursor_y < editor.y + editor.height
            {
                let char_width = state.win.text_renderer.char_width();
                // delta > 0 表示向右滚动触控板，光标向右移动查看右侧内容
                state.scroll_horizontal(-delta * char_width);
                invalidate_window(hwnd);
            }
        }
    });
    LRESULT(0)
}
