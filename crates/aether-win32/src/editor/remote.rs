use super::*;

/// C-09: 异步启动 SSH 连接（后台线程执行 connect + list_dir，不阻塞 UI）
pub fn start_ssh_connect(state: &mut EditorState, config: aether_remote::ssh::SshConfig) {
    // P0-2: 预检 ssh.exe 是否可用，缺失时引导用户安装并跳转下载页
    if !aether_remote::ssh_available() {
        state.remote.ssh_connecting = false;
        state.remote.ssh_dialog.visible = false;
        state.ui.status_message = format!(
            "未检测到 ssh，请安装 OpenSSH: {}",
            aether_remote::ssh::SSH_DOWNLOAD_URL
        );
        state.remote.ssh_manager_panel.error_message = Some(format!(
            "系统未安装 ssh.exe，请安装 OpenSSH 后重试。\n下载页: {}",
            aether_remote::ssh::SSH_DOWNLOAD_URL
        ));
        return;
    }
    // P0-2: 预检认证方式——密码认证在 shell out 模式下不支持
    if matches!(config.auth, aether_remote::ssh::SshAuth::Password(_)) {
        state.remote.ssh_connecting = false;
        state.remote.ssh_dialog.visible = false;
        state.ui.status_message = "密码认证不支持，请使用密钥或 Agent 认证".to_string();
        state.remote.ssh_manager_panel.error_message =
            Some("密码认证在 shell out 模式下不支持（无 tty 无法交互输入密码），请配置密钥认证或 Agent 认证。".to_string());
        return;
    }
    let host = config.host.clone();
    state.remote.ssh_connecting = true;
    state.remote.ssh_dialog.visible = false;
    state.ui.status_message = format!("正在连接 {}...", host);
    let send_hwnd = SendHwnd(state.win.hwnd.0 as usize);
    std::thread::spawn(move || {
        let mut session = RemoteSession::new(config);
        let result: SshConnectResult = match session.connect() {
            Ok(()) => match session.list_current_dir() {
                Ok(entries) => SshConnectResult {
                    session: Some(session),
                    entries: Some(entries),
                    error: None,
                },
                Err(e) => SshConnectResult {
                    session: Some(session),
                    entries: None,
                    error: Some(format!("SSH 连接成功，但无法列出目录: {}", e)),
                },
            },
            Err(e) => SshConnectResult {
                session: None,
                entries: None,
                error: Some(e),
            },
        };
        let raw = Box::into_raw(Box::new(result));
        let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
        unsafe {
            post_boxed_message_wparam(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_APP + 4,
                raw,
            );
        }
    });
}

/// C-09: SSH 连接完成回调（在 UI 线程由 WM_APP+4 调用）
pub fn on_ssh_connect_complete(state: &mut EditorState, raw: usize) {
    state.remote.ssh_connecting = false;
    let payload = unsafe { Box::from_raw(raw as *mut SshConnectResult) };
    if let Some(session) = payload.session {
        state.remote.session = Some(session);
        if let Some(entries) = payload.entries {
            state.remote.file_tree = Some(RemoteFileTree::from_entries("/", entries));
            state.ui.sidebar_content = SidebarContent::RemoteFileTree;
            state.ui.status_message = "SSH 连接成功".to_string();
        } else if let Some(e) = payload.error {
            state.ui.status_message = e;
            state.remote.active_ssh_index = None;
        }
    } else if let Some(e) = payload.error {
        // 连接失败：清除活跃索引，在管理面板中显示错误
        state.remote.active_ssh_index = None;
        if state.remote.ssh_dialog.visible {
            state.remote.ssh_dialog.error_message = Some(e);
        } else {
            state.remote.ssh_manager_panel.error_message = Some(e);
        }
        state.ui.status_message = "SSH 连接失败".to_string();
    }
}

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

