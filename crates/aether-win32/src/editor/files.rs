use super::*;

/// 检查当前标签页是否可以重用（空文件且未修改）
pub(super) fn can_reuse_current_tab(state: &EditorState) -> bool {
    state.editor.content.file_path.is_none()
        && !state.editor.content.is_dirty
        && state.editor.content.buffer.len_bytes() == 0
}

/// 重置当前编辑状态到初始值
pub(super) fn reset_editor_state(state: &mut EditorState) {
    state.editor.content.cursor_line = 0;
    state.editor.content.cursor_col = 0;
    state.editor.content.scroll_y = 0.0;
    state.editor.content.history.clear();
    state.editor.content.is_dirty = false;
    state.editor.content.buffer_version += 1;
    state.clear_selection();
}

/// 在新标签页中打开内容
pub(super) fn open_in_new_tab(state: &mut EditorState, tab: Tab) {
    // REQ-P1-09: save current state to old tab, push new tab, swap it in
    state.swap_tab_content(state.editor.tab_bar.active_tab);
    // 直接将新标签页追加到末尾并切换过去。
    // 此前使用 swap(tabs[active], placeholder) + push(placeholder) 的写法，
    // 会让新 tab 留在原 active 位置、旧 tab 被推到末尾，但 active_tab 又被
    // 设置为 len()-1，结果指向了旧 tab，导致打开第二个文件时仍显示旧内容、
    // LSP did_open 也发给了旧文件。改为直接 push 新 tab 即可。
    state.editor.tab_bar.tabs.push(tab);
    state.editor.tab_bar.active_tab = state.editor.tab_bar.tabs.len() - 1;
    state.swap_tab_content(state.editor.tab_bar.active_tab);
    state.editor.is_selecting = false;
    state.emit_event(crate::events::EditorEvent::TabChanged);
    // 标记标签栏和编辑器区域脏区，避免新标签打开时触发全窗口重绘
    let editor_region = state.ui.layout.editor_region();
    let tab_region = state.ui.layout.tab_bar_region(state.show_tab_bar());
    state.win.dirty_tracker.mark_region(
        editor_region.x,
        editor_region.y,
        editor_region.width,
        editor_region.height,
        crate::dirty_rect::DirtyRegionType::EditorContent,
    );
    state.win.dirty_tracker.mark_region(
        tab_region.x,
        tab_region.y,
        tab_region.width,
        tab_region.height,
        crate::dirty_rect::DirtyRegionType::TabBar,
    );
}

pub fn load_file(state: &mut EditorState, path: PathBuf) {
    let lang = Language::from_path(&path);

    if lang == Language::Image {
        load_image_file(state, path);
        return;
    }

    if !is_text_file(&path) {
        show_unsupported_file(state, &path);
        return;
    }

    match PieceTable::from_file(&path) {
        Ok(buffer) => {
            // 仅当存在标签页时才复用当前空标签：tabs 为空（如启动后打开的第一个文件）
            // 时必须走新建标签路径，否则 show_empty_placeholder() 仍为 true，
            // 编辑器区域渲染占位页导致文件"点不开"，要再点一次才能打开。
            if can_reuse_current_tab(state) && !state.editor.tab_bar.tabs.is_empty() {
                state.editor.content.buffer = buffer;
                state.editor.content.file_path = Some(path.clone());
                state.editor.content.language = lang;
                state.editor.content.markdown_preview = false;
                state.editor.markdown_preview = false;
                reset_editor_state(state);
                // REQ-P1-09: state.editor.content 即活动标签页状态，无需再手动同步到 Tab
                state.ui.status_message = format!("已打开: {}", path.display());
            } else {
                let tab = Tab::File(TabContent::with_loaded_buffer(
                    Some(path.clone()),
                    buffer,
                    lang,
                    false,
                ));
                open_in_new_tab(state, tab);
                state.ui.status_message = format!("已打开: {}", path.display());
            }
            state.emit_event(crate::events::EditorEvent::TextChanged {
                start_line: 0,
                end_line: state.editor.content.buffer.len_lines(),
            });
            state.emit_event(crate::events::EditorEvent::StatusBarChanged);
            // 接线 LSP：通知服务器文档已打开（按需启动 server），激活补全/悬停/诊断
            // 冰冻态：先解冻重启语言服务器（解冻路径内已 notify_open 当前文档）
            if state.lsp.lsp.frozen {
                state.thaw_lsp_on_demand();
            } else {
                state.lsp.lsp.notify_open(&state.editor.content);
            }

            state.update_large_file_flag();
        }
        Err(e) => {
            let msg = format!("打开文件失败: {}", e);
            state.ui.status_message = msg.clone();
            Dialogs::show_error(state.win.hwnd, "打开文件", &msg);
        }
    }

    // 文件加载成功后通知 LSP 服务器。
    // 注：lsp_notify_open() 已在上面调用过（会按需启动 server 并 send did_open），
    // 此处无需重复 get_text + lsp_open_document，避免对 UI 线程造成双倍的
    // 全文件拷贝（get_text 是 O(N) String 分配，对大文件耗时明显）。
}

