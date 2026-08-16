use super::*;

/// 新建项目：弹出对话框让用户输入项目名称，确认后在默认项目目录下创建文件夹并打开
pub fn new_project(state: &mut EditorState) {
    state.ui.new_project_dialog.reset();
    state.ui.new_project_dialog.visible = true;
    state.ui.status_message = "新建项目...".to_string();
    state.emit_event(crate::events::EditorEvent::DialogVisibilityChanged);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
            state.win.hwnd,
            crate::window::CARET_TIMER_ID,
            530,
            None,
        );
    }
}

pub(super) fn kill_caret_timer(state: &EditorState) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
            state.win.hwnd,
            crate::window::CARET_TIMER_ID,
        );
    }
}

/// 关闭新建项目对话框
pub fn close_new_project_dialog(state: &mut EditorState) {
    state.ui.new_project_dialog.visible = false;
    kill_caret_timer(state);
    state.emit_event(crate::events::EditorEvent::DialogVisibilityChanged);
}

/// 确认创建项目（由对话框调用）
pub fn confirm_new_project(state: &mut EditorState) {
    if let Err(e) = state.ui.new_project_dialog.validate() {
        state.ui.new_project_dialog.error_message = Some(e);
        return;
    }

    let project_path = state.ui.new_project_dialog.project_path();
    state.ui.new_project_dialog.visible = false;
    kill_caret_timer(state);
    state.emit_event(crate::events::EditorEvent::DialogVisibilityChanged);

    // 确保基础目录存在
    if let Some(parent) = project_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            let msg = format!("创建项目目录失败: {}", e);
            state.ui.status_message = msg.clone();
            Dialogs::show_error(state.win.hwnd, "新建项目", &msg);
            return;
        }
    }

    // 创建项目文件夹
    match std::fs::create_dir(&project_path) {
        Ok(()) => {
            state.ui.status_message = format!("项目已创建: {}", project_path.display());
            // 打开项目文件夹作为工作区
            // 信任检查在 open_folder 之前（不持有 RefCell 借用，避免模态框重入 panic）
            if crate::editor::files::check_workspace_trust(state.win.hwnd, &project_path) {
                state.open_folder(project_path);
            } else {
                state.ui.status_message = "已取消打开不受信任的工作区".to_string();
            }
        }
        Err(e) => {
            let msg = format!("创建项目失败: {}", e);
            state.ui.status_message = msg.clone();
            Dialogs::show_error(state.win.hwnd, "新建项目", &msg);
        }
    }
}

/// 处理新建项目对话框的鼠标点击
pub fn handle_new_project_dialog_click(
    state: &mut EditorState,
    mouse_x: f32,
    mouse_y: f32,
) -> crate::new_project_dialog::NewProjectDialogAction {
    use crate::new_project_dialog::NewProjectDialogAction;
    let dialog = &mut state.ui.new_project_dialog;

    if let Some(rect) = &dialog.confirm_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            return NewProjectDialogAction::Confirm;
        }
    }
    if let Some(rect) = &dialog.cancel_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            return NewProjectDialogAction::Cancel;
        }
    }
    if let Some(rect) = &dialog.input_rect {
        if rect.contains(mouse_x, mouse_y) {
            dialog.focus_field = 0;
            return NewProjectDialogAction::FocusInput;
        }
    }
    NewProjectDialogAction::None
}

/// 粘贴到新建项目对话框的项目名称输入框
pub fn paste_into_new_project_dialog(state: &mut EditorState) {
    if let Some(text) = EditorState::get_clipboard_text() {
        // 移除路径分隔符和非法字符
        state
            .ui
            .new_project_dialog
            .project_name
            .extend(text.chars().filter(|c| {
                !matches!(
                    c,
                    '\\' | '/' | ':' | '*' | '?' | '\"' | '<' | '>' | '|' | '\n' | '\r'
                )
            }));
        state.ui.new_project_dialog.error_message = None;
    }
}

impl EditorState {
    /// 新建项目：弹出对话框让用户输入项目名称，确认后在默认项目目录下创建文件夹并打开
    pub fn new_project(&mut self) {
        new_project(self)
    }

    /// 关闭新建项目对话框
    pub fn close_new_project_dialog(&mut self) {
        close_new_project_dialog(self)
    }

    /// 确认创建项目（由对话框调用）
    pub fn confirm_new_project(&mut self) {
        confirm_new_project(self)
    }

    /// 处理新建项目对话框的鼠标点击
    pub fn handle_new_project_dialog_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> crate::new_project_dialog::NewProjectDialogAction {
        handle_new_project_dialog_click(self, mouse_x, mouse_y)
    }

    /// 粘贴到新建项目对话框的项目名称输入框
    pub fn paste_into_new_project_dialog(&mut self) {
        paste_into_new_project_dialog(self)
    }
}