/// P0-1: 异步加载远程子目录内容（后台线程执行 list_dir，不阻塞 UI）
///
/// 采用与 connect/git_clone 相同的 PostMessage 异步模式。
/// 因 SshRemoteFs 每次 shell out 调用 ssh（无持久连接），后台线程用
/// `new_connected` 复用已验证的配置直接列目录，避免重复 connect 探测。
pub fn start_remote_list_dir(state: &mut EditorState, path: String) {
    // 取当前活跃会话的配置（克隆后传入后台线程）
    let config = match state.remote.session.as_ref() {
        Some(session) => session.config.clone(),
        None => return,
    };
    // 标记目标节点为 loading（防止重复触发，显示指示器）
    if let Some(tree) = state.remote.file_tree.as_mut() {
        if let Some(node) = tree.find_node_mut(&path) {
            if node.is_loading || (node.children_loaded && node.is_expanded) {
                // 已在加载或已展开，无需重复触发
                return;
            }
            node.is_loading = true;
        } else {
            return;
        }
    } else {
        return;
    }
    let send_hwnd = SendHwnd(state.win.hwnd.0 as usize);
    std::thread::spawn(move || {
        // 复用已验证配置，跳过 connect 探测（主线程已确认连接可用）
        let fs = aether_remote::ssh::SshRemoteFs::new_connected(config);
        let result: SshListDirResult = match fs.list_dir(&path) {
            Ok(entries) => SshListDirResult {
                path,
                entries: Some(entries),
                error: None,
            },
            Err(e) => SshListDirResult {
                path,
                entries: None,
                error: Some(e.to_string()),
            },
        };
        let raw = Box::into_raw(Box::new(result)) as usize;
        let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
        unsafe {
            let posted = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_APP + 6,
                windows::Win32::Foundation::WPARAM(raw),
                windows::Win32::Foundation::LPARAM(0),
            );
            if posted.is_err() {
                // P2-3: PostMessage 失败（HWND 已销毁），回收内存避免泄漏
                drop(Box::from_raw(raw as *mut SshListDirResult));
            }
        }
    });
}

/// P0-1: 远程子目录列目录完成回调（在 UI 线程由 WM_APP+6 调用）
pub fn on_ssh_list_dir_complete(state: &mut EditorState, raw: usize) {
    let payload = unsafe { Box::from_raw(raw as *mut SshListDirResult) };
    // 树可能已被清空（用户断开连接），需防御性检查
    let tree = match state.remote.file_tree.as_mut() {
        Some(t) => t,
        None => return,
    };
    if let Some(entries) = payload.entries {
        tree.expand_node(&payload.path, entries);
        state.ui.status_message = format!("已加载目录: {}", payload.path);
    } else {
        // 加载失败：清除 loading 标志，显示错误
        tree.mark_node_load_failed(&payload.path);
        if let Some(e) = payload.error {
            state.ui.status_message = format!("加载目录失败 {}: {}", payload.path, e);
        }
    }
}

// ===== SSH 管理面板方法 =====

/// 获取已保存的 SSH 服务器配置列表
pub fn ssh_servers(state: &EditorState) -> &[aether_shared::settings::SshServerConfig] {
    &state.ui.app_settings.remote.ssh_servers
}

/// 当前活跃连接数（0 或 1）
pub fn active_ssh_count(state: &EditorState) -> usize {
    if state.remote.active_ssh_index.is_some() && state.remote.session.is_some() {
        1
    } else {
        0
    }
}

/// 判断指定索引的服务器是否正在连接中
pub fn is_ssh_connecting(state: &EditorState) -> bool {
    state.remote.ssh_connecting
}

/// 判断指定索引的服务器是否已连接
pub fn is_ssh_connected(state: &EditorState, index: usize) -> bool {
    state.remote.active_ssh_index == Some(index)
        && state
            .remote
            .session
            .as_ref()
            .map(|s| s.is_connected())
            .unwrap_or(false)
}

