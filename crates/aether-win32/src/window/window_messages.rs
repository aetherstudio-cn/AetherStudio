//! 杂项窗口消息（WM_*）处理函数。
//!
//! 从 `window.rs` 拆分而来，保持原有逻辑不变。

use std::path::PathBuf;

use aether_lsp::client::LspEvent;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, ScreenToClient, PAINTSTRUCT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{
    compute_cursor_for_pos, create_editor_window, get_and_set_state, invalidate_window,
    AI_ARCHIVE_TIMER_ID, AI_TIMER_ID, CARET_TIMER_ID, EDITOR_STATE, HOVER_TIMER_ID,
    LP_THRESHOLD_MS, LP_TIMER_ID, POWER_TIMER_ID, SANDBOX_TIMER_ID, TERM_TIMER_ID,
    TOOLTIP_TIMER_ID, UI_ANIM_TIMER_ID,
};
use crate::auto_save::{AUTOSAVE_DEBOUNCE_TIMER_ID, AUTOSAVE_PERIODIC_TIMER_ID};

/// WM_TIMER
pub(crate) unsafe fn on_timer(hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    if wparam.0 == HOVER_TIMER_ID {
        return on_timer_hover(hwnd);
    }
    if wparam.0 == TOOLTIP_TIMER_ID {
        return on_timer_tooltip(hwnd);
    }
    if wparam.0 == TERM_TIMER_ID {
        return on_timer_term_refresh(hwnd);
    }
    if wparam.0 == CARET_TIMER_ID {
        return on_timer_caret(hwnd);
    }
    if wparam.0 == AI_TIMER_ID {
        return on_timer_ai_refresh(hwnd);
    }
    if wparam.0 == LP_TIMER_ID {
        return on_timer_long_press(hwnd);
    }
    if wparam.0 == AUTOSAVE_DEBOUNCE_TIMER_ID {
        return on_timer_autosave_debounce(hwnd);
    }
    if wparam.0 == AUTOSAVE_PERIODIC_TIMER_ID {
        return on_timer_autosave_periodic(hwnd);
    }
    if wparam.0 == AI_ARCHIVE_TIMER_ID {
        return on_timer_ai_archive(hwnd);
    }
    if wparam.0 == UI_ANIM_TIMER_ID {
        return on_timer_ui_anim(hwnd);
    }
    if wparam.0 == POWER_TIMER_ID {
        return on_timer_power(hwnd);
    }
    if wparam.0 == SANDBOX_TIMER_ID {
        return on_timer_sandbox(hwnd);
    }
    LRESULT(0)
}

/// 智能体沙盒评测：评测运行期间驱动流式消费、倒计时与任务推进；
/// 评测结束（进入打分/配置阶段）后自动停止定时器。
unsafe fn on_timer_sandbox(hwnd: HWND) -> LRESULT {
    let Some(state) = get_and_set_state(hwnd) else {
        let _ = KillTimer(hwnd, SANDBOX_TIMER_ID);
        return LRESULT(0);
    };
    let (dirty, active) = {
        let mut st = state.borrow_mut();
        let app = st.ui.app_settings.clone();
        let dirty = st.ui.sandbox_eval.tick(&app);
        (dirty, st.ui.sandbox_eval.is_active())
    };
    if dirty {
        invalidate_window(hwnd);
    }
    if !active {
        let _ = KillTimer(hwnd, SANDBOX_TIMER_ID);
        // 阶段切换（如进入打分页）需要最后一帧重绘
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// 冰冻看门狗：低频检查空闲时长，满足「失焦 + 空闲超阈」时进入冰冻态释放内存
unsafe fn on_timer_power(hwnd: HWND) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        if let Ok(mut st) = state.try_borrow_mut() {
            if !st.power.frozen
                && !st.input.focus_manager.is_window_focused()
                && st.power.idle_secs() >= crate::power::IDLE_FREEZE_SECS
            {
                st.enter_frozen();
            }
        }
    }
    LRESULT(0)
}

/// UI 动画：驱动历史记录下拉面板展开/收起动画，动画结束后自动停止定时器
unsafe fn on_timer_ui_anim(hwnd: HWND) -> LRESULT {
    let animating = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow_mut().ai.ai_panel.tick_history_anim())
            .unwrap_or(false)
    });
    if !animating {
        let _ = KillTimer(hwnd, UI_ANIM_TIMER_ID);
    }
    // 无论是否结束都重绘一次，确保最后一帧（完全展开/收起）被绘制
    invalidate_window(hwnd);
    LRESULT(0)
}

/// AI 温数据归档：周期检查空闲会话并异步归档进 MemoryStore（AetherDB）
unsafe fn on_timer_ai_archive(hwnd: HWND) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        state.borrow_mut().ai.ai_panel.trigger_warm_archive();
    }
    LRESULT(0)
}

