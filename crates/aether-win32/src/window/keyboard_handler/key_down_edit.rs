//! WM_KEYDOWN 非 Ctrl 编辑器按键处理。
//!
//! 从 `window.rs` 拆分而来，保持原有逻辑不变。
//! 处理 Return/Back/Delete/F3/Escape/方向键/Home/End/PageUp/Down/Tab。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::SetTimer;

use crate::editor::EditorState;

use super::super::{invalidate_window, AI_REFRESH_MS, AI_TIMER_ID, EDITOR_STATE};

/// 非 Ctrl 按键总分发
pub(crate) unsafe fn okd_edit_dispatch(hwnd: HWND, vk: VIRTUAL_KEY, shift: bool) {
    // 终端聚焦时，将编辑键直接发送到 ConPTY（同步终端模式）
    let terminal_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow().terminal.terminal_panel.focused)
            .unwrap_or(false)
    });
    // IME 合成期间（中文/日文候选框打开时）不要拦截任何按键，
    // 让 IME 自己处理 Backspace/字母/方向键等以更新或取消合成串
    let ime_composing = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow().editor.composition.is_some())
            .unwrap_or(false)
    });
    if terminal_active && !ime_composing && okd_edit_terminal(hwnd, vk) {
        return;
    }

    // 历史浮窗：编辑态/搜索态时拦截编辑键（优先于 AI 输入框与编辑器）
    if !ime_composing && okd_history_window(hwnd, vk) {
        return;
    }

    // 智能体模式：AI 面板可见但输入框未聚焦时，方向键滚动对话历史
    if !ime_composing && okd_agent_ai_scroll(hwnd, vk) {
        return;
    }

    match vk {
        VK_RETURN => okd_edit_return(hwnd),
        VK_BACK => okd_edit_back(hwnd),
        VK_DELETE | VK_F3 | VK_ESCAPE => okd_edit_delete_misc(hwnd, vk, shift),
        VK_LEFT | VK_RIGHT => okd_edit_left_right(hwnd, vk, shift),
        VK_UP | VK_DOWN => okd_edit_up_down(hwnd, vk, shift),
        VK_HOME | VK_END | VK_PRIOR | VK_NEXT => okd_edit_home_end_page(hwnd, vk, shift),
        VK_TAB => okd_edit_tab(hwnd),
        _ => {}
    }
}

/// 智能体模式：AI 面板可见但输入框未聚焦时，方向键/PageUp/PageDown 滚动对话历史
/// 返回 true 表示已处理，false 表示按键不属于此范畴
unsafe fn okd_agent_ai_scroll(hwnd: HWND, vk: VIRTUAL_KEY) -> bool {
    let should_handle = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| {
                let st = state.borrow();
                // 智能体模式 + AI 面板可见 + 输入框未聚焦 + 历史浮窗未打开
                st.editor_mode.is_agent()
                    && st.ai.ai_panel.visible
                    && !st.ai.ai_panel.input_focused
                    && !st.ai.ai_panel.history_open
            })
            .unwrap_or(false)
    });
    if !should_handle {
        return false;
    }

    match vk {
        VK_UP => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    st.ai.ai_panel.scroll_y = (st.ai.ai_panel.scroll_y - 40.0)
                        .clamp(0.0, st.ai.ai_panel.content_height.max(0.0));
                    st.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        VK_DOWN => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    st.ai.ai_panel.scroll_y = (st.ai.ai_panel.scroll_y + 40.0)
                        .clamp(0.0, st.ai.ai_panel.content_height.max(0.0));
                    st.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        VK_PRIOR => {
            // PageUp：滚动一页
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    let page_h = 400.0; // 估算一页高度
                    st.ai.ai_panel.scroll_y = (st.ai.ai_panel.scroll_y - page_h)
                        .clamp(0.0, st.ai.ai_panel.content_height.max(0.0));
                    st.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        VK_NEXT => {
            // PageDown：滚动一页
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    let page_h = 400.0; // 估算一页高度
                    st.ai.ai_panel.scroll_y = (st.ai.ai_panel.scroll_y + page_h)
                        .clamp(0.0, st.ai.ai_panel.content_height.max(0.0));
                    st.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        VK_HOME => {
            // Home：滚动到顶部
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    st.ai.ai_panel.scroll_y = 0.0;
                    st.ai.ai_panel.stick_to_bottom = false;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        VK_END => {
            // End：滚动到底部
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    st.ai.ai_panel.scroll_y = st.ai.ai_panel.content_height.max(0.0);
                    st.ai.ai_panel.stick_to_bottom = true;
                    invalidate_window(hwnd);
                }
            });
            true
        }
        _ => false,
    }
}