/// 添加 SSH 服务器配置（从管理面板表单）
pub fn save_ssh_server_from_form(state: &mut EditorState) -> std::result::Result<(), String> {
    let config = state.remote.ssh_manager_panel.form_to_config()?;
    if let Some(index) = state.remote.ssh_manager_panel.edit_index {
        // 编辑现有
        if index < state.ui.app_settings.remote.ssh_servers.len() {
            state.ui.app_settings.remote.ssh_servers[index] = config;
        }
    } else {
        // 新增
        state.ui.app_settings.remote.ssh_servers.push(config);
    }
    state.remote.ssh_manager_panel.editing = false;
    state.remote.ssh_manager_panel.edit_index = None;
    // P1-3: 持久化失败时向用户反馈，不再静默丢弃
    if let Err(e) = state.ui.app_settings.save() {
        state.ui.status_message = format!("配置保存失败: {}", e);
    }
    Ok(())
}

/// 删除 SSH 服务器配置
pub fn delete_ssh_server(state: &mut EditorState, index: usize) {
    if index < state.ui.app_settings.remote.ssh_servers.len() {
        // 如果正在连接该服务器，先断开
        if state.remote.active_ssh_index == Some(index) {
            disconnect_ssh(state);
        }
        // 调整 active_ssh_index（删除后索引偏移）
        if let Some(ai) = state.remote.active_ssh_index {
            if ai > index {
                state.remote.active_ssh_index = Some(ai - 1);
            } else if ai == index {
                state.remote.active_ssh_index = None;
            }
        }
        state.ui.app_settings.remote.ssh_servers.remove(index);
        // P1-3: 持久化失败时向用户反馈，不再静默丢弃
        if let Err(e) = state.ui.app_settings.save() {
            state.ui.status_message = format!("配置保存失败: {}", e);
        } else {
            state.ui.status_message = "已删除服务器配置".to_string();
        }
    }
}

/// 异步连接指定索引的 SSH 服务器
pub fn connect_ssh_server(state: &mut EditorState, index: usize) {
    if state.remote.ssh_connecting {
        return;
    }
    let servers = &state.ui.app_settings.remote.ssh_servers;
    if index >= servers.len() {
        return;
    }
    let config = &servers[index];
    // P0-2: 认证凭证预检——密码认证不支持，在启动后台连接前拦截
    if config.auth_type.as_str() == "password" {
        state.remote.ssh_manager_panel.error_message = Some(
            "密码认证在 shell out 模式下不支持（无 tty 无法交互输入密码），请编辑该服务器配置为密钥认证或 Agent 认证。".to_string(),
        );
        state.ui.status_message = "密码认证不支持，请使用密钥或 Agent 认证".to_string();
        return;
    }
    let ssh_config = crate::ssh::SshManagerPanel::config_to_ssh_config(config);
    let server_name = config.name.clone();
    // 如果已有连接，先断开
    if state.remote.session.is_some() {
        disconnect_ssh(state);
    }
    state.remote.active_ssh_index = Some(index);
    start_ssh_connect(state, ssh_config);
    state.ui.status_message = format!("正在连接 {}...", server_name);
}

/// 断开当前 SSH 连接
pub fn disconnect_ssh(state: &mut EditorState) {
    if let Some(mut session) = state.remote.session.take() {
        session.disconnect();
    }
    state.remote.active_ssh_index = None;
    state.remote.file_tree = None;
    state.remote.ssh_connecting = false;
    state.ui.status_message = "SSH 已断开".to_string();
}