/// P3.4: Hover tooltip 防抖定时器触发
unsafe fn on_timer_hover(hwnd: HWND) -> LRESULT {
    let _ = KillTimer(hwnd, HOVER_TIMER_ID);
    if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        // 仅在仍有悬停目标时计算 tooltip
        if st.fs.hover_file_node.is_some() || st.remote.hover_node.is_some() {
            if let Some(text) = st.compute_hover_tooltip_text() {
                // tooltip 定位：鼠标右下方，预留 16px 间距
                let tx = st.input.hover.last_mouse_x + 16.0;
                let ty = st.input.hover.last_mouse_y + 16.0;
                let max_w = 400.0;
                st.input.hover.tooltip =
                    Some(crate::editor::HoverTooltip::new(text, tx, ty, max_w));
                invalidate_window(hwnd);
            }
        }
    }
    LRESULT(0)
}

/// Tooltip 延迟显示定时器触发（活动栏/标题栏按钮悬停提示）
unsafe fn on_timer_tooltip(hwnd: HWND) -> LRESULT {
    let _ = KillTimer(hwnd, TOOLTIP_TIMER_ID);
    if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        // 仅在 hover_key 仍有效且 tooltip 尚未显示时触发
        if st.ui.tooltip_state.hover_key.is_some() && st.ui.tooltip_state.visible_text.is_none() {
            let (_, tooltip_text) = st.compute_tooltip_hover_key();
            if let Some(text) = tooltip_text {
                st.ui.tooltip_state.visible_text = Some(text);
                st.ui.tooltip_state.show_pos = (
                    st.ui.tooltip_state.anchor.x as f32,
                    st.ui.tooltip_state.anchor.y as f32,
                );
                invalidate_window(hwnd);
            }
        }
    }
    LRESULT(0)
}

/// 终端刷新：周期性重绘以显示异步到达的 shell 输出。
unsafe fn on_timer_term_refresh(hwnd: HWND) -> LRESULT {
    // 后台任务泥：AI 流 / Agent 回环 / LSP 诊断 / 终端输出。
    // 此前在 render() 入口调用，导致每帧渲染前轮询 IO，影响编辑器点击响应。
    // 现移至独立 33ms 定时器，渲染路径零 IO 阻塞，流畅度显著提升。
    // 冰冻态下此定时器停止，后台任务由 AI 无头泥驱动。
    let (_still_visible, frozen) = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| {
                let st = state.borrow();
                (st.ui.layout.bottom_panel_visible, st.power.frozen)
            })
            .unwrap_or((false, false))
    });
    if frozen {
        let _ = KillTimer(hwnd, TERM_TIMER_ID);
        return LRESULT(0);
    }
    // 执行后台任务泥 + 设置面板轮询
    let need_redraw = if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        st.pump_background_tasks();
        // 设置面板轮询（测试连接 / 模型列表拉取）
        let mut dirty = false;
        match st.ui.settings_panel.poll_test_result() {
            crate::settings::TestPollResult::SuccessWithPendingSave => {
                st.save_ai_settings();
                st.ui.settings_panel.model_editing = false;
                st.win.dirty_tracker.mark_full_window();
                dirty = true;
            }
            crate::settings::TestPollResult::Success
            | crate::settings::TestPollResult::Failed
            | crate::settings::TestPollResult::FailedWithPendingSave => {
                st.win.dirty_tracker.mark_full_window();
                dirty = true;
            }
            crate::settings::TestPollResult::Pending => {}
        }
        if st.ui.settings_panel.poll_models_fetch() {
            st.win.dirty_tracker.mark_full_window();
            dirty = true;
        }
        // 文件系统监控（AI 命令后检测工作区变化）
        if let Some(until) = st.fs.fs_watch_until {
            if std::time::Instant::now() >= until {
                st.fs.fs_watch_until = None;
            } else {
                let sig = st.workspace_root_signature();
                if sig != st.fs.fs_last_root_sig {
                    st.fs.fs_last_root_sig = sig;
                    st.refresh_file_tree_light();
                    dirty = true;
                }
            }
        }
        // 懒加载预扫描
        st.preload_expanded_dirs();
        dirty || st.win.dirty_tracker.has_dirty()
    } else {
        false
    };
    // 只在后台任务产生了状态变化时触发重绘，避免空转帧
    if need_redraw {
        invalidate_window(hwnd);
    }
    // 终端不可见时不停定时器——后台任务仍需周期泥驱动
    LRESULT(0)
}