/// 终端聚焦时的按键处理：将按键转换为 ANSI 序列发送到 ConPTY。
/// 返回 true 表示已处理，false 表示按键不属于终端范畴。
unsafe fn okd_edit_terminal(hwnd: HWND, vk: VIRTUAL_KEY) -> bool {
    use crate::terminal::ArrowKey;
    let handled = match vk {
        VK_RETURN => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_enter();
                }
            });
            true
        }
        VK_BACK => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_backspace();
                }
            });
            true
        }
        VK_DELETE => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_delete();
                }
            });
            true
        }
        VK_TAB => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_tab();
                }
            });
            true
        }
        VK_UP => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state
                        .borrow_mut()
                        .terminal
                        .terminal_panel
                        .send_arrow(ArrowKey::Up);
                }
            });
            true
        }
        VK_DOWN => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state
                        .borrow_mut()
                        .terminal
                        .terminal_panel
                        .send_arrow(ArrowKey::Down);
                }
            });
            true
        }
        VK_LEFT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state
                        .borrow_mut()
                        .terminal
                        .terminal_panel
                        .send_arrow(ArrowKey::Left);
                }
            });
            true
        }
        VK_RIGHT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state
                        .borrow_mut()
                        .terminal
                        .terminal_panel
                        .send_arrow(ArrowKey::Right);
                }
            });
            true
        }
        VK_HOME => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_home();
                }
            });
            true
        }
        VK_END => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().terminal.terminal_panel.send_end();
                }
            });
            true
        }
        _ => false,
    };
    if handled {
        // 标脏底部面板区域：终端按键回显只需局部重绘终端，避免全窗口重绘导致卡顿
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let mut st = state.borrow_mut();
                let bp = st.ui.layout.bottom_panel_region();
                st.win.dirty_tracker.mark_region(
                    bp.x,
                    bp.y,
                    bp.width,
                    bp.height,
                    crate::dirty_rect::DirtyRegionType::BottomPanel,
                );
            }
        });
        invalidate_window(hwnd);
    }
    handled
}

/// 历史浮窗按键处理：编辑态（重命名）优先，其次搜索框。
/// 返回 true 表示已消费该按键。仅处理 Return/Back/Delete/Escape/Left/Right/Home/End。
unsafe fn okd_history_window(hwnd: HWND, vk: VIRTUAL_KEY) -> bool {
    let mode = EDITOR_STATE.with(|s| {
        s.borrow().as_ref().and_then(|state| {
            let st = state.borrow();
            if !st.ai.ai_panel.history_open {
                return None;
            }
            if st.ai.ai_panel.history_editing_id.is_some() {
                Some(1u8) // 编辑态
            } else if st.ai.ai_panel.history_search_focused {
                Some(2u8) // 搜索态
            } else {
                Some(3u8) // 浮窗打开但无输入焦点（仅响应 Esc 关闭）
            }
        })
    });
    let mode = match mode {
        Some(m) => m,
        None => return false,
    };
    let editing = mode == 1;
    // 浮窗打开但无焦点时，仅 Esc（关闭浮窗）生效
    if mode == 3 {
        if vk == VK_ESCAPE {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().ai.ai_panel.close_history_window();
                }
            });
            invalidate_window(hwnd);
            return true;
        }
        return false;
    }
    let handled = match vk {
        VK_RETURN => {
            if editing {
                // 回车：提交重命名
                EDITOR_STATE.with(|s| {
                    if let Some(state) = s.borrow().as_ref() {
                        let mut st = state.borrow_mut();
                        match st.ai.ai_panel.commit_history_edit() {
                            Ok(()) => st.ui.status_message = "已重命名对话".to_string(),
                            Err(e) => st.ui.status_message = e,
                        }
                    }
                });
            } else {
                // 搜索态回车：恢复第一个匹配会话（快速打开）
                let first = EDITOR_STATE.with(|s| {
                    s.borrow()
                        .as_ref()
                        .map(|state| {
                            state
                                .borrow()
                                .ai
                                .ai_panel
                                .history_page_indices()
                                .first()
                                .copied()
                        })
                        .flatten()
                });
                if let Some(i) = first {
                    EDITOR_STATE.with(|s| {
                        if let Some(state) = s.borrow().as_ref() {
                            let mut st = state.borrow_mut();
                            st.ai.ai_panel.restore_from_history(i);
                            st.ai.ai_panel.close_history_window();
                        }
                    });
                }
            }
            true
        }
        VK_ESCAPE => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        st.ai.ai_panel.cancel_history_edit();
                    } else {
                        // 搜索态 Esc：失焦；再按一次（已失焦）由外层关闭浮窗
                        st.ai.ai_panel.history_search_focused = false;
                    }
                }
            });
            true
        }
        VK_BACK => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        st.ai.ai_panel.history_edit_backspace();
                    } else {
                        st.ai.ai_panel.history_search_backspace();
                    }
                }
            });
            true
        }
        VK_LEFT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        let text = st.ai.ai_panel.history_editing_text.clone();
                        let c = st.ai.ai_panel.history_editing_caret;
                        if c > 0 {
                            st.ai.ai_panel.history_editing_caret =
                                text[..c].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                        }
                    } else {
                        st.ai.ai_panel.history_search_move_left();
                    }
                }
            });
            true
        }
        VK_RIGHT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        let text = st.ai.ai_panel.history_editing_text.clone();
                        let len = text.len();
                        let c = st.ai.ai_panel.history_editing_caret;
                        if c < len {
                            st.ai.ai_panel.history_editing_caret = text[c..]
                                .char_indices()
                                .nth(1)
                                .map(|(i, _)| c + i)
                                .unwrap_or(len);
                        }
                    } else {
                        st.ai.ai_panel.history_search_move_right();
                    }
                }
            });
            true
        }
        VK_HOME => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        st.ai.ai_panel.history_editing_caret = 0;
                    } else {
                        st.ai.ai_panel.history_search_caret = 0;
                    }
                }
            });
            true
        }
        VK_END => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if editing {
                        st.ai.ai_panel.history_editing_caret =
                            st.ai.ai_panel.history_editing_text.len();
                    } else {
                        st.ai.ai_panel.history_search_caret = st.ai.ai_panel.history_search.len();
                    }
                }
            });
            true
        }
        _ => false,
    };
    if handled {
        invalidate_window(hwnd);
    }
    handled
}

