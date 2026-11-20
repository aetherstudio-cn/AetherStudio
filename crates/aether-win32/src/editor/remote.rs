use super::*;

/// C-09: 异步启动 Git 克隆（后台线程执行 clone_repo，不阻塞 UI）
pub fn start_git_clone(state: &mut EditorState, url: String, target_path: PathBuf) {
    // P0-2: 预检 git 是否可用，缺失时引导用户安装并跳转下载页
    if !aether_remote::git_available() {
        state.ui.git_cloning = false;
        state.remote.clone_dialog.visible = true;
        state.remote.clone_dialog.error_message = Some(format!(
            "系统未安装 git，请安装 Git 后重试。\n下载页: {}",
            aether_remote::GIT_DOWNLOAD_URL
        ));
        state.ui.status_message = format!(
            "未检测到 git，请安装 Git: {}",
            aether_remote::GIT_DOWNLOAD_URL
        );
        return;
    }
    state.ui.git_cloning = true;
    state.remote.clone_dialog.visible = false;
    state.ui.status_message = format!("正在克隆 {}...", url);
    let send_hwnd = SendHwnd(state.win.hwnd.0 as usize);
    std::thread::spawn(move || {
        let result = crate::git::GitIntegration::clone_repo(&url, &target_path);
        let payload = GitCloneResult {
            target_path: target_path.clone(),
            error: result.err(),
        };
        let raw = Box::into_raw(Box::new(payload));
        let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
        unsafe {
            post_boxed_message_wparam(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_APP + 5,
                raw,
            );
        }
    });
}

/// C-09: Git 克隆完成回调（在 UI 线程由 WM_APP+5 调用）
pub fn on_git_clone_complete(state: &mut EditorState, raw: usize) {
    state.ui.git_cloning = false;
    let payload = unsafe { Box::from_raw(raw as *mut GitCloneResult) };
    match &payload.error {
        None => {
            state.ui.status_message = format!("克隆成功: {}", payload.target_path.display());
            // 信任检查在 open_folder 之前（不持有 RefCell 借用，避免模态框重入 panic）
            if crate::editor::files::check_workspace_trust(state.win.hwnd, &payload.target_path) {
                state.open_folder(payload.target_path.clone());
            } else {
                state.ui.status_message = "已取消打开不受信任的工作区".to_string();
            }
        }
        Some(e) => {
            // 克隆失败：重新打开对话框并显示错误
            state.remote.clone_dialog.visible = true;
            state.remote.clone_dialog.error_message = Some(e.clone());
            state.ui.status_message = "克隆失败".to_string();
        }
    }
}

/// 处理克隆对话框点击
pub fn handle_clone_dialog_click(
    state: &mut EditorState,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<crate::ssh::DialogAction> {
    if let Some(rect) = &state.remote.clone_dialog.clone_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.clone_dialog.hover_button = Some(0);
            return Some(crate::ssh::DialogAction::Connect);
        }
    }
    if let Some(rect) = &state.remote.clone_dialog.cancel_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.clone_dialog.hover_button = Some(1);
            return Some(crate::ssh::DialogAction::Cancel);
        }
    }
    state.remote.clone_dialog.hover_button = None;
    Some(crate::ssh::DialogAction::None)
}

/// H-22: 仅更新克隆对话框悬停视觉状态，不触发点击动作
pub fn handle_clone_dialog_hover(state: &mut EditorState, mouse_x: f32, mouse_y: f32) {
    if let Some(rect) = &state.remote.clone_dialog.clone_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.clone_dialog.hover_button = Some(0);
            return;
        }
    }
    if let Some(rect) = &state.remote.clone_dialog.cancel_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.clone_dialog.hover_button = Some(1);
            return;
        }
    }
    state.remote.clone_dialog.hover_button = None;
}

/// P2-4: 向克隆对话框 URL 字段粘贴剪贴板内容
pub fn paste_into_clone_dialog(state: &mut EditorState) {
    if let Some(text) = EditorState::get_clipboard_text() {
        // 移除换行/回车
        state
            .remote
            .clone_dialog
            .url
            .extend(text.chars().filter(|c| *c != '\n' && *c != '\r'));
    }
}

/// 处理克隆对话框键盘输入
pub fn handle_clone_dialog_key(state: &mut EditorState, ch: char) {
    state.remote.clone_dialog.url.push(ch);
}

/// 处理克隆对话框退格
pub fn handle_clone_dialog_backspace(state: &mut EditorState) {
    state.remote.clone_dialog.url.pop();
}

impl EditorState {
    /// C-09: 异步启动 Git 克隆（后台线程执行 clone_repo，不阻塞 UI）
    pub fn start_git_clone(&mut self, url: String, target_path: PathBuf) {
        start_git_clone(self, url, target_path)
    }
    /// C-09: Git 克隆完成回调（在 UI 线程由 WM_APP+5 调用）
    pub fn on_git_clone_complete(&mut self, raw: usize) {
        on_git_clone_complete(self, raw)
    }
    /// 处理克隆对话框点击
    pub fn handle_clone_dialog_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<crate::ssh::DialogAction> {
        handle_clone_dialog_click(self, mouse_x, mouse_y)
    }
    /// H-22: 仅更新克隆对话框悬停视觉状态，不触发点击动作
    pub fn handle_clone_dialog_hover(&mut self, mouse_x: f32, mouse_y: f32) {
        handle_clone_dialog_hover(self, mouse_x, mouse_y)
    }
    /// P2-4: 向克隆对话框 URL 字段粘贴剪贴板内容
    pub fn paste_into_clone_dialog(&mut self) {
        paste_into_clone_dialog(self)
    }
    /// 处理克隆对话框键盘输入
    pub fn handle_clone_dialog_key(&mut self, ch: char) {
        handle_clone_dialog_key(self, ch)
    }
    /// 处理克隆对话框退格
    pub fn handle_clone_dialog_backspace(&mut self) {
        handle_clone_dialog_backspace(self)
    }
}
