use super::*;

impl EditorState {
    /// 复制最后一条 AI 回复到剪贴板
    pub fn copy_ai_last_response(&mut self) {
        if let Some(t) = self.ai.ai_panel.last_assistant_text() {
            if Self::set_clipboard_text(&t) {
                self.ui.status_message = "已复制 AI 回复".to_string();
            }
        }
    }
    /// 保存 AI 代码块为文件
    /// 如果 filename 为空，则尝试从代码块内容推断或使用默认名称
    pub fn save_ai_code_block(
        &mut self,
        code: &str,
        suggested_filename: Option<&str>,
    ) -> std::result::Result<PathBuf, String> {
        let root = self
            .fs
            .current_folder
            .clone()
            .ok_or_else(|| "请先打开一个工作区文件夹".to_string())?;

        // 确定文件名
        let filename = if let Some(name) = suggested_filename {
            name.to_string()
        } else {
            // 尝试从代码内容推断语言并生成默认文件名
            let ext = if code.contains("fn ") || code.contains("use ") || code.contains("impl ") {
                "rs"
            } else if code.contains("def ") || code.contains("import ") {
                "py"
            } else if code.contains("function ") || code.contains("const ") || code.contains("let ")
            {
                "js"
            } else if code.contains("package ") || code.contains("import java.") {
                "java"
            } else if code.contains("#include") || code.contains("int main") {
                "c"
            } else if code.contains("<?php") {
                "php"
            } else if code.contains("<html") || code.contains("<!DOCTYPE") {
                "html"
            } else if code.contains("body {") || code.contains("@media") {
                "css"
            } else {
                "txt"
            };
            format!("ai_generated.{}", ext)
        };

        let full_path = root.join(&filename);

        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }

        // 写入文件
        std::fs::write(&full_path, code).map_err(|e| format!("写入文件失败: {}", e))?;

        // 打开新创建的文件
        self.load_file(full_path.clone());