/// 判断当前是否有选中文本
fn has_selection(st: &EditorState) -> bool {
    st.editor.content.selection_start.is_some() && st.editor.content.selection_end.is_some()
}

/// VK_RETURN：终端/AI/查找/编辑器各自的回车处理
unsafe fn okd_edit_return(hwnd: HWND) {
    let terminal_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow().terminal.terminal_panel.focused)
            .unwrap_or(false)
    });
    let ai_panel_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            // C-10: 改用 input_focused 而非 right_panel_visible，避免面板可见即劫持回车
            .map(|state| state.borrow().ai.ai_panel.input_focused)
            .unwrap_or(false)
    });
    let find_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| {
                state.borrow().editor.find.visible
                    && state.borrow().editor.find.focus != crate::editor::FindReplaceFocus::None
            })
            .unwrap_or(false)
    });
    if terminal_active {
        // ConPTY 模式下 Enter 已由 okd_edit_terminal 处理，此处不应到达
        // 保留分支防止 fallback 到编辑器逻辑
    } else if ai_panel_active {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let settings = state.borrow().ui.app_settings.active_ai_settings();
                let mode = state.borrow().ai.ai_panel.mode;
                let attachments = state.borrow().ai.ai_panel.attachments.clone();
                let context = state.borrow().gather_context(&attachments);
                let _ = state
                    .borrow_mut()
                    .ai
                    .ai_panel
                    .send_message_with_prepared_context(&settings, context, mode);
                // 启动后台刷新定时器，使流式回复逐字平滑显示，完成后自动停止
                let _ = SetTimer(hwnd, AI_TIMER_ID, AI_REFRESH_MS, None);
                invalidate_window(hwnd);
            }
        });
    } else if find_active {
        okd_edit_return_find(hwnd);
    } else {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let has_sel = has_selection(&state.borrow());
                if has_sel {
                    state.borrow_mut().delete_selection();
                }
                // P1-1: 多光标模式下广播换行到所有光标
                state.borrow_mut().broadcast_insert_newline();
                invalidate_window(hwnd);
            }
        });
    }
}

/// 查找面板激活时 VK_RETURN 的处理
unsafe fn okd_edit_return_find(hwnd: HWND) {
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let focus = state.borrow().editor.find.focus;
            match focus {
                crate::editor::FindReplaceFocus::FindQuery => {
                    let st = &mut *state.borrow_mut();
                    st.editor.find.find_next(&mut st.editor.content);
                }
                crate::editor::FindReplaceFocus::ReplaceText => {
                    let st = &mut *state.borrow_mut();
                    st.editor.find.replace_current(&mut st.editor.content);
                    st.editor.find.find_next(&mut st.editor.content);
                }
                _ => {}
            }
            invalidate_window(hwnd);
        }
    });
}

