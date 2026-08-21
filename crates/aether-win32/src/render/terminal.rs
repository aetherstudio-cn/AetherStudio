use super::*;

/// 底部面板内层标签栏（终端/问题）高度
pub(crate) const TERMINAL_INNER_TAB_H: f32 = 28.0;
/// 终端内容区顶部相对面板顶部的偏移（内层标签栏 + 间距）
pub(crate) const TERMINAL_CONTENT_TOP: f32 = TERMINAL_INNER_TAB_H + 4.0;
/// 终端文本渲染左内边距
pub(crate) const TERMINAL_TEXT_LEFT: f32 = 10.0;
/// 终端行高（11pt 字体行高约 14.7px，16px 行距避免上下行字形贴靠、提升可读性）
pub(crate) const TERMINAL_LINE_H: f32 = 16.0;

impl EditorState {
    pub(super) fn render_bottom_panel(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        unsafe {
            // 终端底色：纯黑（经典终端观感）
            let bg_color = if self.win.theme.glass_enabled {
                color_f(0.02, 0.02, 0.02, 0.96)
            } else {
                color_f(0.02, 0.02, 0.02, 1.0)
            };
            let bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &bg_color)
                .unwrap();
            let border_color = if self.win.theme.glass_enabled {
                self.win.theme.panel_border
            } else {
                color_f(0.2, 0.2, 0.2, 1.0)
            };
            let _border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &border_color)
                .unwrap();
            let text_color = color_f(0.8, 0.8, 0.8, 1.0);
            let _text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &text_color)
                .unwrap();
            let active_color = color_f(1.0, 1.0, 1.0, 1.0);
            let active_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &active_color)
                .unwrap();
            let dim_color = color_f(0.5, 0.5, 0.5, 1.0);
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &dim_color)
                .unwrap();
            let output_color = color_f(0.72, 0.72, 0.72, 1.0);
            let output_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &output_color)
                .unwrap();
            // 提示符用 dim_brush（灰色），已输入内容用 active_brush（白色）

            let ui_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            let mono_format = self
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

            // 背景
            let bg_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&bg_rect, &bg_brush);

            // 顶部边框（聚焦时高亮为强调色，提供视觉反馈）
            let top_border_color = if self.terminal.terminal_panel.focused {
                color_f(0.3, 0.55, 0.85, 1.0)
            } else {
                border_color
            };
            let top_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &top_border_color)
                .unwrap();
            let top_border = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + 2.0,
            };
            target.FillRectangle(&top_border, &top_border_brush);

            // ===== 全局搜索面板（覆盖默认终端内容） =====
            if self.ui.search_panel.visible {
                // 搜索输入框
                let input_height = 24.0;
                let input_rect = D2D_RECT_F {
                    left: x + 10.0,
                    top: y + 6.0,
                    right: x + width - 10.0,
                    bottom: y + 6.0 + input_height,
                };
                let input_bg = color_f(0.18, 0.18, 0.2, 1.0);
                let input_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &input_bg)
                    .unwrap();
                target.FillRectangle(&input_rect, &input_bg_brush);

                // 输入框边框（聚焦时高亮）
                let border_focused = color_f(0.3, 0.55, 0.85, 1.0);
                let border_dim = color_f(0.3, 0.3, 0.3, 1.0);
                let input_border_color = if self.ui.search_panel.visible {
                    border_focused
                } else {
                    border_dim
                };
                let input_border_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &input_border_color)
                    .unwrap();
                // 1px 边框
                let b = 1.0;
                let border_rects = [
                    D2D_RECT_F {
                        left: input_rect.left,
                        top: input_rect.top,
                        right: input_rect.right,
                        bottom: input_rect.top + b,
                    },
                    D2D_RECT_F {
                        left: input_rect.left,
                        top: input_rect.bottom - b,
                        right: input_rect.right,
                        bottom: input_rect.bottom,
                    },
                    D2D_RECT_F {
                        left: input_rect.left,
                        top: input_rect.top,
                        right: input_rect.left + b,
                        bottom: input_rect.bottom,
                    },
                    D2D_RECT_F {
                        left: input_rect.right - b,
                        top: input_rect.top,
                        right: input_rect.right,
                        bottom: input_rect.bottom,
                    },
                ];
                for r in &border_rects {
                    target.FillRectangle(r, &input_border_brush);
                }

                // 搜索图标 + 输入文本
                let prefix = "🔍 ";
                let prefix_wide: Vec<u16> = prefix.encode_utf16().chain(Some(0)).collect();
                let prefix_rect = D2D_RECT_F {
                    left: input_rect.left + 6.0,
                    top: input_rect.top + 4.0,
                    right: input_rect.left + 30.0,
                    bottom: input_rect.bottom - 2.0,
                };
                target.DrawText(
                    &prefix_wide,
                    &ui_format,
                    &prefix_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                let query_text = if self.ui.search_panel.query.is_empty() {
                    "输入搜索内容...".to_string()
                } else {
                    self.ui.search_panel.query.clone()
                };
                let query_wide: Vec<u16> = query_text.encode_utf16().chain(Some(0)).collect();
                let query_rect = D2D_RECT_F {
                    left: input_rect.left + 30.0,
                    top: input_rect.top + 4.0,
                    right: input_rect.right - 100.0,
                    bottom: input_rect.bottom - 2.0,
                };
                let query_brush = if self.ui.search_panel.query.is_empty() {
                    &dim_brush
                } else {
                    &active_brush
                };
                target.DrawText(
                    &query_wide,
                    &ui_format,
                    &query_rect,
                    query_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 选项标签：Aa（大小写）、.*（正则）
                let case_label = if self.ui.search_panel.case_sensitive {
                    "Aa✓"
                } else {
                    "Aa"
                };
                let regex_label = if self.ui.search_panel.regex {
                    ".*✓"
                } else {
                    ".*"
                };
                let opts_x = input_rect.right - 90.0;
                let case_wide: Vec<u16> = case_label.encode_utf16().chain(Some(0)).collect();
                let case_rect = D2D_RECT_F {
                    left: opts_x,
                    top: input_rect.top + 4.0,
                    right: opts_x + 40.0,
                    bottom: input_rect.bottom - 2.0,
                };
                target.DrawText(
                    &case_wide,
                    &ui_format,
                    &case_rect,
                    if self.ui.search_panel.case_sensitive {
                        &active_brush
                    } else {
                        &dim_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                let regex_wide: Vec<u16> = regex_label.encode_utf16().chain(Some(0)).collect();
                let regex_rect = D2D_RECT_F {
                    left: opts_x + 45.0,
                    top: input_rect.top + 4.0,
                    right: opts_x + 85.0,
                    bottom: input_rect.bottom - 2.0,
                };
                target.DrawText(
                    &regex_wide,
                    &ui_format,
                    &regex_rect,
                    if self.ui.search_panel.regex {
                        &active_brush
                    } else {
                        &dim_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 状态行
                let status_y = input_rect.bottom + 4.0;
                let status_text = if self.ui.search_panel.is_searching {
                    "搜索中...".to_string()
                } else if self.ui.search_panel.status.is_empty() {
                    "按 Enter 搜索 · Esc 关闭".to_string()
                } else {
                    self.ui.search_panel.status.clone()
                };
                let status_wide: Vec<u16> = status_text.encode_utf16().chain(Some(0)).collect();
                let status_rect = D2D_RECT_F {
                    left: x + 10.0,
                    top: status_y,
                    right: x + width - 10.0,
                    bottom: status_y + 16.0,
                };
                target.DrawText(
                    &status_wide,
                    &ui_format,
                    &status_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 结果列表
                let results_y = status_y + 18.0;
                let mut line_y = results_y;
                let max_y = y + height - 6.0;
                let line_h = 16.0;
                let results = self.ui.search_panel.results.clone();
                let selected = self.ui.search_panel.selected_index;
                for (i, r) in results.iter().enumerate() {
                    if line_y + line_h > max_y {
                        break;
                    }
                    // 选中行高亮
                    if i == selected {
                        let sel_rect = D2D_RECT_F {
                            left: x + 4.0,
                            top: line_y - 1.0,
                            right: x + width - 4.0,
                            bottom: line_y + line_h - 1.0,
                        };
                        let sel_bg = color_f(0.2, 0.3, 0.5, 1.0);
                        let sel_bg_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &sel_bg)
                            .unwrap();
                        target.FillRectangle(&sel_rect, &sel_bg_brush);
                    }

                    // 文件路径（相对路径）+ 行号
                    let rel_path = self
                        .fs
                        .current_folder
                        .as_ref()
                        .and_then(|root| r.path.strip_prefix(root).ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| r.path.to_string_lossy().to_string());
                    let header = format!("{}:{}:{}", rel_path, r.line, r.col);
                    let header_wide: Vec<u16> = header.encode_utf16().chain(Some(0)).collect();
                    let header_rect = D2D_RECT_F {
                        left: x + 12.0,
                        top: line_y,
                        right: x + width - 12.0,
                        bottom: line_y + line_h,
                    };
                    let header_brush = if i == selected {
                        &active_brush
                    } else {
                        &output_brush
                    };
                    target.DrawText(
                        &header_wide,
                        &mono_format,
                        &header_rect,
                        header_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    line_y += line_h;

                    // 匹配行内容（截断显示）
                    if line_y + line_h > max_y {
                        break;
                    }
                    let content = r.text.trim_end();
                    let content_display = if content.chars().count() > 200 {
                        format!("{}...", content.chars().take(200).collect::<String>())
                    } else {
                        content.to_string()
                    };
                    let content_wide: Vec<u16> =
                        content_display.encode_utf16().chain(Some(0)).collect();
                    let content_rect = D2D_RECT_F {
                        left: x + 24.0,
                        top: line_y,
                        right: x + width - 12.0,
                        bottom: line_y + line_h,
                    };
                    target.DrawText(
                        &content_wide,
                        &mono_format,
                        &content_rect,
                        &dim_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    line_y += line_h + 2.0;
                }
                // 搜索面板模式下结束渲染（不显示终端内容）
                return;
            }

            // 底部面板标签栏（类似 VS Code 底部面板标签）
            // 注意：标签顺序必须与 BottomPanelTab 枚举的 discriminant 一致。
            // 当前只保留"终端"和"问题"两个标签；"输出"标签已移除，
            // 问题面板的引擎/数据采集待后续设计。
            let tab_height = TERMINAL_INNER_TAB_H;
            let tabs: [BottomPanelTab; 2] = [BottomPanelTab::Terminal, BottomPanelTab::Problems];
            let mut tab_x = x + 10.0;
            let tab_w = 60.0;
            for tab_kind in tabs.iter() {
                let is_active = *tab_kind == self.terminal.bottom_panel_tab;
                let tab_rect = D2D_RECT_F {
                    left: tab_x,
                    top: y + 2.0,
                    right: tab_x + tab_w,
                    bottom: y + tab_height - 2.0,
                };
                if is_active {
                    let active_bg = color_f(0.18, 0.18, 0.2, 1.0);
                    let active_bg_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &active_bg)
                        .unwrap();
                    target.FillRectangle(&tab_rect, &active_bg_brush);
                    let top_line = D2D_RECT_F {
                        left: tab_x,
                        top: y + 2.0,
                        right: tab_x + tab_w,
                        bottom: y + 4.0,
                    };
                    target.FillRectangle(&top_line, &active_brush);
                }
                let tab_wide: Vec<u16> = tab_kind.label().encode_utf16().chain(Some(0)).collect();
                let tab_text_rect = D2D_RECT_F {
                    left: tab_x + 8.0,
                    top: y + 4.0,
                    right: tab_x + tab_w - 4.0,
                    bottom: y + tab_height - 4.0,
                };
                target.DrawText(
                    &tab_wide,
                    &ui_format,
                    &tab_text_rect,
                    if is_active { &active_brush } else { &dim_brush },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                tab_x += tab_w + 4.0;
            }

            // 标签下方的内容：根据当前 tab 分支渲染
            // 0 = 终端（已有逻辑）；1 = 问题面板（暂未实现）
            let content_y = y + TERMINAL_CONTENT_TOP;
            let content_h = height - TERMINAL_CONTENT_TOP - 8.0;

            // P-问题: 问题面板占位。问题数据/采集引擎后续从 diagnostics 字段设计。
            // 当前仅渲染居中提示，让用户能验证"终端/问题"切换能力已生效。
            if self.terminal.bottom_panel_tab == BottomPanelTab::Problems {
                let hint_color = color_f(150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 1.0);
                let hint_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &hint_color)
                    .unwrap();
                let hint_format = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .get_format(
                        14.0,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                    )
                    .unwrap();
                let hint_text: Vec<u16> =
                    "问题面板（待实现）".encode_utf16().chain(Some(0)).collect();
                let hint_rect = D2D_RECT_F {
                    left: x,
                    top: content_y,
                    right: x + width,
                    bottom: content_y + content_h,
                };
                target.DrawText(
                    &hint_text,
                    &hint_format,
                    &hint_rect,
                    &hint_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                return;
            }

            // 终端几何：由面板宽高推导行列数，未启动/运行中两个分支共用。
            // 未启动时也同步，确保首次 start() 时 ConPTY 使用与面板匹配的宽度
            //（默认 80 列与面板宽度不匹配会导致换行错位），并为 ANSI 解析器提供正确换行宽度。
            let cell_w = self
                .win
                .render_ctx
                .text_format_cache
                .measure_text_width("M", 11.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                .unwrap_or(7.0);
            let line_h = TERMINAL_LINE_H;
            let content_bottom = y + height - 6.0;
            let visible_lines = ((content_bottom - content_y) / line_h).floor().max(1.0) as usize;
            let term_cols = ((width - 20.0) / cell_w).max(20.0) as i16;
            let term_rows = visible_lines.max(5) as i16;
            self.terminal.terminal_panel.set_size(term_cols, term_rows);

            // 终端未启动时：若有历史输出（进程已退出）则显示输出+重启提示；
            // 否则显示居中引导文案
            if !self.terminal.terminal_panel.running {
                if self.terminal.terminal_panel.output_lines.is_empty() {
                    // 从未启动：居中提示
                    let hint_color = color_f(150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 1.0);
                    let hint_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &hint_color)
                        .unwrap();
                    let hint_format = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .get_format(
                            14.0,
                            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                        )
                        .unwrap();
                    let hint_text: Vec<u16> =
                        "按 Ctrl+` 启动终端".encode_utf16().chain(Some(0)).collect();
                    let hint_rect = D2D_RECT_F {
                        left: x,
                        top: content_y,
                        right: x + width,
                        bottom: content_y + content_h,
                    };
                    target.DrawText(
                        &hint_text,
                        &hint_format,
                        &hint_rect,
                        &hint_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                } else {
                    // 进程已退出：显示历史输出 + 底部重启提示
                    let line_h = TERMINAL_LINE_H;
                    let content_bottom = y + height - 24.0; // 底部留空给重启提示
                    let visible_lines =
                        ((content_bottom - content_y) / line_h).floor().max(1.0) as usize;
                    let lines = self.terminal.terminal_panel.visible_window(visible_lines);
                    let mut line_y = content_y;
                    for line in &lines {
                        if line_y + line_h > content_bottom {
                            break;
                        }
                        let text: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
                        let text_rect = D2D_RECT_F {
                            left: x + 10.0,
                            top: line_y,
                            right: x + width - 10.0,
                            bottom: line_y + line_h,
                        };
                        target.DrawText(
                            &text,
                            &mono_format,
                            &text_rect,
                            &output_brush,
                            // CLIP：超长行裁剪而非视觉换行，避免换行部分叠到相邻行形成重影
                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                        line_y += line_h;
                    }
                    // 底部重启提示
                    let restart_color = color_f(0.3, 0.55, 0.85, 1.0);
                    let restart_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &restart_color)
                        .unwrap();
                    let restart_text: Vec<u16> = "点击此处重新启动终端"
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    let restart_rect = D2D_RECT_F {
                        left: x + 10.0,
                        top: y + height - 22.0,
                        right: x + width - 10.0,
                        bottom: y + height - 6.0,
                    };
                    target.DrawText(
                        &restart_text,
                        &ui_format,
                        &restart_rect,
                        &restart_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            } else {
                // 复用上方统一计算的几何（cell_w/line_h/visible_lines/term_cols/term_rows）
                let lines = self.terminal.terminal_panel.visible_window(visible_lines);

                // 光标与可见窗口信息（行循环与光标绘制共用）
                let total_lines = self.terminal.terminal_panel.output_lines.len();
                let scroll_off = self.terminal.terminal_panel.scroll_offset;
                let end_line = total_lines.saturating_sub(scroll_off);
                let start_line = end_line.saturating_sub(visible_lines);
                let (cursor_row, cursor_col) = self.terminal.terminal_panel.cursor_position();

                // 滚动提示：用户向上浏览历史时显示提示
                if self.terminal.terminal_panel.scroll_offset > 0 {
                    let hint_wide: Vec<u16> = "↑ 历史输出（回车回到最新）"
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    let hint_rect = D2D_RECT_F {
                        left: x + 12.0,
                        top: content_y - 2.0,
                        right: x + width - 12.0,
                        bottom: content_y + 16.0,
                    };
                    target.DrawText(
                        &hint_wide,
                        &ui_format,
                        &hint_rect,
                        &active_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }

                let mut line_y = content_y;
                for (li, line) in lines.iter().enumerate() {
                    if line_y + line_h > content_bottom {
                        break;
                    }
                    let text_rect = D2D_RECT_F {
                        left: x + TERMINAL_TEXT_LEFT,
                        top: line_y,
                        right: x + width - 10.0,
                        bottom: line_y + line_h,
                    };
                    // 当前输入行（光标所在行）：提示符灰色 + 已输入内容白色；
                    // 其余历史输出行统一浅灰
                    let mut drawn_split = false;
                    if start_line + li == cursor_row {
                        let boundary = self
                            .terminal
                            .terminal_panel
                            .fake_prompt_char_count()
                            .or_else(|| prompt_boundary_chars(line));
                        if let Some(b) = boundary.filter(|&b| b > 0 && b <= line.chars().count()) {
                            let byte_idx = line
                                .char_indices()
                                .nth(b)
                                .map(|(i, _)| i)
                                .unwrap_or(line.len());
                            let (prompt_part, rest_part) = line.split_at(byte_idx);
                            let prompt_wide: Vec<u16> =
                                prompt_part.encode_utf16().chain(Some(0)).collect();
                            target.DrawText(
                                &prompt_wide,
                                &mono_format,
                                &text_rect,
                                &dim_brush,
                                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                            if !rest_part.is_empty() {
                                let prompt_utf16 = prompt_part.encode_utf16().count();
                                let prompt_x = self
                                    .win
                                    .render_ctx
                                    .text_format_cache
                                    .text_position_x(
                                        prompt_part,
                                        prompt_utf16,
                                        11.0,
                                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                                    )
                                    .unwrap_or(b as f32 * cell_w);
                                let rest_wide: Vec<u16> =
                                    rest_part.encode_utf16().chain(Some(0)).collect();
                                let rest_rect = D2D_RECT_F {
                                    left: x + TERMINAL_TEXT_LEFT + prompt_x,
                                    ..text_rect
                                };
                                target.DrawText(
                                    &rest_wide,
                                    &mono_format,
                                    &rest_rect,
                                    &active_brush,
                                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                                    DWRITE_MEASURING_MODE_NATURAL,
                                );
                            }
                            drawn_split = true;
                        }
                    }
                    if !drawn_split {
                        let text: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &text,
                            &mono_format,
                            &text_rect,
                            &output_brush,
                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    line_y += line_h;
                }

                // 渲染光标：在光标位置绘制方块
                // ConPTY 模式下光标位置由 ANSI 解析器跟踪（col 为 cell 制）
                if cursor_row >= start_line && cursor_row < end_line {
                    let display_row = cursor_row - start_line;
                    // 光标 x 用 split_line_at_cell 把 cell 列换算成前缀，
                    // 再用 DirectWrite HitTestTextPosition 取前缀尾端精确像素坐标
                    let cursor_x = if let Some(line) =
                        self.terminal.terminal_panel.output_lines.get(cursor_row)
                    {
                        let (byte_len, utf16_len, extra_cells) =
                            crate::terminal::TerminalPanel::split_line_at_cell(line, cursor_col);
                        let prefix = &line[..byte_len];
                        let prefix_x = self
                            .win
                            .render_ctx
                            .text_format_cache
                            .text_position_x(
                                prefix,
                                utf16_len,
                                11.0,
                                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            )
                            .unwrap_or(cursor_col as f32 * cell_w);
                        x + TERMINAL_TEXT_LEFT + prefix_x + extra_cells as f32 * cell_w
                    } else {
                        x + TERMINAL_TEXT_LEFT + cursor_col as f32 * cell_w
                    };
                    let cursor_y = content_y + display_row as f32 * line_h;
                    let cursor_w = self
                        .terminal
                        .terminal_panel
                        .output_lines
                        .get(cursor_row)
                        .and_then(|line| {
                            crate::terminal::TerminalPanel::char_at_cell(line, cursor_col)
                        })
                        .map(|ch| (unicode_char_width(ch) as f32).max(1.0) * cell_w)
                        .unwrap_or(cell_w);
                    let cursor_h = line_h;
                    // 只在光标可见区域内绘制
                    if cursor_y + cursor_h <= content_bottom {
                        let cursor_color = color_f(0.8, 0.8, 0.8, 0.6);
                        let cursor_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &cursor_color)
                            .unwrap();
                        let cursor_rect = D2D_RECT_F {
                            left: cursor_x,
                            top: cursor_y,
                            right: cursor_x + cursor_w,
                            bottom: cursor_y + cursor_h,
                        };
                        if self.terminal.terminal_panel.focused {
                            // 聚焦：实心方块
                            target.FillRectangle(&cursor_rect, &cursor_brush);
                        } else {
                            // 失焦：空心边框（保留位置指示，不吸引注意）
                            target.DrawRectangle(&cursor_rect, &cursor_brush, 1.0, None);
                        }
                    }
                }
            }
        }
    }
}

/// 启发式识别终端输入行的提示符边界（返回字符数，含结束标记）。
///
/// 仅用于当前输入行的双色渲染：优先匹配 PowerShell 风格 "> "，
/// 其次 cmd 风格行尾 ">"，再次 bash/zsh 风格 "$ " / "% "。
fn prompt_boundary_chars(line: &str) -> Option<usize> {
    if let Some(idx) = line.rfind("> ") {
        return Some(line[..idx].chars().count() + 2);
    }
    for marker in ["$ ", "% "] {
        if let Some(idx) = line.rfind(marker) {
            return Some(line[..idx].chars().count() + 2);
        }
    }
    if let Some(idx) = line.rfind('>') {
        return Some(line[..idx].chars().count() + 1);
    }
    None
}