/// AI 后台刷新：流式生成或测试连接期间周期性重绘，两者均结束后自动停止。
/// 冰冻态：转为无头泵——继续消费 AI 流与 Agent 终端命令回环但不触发重绘，
/// 保证最小化/长期空闲期间 AI 生成与文件落盘不中断，全部结束后自杀。
unsafe fn on_timer_ai_refresh(hwnd: HWND) -> LRESULT {
    let Some(state) = get_and_set_state(hwnd) else {
        let _ = KillTimer(hwnd, AI_TIMER_ID);
        return LRESULT(0);
    };
    let frozen = state
        .try_borrow()
        .map(|st| st.power.frozen)
        .unwrap_or(false);
    if frozen {
        let keep = state
            .try_borrow_mut()
            .map(|mut st| {
                st.pump_background_tasks();
                st.ai.ai_panel.any_generating()
                    || st.ui.settings_panel.is_testing
                    || st.terminal.terminal_panel.has_agent_activity()
            })
            .unwrap_or(true);
        if !keep {
            let _ = KillTimer(hwnd, AI_TIMER_ID);
        }
        return LRESULT(0);
    }
    let active = state
        .try_borrow()
        .map(|st| st.ai.ai_panel.any_generating() || st.ui.settings_panel.is_testing)
        .unwrap_or(false);
    if active {
        invalidate_window(hwnd);
    } else {
        let _ = KillTimer(hwnd, AI_TIMER_ID);
    }
    LRESULT(0)
}

/// 新建项目对话框输入框光标闪烁
unsafe fn on_timer_caret(hwnd: HWND) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        let mut need_invalidate = false;
        let mut any_active = false;
        if st.ui.new_project_dialog.visible {
            st.ui.new_project_dialog.caret_visible = !st.ui.new_project_dialog.caret_visible;
            need_invalidate = true;
            any_active = true;
        }
        if st.fs.file_tree_input.is_some() {
            if let Some(input) = st.fs.file_tree_input.as_mut() {
                input.caret_visible = !input.caret_visible;
            }
            need_invalidate = true;
            any_active = true;
        }
        // AI 助手输入框光标闪烁（智能体模式中间列 / 经典模式右面板）
        if st.ai.ai_panel.input_focused {
            st.ai.ai_panel.caret_visible = !st.ai.ai_panel.caret_visible;
            st.mark_ai_panel_dirty();
            need_invalidate = true;
            any_active = true;
        }
        // 新标签页快捷搜索框光标闪烁（智能体模式右面板 / 经典模式编辑器内容区）
        if st.browser.empty_search_focused {
            st.browser.empty_search_caret_visible = !st.browser.empty_search_caret_visible;
            let ntp_region = st.new_tab_page_region(&st.ui.layout).clone();
            let region_type = if st.editor_mode.is_agent() {
                crate::dirty_rect::DirtyRegionType::RightPanel
            } else {
                crate::dirty_rect::DirtyRegionType::EditorContent
            };
            st.win.dirty_tracker.mark_region(
                ntp_region.x,
                ntp_region.y,
                ntp_region.width,
                ntp_region.height,
                region_type,
            );
            need_invalidate = true;
            any_active = true;
        }
        // 沙盒评测页输入框光标闪烁（编辑器内容区）
        if st.ui.sandbox_eval.active_field.is_some() {
            st.ui.sandbox_eval.caret_visible = !st.ui.sandbox_eval.caret_visible;
            let er = st.ui.layout.editor_region().clone();
            st.win.dirty_tracker.mark_region(
                er.x,
                er.y,
                er.width,
                er.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
            need_invalidate = true;
            any_active = true;
        }
        // 编辑器内容区光标闪烁（文件编辑状态）
        if st
            .editor
            .tab_bar
            .tabs
            .get(st.editor.tab_bar.active_tab)
            .map(|t| t.is_file())
            .unwrap_or(false)
        {
            st.editor.content.caret_visible = !st.editor.content.caret_visible;
            let er = st.ui.layout.editor_region().clone();
            st.win.dirty_tracker.mark_region(
                er.x,
                er.y,
                er.width,
                er.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
            need_invalidate = true;
            any_active = true;
        }
        // 无任何活跃输入时停止定时器，避免空转
        if !any_active {
            let _ = KillTimer(hwnd, CARET_TIMER_ID);
        }
        if need_invalidate {
            let region = st.ui.layout.sidebar_region().clone();
            st.win.dirty_tracker.mark_region(
                region.x,
                region.y,
                region.width,
                region.height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            invalidate_window(hwnd);
        }
    }
    LRESULT(0)
}

/// 长按检测定时器触发
unsafe fn on_timer_long_press(hwnd: HWND) -> LRESULT {
    let _ = KillTimer(hwnd, LP_TIMER_ID);
    if let Some(state) = get_and_set_state(hwnd) {
        let mut st = state.borrow_mut();
        if st.input.mouse_press.lbutton_down {
            if let Some(target) = st.input.mouse_press.lpress_target {
                // 检查按下时间是否达到长按阈值
                if let Some(start) = st.input.mouse_press.lpress_start {
                    if start.elapsed() >= std::time::Duration::from_millis(LP_THRESHOLD_MS as u64) {
                        let idx = st.input.mouse_press.lpress_index;
                        match target {
                            crate::input::PressTarget::ActivityBar => {
                                st.ui.activity_bar.begin_drag(idx);
                                st.ui.status_message =
                                    "活动栏自定义模式（拖拽排序，Esc 退出）".to_string();
                            }
                            crate::input::PressTarget::MenuBar => {
                                st.ui.menu_bar.begin_drag(idx);
                                st.ui.status_message =
                                    "菜单栏自定义模式（拖拽排序，Esc 退出）".to_string();
                            }
                        }
                        st.input.mouse_press.lpress_start = None;
                        invalidate_window(hwnd);
                        return LRESULT(0);
                    }
                }
            }
        }
    }
    LRESULT(0)
}

/// 自动保存：空闲防抖定时器触发
unsafe fn on_timer_autosave_debounce(hwnd: HWND) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        state.borrow_mut().on_autosave_debounce_timer();
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// 自动保存：周期兜底定时器触发
unsafe fn on_timer_autosave_periodic(hwnd: HWND) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        state.borrow_mut().on_autosave_periodic_timer();
        invalidate_window(hwnd);
    }
    LRESULT(0)
}
pub(crate) unsafe fn on_wm_app_2(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // 新建窗口请求
    let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
    create_editor_window(instance.into(), Some(hwnd));
    LRESULT(0)
}

