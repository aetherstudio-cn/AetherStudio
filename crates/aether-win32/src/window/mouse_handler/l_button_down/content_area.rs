//! `WM_LBUTTONDOWN` 内容区域处理：活动栏 / 侧边栏 / 面板 / 编辑器。
//!
//! 从 `l_button_down.rs` 拆分而来，保持原有逻辑不变。

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LRESULT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::dialogs::Dialogs;
use crate::editor::{BottomPanelTab, EditorState};

use super::super::super::{
    invalidate_window, AI_REFRESH_MS, AI_TIMER_ID, LP_THRESHOLD_MS, LP_TIMER_ID,
};

/// 活动栏点击 + 长按检测。
pub(super) unsafe fn lbd_activity_bar(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let activity_region = layout.activity_bar_region();
    if !activity_region.contains(mouse_x, mouse_y) {
        return None;
    }
    let mut st = state.borrow_mut();
    let idx = st
        .ui
        .activity_bar
        .hit_test(mouse_x, mouse_y, activity_region.y)?;
    // 长按检测
    st.input.mouse_press.lpress_start = Some(std::time::Instant::now());
    st.input.mouse_press.lpress_x = mouse_x;
    st.input.mouse_press.lpress_y = mouse_y;
    st.input.mouse_press.lpress_target = Some(crate::input::PressTarget::ActivityBar);
    st.input.mouse_press.lpress_index = idx;
    let _ = SetTimer(hwnd, LP_TIMER_ID, LP_THRESHOLD_MS, None);
    // 自定义模式下：不切换活动，而是开始拖拽
    if st.ui.activity_bar.customize_mode {
        st.ui.activity_bar.begin_drag(idx);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    let view = st.ui.activity_bar.items[idx].view;
    if view == crate::layout::ActivityBarView::AiAssistant {
        st.ui.layout.right_panel_visible = !st.ui.layout.right_panel_visible;
        if st.ui.layout.right_panel_visible && st.ui.layout.right_panel_width < 1.0 {
            st.ui.layout.right_panel_width = 320.0;
        }
        if !st.ui.layout.right_panel_visible {
            st.ai.ai_panel.input_focused = false;
        }
        st.ui.activity_bar.switch_to(idx);
        st.ui.activity_view = view;
        st.ui.status_message = if st.ui.layout.right_panel_visible {
            "AI 面板已打开".to_string()
        } else {
            "AI 面板已关闭".to_string()
        };
    } else {
        st.ui.activity_bar.switch_to(idx);
        st.ui.activity_view = view;
        st.ui.layout.sidebar_visible = true;
        st.ui.sidebar_content = crate::layout::SidebarContent::from_view(view);
        // 欢迎页：点击功能按钮调起侧边栏面板（活动栏固定常驻）
        if st.ui.layout.welcome_active {
            st.ui.layout.welcome_left_opened = true;
        }
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 面板调整边框点击（右侧/底部面板拖拽区域）。
pub(super) unsafe fn lbd_panel_resizing(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let editor_region = layout.editor_region();
    // 拐角手柄命中（优先于单线）：左下 = 侧边栏×底部面板，右下 = 右面板×底部面板
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
    // 侧边栏调整区域（显示时：右边缘 ±4px；收起时：活动栏右缘拖回把手）
    let sidebar_region = layout.sidebar_region();
    let sidebar_resize_zone = if layout.sidebar_visible {
        (mouse_x >= sidebar_region.right() - 4.0 && mouse_x <= sidebar_region.right() + 4.0)
            && mouse_y >= sidebar_region.y
            && mouse_y < sidebar_region.y + sidebar_region.height
    } else {
        let edge = if layout.activity_bar_visible {
            layout.activity_bar_width
        } else {
            0.0
        };
        (mouse_x >= edge - 2.0 && mouse_x <= edge + 4.0)
            && mouse_y >= sidebar_region.y
            && mouse_y < sidebar_region.y + sidebar_region.height
    };
    let mut st = state.borrow_mut();
    // 拐角优先：拖拽交点同时调整两条分割线
    if corner_left_hit {
        st.ui.layout.corner_left_resizing = true;
        st.ui.layout.cancel_sidebar_anim();
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if corner_right_hit {
        st.ui.layout.corner_right_resizing = true;
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if right_panel_resize_zone {
        st.ui.layout.right_panel_resizing = true;
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if bottom_panel_resize_zone {
        st.ui.layout.bottom_panel_resizing = true;
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    if sidebar_resize_zone {
        st.ui.layout.sidebar_resizing = true;
        // 拖拽开始即打断可能正在进行的收起/展开动画
        st.ui.layout.cancel_sidebar_anim();
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    None
}

/// 侧边栏点击（SSH 管理面板 / 通用侧边栏）。
pub(super) unsafe fn lbd_sidebar(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let sidebar_region = layout.sidebar_region();
    if !sidebar_region.contains(mouse_x, mouse_y) {
        return None;
    }
    let mut st = state.borrow_mut();
    if st.ui.sidebar_content == crate::layout::SidebarContent::RemoteManagerPanel {
        drop(st);
        return lbd_ssh_manager_panel(hwnd, state, mouse_x, mouse_y);
    }

    // 智能体模式：处理左侧边栏的对话标签页点击
    if st.editor_mode.is_agent() {
        // 1. 新会话按钮
        if let Some((bx, by, bw, bh)) = st.ai.ai_panel.agent_new_chat_region {
            if mouse_x >= bx && mouse_x < bx + bw && mouse_y >= by && mouse_y < by + bh {
                st.ai.ai_panel.new_conversation();
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
        // 2. 对话标签页关闭按钮
        for &(idx, rx, ry, rw, rh) in &st.ai.ai_panel.agent_tab_close_regions {
            if mouse_x >= rx && mouse_x < rx + rw && mouse_y >= ry && mouse_y < ry + rh {
                st.ai.ai_panel.snapshot_active_into_slot();
                st.ai.ai_panel.close_conversation(idx);
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
        // 3. 对话标签页切换
        for &(idx, rx, ry, rw, rh) in &st.ai.ai_panel.agent_tab_regions {
            if mouse_x >= rx && mouse_x < rx + rw && mouse_y >= ry && mouse_y < ry + rh {
                st.ai.ai_panel.switch_to(idx);
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
        // 4. 对话历史折叠/展开
        // TODO: 添加对话历史折叠/展开的命中检测

        // 5. 工作区文件列表点击（复用现有逻辑）
        // 智能体模式下，文件树区域被下移（上面有新会话按钮和对话标签页），
        // 使用渲染时记录的偏移量调整坐标
        let sidebar_rel_x = mouse_x - sidebar_region.x;
        let sidebar_rel_y = mouse_y - sidebar_region.y - st.ai.ai_panel.agent_file_tree_offset_y;
        if st.handle_sidebar_click(sidebar_rel_x, sidebar_rel_y) {
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 点击已确认在侧边栏区域内，即使未命中任何交互元素也应消费事件
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    let sidebar_rel_x = mouse_x - sidebar_region.x;
    let sidebar_rel_y = mouse_y - sidebar_region.y;
    // 文件树拖拽候选：按下命中节点名称区域时记录（超过阈值后才进入拖拽，
    // 普通点击行为不受影响）；必须在 handle_sidebar_click 之前命中测试，
    // 因为点击可能展开/折叠目录改变可见行布局
    if st.ui.sidebar_content == crate::layout::SidebarContent::FileTree
        && st.fs.file_tree_root_expanded
        && st.fs.file_tree_input.is_none()
    {
        let nodes_start_y = st.file_tree_nodes_start_y();
        let sidebar_width = st.ui.layout.sidebar_width;
        if let Some((node_idx, _, part)) =
            st.file_tree_hit_test(sidebar_rel_x, sidebar_rel_y, nodes_start_y, sidebar_width)
        {
            if part == crate::editor::FileTreeClickPart::Label {
                st.file_drag_begin_press(node_idx, mouse_x, mouse_y);
            }
        }
    }
    if st.handle_sidebar_click(sidebar_rel_x, sidebar_rel_y) {
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // 点击已确认在侧边栏区域内，即使未命中任何交互元素也应消费事件，
    // 防止穿透到编辑器区域导致光标被错误设置
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// SSH 管理面板按钮点击（连接/编辑/删除/添加/保存/取消）。
unsafe fn lbd_ssh_manager_panel(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<LRESULT> {
    // 操作按钮（连接/编辑/删除）
    if lbd_ssh_manager_buttons(hwnd, state, mouse_x, mouse_y).is_some() {
        return Some(LRESULT(0));
    }
    // 添加按钮 + 保存/取消 + 回退
    let mut st = state.borrow_mut();
    let panel = &st.remote.ssh_manager_panel;
    // 检测添加按钮
    if let Some(ref rect) = panel.add_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            st.remote.ssh_manager_panel.start_add();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 检测保存/取消按钮（编辑模式）
    if panel.editing {
        if let Some(ref rect) = panel.save_btn_rect {
            if rect.contains(mouse_x, mouse_y) {
                match st.save_ssh_server_from_form() {
                    Ok(()) => st.ui.status_message = "服务器配置已保存".to_string(),
                    Err(e) => st.remote.ssh_manager_panel.error_message = Some(e),
                }
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
        if let Some(ref rect) = panel.cancel_btn_rect {
            if rect.contains(mouse_x, mouse_y) {
                st.remote.ssh_manager_panel.cancel_edit();
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// SSH 管理面板操作按钮（连接/编辑/删除/认证方式切换）。
unsafe fn lbd_ssh_manager_buttons(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();
    let panel = &st.remote.ssh_manager_panel;
    let mut clicked_btn = None;
    for &(idx, action, ref rect) in &panel.item_btn_rects {
        if rect.contains(mouse_x, mouse_y) {
            clicked_btn = Some((idx, action));
            break;
        }
    }
    let (idx, action) = clicked_btn?;
    if idx < 997 {
        match action {
            0 => {
                if st.is_ssh_connected(idx) {
                    st.disconnect_ssh();
                } else {
                    st.connect_ssh_server(idx);
                }
            }
            1 => {
                if let Some(config) = st.ssh_servers().get(idx).cloned() {
                    st.remote.ssh_manager_panel.start_edit(idx, &config);
                }
            }
            2 => st.delete_ssh_server(idx),
            _ => {}
        }
    } else if idx == 997 {
        st.remote.ssh_manager_panel.start_add();
    } else if idx == 998 {
        match action {
            0 => match st.save_ssh_server_from_form() {
                Ok(()) => st.ui.status_message = "服务器配置已保存".to_string(),
                Err(e) => st.remote.ssh_manager_panel.error_message = Some(e),
            },
            1 => st.remote.ssh_manager_panel.cancel_edit(),
            _ => {}
        }
    } else if idx == 999 {
        st.remote.ssh_manager_panel.cycle_auth_type();
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 右侧面板点击（智能体模式：文件编辑区；开发者模式：AI 面板）。
pub(super) unsafe fn lbd_right_panel(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let right_panel_region = layout.right_panel_region();
    if !(layout.right_panel_visible && right_panel_region.contains(mouse_x, mouse_y)) {
        return None;
    }

    // 智能体模式：右侧面板是文件编辑区，处理编辑器光标设置
    {
        let st = state.borrow();
        if st.editor_mode.is_agent() {
            drop(st);
            return lbd_agent_right_panel_editor(hwnd, state, mouse_x, mouse_y, layout);
        }
    }

    // 开发者模式：右侧面板是 AI 面板
    // 对话标签条：切换 / 关闭 / 新建 / 历史（命中区为渲染时注册的绝对坐标）
    if let Some(result) = lbd_right_panel_tabs(hwnd, state, mouse_x, mouse_y) {
        return Some(result);
    }
    // 思考过程块：点击标题折叠/展开（命中区为渲染时注册的绝对坐标）
    {
        let hit = {
            let st = state.borrow();
            st.ai
                .ai_panel
                .reasoning_toggle_regions
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
                })
                .map(|(i, ..)| *i)
        };
        if let Some(i) = hit {
            let mut st = state.borrow_mut();
            if let Some(msg) = st.ai.ai_panel.messages.get_mut(i) {
                msg.reasoning_collapsed = !msg.reasoning_collapsed;
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 先检测输入框和按钮点击，如果命中则直接返回（不取消聚焦）
    if let Some(result) =
        lbd_right_panel_apply_input(hwnd, state, mouse_x, mouse_y, &right_panel_region)
    {
        return Some(result);
    }

    // 检测代码块保存按钮点击
    // 简化实现：检测是否在消息区域右侧的代码块保存按钮位置
    let rp_rel_x = mouse_x - right_panel_region.x;
    let rp_rel_y = mouse_y - right_panel_region.y;
    let margin = 10.0f32;
    let content_right = right_panel_region.width - margin;
    let save_btn_w = 60.0f32;
    let save_btn_x = content_right - save_btn_w - 4.0;

    // 遍历消息查找代码块位置
    {
        let st = state.borrow();
        let chat_top = 52.0f32; // 标题 + 分隔线后的起始位置
        let _chat_bottom = right_panel_region.height - 80.0f32; // 输入框上方
        let mut msg_y = chat_top - st.ai.ai_panel.scroll_y;
        let seg_pad = 6.0f32;
        let msg_gap = 12.0f32;
        let seg_gap = 4.0f32;
        let label_h = 14.0f32;

        for msg in &st.ai.ai_panel.messages {
            if msg.role == crate::ai_panel::AiRole::System {
                continue;
            }
            let is_user = msg.role == crate::ai_panel::AiRole::User;
            let is_tool = msg.role == crate::ai_panel::AiRole::Tool;
            // Tool 消息无角色标签行
            if !is_tool {
                msg_y += label_h;
            }

            // 按 ``` 代码围栏拆分
            let mut segments: Vec<(bool, String)> = Vec::new();
            {
                let mut in_code = false;
                let mut buf: Vec<&str> = Vec::new();
                for line in msg.content.lines() {
                    if line.trim_start().starts_with("```") {
                        if !buf.is_empty() {
                            segments.push((in_code, buf.join("\n")));
                            buf.clear();
                        }
                        in_code = !in_code;
                        continue;
                    }
                    buf.push(line);
                }
                if !buf.is_empty() {
                    segments.push((in_code, buf.join("\n")));
                }
            }

            for (is_code, seg_text) in &segments {
                // 估算段高度（简化）
                let line_count = seg_text.lines().count().max(1);
                let seg_h = if *is_code {
                    (line_count as f32 * 16.0 + seg_pad * 2.0).max(30.0)
                } else {
                    (line_count as f32 * 16.0 + seg_pad * 2.0).max(20.0)
                };

                // 检查是否在视口内且是代码块
                if *is_code && !is_user && !seg_text.is_empty() {
                    let save_btn_y = msg_y + 2.0;
                    let save_btn_h = 18.0f32;
                    if rp_rel_y >= save_btn_y
                        && rp_rel_y < save_btn_y + save_btn_h
                        && rp_rel_x >= save_btn_x
                        && rp_rel_x < save_btn_x + save_btn_w
                    {
                        // 点击了保存按钮 - 先收集需要的信息，然后释放借用
                        let code_to_save = seg_text.clone();
                        let suggested_name = msg
                            .content
                            .lines()
                            .find(|l| {
                                l.trim_start().starts_with("```")
                                    && !l.trim_start().starts_with("```\n")
                            })
                            .and_then(crate::ai_panel::AiPanel::extract_filename_from_fence);
                        drop(st);
                        let mut st_mut = state.borrow_mut();
                        match st_mut.save_ai_code_block(&code_to_save, suggested_name.as_deref()) {
                            Ok(path) => {
                                st_mut.ui.status_message = format!("已保存: {}", path.display());
                            }
                            Err(e) => {
                                st_mut.ui.status_message = format!("保存失败: {}", e);
                            }
                        }
                        drop(st_mut);
                        invalidate_window(hwnd);
                        return Some(LRESULT(0));
                    }
                }
                msg_y += seg_h + seg_gap;
            }
            msg_y += msg_gap;
        }
    }

    // 模式切换 / 上下文附件 / 变更列表按钮（基于渲染时注册的绝对坐标命中区）
    if lbd_right_panel_ai_controls(hwnd, state, mouse_x, mouse_y).is_some() {
        return Some(LRESULT(0));
    }
    // C-10: 点击 AI 面板非输入框/按钮区域时取消输入框聚焦
    {
        let mut st = state.borrow_mut();
        st.ai.ai_panel.input_focused = false;
        drop(st);
    }
    // 点击已确认在 AI 面板区域内，即使未命中任何交互元素也应消费事件，
    // 防止穿透到编辑器区域导致光标被错误设置
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// AI 面板：对话标签条点击（历史按钮 / 关闭标签 / 切换标签 / 新建对话 / 历史条目）。
/// 命中区为渲染时注册的绝对窗口坐标，直接用 mouse_x/mouse_y 测试。
unsafe fn lbd_right_panel_tabs(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<LRESULT> {
    let hit = |regions: &[(usize, f32, f32, f32, f32)]| -> Option<usize> {
        regions
            .iter()
            .find(|(_, rx, ry, rw, rh)| {
                mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
            })
            .map(|(i, ..)| *i)
    };
    // 1. 历史记录按钮：打开/关闭历史浮窗
    {
        let mut st = state.borrow_mut();
        if let Some((hx, hy, hw, hh)) = st.ai.ai_panel.history_button_region {
            if mouse_x >= hx && mouse_x < hx + hw && mouse_y >= hy && mouse_y < hy + hh {
                if st.ai.ai_panel.history_open {
                    st.ai.ai_panel.close_history_window();
                } else {
                    st.ai.ai_panel.open_history_window();
                    st.refresh_ai_history();
                }
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }
    // 2.2 Playbook 策略库按钮：切换管理面板
    {
        let mut st = state.borrow_mut();
        if let Some((px, py, pw, ph)) = st.ai.ai_panel.playbook_button_region {
            if mouse_x >= px && mouse_x < px + pw && mouse_y >= py && mouse_y < py + ph {
                st.ai.ai_panel.toggle_playbook_panel();
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }
    // 2.3 Playbook 条目删除按钮（需二次确认）
    {
        let del_hit = {
            let st = state.borrow();
            if st.ai.ai_panel.playbook_open {
                hit(&st.ai.ai_panel.playbook_delete_regions)
            } else {
                None
            }
        };
        if let Some(i) = del_hit {
            let mut st = state.borrow_mut();
            let content = st
                .ai
                .ai_panel
                .playbook_items
                .get(i)
                .map(|b| b.content.clone())
                .unwrap_or_default();
            let msg = format!("确定删除这条策略吗？\n\n{}", content);
            if crate::dialogs::Dialogs::confirm_yes_no(hwnd, "删除策略条目", &msg) {
                if let Err(e) = st.ai.ai_panel.delete_playbook_item(i) {
                    st.ui.status_message = format!("删除策略失败: {}", e);
                }
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 3. 关闭标签（优先于切换，避免点 × 时误切换）
    {
        let close_hit = {
            let st = state.borrow();
            hit(&st.ai.ai_panel.tab_close_regions)
        };
        if let Some(i) = close_hit {
            let mut st = state.borrow_mut();
            st.ai.ai_panel.snapshot_active_into_slot();
            st.ai.ai_panel.close_conversation(i);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 4. 切换标签
    {
        let tab_hit = {
            let st = state.borrow();
            hit(&st.ai.ai_panel.tab_regions)
        };
        if let Some(i) = tab_hit {
            state.borrow_mut().ai.ai_panel.switch_to(i);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 5. 新建对话
    {
        let mut st = state.borrow_mut();
        if let Some((px, py, pw, ph)) = st.ai.ai_panel.new_tab_region {
            if mouse_x >= px && mouse_x < px + pw && mouse_y >= py && mouse_y < py + ph {
                st.ai.ai_panel.new_conversation();
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }
    None
}

/// AI 面板：模式切换 / 上下文附件切换 / 浏览文件夹。
///
/// 使用渲染时注册的绝对坐标命中区（mode_button_regions / attachment_chip_regions），
/// 与旧的硬编码坐标处理器互不冲突。
unsafe fn lbd_right_panel_ai_controls(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<LRESULT> {
    // 1. 模式切换按钮
    {
        let mut st = state.borrow_mut();
        if let Some(mode) = st.ai.ai_panel.hit_test_mode_button(mouse_x, mouse_y) {
            st.ai.ai_panel.mode = mode;
            st.ui.status_message = format!("AI 模式：{}", mode.label());
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 2. 上下文附件切换按钮（索引 → toggleable_attachments）
    {
        let mut st = state.borrow_mut();
        if let Some(i) = st.ai.ai_panel.hit_test_attachment(mouse_x, mouse_y) {
            let items = crate::ai_panel::AiPanel::toggleable_attachments();
            if let Some(att) = items.get(i) {
                let att = att.clone();
                st.ai.ai_panel.toggle_attachment(att);
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 3. "浏览并选择文件夹"按钮
    {
        let hit = {
            let st = state.borrow();
            if let Some((bx, by, bw, bh)) = st.ai.ai_panel.browse_folder_region {
                mouse_x >= bx && mouse_x < bx + bw && mouse_y >= by && mouse_y < by + bh
            } else {
                false
            }
        };
        if hit {
            // 弹出系统文件夹选择对话框
            if let Some(path) = Dialogs::open_folder_dialog(hwnd, "选择工作区文件夹") {
                // 信任检查在 borrow_mut 之前（模态框泵消息，避免 RefCell 重入 panic）
                if crate::editor::files::check_workspace_trust(hwnd, &path) {
                    let mut st = state.borrow_mut();
                    st.open_folder(path.clone());
                    st.ui.status_message = format!("已打开: {}", path.display());
                    drop(st);
                } else {
                    state.borrow_mut().ui.status_message = "已取消打开不受信任的工作区".to_string();
                }
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    None
}

/// AI 面板 Apply 按钮 + 输入框点击。
unsafe fn lbd_right_panel_apply_input(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    right_panel_region: &crate::layout::Region,
) -> Option<LRESULT> {
    let rp_rel_x = mouse_x - right_panel_region.x;
    let rp_rel_y = mouse_y - right_panel_region.y;
    let margin = 10.0;
    let input_margin = 8.0;

    // ===== "继续生成" 按钮 =====
    if let Some((bx, by, bw, bh)) = state.borrow().ai.ai_panel.continue_button_region {
        if rp_rel_x >= bx && rp_rel_x < bx + bw && rp_rel_y >= by && rp_rel_y < by + bh {
            let mut st = state.borrow_mut();
            let ai_settings = st.ui.app_settings.ai.clone();
            match st.ai.ai_panel.continue_truncated_generation(&ai_settings) {
                Ok(_) => {
                    st.ui.status_message = "继续生成中...".to_string();
                    invalidate_window(hwnd);
                }
                Err(e) => st.ui.status_message = format!("无法继续：{}", e),
            }
            return Some(LRESULT(0));
        }
    }

    // ===== "重试" 按钮 =====
    if let Some((bx, by, bw, bh)) = state.borrow().ai.ai_panel.retry_button_region {
        if rp_rel_x >= bx && rp_rel_x < bx + bw && rp_rel_y >= by && rp_rel_y < by + bh {
            let mut st = state.borrow_mut();
            let ai_settings = st.ui.app_settings.ai.clone();
            match st.ai.ai_panel.retry_last_request(&ai_settings) {
                Ok(_) => {
                    st.ui.status_message = "重试中...".to_string();
                    invalidate_window(hwnd);
                }
                Err(e) => st.ui.status_message = format!("无法重试：{}", e),
            }
            return Some(LRESULT(0));
        }
    }

    // ===== 文件卡片：点击标题行切换展开/折叠预览（窗口绝对坐标）=====
    if let Some((mi, bi)) = state
        .borrow()
        .ai
        .ai_panel
        .hit_test_file_card(mouse_x, mouse_y)
    {
        let mut st = state.borrow_mut();
        st.ai.ai_panel.toggle_file_card_expand(mi, bi);
        st.win.dirty_tracker.mark_full_window();
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // ===== 当前模型下拉（底部工具栏，在对话框内切换当前使用的模型）=====
    // 几何与 render_ai_assistant_sidebar 中的模型按钮/弹层保持一致（相对右侧面板坐标）。
    {
        let toolbar_y = right_panel_region.height - 30.0;
        let toolbar_h = 26.0f32;
        let model_btn_x = margin + input_margin;
        let model_btn_w = 140.0f32;

        let menu_open = state.borrow().ai.ai_panel.model_menu_open;
        if menu_open {
            // 收集已启用模型（可作为"当前使用"的候选）
            let models: Vec<(String, String)> = {
                let st = state.borrow();
                st.ui
                    .app_settings
                    .ai_models
                    .iter()
                    .filter(|m| m.enabled)
                    .map(|m| {
                        let label = if !m.display_name.is_empty() {
                            m.display_name.clone()
                        } else if !m.model.is_empty() {
                            m.model.clone()
                        } else {
                            "(未命名模型)".to_string()
                        };
                        (m.id.clone(), label)
                    })
                    .collect()
            };
            if !models.is_empty() {
                let item_h = 30.0f32;
                let menu_w = model_btn_w.max(200.0);
                let menu_x = model_btn_x;
                let menu_bottom = toolbar_y - 4.0;
                let menu_h = models.len() as f32 * item_h + 8.0;
                let menu_top = menu_bottom - menu_h;
                // 命中某个菜单项 → 切换为当前使用的模型并持久化
                if rp_rel_x >= menu_x
                    && rp_rel_x < menu_x + menu_w
                    && rp_rel_y >= menu_top
                    && rp_rel_y < menu_bottom
                {
                    let idx = ((rp_rel_y - (menu_top + 4.0)) / item_h).floor() as i32;
                    if idx >= 0 && (idx as usize) < models.len() {
                        let (id, label) = models[idx as usize].clone();
                        let mut st = state.borrow_mut();
                        st.ui.app_settings.active_model_id = Some(id.clone());
                        // 同步设置面板高亮（设置页已加载时生效）
                        st.ui.settings_panel.active_model_id = Some(id);
                        match st.ui.app_settings.save() {
                            Ok(_) => st.ui.status_message = format!("已切换当前模型：{}", label),
                            Err(e) => st.ui.status_message = format!("切换模型失败：{}", e),
                        }
                        st.ai.ai_panel.model_menu_open = false;
                        invalidate_window(hwnd);
                        return Some(LRESULT(0));
                    }
                }
            }
            // 菜单展开时点击其它区域 → 收起
            state.borrow_mut().ai.ai_panel.model_menu_open = false;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 命中模型按钮 → 展开下拉
        if rp_rel_x >= model_btn_x
            && rp_rel_x < model_btn_x + model_btn_w
            && rp_rel_y >= toolbar_y
            && rp_rel_y < toolbar_y + toolbar_h
        {
            state.borrow_mut().ai.ai_panel.model_menu_open = true;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // ===== 输入框区域（新设计：参考图样式，支持自适应高度）=====
    // 使用动态计算的输入框高度
    let input_computed_height = state.borrow().ai.ai_panel.input_computed_height;
    let input_area_h = input_computed_height + 44.0f32;
    let input_y = right_panel_region.height - input_area_h;
    let text_input_y = input_y + 6.0; // 中间文本输入区域
    let text_input_h = input_computed_height;

    // 检测是否在输入框区域内（先检测输入框，避免被其他按钮逻辑覆盖）
    if rp_rel_y >= text_input_y
        && rp_rel_y < text_input_y + text_input_h
        && rp_rel_x >= margin + input_margin
        && rp_rel_x < right_panel_region.width - margin - input_margin
    {
        let mut st = state.borrow_mut();
        // 扩写期间锁定输入框，不允许聚焦编辑
        if st.ai.ai_panel.is_expanding {
            drop(st);
            return Some(LRESULT(0));
        }
        st.ai.ai_panel.input_focused = true;
        st.ai.ai_panel.caret_visible = true;
        // 若终端持有焦点则释放，否则低层钩子/字符路由会把按键继续送入终端
        if st.terminal.terminal_panel.focused {
            st.terminal.terminal_panel.focused = false;
            st.set_terminal_ime_bypass(false);
        }
        // 释放空状态搜索框焦点，避免键盘路由仍指向它
        if st.browser.empty_search_focused {
            st.browser.reset_empty_search();
        }
        // 点击输入框时将光标移到末尾
        st.ai.ai_panel.caret_pos = st.ai.ai_panel.input.len();
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
            hwnd,
            crate::window::CARET_TIMER_ID,
            530,
            None,
        );
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 底部工具栏按钮检测（发送按钮等）
    let toolbar_sep_y = input_y + input_area_h - 34.0;
    let toolbar_y = toolbar_sep_y + 4.0;
    let _toolbar_h = 26.0f32;

    // 发送/中断按钮（根据生成状态切换）
    let send_btn_size = 24.0f32;
    let right_btn_area_x = right_panel_region.width - margin - input_margin;
    let send_btn_x = right_btn_area_x - send_btn_size;
    let send_btn_y = toolbar_y + 1.0;
    if rp_rel_x >= send_btn_x
        && rp_rel_x < send_btn_x + send_btn_size
        && rp_rel_y >= send_btn_y
        && rp_rel_y < send_btn_y + send_btn_size
    {
        let mut st = state.borrow_mut();

        // 如果正在生成，则触发中断
        if st.ai.ai_panel.is_generating {
            st.ai.ai_panel.stop_generation();
            st.ui.status_message = "已中断 AI 生成".to_string();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }

        // 否则发送消息（使用当前模式 + 编辑器上下文，与 Enter 键行为一致，
        // 以便 Agent 模式收到工具指令并输出 FILE/RUN 标记）
        let settings = st.ui.app_settings.active_ai_settings();
        let mode = st.ai.ai_panel.mode;
        let attachments = st.ai.ai_panel.attachments.clone();
        let context = st.gather_context(&attachments);
        if let Err(e) = st
            .ai
            .ai_panel
            .send_message_with_prepared_context(&settings, context, mode)
        {
            st.ui.status_message = e;
        } else {
            st.ui.status_message = "AI 请求已发送".to_string();
            let _ = SetTimer(hwnd, AI_TIMER_ID, AI_REFRESH_MS, None);
        }
        st.ai.ai_panel.input_focused = false;
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 星星按钮（问题扩写）——发送按钮左侧
    let star_btn_size = 24.0f32;
    let star_btn_x = send_btn_x - star_btn_size - 4.0;
    if rp_rel_x >= star_btn_x
        && rp_rel_x < star_btn_x + star_btn_size
        && rp_rel_y >= send_btn_y
        && rp_rel_y < send_btn_y + star_btn_size
    {
        let mut st = state.borrow_mut();
        let settings = st.ui.app_settings.active_ai_settings();
        match st.ai.ai_panel.expand_input(&settings) {
            Ok(msg) => {
                st.ui.status_message = msg;
                let _ = SetTimer(hwnd, AI_TIMER_ID, AI_REFRESH_MS, None);
            }
            Err(e) => {
                st.ui.status_message = e;
            }
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 停止生成按钮（当正在生成时显示）
    let is_gen = state.borrow().ai.ai_panel.is_generating;
    if is_gen {
        let stop_w = 96.0f32;
        let stop_x = margin + input_margin;
        let stop_y = input_y + 4.0; // 在输入框卡片上方区域
        let stop_h = 26.0f32;
        if rp_rel_x >= stop_x
            && rp_rel_x < stop_x + stop_w
            && rp_rel_y >= stop_y
            && rp_rel_y < stop_y + stop_h
        {
            state.borrow_mut().ai.ai_panel.stop_generation();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    None
}

/// 设置页点击（导航标签 / 字段聚焦 / 下拉选择 / 保存 / 测试连接）。
///
/// 设置页在经典模式渲染于编辑区、智能体模式渲染于右面板内容区，
/// `region` 为实际渲染区域；各命中区由 render_settings_sidebar 以绝对坐标注册，此处直接命中测试。
pub(super) unsafe fn lbd_settings_page(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    region: crate::layout::Region,
) -> Option<LRESULT> {
    {
        let st = state.borrow();
        if !st.active_tab_is_settings() {
            return None;
        }
    }
    if !region.contains(mouse_x, mouse_y) {
        return None;
    }

    let mut st = state.borrow_mut();

    // 1. 下拉展开时，优先处理选项点击
    if st.ui.settings_panel.open_dropdown.is_some() {
        if let Some((kind, idx)) = st
            .ui
            .settings_panel
            .hit_test_dropdown_item(mouse_x, mouse_y)
        {
            match kind {
                crate::settings::SettingsDropdownKind::Provider => {
                    st.ui.settings_panel.select_provider_by_index(idx);
                }
                crate::settings::SettingsDropdownKind::Model => {
                    st.ui.settings_panel.select_model_by_index(idx);
                }
            }
            st.ui.settings_panel.open_dropdown = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // 2. 下拉触发区 → 开/关（以当前状态切换）
    if let Some(kind) = st
        .ui
        .settings_panel
        .hit_test_dropdown_trigger(mouse_x, mouse_y)
    {
        let opening = st.ui.settings_panel.open_dropdown != Some(kind);
        st.ui.settings_panel.open_dropdown = if opening { Some(kind) } else { None };
        st.ui.settings_panel.active_field = None;
        // 打开「模型」下拉时，自动从 /models 拉取该厂商当前可用模型
        if opening && kind == crate::settings::SettingsDropdownKind::Model {
            st.ui.settings_panel.ensure_models_fetched();
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 3. 若下拉处于展开且点击了别处 → 先关闭，再继续尝试其它命中
    if st.ui.settings_panel.open_dropdown.is_some() {
        st.ui.settings_panel.open_dropdown = None;
    }

    // 4. 导航标签切换
    if let Some(tab) = st.ui.settings_panel.hit_test_tab(mouse_x, mouse_y) {
        st.ui.settings_panel.active_tab = tab;
        st.ui.settings_panel.active_field = None;
        // 切换导航时退出模型编辑态，返回「模型」页默认展示列表
        st.ui.settings_panel.model_editing = false;
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 4b. 外观页：任务栏显示开关
    if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Appearance {
        if let Some((rx, ry, rw, rh)) = st.ui.settings_panel.taskbar_toggle_region {
            if mouse_x >= rx && mouse_x < rx + rw && mouse_y >= ry && mouse_y < ry + rh {
                st.ui.app_settings.ui.show_taskbar_when_maximized =
                    !st.ui.app_settings.ui.show_taskbar_when_maximized;
                let enabled = st.ui.app_settings.ui.show_taskbar_when_maximized;
                match st.ui.app_settings.save() {
                    Ok(_) => {
                        st.ui.status_message = if enabled {
                            "已开启：最大化时显示任务栏".to_string()
                        } else {
                            "已关闭：最大化时隐藏任务栏".to_string()
                        };
                    }
                    Err(e) => {
                        st.ui.status_message = format!("保存设置失败：{}", e);
                    }
                }
                // 若当前处于最大化状态，重新触发最大化以应用新的任务栏设置
                let is_max = st.win.is_maximized;
                let show_taskbar = st.ui.app_settings.ui.show_taskbar_when_maximized;
                drop(st);
                if is_max {
                    use windows::Win32::Graphics::Gdi::{
                        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
                    };
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
                    };

                    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                    let mut mi = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                        if show_taskbar {
                            // 显示任务栏：窗口设置为工作区大小（不包含任务栏）
                            let _ = SetWindowPos(
                                hwnd,
                                windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                                mi.rcWork.left,
                                mi.rcWork.top,
                                mi.rcWork.right - mi.rcWork.left,
                                mi.rcWork.bottom - mi.rcWork.top,
                                SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        } else {
                            // 隐藏任务栏：窗口设置为显示器全尺寸
                            let _ = SetWindowPos(
                                hwnd,
                                windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                                mi.rcMonitor.left,
                                mi.rcMonitor.top,
                                mi.rcMonitor.right - mi.rcMonitor.left,
                                mi.rcMonitor.bottom - mi.rcMonitor.top,
                                SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }

    // 4b2. 通用页：默认启动模式切换（点击切换 开发者/智能体，立即持久化，重启后生效）
    if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::General {
        if let Some((rx, ry, rw, rh)) = st.ui.settings_panel.default_mode_toggle_region {
            if mouse_x >= rx && mouse_x < rx + rw && mouse_y >= ry && mouse_y < ry + rh {
                let to_agent = st.ui.app_settings.ui.editor_mode != "agent";
                st.ui.app_settings.ui.editor_mode =
                    if to_agent { "agent" } else { "developer" }.to_string();
                match st.ui.app_settings.save() {
                    Ok(_) => {
                        st.ui.status_message = if to_agent {
                            "默认启动模式已设为：智能体模式（重启后生效）".to_string()
                        } else {
                            "默认启动模式已设为：开发者模式（重启后生效）".to_string()
                        };
                    }
                    Err(e) => {
                        st.ui.status_message = format!("保存设置失败：{}", e);
                    }
                }
                invalidate_window(hwnd);
                return Some(LRESULT(0));
            }
        }
    }

    // 4c. 更新页："立即检查更新"按钮（独立于 AI/模型页的按钮检测门槛）
    if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Update {
        if let Some(crate::settings::SettingsButton::CheckUpdate) =
            st.ui.settings_panel.hit_test_button(mouse_x, mouse_y)
        {
            let hwnd_val = st.win.hwnd;
            crate::updater::start_check(hwnd_val, true);
            st.ui.status_message = "正在检查更新...".to_string();
            st.ui.update_checking = true;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // 5. 模型编辑表单（AI 配置）：显隐按钮 / 温度滑块 / 保存 / 测试连接 / 返回列表
    // AI 配置已并入「模型」页，仅在 model_editing 编辑态下展示与响应。
    if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Ai
        || (st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Models
            && st.ui.settings_panel.model_editing)
    {
        // API 密钥显隐切换
        if st
            .ui
            .settings_panel
            .hit_test_api_key_toggle(mouse_x, mouse_y)
        {
            st.ui.settings_panel.toggle_api_key_visibility();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 深度思考开关切换（DeepSeek 专属）
        if st
            .ui
            .settings_panel
            .hit_test_thinking_toggle(mouse_x, mouse_y)
        {
            st.ui.settings_panel.toggle_thinking();
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 思考强度分段切换（DeepSeek 思考模式专属）
        if let Some(effort) = st.ui.settings_panel.hit_test_effort(mouse_x, mouse_y) {
            st.ui.settings_panel.reasoning_effort = effort.to_string();
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 温度滑块：点击轨道即定位，并进入拖拽
        if let Some(v) = st.ui.settings_panel.hit_test_temp_slider(mouse_x, mouse_y) {
            st.ui.settings_panel.temperature = format!("{:.1}", v);
            st.ui.settings_panel.temp_slider_dragging = true;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // Top-p 滑块：点击轨道即定位，并进入拖拽
        if let Some(v) = st.ui.settings_panel.hit_test_top_p_slider(mouse_x, mouse_y) {
            st.ui.settings_panel.top_p = format!("{:.2}", v);
            st.ui.settings_panel.top_p_slider_dragging = true;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 开发者参数区：标题点击展开/折叠
        if st
            .ui
            .settings_panel
            .hit_test_dev_params_toggle(mouse_x, mouse_y)
        {
            st.ui.settings_panel.dev_params_expanded = !st.ui.settings_panel.dev_params_expanded;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 响应格式分段切换（文本 / JSON）
        if let Some(fmt) = st
            .ui
            .settings_panel
            .hit_test_response_format(mouse_x, mouse_y)
        {
            st.ui.settings_panel.response_format = fmt.to_string();
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // logprobs 调试开关切换
        if st
            .ui
            .settings_panel
            .hit_test_logprobs_toggle(mouse_x, mouse_y)
        {
            st.ui.settings_panel.logprobs = !st.ui.settings_panel.logprobs;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 流式用量统计开关切换
        if st
            .ui
            .settings_panel
            .hit_test_include_usage_toggle(mouse_x, mouse_y)
        {
            st.ui.settings_panel.include_usage = !st.ui.settings_panel.include_usage;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 频率惩罚滑块：点击轨道即定位，并进入拖拽
        if let Some(v) = st.ui.settings_panel.hit_test_freq_slider(mouse_x, mouse_y) {
            st.ui.settings_panel.frequency_penalty = format!("{:.1}", v);
            st.ui.settings_panel.freq_slider_dragging = true;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 存在惩罚滑块：点击轨道即定位，并进入拖拽
        if let Some(v) = st.ui.settings_panel.hit_test_pres_slider(mouse_x, mouse_y) {
            st.ui.settings_panel.presence_penalty = format!("{:.1}", v);
            st.ui.settings_panel.pres_slider_dragging = true;
            st.ui.settings_panel.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        if let Some(btn) = st.ui.settings_panel.hit_test_button(mouse_x, mouse_y) {
            match btn {
                crate::settings::SettingsButton::BackToModels => {
                    // 返回模型列表：不保存（草稿/未保存编辑将被丢弃，只有「保存」才写入）
                    st.ui.settings_panel.model_editing = false;
                }
                crate::settings::SettingsButton::Save => {
                    st.save_ai_settings_with_test();
                }
                crate::settings::SettingsButton::TestConnection => {
                    st.start_ai_test_connection();
                }
                crate::settings::SettingsButton::CheckUpdate => {
                    // 手动触发更新检查
                    let hwnd_val = st.win.hwnd;
                    crate::updater::start_check(hwnd_val, true);
                    st.ui.status_message = "正在检查更新...".to_string();
                }
            }
            let started_test = st.ui.settings_panel.is_testing;
            st.ui.settings_panel.active_field = None;
            drop(st);
            // 测试期间启动后台刷新定时器，结果到达后自动停止
            if started_test {
                let _ = SetTimer(hwnd, AI_TIMER_ID, AI_REFRESH_MS, None);
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // 5b. 模型管理页（列表视图）：模型按钮点击
    // 命中区由 render_models_management 以绝对坐标注册（原点为 editor_content_region），
    // 因此这里必须用绝对 mouse_x/mouse_y 命中测试，与标签栏 / 编辑表单保持一致。
    if st.ui.settings_panel.active_tab == crate::settings::SettingsTab::Models
        && !st.ui.settings_panel.model_editing
    {
        if let Some((btn, model_id)) = st.ui.settings_panel.hit_test_model_button(mouse_x, mouse_y)
        {
            match btn {
                crate::settings::ModelButton::Add => {
                    // 新建：进入空白草稿编辑表单，但不加入列表、不持久化；
                    // 只有点击「保存」（连接验证通过）后才会真正创建并保存该模型。
                    st.ui.settings_panel.begin_new_model_draft();
                    st.ui.settings_panel.model_editing = true;
                }
                crate::settings::ModelButton::Edit => {
                    // 编辑：加载该模型字段进表单，不持久化；改动只有点击「保存」后才写入。
                    let fallback = st.ui.app_settings.ai.clone();
                    st.ui.settings_panel.active_model_id = Some(model_id.clone());
                    st.ui.settings_panel.load_active_model_fields(&fallback);
                    st.ui.settings_panel.model_editing = true;
                }
                crate::settings::ModelButton::Delete => {
                    st.ui.settings_panel.delete_model(&model_id);
                    if st.ui.settings_panel.active_model_id.is_none() {
                        st.ui.settings_panel.active_model_id =
                            st.ui.settings_panel.models.first().map(|m| m.id.clone());
                    }
                    let fallback = st.ui.app_settings.ai.clone();
                    st.ui.settings_panel.load_active_model_fields(&fallback);
                    st.persist_models();
                }
                crate::settings::ModelButton::ToggleEnabled => {
                    st.ui.settings_panel.toggle_model_enabled(&model_id);
                    st.persist_models();
                }
                crate::settings::ModelButton::Eval => {
                    // 从该模型的配置生成 AiSettings 并跳转到沙盒评测页
                    let model_name = st
                        .ui
                        .settings_panel
                        .models
                        .iter()
                        .find(|m| m.id == model_id)
                        .map(|m| {
                            if m.display_name.is_empty() {
                                m.name.clone()
                            } else {
                                m.display_name.clone()
                            }
                        })
                        .unwrap_or_else(|| model_id.clone());
                    st.ui.sandbox_eval.target_model_id = Some(model_id.clone());
                    st.ui.sandbox_eval.target_model_name = Some(model_name);
                    st.ui.sandbox_eval.reset_for_new_round();
                    st.open_sandbox_eval_tab();
                }
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        // 点击模型卡片 → 设为激活模型
        if let Some(model_id) = st.ui.settings_panel.hit_test_model_item(mouse_x, mouse_y) {
            st.ui.settings_panel.selected_model_id = Some(model_id.clone());
            let fallback = st.ui.app_settings.ai.clone();
            st.ui.settings_panel.set_active_model(&model_id, &fallback);
            st.persist_models();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // 6. 输入字段聚焦
    if let Some(field) = st.ui.settings_panel.hit_test_field(mouse_x, mouse_y) {
        st.ui.settings_panel.active_field = Some(field);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 7. 点击设置区空白 → 清除聚焦与下拉，消费点击
    st.ui.settings_panel.active_field = None;
    st.ui.settings_panel.open_dropdown = None;
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 标签栏点击。
///
/// 重要：处理关闭按钮时，**不能**在 `borrow_mut()` 持有期间弹出模态确认对话框。
/// `MessageBoxW` / `TaskDialog` 等模态对话框会启动自己的消息循环，期间会派发
/// `WM_PAINT` / `WM_KILLFOCUS` 等消息，这些消息处理函数会再次尝试 `borrow_mut()`，
/// 触发 `RefCell already borrowed` panic，导致应用卡死。
///
/// 解决方案：先在 `borrow()` 下完成所有点击检测，drop borrow 后再执行；
/// 关闭路径额外提取 dirty 状态信息，drop borrow 后弹窗，确认后重新
/// `borrow_mut()` 执行关闭。
pub(super) unsafe fn lbd_tab_bar(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    _layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    // 阶段 1：在 borrow_mut() 内处理拖拽预备（标签体命中 → 延迟切换）
    {
        let mut st = state.borrow_mut();
        // 智能体模式下标签栏渲染在右侧面板顶部，需使用实际区域
        let tab_region = st.effective_tab_bar_region();
        if !tab_region.contains(mouse_x, mouse_y) {
            return None;
        }
        if let Some(tab_idx) = st.tab_body_hit_test(mouse_x, mouse_y, tab_region.x, tab_region.y) {
            st.editor.tab_bar.tab_drag_start = Some((mouse_x as i32, mouse_y as i32));
            st.editor.tab_bar.hover_tab = Some(tab_idx);
            return Some(LRESULT(0));
        }
    }

    // 阶段 2：在 borrow() 下检测点击类型（不修改 state）
    enum TabBarAction {
        CloseTab {
            index: usize,
            is_dirty: bool,
            file_name: String,
        },
        SwitchTab(usize),
    }
    let action: Option<TabBarAction> = {
        let st = state.borrow();
        let show_tab_bar = st.show_tab_bar();
        if !show_tab_bar {
            return None;
        }
        let tab_region = st.effective_tab_bar_region();
        let editor_x = tab_region.x;

        // "+" 新建按钮
        if let Some((pl, pt, pr, pb)) = st.editor.tab_bar.plus_button_rect {
            if mouse_x >= pl && mouse_x < pr && mouse_y >= pt && mouse_y < pb {
                // 先释放不可变借用，再调用 handle_new_tab（内部会 borrow_mut）
                drop(st);
                return handle_new_tab(state, hwnd);
            }
        }

        if mouse_x < editor_x {
            return None;
        }

        // 遍历 tab_layouts 检测关闭按钮或 tab 体
        let rel_x = mouse_x - editor_x + st.editor.tab_bar.tab_scroll_x;
        let mut found: Option<TabBarAction> = None;
        for layout_entry in &st.editor.tab_bar.tab_layouts {
            if rel_x >= layout_entry.x && rel_x < layout_entry.x + layout_entry.width {
                // 关闭按钮
                if rel_x >= layout_entry.close_x
                    && rel_x < layout_entry.close_x + layout_entry.close_width
                {
                    let index = layout_entry.index;
                    let is_active = index == st.editor.tab_bar.active_tab;
                    let (is_dirty, file_name) = if is_active {
                        let dirty = st.editor.content.is_dirty;
                        let name = st
                            .editor
                            .content
                            .file_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "未命名".to_string());
                        (dirty, name)
                    } else {
                        let dirty = st
                            .editor
                            .tab_bar
                            .tabs
                            .get(index)
                            .map(|t| t.is_dirty())
                            .unwrap_or(false);
                        let name = st
                            .editor
                            .tab_bar
                            .tabs
                            .get(index)
                            .and_then(|t| t.file_path())
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "未命名".to_string());
                        (dirty, name)
                    };
                    found = Some(TabBarAction::CloseTab {
                        index,
                        is_dirty,
                        file_name,
                    });
                    break;
                }
                // tab 体（不在关闭按钮区域）→ 立即切换
                found = Some(TabBarAction::SwitchTab(layout_entry.index));
                break;
            }
        }
        found
    };

    // 阶段 3：drop borrow 后执行
    match action {
        Some(TabBarAction::CloseTab {
            index,
            is_dirty,
            file_name,
        }) => {
            if is_dirty {
                let msg = format!("{} 有未保存的修改，是否保存并关闭？", file_name);
                // 弹窗在 borrow 释放后进行 → 不触发 RefCell panic
                let confirmed = crate::dialogs::Dialogs::confirm_yes_no(hwnd, "关闭标签页", &msg);
                if !confirmed {
                    state.borrow_mut().ui.status_message = "已取消关闭".to_string();
                    invalidate_window(hwnd);
                    return Some(LRESULT(0));
                }
            }
            // 弹窗结束后重新 borrow_mut() 执行关闭
            state.borrow_mut().close_tab(index);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        Some(TabBarAction::SwitchTab(idx)) => {
            state.borrow_mut().switch_tab(idx);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        None => {}
    }
    None
}

/// 新建标签页（保持 borrow_mut() 持有时间最短）。
unsafe fn handle_new_tab(state: &Rc<RefCell<EditorState>>, hwnd: HWND) -> Option<LRESULT> {
    {
        let mut st = state.borrow_mut();
        st.new_tab();
        st.ui.status_message = "已新建标签页".to_string();
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 查找替换面板点击。
pub(super) unsafe fn lbd_find_panel(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();
    if !st.editor.find.visible {
        return None;
    }
    let show_tab_bar = st.show_tab_bar();
    let editor_region = layout.editor_content_region(show_tab_bar);
    let panel_height = if st.editor.find.replace_visible {
        72.0
    } else {
        40.0
    };
    let panel_width = editor_region.width.min(600.0);
    let panel_x = editor_region.x + editor_region.width - panel_width - 10.0;
    let panel_y = editor_region.y;
    if !(mouse_x >= panel_x
        && mouse_x < panel_x + panel_width
        && mouse_y >= panel_y
        && mouse_y < panel_y + panel_height)
    {
        return None;
    }
    let input_h = 24.0;
    let input_w = panel_width - 120.0;
    let find_y = panel_y + 8.0;
    let find_input_x = panel_x + 50.0;
    let find_input_w = input_w;
    if mouse_x >= find_input_x
        && mouse_x < find_input_x + find_input_w
        && mouse_y >= find_y
        && mouse_y < find_y + input_h
    {
        st.editor.find.focus = crate::editor::FindReplaceFocus::FindQuery;
    } else if st.editor.find.replace_visible {
        let replace_y = panel_y + 8.0 + input_h + 8.0;
        let replace_input_x = panel_x + 50.0;
        let replace_input_w = input_w;
        if mouse_x >= replace_input_x
            && mouse_x < replace_input_x + replace_input_w
            && mouse_y >= replace_y
            && mouse_y < replace_y + input_h
        {
            st.editor.find.focus = crate::editor::FindReplaceFocus::ReplaceText;
        }
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 底部面板点击（经典模式入口）。
pub(super) unsafe fn lbd_bottom_panel(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let bottom_panel_region = layout.bottom_panel_region();
    if !bottom_panel_region.contains(mouse_x, mouse_y) {
        return None;
    }
    lbd_terminal_view(hwnd, state, mouse_x, mouse_y, bottom_panel_region, false)
}

/// 终端视图点击：经典模式底部面板与智能体模式右面板终端标签共用。
/// `region` 为终端视图实际渲染区域（内部标签条/关闭按钮几何相对其计算）；
/// `agent_tab` 表示智能体模式终端标签（关闭按钮改为关闭标签页）。
unsafe fn lbd_terminal_view(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    bottom_panel_region: crate::layout::Region,
    agent_tab: bool,
) -> Option<LRESULT> {
    tracing::info!(
        mx = mouse_x,
        my = mouse_y,
        agent_tab = agent_tab,
        running = state.borrow().terminal.terminal_panel.running,
        tab = ?state.borrow().terminal.bottom_panel_tab,
        "lbd_terminal_view: 点击终端视图"
    );

    // 标签栏 hit test：与 render_bottom_panel 中标签的坐标布局完全一致
    // (tab_height=28, tab_w=60, 间距=4, 起始 x=region.x+10)
    // 命中则切换 bottom_panel_tab，不进入终端 focus 逻辑。
    let tab_height: f32 = 28.0;
    let tab_w: f32 = 60.0;
    let tab_gap: f32 = 4.0;
    let tab_start_x = bottom_panel_region.x + 10.0;
    let tab_top = bottom_panel_region.y + 2.0;
    let tab_bottom = tab_top + tab_height - 2.0;
    if mouse_y >= tab_top && mouse_y <= tab_bottom {
        let rel_x = mouse_x - tab_start_x;
        if rel_x >= 0.0 {
            let step = tab_w + tab_gap;
            let idx_f = rel_x / step;
            let idx = idx_f as i32;
            // 必须在标签的 x 范围内（防止落在 tab 间隙时误判）
            let in_tab_x = (rel_x - idx_f * step) < tab_w;
            if idx >= 0 && in_tab_x {
                let tab = match idx {
                    0 => BottomPanelTab::Terminal,
                    1 => BottomPanelTab::Problems,
                    _ => return None,
                };
                let mut st = state.borrow_mut();
                if st.terminal.bottom_panel_tab != tab {
                    tracing::info!(?tab, "lbd_bottom_panel: 切换底部面板 tab");
                    st.terminal.bottom_panel_tab = tab;
                    // 切到问题面板时取消终端 focus，避免低层钩子继续拦截 Backspace
                    if tab == BottomPanelTab::Problems && st.terminal.terminal_panel.focused {
                        st.terminal.terminal_panel.focused = false;
                        st.set_terminal_ime_bypass(false);
                    }
                    invalidate_window(hwnd);
                }
                return Some(LRESULT(0));
            }
        }
    }

    // 检测点击关闭按钮（×）—— 与 render_bottom_panel 中绘制的位置保持一致
    const CLOSE_BTN_SIZE: f32 = 28.0;
    const TITLE_BAR_H: f32 = 30.0;
    let close_btn_x = bottom_panel_region.x + bottom_panel_region.width - CLOSE_BTN_SIZE;
    let close_btn_y = bottom_panel_region.y;
    if mouse_x >= close_btn_x
        && mouse_x <= close_btn_x + CLOSE_BTN_SIZE
        && mouse_y >= close_btn_y
        && mouse_y <= close_btn_y + TITLE_BAR_H
    {
        let mut st = state.borrow_mut();
        if agent_tab {
            // 智能体模式：关闭按钮关闭终端标签页
            tracing::info!("lbd_terminal_view: 智能体模式关闭终端标签");
            st.terminal.terminal_panel.focused = false;
            st.set_terminal_ime_bypass(false);
            st.close_current_tab();
        } else {
            tracing::info!("lbd_bottom_panel: 点击关闭按钮，关闭底部面板");
            st.ui.layout.toggle_terminal_panel();
            st.terminal.terminal_panel.focused = false;
            st.set_terminal_ime_bypass(false);
        }
        let _ = KillTimer(hwnd, super::super::super::TERM_TIMER_ID);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 点击在标签栏之外但在底部面板内：
    // - 终端 tab：focus 终端，自动启动
    // - 问题 tab：暂不响应（问题项点击后续在问题面板内部实现）
    if state.borrow().terminal.bottom_panel_tab != BottomPanelTab::Terminal {
        return Some(LRESULT(0));
    }

    let mut st = state.borrow_mut();
    st.terminal.terminal_panel.focused = true;
    st.set_terminal_ime_bypass(true);
    // 如果终端未运行，点击时自动启动
    if !st.terminal.terminal_panel.running {
        tracing::info!("lbd_bottom_panel: 终端未运行，自动启动");
        let _ = st.terminal.terminal_panel.start();
    }
    // 确保刷新定时器在运行（覆盖从按钮打开/关闭后定时器可能未启动的情况）
    let _ = SetTimer(
        hwnd,
        super::super::super::TERM_TIMER_ID,
        super::super::super::TERM_REFRESH_MS,
        None,
    );
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 欢迎页 / 编辑器区域点击。
pub(super) unsafe fn lbd_welcome_or_editor(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let welcome_x = 0.0;
    let welcome_width = {
        let st = state.borrow();
        st.win.window_width as f32
    };
    let welcome_y = layout.top_offset();
    let welcome_height = {
        let st = state.borrow();
        st.win.window_height as f32
            - welcome_y
            - if layout.status_bar_visible {
                layout.status_bar_height
            } else {
                0.0
            }
            - if layout.bottom_panel_visible {
                layout.bottom_panel_height
            } else {
                0.0
            }
    };
    let welcome_region =
        crate::layout::Region::new(welcome_x, welcome_y, welcome_width, welcome_height);
    if !welcome_region.contains(mouse_x, mouse_y) {
        return None;
    }
    let mut st = state.borrow_mut();
    // 智能体模式：中间区域是AI对话面板，不是编辑器，跳过编辑器光标设置
    if st.editor_mode.is_agent() {
        // 智能体模式下，AI 对话面板渲染在编辑器内容区域（中间面板），
        // 输入框/发送按钮/模型下拉等命中检测需基于中间面板坐标（而非右侧面板）
        // 智能体模式标签栏在右侧面板，中间区域不预留标签栏高度（与渲染一致）
        let center_panel_region = layout.editor_content_region(false);
        drop(st);
        if center_panel_region.contains(mouse_x, mouse_y) {
            if let Some(result) =
                lbd_right_panel_apply_input(hwnd, state, mouse_x, mouse_y, &center_panel_region)
            {
                return Some(result);
            }
        }
        // 未命中按钮/输入框时消费事件，防止穿透到编辑器区域
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    // 新标签页（NTP）：快捷搜索框 + 快捷操作按钮，委托共享点击逻辑
    if st.ntp_active() {
        drop(st);
        return lbd_new_tab_page(hwnd, state, mouse_x, mouse_y, layout);
    }
    if st.show_welcome() {
        let action = st.handle_welcome_click(
            mouse_x,
            mouse_y,
            welcome_x,
            welcome_y,
            welcome_width,
            welcome_height,
        );
        if let Some(action) = action {
            drop(st);
            lbd_welcome_action(hwnd, state, action);
            return Some(LRESULT(0));
        }
    } else {
        // Markdown 预览切换按钮命中检测（优先于编辑器光标设置）
        if st.editor.content.language == aether_core::lexer::Language::Markdown {
            if let Some(btn) = &st.editor.markdown_toggle_btn {
                if btn.contains(mouse_x, mouse_y) {
                    st.toggle_markdown_preview();
                    invalidate_window(hwnd);
                    return Some(LRESULT(0));
                }
            }
        }
        // Markdown 预览模式下点击编辑区不设置光标（预览不可编辑）
        if st.editor.content.language == aether_core::lexer::Language::Markdown
            && st.editor.markdown_preview
        {
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
        let editor_content = layout.editor_content_region(st.show_tab_bar());
        st.set_cursor_from_mouse(mouse_x, mouse_y, editor_content.x, editor_content.y);
        // 若终端持有焦点则释放，避免低层钩子继续拦截按键
        if st.terminal.terminal_panel.focused {
            st.terminal.terminal_panel.focused = false;
            st.set_terminal_ime_bypass(false);
        }
        // 单击只设置光标位置，不启动选区
        // 选区在鼠标移动时（WM_MOUSEMOVE + is_dragging）启动
        st.clear_selection();
        st.editor.is_selecting = false;
        // 重置光标闪烁状态并启动定时器
        st.editor.content.caret_visible = true;
        let _ = SetTimer(hwnd, crate::window::CARET_TIMER_ID, 530, None);
        // 标记编辑区+状态栏脏区：避免无脏区退化为全窗口无裁剪重绘，
        // 点击落光标只需重绘编辑内容与状态栏行列信息
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
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }
    None
}

/// 欢迎页点击动作执行
unsafe fn lbd_welcome_action(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    action: crate::welcome::WelcomeAction,
) {
    match action {
        crate::welcome::WelcomeAction::OpenFolder => {
            if let Some(path) = Dialogs::open_folder_dialog(hwnd, "打开文件夹") {
                // 信任检查在 borrow_mut 之前（模态框泵消息，避免 RefCell 重入 panic）
                if crate::editor::files::check_workspace_trust(hwnd, &path) {
                    state.borrow_mut().open_folder(path);
                } else {
                    state.borrow_mut().ui.status_message = "已取消打开不受信任的工作区".to_string();
                }
                invalidate_window(hwnd);
            }
        }
        crate::welcome::WelcomeAction::NewProject => {
            state.borrow_mut().new_project();
            invalidate_window(hwnd);
        }
        crate::welcome::WelcomeAction::CloneRepo => {
            state.borrow_mut().remote.clone_dialog.visible = true;
            state.borrow_mut().remote.clone_dialog.reset();
            invalidate_window(hwnd);
        }
        crate::welcome::WelcomeAction::OpenRemote => {
            state.borrow_mut().remote.ssh_dialog.visible = true;
            state.borrow_mut().remote.ssh_dialog.reset();
            invalidate_window(hwnd);
        }
        crate::welcome::WelcomeAction::OpenRecentProject(path_str) => {
            let path = PathBuf::from(path_str);
            // 信任检查在 borrow_mut 之前（模态框泵消息，避免 RefCell 重入 panic）
            if crate::editor::files::check_workspace_trust(hwnd, &path) {
                state.borrow_mut().open_folder(path);
            } else {
                state.borrow_mut().ui.status_message = "已取消打开不受信任的工作区".to_string();
            }
            invalidate_window(hwnd);
        }
        crate::welcome::WelcomeAction::MoreRecentProjects => {
            if let Some(path) = Dialogs::open_folder_dialog(hwnd, "打开文件夹") {
                // 信任检查在 borrow_mut 之前（模态框泵消息，避免 RefCell 重入 panic）
                if crate::editor::files::check_workspace_trust(hwnd, &path) {
                    state.borrow_mut().open_folder(path);
                } else {
                    state.borrow_mut().ui.status_message = "已取消打开不受信任的工作区".to_string();
                }
                invalidate_window(hwnd);
            }
        }
    }
}

/// 智能体沙盒评测页点击（字段聚焦 / 参数选择 / 开始 / 终止 / 打分 / 导出）。
///
/// 沙盒评测页渲染在编辑区，各命中区由 render_sandbox_eval_page 以绝对坐标注册。
pub(super) unsafe fn lbd_sandbox_page(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    {
        let st = state.borrow();
        if !st.active_tab_is_sandbox_eval() {
            return None;
        }
    }
    let editor_region = layout.editor_region();
    if !editor_region.contains(mouse_x, mouse_y) {
        return None;
    }

    use crate::sandbox_eval::rect_hit;

    let mut st = state.borrow_mut();
    let regions = st.ui.sandbox_eval.regions.clone();

    // ---- 主题输入框聚焦 ----
    if let Some(r) = regions.topic_field {
        if rect_hit(&r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.active_field = Some(crate::sandbox_eval::SandboxField::Topic);
            st.ui.sandbox_eval.caret_visible = true;
            let _ = SetTimer(hwnd, crate::window::CARET_TIMER_ID, 530, None);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 自定义任务数量输入框聚焦 ----
    if let Some(r) = regions.custom_count_field {
        if rect_hit(&r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.active_field = Some(crate::sandbox_eval::SandboxField::CustomCount);
            st.ui.sandbox_eval.caret_visible = true;
            let _ = SetTimer(hwnd, crate::window::CARET_TIMER_ID, 530, None);
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 任务数量 chip ----
    for (n, r) in &regions.agent_chips {
        if rect_hit(r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.agent_count = *n;
            st.ui.sandbox_eval.custom_count.clear();
            st.ui.sandbox_eval.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 模式 chip ----
    for (i, r) in &regions.mode_chips {
        if rect_hit(r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.mode = if *i == 1 {
                crate::sandbox_eval::SandboxMode::Timed
            } else {
                crate::sandbox_eval::SandboxMode::Untimed
            };
            st.ui.sandbox_eval.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 时长 chip ----
    for (mins, r) in &regions.duration_chips {
        if rect_hit(r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.duration_min = *mins;
            st.ui.sandbox_eval.active_field = None;
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 开始评测 ----
    if let Some(r) = regions.start_button {
        if rect_hit(&r, mouse_x, mouse_y) {
            let app = st.ui.app_settings.clone();
            match st.ui.sandbox_eval.start(&app) {
                Ok(()) => {
                    st.ui.status_message = "沙盒评测已开始".to_string();
                    let _ = SetTimer(
                        hwnd,
                        crate::window::SANDBOX_TIMER_ID,
                        crate::window::SANDBOX_REFRESH_MS,
                        None,
                    );
                }
                Err(e) => {
                    st.ui.sandbox_eval.error = Some(e);
                }
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 终止评测 ----
    if let Some(r) = regions.stop_button {
        if rect_hit(&r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.stop();
            st.ui.status_message = "沙盒评测已终止".to_string();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 打分 chip ----
    for (task_idx, score, r) in &regions.score_chips {
        if rect_hit(r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.set_score(*task_idx, *score);
            if let Some(avg) = st.ui.sandbox_eval.average_score() {
                st.ui.status_message = format!("当前平均分 {:.1} / 10", avg);
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 打包导出 ----
    if let Some(r) = regions.export_button {
        if rect_hit(&r, mouse_x, mouse_y) {
            let topic = st.ui.sandbox_eval.topic.trim().to_string();
            drop(st);
            let default_name = format!(
                "沙盒评测-{}.zip",
                if topic.is_empty() {
                    "结果".to_string()
                } else {
                    crate::sandbox_eval::truncate_chars(&topic, 20).replace('…', "")
                }
            );
            let dest = Dialogs::save_file_dialog(hwnd, "打包导出评测结果", &default_name);
            let mut st = state.borrow_mut();
            if let Some(dest) = dest {
                match st.ui.sandbox_eval.export(&dest) {
                    Ok(msg) => {
                        st.ui.sandbox_eval.export_message = Some(msg.clone());
                        st.ui.status_message = msg;
                    }
                    Err(e) => {
                        st.ui.sandbox_eval.export_message = Some(format!("导出失败: {}", e));
                        st.ui.status_message = format!("导出失败: {}", e);
                    }
                }
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 再来一轮 ----
    if let Some(r) = regions.restart_button {
        if rect_hit(&r, mouse_x, mouse_y) {
            st.ui.sandbox_eval.reset_for_new_round();
            st.ui.status_message = "已重置，可开始新一轮评测".to_string();
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // ---- 打开沙盒目录 ----
    if let Some(r) = regions.open_dir_button {
        if rect_hit(&r, mouse_x, mouse_y) {
            if let Some(dir) = st.ui.sandbox_eval.run_dir.clone() {
                let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
            }
            drop(st);
            return Some(LRESULT(0));
        }
    }

    // 点击空白处：取消输入焦点
    if st.ui.sandbox_eval.active_field.is_some() {
        st.ui.sandbox_eval.active_field = None;
        invalidate_window(hwnd);
    }
    Some(LRESULT(0))
}

/// 历史对话浮动窗口点击（全局最顶层，优先于所有面板/编辑器）。
///
/// 命中优先级：关闭按钮 > 搜索框 > 时间标签 > 条目删除 > 条目（单击恢复/双击编辑）
/// > 分页 > 清空 > 标题栏拖动 > 浮窗体（消费点击防穿透）> 浮窗外部（关闭浮窗）。
/// 返回 Some 表示已消费点击；浮窗未打开时返回 None。
pub(super) unsafe fn lbd_history_window(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<LRESULT> {
    // 浮窗未打开：不处理
    if !state.borrow().ai.ai_panel.history_open {
        return None;
    }
    let hit = |regions: &[(usize, f32, f32, f32, f32)]| -> Option<usize> {
        regions
            .iter()
            .find(|(_, rx, ry, rw, rh)| {
                mouse_x >= *rx && mouse_x < *rx + *rw && mouse_y >= *ry && mouse_y < *ry + *rh
            })
            .map(|(i, ..)| *i)
    };
    let in_region = |region: Option<(f32, f32, f32, f32)>| -> bool {
        region
            .map(|(rx, ry, rw, rh)| {
                mouse_x >= rx && mouse_x < rx + rw && mouse_y >= ry && mouse_y < ry + rh
            })
            .unwrap_or(false)
    };

    // 1. 关闭按钮
    if in_region(state.borrow().ai.ai_panel.history_win_close_region) {
        state.borrow_mut().ai.ai_panel.close_history_window();
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 2. 搜索框：聚焦（与 AI 输入框互斥）
    if in_region(state.borrow().ai.ai_panel.history_search_region) {
        let mut st = state.borrow_mut();
        st.ai.ai_panel.history_search_focused = true;
        st.ai.ai_panel.input_focused = false;
        st.ai.ai_panel.history_editing_id = None; // 点击搜索框时退出标题编辑
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 3. 时间筛选标签
    if let Some(fi) = hit(&state.borrow().ai.ai_panel.history_time_filter_regions) {
        let mut st = state.borrow_mut();
        if let Some(f) = crate::ai_panel::HistoryTimeFilter::ALL.get(fi).copied() {
            st.ai.ai_panel.set_history_time_filter(f);
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 4. 条目删除按钮（需二次确认；优先于条目点击）
    if let Some(i) = hit(&state.borrow().ai.ai_panel.history_delete_regions) {
        let mut st = state.borrow_mut();
        let title = st
            .ai
            .ai_panel
            .history
            .get(i)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        let msg = format!("确定删除这条历史对话吗？\n\n{}", title);
        if Dialogs::confirm_yes_no(hwnd, "删除历史记录", &msg) {
            if let Err(e) = st.ai.ai_panel.delete_history_item(i) {
                st.ui.status_message = format!("删除历史失败: {}", e);
            }
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 5. 条目：单击恢复会话 / 双击进入标题编辑
    if let Some(i) = hit(&state.borrow().ai.ai_panel.history_item_regions) {
        let conv_id = {
            let st = state.borrow();
            st.ai.ai_panel.history.get(i).map(|m| m.id.clone())
        };
        if let Some(conv_id) = conv_id {
            let is_double = state
                .borrow_mut()
                .ai
                .ai_panel
                .history_click_or_double(&conv_id);
            if is_double {
                // 双击：进入标题编辑态
                state.borrow_mut().ai.ai_panel.begin_history_edit(i);
                invalidate_window(hwnd);
            } else {
                // 单击：恢复会话为活动标签页，并关闭浮窗
                let mut st = state.borrow_mut();
                st.ai.ai_panel.restore_from_history(i);
                st.ai.ai_panel.close_history_window();
                invalidate_window(hwnd);
            }
        }
        return Some(LRESULT(0));
    }

    // 6. 分页：上一页 / 下一页
    {
        let page_dir = {
            let st = state.borrow();
            if in_region(st.ai.ai_panel.history_page_prev_region) {
                Some(-1i32)
            } else if in_region(st.ai.ai_panel.history_page_next_region) {
                Some(1i32)
            } else {
                None
            }
        };
        if let Some(dir) = page_dir {
            let mut st = state.borrow_mut();
            if dir < 0 {
                st.ai.ai_panel.history_prev_page();
            } else {
                st.ai.ai_panel.history_next_page();
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }

    // 7. 清空全部（需二次确认）
    if in_region(state.borrow().ai.ai_panel.history_clear_all_region) {
        let mut st = state.borrow_mut();
        let count = st.ai.ai_panel.history.len();
        let msg = format!("确定清空全部 {} 条历史对话吗？\n\n此操作不可恢复。", count);
        if Dialogs::confirm_yes_no(hwnd, "清空历史记录", &msg) {
            match st.ai.ai_panel.clear_all_history() {
                Ok(n) => st.ui.status_message = format!("已清空 {} 条历史记录", n),
                Err(e) => st.ui.status_message = format!("清空历史失败: {}", e),
            }
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 8. 标题栏：开始拖动（记录鼠标相对浮窗左上角的偏移）
    if in_region(state.borrow().ai.ai_panel.history_win_titlebar_region) {
        let mut st = state.borrow_mut();
        // 计算当前浮窗实际位置（可能与渲染时默认居中一致）
        let (win_w, win_h) = st.ai.ai_panel.history_win_size;
        let (px, py) = st.ai.ai_panel.history_win_pos.unwrap_or_else(|| {
            (
                ((st.win.window_width as f32 - win_w) / 2.0).max(0.0),
                ((st.win.window_height as f32 - win_h) / 2.0).max(0.0),
            )
        });
        st.ai.ai_panel.history_win_pos = Some((px, py));
        st.ai.ai_panel.history_win_drag = Some((mouse_x - px, mouse_y - py));
        drop(st);
        // 按下标题栏开始拖动：设置握紧手形光标
        let hcursor = LoadCursorW(None, IDC_SIZEALL).unwrap_or_default();
        let _ = SetCursor(hcursor);
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 9. 浮窗体其他区域（列表留白等）：消费点击防穿透
    if in_region(state.borrow().ai.ai_panel.history_win_region) {
        // 点击浮窗内非交互区：仅取消搜索/编辑焦点
        let mut st = state.borrow_mut();
        if st.ai.ai_panel.history_search_focused || st.ai.ai_panel.history_editing_id.is_some() {
            st.ai.ai_panel.history_search_focused = false;
            // 编辑态点击空白：提交编辑（类 VS Code 行为）
            if st.ai.ai_panel.history_editing_id.is_some() {
                let _ = st.ai.ai_panel.commit_history_edit();
            }
            invalidate_window(hwnd);
        }
        return Some(LRESULT(0));
    }

    // 10. 浮窗外部：关闭浮窗
    {
        let mut st = state.borrow_mut();
        st.ai.ai_panel.close_history_window();
        drop(st);
        invalidate_window(hwnd);
        // 返回 None 让点击继续传递给下层（如编辑器），符合浮窗"点击外部关闭但不拦截"的惯例
        return None;
    }
}

/// 新标签页（NTP）点击：快捷搜索框聚焦 / 打开本地文件 / 新建文件 / 打开终端。
///
/// 经典模式与智能体模式共用；几何取自 new_tab_page_region + new_tab_page_geom_in，
/// 与渲染/光标/IME 保持一致。
unsafe fn lbd_new_tab_page(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();
    let search_box = crate::render::new_tab_page_geom_in(st.new_tab_page_region(layout));
    let btn_height = crate::render::NEW_TAB_BTN_HEIGHT;
    let btn_gap = crate::render::NEW_TAB_BTN_GAP;
    for i in 0..crate::render::NEW_TAB_BTN_COUNT {
        let btn_y = search_box.y + i as f32 * (btn_height + btn_gap);
        let rect = crate::layout::Region::new(search_box.x, btn_y, search_box.width, btn_height);
        if rect.contains(mouse_x, mouse_y) {
            match i {
                0 => {
                    // 快捷搜索框：聚焦后可直接输入，回车即搜（无需先进浏览器）
                    if !st.browser.empty_search_focused {
                        st.browser.empty_search_focused = true;
                        st.browser.empty_search_caret_visible = true;
                    }
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                        hwnd,
                        crate::window::CARET_TIMER_ID,
                        530,
                        None,
                    );
                }
                1 => {
                    // 打开本地文件：模态对话框需在释放借用后弹出，避免与对话框消息循环重入冲突
                    st.browser.reset_empty_search();
                    drop(st);
                    if let Some(path) =
                        crate::dialogs::Dialogs::open_file_dialog(hwnd, "打开文件", &[])
                    {
                        state.borrow_mut().load_file(path);
                    }
                }
                2 => {
                    // 新建文件：原标签栏 + 按钮的行为（新建空白文件标签）
                    st.browser.reset_empty_search();
                    st.new_file_tab();
                    st.ui.status_message = "已新建文件".to_string();
                }
                3 => {
                    // 打开终端：智能体模式新建终端标签，经典模式切换底部终端面板
                    st.browser.reset_empty_search();
                    if st.editor_mode.is_agent() {
                        st.open_terminal_tab();
                    } else {
                        st.ui.layout.toggle_terminal_panel();
                    }
                }
                _ => {}
            }
            invalidate_window(hwnd);
            return Some(LRESULT(0));
        }
    }
    // 点击新标签页其他区域：释放搜索框焦点
    if st.browser.empty_search_focused {
        st.browser.reset_empty_search();
    }
    invalidate_window(hwnd);
    Some(LRESULT(0))
}

/// 智能体模式右侧文件编辑区点击。
///
/// 智能体模式下，右侧面板是文件编辑区（标签页容器），需要处理编辑器光标设置。
unsafe fn lbd_agent_right_panel_editor(
    hwnd: HWND,
    state: &Rc<RefCell<EditorState>>,
    mouse_x: f32,
    mouse_y: f32,
    layout: &crate::layout::LayoutManager,
) -> Option<LRESULT> {
    let mut st = state.borrow_mut();

    // 标签栏区域（右侧面板顶部条带）交由 lbd_tab_bar 处理，这里不消费
    if !st.editor.tab_bar.tabs.is_empty() {
        let tab_region = st.effective_tab_bar_region();
        if tab_region.contains(mouse_x, mouse_y) {
            return None;
        }
    }

    // 新标签页（NTP）：快捷搜索框 + 快捷操作按钮，委托共享点击逻辑
    if st.ntp_active() {
        drop(st);
        return lbd_new_tab_page(hwnd, state, mouse_x, mouse_y, layout);
    }

    // 浏览器标签：仅工具栏需要点击处理（网页正文由 WebView2 子窗口原生接收）
    if st.active_tab_is_browser() {
        if let (Some(id), Some(tl)) = (st.active_browser_id(), st.browser.toolbar_layout.clone()) {
            if tl.back.contains(mouse_x, mouse_y) {
                if let Some(inst) = st.browser.get(id) {
                    inst.go_back();
                }
            } else if tl.forward.contains(mouse_x, mouse_y) {
                if let Some(inst) = st.browser.get(id) {
                    inst.go_forward();
                }
            } else if tl.refresh.contains(mouse_x, mouse_y) {
                if let Some(inst) = st.browser.get(id) {
                    inst.reload();
                }
            } else if tl.address.contains(mouse_x, mouse_y) {
                // 进入地址栏编辑模式（回车导航/Esc 取消，见键盘路由）
                if let Some(inst) = st.browser.get_mut(id) {
                    inst.address_editing = true;
                }
            }
        }
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 终端标签：委托终端视图点击逻辑（区域 = 右面板内容区，与渲染几何一致）
    if st.active_tab_is_terminal() {
        let rp = layout.right_panel_region();
        let tab_h = if st.editor.tab_bar.tabs.is_empty() {
            0.0
        } else {
            crate::layout::TAB_BAR_HEIGHT
        };
        let term_region =
            crate::layout::Region::new(rp.x, rp.y + tab_h, rp.width, rp.height - tab_h);
        drop(st);
        return lbd_terminal_view(hwnd, state, mouse_x, mouse_y, term_region, true);
    }

    // 设置标签：委托设置页点击逻辑（区域与渲染几何一致，含导航/字段/按钮等全部交互）
    if st.active_tab_is_settings() {
        let region = st.settings_page_region(layout);
        drop(st);
        return lbd_settings_page(hwnd, state, mouse_x, mouse_y, region);
    }

    // 沙盒评测标签：有自己的点击处理逻辑，这里直接消费事件
    if st.active_tab_is_sandbox_eval() {
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // Markdown 预览模式下点击编辑区不设置光标（预览不可编辑）
    if st.editor.content.language == aether_core::lexer::Language::Markdown
        && st.editor.markdown_preview
    {
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 图片预览模式下不设置光标
    if st.editor.content.language == aether_core::lexer::Language::Image {
        invalidate_window(hwnd);
        return Some(LRESULT(0));
    }

    // 计算右侧面板的内容区域（排除标签栏）
    let right_panel_region = layout.right_panel_region();
    let tab_bar_height = if st.show_tab_bar() {
        crate::layout::TAB_BAR_HEIGHT
    } else {
        0.0
    };
    let content_x = right_panel_region.x;
    let content_y = right_panel_region.y + tab_bar_height;

    // 设置编辑器光标
    st.set_cursor_from_mouse(mouse_x, mouse_y, content_x, content_y);
    // 单击只设置光标位置，不启动选区
    st.clear_selection();
    st.editor.is_selecting = false;
    // 重置光标闪烁状态并启动定时器
    st.editor.content.caret_visible = true;
    let _ = SetTimer(hwnd, crate::window::CARET_TIMER_ID, 530, None);

    // 标记编辑区+状态栏脏区
    st.win.dirty_tracker.mark_region(
        content_x,
        content_y,
        right_panel_region.width,
        right_panel_region.height - tab_bar_height,
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
    invalidate_window(hwnd);
    Some(LRESULT(0))
}
