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
            self.win
                .render_ctx
                .fill_rect(x, y, width, height, &self.win.theme.editor_bg);

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
                .get_brush(target, &color_f(0.9, 0.9, 0.9, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let dim_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.52, 0.56, 0.62, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let user_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.13, 0.19, 0.28, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let assistant_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.16, 0.17, 0.20, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let tool_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.13, 0.14, 0.16, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let input_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.11, 0.12, 0.14, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let sep_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.22, 0.24, 0.28, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let _accent_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.0, 0.47, 0.83, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let _green_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.30, 0.78, 0.42, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let yellow_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.9, 0.7, 0.2, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let code_bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.08, 0.08, 0.09, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let code_text_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.85, 0.85, 0.85, 1.0))
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

            // ===== 对话标签条（多会话）：仅开发者模式渲染，智能体模式移至左侧边栏 =====
            if !self.editor_mode.is_agent() {
                let tab_h = 24.0f32;
                let tab_y = cy;
                let gap = 4.0f32;
                let tab_w = 92.0f32;
                let close_w = 16.0f32;
                // 右侧预留：加号图标 + 历史图标（各 22px + 2px 间距）
                let right_reserve = 22.0f32 * 2.0 + 2.0;
                let strip_right = x + width - margin - right_reserve;
                let mut tx = x + margin;
                let n = self.ai.ai_panel.conversations.len();
                for i in 0..n {
                    if tx + tab_w > strip_right {
                        break; // 溢出裁剪：其余会话经"历史"访问
                    }
                    let is_active = i == self.ai.ai_panel.active;
                    let generating = self.ai.ai_panel.conv_is_generating(i);
                    let title = self.ai.ai_panel.conv_title(i).to_string();
                    let tab_rect = D2D_RECT_F {
                        left: tx,
                        top: tab_y,
                        right: tx + tab_w,
                        bottom: tab_y + tab_h,
                    };
                    let bg = if is_active {
                        color_f(0.18, 0.30, 0.48, 1.0)
                    } else if !self.ai.ai_panel.history_open
                        && self.ai.ai_panel.hover_tab == Some(i)
                    {
                        color_f(0.22, 0.24, 0.29, 1.0)
                    } else {
                        color_f(0.16, 0.17, 0.20, 1.0)
                    };
                    if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
                        fill_round_rect(target, &tab_rect, 5.0, &b);
                    }
                    // 激活会话标签：底部 2px 强调条，明确当前会话归属
                    if is_active {
                        if let Ok(ab) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &color_f(0.30, 0.62, 1.0, 1.0))
                        {
                            target.FillRectangle(
                                &D2D_RECT_F {
                                    left: tx + 7.0,
                                    top: tab_y + tab_h - 2.0,
                                    right: tx + tab_w - 7.0,
                                    bottom: tab_y + tab_h,
                                },
                                &ab,
                            );
                        }
                    }
                    let mut title_left = tx + 8.0;
                    if generating {
                        if let Ok(gb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &color_f(0.95, 0.75, 0.2, 1.0))
                        {
                            let dot = D2D_RECT_F {
                                left: tx + 6.0,
                                top: tab_y + tab_h / 2.0 - 3.0,
                                right: tx + 12.0,
                                bottom: tab_y + tab_h / 2.0 + 3.0,
                            };
                            target.FillRectangle(&dot, &gb);
                        }
                        title_left = tx + 16.0;
                    }
                    let title_rect = D2D_RECT_F {
                        left: title_left,
                        top: tab_y + 3.0,
                        right: tx + tab_w - close_w - 2.0,
                        bottom: tab_y + tab_h - 2.0,
                    };
                    // 显示层缩写：标题超出可用宽度时截断加 "…"，避免溢出到关闭按钮
                    let title = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .truncate_with_ellipsis(
                            &title,
                            10.0,
                            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            title_rect.right - title_rect.left,
                        );
                    let tw: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                    let tcol: &ID2D1SolidColorBrush =
                        if is_active { &white_brush } else { &dim_brush };
                    target.DrawText(
                        &tw,
                        &small_format,
                        &title_rect,
                        tcol,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    let close_x = tx + tab_w - close_w;
                    let close_rect = D2D_RECT_F {
                        left: close_x,
                        top: tab_y + 3.0,
                        right: tx + tab_w - 2.0,
                        bottom: tab_y + tab_h - 2.0,
                    };
                    let xw: Vec<u16> = "×".encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &xw,
                        &small_format,
                        &close_rect,
                        &dim_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    self.ai
                        .ai_panel
                        .tab_regions
                        .push((i, tx, tab_y, tab_w - close_w, tab_h));
                    self.ai
                        .ai_panel
                        .tab_close_regions
                        .push((i, close_x, tab_y, close_w, tab_h));
                    tx += tab_w + gap;
                }
                // 右侧图标区域：竖线分隔 + Plus 图标 + Clock 图标（靠右对齐）
                let icon_btn = 22.0f32;
                let icon_gap = 4.0f32;
                let icon_size = 14.0f32;
                let icon_pad = (icon_btn - icon_size) / 2.0;
                let icons_right = x + width - margin;
                let icons_w = icon_btn * 2.0 + icon_gap;
                let icons_left = icons_right - icons_w;

                // 竖线分隔（图标区与标签区之间）
                let sep_x = icons_left - 6.0;
                let sep_line = D2D_RECT_F {
                    left: sep_x,
                    top: tab_y + 4.0,
                    right: sep_x + 1.0,
                    bottom: tab_y + tab_h - 4.0,
                };
                target.FillRectangle(&sep_line, &sep_brush);

                // Plus 图标（新建对话）
                let plus_x = icons_left;
                self.ui.icons.draw(
                    target,
                    crate::icons::IconKind::Plus,
                    plus_x + icon_pad,
                    tab_y + (tab_h - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                    &dim_brush,
                );
                self.ai.ai_panel.new_tab_region = Some((plus_x, tab_y, icon_btn, tab_h));

                // Clock 图标（历史记录）
                let hb_x = icons_left + icon_btn + icon_gap;
                self.ui.icons.draw(
                    target,
                    crate::icons::IconKind::Clock,
                    hb_x + icon_pad,
                    tab_y + (tab_h - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                    &dim_brush,
                );
                self.ai.ai_panel.history_button_region = Some((hb_x, tab_y, icon_btn, tab_h));
                crate::hit_test::register_hit_region(
                    "ai:history_button",
                    hb_x,
                    tab_y,
                    icon_btn,
                    tab_h,
                );

                cy += tab_h + 8.0;
                let sep2 = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + width - margin,
                    bottom: cy + 1.0,
                };
                target.FillRectangle(&sep2, &sep_brush);
                cy += 8.0;
            }

            // ===== 策略库已迁移到设置面板，此处不再渲染 =====

            // ===== 欢迎页/空工作区提示 =====
            let has_workspace =
                self.fs.current_folder.is_some() || self.editor.content.file_path.is_some();
            if !has_workspace {
                let hint_bg_color = color_f(0.15, 0.15, 0.17, 1.0);
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
                    .get_brush(target, &color_f(0.0, 0.47, 0.83, 1.0))
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
                                        .get_brush(target, &color_f(0.14, 0.13, 0.18, 1.0))
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
                                        .get_brush(target, &color_f(0.66, 0.68, 0.74, 1.0))
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
                            for line in t.lines() {
                                if line.trim_start().starts_with("```") {
                                    if !buf.is_empty() {
                                        render_items.push(AiRenderItem::Seg {
                                            is_code: in_code,
                                            text: buf.join("\n"),
                                        });
                                        buf.clear();
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
                    let (is_code, seg_text) = match item {
                        AiRenderItem::Seg { is_code, text } => (is_code, text),
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
                                        .get_brush(target, &color_f(0.14, 0.15, 0.19, 1.0))
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
                                        .get_brush(target, &color_f(0.62, 0.72, 0.64, 1.0))
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
                                    .get_brush(target, &color_f(0.15, 0.15, 0.20, 1.0))
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
                                    .get_brush(target, &color_f(0.88, 0.88, 0.92, 1.0))
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
                                        color_f(0.28, 0.38, 0.58, 1.0)
                                    } else {
                                        color_f(0.19, 0.20, 0.26, 1.0)
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
                                        color_f(0.95, 0.97, 1.0, 1.0)
                                    } else {
                                        color_f(0.78, 0.80, 0.85, 1.0)
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
                                    color_f(0.28, 0.38, 0.58, 1.0)
                                } else {
                                    color_f(0.16, 0.17, 0.21, 1.0)
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
                                    color_f(0.95, 0.97, 1.0, 1.0)
                                } else {
                                    color_f(0.62, 0.64, 0.70, 1.0)
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
                                    .get_brush(target, &color_f(0.16, 0.17, 0.20, 1.0))
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
                                    .get_brush(target, &color_f(0.72, 0.76, 0.82, 1.0))
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
                                    .get_brush(target, &color_f(0.85, 0.87, 0.90, 1.0))
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
                                        .get_brush(target, &color_f(0.60, 0.62, 0.66, 1.0))
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
                                                            &color_f(0.55, 0.57, 0.60, 1.0),
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
                                                        (" ", color_f(0.62, 0.64, 0.68, 1.0), None)
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
                                                .get_brush(target, &color_f(0.72, 0.76, 0.70, 1.0))
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
                                    .get_brush(target, &color_f(0.16, 0.17, 0.20, 1.0))
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
                                    .get_brush(target, &color_f(0.78, 0.80, 0.84, 1.0))
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
                    } else {
                        (content_right - content_left - seg_pad * 2.0).max(20.0)
                    };

                    // 普通段解析轻量 Markdown；代码段保持原文
                    #[allow(clippy::type_complexity)]
                    let (layout_wide, bolds, headings, codes): (
                        Vec<u16>,
                        Vec<(u32, u32)>,
                        Vec<(u32, u32, f32)>,
                        Vec<(u32, u32)>,
                    ) = if *is_code {
                        (
                            seg_text.encode_utf16().collect(),
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                        )
                    } else {
                        crate::ai_panel::parse_markdown_segment(seg_text)
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
                        fill_round_rect(target, &seg_rect, 6.0, seg_bg);

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
                        let save_bg = color_f(0.2, 0.5, 0.3, 1.0);
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
                            .get_brush(target, &color_f(0.45, 0.72, 1.0, 0.9))
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
                    .get_brush(target, &color_f(0.45, 0.47, 0.53, 0.80))
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
                    .get_brush(target, &color_f(0.20, 0.60, 0.86, 1.0))
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
                    color_f(0.0, 0.55, 0.95, 1.0)
                } else {
                    color_f(0.0, 0.47, 0.83, 1.0)
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
                    .get_brush(target, &color_f(0.62, 0.24, 0.24, 1.0))
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
            fill_round_rect(target, &card_rect, 8.0, &input_bg_brush);

            // 卡片描边：聚焦时切换为强调色，提供明确的焦点视觉反馈
            let card_border_color = if self.ai.ai_panel.input_focused {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.24, 0.26, 0.30, 1.0)
            };
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
                radiusX: 8.0,
                radiusY: 8.0,
            };
            target.DrawRoundedRectangle(&card_rounded, &card_border_brush, 1.0, None);

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
                        let fade_color = color_f(0.9, 0.9, 0.9, alpha);
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
                            let stream_color = color_f(0.9, 0.9, 0.9, 0.92);
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
                                let indicator_color = color_f(0.0, 0.47, 0.83, 0.8);
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
                        .get_brush(target, &color_f(1.0, 0.9, 0.4, 1.0))
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
                        color_f(0.28, 0.24, 0.24, 1.0)
                    } else {
                        color_f(0.20, 0.21, 0.24, 1.0)
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
                        .get_brush(target, &color_f(0.32, 0.32, 0.36, 1.0))
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
            let btn_bg = color_f(0.18, 0.18, 0.20, 1.0);
            let btn_bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &btn_bg) {
                Ok(b) => b,
                Err(_) => return,
            };
            let btn_hover_bg = color_f(0.25, 0.25, 0.28, 1.0);
            let _btn_hover_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_hover_bg)
            {
                Ok(b) => b,
                Err(_) => return,
            };

            // 左侧：模式切换按钮（Ask / Agent）
            let mode_btn_w = 52.0f32;
            let mode_btn_h = toolbar_h;
            let mode_btn_y = toolbar_y;
            let mode_gap = 4.0f32;
            let mut mode_x = x + margin + input_margin;

            // 清空并重建模式按钮命中区域
            self.ai.ai_panel.mode_button_regions.clear();

            for mode in [crate::ai_panel::AiMode::Ask, crate::ai_panel::AiMode::Agent] {
                let is_active = self.ai.ai_panel.mode == mode;
                let btn_rect = D2D_RECT_F {
                    left: mode_x,
                    top: mode_btn_y,
                    right: mode_x + mode_btn_w,
                    bottom: mode_btn_y + mode_btn_h,
                };
                // 背景：激活时高亮
                let bg = if is_active {
                    color_f(0.0, 0.47, 0.83, 1.0)
                } else {
                    btn_bg
                };
                let bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &bg) {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &btn_rect, 4.0, &bg_brush);

                // 文字
                let label = mode.label();
                let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let text_rect = D2D_RECT_F {
                    left: mode_x,
                    top: mode_btn_y + 4.0,
                    right: mode_x + mode_btn_w,
                    bottom: mode_btn_y + mode_btn_h - 2.0,
                };
                let text_color = if is_active { &white_brush } else { &dim_brush };
                target.DrawText(
                    &label_wide,
                    &small_format,
                    &text_rect,
                    text_color,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 注册命中区域（绝对坐标）
                self.ai
                    .ai_panel
                    .mode_button_regions
                    .push((mode, mode_x, mode_btn_y, mode_btn_w, mode_btn_h));

                mode_x += mode_btn_w + mode_gap;
            }

            // 附件 chips（上下文附件切换）
            let attachments = crate::ai_panel::AiPanel::toggleable_attachments();
            let chip_h = toolbar_h;
            let chip_gap = 4.0f32;
            let mut chip_x = mode_x + 8.0; // 与模式按钮间距

            // 清空并重建附件 chip 命中区域
            self.ai.ai_panel.attachment_chip_regions.clear();

            for (i, att) in attachments.iter().enumerate() {
                let is_attached = self.ai.ai_panel.has_attachment(att);
                let label = att.short_label();
                let chip_w = (label.chars().count() as f32 * 7.0 + 16.0).clamp(40.0, 80.0);

                let chip_rect = D2D_RECT_F {
                    left: chip_x,
                    top: mode_btn_y,
                    right: chip_x + chip_w,
                    bottom: mode_btn_y + chip_h,
                };

                // 背景：已附加时高亮
                let bg = if is_attached {
                    color_f(0.16, 0.30, 0.46, 1.0)
                } else {
                    btn_bg
                };
                let bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &bg) {
                    Ok(b) => b,
                    Err(_) => return,
                };
                fill_round_rect(target, &chip_rect, 4.0, &bg_brush);

                // 文字
                let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let text_rect = D2D_RECT_F {
                    left: chip_x,
                    top: mode_btn_y + 4.0,
                    right: chip_x + chip_w,
                    bottom: mode_btn_y + chip_h - 2.0,
                };
                let text_color = if is_attached {
                    &white_brush
                } else {
                    &dim_brush
                };
                target.DrawText(
                    &label_wide,
                    &small_format,
                    &text_rect,
                    text_color,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 注册命中区域（绝对坐标）
                self.ai
                    .ai_panel
                    .attachment_chip_regions
                    .push((i, chip_x, mode_btn_y, chip_w, chip_h));

                chip_x += chip_w + chip_gap;
            }

            // 中间：模型选择下拉按钮
            let model_btn_w = 140.0f32;
            let model_btn_x = chip_x + 8.0; // 与附件 chips 间距
            let model_btn_rect = D2D_RECT_F {
                left: model_btn_x,
                top: toolbar_y,
                right: model_btn_x + model_btn_w,
                bottom: toolbar_y + toolbar_h,
            };
            fill_round_rect(target, &model_btn_rect, 4.0, &btn_bg_brush);
            // 获取当前激活模型显示名称（与设置面板同步）
            let model_label = self.ui.app_settings.active_model_display_name();
            let model_text: Vec<u16> = format!("{} ▼", model_label)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let model_text_rect = D2D_RECT_F {
                left: model_btn_x + 6.0,
                top: toolbar_y + 4.0,
                right: model_btn_x + model_btn_w - 4.0,
                bottom: toolbar_y + toolbar_h - 2.0,
            };
            target.DrawText(
                &model_text,
                &small_format,
                &model_text_rect,
                &dim_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

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
                    let menu_bg = color_f(0.15, 0.15, 0.17, 1.0);
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
                    let menu_border = color_f(0.32, 0.32, 0.36, 1.0);
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
                    let sel_bg = color_f(0.16, 0.30, 0.46, 1.0);
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
                        target.DrawText(
                            &item_wide,
                            &small_format,
                            &D2D_RECT_F {
                                left: menu_x + 10.0,
                                top: iy + 6.0,
                                right: menu_x + menu_w - 10.0,
                                bottom: iy + item_h,
                            },
                            if *is_active { &white_brush } else { text_brush },
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                }
            }

            // 右侧功能按钮区域
            let right_btn_area_x = x + width - margin - input_margin;

            // 发送/中断按钮（根据生成状态切换）
            let send_btn_size = 24.0f32;
            let send_btn_x = right_btn_area_x - send_btn_size;
            let send_btn_y = toolbar_y + 1.0;
            let send_btn_rect = D2D_RECT_F {
                left: send_btn_x,
                top: send_btn_y,
                right: send_btn_x + send_btn_size,
                bottom: send_btn_y + send_btn_size,
            };

            // 根据是否正在生成切换按钮样式
            let is_generating = self.ai.ai_panel.is_generating;
            let (btn_bg, icon_kind) = if is_generating {
                // 中断按钮：红色背景 + 停止图标
                (
                    color_f(0.83, 0.18, 0.18, 1.0),
                    crate::icons::IconKind::Close,
                )
            } else {
                // 发送按钮：蓝色背景 + 发送图标
                (color_f(0.0, 0.47, 0.83, 1.0), crate::icons::IconKind::Send)
            };

            let send_bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &btn_bg) {
                Ok(b) => b,
                Err(_) => return,
            };
            fill_round_rect(target, &send_btn_rect, 6.0, &send_bg_brush);
            // 使用 SVG 图标绘制按钮
            self.ui.icons.draw(
                target,
                icon_kind,
                send_btn_x + 2.0,
                send_btn_y + 2.0,
                send_btn_size - 4.0,
                send_btn_size - 4.0,
                &white_brush,
            );

            // 快捷按钮（星星）——问题扩写
            let star_btn_size = 24.0f32;
            let star_btn_x = send_btn_x - star_btn_size - 4.0;
            let star_btn_rect = D2D_RECT_F {
                left: star_btn_x,
                top: send_btn_y,
                right: star_btn_x + star_btn_size,
                bottom: send_btn_y + star_btn_size,
            };
            fill_round_rect(target, &star_btn_rect, 4.0, &btn_bg_brush);
            // 使用 SVG 图标绘制闪光/星星
            self.ui.icons.draw(
                target,
                crate::icons::IconKind::Sparkles,
                star_btn_x + 2.0,
                send_btn_y + 2.0,
                star_btn_size - 4.0,
                send_btn_size - 4.0,
                &dim_brush,
            );

            // 附加图片按钮（多模态）：仅当前激活模型声明支持多模态时显示，
            // 点击打开文件对话框选择图片，附加后随下一条消息发送。
            let active_multimodal = self.ui.app_settings.active_ai_settings().multimodal;
            if active_multimodal {
                let img_btn_size = 24.0f32;
                let img_btn_x = star_btn_x - img_btn_size - 4.0;
                let img_btn_rect = D2D_RECT_F {
                    left: img_btn_x,
                    top: send_btn_y,
                    right: img_btn_x + img_btn_size,
                    bottom: send_btn_y + img_btn_size,
                };
                fill_round_rect(target, &img_btn_rect, 4.0, &btn_bg_brush);
                self.ui.icons.draw(
                    target,
                    crate::icons::IconKind::Image,
                    img_btn_x + 2.0,
                    send_btn_y + 2.0,
                    img_btn_size - 4.0,
                    img_btn_size - 4.0,
                    &dim_brush,
                );
                // 命中区以面板相对坐标注册（点击处理用 rp_rel_x/rp_rel_y 比对，
                // 与发送/星星按钮的相对坐标系一致；渲染用的 img_btn_x/send_btn_y
                // 是绝对坐标，需减去面板原点 x/y 转换）。
                self.ai.ai_panel.image_button_region =
                    Some((img_btn_x - x, send_btn_y - y, img_btn_size, img_btn_size));
            } else {
                self.ai.ai_panel.image_button_region = None;
            }
        }
    }
}

/// AI 面板消息渲染项：文本/代码段，或 AI 文件/命令操作卡片。
enum AiRenderItem {
    Seg {
        is_code: bool,
        text: String,
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