/// msg if msg == WM_APP + 3
/// LSP 事件转发：tokio task 通过 PostMessageW 将 LspEvent 投递到 UI 线程。
/// 由 EditorState::new() 中 spawn 的事件 forwarder task 发送。
/// 处理诊断更新、补全结果、悬停结果等 LSP 事件。
pub(crate) unsafe fn on_wm_app_3(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let raw = lparam.0 as usize;
    // H-09: 立即重建 Box 确保 Rust drop 语义保证清理，即使后续处理 panic 也不会内存泄漏
    let _event_guard = unsafe { Box::from_raw(raw as *mut LspEvent) };
    let event: &LspEvent = &_event_guard;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let st = &mut *state.borrow_mut();
            if let Some(msg) = st
                .lsp
                .lsp
                .handle_event(event.clone(), st.editor.content.file_path.as_ref())
            {
                st.ui.status_message = msg;
            }
        }
    });
    // REQ-P1-07: 不直接调用 render()，触发 WM_PAINT 统一渲染，避免双重渲染
    invalidate_window(_hwnd);
    LRESULT(0)
}

/// msg if msg == WM_APP + 7
pub(crate) unsafe fn on_wm_app_7(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // 文件夹异步扫描批次完成
    let raw = lparam.0 as usize;
    // H-09: 立即重建 Box 确保 Rust drop 语义保证清理，即使 EDITOR_STATE 为 None
    // 或 on_folder_scan_batch_ref panic 也不会内存泄漏
    let _batch_guard = unsafe { Box::from_raw(raw as *mut crate::editor::ScannedBatch) };
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            // 由 window 层负责重建 Box 并持有 _batch_guard；向 editor 传引用，
            // 避免 editor 内部再次 from_raw 同一块内存造成 double-free。
            state.borrow_mut().on_folder_scan_batch_ref(&_batch_guard);
        }
    });
    // REQ-P1-07: 不直接调用 render()，触发 WM_PAINT 统一渲染，避免双重渲染
    invalidate_window(_hwnd);
    LRESULT(0)
}

/// msg if msg == WM_APP + 12
pub(crate) unsafe fn on_wm_app_12(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // 子目录异步扫描完成
    let raw = lparam.0 as usize;
    // 立即重建 Box 确保 Rust drop 语义保证清理
    let _result_guard = unsafe { Box::from_raw(raw as *mut crate::editor::SubdirScanResult) };
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.borrow_mut().on_subdir_scan_result(&_result_guard);
        }
    });
    // REQ-P1-07: 不直接调用 render()，触发 WM_PAINT 统一渲染，避免双重渲染
    invalidate_window(_hwnd);
    LRESULT(0)
}

/// msg if msg == WM_APP + 14
///
/// AI 对话内文件名链接：后台线程预读完成，携带 (路径, 内容) 回到 UI 线程建标签页。
pub(crate) unsafe fn on_wm_app_14(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let raw = lparam.0 as usize;
    // H-09: 立即重建 Box 确保 drop 语义保证清理，即使 EDITOR_STATE 为 None 也不泄漏
    let _payload_guard = unsafe { Box::from_raw(raw as *mut (std::path::PathBuf, String)) };
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state
                .borrow_mut()
                .finish_open_ai_file_link(&_payload_guard.0, &_payload_guard.1);
        }
    });
    // REQ-P1-07: 触发 WM_PAINT 统一渲染，避免双重渲染
    invalidate_window(_hwnd);
    LRESULT(0)
}