/// 加载图片文件
pub(super) fn load_image_file(state: &mut EditorState, path: PathBuf) {
    // 解码图片（自动嗅探格式；GIF 取首帧；SVG/RAW/PSD/损坏文件返回 Err → 占位提示）
    let image_data = match crate::bitmap_loader::decode_image_file(&path) {
        Ok(img) => Some(img),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "图片解码失败，显示占位提示");
            None
        }
    };
    // 打开新图片前使旧位图缓存失效（位图绑定具体图片）
    state.win.image_bitmap = None;
    // 重置缩放状态
    state.win.image_zoom = 1.0;
    state.win.image_offset_x = 0.0;
    state.win.image_offset_y = 0.0;

    let content = format!("[图片预览] {}", path.display());
    // tabs 为空时同样不能复用（否则渲染占位页），与 load_file 保持一致
    if can_reuse_current_tab(state) && !state.editor.tab_bar.tabs.is_empty() {
        state.editor.content.file_path = Some(path.clone());
        state.editor.content.language = Language::Image;
        state.editor.content.buffer = PieceTable::from_string(content);
        state.editor.content.image_data = image_data;
        reset_editor_state(state);
        state.ui.status_message = format!("已打开图片: {}", path.display());
    } else {
        let mut tab_content = TabContent::with_loaded_buffer(
            Some(path.clone()),
            PieceTable::from_string(content),
            Language::Image,
            false,
        );
        tab_content.image_data = image_data;
        let tab = Tab::File(tab_content);
        open_in_new_tab(state, tab);
        state.ui.status_message = format!("已打开图片: {}", path.display());
    }
}

/// 显示不支持的文件提示
pub(super) fn show_unsupported_file(state: &mut EditorState, path: &Path) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown");
    let message = format!("不支持的文件格式: .{}\n文件: {}", ext, path.display());
    // tabs 为空时同样不能复用（否则渲染占位页），与 load_file 保持一致
    if can_reuse_current_tab(state) && !state.editor.tab_bar.tabs.is_empty() {
        state.editor.content.file_path = Some(path.to_path_buf());
        state.editor.content.language = Language::PlainText;
        state.editor.content.buffer = PieceTable::from_string(message);
        reset_editor_state(state);
        state.ui.status_message = format!("不支持的文件格式: .{}", ext);
    } else {
        let tab = Tab::File(TabContent::with_loaded_buffer(
            Some(path.to_path_buf()),
            PieceTable::from_string(message),
            Language::PlainText,
            false,
        ));
        open_in_new_tab(state, tab);
        state.ui.status_message = format!("不支持的文件格式: .{}", ext);
    }
}

