//! `WM_RBUTTONDOWN` / `WM_RBUTTONUP` 处理：标签 + 资源管理器空白区域上下文菜单。
//!
//! 标签右键：当用户在标签栏的某个标签上右键点击时弹出标签上下文菜单。
//! 资源管理器空白区域：当用户在侧边栏文件树的空白区域（未命中任何文件/文件夹节点，
//! 也未命中标题栏的新建按钮）右键点击时，弹出上下文菜单。
//! 在其他区域右键点击时，关闭已打开的上下文菜单。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use super::super::{get_and_set_state, invalidate_window};
use crate::activity_bar_context_menu::ActivityBarContextMenuState;
use crate::layout::SidebarContent;
use crate::tab_context_menu::TabContextMenuState;

/// WM_RBUTTONDOWN
pub(crate) unsafe fn on_r_button_down(
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
    let (mouse_x, mouse_y, window_w, window_h) = {
        let st = state.borrow();
        (
            raw_x / st.dpi_scale,
            raw_y / st.dpi_scale,
            st.window_width as f32,
            st.window_height as f32,
        )
    };

    let mut st = state.borrow_mut();

    // 记录用户右键那一帧的树节点列表起始 Y（根目录行之下，含内联输入框偏移）。
    // 必须在 cancel_file_tree_input 之前读取：用户是按屏幕上已渲染的
    //（带输入框、节点整体下移）布局点击的，若取消后再算偏移会错位约两行。
    let nodes_start_y = st.file_tree_nodes_start_y();

    // 若正在内联输入，右键先取消输入（与左键逻辑一致）
    if st.file_tree_input.is_some() {
        st.cancel_file_tree_input();
    }

    // SubTask 9.3: 标签右键——检测是否命中标签栏的某个标签
    let show_tab_bar = st.show_tab_bar();
    let tab_region = st.layout.tab_bar_region(show_tab_bar);
    if show_tab_bar && tab_region.contains(mouse_x, mouse_y) {
        if let Some(tab_idx) = st.tab_body_hit_test(mouse_x, mouse_y, tab_region.x, tab_region.y) {
            // 获取该标签的 file_path（用于判断 has_path 和复制路径）
            let has_path = st
                .tab_bar
                .tabs
                .get(tab_idx)
                .and_then(|t| t.file_path())
                .is_some();
            let mut menu = TabContextMenuState::build_for_tab(tab_idx, has_path);
            menu.open_at(mouse_x, mouse_y, window_w, window_h);
            st.context_menus.tab = menu;
            // 关闭可能打开的资源管理器菜单，避免重叠
            if st.context_menus.explorer.is_open {
                st.context_menus.explorer.close();
            }
            // 菜单互斥：关闭活动栏菜单
            if st.context_menus.activity_bar.visible {
                st.context_menus.activity_bar.hide();
            }
            // 菜单开合必须走完整首帧渲染（擦除旧菜单/绘制新菜单），
            // 否则紧随其后的 hover 会以 Dialog 脏区触发快速路径，留下残影。
            st.dirty_tracker.mark_full_window();
            drop(st);
            invalidate_window(hwnd);
            return LRESULT(0);
        }
    }

    // SubTask 14.1: 活动栏右键——检测是否落在活动栏区域
    let activity_region = st.layout.activity_bar_region();
    if st.layout.activity_bar_visible
        && activity_region.width > 0.0
        && activity_region.contains(mouse_x, mouse_y)
    {
        let active_view = st.activity_view;
        let mut menu = ActivityBarContextMenuState::build(active_view);
        menu.open_at(mouse_x, mouse_y, window_w, window_h);
        st.context_menus.activity_bar = menu;
        // 菜单互斥：关闭标签菜单与资源管理器菜单
        if st.context_menus.tab.visible {
            st.context_menus.tab.hide();
        }
        if st.context_menus.explorer.is_open {
            st.context_menus.explorer.close();
        }
        st.dirty_tracker.mark_full_window();
        drop(st);
        invalidate_window(hwnd);
        return LRESULT(0);
    }

    // 仅当侧边栏可见且当前为文件树视图时，才可能弹出空白区域菜单
    let sidebar_region = st.layout.sidebar_region();
    let in_sidebar_file_tree = st.layout.sidebar_visible
        && sidebar_region.width > 0.0
        && sidebar_region.contains(mouse_x, mouse_y)
        && st.sidebar_content == SidebarContent::FileTree;

    if !in_sidebar_file_tree {
        // 在侧边栏外右键：关闭已打开的菜单
        let mut need_invalidate = false;
        if st.context_menus.explorer.is_open {
            st.context_menus.explorer.close();
            need_invalidate = true;
        }
        if st.context_menus.tab.visible {
            st.context_menus.tab.hide();
            need_invalidate = true;
        }
        if st.context_menus.activity_bar.visible {
            st.context_menus.activity_bar.hide();
            need_invalidate = true;
        }
        if need_invalidate {
            st.dirty_tracker.mark_full_window();
        }
        drop(st);
        if need_invalidate {
            invalidate_window(hwnd);
        }
        return LRESULT(0);
    }

    // 命中标题栏的新建按钮 → 不弹出空白菜单（交由左键处理）
    let on_new_file_btn = st
        .file_tree_new_file_btn
        .as_ref()
        .map(|r| r.contains(mouse_x, mouse_y))
        .unwrap_or(false);
    let on_new_folder_btn = st
        .file_tree_new_folder_btn
        .as_ref()
        .map(|r| r.contains(mouse_x, mouse_y))
        .unwrap_or(false);
    if on_new_file_btn || on_new_folder_btn {
        return LRESULT(0);
    }

    // 侧边栏内坐标（相对侧边栏左上角），与左键 handle_file_tree_click /
    // hover 路径同一坐标基准；起始 Y 统一复用 file_tree_list_start_y()，
    // 避免手写偏移公式与渲染布局漂移。
    let sidebar_rel_x = mouse_x - sidebar_region.x;
    let sidebar_rel_y = mouse_y - sidebar_region.y;
    let sidebar_width = st.layout.sidebar_width;

    // 命中文件/文件夹节点 → 弹出文件节点上下文菜单（携带节点类型，文件夹额外提供新建入口）
    // 根目录行折叠时节点不可见，跳过节点命中检测
    let hit_node: Option<(u32, bool)> = if st.file_tree_root_expanded {
        // P5-1: O(1) 行定位命中测试，替代递归遍历
        st.file_tree_hit_test(sidebar_rel_x, sidebar_rel_y, nodes_start_y, sidebar_width)
            .map(|(node_idx, kind, _)| {
                (
                    node_idx,
                    kind == aether_core::workspace::file_tree::FileKind::Directory,
                )
            })
    } else {
        None
    };

    if let Some((node_idx, is_dir)) = hit_node {
        // 节点命中：选中节点，关闭旧菜单，弹出节点级上下文菜单
        if st.context_menus.explorer.is_open {
            st.context_menus.explorer.close();
        }
        if st.context_menus.file_node.is_open {
            st.context_menus.file_node.close();
        }
        if st.context_menus.tab.visible {
            st.context_menus.tab.hide();
        }
        if st.context_menus.activity_bar.visible {
            st.context_menus.activity_bar.hide();
        }
        st.selected_file_node = Some(node_idx);
        st.emit_event(crate::events::EditorEvent::SidebarChanged);
        // 弹出文件节点上下文菜单
        st.context_menus
            .file_node
            .open(mouse_x, mouse_y, window_w, window_h, node_idx, is_dir);
        st.dirty_tracker.mark_full_window();
        drop(st);
        invalidate_window(hwnd);
        return LRESULT(0);
    }

    // 空白区域：弹出上下文菜单（菜单内部会做窗口边界校正）
    st.context_menus
        .explorer
        .open(mouse_x, mouse_y, window_w, window_h);
    // 关闭可能打开的其他菜单，避免重叠
    if st.context_menus.file_node.is_open {
        st.context_menus.file_node.close();
    }
    if st.context_menus.tab.visible {
        st.context_menus.tab.hide();
    }
    if st.context_menus.activity_bar.visible {
        st.context_menus.activity_bar.hide();
    }
    // 空白区域右键同时清除当前选中节点（符合"未选中任何文件或文件夹"语义）
    st.selected_file_node = None;
    st.emit_event(crate::events::EditorEvent::SidebarChanged);
    st.dirty_tracker.mark_full_window();
    drop(st);
    invalidate_window(hwnd);
    LRESULT(0)
}

/// WM_RBUTTONUP
///
/// 当前上下文菜单由 WM_LBUTTONDOWN 处理点击，右键抬起无需做事。
/// 保留空实现以阻止 DefWindowProc 弹出系统默认菜单。
pub(crate) unsafe fn on_r_button_up(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    LRESULT(0)
}