/// msg if msg == crate::browser::WM_BROWSER_EVENT
///
/// 内置浏览器：WebView2 回调在 UI 线程把动作压入挂起队列并 PostMessage 到这里，
/// 此处统一排空应用（环境/控制器就绪、URL/标题/历史变化等）
pub(crate) unsafe fn on_browser_event(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    if let Some(state) = get_and_set_state(hwnd) {
        state.borrow_mut().browser.drain_pending(hwnd);
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// msg if msg == WM_APP + 4
pub(crate) unsafe fn on_wm_app_4(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // C-09: SSH 异步连接完成
    let raw = wparam.0;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.borrow_mut().on_ssh_connect_complete(raw);
            invalidate_window(hwnd);
        }
    });
    LRESULT(0)
}

/// msg if msg == WM_APP + 5
pub(crate) unsafe fn on_wm_app_5(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // C-09: Git 异步克隆完成
    let raw = wparam.0;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.borrow_mut().on_git_clone_complete(raw);
            invalidate_window(hwnd);
        }
    });
    LRESULT(0)
}

/// msg if msg == WM_APP + 6
pub(crate) unsafe fn on_wm_app_6(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // P0-1: 远程子目录异步列目录完成
    let raw = wparam.0;
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.borrow_mut().on_ssh_list_dir_complete(raw);
            invalidate_window(hwnd);
        }
    });
    LRESULT(0)
}

/// msg if msg == WM_APP + 8（updater::WM_UPDATE_CHECK_DONE）
/// 更新检查完成：后台线程下载完安装包后投递，UI 线程弹窗询问是否更新
pub(crate) unsafe fn on_wm_app_8(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use crate::dialogs::Dialogs;
    use crate::updater::{UpdateCheckMessage, UpdateCheckResult};

    // 立即重建 Box 保证 drop 语义，即使后续处理 panic 也不会泄漏
    let msg = unsafe { Box::from_raw(lparam.0 as *mut UpdateCheckMessage) };
    // 清除检查中状态
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            state.borrow_mut().ui.update_checking = false;
        }
    });
    match &msg.result {
        UpdateCheckResult::UpToDate => {
            if msg.manual {
                Dialogs::show_info(
                    hwnd,
                    "检查更新",
                    &format!("当前已是最新版本（v{}）。", crate::updater::APP_VERSION),
                );
            }
        }
        UpdateCheckResult::Error(e) => {
            if msg.manual {
                Dialogs::show_error(hwnd, "检查更新失败", e);
            }
        }
        UpdateCheckResult::Available {
            version,
            setup_path,
        } => {
            // 设置 badge 状态（齿轮图标绿点 + 设置页提示）
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().ui.update_available_version = Some(version.clone());
                }
            });
            invalidate_window(hwnd);
            let text = format!(
                "发现新版本 {version}（当前 v{}）。\n安装包已下载完成，是否立即更新？\n应用将退出并完成静默安装，随后自动重启。",
                crate::updater::APP_VERSION
            );
            if Dialogs::confirm_yes_no(hwnd, "发现新版本", &text) {
                if let Err(e) = crate::updater::run_setup_and_exit(setup_path) {
                    Dialogs::show_error(hwnd, "更新失败", &e);
                }
            }
        }
    }
    LRESULT(0)
}

/// msg if msg == WM_APP + 9
/// 异步打开文件夹：避免在 RefCell 借用期间弹模态对话框导致重入 panic
pub(crate) unsafe fn on_wm_app_9(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    if let Some(path) = crate::dialogs::Dialogs::open_folder_dialog(hwnd, "打开文件夹") {
        if crate::editor::files::check_workspace_trust(hwnd, &path) {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().open_folder(path);
                }
            });
        } else {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().ui.status_message = "已取消打开不受信任的工作区".to_string();
                }
            });
        }
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// msg if msg == WM_APP + 10
/// 异步打开文件：避免在 RefCell 借用期间弹模态对话框导致重入 panic
pub(crate) unsafe fn on_wm_app_10(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    if let Some(path) = crate::dialogs::Dialogs::open_file_dialog(hwnd, "打开文件", &[]) {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                state.borrow_mut().load_file(path);
            }
        });
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// msg if msg == WM_APP + 11
/// 异步另存为：避免在 RefCell 借用期间弹模态对话框导致重入 panic
pub(crate) unsafe fn on_wm_app_11(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    if let Some(path) = crate::dialogs::Dialogs::save_file_dialog(hwnd, "另存为", "untitled.txt")
    {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                state.borrow_mut().save_as(path);
            }
        });
        invalidate_window(hwnd);
    }
    LRESULT(0)
}