pub(super) fn handle_remote_tree_click(state: &mut EditorState, _mouse_x: f32, mouse_y: f32) -> bool {
    // P0-1: 递归遍历可见节点，按 y 坐标命中目标节点。
    // 在独立作用域内完成对树的只读借用，收集所需信息后释放借用，
    // 避免与后续 &mut state 调用（start_remote_list_dir 等）冲突。
    let (path, is_dir, node_state) = {
        let tree = match state.remote.file_tree.as_ref() {
            Some(t) => t,
            None => return false,
        };
        let node_height = 16.0_f32;
        let mut current_y = 10.0 - state.remote.scroll_y;
        let target =
            find_remote_node_at_y(&tree.nodes, mouse_y, node_height, &mut current_y);
        let (path, is_dir) = match target {
            Some(t) => t,
            None => return false,
        };
        let state_info = tree
            .find_node(&path)
            .map(|n| (n.is_dir, n.is_expanded, n.children_loaded, n.is_loading));
        (path, is_dir, state_info)
    };

    if is_dir {
        // P0-1: 目录节点的展开/折叠/懒加载逻辑
        let (is_dir, is_expanded, children_loaded, is_loading) = match node_state {
            Some(s) => s,
            None => return false,
        };
        if !is_dir || is_loading {
            return false;
        }
        if !children_loaded {
            // 首次展开：异步加载子目录
            start_remote_list_dir(state, path);
            return true;
        }
        // 子节点已加载：切换展开/折叠
        if let Some(tree) = state.remote.file_tree.as_mut() {
            if let Some(n) = tree.find_node_mut(&path) {
                n.is_expanded = !is_expanded;
            }
        }
        true
    } else {
        // 打开远程文件
        state.remote.selected_node = Some(path.clone());
        if let Some(session) = &state.remote.session {
            let remote_path = path.clone();
            match session.read_remote_file(&remote_path) {
                Ok(content) => {
                    let text = String::from_utf8_lossy(&content).to_string();
                    let tab =
                        crate::tabs::Tab::File(crate::tabs::TabContent::with_loaded_buffer(
                            Some(PathBuf::from(format!("remote:{}", remote_path))),
                            PieceTable::from_string(text),
                            Language::PlainText,
                            false,
                        ));
                    state.open_in_new_tab(tab);
                    state.ui.status_message = format!("已打开远程文件: {}", remote_path);
                }
                Err(e) => {
                    state.ui.status_message = format!("读取远程文件失败: {}", e);
                }
            }
        }
        true
    }
}

/// P0-1: 递归查找 y 坐标命中的可见远程节点，返回 (path, is_dir)
///
/// `current_y` 从顶部起始（含滚动偏移），逐节点递增 node_height。
/// 仅遍历展开目录的子节点。
pub(super) fn find_remote_node_at_y(
    nodes: &[crate::ssh::RemoteFileNode],
    mouse_y: f32,
    node_height: f32,
    current_y: &mut f32,
) -> Option<(String, bool)> {
    for node in nodes {
        if mouse_y >= *current_y && mouse_y < *current_y + node_height {
            return Some((node.path.clone(), node.is_dir));
        }
        *current_y += node_height;
        if node.is_expanded {
            if let Some(found) =
                find_remote_node_at_y(&node.children, mouse_y, node_height, current_y)
            {
                return Some(found);
            }
        }
    }
    None
}

pub(super) fn update_remote_tree_hover(state: &mut EditorState, mouse_y: f32) -> bool {
    let tree = match state.remote.file_tree.as_ref() {
        Some(t) => t,
        None => {
            let old = state.remote.hover_node.take();
            return old.is_some();
        }
    };
    // P0-1: 递归遍历可见节点确定悬停目标（按路径标识）
    let node_height = 16.0_f32;
    let mut current_y = 10.0 - state.remote.scroll_y;
    let new_hover =
        find_remote_node_at_y(&tree.nodes, mouse_y, node_height, &mut current_y)
            .map(|(path, _)| path);
    let changed = state.remote.hover_node != new_hover;
    state.remote.hover_node = new_hover;
    changed
}

/// 处理 SSH 对话框点击
pub fn handle_ssh_dialog_click(
    state: &mut EditorState,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<crate::ssh::DialogAction> {
    if let Some(rect) = &state.remote.ssh_dialog.connect_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.ssh_dialog.hover_button = Some(0);
            return Some(crate::ssh::DialogAction::Connect);
        }
    }
    if let Some(rect) = &state.remote.ssh_dialog.cancel_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.ssh_dialog.hover_button = Some(1);
            return Some(crate::ssh::DialogAction::Cancel);
        }
    }
    state.remote.ssh_dialog.hover_button = None;
    Some(crate::ssh::DialogAction::None)
}