/// VK_BACK：终端/AI/查找/编辑器各自的退格处理
unsafe fn okd_edit_back(hwnd: HWND) {
    let terminal_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow().terminal.terminal_panel.focused)
            .unwrap_or(false)
    });
    let ai_panel_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| state.borrow().ai.ai_panel.input_focused)
            .unwrap_or(false)
    });
    let find_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| {
                state.borrow().editor.find.visible
                    && state.borrow().editor.find.focus != crate::editor::FindReplaceFocus::None
            })
            .unwrap_or(false)
    });
    if terminal_active {
        // ConPTY 模式下 Backspace 已由 okd_edit_terminal 处理，此处不应到达
    } else if ai_panel_active {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                state.borrow_mut().ai.ai_panel.backspace();
                invalidate_window(hwnd);
            }
        });
    } else if find_active {
        okd_edit_back_find(hwnd);
    } else {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let has_sel = has_selection(&state.borrow());
                if has_sel {
                    state.borrow_mut().delete_selection();
                } else {
                    // P1-1: 多光标模式下广播退格到所有光标
                    state.borrow_mut().broadcast_delete_char();
                }
                invalidate_window(hwnd);
            }
        });
    }
}

/// 查找面板激活时 VK_BACK 的处理
unsafe fn okd_edit_back_find(hwnd: HWND) {
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let focus = state.borrow().editor.find.focus;
            match focus {
                crate::editor::FindReplaceFocus::FindQuery => {
                    let st = &mut *state.borrow_mut();
                    st.editor.find.query.pop();
                    st.editor.find.find_all(&st.editor.content);
                }
                crate::editor::FindReplaceFocus::ReplaceText => {
                    state.borrow_mut().editor.find.replace_text.pop();
                }
                _ => {}
            }
            invalidate_window(hwnd);
        }
    });
}

/// VK_DELETE/F3/Escape：删除/查找下一个/关闭查找
unsafe fn okd_edit_delete_misc(hwnd: HWND, vk: VIRTUAL_KEY, shift: bool) {
    match vk {
        VK_DELETE => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let has_sel = has_selection(&state.borrow());
                    if has_sel {
                        state.borrow_mut().delete_selection();
                    } else {
                        state.borrow_mut().delete_forward();
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        VK_F3 => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    if shift {
                        let st = &mut *state.borrow_mut();
                        st.editor.find.find_prev(&mut st.editor.content);
                    } else {
                        let st = &mut *state.borrow_mut();
                        st.editor.find.find_next(&mut st.editor.content);
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        VK_ESCAPE => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    state.borrow_mut().editor.find.close_find_replace();
                    invalidate_window(hwnd);
                }
            });
        }
        _ => {}
    }
}

/// VK_LEFT/VK_RIGHT：光标左右移动（含 Shift 选择）
unsafe fn okd_edit_left_right(hwnd: HWND, vk: VIRTUAL_KEY, shift: bool) {
    match vk {
        VK_LEFT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if shift {
                        if st.editor.content.selection_start.is_none() {
                            st.start_selection();
                        }
                        st.move_cursor_left();
                        st.update_selection();
                    } else {
                        if st.editor.content.selection_start.is_some() {
                            st.clear_selection();
                        }
                        st.move_cursor_left();
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        VK_RIGHT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if shift {
                        if st.editor.content.selection_start.is_none() {
                            st.start_selection();
                        }
                        st.move_cursor_right();
                        st.update_selection();
                    } else {
                        if st.editor.content.selection_start.is_some() {
                            st.clear_selection();
                        }
                        st.move_cursor_right();
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        _ => {}
    }
}

/// VK_UP/VK_DOWN：光标上下移动（含 Shift 选择）
unsafe fn okd_edit_up_down(hwnd: HWND, vk: VIRTUAL_KEY, shift: bool) {
    match vk {
        VK_UP => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if shift {
                        if st.editor.content.selection_start.is_none() {
                            st.start_selection();
                        }
                        st.move_cursor_up();
                        st.update_selection();
                    } else {
                        if st.editor.content.selection_start.is_some() {
                            st.clear_selection();
                        }
                        st.move_cursor_up();
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        VK_DOWN => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let mut st = state.borrow_mut();
                    if shift {
                        if st.editor.content.selection_start.is_none() {
                            st.start_selection();
                        }
                        st.move_cursor_down();
                        st.update_selection();
                    } else {
                        if st.editor.content.selection_start.is_some() {
                            st.clear_selection();
                        }
                        st.move_cursor_down();
                    }
                    invalidate_window(hwnd);
                }
            });
        }
        _ => {}
    }
}

