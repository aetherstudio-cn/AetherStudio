use super::*;

use aether_core::buffer::text_buffer::TextBuffer as _;

impl EditorState {
    /// 切换活动视图到指定视图（非 AI 助手）。
    ///
    /// 更新活动栏高亮、`activity_view`、侧边栏可见性与内容。
    /// 供活动栏左键点击与右键上下文菜单共用。
    pub fn switch_activity_view(&mut self, view: ActivityBarView) {
        self.ui.activity_bar.switch_to_view(view);
        self.ui.activity_view = view;
        // 切换活动栏视图时打开侧边栏：恢复上次保存的宽度
        self.ui.layout.show_sidebar();
        self.ui.sidebar_content = SidebarContent::from_view(view);
    }
    /// P2-3: 调整字体大小（Ctrl+= 放大 / Ctrl+- 缩小 / Ctrl+0 重置）。
    /// delta 为正放大、为负缩小；传 None 则重置为 14.0。
    pub fn zoom_font(&mut self, delta: Option<f32>) {
        let current = self.win.text_renderer.font_size();
        let new_size = match delta {
            Some(d) => current + d,
            None => 14.0,
        };
        self.win.text_renderer.set_font_size(new_size);
        // 重建文本格式缓存（与 set_font_size 同步，避免渲染时使用旧格式）
        let fs = self.win.text_renderer.font_size();
        self.win
            .render_ctx
            .text_format_cache
            .init_common_formats(fs);
        self.ui.status_message = format!("字体大小: {:.1} px", fs);
    }
    /// 发射一个编辑器事件到事件队列
    pub fn emit_event(&mut self, event: crate::events::EditorEvent) {
        self.input.event_queue.push(event);
    }
    /// P3.1: 请求内联补全建议（占位实现）
    pub fn request_inline_completion(&mut self) {
        // 收集光标前后文本作为上下文
        let prefix = self
            .editor
            .content
            .buffer
            .get_line(self.editor.content.cursor_line)
            .map(|s| {
                let pos = s.floor_char_boundary(self.editor.content.cursor_col.min(s.len()));
                s[..pos].to_string()
            })
            .unwrap_or_default();
        let suffix = self
            .editor
            .content
            .buffer
            .get_line(self.editor.content.cursor_line)
            .map(|s| {
                let pos = s.floor_char_boundary(self.editor.content.cursor_col.min(s.len()));
                s[pos..].to_string()
            })
            .unwrap_or_default();

        if let Some(suggestion) = self
            .editor
            .inline_completion_service
            .request(&prefix, &suffix)
        {
            self.editor.content.inline_completion =
                Some(crate::inline_completion::InlineCompletion {
                    text: suggestion.text,
                    trigger_line: self.editor.content.cursor_line,
                    trigger_col: self.editor.content.cursor_col,
                    version: suggestion.version,
                });
            self.emit_event(crate::events::EditorEvent::CursorMoved);
        }
    }
    /// P3.1: 清除当前内联补全建议
    /// 清除后标记编辑区脏矩形，确保幽灵文本从屏幕上擦除（否则残留为重影）
    pub fn clear_inline_completion(&mut self) {
        if self.editor.content.inline_completion.take().is_some() {
            self.emit_event(crate::events::EditorEvent::CursorMoved);
        }
    }
    /// P3.3: 接受当前内联补全建议，将建议文本插入到光标处
    pub fn accept_inline_completion(&mut self) -> bool {
        let Some(comp) = self.editor.content.inline_completion.take() else {
            return false;
        };
        if comp.trigger_line != self.editor.content.cursor_line
            || comp.trigger_col != self.editor.content.cursor_col
        {
            // 补全已被 take() 清除但位置不匹配：标记脏区域擦除幽灵文本残留
            self.emit_event(crate::events::EditorEvent::CursorMoved);
            return false;
        }
        let pos = self.cursor_byte_pos();
        self.editor.content.buffer.insert(pos, &comp.text);
        self.editor.content.cursor_col += comp.text.len();
        self.editor.content.is_dirty = true;
        if let Some(tab) = self
            .editor
            .tab_bar
            .tabs
            .get_mut(self.editor.tab_bar.active_tab)
        {
            tab.mark_dirty();
        }
        self.editor.content.buffer_version += 1;
        self.emit_edit_events();
        true
    }
    /// 发射文本编辑相关事件（TextChanged + CursorMoved）
    pub(super) fn emit_edit_events(&mut self) {
        self.emit_event(crate::events::EditorEvent::TextChanged {
            start_line: self.editor.content.cursor_line,
            end_line: self.editor.content.cursor_line + 1,
        });
        self.emit_event(crate::events::EditorEvent::CursorMoved);
        // 自动保存：文本变更后按防抖延迟（重）设空闲保存定时器
        self.schedule_autosave_debounce();
    }
    /// 将事件队列中所有事件转换为脏矩形标记
    pub fn flush_events_to_dirty_tracker(&mut self) {
        // 预取布局区域，避免闭包多次借用 self.ui.layout
        let editor_region = self.ui.layout.editor_region();
        let status_region = self.ui.layout.status_bar_region();
        let sidebar_region = self.ui.layout.sidebar_region();
        let right_panel_region = self.ui.layout.right_panel_region();
        let bottom_region = self.ui.layout.bottom_panel_region();
        let line_height = self.win.text_renderer.line_height();
        // REQ-P1-03: 用字符列（而非字节偏移）计算脏矩形光标 x 坐标，
        // 避免非 ASCII 文本时光标残影/撕裂
        let char_col = self
            .editor
            .content
            .buffer
            .get_line(self.editor.content.cursor_line)
            .map(|line| {
                let pos = line.floor_char_boundary(self.editor.content.cursor_col.min(line.len()));
                line[..pos].chars().count()
            })
            .unwrap_or(0);
        let cursor_x =
            editor_region.x + 60.0 + 5.0 + char_col as f32 * self.win.text_renderer.char_width()
                - self.editor.content.scroll_x;
        let cursor_y = editor_region.y + self.editor.content.cursor_line as f32 * line_height
            - self.editor.content.scroll_y;

        self.input
            .event_queue
            .drain_to_dirty_tracker(&mut self.win.dirty_tracker, |event| {
                use crate::events::EditorEvent;
                match event {
                    EditorEvent::TextChanged { .. } => Some((
                        editor_region.x,
                        editor_region.y,
                        editor_region.width,
                        editor_region.height,
                    )),
                    EditorEvent::CursorMoved => Some((cursor_x, cursor_y, 2.0, line_height)),
                    EditorEvent::SelectionChanged => Some((
                        editor_region.x,
                        editor_region.y,
                        editor_region.width,
                        editor_region.height,
                    )),
                    EditorEvent::Scrolled => Some((
                        editor_region.x,
                        editor_region.y,
                        editor_region.width,
                        editor_region.height,
                    )),
                    EditorEvent::TabChanged => None, // 由 switch_tab 显式标记局部区域
                    EditorEvent::SidebarChanged => {
                        if sidebar_region.width > 0.0 {
                            Some((
                                sidebar_region.x,
                                sidebar_region.y,
                                sidebar_region.width,
                                sidebar_region.height,
                            ))
                        } else {
                            None
                        }
                    }
                    EditorEvent::RightPanelChanged => {
                        if right_panel_region.width > 0.0 {
                            Some((
                                right_panel_region.x,
                                right_panel_region.y,
                                right_panel_region.width,
                                right_panel_region.height,
                            ))
                        } else {
                            None
                        }
                    }
                    EditorEvent::BottomPanelChanged => {
                        if bottom_region.height > 0.0 {
                            Some((
                                bottom_region.x,
                                bottom_region.y,
                                bottom_region.width,
                                bottom_region.height,
                            ))
                        } else {
                            None
                        }
                    }
                    EditorEvent::StatusBarChanged => Some((
                        status_region.x,
                        status_region.y,
                        status_region.width,
                        status_region.height,
                    )),
                    EditorEvent::WindowResized => None, // 全窗口事件在内部处理
                    EditorEvent::FindReplaceChanged => None, // 由调用方显式标记
                    EditorEvent::DialogVisibilityChanged => None, // 全窗口事件在内部处理
                }
            });
    }
    /// P2.3: 根据当前 buffer 大小更新大文件标记
    pub fn update_large_file_flag(&mut self) {
        let line_count = self.editor.content.buffer.len_lines();
        let byte_count = self.editor.content.buffer.len_bytes();
        self.editor.content.is_large_file = line_count > Self::LARGE_FILE_LINE_THRESHOLD
            || byte_count > Self::LARGE_FILE_BYTE_THRESHOLD;
    }
    /// 执行菜单命令
    pub fn execute_command(&mut self, cmd: crate::menu_bar::CommandId, hwnd: HWND) {
        match cmd {
            crate::menu_bar::CommandId::FileNew => {
                self.new_project();
            }
            crate::menu_bar::CommandId::FileNewWindow => {
                // 通过 PostMessage 通知窗口过程创建新窗口
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::WM_APP + 2,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
            crate::menu_bar::CommandId::FileOpen => {
                // 异步处理：通过 PostMessage 避免在 RefCell 借用期间弹模态对话框
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::WM_APP + 10,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
            crate::menu_bar::CommandId::FileOpenFolder => {
                // 异步处理：通过 PostMessage 避免在 RefCell 借用期间弹模态对话框
                // 否则泵消息触发 WM_PAINT 会导致重入 panic（被 catch_unwind 吞掉，表现为无反应）
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::WM_APP + 9,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
            crate::menu_bar::CommandId::FileCloseWorkspace => {
                self.close_workspace();
            }
            crate::menu_bar::CommandId::FileSave => {
                self.save_file();
            }
            crate::menu_bar::CommandId::FileSaveAs => {
                // 异步处理：通过 PostMessage 避免在 RefCell 借用期间弹模态对话框
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::WM_APP + 11,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
            crate::menu_bar::CommandId::FileExit => unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            },
            crate::menu_bar::CommandId::EditUndo => {
                self.undo();
            }
            crate::menu_bar::CommandId::EditRedo => {
                self.redo();
            }
            crate::menu_bar::CommandId::EditCut => {
                self.cut();
            }
            crate::menu_bar::CommandId::EditCopy => {
                self.copy();
            }
            crate::menu_bar::CommandId::EditPaste => {
                self.paste();
            }
            crate::menu_bar::CommandId::EditFind => {
                self.editor.find.toggle_find(&self.editor.content);
            }
            crate::menu_bar::CommandId::EditReplace => {
                self.editor.find.toggle_replace(&self.editor.content);
            }
            crate::menu_bar::CommandId::EditSelectAll => {
                self.select_all();
            }
            crate::menu_bar::CommandId::ViewToggleSidebar => {
                self.ui.layout.toggle_sidebar();
            }
            crate::menu_bar::CommandId::ViewToggleActivityBar => {
                self.ui.layout.toggle_activity_bar();
            }
            crate::menu_bar::CommandId::ViewToggleStatusBar => {
                self.ui.layout.status_bar_visible = !self.ui.layout.status_bar_visible;
            }
            crate::menu_bar::CommandId::ViewZoomIn => {
                self.ui.status_message = "放大功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::ViewZoomOut => {
                self.ui.status_message = "缩小功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::GotoFile => {
                self.ui.status_message = "转到文件功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::GotoLine => {
                self.ui.status_message = "转到行功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::RunStart => {
                self.ui.status_message = "运行功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::RunDebug => {
                self.ui.status_message = "调试功能即将推出".to_string();
            }
            crate::menu_bar::CommandId::RunSandboxEval => {
                self.open_sandbox_eval_tab();
            }
            crate::menu_bar::CommandId::SearchGlobal => {
                self.ui.search_panel.toggle();
                if self.ui.search_panel.visible {
                    self.ui
                        .search_panel
                        .search(self.fs.current_folder.as_deref());
                }
            }
            crate::menu_bar::CommandId::AiFixDiagnostics => {
                self.ai_fix_diagnostics();
            }
            crate::menu_bar::CommandId::TerminalNew => {
                self.ui.layout.toggle_terminal_panel();
                if self.ui.layout.bottom_panel_visible {
                    self.terminal.terminal_panel.focused = true;
                    self.set_terminal_ime_bypass(true);
                    if !self.terminal.terminal_panel.running {
                        let _ = self.terminal.terminal_panel.start();
                    }
                    // 启动周期刷新定时器以显示异步 shell 输出
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                            self.win.hwnd,
                            0xA002,
                            50,
                            None,
                        );
                    }
                } else {
                    self.terminal.terminal_panel.focused = false;
                    self.set_terminal_ime_bypass(false);
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(
                            self.win.hwnd,
                            0xA002,
                        );
                    }
                }
                self.ui.status_message = if self.ui.layout.bottom_panel_visible {
                    "终端已打开"
                } else {
                    "终端已关闭"
                }
                .to_string();
            }
            crate::menu_bar::CommandId::HelpCheckUpdate => {
                self.ui.status_message = "正在检查更新...".to_string();
                crate::updater::start_check(hwnd, true);
            }
            crate::menu_bar::CommandId::HelpAbout => {
                self.ui.status_message = format!("牧羊人编辑器 v{}", crate::updater::APP_VERSION);
            }
            crate::menu_bar::CommandId::MarkdownTogglePreview => {
                self.toggle_markdown_preview();
            }
            crate::menu_bar::CommandId::None => {}
        }
    }
    /// 增量重建缓存：只重建可见行范围内的缓存，大幅减少大文件的词法分析开销
    ///
    /// 视口优先策略：
    /// 1. 优先高亮可见区域（visible_start..visible_end），让用户立即看到内容
    /// 2. 然后高亮视口扩展区域（±padding），为滚动做准备
    /// 3. 后台处理不可见区域（通过 tree-sitter 异步处理）
    pub(crate) fn rebuild_cache(&mut self, visible_start: usize, visible_end: usize) {
        // === 0延迟切换：刚切换过来的标签页首帧跳过所有重建 ===
        // 直接渲染已有缓存，下一帧再恢复正常逻辑
        // 但如果缓存中没有高亮数据（首次打开或缓存被清空），仍然需要高亮
        if self.editor.content.just_switched {
            self.editor.content.just_switched = false;
            // 只更新签名，让后续帧能正确命中
            let total_lines = self.editor.content.buffer.len_lines().max(1);
            self.editor.content.last_cache_signature = (
                self.editor.content.buffer_version,
                visible_start,
                visible_end,
                total_lines,
            );
            // 检查可见区域是否已有高亮缓存
            let has_highlight = (visible_start..visible_end.min(total_lines)).all(|i| {
                i < self.editor.content.cached_tokens.len()
                    && !self.editor.content.cached_tokens[i].is_empty()
            });
            if has_highlight {
                return; // 缓存完整，0延迟渲染
            }
            // 缓存不完整：继续执行下方的高亮逻辑
        }

        let total_lines = self.editor.content.buffer.len_lines().max(1);

        // REQ-P2-01: 变化检测
        let signature = (
            self.editor.content.buffer_version,
            visible_start,
            visible_end,
            total_lines,
        );
        // P0-A: 窗口化后附加校验窗口长度
        let cache_start = visible_start.saturating_sub(2);
        let cache_end = (visible_end + 2).min(total_lines).max(cache_start);
        let window_len = cache_end - cache_start;
        if self.editor.content.last_cache_signature == signature
            && self.editor.content.cache_window_start == cache_start
            && self.editor.content.cached_lines.len() == window_len
        {
            return;
        }
        self.editor.content.last_cache_signature = signature;

        // P2.3: 大文件检测与行偏移缓存
        // 优化：只在必要时更新大文件标记（行数或字节数变化时）
        let line_count = self.editor.content.buffer.len_lines();
        let byte_count = self.editor.content.buffer.len_bytes();
        let new_is_large = line_count > Self::LARGE_FILE_LINE_THRESHOLD
            || byte_count > Self::LARGE_FILE_BYTE_THRESHOLD;
        if self.editor.content.is_large_file != new_is_large {
            self.editor.content.is_large_file = new_is_large;
        }
        self.rebuild_line_y_offsets();

        // P0-A: 平移行文本缓存窗口
        self.editor
            .content
            .slide_cache_window(cache_start, window_len);

        // tokens 仍为全文件索引，行数变化时调整
        if self.editor.content.cached_tokens.len() != total_lines {
            self.editor
                .content
                .cached_tokens
                .resize_with(total_lines, Vec::new);
        }

        // P2.3: 大文件模式下跳过语法高亮
        let mut lexer: Option<Box<dyn aether_core::lexer::Lexer>> = None;

        // === GPU 高亮优先尝试 ===
        // 优化：只在文件内容变化时运行 GPU 词法分析，切换标签页时复用缓存
        let mut gpu_highlighted = false;
        if let Some(ref mut gpu_lexer) = self.win.gpu_highlighter {
            if self.win.gpu_highlight_config.enabled
                && !self.editor.content.is_large_file
                && self.editor.content.buffer.len_bytes()
                    >= self.win.gpu_highlight_config.min_file_size
            {
                let vp_cache = self
                    .editor
                    .content
                    .viewport_highlight_cache
                    .get_or_insert_with(aether_render::gpu::viewport::ViewportHighlightCache::new);

                // 检查是否需要重新运行 GPU 分析：
                // 1. 视口范围变化 2. buffer_version 变化（内容编辑）
                let vp_changed =
                    vp_cache.window_start() != cache_start || vp_cache.window_len() != window_len;
                let content_changed =
                    vp_cache.buffer_version() != self.editor.content.buffer_version;
                let need_gpu_rebuild = vp_changed || content_changed || vp_cache.is_empty();

                if need_gpu_rebuild {
                    vp_cache.resize_window(
                        cache_start,
                        window_len,
                        self.editor.content.buffer_version,
                    );

                    let current_lines: Vec<String> = (cache_start..cache_end)
                        .map(|i| self.editor.content.buffer.get_line(i).unwrap_or_default())
                        .collect();
                    vp_cache.update_with_edit_distance(
                        &current_lines,
                        self.editor.content.buffer_version,
                        self.win.gpu_highlight_config.edit_distance_threshold,
                    );

                    let dirty_lines = vp_cache.dirty_line_indices();

                    if !dirty_lines.is_empty() {
                        let text = self.editor.content.buffer.get_all_text();
                        if let Ok(tokens) = gpu_lexer.lex(text.as_bytes()) {
                            if !tokens.is_empty() {
                                let gpu_spans =
                                    aether_render::gpu::render::gpu_tokens_to_lexeme_spans(
                                        &tokens, None,
                                    );
                                gpu_highlighted = true;

                                for line_idx in cache_start..cache_end {
                                    let line_start = self
                                        .editor
                                        .content
                                        .buffer
                                        .line_byte_range(line_idx)
                                        .map(|(s, _)| s as u32)
                                        .unwrap_or(0);
                                    let line_end = self
                                        .editor
                                        .content
                                        .buffer
                                        .line_byte_range(line_idx)
                                        .map(|(_, e)| e as u32)
                                        .unwrap_or(text.len() as u32);

                                    let line_tokens: Vec<aether_core::lexer::LexemeSpan> =
                                        gpu_spans
                                            .iter()
                                            .filter(|span| {
                                                span.start >= line_start && span.start < line_end
                                            })
                                            .cloned()
                                            .collect();

                                    vp_cache.set_line_tokens(
                                        line_idx,
                                        line_tokens,
                                        self.editor.content.buffer_version,
                                    );
                                }
                            }
                        }
                    } else {
                        gpu_highlighted = true;
                    }
                } else {
                    // 视口和内容均未变化：直接复用缓存
                    gpu_highlighted = true;
                }
            }
        }

        // 将 ViewportHighlightCache 中的 token 同步到 cached_tokens
        if gpu_highlighted {
            if let Some(ref vp_cache) = self.editor.content.viewport_highlight_cache {
                for line_idx in cache_start..cache_end {
                    if let Some(tokens) = vp_cache.get_line_tokens(line_idx) {
                        if line_idx < self.editor.content.cached_tokens.len() {
                            self.editor.content.cached_tokens[line_idx] = tokens.to_vec();
                        }
                    }
                }
            }
        }

        // === 视口优先高亮策略 ===
        // 1. 首先处理可见区域（用户当前看到的内容）
        // 2. 然后处理扩展区域（视口上下 padding）
        // 3. 对于大文件，只处理可见区域，跳过不可见区域

        // 计算可见区域和扩展区域
        let viewport_start = visible_start;
        let viewport_end = visible_end.min(total_lines);
        let extended_start = cache_start;
        let extended_end = cache_end;

        // 第一遍：优先处理可见区域
        for i in viewport_start..viewport_end {
            if i >= cache_start && i < cache_end {
                let slot = i - cache_start;
                if self.editor.content.line_cache_versions[slot]
                    != self.editor.content.buffer_version
                {
                    self.highlight_line(i, slot, gpu_highlighted, &mut lexer);
                }
            }
        }

        // 第二遍：处理扩展区域（不可见但接近视口）
        for i in extended_start..extended_end {
            if i < viewport_start || i >= viewport_end {
                let slot = i - cache_start;
                if self.editor.content.line_cache_versions[slot]
                    != self.editor.content.buffer_version
                {
                    self.highlight_line(i, slot, gpu_highlighted, &mut lexer);
                }
            }
        }
    }

    /// 高亮单行（提取为辅助方法）
    fn highlight_line(
        &mut self,
        line_idx: usize,
        slot: usize,
        gpu_highlighted: bool,
        lexer: &mut Option<Box<dyn aether_core::lexer::Lexer>>,
    ) {
        let line = self
            .editor
            .content
            .buffer
            .get_line(line_idx)
            .unwrap_or_default();

        if self.editor.content.is_large_file {
            self.editor.content.cached_lines[slot] = line;
            self.editor.content.cached_tokens[line_idx] = Vec::new();
            self.editor.content.line_cache_versions[slot] = self.editor.content.buffer_version;
        } else if gpu_highlighted {
            self.editor.content.cached_lines[slot] = line;
            self.editor.content.line_cache_versions[slot] = self.editor.content.buffer_version;
        } else {
            if lexer.is_none() {
                *lexer = Some(self.editor.content.language.create_lexer());
            }
            let tokens = if let Some(lex) = lexer.as_ref() {
                lex.lex_full(&line)
            } else {
                Vec::new()
            };
            self.editor.content.cached_lines[slot] = line;
            self.editor.content.cached_tokens[line_idx] = tokens;
            self.editor.content.line_cache_versions[slot] = self.editor.content.buffer_version;
        }
    }
}
