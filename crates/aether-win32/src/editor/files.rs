use super::*;

impl EditorState {
    /// 检查当前标签页是否可以重用（空文件且未修改）
    pub(super) fn can_reuse_current_tab(&self) -> bool {
        self.content.file_path.is_none()
            && !self.content.is_dirty
            && self.content.buffer.len_bytes() == 0
    }
    /// 重置当前编辑状态到初始值
    pub(super) fn reset_editor_state(&mut self) {
        self.content.cursor_line = 0;
        self.content.cursor_col = 0;
        self.content.scroll_y = 0.0;
        self.content.history.clear();
        self.content.is_dirty = false;
        self.content.buffer_version += 1;
        self.clear_selection();
    }
    /// 在新标签页中打开内容
    pub(super) fn open_in_new_tab(&mut self, tab: Tab) {
        // REQ-P1-09: save current state to old tab, push new tab, swap it in
        self.swap_tab_content(self.tab_bar.active_tab);
        // 直接将新标签页追加到末尾并切换过去。
        // 此前使用 swap(tabs[active], placeholder) + push(placeholder) 的写法，
        // 会让新 tab 留在原 active 位置、旧 tab 被推到末尾，但 active_tab 又被
        // 设置为 len()-1，结果指向了旧 tab，导致打开第二个文件时仍显示旧内容、
        // LSP did_open 也发给了旧文件。改为直接 push 新 tab 即可。
        self.tab_bar.tabs.push(tab);
        self.tab_bar.active_tab = self.tab_bar.tabs.len() - 1;
        self.swap_tab_content(self.tab_bar.active_tab);
        self.is_selecting = false;
        self.emit_event(crate::events::EditorEvent::TabChanged);
        // 标记标签栏和编辑器区域脏区，避免新标签打开时触发全窗口重绘
        let editor_region = self.layout.editor_region();
        let tab_region = self.layout.tab_bar_region(self.show_tab_bar());
        self.dirty_tracker.mark_region(
            editor_region.x,
            editor_region.y,
            editor_region.width,
            editor_region.height,
            crate::dirty_rect::DirtyRegionType::EditorContent,
        );
        self.dirty_tracker.mark_region(
            tab_region.x,
            tab_region.y,
            tab_region.width,
            tab_region.height,
            crate::dirty_rect::DirtyRegionType::TabBar,
        );
    }
    pub fn load_file(&mut self, path: PathBuf) {
        let lang = Language::from_path(&path);

        if lang == Language::Image {
            self.load_image_file(path);
            return;
        }

        if !is_text_file(&path) {
            self.show_unsupported_file(&path);
            return;
        }

        match PieceTable::from_file(&path) {
            Ok(buffer) => {
                if self.can_reuse_current_tab() {
                    self.content.buffer = buffer;
                    self.content.file_path = Some(path.clone());
                    self.content.language = lang;
                    self.reset_editor_state();
                    // REQ-P1-09: self.content 即活动标签页状态，无需再手动同步到 Tab
                    self.status_message = format!("已打开: {}", path.display());
                } else {
                    let tab = Tab::File(TabContent::with_loaded_buffer(
                        Some(path.clone()),
                        buffer,
                        lang,
                        false,
                    ));
                    self.open_in_new_tab(tab);
                    self.status_message = format!("已打开: {}", path.display());
                }
                self.emit_event(crate::events::EditorEvent::TextChanged {
                    start_line: 0,
                    end_line: self.content.buffer.len_lines(),
                });
                self.emit_event(crate::events::EditorEvent::StatusBarChanged);
                // 接线 LSP：通知服务器文档已打开（按需启动 server），激活补全/悬停/诊断
                // 冰冻态：先解冻重启语言服务器（解冻路径内已 notify_open 当前文档）
                if self.lsp.frozen {
                    self.thaw_lsp_on_demand();
                } else {
                    self.lsp.notify_open(&self.content);
                }

                // 启动高亮刷新定时器：tree-sitter 语言且非大文件时，后台高亮在工作线程
                // 完成后需要一次重绘才能着色。此定时器周期性重绘直至高亮到达，随后自动停止，
                // 避免文件打开后停留在无高亮纯文本、要等到鼠标移动/光标闪烁才着色的卡顿感。
                self.update_large_file_flag();
                if language_to_ts_str(self.content.language).is_some()
                    && !self.content.is_large_file
                {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                            self.hwnd,
                            crate::window::HIGHLIGHT_TIMER_ID,
                            crate::window::HIGHLIGHT_REFRESH_MS,
                            None,
                        );
                    }
                }
            }
            Err(e) => {
                let msg = format!("打开文件失败: {}", e);
                self.status_message = msg.clone();
                Dialogs::show_error(self.hwnd, "打开文件", &msg);
            }
        }

        // 文件加载成功后通知 LSP 服务器。
        // 注：lsp_notify_open() 已在上面调用过（会按需启动 server 并 send did_open），
        // 此处无需重复 get_text + lsp_open_document，避免对 UI 线程造成双倍的
        // 全文件拷贝（get_text 是 O(N) String 分配，对大文件耗时明显）。
    }
    /// 加载图片文件
    pub(super) fn load_image_file(&mut self, path: PathBuf) {
        let content = format!("[图片预览] {}", path.display());
        if self.can_reuse_current_tab() {
            self.content.file_path = Some(path.clone());
            self.content.language = Language::Image;
            self.content.buffer = PieceTable::from_string(content);
            self.reset_editor_state();
            self.status_message = format!("已打开图片: {}", path.display());
        } else {
            let tab = Tab::File(TabContent::with_loaded_buffer(
                Some(path.clone()),
                PieceTable::from_string(content),
                Language::Image,
                false,
            ));
            self.open_in_new_tab(tab);
            self.status_message = format!("已打开图片: {}", path.display());
        }
    }
    /// 显示不支持的文件提示
    pub(super) fn show_unsupported_file(&mut self, path: &Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");
        let message = format!("不支持的文件格式: .{}\n文件: {}", ext, path.display());
        if self.can_reuse_current_tab() {
            self.content.file_path = Some(path.to_path_buf());
            self.content.language = Language::PlainText;
            self.content.buffer = PieceTable::from_string(message);
            self.reset_editor_state();
            self.status_message = format!("不支持的文件格式: .{}", ext);
        } else {
            let tab = Tab::File(TabContent::with_loaded_buffer(
                Some(path.to_path_buf()),
                PieceTable::from_string(message),
                Language::PlainText,
                false,
            ));
            self.open_in_new_tab(tab);
            self.status_message = format!("不支持的文件格式: .{}", ext);
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
    pub fn save_file(&mut self) -> bool {
        if let Some(path) = &self.content.file_path.clone() {
            // 处理远程文件保存
            if let Some(remote_path) = path.to_str().and_then(|s| s.strip_prefix("remote:")) {
                // 远程路径仍需 &[u8]，这里不得不做一次全量拷贝
                let mut buf: Vec<u8> = Vec::with_capacity(self.content.buffer.len_bytes());
                if let Err(e) = self.content.buffer.write_to(&mut buf) {
                    self.status_message = format!("保存失败: {}", e);
                    return false;
                }
                if let Some(session) = &self.remote.session {
                    match session.write_remote_file(remote_path, &buf) {
                        Ok(()) => {
                            self.content.is_dirty = false;
                            self.status_message = format!("已保存到远程: {}", remote_path);
                            // 同步自动保存状态（去重基线 / 冲突复位 / 停止防抖）
                            self.note_save_succeeded();
                            return true;
                        }
                        Err(e) => {
                            self.status_message = format!("保存远程文件失败: {}", e);
                            return false;
                        }
                    }
                } else {
                    self.status_message = "远程会话未连接".to_string();
                    return false;
                }
            }
            // 本地文件保存：直接将 buffer 流式写入临时文件，避免 get_all_text 的
            // 全量 String 分配和 UTF-8 lossy 转换。对未编辑的 mmap 大文件尤其显著。
            // P4-2: 仍保持原子写入语义（临时文件 + fsync + rename）。
            match Self::atomic_write_stream(path, |w| self.content.buffer.write_to(w)) {
                Ok(()) => {
                    self.content.is_dirty = false;
                    self.status_message = "已保存".to_string();
                    // 同步自动保存状态（去重基线 / 冲突复位 / mtime 刷新 / 停止防抖）
                    self.note_save_succeeded();
                    true
                }
                Err(e) => {
                    self.status_message = format!("保存失败: {}", e);
                    false
                }
            }
        } else {
            self.status_message = "没有文件路径，请使用另存为".to_string();
            false
        }
    }
    /// 另存为
    pub fn save_as(&mut self, path: PathBuf) -> bool {
        match Self::atomic_write_stream(&path, |w| self.content.buffer.write_to(w)) {
            Ok(()) => {
                self.content.file_path = Some(path.clone());
                self.content.is_dirty = false;
                self.status_message = format!("已保存: {}", path.display());
                // 同步自动保存状态（去重基线 / 冲突复位 / mtime 刷新 / 停止防抖）
                self.note_save_succeeded();
                true
            }
            Err(e) => {
                self.status_message = format!("保存失败: {}", e);
                false
            }
        }
    }
    /// 确保文件夹已被信任：未信任则弹出模态确认框。
    /// 返回 true 表示已信任（或用户本次选择信任），false 表示用户拒绝。
    ///
    /// 关键：本函数**不接收 `&self`**，也不触碰任何 `RefCell<EditorState>`。
    /// 因为内部 `confirm_yes_no` 会启动模态消息循环，期间 WM_PAINT 会让渲染再次
    /// `state.borrow()`。调用方必须在**未持有** state 借用时调用本函数，
    /// 否则会触发 RefCell 双重借用 panic（panic=abort 下表现为静默崩溃）。
    pub fn ensure_folder_trusted(hwnd: HWND, path: &std::path::Path) -> bool {
        if crate::dialogs::trusted_folders::is_trusted(path) {
            return true;
        }
        let title = "工作区信任";
        let msg = format!(
            "是否信任此文件夹中的代码作者？\n\n{}\n\n\
             信任后将允许执行 Git 检测、LSP、插件等可能运行该目录中代码的功能。",
            path.display()
        );
        if !Dialogs::confirm_yes_no(hwnd, title, &msg) {
            return false;
        }
        crate::dialogs::trusted_folders::add_trusted(path);
        true
    }
    pub fn open_folder(&mut self, path: PathBuf) {
        // 异步扫描：先快速同步验证路径可读，再启动后台线程扫描根层
        // 同步预检避免无效路径白白启动线程
        if let Err(e) = std::fs::read_dir(&path) {
            let msg = format!("打开文件夹失败: {}", e);
            self.status_message = msg.clone();
            Dialogs::show_error(self.hwnd, "打开文件夹", &msg);
            return;
        }

        // 工作区信任检查：未信任目录先弹窗询问。
        // 注意：ensure_folder_trusted 内部可能弹出模态框，仅当调用方未持有 state 借用时安全。
        // 交互式入口（AI 面板浏览按钮等）应在获取借用前先调用 ensure_folder_trusted；
        // 此处保留作为其它调用路径（启动恢复/克隆/刷新）的信任兜底。
        if !Self::ensure_folder_trusted(self.hwnd, &path) {
            self.status_message = "已取消打开不受信任的工作区".to_string();
            return;
        }

        // 设置 loading 状态，立即重绘显示 spinner
        self.is_loading_folder = true;
        self.folder_generation = self.folder_generation.wrapping_add(1);
        let generation = self.folder_generation;
        self.current_folder = Some(path.clone());
        // 工作区哈希绑定：后续对话归档将关联该工作区（VS Code workspaceStorage 同款）
        if let Some(warm) = self.ai_panel.warm_data_store.as_ref() {
            warm.set_workspace(&path);
        }
        // 同步终端工作目录到新工作区
        self.terminal_panel.cwd = path.to_string_lossy().to_string();
        // 立即持久化 last_workspace（读盘改写，避免其它窗口的陈旧副本覆写）
        self.app_settings.ui.last_workspace = self.current_folder.clone();
        if let Err(e) = aether_shared::settings::AppSettings::persist_last_workspace(Some(&path)) {
            eprintln!("警告: 保存 last_workspace 失败: {}", e);
        }
        self.status_message = format!("正在扫描: {}...", path.display());
        self.recent_projects.add(&path);
        self.file_tree = Some(FileTree::new());
        // UI-T01: 工作区切换后标题栏需要立即更新，标记全窗口重绘
        self.dirty_tracker.mark_full_window();

        // 初始化 LSP 客户端（启动 rust-analyzer 等语言服务器）
        // 先清空旧工作区的诊断表/补全结果：诊断按 Url 存储，
        // 不清理会随切换过的工作区只增不减地驻留内存
        self.lsp.diagnostics.clear();
        self.lsp.completion_items.clear();
        self.lsp.completion_visible = false;
        self.lsp.init(&path);

        let hwnd = self.hwnd;
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
    pub(crate) fn on_folder_scan_batch_ref(&mut self, batch: &ScannedBatch) {
        if batch.generation != self.folder_generation {
            return;
        }
        if batch.complete {
            self.is_loading_folder = false;
            if let Some(folder) = self.current_folder.clone() {
                self.git.detect(&folder);
                if let Some(branch) = self.git.current_branch_name() {
                    self.status_bar.update_git_branch(Some(&branch));
                } else {
                    self.status_bar.update_git_branch(None);
                }
                self.status_message = format!("已打开文件夹: {}", folder.display());
                self.welcome_focus_action = None;
                // 自动打开 README（若存在）
                self.try_open_readme(&folder);
            }
            return;
        }
        if let Some(ref mut tree) = self.file_tree {
            for entry in &batch.entries {
                tree.add_node(&entry.name, entry.kind, u32::MAX, entry.depth);
            }
        }
    }
    /// 在打开的文件夹根目录查找 README 并自动加载
    /// P2-7: 仅在当前标签页为空且未修改时才自动加载，避免覆盖用户已有内容
    pub(super) fn try_open_readme(&mut self, folder: &Path) {
        // 当前标签页有内容或未保存的修改时，不自动加载 README
        if self.content.is_dirty
            || self.content.buffer.len_bytes() > 0
            || self.content.file_path.is_some()
        {
            return;
        }
        let candidates = ["README.md", "README.MD", "README", "readme.md", "Readme.md"];
        for name in candidates {
            let readme_path = folder.join(name);
            if readme_path.is_file() {
                self.load_file(readme_path);
                return;
            }
        }
    }
    pub fn close_workspace(&mut self) {
        self.file_tree = None;
        self.current_folder = None;
        // 同步清空持久化的 last_workspace，避免下次启动重新打开已被用户主动关闭的工作区
        self.app_settings.ui.last_workspace = None;
        if let Err(e) = aether_shared::settings::AppSettings::persist_last_workspace(None) {
            eprintln!("警告: 清除 last_workspace 失败: {}", e);
        }
        self.content.file_path = None;
        self.content.buffer = PieceTable::from_string(String::new());
        self.content.cursor_line = 0;
        self.content.cursor_col = 0;
        self.content.scroll_y = 0.0;
        self.content.selection_start = None;
        self.content.selection_end = None;
        self.content.is_dirty = false;
        self.content.cached_lines.clear();
        self.content.line_cache_versions.clear();
        self.content.cache_window_start = 0;
        self.content.cached_tokens.clear();
        self.content.language = Language::PlainText;
        self.tab_bar.tabs.clear();
        self.tab_bar.tabs.push(crate::tabs::Tab::new());
        self.tab_bar.active_tab = 0;
        self.selected_file_node = None;
        self.welcome_focus_action = None;
        // 释放旧工作区的 LSP 诊断与补全缓存（否则随 Url key 永久驻留）
        self.lsp.diagnostics.clear();
        self.lsp.completion_items.clear();
        self.lsp.completion_visible = false;
        self.git.detect(std::path::Path::new("."));
        self.status_bar.update_git_branch(None);
        self.status_message = "已关闭工作区".to_string();
        // UI-T01: 关闭工作区后标题栏需要立即恢复为应用名
        self.dirty_tracker.mark_full_window();
    }
}