/// WM_DROPFILES
pub(crate) unsafe fn on_dropfiles(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};
    let hdrop = HDROP(wparam.0 as *mut std::ffi::c_void);

    let file_count = DragQueryFileW(hdrop, u32::MAX, None);
    for i in 0..file_count {
        let path_len = DragQueryFileW(hdrop, i, None);
        if path_len == 0 {
            continue;
        }
        let mut path_buf = vec![0u16; (path_len + 1) as usize];
        let _ = DragQueryFileW(hdrop, i, Some(&mut path_buf));
        if let Ok(path_str) = String::from_utf16(&path_buf[..path_len as usize]) {
            let path = PathBuf::from(path_str);
            if path.is_dir() {
                // 信任检查在 borrow_mut 之前（模态框泵消息，避免 RefCell 重入 panic）
                if crate::editor::files::check_workspace_trust(hwnd, &path) {
                    EDITOR_STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            state.borrow_mut().open_folder(path);
                            invalidate_window(hwnd);
                        }
                    });
                } else {
                    EDITOR_STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            state.borrow_mut().ui.status_message =
                                "已取消打开不受信任的工作区".to_string();
                            invalidate_window(hwnd);
                        }
                    });
                }
                break;
            } else {
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        state.borrow_mut().load_file(path);
                        invalidate_window(hwnd);
                    }
                });
            }
        }
    }
    DragFinish(hdrop);
    LRESULT(0)
}

/// WM_SIZE
pub(crate) unsafe fn on_size(hwnd: HWND, _msg: u32, wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    let mut client_rect = RECT::default();
    if GetClientRect(hwnd, &mut client_rect).is_ok() {
        let width = (client_rect.right - client_rect.left) as u32;
        let height = (client_rect.bottom - client_rect.top) as u32;
        let is_max = wparam.0 == SIZE_MAXIMIZED as usize;
        let is_min = wparam.0 == SIZE_MINIMIZED as usize;
        // 按 hwnd 取状态（而非线程局部活跃状态），
        // 避免多窗口/启动最大化时把尺寸更新到错误的窗口状态上
        if let Some(state) = get_and_set_state(hwnd) {
            let was_frozen;
            {
                let mut st = state.borrow_mut();
                st.win.is_maximized = is_max;
                was_frozen = st.power.frozen && !is_min;
                if is_min {
                    // 最小化：立即进入冰冻态释放内存（AI/Agent 经无头泵保活）
                    st.power.minimized = true;
                    st.enter_frozen();
                } else {
                    st.power.minimized = false;
                    st.resize(width, height);
                    // 从最小化恢复：解冻（全量重绘 + 懒重建 D2D 资源）
                    if st.power.frozen {
                        st.exit_frozen();
                    }
                }
            }
            // 若最大化且用户设置了显示任务栏，调整窗口为工作区大小
            let need_adjust = is_max
                && state
                    .borrow()
                    .ui
                    .app_settings
                    .ui
                    .show_taskbar_when_maximized;
            if need_adjust {
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
                    let _ = SetWindowPos(
                        hwnd,
                        windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                        mi.rcWork.left,
                        mi.rcWork.top,
                        mi.rcWork.right - mi.rcWork.left,
                        mi.rcWork.bottom - mi.rcWork.top,
                        SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            if !is_min {
                invalidate_window(hwnd);
                // 从最小化恢复时：同步触发 WM_PAINT 完成首帧渲染，
                // 避免 D2D 渲染目标重建期间标题栏按钮等区域短暂空白
                if was_frozen {
                    let _ = windows::Win32::Graphics::Gdi::UpdateWindow(hwnd);
                }
            }
        }
    }
    LRESULT(0)
}

/// WM_DPICHANGED
pub(crate) unsafe fn on_dpichanged(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let new_dpi = (wparam.0 & 0xFFFF) as f32;
    let new_scale = new_dpi / 96.0;

    if lparam.0 != 0 {
        let suggested_rect: *const RECT = lparam.0 as *const RECT;
        let rect = &*suggested_rect;
        let _ = SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            st.win.dpi_scale = new_scale;
            st.win.render_ctx.set_dpi(new_dpi);
            st.win.text_renderer.set_dpi_scale(new_scale);
            // REQ-P2-07: DPI 变化时重新缩放布局常量
            st.ui.layout.apply_dpi_scale(new_scale);
            // REQ-P2-04: IME 候选/合成窗口尺寸按新 DPI 缩放
            st.ui.ime.set_dpi_scale(new_scale);
            st.ui.status_message =
                format!("DPI: {} ({}%)", new_dpi as u32, (new_scale * 100.0) as u32);
            // UI-M09: DPI 切换后重建渲染目标，确保尺寸与新 DPI 匹配
            let _ = st.init_render_target();
            // P3-1: DPI 切换后必须重建 text_format_cache 与 brush_cache，
            // 否则缓存的 IDWriteTextFormat 仍使用旧 font_size，导致渲染尺寸不一致
            // Theme 已 derive(Copy)，按值拷贝避免借用冲突
            let theme = st.win.theme;
            let font_size = st.win.text_renderer.font_size();
            st.win.render_ctx.init_common_resources(&theme, font_size);
            invalidate_window(hwnd);
        }
    });
    LRESULT(0)
}