/// H-22: 仅更新悬停视觉状态，不触发点击动作的副作用
pub fn handle_ssh_dialog_hover(state: &mut EditorState, mouse_x: f32, mouse_y: f32) {
    if let Some(rect) = &state.remote.ssh_dialog.connect_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.ssh_dialog.hover_button = Some(0);
            return;
        }
    }
    if let Some(rect) = &state.remote.ssh_dialog.cancel_btn_rect {
        if rect.contains(mouse_x, mouse_y) {
            state.remote.ssh_dialog.hover_button = Some(1);
            return;
        }
    }
    state.remote.ssh_dialog.hover_button = None;
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

/// 处理 SSH 对话框键盘输入
pub fn handle_ssh_dialog_key(state: &mut EditorState, ch: char) {
    // P2-4: 复用 paste 路径的字段解析逻辑
    // 先读取 focus_field 避免与可变借用冲突
    let focus = state.remote.ssh_dialog.focus_field;
    if let Some(field) = ssh_dialog_active_field_mut(state) {
        // port 字段（focus_field == 1）只接受数字
        let is_port = focus == 1;
        if !is_port || ch.is_ascii_digit() {
            field.push(ch);
        }
    }
}

/// P2-4: 返回 SSH 对话框当前 focus_field 对应的可变字段引用
pub(super) fn ssh_dialog_active_field_mut(state: &mut EditorState) -> Option<&mut String> {
    match state.remote.ssh_dialog.focus_field {
        0 => Some(&mut state.remote.ssh_dialog.host),
        1 => Some(&mut state.remote.ssh_dialog.port),
        2 => Some(&mut state.remote.ssh_dialog.username),
        3 => match state.remote.ssh_dialog.auth_type {
            crate::ssh::SshAuthType::Password => Some(&mut state.remote.ssh_dialog.password),
            crate::ssh::SshAuthType::Key => Some(&mut state.remote.ssh_dialog.key_path),
            crate::ssh::SshAuthType::Agent => None,
        },
        4 => Some(&mut state.remote.ssh_dialog.key_passphrase),
        _ => None,
    }
}

/// P2-4: 向 SSH 对话框当前字段粘贴剪贴板内容（port 字段过滤非数字）
pub fn paste_into_ssh_dialog(state: &mut EditorState) {
    if let Some(text) = EditorState::get_clipboard_text() {
        // 先读取 focus_field 避免与可变借用冲突
        let focus = state.remote.ssh_dialog.focus_field;
        if let Some(field) = ssh_dialog_active_field_mut(state) {
            let is_port = focus == 1;
            if is_port {
                // 仅保留数字字符
                field.extend(text.chars().filter(|c| c.is_ascii_digit()));
            } else {
                // 移除换行/回车避免破坏单行输入
                field.extend(text.chars().filter(|c| *c != '\n' && *c != '\r'));
            }
        }
    }
}

/// P2-4: 向克隆对话框 URL 字段粘贴剪贴板内容
pub fn paste_into_clone_dialog(state: &mut EditorState) {
    if let Some(text) = EditorState::get_clipboard_text() {
        // 移除换行/回车
        state.remote
            .clone_dialog
            .url
            .extend(text.chars().filter(|c| *c != '\n' && *c != '\r'));
    }
}