/// VK_HOME/END/PRIOR/NEXT：行首末/文件首末/翻页
unsafe fn okd_edit_home_end_page(hwnd: HWND, vk: VIRTUAL_KEY, shift: bool) {
    match vk {
        VK_HOME => okd_edit_home(hwnd, shift),
        VK_END => okd_edit_end(hwnd, shift),
        VK_PRIOR => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let page = state.borrow().win.window_height as f32 - 24.0;
                    state.borrow_mut().scroll(-page);
                    invalidate_window(hwnd);
                }
            });
        }
        VK_NEXT => {
            EDITOR_STATE.with(|s| {
                if let Some(state) = s.borrow().as_ref() {
                    let page = state.borrow().win.window_height as f32 - 24.0;
                    state.borrow_mut().scroll(page);
                    invalidate_window(hwnd);
                }
            });
        }
        _ => {}
    }
}

/// VK_HOME：Smart Home（已在首个非空白位置时跳到行首）
unsafe fn okd_edit_home(hwnd: HWND, shift: bool) {
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            // 计算当前行首个非空白位置，判断是否已在该位置
            let already_at_smart = st
                .editor
                .content
                .buffer
                .get_line(st.editor.content.cursor_line)
                .map(|text| {
                    let first_non_ws = text
                        .char_indices()
                        .skip_while(|(_, c)| c.is_whitespace())
                        .map(|(i, _)| i)
                        .next()
                        .unwrap_or(text.len());
                    st.editor.content.cursor_col == first_non_ws
                })
                .unwrap_or(false);
            if shift {
                if st.editor.content.selection_start.is_none() {
                    st.start_selection();
                }
                st.move_cursor_smart_home(already_at_smart);
                st.update_selection();
            } else {
                if st.editor.content.selection_start.is_some() {
                    st.clear_selection();
                }
                st.move_cursor_smart_home(already_at_smart);
            }
            invalidate_window(hwnd);
        }
    });
}

/// VK_END：行末（含 Shift 选择到行末）
unsafe fn okd_edit_end(hwnd: HWND, shift: bool) {
    EDITOR_STATE.with(|s| {
        if let Some(state) = s.borrow().as_ref() {
            let mut st = state.borrow_mut();
            if shift {
                if st.editor.content.selection_start.is_none() {
                    st.start_selection();
                }
                st.move_cursor_end();
                st.update_selection();
            } else {
                if st.editor.content.selection_start.is_some() {
                    st.clear_selection();
                }
                st.move_cursor_end();
            }
            invalidate_window(hwnd);
        }
    });
}

/// VK_TAB：查找面板焦点切换/编辑器 Tab 键
unsafe fn okd_edit_tab(hwnd: HWND) {
    let find_active = EDITOR_STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|state| {
                state.borrow().editor.find.visible
                    && state.borrow().editor.find.focus != crate::editor::FindReplaceFocus::None
            })
            .unwrap_or(false)
    });
    if find_active {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                let focus = state.borrow().editor.find.focus;
                let replace_visible = state.borrow().editor.find.replace_visible;
                let new_focus = match focus {
                    crate::editor::FindReplaceFocus::FindQuery => {
                        if replace_visible {
                            crate::editor::FindReplaceFocus::ReplaceText
                        } else {
                            crate::editor::FindReplaceFocus::FindQuery
                        }
                    }
                    crate::editor::FindReplaceFocus::ReplaceText => {
                        crate::editor::FindReplaceFocus::FindQuery
                    }
                    _ => crate::editor::FindReplaceFocus::FindQuery,
                };
                state.borrow_mut().editor.find.focus = new_focus;
                invalidate_window(hwnd);
            }
        });
    } else {
        EDITOR_STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                // P3.3: 若有内联补全建议，Tab 接受建议；否则插入制表符
                let accepted = {
                    let st = &mut *state.borrow_mut();
                    st.accept_inline_completion()
                };
                if accepted {
                    invalidate_window(hwnd);
                } else {
                    let has_sel = has_selection(&state.borrow());
                    if has_sel {
                        state.borrow_mut().delete_selection();
                    }
                    state.borrow_mut().insert_tab();
                    invalidate_window(hwnd);
                }
            }
        });
    }
}
