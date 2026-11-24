use super::*;

impl EditorState {
    pub(super) fn render_ai_assistant_sidebar(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        unsafe {
            // 防御性检查：面板太小则跳过渲染
            if width < 20.0 || height < 20.0 {
                return;
            }

            // 局部重绘自包含：脏矩形裁剪帧跳过全窗口 clear（render/mod.rs），
            // 若此处不先铺面板背景，流式生成期间新文字会叠加在上一帧残影上，
            // 抗锯齿边缘逐帧累积导致文字变暗/闪烁（全窗口帧与裁剪帧交替时尤为明显）。
            let panel_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            app_theme::fill_app_gradient(target, &panel_rect, self.win.window_height as f32);

            // 文件卡片命中区域每帧重建（P0 可展开预览）；
            // 放在函数开头，历史视图/早退分支下也不会残留旧区域。
            self.ai.ai_panel.file_card_regions.clear();
            // 对话内文件名链接命中区 / Tab 无障碍焦点区同样每帧重建
            self.ai.ai_panel.file_link_regions.clear();
            self.ai.ai_panel.tab_focus_regions.clear();
            // 询问卡片选项/自定义命中区同样每帧重建
            self.ai.ai_panel.ask_option_regions.clear();
            self.ai.ai_panel.ask_custom_regions.clear();

            // 确保矢量图标几何已创建（AI 面板工具栏图标）
            self.ui.icons.ensure_created_from_target(target);

            // 安全获取文本格式，失败时跳过渲染
            let _bold_format = match self.win.render_ctx.text_format_cache.get_format(
                13.0,
                DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            let msg_format = match self.win.render_ctx.text_format_cache.get_format(
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            let small_format = match self.win.render_ctx.text_format_cache.get_format(
                10.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            // 右对齐小字体（文件卡片右侧的 +/- 行数统计）
            let stats_format = match self.win.render_ctx.text_format_cache.get_format(
                10.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_TRAILING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };

            // 安全获取画刷，失败时返回
            let _title_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_primary())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let dim_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_secondary())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let user_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_soft_hover())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let assistant_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::surface())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let tool_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::surface_alt())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let input_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::input_bg())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let sep_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::border())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let _accent_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let _green_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let yellow_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::warning())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let code_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::surface_alt())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let code_text_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_primary())
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let white_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(1.0, 1.0, 1.0, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };

            let margin = 10.0f32;
            // 顶部间距 6px（智能体模式下对话标签条移至左侧边栏，此处不再渲染）
            let mut cy = y + 6.0;

            // 清空命中区域（每帧重建；必须在注册任何命中区之前调用）
            self.ai.ai_panel.clear_hit_regions();

            // ===== 策略库已迁移到设置面板，此处不再渲染 =====

            // ===== 欢迎页/空工作区提示 =====
            let has_workspace =
                self.fs.current_folder.is_some() || self.editor.content.file_path.is_some();
            if !has_workspace {
                let hint_bg_color = app_theme::accent_soft();
                let hint_bg_brush = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &hint_bg_color)
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                let hint_bg_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + width - margin,
                    bottom: cy + 70.0,
                };
                fill_round_rect(target, &hint_bg_rect, 6.0, &hint_bg_brush);

                let hint_text: Vec<u16> = "当前工作区为空，请打开一个文件夹以继续。"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let hint_rect = D2D_RECT_F {
                    left: x + margin + 8.0,
                    top: cy + 10.0,
                    right: x + width - margin - 8.0,
                    bottom: cy + 28.0,
                };
                target.DrawText(
                    &hint_text,
                    &msg_format,
                    &hint_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // "浏览并选择文件夹" 按钮
                let open_btn_w = 120.0f32;
                let open_btn_h = 28.0f32;
                let open_btn_x = x + margin + 8.0;
                let open_btn_y = cy + 32.0;
                let open_btn_rect = D2D_RECT_F {
                    left: open_btn_x,
                    top: open_btn_y,
                    right: open_btn_x + open_btn_w,
                    bottom: open_btn_y + open_btn_h,
                };
                let open_btn_brush = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::accent())
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &open_btn_rect, 5.0, &open_btn_brush);
                let open_btn_text: Vec<u16> =
                    "浏览并选择文件夹".encode_utf16().chain(Some(0)).collect();
                let open_btn_text_rect = D2D_RECT_F {
                    left: open_btn_x,
                    top: open_btn_y + 5.0,
                    right: open_btn_x + open_btn_w,
                    bottom: open_btn_y + open_btn_h - 3.0,
                };
                target.DrawText(
                    &open_btn_text,
                    &small_format,
                    &open_btn_text_rect,
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 注册"浏览并选择文件夹"按钮命中区域（窗口绝对坐标）
                self.ai.ai_panel.browse_folder_region =
                    Some((open_btn_x, open_btn_y, open_btn_w, open_btn_h));

                cy += 80.0;

                // 分隔线
                let sep3_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + width - margin,
                    bottom: cy + 1.0,
                };
                target.FillRectangle(&sep3_rect, &sep_brush);
                cy += 10.0;
            }

            // ===== 聊天消息区域 =====
            // 使用上一帧计算的输入框高度（避免在渲染过程中重复计算）
            // 输入框区域高度 = 文本输入高度 + 工具栏与间距(44) + 图片 chips 行（若有）
            let input_area_h = self.ai.ai_panel.input_area_height();
            let chat_top = cy;
            let chat_bottom = y + height - input_area_h - 8.0;
            // 消息区域（自动换行 + 完整显示 + 代码块分段，不再按 80 字符截断）
            let content_left = x + margin;
            let content_right = x + width - margin;
            let seg_pad = 6.0f32;
            let _label_h = 14.0f32;
            let msg_gap = 12.0f32;
            let seg_gap = 4.0f32;
            // 自动滚到底：吸附底部时对齐到最新消息（用上一帧的最大滚动量）
            if self.ai.ai_panel.stick_to_bottom {
                self.ai.ai_panel.scroll_y = self.ai.ai_panel.content_height;
            }
            let dwrite = self.win.text_renderer.dwrite_factory();
            let content_start_y = chat_top - self.ai.ai_panel.scroll_y;
            let mut msg_y = content_start_y;
            let mut reasoning_regions_local: Vec<(usize, f32, f32, f32, f32)> = Vec::new();

            // 设置消息区域裁剪，防止滚动内容覆盖到上方标签栏和下方输入框
            let chat_clip_rect = D2D_RECT_F {
                left: x,
                top: chat_top,
                right: x + width,
                bottom: chat_bottom,
            };
            target.PushAxisAlignedClip(&chat_clip_rect, D2D1_ANTIALIAS_MODE_ALIASED);

            // 空会话引导卡（"你好，我是 AI 助手" + 快捷提示 chips）已移除：
            // 无消息时聊天区保持空白，引导由底部输入框承担。

            for (msg_index, msg) in self.ai.ai_panel.messages.iter().enumerate() {
                if msg.role == crate::ai_panel::AiRole::System {
                    continue;
                }
                let is_user = msg.role == crate::ai_panel::AiRole::User;
                let is_tool = msg.role == crate::ai_panel::AiRole::Tool;
                let is_pending_confirmation =
                    msg.role == crate::ai_panel::AiRole::PendingConfirmation;

                // 角色标签已移除：AI 消息直接显示文本，用户消息用气泡框区分
                if !is_tool && !is_pending_confirmation {
                    // 不再渲染"你"/"AI"文字标签，通过气泡样式区分
                }

                // 思考过程（DeepSeek 深度思考 reasoning_content）：独立分类、可折叠展示。
                // 与"回答"、"操作卡片"分开，视觉上弱化（紫灰、缩进、左强调条）。
                if !is_user {
                    if let Some(reasoning) = msg.reasoning.as_ref().filter(|r| !r.trim().is_empty())
                    {
                        let collapsed = msg.reasoning_collapsed;
                        let hdr_h = 20.0f32;
                        if msg_y + hdr_h >= chat_top && msg_y <= chat_bottom {
                            let arrow = if collapsed { "▶" } else { "▼" };
                            // 标题带思考耗时："深度思考 · 17s"；思考进行中实时累计
                            let hdr_text = match (msg.reasoning_ms, msg.reasoning_started_ms) {
                                (Some(ms), _) => format!(
                                    "{}  深度思考 · {}",
                                    arrow,
                                    crate::ai_panel::format_reasoning_duration(ms)
                                ),
                                (None, Some(start)) => {
                                    let elapsed =
                                        crate::ai_panel::now_millis().saturating_sub(start);
                                    format!(
                                        "{}  深度思考中… · {}",
                                        arrow,
                                        crate::ai_panel::format_reasoning_duration(elapsed)
                                    )
                                }
                                _ => format!("{}  深度思考", arrow),
                            };
                            let hw: Vec<u16> = hdr_text.encode_utf16().chain(Some(0)).collect();
                            if let Ok(hb) = self
                                .win
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &color_f(0.62, 0.55, 0.85, 1.0))
                            {
                                target.DrawText(
                                    &hw,
                                    &small_format,
                                    &D2D_RECT_F {
                                        left: content_left + 4.0,
                                        top: msg_y,
                                        right: content_right,
                                        bottom: msg_y + hdr_h,
                                    },
                                    &hb,
                                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                                    DWRITE_MEASURING_MODE_NATURAL,
                                );
                            }
                        }
                        reasoning_regions_local.push((
                            msg_index,
                            content_left,
                            msg_y,
                            content_right - content_left,
                            hdr_h,
                        ));
                        msg_y += hdr_h;
                        if !collapsed {
                            let inner_w =
                                (content_right - content_left - seg_pad * 2.0 - 10.0).max(20.0);
                            let r_wide: Vec<u16> = reasoning.encode_utf16().collect();
                            if let Ok(layout) =
                                dwrite.CreateTextLayout(&r_wide, &small_format, inner_w, 100000.0)
                            {
                                let mut m = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                                let text_h = if layout.GetMetrics(&mut m).is_ok() {
                                    m.height.max(12.0)
                                } else {
                                    12.0
                                };
                                let box_h = text_h + seg_pad * 2.0;
                                if msg_y + box_h >= chat_top && msg_y <= chat_bottom {
                                    if let Ok(bg) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &app_theme::accent_soft())
                                    {
                                        fill_round_rect(
                                            target,
                                            &D2D_RECT_F {
                                                left: content_left + 6.0,
                                                top: msg_y,
                                                right: content_right,
                                                bottom: msg_y + box_h,
                                            },
                                            4.0,
                                            &bg,
                                        );
                                    }
                                    if let Ok(ab) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &color_f(0.55, 0.48, 0.80, 1.0))
                                    {
                                        target.FillRectangle(
                                            &D2D_RECT_F {
                                                left: content_left + 6.0,
                                                top: msg_y,
                                                right: content_left + 9.0,
                                                bottom: msg_y + box_h,
                                            },
                                            &ab,
                                        );
                                    }
                                    if let Ok(fg) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &app_theme::text_secondary())
                                    {
                                        let origin = windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                                            x: content_left + 14.0,
                                            y: msg_y + seg_pad,
                                        };
                                        target.DrawTextLayout(
                                            origin,
                                            &layout,
                                            &fg,
                                            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                                        );
                                    }
                                }
                                msg_y += box_h + seg_gap;
                            }
                        }
                    }
                }

                // 将消息拆为渲染项：文本/代码段 + AI 文件/命令操作卡片。
                // 助手消息里的 <<<<<<< FILE/RUN >>>>>>> 标记转为清晰的操作卡片，隐藏原始标记；
                // 用户消息无标记，整体作为一段文本；Tool 消息同样整体作为一段文本。
                // 待确认消息也整体作为一段文本，但使用特殊样式渲染。
                let display_blocks = if is_user || is_tool || is_pending_confirmation {
                    // 用户消息附带过图片时，在正文前展示图片文件名标注（数据不持久化重发）
                    if is_user && !msg.image_names.is_empty() {
                        let img_line = format!("🖼 图片：{}\n", msg.image_names.join("、"));
                        vec![crate::ai_panel::AgentDisplayBlock::Text(format!(
                            "{}{}",
                            img_line, msg.content
                        ))]
                    } else {
                        vec![crate::ai_panel::AgentDisplayBlock::Text(
                            msg.content.clone(),
                        )]
                    }
                } else {
                    crate::ai_panel::parse_display_blocks(&msg.content)
                };
                let mut render_items: Vec<AiRenderItem> = Vec::new();
                for block in &display_blocks {
                    match block {
                        crate::ai_panel::AgentDisplayBlock::Text(t) => {
                            // 按 ``` 代码围栏拆分为普通段 / 代码段
                            let mut in_code = false;
                            let mut buf: Vec<&str> = Vec::new();
                            let mut current_lang: Option<String> = None;
                            for line in t.lines() {
                                if line.trim_start().starts_with("```") {
                                    if !buf.is_empty() {
                                        render_items.push(AiRenderItem::Seg {
                                            is_code: in_code,
                                            text: buf.join("\n"),
                                            lang: current_lang.clone(),
                                        });
                                        buf.clear();
                                    }
                                    // 提取语言标识
                                    if !in_code {
                                        current_lang = crate::ai_panel::syntax_highlight::extract_language_from_fence(line);
                                    } else {
                                        current_lang = None;
                                    }
                                    in_code = !in_code;
                                    continue;
                                }
                                buf.push(line);
                            }
                            if !buf.is_empty() {
                                render_items.push(AiRenderItem::Seg {
                                    is_code: in_code,
                                    text: buf.join("\n"),
                                    lang: current_lang,
                                });
                            }
                        }
                        crate::ai_panel::AgentDisplayBlock::File {
                            kind,
                            path,
                            content,
                            old,
                        } => {
                            render_items.push(AiRenderItem::File {
                                kind: kind.clone(),
                                path: path.clone(),
                                content: content.clone(),
                                old: old.clone(),
                            });
                        }
                        crate::ai_panel::AgentDisplayBlock::Run { cmd } => {
                            render_items.push(AiRenderItem::Run { cmd: cmd.clone() });
                        }
                        crate::ai_panel::AgentDisplayBlock::Read { path } => {
                            render_items.push(AiRenderItem::Read { path: path.clone() });
                        }
                        crate::ai_panel::AgentDisplayBlock::List { path } => {
                            render_items.push(AiRenderItem::List { path: path.clone() });
                        }
                        crate::ai_panel::AgentDisplayBlock::Symbol { name } => {
                            render_items.push(AiRenderItem::Symbol { name: name.clone() });
                        }
                        crate::ai_panel::AgentDisplayBlock::Open { query } => {
                            render_items.push(AiRenderItem::Open {
                                query: query.clone(),
                            });
                        }
                        crate::ai_panel::AgentDisplayBlock::ReadPage { url } => {
                            render_items.push(AiRenderItem::ReadPage { url: url.clone() });
                        }
                        crate::ai_panel::AgentDisplayBlock::Incomplete { path } => {
                            // 流式生成中的最后一条消息：未闭合块是正常中间态（正在生成），
                            // 只有生成结束后仍未闭合才是真正的"生成中断"。
                            let streaming = self.ai.ai_panel.is_generating
                                && msg_index + 1 == self.ai.ai_panel.messages.len();
                            if streaming {
                                render_items.push(AiRenderItem::Generating { path: path.clone() });
                            } else {
                                render_items.push(AiRenderItem::Incomplete { path: path.clone() });
                            }
                        }
                        crate::ai_panel::AgentDisplayBlock::Ask { question, options } => {
                            render_items.push(AiRenderItem::Ask {
                                question: question.clone(),
                                options: options.clone(),
                            });
                        }
                    }
                }
                if render_items.is_empty() {
                    render_items.push(AiRenderItem::Seg {
                        is_code: false,
                        text: String::new(),
                        lang: None,
                    });
                }

                // 引导式询问：本消息问题总数与当前活动问题序号（第一个未回答的），
                // 渲染时按序一次只展示一个，答完自动进入下一个
                let (ask_total, ask_active_seq) = {
                    let total = render_items
                        .iter()
                        .filter(|i| matches!(i, AiRenderItem::Ask { .. }))
                        .count();
                    let active = self
                        .ai
                        .ai_panel
                        .messages
                        .get(msg_index)
                        .map(|m| {
                            if m.ask_answers.is_empty() {
                                0
                            } else {
                                m.ask_answers
                                    .iter()
                                    .position(|a| a.is_none())
                                    .unwrap_or(total)
                            }
                        })
                        .unwrap_or(total);
                    (total, active)
                };

                // 消息内 File 卡序号（用于展开状态与命中区域的稳定标识）
                let mut file_block_seq = 0usize;
                // 消息内 Ask 卡序号（与 ask_answers 对齐）
                let mut ask_block_seq = 0usize;
                for item in &render_items {
                    // AI 文件/命令操作 → 渲染为清晰的操作卡片；其余按文本/代码段渲染
                    let (is_code, seg_text, seg_lang) = match item {
                        AiRenderItem::Seg {
                            is_code,
                            text,
                            lang,
                        } => (is_code, text, lang),
                        // 询问卡片：问题文本 + 选项按钮 + 自定义回答入口
                        AiRenderItem::Ask { question, options } => {
                            let seq = ask_block_seq;
                            ask_block_seq += 1;
                            // 回答状态（None = 未回答）与自定义输入等待态：先取数后绘制
                            let (answered, awaiting_custom) = {
                                let panel = &self.ai.ai_panel;
                                let ans = panel
                                    .messages
                                    .get(msg_index)
                                    .and_then(|m| m.ask_answers.get(seq).cloned())
                                    .flatten();
                                (ans, panel.pending_ask_custom == Some((msg_index, seq)))
                            };
                            // 已回答：折叠为紧凑单行摘要（✓ 问题 —— 答案）
                            if let Some(ans) = answered.as_deref() {
                                let row_h = 26.0f32;
                                if msg_y + row_h >= chat_top && msg_y <= chat_bottom {
                                    let summary = format!("✓ {} —— {}", question, ans);
                                    if let Ok(rb) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &app_theme::accent_soft())
                                    {
                                        fill_round_rect(
                                            target,
                                            &D2D_RECT_F {
                                                left: content_left,
                                                top: msg_y,
                                                right: content_right,
                                                bottom: msg_y + row_h,
                                            },
                                            5.0,
                                            &rb,
                                        );
                                    }
                                    if let Ok(sb) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &app_theme::accent_text())
                                    {
                                        let sw: Vec<u16> =
                                            summary.encode_utf16().chain(Some(0)).collect();
                                        target.DrawText(
                                            &sw,
                                            &small_format,
                                            &D2D_RECT_F {
                                                left: content_left + 12.0,
                                                top: msg_y,
                                                right: content_right - 8.0,
                                                bottom: msg_y + row_h,
                                            },
                                            &sb,
                                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                }
                                msg_y += row_h + 4.0;
                                continue;
                            }
                            if seq != ask_active_seq {
                                // 引导顺序：后续问题待当前问题回答后再展示，不占布局高度
                                continue;
                            }
                            let interactive = !self.ai.ai_panel.is_generating;

                            let pad = 10.0f32;
                            let header_h = if ask_total > 1 { 18.0f32 } else { 0.0 };
                            let inner_w = (content_right - content_left - pad * 2.0).max(20.0);
                            let q_wide: Vec<u16> = question.encode_utf16().collect();
                            let q_h = if let Ok(layout) =
                                dwrite.CreateTextLayout(&q_wide, &small_format, inner_w, 100000.0)
                            {
                                let mut m = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                                if layout.GetMetrics(&mut m).is_ok() {
                                    m.height.max(14.0)
                                } else {
                                    14.0
                                }
                            } else {
                                14.0
                            };
                            let opt_h = 30.0f32;
                            let opt_gap = 6.0f32;
                            let custom_h = 26.0f32;
                            let opts_total = options.len() as f32 * opt_h
                                + options.len().saturating_sub(1) as f32 * opt_gap;
                            let card_h =
                                pad + header_h + q_h + 8.0 + opts_total + 8.0 + custom_h + pad;

                            if msg_y + card_h >= chat_top && msg_y <= chat_bottom {
                                // 卡片背景 + 左强调条（与思考块统一紫色视觉）
                                if let Ok(bg) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::surface_alt())
                                {
                                    fill_round_rect(
                                        target,
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y,
                                            right: content_right,
                                            bottom: msg_y + card_h,
                                        },
                                        6.0,
                                        &bg,
                                    );
                                }
                                if let Ok(accent) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &color_f(0.62, 0.55, 0.85, 1.0))
                                {
                                    target.FillRectangle(
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y + 4.0,
                                            right: content_left + 3.0,
                                            bottom: msg_y + card_h - 4.0,
                                        },
                                        &accent,
                                    );
                                }
                                // 步骤指示（多问题时）：引导式提问 X / N
                                if ask_total > 1 {
                                    if let Ok(hb) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &color_f(0.62, 0.55, 0.85, 1.0))
                                    {
                                        let hlabel: Vec<u16> =
                                            format!("❓ 引导式提问 {} / {}\0", seq + 1, ask_total)
                                                .encode_utf16()
                                                .collect();
                                        target.DrawText(
                                            &hlabel,
                                            &small_format,
                                            &D2D_RECT_F {
                                                left: content_left + pad + 4.0,
                                                top: msg_y + pad,
                                                right: content_right - pad,
                                                bottom: msg_y + pad + header_h,
                                            },
                                            &hb,
                                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                }
                                // 问题文本
                                if let Ok(qb) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::text_primary())
                                {
                                    let qw: Vec<u16> =
                                        question.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &qw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + pad + 4.0,
                                            top: msg_y + pad + header_h,
                                            right: content_right - pad,
                                            bottom: msg_y + pad + header_h + q_h,
                                        },
                                        &qb,
                                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                }
                                // 选项按钮（选中高亮 + ✓ 前缀）
                                let opts_top = msg_y + pad + header_h + q_h + 8.0;
                                for (oi, opt) in options.iter().enumerate() {
                                    let oy = opts_top + oi as f32 * (opt_h + opt_gap);
                                    let selected = answered.as_deref() == Some(opt.as_str());
                                    let bgc = if selected {
                                        app_theme::accent_soft_hover()
                                    } else {
                                        app_theme::surface_alt()
                                    };
                                    if let Ok(ob) =
                                        self.win.render_ctx.brush_cache.get_brush(target, &bgc)
                                    {
                                        fill_round_rect(
                                            target,
                                            &D2D_RECT_F {
                                                left: content_left + pad,
                                                top: oy,
                                                right: content_right - pad,
                                                bottom: oy + opt_h,
                                            },
                                            5.0,
                                            &ob,
                                        );
                                    }
                                    let label = if selected {
                                        format!("✓ {}", opt)
                                    } else {
                                        opt.clone()
                                    };
                                    let tc = if selected {
                                        app_theme::accent_text()
                                    } else {
                                        app_theme::text_secondary()
                                    };
                                    if let Ok(tb) =
                                        self.win.render_ctx.brush_cache.get_brush(target, &tc)
                                    {
                                        let ow: Vec<u16> =
                                            label.encode_utf16().chain(Some(0)).collect();
                                        target.DrawText(
                                            &ow,
                                            &small_format,
                                            &D2D_RECT_F {
                                                left: content_left + pad + 10.0,
                                                top: oy,
                                                right: content_right - pad - 8.0,
                                                bottom: oy + opt_h,
                                            },
                                            &tb,
                                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                    if interactive {
                                        self.ai.ai_panel.ask_option_regions.push((
                                            msg_index,
                                            seq,
                                            oi,
                                            content_left + pad,
                                            oy,
                                            content_right - content_left - pad * 2.0,
                                            opt_h,
                                        ));
                                    }
                                }
                                // 自定义回答入口（选中时展示回答内容）
                                let cy = opts_top + opts_total + 8.0;
                                let custom_selected = answered.is_some()
                                    && !options
                                        .iter()
                                        .any(|o| Some(o.as_str()) == answered.as_deref());
                                let cbgc = if custom_selected {
                                    app_theme::accent_soft_hover()
                                } else {
                                    app_theme::surface_alt()
                                };
                                if let Ok(cb) =
                                    self.win.render_ctx.brush_cache.get_brush(target, &cbgc)
                                {
                                    fill_round_rect(
                                        target,
                                        &D2D_RECT_F {
                                            left: content_left + pad,
                                            top: cy,
                                            right: content_right - pad,
                                            bottom: cy + custom_h,
                                        },
                                        5.0,
                                        &cb,
                                    );
                                }
                                let clabel = if awaiting_custom {
                                    "✎ 请在下方输入框输入自定义回答并发送".to_string()
                                } else if custom_selected {
                                    format!(
                                        "✓ 自定义回答：{}",
                                        answered.as_deref().unwrap_or_default()
                                    )
                                } else {
                                    "✎ 自定义回答…".to_string()
                                };
                                let ctc = if awaiting_custom || custom_selected {
                                    app_theme::accent_text()
                                } else {
                                    app_theme::text_secondary()
                                };
                                if let Ok(ctb) =
                                    self.win.render_ctx.brush_cache.get_brush(target, &ctc)
                                {
                                    let cw: Vec<u16> =
                                        clabel.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &cw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + pad + 10.0,
                                            top: cy,
                                            right: content_right - pad - 8.0,
                                            bottom: cy + custom_h,
                                        },
                                        &ctb,
                                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                }
                                if interactive {
                                    self.ai.ai_panel.ask_custom_regions.push((
                                        msg_index,
                                        seq,
                                        content_left + pad,
                                        cy,
                                        content_right - content_left - pad * 2.0,
                                        custom_h,
                                    ));
                                }
                            }
                            msg_y += card_h + seg_gap;
                            continue;
                        }
                        AiRenderItem::File {
                            kind,
                            path,
                            content,
                            old,
                        } => {
                            let seq = file_block_seq;
                            file_block_seq += 1;
                            let card_h = 30.0f32;
                            // 展开状态：优先用差异快照（旧 vs 新）；无快照回退原始内容预览
                            let can_preview = !(content.trim().is_empty() && old.trim().is_empty());
                            let expanded = can_preview
                                && self
                                    .ai
                                    .ai_panel
                                    .expanded_file_cards
                                    .contains(&(msg_index, seq));
                            let diff_lines = if expanded {
                                // 差异预览查询（内联字段访问：messages 循环内无法调用
                                // ai_panel 的 &mut 方法；命中缓存或从快照现算并缓存）
                                let key = (msg_index, seq);
                                if let Some(c) = self.ai.ai_panel.diff_preview_cache.get(&key) {
                                    Some(c.clone())
                                } else if let Some((_, o, n)) = self
                                    .ai
                                    .ai_panel
                                    .diff_snapshots
                                    .iter()
                                    .rev()
                                    .find(|(p, _, _)| p == path)
                                {
                                    let d = crate::ai_panel::diff::line_diff(o, n);
                                    self.ai.ai_panel.diff_preview_cache.insert(key, d.clone());
                                    Some(d)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            let preview_h = if expanded {
                                let n = if let Some(d) = &diff_lines {
                                    d.len() as f32
                                } else {
                                    let src = if content.trim().is_empty() {
                                        old
                                    } else {
                                        content
                                    };
                                    src.lines().count() as f32
                                };
                                (n * 16.0 + 12.0).clamp(24.0, 240.0)
                            } else {
                                0.0
                            };
                            let total_h = card_h + preview_h;
                            if msg_y + total_h >= chat_top && msg_y <= chat_bottom {
                                // 操作类型主题色：新建=绿 / 修改=蓝 / 删除=红
                                let (op_color, op_label) = match kind {
                                    crate::ai_panel::FileOpKind::Create => {
                                        (color_f(0.40, 0.80, 0.52, 1.0), "新建")
                                    }
                                    crate::ai_panel::FileOpKind::Modify => {
                                        (color_f(0.40, 0.70, 1.0, 1.0), "修改")
                                    }
                                    crate::ai_panel::FileOpKind::Delete => {
                                        (color_f(0.92, 0.52, 0.52, 1.0), "删除")
                                    }
                                };
                                // 卡片背景（头部 + 预览区整体一个圆角矩形）
                                if let Ok(cb) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::surface())
                                {
                                    fill_round_rect(
                                        target,
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y,
                                            right: content_right,
                                            bottom: msg_y + total_h,
                                        },
                                        5.0,
                                        &cb,
                                    );
                                }
                                // 左侧主题色竖条
                                if let Ok(ab) =
                                    self.win.render_ctx.brush_cache.get_brush(target, &op_color)
                                {
                                    target.FillRectangle(
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y,
                                            right: content_left + 3.0,
                                            bottom: msg_y + total_h,
                                        },
                                        &ab,
                                    );
                                }
                                // 文件图标（按扩展名映射语言图标，未命中回退通用文件图标）
                                let file_name =
                                    path.rsplit(['/', '\\']).next().unwrap_or(path.as_str());
                                let icon_kind = self
                                    .get_file_vector_icon(file_name)
                                    .unwrap_or(crate::icons::IconKind::File);
                                if let Ok(icon_brush) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::text_secondary())
                                {
                                    self.ui.icons.draw(
                                        target,
                                        icon_kind,
                                        content_left + 10.0,
                                        msg_y + (card_h - 16.0) / 2.0,
                                        16.0,
                                        16.0,
                                        &icon_brush,
                                    );
                                }
                                // 文件名（右侧预留统计/徽章位，超长裁剪）
                                if let Ok(nb) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::text_primary())
                                {
                                    let nw: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &nw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + 32.0,
                                            top: msg_y,
                                            right: content_right - 178.0,
                                            bottom: msg_y + card_h,
                                        },
                                        &nb,
                                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                }
                                // 行数统计：+新增 / -删除（基于修改前快照的差异）
                                let (added, removed) = if old.trim().is_empty() {
                                    (content.lines().count(), 0)
                                } else {
                                    crate::ai_panel::diff::diff_stats(
                                        &crate::ai_panel::diff::line_diff(old, content),
                                    )
                                };
                                if added + removed > 0 {
                                    let (stats_text, stats_color) = match kind {
                                        crate::ai_panel::FileOpKind::Create => {
                                            (format!("+{}", added), color_f(0.40, 0.80, 0.52, 1.0))
                                        }
                                        crate::ai_panel::FileOpKind::Delete => (
                                            format!("-{}", removed),
                                            color_f(0.92, 0.52, 0.52, 1.0),
                                        ),
                                        crate::ai_panel::FileOpKind::Modify => (
                                            format!("+{} -{}", added, removed),
                                            color_f(0.60, 0.68, 0.78, 1.0),
                                        ),
                                    };
                                    if let Ok(sb) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &stats_color)
                                    {
                                        let sw: Vec<u16> =
                                            stats_text.encode_utf16().chain(Some(0)).collect();
                                        target.DrawText(
                                            &sw,
                                            &stats_format,
                                            &D2D_RECT_F {
                                                left: content_right - 178.0,
                                                top: msg_y,
                                                right: content_right - 100.0,
                                                bottom: msg_y + card_h,
                                            },
                                            &sb,
                                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                }
                                // 操作类型徽章（半透明主题色底 + 主题色文字）
                                {
                                    let lw0: Vec<u16> = op_label.encode_utf16().collect();
                                    let mut text_w = 22.0f32;
                                    if let Ok(bl) =
                                        dwrite.CreateTextLayout(&lw0, &small_format, 200.0, 20.0)
                                    {
                                        let mut bm = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                                        if bl.GetMetrics(&mut bm).is_ok() {
                                            text_w = bm.width;
                                        }
                                    }
                                    let badge_w = text_w + 12.0;
                                    let badge_x = content_right - 30.0 - badge_w;
                                    let mut badge_bg = op_color;
                                    badge_bg.a = 0.16;
                                    if let Ok(bb) =
                                        self.win.render_ctx.brush_cache.get_brush(target, &badge_bg)
                                    {
                                        fill_round_rect(
                                            target,
                                            &D2D_RECT_F {
                                                left: badge_x,
                                                top: msg_y + 6.0,
                                                right: badge_x + badge_w,
                                                bottom: msg_y + card_h - 6.0,
                                            },
                                            3.0,
                                            &bb,
                                        );
                                    }
                                    if let Ok(tb) =
                                        self.win.render_ctx.brush_cache.get_brush(target, &op_color)
                                    {
                                        let lwz: Vec<u16> =
                                            op_label.encode_utf16().chain(Some(0)).collect();
                                        target.DrawText(
                                            &lwz,
                                            &small_format,
                                            &D2D_RECT_F {
                                                left: badge_x + (badge_w - text_w) / 2.0,
                                                top: msg_y + 6.0,
                                                right: badge_x + badge_w,
                                                bottom: msg_y + card_h - 6.0,
                                            },
                                            &tb,
                                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                }
                                // 展开指示符与预览区（差异可视化：新增=绿半透明/删除=红半透明）
                                if can_preview {
                                    if let Ok(ib) = self
                                        .win
                                        .render_ctx
                                        .brush_cache
                                        .get_brush(target, &app_theme::text_secondary())
                                    {
                                        let ind = if expanded { "v" } else { ">" };
                                        let iw: Vec<u16> =
                                            ind.encode_utf16().chain(Some(0)).collect();
                                        target.DrawText(
                                            &iw,
                                            &small_format,
                                            &D2D_RECT_F {
                                                left: content_right - 22.0,
                                                top: msg_y,
                                                right: content_right - 6.0,
                                                bottom: msg_y + card_h,
                                            },
                                            &ib,
                                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                                            DWRITE_MEASURING_MODE_NATURAL,
                                        );
                                    }
                                    if expanded && preview_h > 0.0 {
                                        let prev_top = msg_y + card_h;
                                        if let Some(diff) = &diff_lines {
                                            // 差异预览：逐行按类型着色背景（Direct2D 半透明填充）
                                            let max_lines = (((preview_h - 12.0) / 16.0).floor()
                                                as usize)
                                                .max(1);
                                            let mut ly = prev_top + 6.0;
                                            for (i, dl) in diff.iter().enumerate() {
                                                if i >= max_lines {
                                                    if let Ok(mb) =
                                                        self.win.render_ctx.brush_cache.get_brush(
                                                            target,
                                                            &app_theme::text_secondary(),
                                                        )
                                                    {
                                                        let mw: Vec<u16> = format!(
                                                            "…（共 {} 行差异）",
                                                            diff.len()
                                                        )
                                                        .encode_utf16()
                                                        .chain(Some(0))
                                                        .collect();
                                                        target.DrawText(
                                                            &mw,
                                                            &small_format,
                                                            &D2D_RECT_F {
                                                                left: content_left + 10.0,
                                                                top: ly,
                                                                right: content_right - 8.0,
                                                                bottom: ly + 16.0,
                                                            },
                                                            &mb,
                                                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                                            DWRITE_MEASURING_MODE_NATURAL,
                                                        );
                                                    }
                                                    break;
                                                }
                                                let (prefix, text_color, bg_color) = match dl.kind {
                                                    crate::ai_panel::diff::DiffKind::Add => (
                                                        "+",
                                                        color_f(0.62, 0.86, 0.62, 1.0),
                                                        Some(color_f(0.30, 0.65, 0.35, 0.22)),
                                                    ),
                                                    crate::ai_panel::diff::DiffKind::Del => (
                                                        "-",
                                                        color_f(0.90, 0.62, 0.62, 1.0),
                                                        Some(color_f(0.85, 0.35, 0.35, 0.25)),
                                                    ),
                                                    crate::ai_panel::diff::DiffKind::Same => {
                                                        (" ", app_theme::text_secondary(), None)
                                                    }
                                                };
                                                if let Some(bg) = bg_color {
                                                    if let Ok(bb2) = self
                                                        .win
                                                        .render_ctx
                                                        .brush_cache
                                                        .get_brush(target, &bg)
                                                    {
                                                        target.FillRectangle(
                                                            &D2D_RECT_F {
                                                                left: content_left + 6.0,
                                                                top: ly,
                                                                right: content_right - 6.0,
                                                                bottom: ly + 15.0,
                                                            },
                                                            &bb2,
                                                        );
                                                    }
                                                }
                                                let line_text = format!("{} {}", prefix, dl.text);
                                                let tw: Vec<u16> = line_text
                                                    .encode_utf16()
                                                    .chain(Some(0))
                                                    .collect();
                                                if let Ok(tb2) = self
                                                    .win
                                                    .render_ctx
                                                    .brush_cache
                                                    .get_brush(target, &text_color)
                                                {
                                                    target.DrawText(
                                                        &tw,
                                                        &small_format,
                                                        &D2D_RECT_F {
                                                            left: content_left + 10.0,
                                                            top: ly,
                                                            right: content_right - 8.0,
                                                            bottom: ly + 16.0,
                                                        },
                                                        &tb2,
                                                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                                        DWRITE_MEASURING_MODE_NATURAL,
                                                    );
                                                }
                                                ly += 16.0;
                                            }
                                        } else {
                                            // 无快照：回退纯内容预览（删除卡显示旧内容）
                                            let src = if content.trim().is_empty() {
                                                old.as_str()
                                            } else {
                                                content.as_str()
                                            };
                                            let max_lines = (((preview_h - 12.0) / 16.0).floor()
                                                as usize)
                                                .max(1);
                                            let all: Vec<&str> = src.lines().collect();
                                            let shown: String = if all.len() > max_lines {
                                                format!(
                                                    "{}\n…（共 {} 行）",
                                                    all[..max_lines.saturating_sub(1)].join("\n"),
                                                    all.len()
                                                )
                                            } else {
                                                src.to_string()
                                            };
                                            if let Ok(tb2) = self
                                                .win
                                                .render_ctx
                                                .brush_cache
                                                .get_brush(target, &app_theme::text_secondary())
                                            {
                                                let cw: Vec<u16> =
                                                    shown.encode_utf16().chain(Some(0)).collect();
                                                target.DrawText(
                                                    &cw,
                                                    &small_format,
                                                    &D2D_RECT_F {
                                                        left: content_left + 10.0,
                                                        top: prev_top + 6.0,
                                                        right: content_right - 8.0,
                                                        bottom: prev_top + preview_h - 4.0,
                                                    },
                                                    &tb2,
                                                    D2D1_DRAW_TEXT_OPTIONS_CLIP
                                                        | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                                                    DWRITE_MEASURING_MODE_NATURAL,
                                                );
                                            }
                                        }
                                    }
                                    // 命中区域：点击标题行切换展开；登记 Tab 无障碍焦点
                                    self.ai.ai_panel.file_card_regions.push((
                                        msg_index,
                                        seq,
                                        content_left,
                                        msg_y,
                                        content_right - content_left,
                                        card_h,
                                    ));
                                    self.ai.ai_panel.tab_focus_regions.push((
                                        content_left,
                                        msg_y,
                                        content_right - content_left,
                                        card_h,
                                        crate::ai_panel::TabFocusAction::ToggleCard(msg_index, seq),
                                    ));
                                }
                            }
                            msg_y += total_h + seg_gap;
                            continue;
                        }
                        _ => {
                            let card_h = 30.0f32;
                            let total_h = card_h;
                            if msg_y + total_h >= chat_top && msg_y <= chat_bottom {
                                let (glyph, label, detail, op_color) = agent_op_display(item);
                                if let Ok(cb) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::surface())
                                {
                                    fill_round_rect(
                                        target,
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y,
                                            right: content_right,
                                            bottom: msg_y + card_h,
                                        },
                                        5.0,
                                        &cb,
                                    );
                                }
                                if let Ok(ab) =
                                    self.win.render_ctx.brush_cache.get_brush(target, &op_color)
                                {
                                    target.FillRectangle(
                                        &D2D_RECT_F {
                                            left: content_left,
                                            top: msg_y,
                                            right: content_left + 3.0,
                                            bottom: msg_y + card_h,
                                        },
                                        &ab,
                                    );
                                    let gw: Vec<u16> =
                                        glyph.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &gw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + 10.0,
                                            top: msg_y,
                                            right: content_left + 30.0,
                                            bottom: msg_y + card_h,
                                        },
                                        &ab,
                                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                    let lw: Vec<u16> =
                                        label.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &lw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + 30.0,
                                            top: msg_y,
                                            right: content_left + 96.0,
                                            bottom: msg_y + card_h,
                                        },
                                        &ab,
                                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                }
                                if let Ok(db) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &app_theme::text_secondary())
                                {
                                    let dw: Vec<u16> =
                                        detail.encode_utf16().chain(Some(0)).collect();
                                    target.DrawText(
                                        &dw,
                                        &small_format,
                                        &D2D_RECT_F {
                                            left: content_left + 100.0,
                                            top: msg_y,
                                            right: content_right - 8.0,
                                            bottom: msg_y + card_h,
                                        },
                                        &db,
                                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                                        DWRITE_MEASURING_MODE_NATURAL,
                                    );
                                }
                            }
                            msg_y += total_h + seg_gap;
                            continue;
                        }
                    };
                    let inner_w = if *is_code {
                        (content_right - content_left - seg_pad * 2.0 - 8.0).max(20.0)
                    } else if is_user {
                        // 用户气泡最大宽度约内容区 72%，右对齐、随内容收缩（参考 Qoder）
                        ((content_right - content_left) * 0.72 - seg_pad * 2.0).max(40.0)
                    } else {
                        (content_right - content_left - seg_pad * 2.0).max(20.0)
                    };

                    // 普通段解析轻量 Markdown；代码段应用语法高亮
                    #[allow(clippy::type_complexity)]
                    let (layout_wide, bolds, headings, codes, highlight_ranges): (
                        Vec<u16>,
                        Vec<(u32, u32)>,
                        Vec<(u32, u32, f32)>,
                        Vec<(u32, u32)>,
                        Vec<crate::ai_panel::syntax_highlight::HighlightRange>,
                    ) = if *is_code {
                        // 代码段：应用语法高亮
                        let (text, ranges) = crate::ai_panel::syntax_highlight::highlight_code_block(
                            seg_text,
                            seg_lang.as_deref(),
                        );
                        (text, Vec::new(), Vec::new(), Vec::new(), ranges)
                    } else {
                        // 普通段：解析轻量 Markdown
                        let (text, b, h, c) = crate::ai_panel::parse_markdown_segment(seg_text);
                        (text, b, h, c, Vec::new())
                    };
                    let layout =
                        match dwrite.CreateTextLayout(&layout_wide, &msg_format, inner_w, 100000.0)
                        {
                            Ok(l) => l,
                            Err(_) => {
                                msg_y += 14.0 + seg_pad * 2.0 + seg_gap;
                                continue;
                            }
                        };
                    // 本段内可点击的文件链接范围 (start, len, 路径)（仅普通段）
                    let mut link_ranges: Vec<(u32, u32, String)> = Vec::new();
                    if !*is_code {
                        for (bs, bl) in &bolds {
                            let _ = layout.SetFontWeight(
                                DWRITE_FONT_WEIGHT_BOLD,
                                windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                                    startPosition: *bs,
                                    length: *bl,
                                },
                            );
                        }
                        for (hs, hl, hsize) in &headings {
                            let r = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                                startPosition: *hs,
                                length: *hl,
                            };
                            let _ = layout.SetFontSize(*hsize, r);
                            let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, r);
                        }
                        // 行内代码 span：类路径 token 链接化（下划线 + 点击/Tab 命中区）
                        for &(cs, cl) in &codes {
                            let s = cs as usize;
                            let e = ((cs + cl) as usize).min(layout_wide.len());
                            if e <= s {
                                continue;
                            }
                            let tok = String::from_utf16_lossy(&layout_wide[s..e])
                                .trim()
                                .to_string();
                            if crate::ai_panel::is_path_like_token(&tok) {
                                let _ = layout.SetUnderline(
                                    true,
                                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                                        startPosition: cs,
                                        length: cl,
                                    },
                                );
                                link_ranges.push((cs, cl, tok));
                            }
                        }
                    } else {
                        // 代码段：应用语法高亮样式
                        for range in &highlight_ranges {
                            let dwrite_range =
                                windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                                    startPosition: range.start,
                                    length: range.len,
                                };

                            // 应用前景色
                            let (r, g, b, a) = range.fg_color;
                            if let Ok(brush) = self
                                .win
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &color_f(r, g, b, a))
                            {
                                let _ = layout.SetDrawingEffect(&brush, dwrite_range);
                            }

                            // 应用粗体
                            if range.bold {
                                let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, dwrite_range);
                            }

                            // 应用斜体
                            if range.italic {
                                let _ = layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, dwrite_range);
                            }
                        }
                    }
                    let mut m =
                        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                    let text_h = if layout.GetMetrics(&mut m).is_ok() {
                        m.height.max(14.0)
                    } else {
                        14.0
                    };
                    let seg_h = text_h + seg_pad * 2.0;

                    // 完全在视口外：仅累加高度，跳过绘制
                    if msg_y + seg_h < chat_top || msg_y > chat_bottom {
                        msg_y += seg_h + seg_gap;
                        continue;
                    }

                    let seg_left = if *is_code {
                        content_left + 4.0
                    } else if is_user {
                        // 右对齐气泡：宽度随文本内容收缩，最大不超过 inner_w + 内边距
                        let bubble_w = (m.width + seg_pad * 2.0).min(inner_w + seg_pad * 2.0);
                        content_right - bubble_w
                    } else {
                        content_left
                    };
                    // AI 普通文本段不画气泡背景（直接显示文字），
                    // 用户/代码/工具/待确认消息保留气泡背景
                    let is_ai_plain_text =
                        !is_user && !is_tool && !is_pending_confirmation && !*is_code;
                    if !is_ai_plain_text {
                        let seg_bg: &ID2D1SolidColorBrush = if *is_code {
                            &code_bg_brush
                        } else if is_user {
                            &user_bg_brush
                        } else if is_tool {
                            &tool_bg_brush
                        } else if is_pending_confirmation {
                            // 待确认消息使用浅黄色背景（#4A4020）
                            &yellow_brush
                        } else {
                            &assistant_bg_brush
                        };
                        let seg_rect = D2D_RECT_F {
                            left: seg_left,
                            top: msg_y,
                            right: content_right,
                            bottom: msg_y + seg_h,
                        };
                        let seg_radius = if is_user { 12.0 } else { 6.0 };
                        fill_round_rect(target, &seg_rect, seg_radius, seg_bg);

                        // 待确认消息添加左侧黄色竖线标识
                        if is_pending_confirmation {
                            let accent_rect = D2D_RECT_F {
                                left: seg_left,
                                top: msg_y,
                                right: seg_left + 3.0,
                                bottom: msg_y + seg_h,
                            };
                            target.FillRectangle(&accent_rect, &yellow_brush);
                        }
                    }

                    let seg_fg: &ID2D1SolidColorBrush = if *is_code {
                        &code_text_brush
                    } else if is_tool {
                        &dim_brush
                    } else if is_pending_confirmation {
                        // 待确认消息使用白色文本以确保在黄色背景上可读
                        &white_brush
                    } else {
                        text_brush
                    };
                    let origin = windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                        x: seg_left + seg_pad,
                        y: msg_y + seg_pad,
                    };
                    target.DrawTextLayout(
                        origin,
                        &layout,
                        seg_fg,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );

                    // 文件链接：绘制强调色下划线并注册点击命中区 / Tab 焦点区
                    if !link_ranges.is_empty() {
                        if let Ok(lb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &color_f(0.45, 0.72, 1.0, 1.0))
                        {
                            for (cs, cl, lpath) in &link_ranges {
                                let mut hit: Vec<
                                    windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS,
                                > = vec![Default::default(); *cl as usize];
                                let mut actual = 0u32;
                                if layout
                                    .HitTestTextRange(
                                        *cs,
                                        *cl,
                                        0.0,
                                        0.0,
                                        Some(&mut hit),
                                        &mut actual,
                                    )
                                    .is_ok()
                                {
                                    for hm in hit.iter().take(actual as usize) {
                                        if hm.width <= 0.0 || hm.height <= 0.0 {
                                            continue;
                                        }
                                        let rx = origin.x + hm.left;
                                        let ry = origin.y + hm.top;
                                        target.FillRectangle(
                                            &D2D_RECT_F {
                                                left: rx,
                                                top: ry + hm.height - 1.0,
                                                right: rx + hm.width,
                                                bottom: ry + hm.height,
                                            },
                                            &lb,
                                        );
                                        self.ai.ai_panel.file_link_regions.push((
                                            rx,
                                            ry,
                                            hm.width,
                                            hm.height,
                                            lpath.clone(),
                                        ));
                                        self.ai.ai_panel.tab_focus_regions.push((
                                            rx,
                                            ry,
                                            hm.width,
                                            hm.height,
                                            crate::ai_panel::TabFocusAction::OpenFile(
                                                lpath.clone(),
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    // 代码块添加"保存为文件"按钮（仅 AI 助手消息）
                    if *is_code && !is_user && !is_tool && !seg_text.is_empty() {
                        let save_btn_w = 60.0f32;
                        let save_btn_h = 18.0f32;
                        let save_btn_x = content_right - save_btn_w - 4.0;
                        let save_btn_y = msg_y + 2.0;
                        let save_btn_rect = D2D_RECT_F {
                            left: save_btn_x,
                            top: save_btn_y,
                            right: save_btn_x + save_btn_w,
                            bottom: save_btn_y + save_btn_h,
                        };
                        let save_bg = app_theme::accent();
                        if let Ok(save_brush) =
                            self.win.render_ctx.brush_cache.get_brush(target, &save_bg)
                        {
                            fill_round_rect(target, &save_btn_rect, 3.0, &save_brush);
                        }
                        let save_text: Vec<u16> = "保存".encode_utf16().chain(Some(0)).collect();
                        let save_text_rect = D2D_RECT_F {
                            left: save_btn_x,
                            top: save_btn_y + 1.0,
                            right: save_btn_x + save_btn_w,
                            bottom: save_btn_y + save_btn_h - 1.0,
                        };
                        target.DrawText(
                            &save_text,
                            &small_format,
                            &save_text_rect,
                            &white_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                        // 注册保存按钮区域（简化：只存储 y 范围，点击时通过内容匹配）
                        // 实际文件名从消息内容中解析
                    }

                    msg_y += seg_h + seg_gap;
                }

                msg_y += msg_gap;
            }
            // 提交本帧收集的思考块折叠命中区（循环内借用了 messages，无法直接写回，故循环后赋值）
            self.ai.ai_panel.reasoning_toggle_regions = reasoning_regions_local;

            // Tab 无障碍焦点环：高亮当前焦点区域（文件卡/文件链接）
            if let Some(fi) = self.ai.ai_panel.tab_focus_index {
                if let Some((fx, fy, fw, fh, _)) = self.ai.ai_panel.tab_focus_regions.get(fi) {
                    let (fx, fy, fw, fh) = (*fx, *fy, *fw, *fh);
                    if fy + fh >= chat_top && fy <= chat_bottom {
                        if let Ok(fb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &app_theme::accent_border())
                        {
                            let _ = target.DrawRectangle(
                                &D2D_RECT_F {
                                    left: fx - 1.0,
                                    top: fy - 1.0,
                                    right: fx + fw + 1.0,
                                    bottom: fy + fh + 1.0,
                                },
                                &fb,
                                1.5,
                                None,
                            );
                        }
                    }
                }
            }

            // 记录内容高度与最大滚动量（供滚轮/滚动条），并绘制滚动条
            let viewport_h = (chat_bottom - chat_top).max(1.0);
            let total_content = (msg_y - content_start_y).max(0.0);
            self.ai.ai_panel.content_height = (total_content - viewport_h).max(0.0);
            if self.ai.ai_panel.content_height > 0.0 {
                let track_h = viewport_h;
                let total = total_content.max(viewport_h);
                let thumb_h = (viewport_h / total * track_h).max(24.0);
                let denom = self.ai.ai_panel.content_height.max(1.0);
                let scroll_ratio = (self.ai.ai_panel.scroll_y / denom).clamp(0.0, 1.0);
                let thumb_y = chat_top + scroll_ratio * (track_h - thumb_h);
                let track_x = x + width - 6.0;
                let thumb_rect = D2D_RECT_F {
                    left: track_x,
                    top: thumb_y,
                    right: track_x + 4.0,
                    bottom: thumb_y + thumb_h,
                };
                if let Ok(sb) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::border_strong())
                {
                    fill_round_rect(target, &thumb_rect, 2.0, &sb);
                }
            }

            // 正在生成指示器（带动画点）
            if self.ai.ai_panel.is_generating && msg_y < chat_bottom && msg_y + 16.0 > chat_top {
                let typing_text = format!(
                    "AI 正在思考{}",
                    ".".repeat((self.ai.ai_panel.messages.len() % 3) + 1)
                );
                let typing: Vec<u16> = typing_text.encode_utf16().chain(Some(0)).collect();
                let typing_rect = D2D_RECT_F {
                    left: x + margin + 4.0,
                    top: msg_y,
                    right: x + width - margin,
                    bottom: msg_y + 16.0,
                };
                target.DrawText(
                    &typing,
                    &small_format,
                    &typing_rect,
                    &yellow_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // 弹出消息区域裁剪
            target.PopAxisAlignedClip();

            // ===== "继续生成" 和 "重试" 按钮 =====
            self.ai.ai_panel.continue_button_region = None;
            self.ai.ai_panel.retry_button_region = None;

            // 检查是否需要显示重试按钮（有错误消息且不在生成中）
            let should_show_retry = !self.ai.ai_panel.is_generating
                && self
                    .ai
                    .ai_panel
                    .messages
                    .last()
                    .map(|m| {
                        m.role == crate::ai_panel::AiRole::Assistant
                            && (m.content.contains("[错误]")
                                || m.content.contains("[超时]")
                                || m.content.contains("[本地调用失败]")
                                || m.content.contains("[API 返回错误]"))
                    })
                    .unwrap_or(false);

            if self.ai.ai_panel.last_truncated && !self.ai.ai_panel.is_generating {
                let continue_btn_y = y + height - 78.0;
                let continue_btn_w = 90.0f32;
                let continue_btn_h = 26.0f32;
                let continue_btn_x = x + margin;
                let continue_btn_rect = D2D_RECT_F {
                    left: continue_btn_x,
                    top: continue_btn_y,
                    right: continue_btn_x + continue_btn_w,
                    bottom: continue_btn_y + continue_btn_h,
                };
                self.ai.ai_panel.continue_button_region = Some((
                    continue_btn_x,
                    continue_btn_y,
                    continue_btn_w,
                    continue_btn_h,
                ));
                let continue_bg_brush = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.95, 0.60, 0.0, 1.0))
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &continue_btn_rect, 5.0, &continue_bg_brush);
                let continue_text: Vec<u16> = "继续生成".encode_utf16().chain(Some(0)).collect();
                let continue_text_rect = D2D_RECT_F {
                    left: continue_btn_x,
                    top: continue_btn_y + 4.0,
                    right: continue_btn_x + continue_btn_w,
                    bottom: continue_btn_y + continue_btn_h - 2.0,
                };
                target.DrawText(
                    &continue_text,
                    &small_format,
                    &continue_text_rect,
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            } else if should_show_retry {
                // 显示重试按钮
                let retry_btn_y = y + height - 78.0;
                let retry_btn_w = 90.0f32;
                let retry_btn_h = 26.0f32;
                let retry_btn_x = x + margin;
                let retry_btn_rect = D2D_RECT_F {
                    left: retry_btn_x,
                    top: retry_btn_y,
                    right: retry_btn_x + retry_btn_w,
                    bottom: retry_btn_y + retry_btn_h,
                };
                self.ai.ai_panel.retry_button_region =
                    Some((retry_btn_x, retry_btn_y, retry_btn_w, retry_btn_h));
                let retry_bg_brush = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::accent())
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &retry_btn_rect, 5.0, &retry_bg_brush);
                let retry_text: Vec<u16> = "重试".encode_utf16().chain(Some(0)).collect();
                let retry_text_rect = D2D_RECT_F {
                    left: retry_btn_x,
                    top: retry_btn_y + 4.0,
                    right: retry_btn_x + retry_btn_w,
                    bottom: retry_btn_y + retry_btn_h - 2.0,
                };
                target.DrawText(
                    &retry_text,
                    &small_format,
                    &retry_text_rect,
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // ===== Apply 按钮区域 =====
            let has_code = self.ai.ai_panel.extract_last_code_block().is_some();
            if has_code && !self.ai.ai_panel.is_generating {
                let apply_y = y + height - 78.0;
                let apply_btn_w = 90.0f32;
                let apply_btn_h = 26.0f32;
                let apply_btn_x = x + width - margin - apply_btn_w;
                let apply_btn_rect = D2D_RECT_F {
                    left: apply_btn_x,
                    top: apply_y,
                    right: apply_btn_x + apply_btn_w,
                    bottom: apply_y + apply_btn_h,
                };
                let apply_bg_color = if self.ai.ai_panel.hover_apply_button {
                    app_theme::accent_hover()
                } else {
                    app_theme::accent()
                };
                let apply_bg_brush = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &apply_bg_color)
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &apply_btn_rect, 5.0, &apply_bg_brush);
                let apply_text: Vec<u16> = "应用代码".encode_utf16().chain(Some(0)).collect();
                let apply_text_rect = D2D_RECT_F {
                    left: apply_btn_x,
                    top: apply_y + 4.0,
                    right: apply_btn_x + apply_btn_w,
                    bottom: apply_y + apply_btn_h - 2.0,
                };
                target.DrawText(
                    &apply_text,
                    &small_format,
                    &apply_text_rect,
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // ===== 停止 / 复制 / 重新生成 按钮（浮层行，左侧） =====
            let act_y = y + height - 78.0;
            let act_h = 26.0f32;
            if self.ai.ai_panel.is_generating {
                let stop_w = 96.0f32;
                let stop_x = x + margin;
                let stop_rect = D2D_RECT_F {
                    left: stop_x,
                    top: act_y,
                    right: stop_x + stop_w,
                    bottom: act_y + act_h,
                };
                if let Ok(b) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::danger())
                {
                    fill_round_rect(target, &stop_rect, 5.0, &b);
                }
                let t: Vec<u16> = "■ 停止生成".encode_utf16().chain(Some(0)).collect();
                let tr = D2D_RECT_F {
                    left: stop_x,
                    top: act_y + 4.0,
                    right: stop_x + stop_w,
                    bottom: act_y + act_h - 2.0,
                };
                target.DrawText(
                    &t,
                    &small_format,
                    &tr,
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // ===== 变更列表 + Diff 预览已移除（Edit 模式删除，Agent 生成完成直接落盘） =====

            // ===== 输入框区域（新设计：参考图样式，支持自适应高度） =====
            let input_margin = 8.0f32;
            // 计算输入框文本高度（根据内容动态调整）
            let input_text = &self.ai.ai_panel.input;
            let text_input_width = width - margin * 2.0 - input_margin * 2.0 - 8.0; // 减去内边距
            let min_input_h = 36.0f32; // 最小输入框高度（两行）
            let max_input_h = 120.0f32; // 最大输入框高度（约6-7行）

            // 使用 DirectWrite 测量文本高度
            let text_input_h = if input_text.is_empty() {
                min_input_h
            } else {
                // 创建文本布局测量高度
                let wide: Vec<u16> = input_text.encode_utf16().collect();
                let dwrite = self.win.text_renderer.dwrite_factory();
                let msg_format = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .get_format(
                        11.0,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                        DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                    )
                    .unwrap();

                match dwrite.CreateTextLayout(&wide, &msg_format, text_input_width, 10000.0) {
                    Ok(layout) => {
                        let mut metrics =
                            windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                        if layout.GetMetrics(&mut metrics).is_ok() {
                            // 文本高度 + 上下内边距
                            (metrics.height + 16.0).clamp(min_input_h, max_input_h)
                        } else {
                            min_input_h
                        }
                    }
                    Err(_) => min_input_h,
                }
            };

            // 更新 AI 面板的输入框高度缓存
            self.ai.ai_panel.input_computed_height = text_input_h;

            // 输入区域总高度 = 文本输入高度 + 工具栏与间距(44) + 图片 chips 行（若有）
            let input_area_h = self.ai.ai_panel.input_area_height();
            let input_y = y + height - input_area_h;

            // 输入框卡片背景（圆角卡片）
            let card_rect = D2D_RECT_F {
                left: x + margin,
                top: input_y,
                right: x + width - margin,
                bottom: input_y + input_area_h,
            };
            fill_round_rect(target, &card_rect, 12.0, &input_bg_brush);

            // 聚焦环：微动效进度淡入淡出（聚焦状态变化时由渲染侧启动 UI_ANIM 定时器）
            let focused = self.ai.ai_panel.input_focused;
            if self.ui.hover_anim.ai_focus_seen != focused {
                self.ui.hover_anim.ai_focus_seen = focused;
                let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                    self.win.hwnd,
                    crate::window::UI_ANIM_TIMER_ID,
                    crate::window::UI_ANIM_INTERVAL_MS,
                    None,
                );
            }
            let f = crate::ui_anim::quantize(self.ui.hover_anim.ai_focus);
            let border_base = app_theme::border();
            let border_focus = app_theme::accent_border();
            let card_border_color = color_f(
                border_base.r + (border_focus.r - border_base.r) * f,
                border_base.g + (border_focus.g - border_base.g) * f,
                border_base.b + (border_focus.b - border_base.b) * f,
                1.0,
            );
            let card_border_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &card_border_color)
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let card_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: card_rect,
                radiusX: 12.0,
                radiusY: 12.0,
            };
            target.DrawRoundedRectangle(&card_rounded, &card_border_brush, 1.0, None);
            // 聚焦外发光：柔和光晕环强化焦点反馈
            if f > 0.0 {
                let mut glow_color = app_theme::accent_border();
                glow_color.a = 0.22 * f;
                if let Ok(glow) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &glow_color)
                {
                    let glow_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: card_rect.left - 2.0,
                            top: card_rect.top - 2.0,
                            right: card_rect.right + 2.0,
                            bottom: card_rect.bottom + 2.0,
                        },
                        radiusX: 14.0,
                        radiusY: 14.0,
                    };
                    target.DrawRoundedRectangle(&glow_rounded, &glow, 3.0, None);
                }
            }

            // 2. 中间输入区域（使用前面计算的动态高度）
            let text_input_y = input_y + 6.0;
            let text_input_rect = D2D_RECT_F {
                left: x + margin + input_margin,
                top: text_input_y,
                right: x + width - margin - input_margin,
                bottom: text_input_y + text_input_h,
            };

            // 占位提示仅在"无输入且无 IME 组合串"时显示，避免拼音输入阶段与提示叠字
            let composing = self
                .ai
                .ai_panel
                .composition
                .as_ref()
                .is_some_and(|c| !c.is_empty());

            // ===== 扩写动画渲染 =====
            let is_expand_anim = self.ai.ai_panel.is_expanding
                && self.ai.ai_panel.expand_anim_phase != crate::ai_panel::ExpandAnimPhase::None;

            if is_expand_anim {
                match self.ai.ai_panel.expand_anim_phase {
                    crate::ai_panel::ExpandAnimPhase::FadeOut => {
                        // 渐隐阶段：显示原文，透明度从 1.0 渐变为 0.0
                        let alpha = 1.0 - self.ai.ai_panel.expand_anim_progress;
                        let mut fade_color = app_theme::text_primary();
                        fade_color.a = alpha;
                        if let Ok(fade_brush) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &fade_color)
                        {
                            let orig_text = &self.ai.ai_panel.expand_original_text;
                            let orig_wide: Vec<u16> =
                                orig_text.encode_utf16().chain(Some(0)).collect();
                            let fade_rect = D2D_RECT_F {
                                left: text_input_rect.left + 4.0,
                                top: text_input_y + 8.0,
                                right: text_input_rect.right - 4.0,
                                bottom: text_input_y + text_input_h - 4.0,
                            };
                            target.DrawText(
                                &orig_wide,
                                &msg_format,
                                &fade_rect,
                                &fade_brush,
                                D2D1_DRAW_TEXT_OPTIONS_NONE,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                        }
                    }
                    crate::ai_panel::ExpandAnimPhase::Streaming => {
                        // 流式写入阶段：显示已接收的新文本，带轻微渐显效果
                        let new_text = &self.ai.ai_panel.input;
                        if !new_text.is_empty() {
                            // 新文本使用带轻微透明度的白色，营造"写入中"感
                            let mut stream_color = app_theme::text_primary();
                            stream_color.a = 0.92;
                            if let Ok(stream_brush) = self
                                .win
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &stream_color)
                            {
                                let new_wide: Vec<u16> =
                                    new_text.encode_utf16().chain(Some(0)).collect();
                                let stream_rect = D2D_RECT_F {
                                    left: text_input_rect.left + 4.0,
                                    top: text_input_y + 8.0,
                                    right: text_input_rect.right - 4.0,
                                    bottom: text_input_y + text_input_h - 4.0,
                                };
                                target.DrawText(
                                    &new_wide,
                                    &msg_format,
                                    &stream_rect,
                                    &stream_brush,
                                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                                    DWRITE_MEASURING_MODE_NATURAL,
                                );
                            }
                            // 流式写入中显示一个闪烁的写入指示器（竖线光标）
                            let tw = self
                                .win
                                .render_ctx
                                .text_format_cache
                                .measure_text_width(
                                    new_text,
                                    11.0,
                                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                                )
                                .unwrap_or(0.0);
                            let indicator_x = text_input_rect.left + 4.0 + tw;
                            // 闪烁效果：基于时间戳
                            let blink = (crate::ai_panel::now_millis() / 400) % 2 == 0;
                            if blink {
                                let mut indicator_color = app_theme::accent();
                                indicator_color.a = 0.8;
                                if let Ok(ind_brush) = self
                                    .win
                                    .render_ctx
                                    .brush_cache
                                    .get_brush(target, &indicator_color)
                                {
                                    let ind_rect = D2D_RECT_F {
                                        left: indicator_x,
                                        top: text_input_y + 10.0,
                                        right: indicator_x + 2.0,
                                        bottom: text_input_y + text_input_h - 10.0,
                                    };
                                    target.FillRectangle(&ind_rect, &ind_brush);
                                }
                            }
                        }
                    }
                    crate::ai_panel::ExpandAnimPhase::None => {}
                }
            } else {
                // ===== 正常输入框渲染 =====
                let show_placeholder = self.ai.ai_panel.input.is_empty() && !composing;
                let input_text = if show_placeholder {
                    "输入问题..."
                } else {
                    &self.ai.ai_panel.input
                };
                let input_color: &ID2D1SolidColorBrush = if show_placeholder {
                    &dim_brush
                } else {
                    text_brush
                };
                let input_wide: Vec<u16> = input_text.encode_utf16().chain(Some(0)).collect();
                let input_text_rect = D2D_RECT_F {
                    left: text_input_rect.left + 4.0,
                    top: text_input_y + 8.0,
                    right: text_input_rect.right - 4.0,
                    bottom: text_input_y + text_input_h - 4.0,
                };
                target.DrawText(
                    &input_wide,
                    &msg_format,
                    &input_text_rect,
                    input_color,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // IME 合成串（pre-edit text）显示在光标位置之后
            if let Some(comp) = &self.ai.ai_panel.composition {
                if !comp.is_empty() {
                    let comp_text: Vec<u16> = comp.encode_utf16().collect();
                    // 合成串定位到光标处（光标前文本宽度），而非整段输入末尾
                    let caret_prefix = if self.ai.ai_panel.caret_pos <= self.ai.ai_panel.input.len()
                    {
                        &self.ai.ai_panel.input[..self.ai.ai_panel.caret_pos]
                    } else {
                        self.ai.ai_panel.input.as_str()
                    };
                    let input_width = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .measure_text_width(caret_prefix, 11.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                        .unwrap_or(0.0);
                    let comp_x = text_input_rect.left + 4.0 + input_width;
                    let comp_rect = D2D_RECT_F {
                        left: comp_x,
                        top: text_input_y + 8.0,
                        right: text_input_rect.right - 4.0,
                        bottom: text_input_y + text_input_h - 4.0,
                    };
                    let comp_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::warning())
                        .unwrap();
                    target.DrawText(
                        &comp_text,
                        &msg_format,
                        &comp_rect,
                        &comp_brush,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    let comp_width = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .measure_text_width(comp, 11.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                        .unwrap_or(0.0);
                    let underline_rect = D2D_RECT_F {
                        left: comp_x,
                        top: text_input_y + text_input_h - 10.0,
                        right: comp_x + comp_width,
                        bottom: text_input_y + text_input_h - 9.0,
                    };
                    target.FillRectangle(&underline_rect, &comp_brush);
                }
            }

            // 输入框光标（聚焦且 caret_visible 时闪烁）
            if self.ai.ai_panel.input_focused && self.ai.ai_panel.caret_visible {
                // 根据 caret_pos 计算光标前正文宽度
                let text_before_caret =
                    if self.ai.ai_panel.caret_pos <= self.ai.ai_panel.input.len() {
                        &self.ai.ai_panel.input[..self.ai.ai_panel.caret_pos]
                    } else {
                        &self.ai.ai_panel.input
                    };
                let tw = if text_before_caret.is_empty() {
                    0.0
                } else {
                    self.win
                        .render_ctx
                        .text_format_cache
                        .measure_text_width(
                            text_before_caret,
                            11.0,
                            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        )
                        .unwrap_or(0.0)
                };
                // IME 组合中：光标应位于预输入拼音之后，而非其前
                let comp_w = self
                    .ai
                    .ai_panel
                    .composition
                    .as_ref()
                    .filter(|c| !c.is_empty())
                    .map(|c| {
                        self.win
                            .render_ctx
                            .text_format_cache
                            .measure_text_width(c, 11.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                            .unwrap_or(0.0)
                    })
                    .unwrap_or(0.0);
                let caret_x = text_input_rect.left + 4.0 + tw + comp_w;
                let caret_rect = D2D_RECT_F {
                    left: caret_x,
                    top: text_input_y + 10.0,
                    right: caret_x + 1.5,
                    bottom: text_input_y + text_input_h - 10.0,
                };
                target.FillRectangle(&caret_rect, text_brush);
            }

            // 3. 底部分隔线
            let toolbar_sep_y = input_y + input_area_h - 34.0;
            let toolbar_sep = D2D_RECT_F {
                left: x + margin + input_margin,
                top: toolbar_sep_y,
                right: x + width - margin - input_margin,
                bottom: toolbar_sep_y + 1.0,
            };
            target.FillRectangle(&toolbar_sep, &sep_brush);

            // 3.5 待发送图片 chips 行（多模态：附加的图片以文件名胶囊展示，点击移除）
            self.ai.ai_panel.image_chip_regions.clear();
            if !self.ai.ai_panel.pending_images.is_empty() {
                let chips_top = input_y + 6.0 + text_input_h + 4.0;
                let chip_h = 24.0f32;
                let mut chip_x = x + margin + input_margin;
                let max_chip_x = x + width - margin - input_margin;
                for (i, img) in self.ai.ai_panel.pending_images.iter().enumerate() {
                    // 文件名过长时截断展示
                    let display_name: String = if img.filename.chars().count() > 18 {
                        let mut s: String = img.filename.chars().take(17).collect();
                        s.push('…');
                        s
                    } else {
                        img.filename.clone()
                    };
                    let chip_w =
                        ((display_name.chars().count() as f32) * 6.5 + 42.0).clamp(64.0, 180.0);
                    if chip_x + chip_w > max_chip_x {
                        break; // 横向空间不足，余下图片仍保留在待发列表
                    }
                    let hovered = self.ai.ai_panel.hover_image_chip == Some(i);
                    let chip_bg = if hovered {
                        app_theme::accent_soft_hover()
                    } else {
                        app_theme::surface_alt()
                    };
                    if let Ok(cb) = self.win.render_ctx.brush_cache.get_brush(target, &chip_bg) {
                        fill_round_rect(
                            target,
                            &D2D_RECT_F {
                                left: chip_x,
                                top: chips_top,
                                right: chip_x + chip_w,
                                bottom: chips_top + chip_h,
                            },
                            4.0,
                            &cb,
                        );
                    }
                    if let Ok(bb) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::border_strong())
                    {
                        target.DrawRectangle(
                            &D2D_RECT_F {
                                left: chip_x,
                                top: chips_top,
                                right: chip_x + chip_w,
                                bottom: chips_top + chip_h,
                            },
                            &bb,
                            1.0,
                            None,
                        );
                    }
                    // 图片小图标 + 文件名 + 移除标记
                    self.ui.icons.draw(
                        target,
                        crate::icons::IconKind::Image,
                        chip_x + 6.0,
                        chips_top + 5.0,
                        14.0,
                        14.0,
                        &dim_brush,
                    );
                    let chip_text: Vec<u16> = format!("{}  ✕", display_name)
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    target.DrawText(
                        &chip_text,
                        &small_format,
                        &D2D_RECT_F {
                            left: chip_x + 24.0,
                            top: chips_top + 4.0,
                            right: chip_x + chip_w - 4.0,
                            bottom: chips_top + chip_h,
                        },
                        &dim_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    // 命中区以面板相对坐标注册（点击/悬停用 rp_rel 比对）
                    self.ai.ai_panel.image_chip_regions.push((
                        i,
                        chip_x - x,
                        chips_top - y,
                        chip_w,
                        chip_h,
                    ));
                    chip_x += chip_w + 6.0;
                }
            }

            // 4. 底部工具栏
            let toolbar_y = toolbar_sep_y + 4.0;
            let toolbar_h = 26.0f32;

            // 左侧：模型选择下拉按钮（宽度随模型名自适应，悬停微动效提亮）
            let model_label = self.ui.app_settings.active_model_display_name();
            let label_w = self
                .win
                .render_ctx
                .text_format_cache
                .measure_text_width(&model_label, 10.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                .unwrap_or(60.0);
            let model_btn_w = (label_w + 34.0).clamp(96.0, 200.0);
            let model_btn_x = x + margin + input_margin;
            let model_btn_rect = D2D_RECT_F {
                left: model_btn_x,
                top: toolbar_y,
                right: model_btn_x + model_btn_w,
                bottom: toolbar_y + toolbar_h,
            };
            let pm = crate::ui_anim::quantize(self.ui.hover_anim.ai_btn_progress(1));
            let model_base = app_theme::surface_alt();
            let model_hover = app_theme::accent_soft_hover();
            let model_bg = color_f(
                model_base.r + (model_hover.r - model_base.r) * pm,
                model_base.g + (model_hover.g - model_base.g) * pm,
                model_base.b + (model_hover.b - model_base.b) * pm,
                1.0,
            );
            if let Ok(mb) = self.win.render_ctx.brush_cache.get_brush(target, &model_bg) {
                fill_round_rect(target, &model_btn_rect, 8.0, &mb);
            }
            let model_text: Vec<u16> = format!("{} ▼", model_label)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let model_text_rect = D2D_RECT_F {
                left: model_btn_x + 8.0,
                top: toolbar_y + 4.0,
                right: model_btn_x + model_btn_w - 6.0,
                bottom: toolbar_y + toolbar_h - 2.0,
            };
            let model_text_base = app_theme::text_secondary();
            let model_text_hover = app_theme::accent_text();
            let model_fg = color_f(
                model_text_base.r + (model_text_hover.r - model_text_base.r) * pm,
                model_text_base.g + (model_text_hover.g - model_text_base.g) * pm,
                model_text_base.b + (model_text_hover.b - model_text_base.b) * pm,
                1.0,
            );
            if let Ok(mf) = self.win.render_ctx.brush_cache.get_brush(target, &model_fg) {
                target.DrawText(
                    &model_text,
                    &small_format,
                    &model_text_rect,
                    &mf,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            self.ai.ai_panel.model_btn_region =
                Some((model_btn_x, toolbar_y, model_btn_w, toolbar_h));

            // 当前模型下拉弹层（点击模型按钮展开，向上弹出，列出所有已启用模型）
            if self.ai.ai_panel.model_menu_open {
                let models: Vec<(String, String, bool)> = self
                    .ui
                    .app_settings
                    .ai_models
                    .iter()
                    .filter(|m| m.enabled)
                    .map(|m| {
                        let label = if !m.display_name.is_empty() {
                            m.display_name.clone()
                        } else if !m.settings.model.is_empty() {
                            m.settings.model.clone()
                        } else {
                            "(未命名模型)".to_string()
                        };
                        let is_active =
                            self.ui.app_settings.active_model_id.as_deref() == Some(m.id.as_str());
                        (m.id.clone(), label, is_active)
                    })
                    .collect();
                if !models.is_empty() {
                    let item_h = 30.0f32;
                    let menu_w = model_btn_w.max(200.0);
                    let menu_x = model_btn_x;
                    let menu_bottom = toolbar_y - 4.0;
                    let menu_h = models.len() as f32 * item_h + 8.0;
                    let menu_top = menu_bottom - menu_h;
                    // 弹层背景 + 边框
                    let menu_bg = app_theme::surface();
                    if let Ok(menu_bg_brush) =
                        self.win.render_ctx.brush_cache.get_brush(target, &menu_bg)
                    {
                        fill_round_rect(
                            target,
                            &D2D_RECT_F {
                                left: menu_x,
                                top: menu_top,
                                right: menu_x + menu_w,
                                bottom: menu_bottom,
                            },
                            6.0,
                            &menu_bg_brush,
                        );
                    }
                    let menu_border = app_theme::border_strong();
                    if let Ok(menu_border_brush) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &menu_border)
                    {
                        target.DrawRectangle(
                            &D2D_RECT_F {
                                left: menu_x,
                                top: menu_top,
                                right: menu_x + menu_w,
                                bottom: menu_bottom,
                            },
                            &menu_border_brush,
                            1.0,
                            None,
                        );
                    }
                    let sel_bg = app_theme::accent_soft_hover();
                    let sel_bg_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &sel_bg)
                        .ok();
                    for (i, (_id, label, is_active)) in models.iter().enumerate() {
                        let iy = menu_top + 4.0 + i as f32 * item_h;
                        if *is_active {
                            if let Some(b) = &sel_bg_brush {
                                fill_round_rect(
                                    target,
                                    &D2D_RECT_F {
                                        left: menu_x + 2.0,
                                        top: iy,
                                        right: menu_x + menu_w - 2.0,
                                        bottom: iy + item_h,
                                    },
                                    4.0,
                                    b,
                                );
                            }
                        }
                        let item_str = if *is_active {
                            format!("● {}", label)
                        } else {
                            format!("    {}", label)
                        };
                        let item_wide: Vec<u16> = item_str.encode_utf16().chain(Some(0)).collect();
                        let active_item_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &app_theme::accent_text())
                            .unwrap();
                        target.DrawText(
                            &item_wide,
                            &small_format,
                            &D2D_RECT_F {
                                left: menu_x + 10.0,
                                top: iy + 6.0,
                                right: menu_x + menu_w - 10.0,
                                bottom: iy + item_h,
                            },
                            if *is_active {
                                &active_item_brush
                            } else {
                                text_brush
                            },
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                }
            }

            // 右侧功能按钮区域
            let right_btn_area_x = x + width - margin - input_margin;

            // 发送/中断按钮（根据生成状态切换；空输入时禁用态降低误导）
            let send_btn_size = 26.0f32;
            let send_btn_x = right_btn_area_x - send_btn_size;
            let send_btn_y = toolbar_y;
            let send_btn_rect = D2D_RECT_F {
                left: send_btn_x,
                top: send_btn_y,
                right: send_btn_x + send_btn_size,
                bottom: send_btn_y + send_btn_size,
            };

            // 根据是否正在生成切换按钮样式；悬停经微动效提亮
            let is_generating = self.ai.ai_panel.is_generating;
            let send_enabled = is_generating || !self.ai.ai_panel.input.trim().is_empty();
            let ps = crate::ui_anim::quantize(self.ui.hover_anim.ai_btn_progress(2));
            let smix = |a: f32, b: f32| a + (b - a) * ps;
            let danger = app_theme::danger();
            let danger_hover = color_f(0.68, 0.18, 0.18, 1.0);
            let accent = app_theme::accent();
            let accent_hover = app_theme::accent_hover();
            let (btn_bg, icon_kind) = if is_generating {
                (
                    color_f(
                        smix(danger.r, danger_hover.r),
                        smix(danger.g, danger_hover.g),
                        smix(danger.b, danger_hover.b),
                        1.0,
                    ),
                    crate::icons::IconKind::Close,
                )
            } else if send_enabled {
                (
                    color_f(
                        smix(accent.r, accent_hover.r),
                        smix(accent.g, accent_hover.g),
                        smix(accent.b, accent_hover.b),
                        1.0,
                    ),
                    crate::icons::IconKind::Send,
                )
            } else {
                (app_theme::surface_alt(), crate::icons::IconKind::Send)
            };

            let send_bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &btn_bg) {
                Ok(b) => b,
                Err(_) => return,
            };
            fill_round_rect(target, &send_btn_rect, 9.0, &send_bg_brush);
            // 使用 SVG 图标绘制按钮（禁用态图标同步降亮度）
            let send_icon_brush = if send_enabled { white_brush } else { dim_brush };
            self.ui.icons.draw(
                target,
                icon_kind,
                send_btn_x + 3.0,
                send_btn_y + 3.0,
                send_btn_size - 6.0,
                send_btn_size - 6.0,
                &send_icon_brush,
            );
            self.ai.ai_panel.send_btn_region =
                Some((send_btn_x, send_btn_y, send_btn_size, send_btn_size));

            // 工具栏中部状态 chip（人体工程学：外围视野、紧凑扁平、不侵占聊天区，
            // 窄面板自动隐藏）。流水线运行时显示任务进度，否则显示上下文占用。
            self.ai.ai_panel.context_chip_region = None;
            let pipeline_progress = self
                .ai
                .ai_panel
                .agent_pipeline
                .as_ref()
                .map(|p| p.progress());
            let chip_x = model_btn_x + model_btn_w + 8.0;
            let right_limit = send_btn_x - 8.0;
            if let Some((done, failed, total, cursor)) = pipeline_progress {
                // ===== 任务进度 chip =====
                let cur = (cursor + 1).min(total).max(1);
                let main_label = format!("任务 {}/{}", cur, total);
                let suffix = if failed > 0 {
                    format!("  ✕{}", failed)
                } else {
                    String::new()
                };
                let main_w = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(&main_label, 10.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                    .unwrap_or(50.0);
                let suffix_w = if suffix.is_empty() {
                    0.0
                } else {
                    self.win
                        .render_ctx
                        .text_format_cache
                        .measure_text_width(&suffix, 10.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                        .unwrap_or(20.0)
                };
                let bar_w = 30.0f32;
                let chip_w = 8.0 + bar_w + 6.0 + main_w + suffix_w + 8.0;
                if chip_x + chip_w <= right_limit {
                    let chip_rect = D2D_RECT_F {
                        left: chip_x,
                        top: toolbar_y,
                        right: chip_x + chip_w,
                        bottom: toolbar_y + toolbar_h,
                    };
                    if let Ok(bg) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::surface_alt())
                    {
                        fill_round_rect(target, &chip_rect, 8.0, &bg);
                    }
                    // 迷你进度条：已完成（含失败）占比，强调色填充
                    let bar_x = chip_x + 8.0;
                    let bar_y = toolbar_y + toolbar_h / 2.0 - 2.5;
                    if let Ok(bb) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::track_bg())
                    {
                        let r = D2D_RECT_F {
                            left: bar_x,
                            top: bar_y,
                            right: bar_x + bar_w,
                            bottom: bar_y + 5.0,
                        };
                        fill_round_rect(target, &r, 2.5, &bb);
                    }
                    let frac = if total > 0 {
                        (done + failed) as f32 / total as f32
                    } else {
                        0.0
                    };
                    let fill_w = bar_w * frac.clamp(0.0, 1.0);
                    if fill_w > 0.5 {
                        if let Ok(fb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &app_theme::accent())
                        {
                            let r = D2D_RECT_F {
                                left: bar_x,
                                top: bar_y,
                                right: bar_x + fill_w.max(3.0),
                                bottom: bar_y + 5.0,
                            };
                            fill_round_rect(target, &r, 2.5, &fb);
                        }
                    }
                    // 主标签（灰）+ 失败计数（柔和红）
                    let text_left = bar_x + bar_w + 6.0;
                    let main_wide: Vec<u16> = main_label.encode_utf16().chain(Some(0)).collect();
                    if let Ok(fg) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::text_secondary())
                    {
                        target.DrawText(
                            &main_wide,
                            &small_format,
                            &D2D_RECT_F {
                                left: text_left,
                                top: toolbar_y + 4.0,
                                right: text_left + main_w,
                                bottom: toolbar_y + toolbar_h - 2.0,
                            },
                            &fg,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    if !suffix.is_empty() {
                        let suf_wide: Vec<u16> = suffix.encode_utf16().chain(Some(0)).collect();
                        if let Ok(fr) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &app_theme::danger())
                        {
                            target.DrawText(
                                &suf_wide,
                                &small_format,
                                &D2D_RECT_F {
                                    left: text_left + main_w,
                                    top: toolbar_y + 4.0,
                                    right: text_left + main_w + suffix_w,
                                    bottom: toolbar_y + toolbar_h - 2.0,
                                },
                                &fr,
                                D2D1_DRAW_TEXT_OPTIONS_NONE,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                        }
                    }
                    self.ai.ai_panel.context_chip_region =
                        Some((chip_x, toolbar_y, chip_w, toolbar_h));
                }
            } else {
                // ===== 上下文占用 chip（P1）=====
                let budget = self
                    .ui
                    .app_settings
                    .active_ai_settings()
                    .max_input_tokens
                    .map(|v| v as usize)
                    .unwrap_or(24000);
                let (_used, _budget, ratio) = self.ai.ai_panel.context_usage(budget);
                let has_memory = !self.ai.ai_panel.context_memory.trim().is_empty();
                let pct = (ratio * 100.0).round() as u32;
                let label = if has_memory {
                    format!("上下文 {}% · 记忆", pct)
                } else {
                    format!("上下文 {}%", pct)
                };
                let text_w = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(&label, 10.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                    .unwrap_or(60.0);
                let chip_w = text_w + 24.0;
                // 响应式：中部空位不足时不渲染，避免与发送按钮拥挤
                if chip_x + chip_w <= right_limit {
                    let chip_rect = D2D_RECT_F {
                        left: chip_x,
                        top: toolbar_y,
                        right: chip_x + chip_w,
                        bottom: toolbar_y + toolbar_h,
                    };
                    if let Ok(bg) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::surface_alt())
                    {
                        fill_round_rect(target, &chip_rect, 8.0, &bg);
                    }
                    // 占用分级柔和色：<60% 绿、<85% 琥珀、否则红
                    let dot_color = if ratio < 0.60 {
                        color_f(0.32, 0.72, 0.48, 1.0)
                    } else if ratio < 0.85 {
                        color_f(0.86, 0.66, 0.28, 1.0)
                    } else {
                        app_theme::danger()
                    };
                    if let Ok(db) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &dot_color)
                    {
                        let dot = D2D_RECT_F {
                            left: chip_x + 8.0,
                            top: toolbar_y + toolbar_h / 2.0 - 3.0,
                            right: chip_x + 14.0,
                            bottom: toolbar_y + toolbar_h / 2.0 + 3.0,
                        };
                        fill_round_rect(target, &dot, 3.0, &db);
                    }
                    let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                    if let Ok(fg) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::text_secondary())
                    {
                        target.DrawText(
                            &label_wide,
                            &small_format,
                            &D2D_RECT_F {
                                left: chip_x + 18.0,
                                top: toolbar_y + 4.0,
                                right: chip_x + chip_w - 6.0,
                                bottom: toolbar_y + toolbar_h - 2.0,
                            },
                            &fg,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    self.ai.ai_panel.context_chip_region =
                        Some((chip_x, toolbar_y, chip_w, toolbar_h));
                }
            }

            // 快捷按钮（星星）——问题扩写（已移除，由 AI 自动决策）
            // 附加图片按钮（多模态）——已移除，由 AI 自动决策
            self.ai.ai_panel.image_button_region = None;
        }
    }
}

/// AI 面板消息渲染项：文本/代码段，或 AI 文件/命令操作卡片。
enum AiRenderItem {
    Seg {
        is_code: bool,
        text: String,
        /// 代码块语言标识（从 ``` 围栏提取）
        lang: Option<String>,
    },
    File {
        kind: crate::ai_panel::FileOpKind,
        path: String,
        content: String,
        /// 修改前旧内容（search 段），供差异可视化与行数统计
        old: String,
    },
    Run {
        cmd: String,
    },
    Read {
        path: String,
    },
    List {
        path: String,
    },
    /// 符号查找卡片（轻量符号索引：按名称定位函数/变量/类型定义）
    Symbol {
        name: String,
    },
    /// 内置浏览器打开卡片（AETHER_OPEN：把网页呈现给用户，不回喂内容）
    Open {
        query: String,
    },
    /// 内置浏览器正文读取卡片（AETHER_READPAGE：提取网页内容回喂 AI）
    ReadPage {
        url: String,
    },
    /// 未闭合的文件块（流式截断）——渲染为"未落盘"警告卡片
    Incomplete {
        path: String,
    },
    /// 询问卡片：需求不清时 AI 提问 + 选项（用户可点选或自定义回答）
    Ask {
        question: String,
        options: Vec<String>,
    },
    /// 流式生成中的文件块（未闭合但仍在生成）——渲染为进行中卡片
    Generating {
        path: String,
    },
}

/// 圆角矩形填充（AI 面板通用绘制辅助，统一面板圆角视觉语言）。
fn fill_round_rect(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    rect: &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F,
    radius: f32,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
        rect: *rect,
        radiusX: radius,
        radiusY: radius,
    };
    unsafe { target.FillRoundedRectangle(&rounded, brush) };
}

/// 返回操作卡片的展示要素：(图标, 类型标签, 详情文本, 主题色)。
fn agent_op_display(
    item: &AiRenderItem,
) -> (
    &'static str,
    &'static str,
    String,
    windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
) {
    match item {
        AiRenderItem::File { kind, path, .. } => {
            let (glyph, label, color) = match kind {
                crate::ai_panel::FileOpKind::Create => {
                    ("+", "新建", color_f(0.40, 0.80, 0.52, 1.0))
                }
                crate::ai_panel::FileOpKind::Modify => ("~", "修改", color_f(0.40, 0.70, 1.0, 1.0)),
                crate::ai_panel::FileOpKind::Delete => {
                    ("-", "删除", color_f(0.92, 0.52, 0.52, 1.0))
                }
            };
            (glyph, label, path.clone(), color)
        }
        AiRenderItem::Run { cmd } => ("▶", "运行命令", cmd.clone(), color_f(0.70, 0.62, 1.0, 1.0)),
        AiRenderItem::Read { path } => (
            "◎",
            "读取文件",
            path.clone(),
            color_f(0.55, 0.78, 0.85, 1.0),
        ),
        AiRenderItem::List { path } => (
            "◇",
            "列出目录",
            path.clone(),
            color_f(0.60, 0.72, 0.88, 1.0),
        ),
        AiRenderItem::Symbol { name } => (
            "✦",
            "查找符号",
            name.clone(),
            color_f(0.98, 0.75, 0.43, 1.0),
        ),
        AiRenderItem::Open { query } => (
            "◍",
            "打开网页",
            query.clone(),
            color_f(0.45, 0.72, 0.95, 1.0),
        ),
        AiRenderItem::ReadPage { url } => {
            ("◉", "读取网页", url.clone(), color_f(0.40, 0.82, 0.78, 1.0))
        }
        AiRenderItem::Incomplete { path } => (
            "!",
            "生成中断",
            format!("{} 未写入磁盘", path),
            color_f(0.90, 0.60, 0.20, 1.0),
        ),
        AiRenderItem::Generating { path } => {
            ("…", "正在生成", path.clone(), color_f(0.40, 0.70, 1.0, 1.0))
        }
        AiRenderItem::Seg { .. } => ("", "", String::new(), color_f(0.5, 0.5, 0.5, 1.0)),
        // 询问卡片有专属绘制分支，不会走到此分支
        AiRenderItem::Ask { .. } => ("?", "提问", String::new(), color_f(0.62, 0.55, 0.85, 1.0)),
    }
}