/// P4-2: 原子写入文件，避免写入中途崩溃导致文件损坏
/// 先写入同目录的临时文件并 fsync，再原子 rename 替换目标文件
#[allow(dead_code)]
pub(super) fn atomic_write(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::path::Path;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = dir.join(format!(
        ".aether-save-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file); // 关闭句柄后再 rename
        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();

    // 任何步骤失败时清理临时文件
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// 流式原子写入：通过回调函数写入数据，避免在内存中构造完整的 &[u8]。
/// 用于保存大文件时避免 get_all_text 的中间 String/Vec 分配。
/// 语义与 atomic_write 一致：临时文件 + fsync + rename。
pub(super) fn atomic_write_stream<F>(
    path: &std::path::Path,
    writer_fn: F,
) -> std::io::Result<()>
where
    F: FnOnce(&mut std::fs::File) -> std::io::Result<()>,
{
    use std::path::Path;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = dir.join(format!(
        ".aether-save-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp_path)?;
        writer_fn(&mut file)?;
        file.sync_all()?;
        drop(file); // 关闭句柄后再 rename
        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// 保存文件，返回是否成功
pub fn save_file(state: &mut EditorState) -> bool {
    if let Some(path) = &state.editor.content.file_path.clone() {
        // 处理远程文件保存
        if let Some(remote_path) = path.to_str().and_then(|s| s.strip_prefix("remote:")) {
            // 远程路径仍需 &[u8]，这里不得不做一次全量拷贝
            let mut buf: Vec<u8> = Vec::with_capacity(state.editor.content.buffer.len_bytes());
            if let Err(e) = state.editor.content.buffer.write_to(&mut buf) {
                state.ui.status_message = format!("保存失败: {}", e);
                return false;
            }
            if let Some(session) = &state.remote.session {
                match session.write_remote_file(remote_path, &buf) {
                    Ok(()) => {
                        state.editor.content.is_dirty = false;
                        state.ui.status_message = format!("已保存到远程: {}", remote_path);
                        // 同步自动保存状态（去重基线 / 冲突复位 / 停止防抖）
                        state.note_save_succeeded();
                        return true;
                    }
                    Err(e) => {
                        state.ui.status_message = format!("保存远程文件失败: {}", e);
                        return false;
                    }
                }
            } else {
                state.ui.status_message = "远程会话未连接".to_string();
                return false;
            }
        }
        // 本地文件保存：直接将 buffer 流式写入临时文件，避免 get_all_text 的
        // 全量 String 分配和 UTF-8 lossy 转换。对未编辑的 mmap 大文件尤其显著。
        // P4-2: 仍保持原子写入语义（临时文件 + fsync + rename）。
        match atomic_write_stream(path, |w| state.editor.content.buffer.write_to(w)) {
            Ok(()) => {
                state.editor.content.is_dirty = false;
                state.ui.status_message = "已保存".to_string();
                // 同步自动保存状态（去重基线 / 冲突复位 / mtime 刷新 / 停止防抖）
                state.note_save_succeeded();
                true
            }
            Err(e) => {
                state.ui.status_message = format!("保存失败: {}", e);
                false
            }
        }
    } else {
        state.ui.status_message = "没有文件路径，请使用另存为".to_string();
        false
    }
}

/// 另存为
pub fn save_as(state: &mut EditorState, path: PathBuf) -> bool {
    match atomic_write_stream(&path, |w| state.editor.content.buffer.write_to(w)) {
        Ok(()) => {
            state.editor.content.file_path = Some(path.clone());
            state.editor.content.is_dirty = false;
            state.ui.status_message = format!("已保存: {}", path.display());
            // 同步自动保存状态（去重基线 / 冲突复位 / mtime 刷新 / 停止防抖）
            state.note_save_succeeded();
            true
        }
        Err(e) => {
            state.ui.status_message = format!("保存失败: {}", e);
            false
        }
    }
}

pub fn open_folder(state: &mut EditorState, path: PathBuf) {
    // 异步扫描：先快速同步验证路径可读，再启动后台线程扫描根层
    // 同步预检避免无效路径白白启动线程
    if let Err(e) = std::fs::read_dir(&path) {
        let msg = format!("打开文件夹失败: {}", e);
        state.ui.status_message = msg.clone();
        Dialogs::show_error(state.win.hwnd, "打开文件夹", &msg);
        return;
    }

    // 工作区信任检查已上移至调用方（check_workspace_trust），
    // 避免在持有 RefCell borrow_mut 期间弹模态框泵消息导致重入 panic。
    // 此处假定调用方已完成信任确认。

    // 【时序关键】先保存旧工作区的 AI 会话快照，再切换到新工作区。
    // 若颠倒顺序，save 时 current_folder 已是新工作区，
    // 旧工作区的对话会被错误地保存到新工作区的哈希名下。
    save_current_workspace_ai_session(state);

    // 设置 loading 状态，立即重绘显示 spinner
    state.fs.is_loading_folder = true;
    state.fs.folder_generation = state.fs.folder_generation.wrapping_add(1);
    let generation = state.fs.folder_generation;
    state.fs.current_folder = Some(path.clone());
    // 工作区哈希绑定：后续对话归档将关联该工作区（VS Code workspaceStorage 同款）
    if let Some(warm) = state.ai.ai_panel.warm_data_store.as_ref() {
        warm.set_workspace(&path);
    }
    // 同步终端工作目录到新工作区
    state.terminal.terminal_panel.cwd = path.to_string_lossy().to_string();
    // 立即持久化 last_workspace（读盘改写，避免其它窗口的陈旧副本覆写）
    state.ui.app_settings.ui.last_workspace = state.fs.current_folder.clone();
    if let Err(e) = aether_shared::settings::AppSettings::persist_last_workspace(Some(&path)) {
        eprintln!("警告: 保存 last_workspace 失败: {}", e);
    }
    state.ui.status_message = format!("正在扫描: {}...", path.display());
    state.ui.recent_projects.add(&path);

    // 工作区 AI 会话隔离：加载目标工作区的会话
    // （保存已在上面完成，此处仅加载）
    let workspace_hash = workspace_path_hash(&path);
    load_workspace_ai_session(state, &workspace_hash);

    state.fs.file_tree = Some(FileTree::new());
    // 重置文件树缓存与交互状态，防止旧索引在新树中越界/错位
    state.fs.file_tree_visible_rows.clear();
    state.fs.file_tree_rows_dirty = true;
    state.fs.file_tree_rows_tree_len = 0;
    state.fs.selected_file_node = None;
    state.fs.hover_file_node = None;
    state.fs.hover_file_tree_root = false;
    state.fs.file_tree_input = None;
    state.fs.sidebar_scroll_y = 0.0;
    // UI-T01: 工作区切换后标题栏需要立即更新，标记全窗口重绘
    state.win.dirty_tracker.mark_full_window();

    // 初始化 LSP 客户端（启动 rust-analyzer 等语言服务器）
    // 先清空旧工作区的诊断表/补全结果：诊断按 Url 存储，
    // 不清理会随切换过的工作区只增不减地驻留内存
    state.lsp.lsp.diagnostics.clear();
    state.lsp.lsp.completion_items.clear();
    state.lsp.lsp.completion_visible = false;
    state.lsp.lsp.init(&path);

    let hwnd = state.win.hwnd;
    let path_clone = path.clone();
    // HWND 不是 Send，但实际只是个指针，PostMessageW 是线程安全的
    // 用 SendHwnd 包装以通过类型检查
    let send_hwnd = SendHwnd(hwnd.0 as usize);
    std::thread::spawn(move || {
        let entries = scan_file_tree_entries(&path_clone);
        const BATCH_SIZE: usize = 50;
        for chunk in entries.chunks(BATCH_SIZE) {
            let batch = ScannedBatch {
                generation,
                entries: chunk.to_vec(),
                complete: false,
            };
            let ptr = Box::into_raw(Box::new(batch));
            let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
            unsafe {
                post_boxed_message_lparam(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::WM_APP + 7,
                    ptr,
                );
            }
        }
        let complete = ScannedBatch {
            generation,
            entries: Vec::new(),
            complete: true,
        };
        let ptr = Box::into_raw(Box::new(complete));
        let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
        unsafe {
            post_boxed_message_lparam(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::WM_APP + 7,
                ptr,
            );
        }
    });
}

/// H-09: 接收 &ScannedBatch 引用，由调用方负责 Box 的 drop
pub(crate) fn on_folder_scan_batch_ref(state: &mut EditorState, batch: &ScannedBatch) {
    if batch.generation != state.fs.folder_generation {
        return;
    }
    if batch.complete {
        state.fs.is_loading_folder = false;
        if let Some(folder) = state.fs.current_folder.clone() {
            state.ui.git.detect(&folder);
            if let Some(branch) = state.ui.git.current_branch_name() {
                state.ui.status_bar.update_git_branch(Some(&branch));
            } else {
                state.ui.status_bar.update_git_branch(None);
            }
            state.ui.status_message = format!("已打开文件夹: {}", folder.display());
            state.ui.welcome_focus_action = None;
            // 自动打开 README（若存在）
            try_open_readme(state, &folder);
        }
        return;
    }
    if let Some(ref mut tree) = state.fs.file_tree {
        for entry in &batch.entries {
            tree.add_node(&entry.name, entry.kind, u32::MAX, entry.depth);
        }
        // 节点数变化，可见行数组需重建
        state.mark_file_tree_rows_dirty();
    }
}

/// 在打开的文件夹根目录查找 README 并自动加载
/// P2-7: 仅在当前标签页为空且未修改时才自动加载，避免覆盖用户已有内容
pub(super) fn try_open_readme(state: &mut EditorState, folder: &Path) {
    // 当前标签页有内容或未保存的修改时，不自动加载 README
    if state.editor.content.is_dirty
        || state.editor.content.buffer.len_bytes() > 0
        || state.editor.content.file_path.is_some()
    {
        return;
    }
    let candidates = ["README.md", "README.MD", "README", "readme.md", "Readme.md"];
    for name in candidates {
        let readme_path = folder.join(name);
        if readme_path.is_file() {
            load_file(state, readme_path);
            return;
        }
    }
}

pub fn close_workspace(state: &mut EditorState) {
    // 保存当前工作区的 AI 会话状态
    save_current_workspace_ai_session(state);

    // 清空 AI 面板：关闭工作区后对话标签页不应残留
    state.ai.ai_panel.conversations = vec![crate::ai_panel::AiConversation::new(
        crate::ai_panel::gen_conversation_id(),
        "新对话".to_string(),
    )];
    state.ai.ai_panel.load_slot_into_active(0);
    state.ai.current_workspace_ai_session = None;
    // 清除温数据存储的工作区绑定，后续归档不再关联旧工作区
    if let Some(warm) = state.ai.ai_panel.warm_data_store.as_ref() {
        warm.set_workspace(std::path::Path::new(""));
    }

    state.fs.file_tree = None;
    state.fs.current_folder = None;
    // 同步清空持久化的 last_workspace，避免下次启动重新打开已被用户主动关闭的工作区
    state.ui.app_settings.ui.last_workspace = None;
    if let Err(e) = aether_shared::settings::AppSettings::persist_last_workspace(None) {
        eprintln!("警告: 清除 last_workspace 失败: {}", e);
    }
    state.editor.content.file_path = None;
    state.editor.content.buffer = PieceTable::from_string(String::new());
    state.editor.content.cursor_line = 0;
    state.editor.content.cursor_col = 0;
    state.editor.content.scroll_y = 0.0;
    state.editor.content.selection_start = None;
    state.editor.content.selection_end = None;
    state.editor.content.is_dirty = false;
    state.editor.content.cached_lines.clear();
    state.editor.content.line_cache_versions.clear();
    state.editor.content.cache_window_start = 0;
    state.editor.content.cached_tokens.clear();
    state.editor.content.language = Language::PlainText;
    state.editor.tab_bar.tabs.clear();
    state.editor.tab_bar.tabs.push(crate::tabs::Tab::new());
    state.editor.tab_bar.active_tab = 0;
    state.fs.selected_file_node = None;
    state.fs.hover_file_node = None;
    state.fs.hover_file_tree_root = false;
    state.fs.file_tree_input = None;
    state.fs.file_tree_visible_rows.clear();
    state.fs.file_tree_rows_dirty = true;
    state.fs.file_tree_rows_tree_len = 0;
    state.fs.sidebar_scroll_y = 0.0;
    state.ui.welcome_focus_action = None;
    // 释放旧工作区的 LSP 诊断与补全缓存（否则随 Url key 永久驻留）
    state.lsp.lsp.diagnostics.clear();
    state.lsp.lsp.completion_items.clear();
    state.lsp.lsp.completion_visible = false;
    state.ui.git.detect(std::path::Path::new("."));
    state.ui.status_bar.update_git_branch(None);
    state.ui.status_message = "已关闭工作区".to_string();
    // UI-T01: 关闭工作区后标题栏需要立即恢复为应用名
    state.win.dirty_tracker.mark_full_window();
}

/// 保存当前工作区的 AI 会话状态（完整标签页组快照）
fn save_current_workspace_ai_session(state: &mut EditorState) {
    if let Some(ref folder) = state.fs.current_folder {
        let workspace_hash = workspace_path_hash(folder);
        // 先把活动会话的扁平状态回填到槽位，保证快照完整
        state.ai.ai_panel.snapshot_active_into_slot();
        // 归档所有非空会话到温数据层（异步，不阻塞切换）
        for conv in &state.ai.ai_panel.conversations {
            let has_user_msg = conv
                .messages
                .iter()
                .any(|m| m.role == crate::ai_panel::AiRole::User);
            if has_user_msg && !conv.hibernated {
                if let Some(warm) = state.ai.ai_panel.warm_data_store.as_ref() {
                    warm.request_archive(conv.id.clone(), conv.clone());
                }
            }
        }
        // 保存完整标签页组快照
        let snapshot = crate::editor::WorkspaceAiSessionSnapshot {
            conversations: state.ai.ai_panel.conversations.clone(),
            active: state.ai.ai_panel.active,
        };
        state.ai.workspace_ai_sessions.insert(workspace_hash, snapshot);
    }
}

/// 加载目标工作区的 AI 会话（恢复标签页组或从数据库加载）
fn load_workspace_ai_session(state: &mut EditorState, workspace_hash: &str) {
    // 1. 优先从内存快照恢复该工作区之前的标签页组
    if let Some(snapshot) = state.ai.workspace_ai_sessions.get(workspace_hash).cloned() {
        state.ai.ai_panel.conversations = snapshot.conversations;
        let active = snapshot
            .active
            .min(state.ai.ai_panel.conversations.len().saturating_sub(1));
        if !state.ai.ai_panel.conversations.is_empty() {
            state.ai.ai_panel.load_slot_into_active(active);
        }
        state.ai.current_workspace_ai_session = state
            .ai.ai_panel
            .conversations
            .get(active)
            .map(|c| c.id.clone());
        return;
    }

    // 2. 无内存快照：从温数据存储按工作区过滤加载该工作区的会话
    if let Some(ref warm_store) = state.ai.ai_panel.warm_data_store {
        // 强制按工作区过滤（不依赖 history_workspace_only 的 UI 状态）
        let ws_hash = warm_store.current_workspace_hash();
        if !ws_hash.is_empty() {
            // 优先使用退出时持久化的打开标签页快照，只恢复用户未关闭的标签
            let saved_tabs = state
                .ui.app_settings
                .ui
                .ai_open_tabs
                .get(workspace_hash)
                .cloned();
            if let Some(tabs_snapshot) = saved_tabs {
                // 按快照中的 ID 列表逐个加载对话
                let mut loaded_convs = Vec::new();
                for conv_id in &tabs_snapshot.conversation_ids {
                    if let Ok(conv) = warm_store.load_conversation(conv_id) {
                        loaded_convs.push(conv);
                    }
                }
                if !loaded_convs.is_empty() {
                    let active = tabs_snapshot
                        .active
                        .min(loaded_convs.len().saturating_sub(1));
                    state.ai.ai_panel.conversations = loaded_convs;
                    state.ai.ai_panel.load_slot_into_active(active);
                    state.ai.current_workspace_ai_session = state
                        .ai.ai_panel
                        .conversations
                        .get(active)
                        .map(|c| c.id.clone());
                    return;
                }
                // 快照中的对话均加载失败（可能已被清理），走新建空对话
            }
        }
    }

    // 3. 该工作区无任何历史会话：创建一个全新的空对话
    state.ai.ai_panel.conversations = vec![crate::ai_panel::AiConversation::new(
        crate::ai_panel::gen_conversation_id(),
        "新对话".to_string(),
    )];
    state.ai.ai_panel.load_slot_into_active(0);
    state.ai.current_workspace_ai_session = None;
}

/// 工作区路径 → 短哈希（与 ai_warm_data.rs 中的 fnv1a_hex 一致）
pub(crate) fn workspace_path_hash(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// 工作区信任检查（自由函数，不持有 EditorState 借用）。
///
/// 必须在调用 `open_folder` 之前、且不持有 `RefCell` `borrow_mut` 时调用，
/// 否则模态确认框（MessageBoxW）会泵消息，导致嵌套的 WM_TIMER/WM_PAINT
/// 对同一 RefCell 再次借用而触发重入 panic（消息被 catch_unwind 吞掉）。
///
/// 返回 true 表示路径已受信任（或用户刚刚确认信任），可以继续 open_folder；
/// 返回 false 表示用户拒绝信任，调用方应中止并提示。
pub fn check_workspace_trust(hwnd: HWND, path: &std::path::Path) -> bool {
    if crate::dialogs::trusted_folders::is_trusted(path) {
        return true;
    }
    let msg = format!(
        "是否信任此文件夹中的代码作者？\n\n{}\n\n\
         信任后将允许执行 Git 检测、LSP、插件等可能运行该目录中代码的功能。",
        path.display()
    );
    if !Dialogs::confirm_yes_no(hwnd, "工作区信任", &msg) {
        return false;
    }
    crate::dialogs::trusted_folders::add_trusted(path);
    true
}

impl EditorState {
    /// 检查当前标签页是否可以重用（空文件且未修改）
    pub(super) fn can_reuse_current_tab(&self) -> bool {
        can_reuse_current_tab(self)
    }
    /// 重置当前编辑状态到初始值
    pub(super) fn reset_editor_state(&mut self) {
        reset_editor_state(self)
    }
    /// 在新标签页中打开内容
    pub(super) fn open_in_new_tab(&mut self, tab: Tab) {
        open_in_new_tab(self, tab)
    }
    pub fn load_file(&mut self, path: PathBuf) {
        load_file(self, path)
    }
    /// 加载图片文件
    pub(super) fn load_image_file(&mut self, path: PathBuf) {
        load_image_file(self, path)
    }
    /// 显示不支持的文件提示
    pub(super) fn show_unsupported_file(&mut self, path: &Path) {
        show_unsupported_file(self, path)
    }
    /// P4-2: 原子写入文件，避免写入中途崩溃导致文件损坏
    #[allow(dead_code)]
    pub(super) fn atomic_write(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
        atomic_write(path, data)
    }
    /// 流式原子写入：通过回调函数写入数据，避免在内存中构造完整的 &[u8]。
    pub(super) fn atomic_write_stream<F>(
        path: &std::path::Path,
        writer_fn: F,
    ) -> std::io::Result<()>
    where
        F: FnOnce(&mut std::fs::File) -> std::io::Result<()>,
    {
        atomic_write_stream(path, writer_fn)
    }
    /// 保存文件，返回是否成功
    pub fn save_file(&mut self) -> bool {
        save_file(self)
    }
    /// 另存为
    pub fn save_as(&mut self, path: PathBuf) -> bool {
        save_as(self, path)
    }
    pub fn open_folder(&mut self, path: PathBuf) {
        open_folder(self, path)
    }
    /// H-09: 接收 &ScannedBatch 引用，由调用方负责 Box 的 drop
    pub(crate) fn on_folder_scan_batch_ref(&mut self, batch: &ScannedBatch) {
        on_folder_scan_batch_ref(self, batch)
    }
    /// 在打开的文件夹根目录查找 README 并自动加载
    pub(super) fn try_open_readme(&mut self, folder: &Path) {
        try_open_readme(self, folder)
    }
    pub fn close_workspace(&mut self) {
        close_workspace(self)
    }
    /// 保存当前工作区的 AI 会话状态（完整标签页组快照）
    fn save_current_workspace_ai_session(&mut self) {
        save_current_workspace_ai_session(self)
    }
    /// 加载目标工作区的 AI 会话（恢复标签页组或从数据库加载）
    fn load_workspace_ai_session(&mut self, workspace_hash: &str) {
        load_workspace_ai_session(self, workspace_hash)
    }
    /// 工作区路径 → 短哈希（与 ai_warm_data.rs 中的 fnv1a_hex 一致）
    pub(crate) fn workspace_path_hash(path: &Path) -> String {
        workspace_path_hash(path)
    }
}