/// 处理 SSH 对话框退格
pub fn handle_ssh_dialog_backspace(state: &mut EditorState) {
    match state.remote.ssh_dialog.focus_field {
        0 => {
            state.remote.ssh_dialog.host.pop();
        }
        1 => {
            state.remote.ssh_dialog.port.pop();
        }
        2 => {
            state.remote.ssh_dialog.username.pop();
        }
        3 => match state.remote.ssh_dialog.auth_type {
            crate::ssh::SshAuthType::Password => {
                state.remote.ssh_dialog.password.pop();
            }
            crate::ssh::SshAuthType::Key => {
                state.remote.ssh_dialog.key_path.pop();
            }
            crate::ssh::SshAuthType::Agent => {}
        },
        4 => {
            state.remote.ssh_dialog.key_passphrase.pop();
        }
        _ => {}
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
    /// C-09: 异步启动 SSH 连接（后台线程执行 connect + list_dir，不阻塞 UI）
    pub fn start_ssh_connect(&mut self, config: aether_remote::ssh::SshConfig) {
        start_ssh_connect(self, config)
    }
    /// C-09: SSH 连接完成回调（在 UI 线程由 WM_APP+4 调用）
    pub fn on_ssh_connect_complete(&mut self, raw: usize) {
        on_ssh_connect_complete(self, raw)
    }
    /// C-09: 异步启动 Git 克隆（后台线程执行 clone_repo，不阻塞 UI）
    pub fn start_git_clone(&mut self, url: String, target_path: PathBuf) {
        start_git_clone(self, url, target_path)
    }
    /// C-09: Git 克隆完成回调（在 UI 线程由 WM_APP+5 调用）
    pub fn on_git_clone_complete(&mut self, raw: usize) {
        on_git_clone_complete(self, raw)
    }
    /// P0-1: 异步加载远程子目录内容（后台线程执行 list_dir，不阻塞 UI）
    pub fn start_remote_list_dir(&mut self, path: String) {
        start_remote_list_dir(self, path)
    }
    /// P0-1: 远程子目录列目录完成回调（在 UI 线程由 WM_APP+6 调用）
    pub fn on_ssh_list_dir_complete(&mut self, raw: usize) {
        on_ssh_list_dir_complete(self, raw)
    }
    /// 获取已保存的 SSH 服务器配置列表
    pub fn ssh_servers(&self) -> &[aether_shared::settings::SshServerConfig] {
        ssh_servers(self)
    }
    /// 当前活跃连接数（0 或 1）
    pub fn active_ssh_count(&self) -> usize {
        active_ssh_count(self)
    }
    /// 判断指定索引的服务器是否正在连接中
    pub fn is_ssh_connecting(&self) -> bool {
        is_ssh_connecting(self)
    }
    /// 判断指定索引的服务器是否已连接
    pub fn is_ssh_connected(&self, index: usize) -> bool {
        is_ssh_connected(self, index)
    }
    /// 添加 SSH 服务器配置（从管理面板表单）
    pub fn save_ssh_server_from_form(&mut self) -> std::result::Result<(), String> {
        save_ssh_server_from_form(self)
    }
    /// 删除 SSH 服务器配置
    pub fn delete_ssh_server(&mut self, index: usize) {
        delete_ssh_server(self, index)
    }
    /// 异步连接指定索引的 SSH 服务器
    pub fn connect_ssh_server(&mut self, index: usize) {
        connect_ssh_server(self, index)
    }
    /// 断开当前 SSH 连接
    pub fn disconnect_ssh(&mut self) {
        disconnect_ssh(self)
    }
    pub(super) fn handle_remote_tree_click(&mut self, _mouse_x: f32, mouse_y: f32) -> bool {
        handle_remote_tree_click(self, _mouse_x, mouse_y)
    }
    pub(super) fn update_remote_tree_hover(&mut self, mouse_y: f32) -> bool {
        update_remote_tree_hover(self, mouse_y)
    }
    /// 处理 SSH 对话框点击
    pub fn handle_ssh_dialog_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<crate::ssh::DialogAction> {
        handle_ssh_dialog_click(self, mouse_x, mouse_y)
    }
    /// H-22: 仅更新悬停视觉状态，不触发点击动作的副作用
    pub fn handle_ssh_dialog_hover(&mut self, mouse_x: f32, mouse_y: f32) {
        handle_ssh_dialog_hover(self, mouse_x, mouse_y)
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
    /// 处理 SSH 对话框键盘输入
    pub fn handle_ssh_dialog_key(&mut self, ch: char) {
        handle_ssh_dialog_key(self, ch)
    }
    /// P2-4: 向 SSH 对话框当前字段粘贴剪贴板内容（port 字段过滤非数字）
    pub fn paste_into_ssh_dialog(&mut self) {
        paste_into_ssh_dialog(self)
    }
    /// P2-4: 向克隆对话框 URL 字段粘贴剪贴板内容
    pub fn paste_into_clone_dialog(&mut self) {
        paste_into_clone_dialog(self)
    }
    /// 处理 SSH 对话框退格
    pub fn handle_ssh_dialog_backspace(&mut self) {
        handle_ssh_dialog_backspace(self)
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