        self.ui.status_message = format!("已保存文件: {}", filename);
        Ok(full_path)
    }
    /// AI Agent：处理最后一条助手消息中的动作标记（生成完成时调用一次）。
    ///
    /// - `AETHER_FILE 路径` 块：创建/修改/删除文件（自动建目录）。
    /// - `AETHER_RUN` 块：在集成终端执行命令。
    ///
    /// 执行结果以助手消息形式反馈到 AI 面板，并刷新文件树。
    pub fn process_ai_agent_actions(&mut self) {
        let active = self.ai.ai_panel.active;
        self.process_ai_agent_actions_for(active);
    }

    /// 处理指定会话（conv_idx）刚完成生成时的 Agent 动作：创建/修改文件、执行终端命令。
    /// 支持后台并发会话——反馈写回对应会话，而非总是活动会话。
    pub fn process_ai_agent_actions_for(&mut self, conv_idx: usize) {
        let Some(text) = self.ai.ai_panel.last_assistant_text_of(conv_idx) else {
            return;
        };

        // CoT 编排分流（仅活动会话）：
        // - 流水线运行中：worker 完成 → 落盘并推进下一任务；
        // - Agent 模式且无流水线：每次完成都尝试识别任务清单（含 READ/LIST 探查回喂后的轮次），
        //   识别到 AETHER_PLAN 即进入流水线，否则回退到普通处理（探查/单文件/聊天）。
        if conv_idx == self.ai.ai_panel.active {
            if self.ai.ai_panel.agent_pipeline.is_some() {
                self.advance_agent_pipeline(conv_idx);
                return;
            }
            if matches!(self.ai.ai_panel.mode, crate::ai_panel::AiMode::Agent) {
                if let Some((goal, tasks)) = crate::ai_panel::parse_plan(&text) {
                    self.start_agent_pipeline(goal, tasks);
                    return;
                }
            }
        }

        // 文件/终端操作必须在已打开的工作区内进行；未打开文件夹时提示用户。
        let has_actions = crate::ai_panel::has_agent_markers(&text);
        if has_actions && self.fs.current_folder.is_none() {
            self.ai.ai_panel
                .add_assistant_message_to(conv_idx, "提示：尚未打开工作区文件夹，无法直接创建/修改文件。请先通过“文件 → 打开文件夹”打开一个项目再试。".to_string());
            self.win.dirty_tracker.mark_full_window();
            return;
        }

        // 1. 文件操作（创建/修改/删除）
        let edits = crate::ai_panel::parse_edits(&text, None);
        let mut file_summary: Vec<String> = Vec::new();
        if !edits.is_empty() {
            match self.apply_ai_workspace_edits(&edits) {
                Ok(paths) => {
                    for p in &paths {
                        let name = self
                            .fs
                            .current_folder
                            .as_ref()
                            .and_then(|root| p.strip_prefix(root).ok())
                            .unwrap_or(p.as_path());
                        file_summary.push(format!("✓ 已写入 `{}`", name.display()));
                    }
                }
                Err(e) => {
                    file_summary.push(format!("✕ 文件操作失败: {}", e));
                }
            }
            // 刷新文件树以显示新文件（轻量刷新，保留展开状态，不重启 LSP）
            if self.fs.current_folder.is_some() {
                self.refresh_file_tree_light();
            }
        }

        // 1.5 精准编辑（增量修改）
        let precise_edits = crate::ai_panel::parse_precise_edits(&text);
        if !precise_edits.is_empty() {
            match self.apply_precise_edits(&precise_edits) {
                Ok(paths) => {
                    for p in &paths {
                        let name = self
                            .fs
                            .current_folder
                            .as_ref()
                            .and_then(|root| p.strip_prefix(root).ok())
                            .unwrap_or(p.as_path());
                        file_summary.push(format!("✓ 已精准修改 `{}`", name.display()));
                    }
                }
                Err(e) => {
                    file_summary.push(format!("✕ 精准编辑失败: {}", e));
                }
            }
            // 刷新文件树以显示修改（轻量刷新，保留展开状态，不重启 LSP）
            if self.fs.current_folder.is_some() {
                self.refresh_file_tree_light();
            }
        }

        // 2. 终端命令
        let commands = crate::ai_panel::parse_run_commands(&text);
        let mut cmd_summary: Vec<String> = Vec::new();
        if !commands.is_empty() {
            // 打开底部面板并切换到终端
            self.ui.layout.bottom_panel_visible = true;
            self.terminal.bottom_panel_tab = crate::editor::BottomPanelTab::Terminal;
            // 同步终端工作目录到当前工作区
            if let Some(folder) = self.fs.current_folder.clone() {
                self.terminal.terminal_panel.cwd = folder.to_string_lossy().to_string();
            }
            // 启动终端（若未运行）并排队命令
            if !self.terminal.terminal_panel.running {
                let _ = self.terminal.terminal_panel.start();
            }
            for cmd in &commands {
                self.terminal.terminal_panel.queue_command(cmd.clone());
                cmd_summary.push(format!("▶ 已执行 `{}`", cmd));
            }
            // 命令可能创建/删除文件：开启一段监视窗口，检测到工作区根目录变化即自动
            // 轻量刷新资源管理器，无需用户手动刷新。
            if self.fs.current_folder.is_some() {
                self.fs.fs_last_root_sig = self.workspace_root_signature();
                self.fs.fs_watch_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
            }
            // 启动终端刷新定时器，保证轮询启动结果并刷新命令队列
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                    self.win.hwnd,
                    crate::window::TERM_TIMER_ID,
                    crate::window::TERM_REFRESH_MS,
                    None,
                );
            }
        }

        // 3. 只读探查工具（读取文件 / 列出目录）：同步执行，结果回喂给模型
        let tool_reqs = crate::ai_panel::parse_tool_requests(&text);
        let mut tool_display: Vec<String> = Vec::new();
        let mut tool_feedback = String::new();
        if !tool_reqs.is_empty() {
            let (display, feedback) = self.execute_tool_requests(&tool_reqs);
            tool_display = display;
            tool_feedback = feedback;
        }

        // 4. 反馈汇总到对应会话
        if !file_summary.is_empty() || !cmd_summary.is_empty() || !tool_display.is_empty() {
            // 先计算总结信息（在移动前）
            let file_count = file_summary.len();
            let cmd_count = cmd_summary.len();
            let tool_count = tool_display.len();

            let mut lines = Vec::new();
            lines.extend(file_summary);
            lines.extend(cmd_summary);
            lines.extend(tool_display);

            // 添加操作总结
            let mut summary_lines = Vec::new();
            summary_lines.push("".to_string());
            summary_lines.push("[总结] **操作总结**".to_string());
            summary_lines.push("".to_string());

            if file_count > 0 {
                summary_lines.push(format!("[文件] 文件操作：{} 个", file_count));
            }
            if cmd_count > 0 {
                summary_lines.push(format!("[命令] 命令执行：{} 个", cmd_count));
            }
            if tool_count > 0 {
                summary_lines.push(format!("[工具] 工具调用：{} 个", tool_count));
            }

            lines.extend(summary_lines);

            self.ai
                .ai_panel
                .add_assistant_message_to(conv_idx, lines.join("\n"));
            self.win.dirty_tracker.mark_full_window();
        }

        // 5. 只读探查结果驱动续跑：仅活动会话，且本轮没有 RUN 命令
        //    （RUN 有自身的异步续跑路径，避免重复触发并发请求；受最大轮次限制）。
        if conv_idx == self.ai.ai_panel.active && commands.is_empty() && !tool_feedback.is_empty() {
            let settings = self.ui.app_settings.ai.clone();
            let mode = self.ai.ai_panel.mode;
            if let Err(e) =
                self.ai
                    .ai_panel
                    .continue_agent_with_tool_result(&settings, tool_feedback, mode)
            {
                self.ai
                    .ai_panel
                    .add_assistant_message_to(conv_idx, format!("（{}，如需继续请手动发消息）", e));
            }
        }
    }

    /// 生成中断（网络断开等）时的文件块抢救：
    ///
    /// - 应用该会话中已**完整接收**的 `AETHER_FILE ... AETHER_END_FILE` 块；
    /// - 抢救未闭合的尾部「新建文件」块（部分内容落盘并明确提示可能不完整）；
    /// - 修改/删除类的截断块不抢救（避免破坏现有文件）；
    /// - 不执行 RUN 命令（内容不完整时执行命令风险过高）。
    pub fn salvage_ai_partial_edits(&mut self, conv_idx: usize) {
        // 跳过末尾的错误提示消息，定位携带文件块的内容消息
        let Some(text) = self
            .ai
            .ai_panel
            .last_assistant_text_matching_of(conv_idx, |t| {
                t.contains(crate::ai_panel::FILE_HEADER_PREFIX)
            })
        else {
            return;
        };
        if self.fs.current_folder.is_none() {
            self.ai.ai_panel.add_assistant_message_to(
                conv_idx,
                "[警告] 生成中断：检测到未保存的文件块，但尚未打开工作区文件夹，无法写入。"
                    .to_string(),
            );
            self.win.dirty_tracker.mark_full_window();
            return;
        }

        let mut edits = crate::ai_panel::parse_edits(&text, None);
        // 尾部未闭合的新建文件块（如生成到一半的网页）：部分内容也值得落盘
        let mut partial_name: Option<String> = None;
        if let Some(trailing) = crate::ai_panel::parse_trailing_create_block(&text) {
            if !edits.iter().any(|e| e.path == trailing.path) {
                partial_name = Some(trailing.path.to_string_lossy().to_string());
                edits.push(trailing);
            }
        }
        if edits.is_empty() {
            return;
        }

        let mut lines = vec!["[警告] 生成中断，已抢救接收到的文件块：".to_string()];
        match self.apply_ai_workspace_edits(&edits) {
            Ok(paths) => {
                for p in &paths {
                    let name = self
                        .fs
                        .current_folder
                        .as_ref()
                        .and_then(|root| p.strip_prefix(root).ok())
                        .unwrap_or(p.as_path());
                    lines.push(format!("✓ 已写入 `{}`", name.display()));
                }
            }
            Err(e) => {
                lines.push(format!("✕ 文件操作失败: {}", e));
            }
        }
        if let Some(name) = partial_name {
            lines.push(format!(
                "[警告] `{}` 为生成中断时的部分内容，可能不完整，请检查后再使用。",
                name
            ));
        }
        // 刷新文件树以显示新文件
        if self.fs.current_folder.is_some() {
            self.refresh_file_tree_light();
        }
        self.ai
            .ai_panel
            .add_assistant_message_to(conv_idx, lines.join("\n"));
        self.win.dirty_tracker.mark_full_window();
    }
    /// AI Agent：终端命令执行完成后的结果处理（主线程每帧轮询驱动）。
    ///
    /// 1. 把命令输出以助手消息展示到对应会话；
    /// 2. 活动会话：把输出作为上下文再次发起请求，继续 Agent 推理循环
    ///    （受 ai_panel.agent_iter_count 最大轮次限制）。
    pub fn handle_agent_command_results(
        &mut self,
        results: Vec<crate::terminal::AgentCommandResult>,
    ) {
        for result in results {
            // 展示用输出（截断，避免刷屏）
            let display_output = truncate_chars(&result.output, 2000);
            let display_output = if display_output.is_empty() {
                "（无输出）".to_string()
            } else {
                display_output
            };
            self.ai.ai_panel.add_assistant_message_to(
                result.conv_idx,
                format!(
                    "✓ `{}` 执行完成，输出：\n```\n{}\n```",
                    result.command, display_output
                ),
            );

            // 回喂续跑：仅活动会话，避免后台会话并发请求失控
            if result.conv_idx == self.ai.ai_panel.active {
                let feedback_output = truncate_chars(&result.output, 4000);
                let feedback = format!(
                    "[终端命令执行结果]\n命令: {}\n输出:\n{}",
                    result.command,
                    if feedback_output.is_empty() {
                        "（无输出）"
                    } else {
                        &feedback_output
                    }
                );
                let settings = self.ui.app_settings.ai.clone();
                let mode = self.ai.ai_panel.mode;
                if let Err(e) = self
                    .ai
                    .ai_panel
                    .continue_agent_with_tool_result(&settings, feedback, mode)
                {
                    self.ai
                        .ai_panel
                        .add_assistant_message(format!("（{}，如需继续请手动发消息）", e));
                }
            }
        }
        self.win.dirty_tracker.mark_full_window();
    }

    /// 刷新 AI 历史索引。
    /// 从 AetherDB（温数据层）加载元数据到内存 history；可按工作区过滤。
    pub fn refresh_ai_history(&mut self) {
        if let Some(store) = self.ai.ai_panel.warm_data_store.as_ref() {
            let ws_only = self.ai.ai_panel.history_workspace_only;
            if let Ok(convs) = store.search_conversations("", ws_only, 500) {
                if !convs.is_empty() {
                    self.ai.ai_panel.history = convs
                        .into_iter()
                        .map(|c| crate::ai_panel::ConversationMeta {
                            id: c.id,
                            title: c.title,
                            updated_at: c.updated_at,
                            message_count: c.message_count as usize,
                            preview: String::new(),
                            mode: c.mode,
                        })
                        .collect();
                } else {
                    self.ai.ai_panel.history.clear();
                }
            }
        }
        self.ai.ai_panel.clamp_history_page();
    }

    /// 打开历史记录中的某条会话详情（懒加载完整消息；由详情视图再恢复会话）。
    pub fn open_ai_history_item(&mut self, idx: usize) {
        self.ai.ai_panel.open_history_detail(idx);
    }

    /// 把当前文件的 LSP 诊断发送给 AI 修复
    pub fn ai_fix_diagnostics(&mut self) {
        let settings = self.ui.app_settings.active_ai_settings();
        let context = self.gather_context(&[
            crate::ai_panel::AiContextAttachment::CurrentFile,
            crate::ai_panel::AiContextAttachment::Diagnostics,
        ]);
        let _ = self.ai.ai_panel.send_message_with_prepared_context(
            &settings,
            context,
            crate::ai_panel::AiMode::Agent,
        );
    }
    /// 将设置面板中的 AI 配置应用到 app_settings 并持久化到磁盘
    ///
    /// API 密钥通过 DPAPI 加密单独存储（见 AppSettings::save），不会明文写入 settings.json。
    /// 同时刷新 AI 面板使用的运行时设置。
    pub fn save_ai_settings(&mut self) {
        // 写回激活模型 + 同步模型列表到持久化设置
        self.ui.settings_panel.store_fields_to_active_model();
        self.ui
            .settings_panel
            .sync_to_app_settings(&mut self.ui.app_settings);
        // 兼容：同时更新旧的单一 ai 字段（作为无模型时的回退）
        self.ui.app_settings.ai = self.ui.settings_panel.to_ai_settings();
        match self.ui.app_settings.save() {
            Ok(_) => {
                self.ui.settings_panel.mark_saved();
                self.ui.settings_panel.test_status = "✓ 设置已保存".to_string();
                self.ui.status_message = "AI 设置已保存".to_string();
            }
            Err(e) => {
                self.ui.settings_panel.test_status = format!("✗ 保存失败：{}", e);
            }
        }
    }
    /// 持久化模型列表变更（删除/启用切换/设为激活/新建后调用）
    pub fn persist_models(&mut self) {
        self.ui
            .settings_panel
            .sync_to_app_settings(&mut self.ui.app_settings);
        if let Err(e) = self.ui.app_settings.save() {
            self.ui.settings_panel.test_status = format!("✗ 保存失败：{}", e);
        }
    }
    /// 保存 AI 设置前，先启动测试连接验证密钥有效性。
    /// 测试成功后会自动调用 save_ai_settings 完成保存。
    pub fn save_ai_settings_with_test(&mut self) {
        let ai = self.ui.settings_panel.to_ai_settings();
        if ai.api_key.trim().is_empty() {
            self.ui.settings_panel.test_status = "✗ 请先填写 API 密钥".to_string();
            return;
        }
        self.ui.settings_panel.pending_save = true;
        self.ui.settings_panel.start_test_connection(ai);
    }
    /// 使用设置面板当前配置启动 AI 测试连接（后台线程，不阻塞 UI）
    pub fn start_ai_test_connection(&mut self) {
        let ai = self.ui.settings_panel.to_ai_settings();
        self.ui.settings_panel.start_test_connection(ai);
    }
    /// 根据附件列表收集 AI 上下文
    pub fn gather_context(&self, attachments: &[AiContextAttachment]) -> String {
        let mut parts = Vec::new();
        let current_path = self
            .editor
            .content
            .file_path
            .as_deref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "当前文件".to_string());
        let current_lang = language_str(self.editor.content.language);

        for attachment in attachments {
            match attachment {
                AiContextAttachment::CurrentFile => {
                    let text = self
                        .editor
                        .content
                        .buffer
                        .get_text(0, self.editor.content.buffer.len_bytes());
                    parts.push(wrap_code_block(
                        &current_path,
                        current_lang,
                        &truncate_middle(&text, 30_000),
                    ));
                }
                AiContextAttachment::Selection => {
                    if let Some(text) = self.selected_text() {
                        parts.push(wrap_code_block(
                            &format!("{} (选区)", current_path),
                            current_lang,
                            &truncate_middle(&text, 10_000),
                        ));
                    }
                }
                AiContextAttachment::OpenFiles => {
                    let mut summary = String::from("打开的文件列表：\n");
                    // 活动标签页的内容存于 self.editor.content（swap 后），需提前提取避免借用冲突
                    let active_idx = self.editor.tab_bar.active_tab;
                    let active_path = self
                        .editor
                        .content
                        .file_path
                        .as_deref()
                        .map(|p| p.to_string_lossy().to_string());
                    let active_lang = language_str(self.editor.content.language);
                    let active_text = self
                        .editor
                        .content
                        .buffer
                        .get_text(0, self.editor.content.buffer.len_bytes());
                    for (i, tab) in self.editor.tab_bar.tabs.iter().enumerate() {
                        let (path, lang, text) = if i == active_idx {
                            (
                                active_path
                                    .clone()
                                    .unwrap_or_else(|| format!("未命名-{}", i + 1)),
                                active_lang,
                                active_text.clone(),
                            )
                        } else if let Some(content) = tab.as_file() {
                            let path = content
                                .file_path
                                .as_deref()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("未命名-{}", i + 1));
                            let lang = language_str(content.language);
                            let text = content.buffer.get_text(0, content.buffer.len_bytes());
                            (path, lang, text)
                        } else {
                            continue;
                        };
                        summary.push_str(&wrap_code_block(
                            &path,
                            lang,
                            &truncate_middle(&text, 5_000),
                        ));
                    }
                    parts.push(summary);
                }
                AiContextAttachment::Diagnostics => {
                    let current_key = self
                        .editor
                        .content
                        .file_path
                        .as_deref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let mut all: Vec<&DiagnosticItem> =
                        self.lsp.diagnostics.values().flatten().collect();
                    // 优先显示当前文件，再按 severity 排序（1=Error, 2=Warning）
                    all.sort_by_key(|d| {
                        let is_current = self
                            .editor
                            .content
                            .file_path
                            .as_deref()
                            .map(|p| p.to_string_lossy().to_string() == current_key)
                            .unwrap_or(false);
                        (if is_current { 0 } else { 1 }, d.severity)
                    });
                    if all.is_empty() {
                        parts.push("当前文件暂无 LSP 诊断信息。\n".to_string());
                    } else {
                        let mut text = String::from("当前 LSP 诊断：\n");
                        for d in all.iter().take(20) {
                            let severity = match d.severity {
                                1 => "Error",
                                2 => "Warning",
                                3 => "Information",
                                4 => "Hint",
                                _ => "Diagnostic",
                            };
                            text.push_str(&format!(
                                "[{}] {}:{} {}\n",
                                severity, d.line, d.col, d.message
                            ));
                        }
                        parts.push(text);
                    }
                }
                AiContextAttachment::FileTree => {
                    if let Some(tree) = &self.fs.file_tree {
                        parts.push(format!("工作区文件树：\n{}\n", self.format_file_tree(tree)));
                    } else {
                        parts.push("未加载工作区文件树。\n".to_string());
                    }
                }
                AiContextAttachment::CustomText(text) => {
                    parts.push(format!("用户附加文本：\n{}\n", text));
                }
            }
        }

        parts.join("\n")
    }
    /// 应用 AI 生成的代码到当前编辑器
    pub fn apply_ai_code(&mut self, code: &str) -> bool {
        if code.is_empty() {
            return false;
        }
        // 如果有选区，替换选区内容；否则在当前光标位置插入
        // C-02/H-21: 使用 zip 一次性解构，避免独立 unwrap 在中间状态变更后 panic
        if let Some(((start_line, start_col), (end_line, end_col))) = self
            .editor
            .content
            .selection_start
            .zip(self.editor.content.selection_end)
        {
            let (first_line, first_col) = if (start_line, start_col) <= (end_line, end_col) {
                (start_line, start_col)
            } else {
                (end_line, end_col)
            };
            let (last_line, last_col) = if (start_line, start_col) <= (end_line, end_col) {
                (end_line, end_col)
            } else {
                (start_line, start_col)
            };
            let start_byte = self.line_byte_start(first_line) + first_col;
            let end_byte = self.line_byte_start(last_line) + last_col;

            let old_text = self.editor.content.buffer.get_text(start_byte, end_byte);
            let cursor_before = CursorPosition::new(
                self.editor.content.cursor_line,
                self.editor.content.cursor_col,
            );

            self.editor.content.buffer.delete(start_byte, end_byte);
            self.editor.content.buffer.insert(start_byte, code);

            // 计算新光标位置
            let code_lines: Vec<&str> = code.lines().collect();
            let new_line = first_line + code_lines.len().saturating_sub(1);
            let new_col = if code_lines.len() <= 1 {
                first_col + code.len()
            } else {
                code_lines.last().unwrap_or(&"").len()
            };
            self.editor.content.cursor_line = new_line;
            self.editor.content.cursor_col = new_col;
            let cursor_after = CursorPosition::new(
                self.editor.content.cursor_line,
                self.editor.content.cursor_col,
            );
            self.editor.content.history.record_replace(
                start_byte,
                old_text,
                code,
                cursor_before,
                cursor_after,
            );

            self.clear_selection();
            self.editor.content.is_dirty = true;
            self.editor.content.buffer_version += 1;
            self.ui.status_message = "已应用 AI 代码".to_string();
            return true;
        }
        let pos = self.cursor_byte_pos();
        let cursor_before = CursorPosition::new(
            self.editor.content.cursor_line,
            self.editor.content.cursor_col,
        );

        self.editor.content.buffer.insert(pos, code);

        // 更新光标位置
        let _code_lines: Vec<&str> = code.lines().collect();
        let line_breaks = code.matches('\n').count();
        if line_breaks == 0 {
            self.editor.content.cursor_col += code.len();
        } else {
            self.editor.content.cursor_line += line_breaks;
            self.editor.content.cursor_col = code
                .rsplit_once('\n')
                .map(|(_, last)| last.len())
                .unwrap_or(0);
        }
        let cursor_after = CursorPosition::new(
            self.editor.content.cursor_line,
            self.editor.content.cursor_col,
        );
        self.editor
            .content
            .history
            .record_insert(pos, code, cursor_before, cursor_after);

        self.editor.content.is_dirty = true;
        self.editor.content.buffer_version += 1;
        self.ui.status_message = "已插入 AI 代码".to_string();
        true
    }
    /// 应用 AI 生成的工作区编辑（支持修改已打开/未打开的文件以及创建新文件）
    pub fn apply_ai_workspace_edits(
        &mut self,
        edits: &[AiEdit],
    ) -> std::result::Result<Vec<PathBuf>, String> {
        let mut applied = Vec::new();
        let original_tab = self.editor.tab_bar.active_tab;

        for edit in edits {
            let full_path = self.resolve_edit_path(&edit.path);

            // 删除文件操作
            if edit.is_delete() {
                // 关闭对应 tab（如果有）；用户取消则跳过此文件
                if let Some(idx) = self
                    .editor
                    .tab_bar
                    .tabs
                    .iter()
                    .position(|t| t.file_path() == Some(&full_path))
                {
                    if !self.close_tab(idx) {
                        continue;
                    }
                }
                // 从磁盘删除文件
                if full_path.exists() {
                    std::fs::remove_file(&full_path)
                        .map_err(|e| format!("删除文件 {} 失败: {}", full_path.display(), e))?;
                }
                self.ui.status_message = format!("已删除文件: {}", full_path.display());
                applied.push(full_path);
                continue;
            }

            // 找到或创建对应标签页
            let tab_idx = self
                .editor
                .tab_bar
                .tabs
                .iter()
                .position(|t| t.file_path() == Some(&full_path));
            if let Some(idx) = tab_idx {
                self.switch_tab(idx);
            } else if full_path.exists() {
                self.load_file(full_path.clone());
            } else {
                self.create_new_file_tab(&full_path);
            }

            // 应用单个编辑
            let old_text = self
                .editor
                .content
                .buffer
                .get_text(0, self.editor.content.buffer.len_bytes());
            let new_text = if edit.search.trim().is_empty() {
                edit.replace.clone()
            } else {
                match old_text.find(&edit.search) {
                    Some(pos) => {
                        let mut replaced = old_text.clone();
                        replaced.replace_range(pos..pos + edit.search.len(), &edit.replace);
                        replaced
                    }
                    None => {
                        return Err(format!(
                            "无法在 {} 中找到要替换的代码片段",
                            full_path.display()
                        ));
                    }
                }
            };

            // 记录 undo history，使 AI 工作区编辑可通过 Ctrl+Z 逐文件撤销
            let cursor_before = CursorPosition::new(
                self.editor.content.cursor_line,
                self.editor.content.cursor_col,
            );
            let len = self.editor.content.buffer.len_bytes();
            self.editor.content.buffer.delete(0, len);
            self.editor.content.buffer.insert(0, &new_text);
            // 全量替换后将光标复位到文件开头，避免越界
            self.editor.content.cursor_line = 0;
            self.editor.content.cursor_col = 0;
            let cursor_after = CursorPosition::new(0, 0);
            self.editor.content.history.record_replace(
                0,
                old_text.clone(),
                &new_text,
                cursor_before,
                cursor_after,
            );
            self.editor.content.buffer_version += 1;

            // 关键：将内容实际写入磁盘（当前工作区），而非仅停留在内存缓冲。
            // 先确保父目录存在（支持多级子目录自动创建），再原子写入。
            if let Some(parent) = full_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(format!("创建目录 {} 失败: {}", parent.display(), e));
                }
            }
            if let Err(e) = Self::atomic_write(&full_path, new_text.as_bytes()) {
                return Err(format!("写入文件 {} 失败: {}", full_path.display(), e));
            }
            // 已落盘，清除脏标记
            self.editor.content.is_dirty = false;
            self.ui.status_message = format!("已写入文件: {}", full_path.display());
            applied.push(full_path);
        }

        // 尽量回到原来的标签页
        if original_tab < self.editor.tab_bar.tabs.len() {
            self.switch_tab(original_tab);
        }

        Ok(applied)
    }
    pub(crate) fn resolve_edit_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        self.fs
            .current_folder
            .as_ref()
            .map(|root| root.join(path))
            .unwrap_or_else(|| path.to_path_buf())
    }

    /// 应用精准编辑：支持多种定位方式和编辑操作
    pub fn apply_precise_edits(
        &mut self,
        edits: &[crate::ai_panel::PreciseEdit],
    ) -> std::result::Result<Vec<PathBuf>, String> {
        let mut applied = Vec::new();
        let original_tab = self.editor.tab_bar.active_tab;

        for edit in edits {
            let full_path = self.resolve_edit_path(&edit.path);

            // 读取文件内容
            let content = if full_path.exists() {
                match std::fs::read_to_string(&full_path) {
                    Ok(c) => c,
                    Err(e) => return Err(format!("读取文件 {} 失败: {}", full_path.display(), e)),
                }
            } else {
                // 文件不存在，创建新文件
                String::new()
            };

            // 根据定位方式查找目标位置
            let (start_pos, end_pos) = self.locate_target_position(&content, &edit.location)?;

            // 根据编辑操作应用修改
            let new_content =
                self.apply_edit_operation(&content, start_pos, end_pos, &edit.operation)?;

            // 写入文件
            if let Some(parent) = full_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(format!("创建目录 {} 失败: {}", parent.display(), e));
                }
            }
            if let Err(e) = Self::atomic_write(&full_path, new_content.as_bytes()) {
                return Err(format!("写入文件 {} 失败: {}", full_path.display(), e));
            }

            applied.push(full_path);
        }

        // 尽量回到原来的标签页
        if original_tab < self.editor.tab_bar.tabs.len() {
            self.switch_tab(original_tab);
        }

        Ok(applied)
    }

    /// 根据定位方式查找目标位置
    fn locate_target_position(
        &self,
        content: &str,
        location: &crate::ai_panel::PreciseLocation,
    ) -> std::result::Result<(usize, usize), String> {
        match location {
            crate::ai_panel::PreciseLocation::Keyword {
                keyword,
                context_lines,
            } => self.locate_by_keyword(content, keyword, *context_lines),
            crate::ai_panel::PreciseLocation::LineRange {
                start_line,
                end_line,
            } => self.locate_by_line_range(content, *start_line, *end_line),
            crate::ai_panel::PreciseLocation::CodeSnippet {
                snippet,
                similarity_threshold,
            } => self.locate_by_code_snippet(content, snippet, *similarity_threshold),
        }
    }

    /// 关键词搜索定位
    fn locate_by_keyword(
        &self,
        content: &str,
        keyword: &str,
        context_lines: usize,
    ) -> std::result::Result<(usize, usize), String> {
        let lines: Vec<&str> = content.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if line.contains(keyword) {
                // 计算上下文范围
                let start_line = i.saturating_sub(context_lines);
                let end_line = (i + context_lines + 1).min(lines.len());

                // 转换为字符位置
                let start_pos = lines[..start_line].iter().map(|l| l.len() + 1).sum();
                let end_pos = lines[..end_line].iter().map(|l| l.len() + 1).sum();

                return Ok((start_pos, end_pos));
            }
        }

        Err(format!("未找到关键词: {}", keyword))
    }

    /// 行号范围定位
    fn locate_by_line_range(
        &self,
        content: &str,
        start_line: usize,
        end_line: usize,
    ) -> std::result::Result<(usize, usize), String> {
        let lines: Vec<&str> = content.lines().collect();

        if start_line == 0 || end_line == 0 || start_line > lines.len() || end_line > lines.len() {
            return Err(format!("行号范围无效: {}-{}", start_line, end_line));
        }

        // 转换为字符位置（行号从1开始）
        let start_pos = lines[..start_line - 1].iter().map(|l| l.len() + 1).sum();
        let end_pos = lines[..end_line].iter().map(|l| l.len() + 1).sum();

        Ok((start_pos, end_pos))
    }

    /// 代码片段摘要匹配
    fn locate_by_code_snippet(
        &self,
        content: &str,
        snippet: &str,
        _similarity_threshold: f32,
    ) -> std::result::Result<(usize, usize), String> {
        // 简化实现：直接查找片段
        if let Some(pos) = content.find(snippet) {
            let end_pos = pos + snippet.len();
            Ok((pos, end_pos))
        } else {
            Err(format!("未找到代码片段: {}", snippet))
        }
    }

    /// 根据编辑操作应用修改
    fn apply_edit_operation(
        &self,
        content: &str,
        start_pos: usize,
        end_pos: usize,
        operation: &crate::ai_panel::EditOperation,
    ) -> std::result::Result<String, String> {
        match operation {
            crate::ai_panel::EditOperation::Replace { new_content } => {
                let mut result = String::new();
                result.push_str(&content[..start_pos]);
                result.push_str(new_content);
                result.push_str(&content[end_pos..]);
                Ok(result)
            }
            crate::ai_panel::EditOperation::Insert {
                content: insert_content,
                position,
            } => {
                let mut result = String::new();
                match position {
                    crate::ai_panel::InsertPosition::Before => {
                        result.push_str(&content[..start_pos]);
                        result.push_str(insert_content);
                        result.push_str(&content[start_pos..]);
                    }
                    crate::ai_panel::InsertPosition::After => {
                        result.push_str(&content[..end_pos]);
                        result.push_str(insert_content);
                        result.push_str(&content[end_pos..]);
                    }
                }
                Ok(result)
            }
            crate::ai_panel::EditOperation::Delete => {
                let mut result = String::new();
                result.push_str(&content[..start_pos]);
                result.push_str(&content[end_pos..]);
                Ok(result)
            }
        }
    }

    /// 将工作区相对路径解析为经沙箱校验的绝对路径（仅允许工作区内，禁止逃逸）。
    fn workspace_sandbox_path(&self, rel: &str) -> std::result::Result<PathBuf, String> {
        let root = self
            .fs
            .current_folder
            .as_ref()
            .ok_or_else(|| "未打开工作区文件夹".to_string())?;
        let root_canon = root
            .canonicalize()
            .map_err(|e| format!("工作区路径无效: {}", e))?;
        // 拒绝绝对路径，避免逃逸工作区
        if Path::new(rel).is_absolute() {
            return Err("路径必须相对于工作区根目录".to_string());
        }
        let canon = root
            .join(rel)
            .canonicalize()
            .map_err(|e| format!("路径不存在或无法访问: {}", e))?;
        if !canon.starts_with(&root_canon) {
            return Err("禁止访问工作区之外的路径".to_string());
        }
        Ok(canon)
    }

    /// 读取工作区内文件内容（沙箱校验 + 大小/长度限制），供 Agent 只读探查。
    fn read_workspace_file(&self, rel: &str) -> std::result::Result<String, String> {
        let path = self.workspace_sandbox_path(rel)?;
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            return Err("这是一个目录，请改用列目录工具".to_string());
        }
        const MAX_BYTES: u64 = 1024 * 1024; // 1MB 上限，避免超大文件撑爆上下文
        if meta.len() > MAX_BYTES {
            return Err(format!(
                "文件过大（{} 字节，上限 1MB），请分段查看或改用命令",
                meta.len()
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let content = String::from_utf8_lossy(&bytes);
        // 限制回喂给模型的长度，避免占满上下文预算
        Ok(truncate_chars(&content, 8000))
    }

    /// 列出工作区内目录条目（沙箱校验 + 数量限制），目录在前、名称排序。
    fn list_workspace_dir(&self, rel: &str) -> std::result::Result<String, String> {
        let path = self.workspace_sandbox_path(rel)?;
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_dir() {
            return Err("这不是目录，请改用读取文件工具".to_string());
        }
        let mut items: Vec<(bool, String)> = Vec::new();
        for entry in std::fs::read_dir(&path)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            items.push((is_dir, name));
        }
        // 目录优先，其后按名称排序
        items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        const MAX_ENTRIES: usize = 300;
        let total = items.len();
        let mut lines: Vec<String> = items
            .into_iter()
            .take(MAX_ENTRIES)
            .map(|(is_dir, name)| if is_dir { format!("{}/", name) } else { name })
            .collect();
        if total > MAX_ENTRIES {
            lines.push(format!("…（共 {} 项，已省略其余）", total));
        }
        if lines.is_empty() {
            return Ok("（空目录）".to_string());
        }
        Ok(lines.join("\n"))
    }

    /// 执行一批只读探查请求，返回 `(面板展示行, 回喂给模型的反馈文本)`。
    fn execute_tool_requests(
        &self,
        reqs: &[crate::ai_panel::ToolRequest],
    ) -> (Vec<String>, String) {
        use crate::ai_panel::ToolRequest;
        let mut display: Vec<String> = Vec::new();
        let mut feedback: Vec<String> = Vec::new();
        for req in reqs {
            match req {
                ToolRequest::Read(path) => match self.read_workspace_file(path) {
                    Ok(content) => {
                        let n = content.lines().count();
                        display.push(format!("◎ 已读取 `{}`（{} 行）", path, n));
                        feedback.push(format!("[文件内容] {}\n```\n{}\n```", path, content));
                    }
                    Err(e) => {
                        display.push(format!("✕ 读取 `{}` 失败：{}", path, e));
                        feedback.push(format!("[文件读取失败] {}：{}", path, e));
                    }
                },
                ToolRequest::List(path) => {
                    let shown = if path.is_empty() { "." } else { path.as_str() };
                    match self.list_workspace_dir(path) {
                        Ok(listing) => {
                            display.push(format!("◇ 已列出目录 `{}`", shown));
                            feedback.push(format!("[目录列表] {}\n{}", shown, listing));
                        }
                        Err(e) => {
                            display.push(format!("✕ 列出 `{}` 失败：{}", shown, e));
                            feedback.push(format!("[目录列出失败] {}：{}", shown, e));
                        }
                    }
                }
            }
        }
        (display, feedback.join("\n\n"))
    }

    // ==================== CoT 多任务编排流水线 ====================

    /// 启动流水线：把规划器原始清单替换为可读的执行计划，创建 pipeline，执行首个任务。
    fn start_agent_pipeline(&mut self, goal: String, tasks: Vec<crate::ai_panel::PlannedTask>) {
        use crate::ai_panel::PlannedTaskKind;
        let mut lines = Vec::new();
        if goal.trim().is_empty() {
            lines.push("执行计划：".to_string());
        } else {
            lines.push(format!("执行计划（{}）：", goal.trim()));
        }
        for (i, t) in tasks.iter().enumerate() {
            let verb = match t.kind {
                PlannedTaskKind::File => "生成文件",
                PlannedTaskKind::Run => "运行",
            };
            if t.description.trim().is_empty() {
                lines.push(format!("{}. {} {}", i + 1, verb, t.target));
            } else {
                lines.push(format!(
                    "{}. {} {} — {}",
                    i + 1,
                    verb,
                    t.target,
                    t.description
                ));
            }
        }
        // 用可读计划替换规划器输出的原始 AETHER_PLAN 块，避免裸标记显示
        self.ai.ai_panel.rewrite_last_assistant(lines.join("\n"));
        self.ai.ai_panel.agent_pipeline = Some(crate::ai_panel::AgentPipeline {
            goal,
            tasks,
            cursor: 0,
            created_files: Vec::new(),
            failed_files: Vec::new(),
        });
        self.win.dirty_tracker.mark_full_window();
        self.run_pipeline_until_file_task_or_finish();
    }

    /// 顺序推进：RUN 任务直接执行并前进；FILE 任务发起 worker 调用后返回（由完成回调续推进）；任务耗尽则收尾。
    fn run_pipeline_until_file_task_or_finish(&mut self) {
        use crate::ai_panel::PlannedTaskKind;
        loop {
            let snapshot = {
                let Some(p) = self.ai.ai_panel.agent_pipeline.as_ref() else {
                    return;
                };
                if p.cursor >= p.tasks.len() {
                    None
                } else {
                    let t = &p.tasks[p.cursor];
                    Some((
                        t.kind.clone(),
                        t.target.clone(),
                        t.description.clone(),
                        p.cursor,
                        p.tasks.len(),
                        p.goal.clone(),
                        p.created_files.clone(),
                    ))
                }
            };
            let Some((kind, target, desc, cursor, total, goal, created)) = snapshot else {
                self.finish_agent_pipeline();
                return;
            };
            match kind {
                PlannedTaskKind::Run => {
                    self.ai.ai_panel.add_assistant_message(format!(
                        "[{}/{}] 运行 `{}`",
                        cursor + 1,
                        total,
                        target
                    ));
                    if self.fs.current_folder.is_some() {
                        self.run_pipeline_command(&target);
                    }
                    if let Some(p) = self.ai.ai_panel.agent_pipeline.as_mut() {
                        p.cursor += 1;
                    }
                    continue;
                }
                PlannedTaskKind::File => {
                    // 仅带目标文件现有内容（若存在）+ 目标 + 已建文件内容，窗口可控
                    let existing = self.read_workspace_file(&target).ok();
                    // 信息传递：把已生成文件的实际内容带给后续 worker（如 css/js 需要
                    // 引用 index.html 的类名/结构），read_workspace_file 自带长度截断。
                    let created_with_content: Vec<(String, String)> = created
                        .iter()
                        .map(|name| {
                            let content = self.read_workspace_file(name).unwrap_or_default();
                            (name.clone(), content)
                        })
                        .collect();
                    let (system, user) = crate::ai_panel::build_worker_prompt(
                        &goal,
                        &target,
                        &desc,
                        existing.as_deref(),
                        &created_with_content,
                    );
                    let mut settings = self.ui.app_settings.active_ai_settings();
                    // worker 为机械生成阶段（规划已完成思考）：强制关闭深度思考。
                    // DeepSeek 思维链计入 completion 输出预算，思考会挤占长文件的
                    // 生成空间，导致刚输出 FILE 头就撞 max_tokens 截断。
                    settings.thinking = Some(false);
                    // 代码生成场景固定低温采样（DeepSeek 官方建议 0.0）。
                    // 注意：思考模式下温度被忽略，但 worker 关闭思考后温度真实生效，
                    // 用户全局温度过高（如 2.0）会导致长代码生成退化为乱语。
                    settings.temperature = Some(0.0);
                    self.ui.status_message =
                        format!("[{}/{}] 正在生成 {} …", cursor + 1, total, target);
                    self.ai.ai_panel.stream_focused(&settings, system, user);
                    return; // 完成后由 advance_agent_pipeline 续推进
                }
            }
        }
    }

    /// worker 完成回调：把刚生成的文件落盘，记入已建清单，推进到下一任务。
    fn advance_agent_pipeline(&mut self, conv_idx: usize) {
        // 用户中途停止 → 中止流水线
        if self
            .ai
            .ai_panel
            .should_stop
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.ai.ai_panel.agent_pipeline = None;
            self.ai
                .ai_panel
                .add_assistant_message("已停止，剩余任务未执行。".to_string());
            self.win.dirty_tracker.mark_full_window();
            return;
        }
        // 落盘当前 worker 生成的文件；成功与否如实记录
        let mut wrote_ok = false;
        if let Some(text) = self.ai.ai_panel.last_assistant_text_of(conv_idx) {
            let edits = crate::ai_panel::parse_edits(&text, None);
            if !edits.is_empty() && self.fs.current_folder.is_some() {
                match self.apply_ai_workspace_edits(&edits) {
                    Ok(paths) => {
                        wrote_ok = !paths.is_empty();
                        for p in &paths {
                            let name = self
                                .fs
                                .current_folder
                                .as_ref()
                                .and_then(|root| p.strip_prefix(root).ok())
                                .unwrap_or(p.as_path());
                            self.ai
                                .ai_panel
                                .add_assistant_message(format!("✓ 已写入 `{}`", name.display()));
                        }
                        self.refresh_file_tree_light();
                    }
                    Err(e) => {
                        self.ai
                            .ai_panel
                            .add_assistant_message(format!("✕ 文件写入失败: {}", e));
                    }
                }
            }
        }
        // 记录成败并前进：成功的内容会传给后续 worker，失败的收尾时汇报
        if let Some(p) = self.ai.ai_panel.agent_pipeline.as_mut() {
            if let Some(task) = p.tasks.get(p.cursor) {
                if wrote_ok {
                    p.created_files.push(task.target.clone());
                } else {
                    p.failed_files.push(task.target.clone());
                }
            }
            p.cursor += 1;
        }
        self.win.dirty_tracker.mark_full_window();
        self.run_pipeline_until_file_task_or_finish();
    }

    /// 流水线收尾：清理状态并如实汇总成败。
    fn finish_agent_pipeline(&mut self) {
        let (total, failed, created_files, goal) = self
            .ai
            .ai_panel
            .agent_pipeline
            .as_ref()
            .map(|p| {
                (
                    p.tasks.len(),
                    p.failed_files.clone(),
                    p.created_files.clone(),
                    p.goal.clone(),
                )
            })
            .unwrap_or((0, Vec::new(), Vec::new(), String::new()));
        self.ai.ai_panel.agent_pipeline = None;

        // 生成详细的最终任务总结
        let mut summary_lines = Vec::new();

        // 标题：显示任务目标
        if !goal.trim().is_empty() {
            summary_lines.push(format!("[任务完成] **{}**", goal.trim()));
        } else {
            summary_lines.push("[任务完成] **任务执行完成**".to_string());
        }
        summary_lines.push("".to_string());

        // 执行结果统计
        let success_count = total - failed.len();
        if failed.is_empty() {
            summary_lines.push(format!("✓ 成功完成全部 {} 个任务", total));
            self.ui.status_message = "任务全部完成".to_string();
        } else {
            summary_lines.push(format!(
                "⚠ 任务部分完成：{} 个成功，{} 个失败",
                success_count,
                failed.len()
            ));
            self.ui.status_message = "部分任务未完成".to_string();
        }

        // 详细列出成功创建的文件
        if !created_files.is_empty() {
            summary_lines.push("".to_string());
            summary_lines.push(format!(
                "**成功创建的文件（{} 个）：**",
                created_files.len()
            ));
            for (i, file) in created_files.iter().enumerate() {
                summary_lines.push(format!("  {}. {}", i + 1, file));
            }
        }

        // 详细列出失败的文件
        if !failed.is_empty() {
            summary_lines.push("".to_string());
            summary_lines.push(format!("**未成功写入的文件（{} 个）：**", failed.len()));
            for (i, file) in failed.iter().enumerate() {
                summary_lines.push(format!("  {}. {}", i + 1, file));
            }
            summary_lines.push("".to_string());
            summary_lines.push("[建议] 可重新发送需求，系统将重试失败的文件".to_string());
        }

        // 添加任务执行时间（如果有）
        summary_lines.push("".to_string());
        summary_lines.push("---".to_string());
        summary_lines.push("[信息] 任务执行完毕，如有问题请随时告知".to_string());

        self.ai
            .ai_panel
            .add_assistant_message(summary_lines.join("\n"));
        self.win.dirty_tracker.mark_full_window();
    }

    /// 流水线内执行单条命令：打开底部终端、同步工作目录、排队执行（射后不理，不回喂）。
    fn run_pipeline_command(&mut self, cmd: &str) {
        self.ui.layout.bottom_panel_visible = true;
        self.terminal.bottom_panel_tab = crate::editor::BottomPanelTab::Terminal;
        if let Some(folder) = self.fs.current_folder.clone() {
            self.terminal.terminal_panel.cwd = folder.to_string_lossy().to_string();
        }
        if !self.terminal.terminal_panel.running {
            let _ = self.terminal.terminal_panel.start();
        }
        self.terminal.terminal_panel.queue_command(cmd.to_string());
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                self.win.hwnd,
                crate::window::TERM_TIMER_ID,
                crate::window::TERM_REFRESH_MS,
                None,
            );
        }
    }
}

/// 按字符数截断字符串（不切断 UTF-8 字符），超长时附加省略提示
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n…（输出过长已截断）");
    out
}