/// WM_NCACTIVATE
pub(crate) unsafe fn on_ncactivate(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // 阻止系统绘制非激活状态的边框（白色边框）
    // 返回 TRUE 表示已处理，不绘制系统默认的 NC 激活指示器
    LRESULT(1)
}

/// WM_NCCALCSIZE
pub(crate) unsafe fn on_nccalcsize(
    hwnd: HWND,
    _msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // 移除系统非客户区边框，避免白色边框线。
    //
    // 当 wparam == 0（FALSE）时，lparam 指向 RECT，返回 0 表示客户区覆盖整个窗口。
    // 当 wparam != 0（TRUE）时，lparam 指向 NCCALCSIZE_PARAMS，rgrc[0] 输入为提议的
    // 新窗口矩形，输出应为新的客户区矩形。返回标志指示哪些矩形有效。
    //
    // 修复（最大化后鼠标与 UI 偏移）：窗口带 WS_THICKFRAME 时，Windows 在
    // 最大化时会把窗口矩形向外膨胀一圈不可见缩放边框（原点移到屏幕外负坐标），
    // 若客户区不收缩，渲染与鼠标命中会整体偏移一个边框宽度。
    // 最大化时把客户区向内收缩边框厚度，使客户区与可视区域一致。
    if wparam.0 != 0 {
        let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);

        // 获取当前窗口所在的显示器信息，用于判断当前是否处于最大化过程
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let is_maximized = if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            // 若 rgrc[0] 与显示器矩形一致（或接近），说明正在最大化
            let rc = &params.rgrc[0];
            rc.left <= mi.rcMonitor.left
                && rc.top <= mi.rcMonitor.top
                && rc.right >= mi.rcMonitor.right
                && rc.bottom >= mi.rcMonitor.bottom
        } else {
            false
        };

        if is_maximized {
            let frame_x = GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
            let frame_y = GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
            params.rgrc[0].left += frame_x;
            params.rgrc[0].top += frame_y;
            params.rgrc[0].right -= frame_x;
            params.rgrc[0].bottom -= frame_y;

            // 返回 WVR_REDRAW 告诉 Windows 根据我们修改后的客户区矩形
            // 重新计算窗口矩形，并触发重绘
            return LRESULT(0x0300); // WVR_REDRAW = WVR_HREDRAW | WVR_VREDRAW
        }
    }
    LRESULT(0)
}

/// WM_NCHITTEST
pub(crate) unsafe fn on_nchittest(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 自定义命中测试，实现无边框窗口的调整大小和拖动
    let x = ((lparam.0 & 0xFFFF) as i16) as i32;
    let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as i32;
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_ok() {
        // UI-H05: 根据 DPI 缩放边框大小，确保高 DPI 下可点击区域足够大
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        let dpi = GetDpiForWindow(hwnd);
        let scale = dpi as f32 / 96.0;
        let border_size = (8.0 * scale) as i32;
        let left = x - rect.left;
        let top = y - rect.top;
        let right = rect.right - x;
        let bottom = rect.bottom - y;

        let mut result = HTCLIENT;
        if top < border_size {
            if left < border_size {
                result = HTTOPLEFT;
            } else if right < border_size {
                result = HTTOPRIGHT;
            } else {
                result = HTTOP;
            }
        } else if bottom < border_size {
            if left < border_size {
                result = HTBOTTOMLEFT;
            } else if right < border_size {
                result = HTBOTTOMRIGHT;
            } else {
                result = HTBOTTOM;
            }
        } else if left < border_size {
            result = HTLEFT;
        } else if right < border_size {
            result = HTRIGHT;
        } else {
            // 标题栏区域全部返回 HTCLIENT，由 WM_LBUTTONDOWN 统一处理菜单/按钮点击和拖动
            // 不返回 HTCAPTION/HTCLOSE 等系统码，因为 WS_POPUP 窗口系统不会正确处理它们
        }
        return LRESULT(result as isize);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// WM_ERASEBKGND
pub(crate) unsafe fn on_erasebkgnd(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    // 阻止系统擦除背景，避免白色闪烁
    LRESULT(1)
}

/// WM_PAINT
pub(crate) unsafe fn on_paint(hwnd: HWND, _msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> LRESULT {
    let mut ps = PAINTSTRUCT::default();
    let _hdc = BeginPaint(hwnd, &mut ps);

    // REQ-P1-??: Windows 触发 WM_PAINT（如 InvalidateRect）时，若内部脏区追踪为空，
    // 强制标记全窗口重绘。否则 render() 会跳过绘制，导致上一帧内容残留（重影）。
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            if !st.win.dirty_tracker.has_dirty() {
                st.win.dirty_tracker.mark_full_window();
            }
        }
    });

    // C-04: 渲染路径中存在 40+ 处 D2D 资源创建 .unwrap()，设备丢失（GPU 驱动崩溃、
    // 显示模式切换）时会 panic。此处统一 catch_unwind 记录诊断并优雅跳过本次绘制，
    // 避免逐个替换 unwrap 的同时也保证 panic 不传播。
    let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                state.borrow_mut().render();
            }
        });
    }));
    if let Err(payload) = render_result {
        let msg = payload
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string panic>");
        eprintln!("[C-04] render panic recovered (D2D device loss?): {}", msg);
    }
    let _ = EndPaint(hwnd, &ps);
    LRESULT(0)
}

/// REQ-P0-05: WM_SETFOCUS — 窗口获得焦点
///
/// 同 `on_kill_focus`，使用 `try_borrow_mut()` 防止模态对话框消息循环
/// 重入时 panic。
pub(crate) unsafe fn on_set_focus(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    get_and_set_state(hwnd);
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            if let Ok(mut st) = state.try_borrow_mut() {
                st.input.focus_manager.on_set_focus();
                st.power.note_input();
                // 冰冻态获焦（非最小化）：立即解冻
                if st.power.frozen && !st.power.minimized {
                    st.exit_frozen();
                }
            }
        }
    });
    LRESULT(0)
}

/// REQ-P0-05: WM_KILLFOCUS — 窗口失去焦点
///
/// 使用 `try_borrow_mut()` 而非 `borrow_mut()`：当用户关闭标签页时，
/// `close_tab` → `close_current_tab_checked` 会弹出模态确认对话框，
/// 对话框的消息循环可能派发 `WM_KILLFOCUS`，此时 `EditorState` 已被
/// `close_tab` 的调用方持有 `borrow_mut()`，直接 `borrow_mut()` 会 panic。
/// `try_borrow_mut` 在此场景下优雅跳过，避免应用程序崩溃。
pub(crate) unsafe fn on_kill_focus(
    hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    get_and_set_state(hwnd);
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            if let Ok(mut st) = state.try_borrow_mut() {
                st.input.focus_manager.on_kill_focus();
                // 自动保存：失焦立即保存（用户离开编辑场景的瞬间落盘）
                st.autosave_on_focus_loss();
            } else {
                tracing::debug!("on_kill_focus: EditorState 已被借用（可能是模态对话框），跳过");
            }
        }
    });
    invalidate_window(hwnd);
    LRESULT(0)
}

/// WM_SETCURSOR
///
/// 根据鼠标所在 UI 区域设置对应的光标类型（Arrow/IBeam/Hand/SizeWE/SizeNS）。
/// 仅在客户区（HTCLIENT）内拦截；非客户区（resize 边框等）交由 DefWindowProcW 处理。
pub(crate) unsafe fn on_setcursor(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // lparam 低字 = 命中测试码，高字 = 鼠标消息 ID
    let hit_test = (lparam.0 & 0xFFFF) as u16 as i32;
    // 非客户区（resize 边框、标题栏系统区等）→ 交由系统处理
    if hit_test != HTCLIENT as i32 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    // 获取鼠标位置（屏幕坐标，物理像素），转换为客户端坐标
    // GetMessagePos 返回 DWORD：低字 = x，高字 = y（屏幕坐标）
    let pos = GetMessagePos();
    let mut client_pt = POINT {
        x: (pos & 0xFFFF) as i16 as i32,
        y: ((pos >> 16) & 0xFFFF) as i16 as i32,
    };
    let _ = ScreenToClient(hwnd, &mut client_pt);
    // 计算光标类型并设置
    let cursor = compute_cursor_for_pos(hwnd, client_pt.x, client_pt.y);
    if let Ok(hcursor) = LoadCursorW(None, cursor.idc_cursor()) {
        let _ = SetCursor(hcursor);
    }
    // 返回 TRUE 阻止默认处理
    LRESULT(1)
}

/// _
pub(crate) unsafe fn on_default(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
