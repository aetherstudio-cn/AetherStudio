use super::*;

impl EditorState {
    /// 为文件类型返回矢量图标（避免 emoji 字体差异）。
    /// 命中常见扩展名时返回对应 IconKind，未命中时由调用方回退到通用 File 图标。
    pub(super) fn get_file_vector_icon(&self, name: &str) -> Option<crate::icons::IconKind> {
        use crate::icons::IconKind;
        // Dockerfile 无扩展名特殊处理
        if name.eq_ignore_ascii_case("Dockerfile") || name.eq_ignore_ascii_case("dockerfile") {
            return Some(IconKind::FileDocker);
        }
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "py" | "pyw" | "pyi" => Some(IconKind::FilePython),
            "java" => Some(IconKind::FileJava),
            "kt" | "kts" => Some(IconKind::FileKotlin),
            "txt" => Some(IconKind::FileText),
            "c" | "h" => Some(IconKind::FileC),
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => Some(IconKind::FileCpp),
            "cs" => Some(IconKind::FileCSharp),
            "go" => Some(IconKind::FileGo),
            "rs" => Some(IconKind::FileRust),
            "js" | "mjs" | "cjs" | "jsx" => Some(IconKind::FileJs),
            "ts" | "tsx" => Some(IconKind::FileTs),
            "html" | "htm" | "shtml" => Some(IconKind::FileHtml),
            "css" | "scss" | "sass" | "less" => Some(IconKind::FileCss),
            "json" | "jsonc" | "json5" => Some(IconKind::FileJson),
            "yml" | "yaml" => Some(IconKind::FileYaml),
            "toml" => Some(IconKind::FileToml),
            "md" | "markdown" => Some(IconKind::FileMarkdown),
            "sh" | "bash" | "zsh" | "ksh" => Some(IconKind::FileShell),
            "sql" => Some(IconKind::FileSql),
            "rb" | "ruby" | "erb" => Some(IconKind::FileRuby),
            "php" | "php5" | "phtml" => Some(IconKind::FilePhp),
            "lua" => Some(IconKind::FileLua),
            "swift" => Some(IconKind::FileSwift),
            "dart" => Some(IconKind::FileSwift), // Dart 与 Swift 风格相似，暂用 Swift 图标
            _ => None,
        }
    }

    pub(super) fn render_editor(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let line_height = self.text_renderer.line_height();
        let char_width = self.text_renderer.char_width();
        let line_number_width = 40.0;

        unsafe {
            let bg_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.editor_bg)
                .unwrap();
            let ln_bg_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.line_number_bg)
                .unwrap();
            let sep_color = color_f(0.3, 0.3, 0.3, 1.0);
            let sep_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &sep_color)
                .unwrap();
            let sel_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.selection_bg)
                .unwrap();
            let hl_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.line_highlight_bg)
                .unwrap();
            let ln_fg_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.line_number_fg)
                .unwrap();
            let cursor_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &self.theme.cursor_color)
                .unwrap();

            let font_size = self.text_renderer.font_size();
            let ln_format = self
                .render_ctx
                .text_format_cache
                .get_line_number_format(font_size)
                .unwrap();
            let code_format = self
                .render_ctx
                .text_format_cache
                .get_code_format(font_size)
                .unwrap();

            // 绘制背景
            let bg_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&bg_rect, &bg_brush);
            let ln_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + line_number_width,
                bottom: y + height,
            };
            target.FillRectangle(&ln_rect, &ln_bg_brush);
            let sep_rect = D2D_RECT_F {
                left: x + line_number_width - 1.0,
                top: y,
                right: x + line_number_width,
                bottom: y + height,
            };
            target.FillRectangle(&sep_rect, &sep_brush);

            let (start_line, end_line) = self.visible_line_range();

            for line_idx in start_line..end_line {
                let line_y = y + (line_idx - start_line) as f32 * line_height
                    - (self.content.scroll_y % line_height);
                if line_y > y + height {
                    break;
                }
                if line_y + line_height < y {
                    continue;
                }

                // 优先使用缓存的行文本，避免重复调用 buffer.get_line()
                // P0-A: 窗口化缓存，窗口外返回 None（可见行必在窗口内）
                let cached_line = self.content.cached_line(line_idx);

                // Selection highlight — Glass 模式下使用柔和光晕
                if let (Some((sel_start_line, sel_start_col)), Some((sel_end_line, sel_end_col))) =
                    (self.content.selection_start, self.content.selection_end)
                {
                    let (first_line, first_col) = if sel_start_line <= sel_end_line {
                        (sel_start_line, sel_start_col)
                    } else {
                        (sel_end_line, sel_end_col)
                    };
                    let (last_line, last_col) = if sel_start_line <= sel_end_line {
                        (sel_end_line, sel_end_col)
                    } else {
                        (sel_start_line, sel_start_col)
                    };

                    if line_idx >= first_line && line_idx <= last_line {
                        let sel_start_char = if let Some(text) = cached_line {
                            let col = if line_idx == first_line { first_col } else { 0 };
                            let safe_col = text.floor_char_boundary(col.min(text.len()));
                            text[..safe_col]
                                .chars()
                                .map(unicode_char_width)
                                .sum::<usize>()
                        } else {
                            0
                        };
                        let sel_end_char = if let Some(text) = cached_line {
                            let col = if line_idx == last_line {
                                last_col
                            } else {
                                text.len()
                            };
                            let safe_col = text.floor_char_boundary(col.min(text.len()));
                            text[..safe_col]
                                .chars()
                                .map(unicode_char_width)
                                .sum::<usize>()
                        } else {
                            0
                        };
                        // P0-3: 选区高亮 x 减去水平滚动偏移
                        let sel_start_x = x + line_number_width + 5.0 - self.content.scroll_x
                            + sel_start_char as f32 * char_width;
                        let sel_end_x = x + line_number_width + 5.0 - self.content.scroll_x
                            + sel_end_char as f32 * char_width;
                        let sel_rect = D2D_RECT_F {
                            left: sel_start_x,
                            top: line_y,
                            right: sel_end_x,
                            bottom: line_y + line_height,
                        };
                        if self.theme.glass_enabled {
                            let _ = glass::draw_glow_selection(
                                target,
                                &mut self.render_ctx.brush_cache,
                                &sel_rect,
                                &self.theme.glow_selection,
                                2.0,
                            );
                        } else {
                            target.FillRectangle(&sel_rect, &sel_brush);
                        }
                    }
                }

                // 当前行高亮
                if line_idx == self.content.cursor_line {
                    let hl_rect = D2D_RECT_F {
                        left: x + line_number_width,
                        top: line_y,
                        right: x + width,
                        bottom: line_y + line_height,
                    };
                    target.FillRectangle(&hl_rect, &hl_brush);
                }

                // 行号（DrawText）—— P1-A: 可见行现场生成 UTF-16（栈上几十字节，
                // 纳秒级），替代原全文件 Vec<Vec<u16>> 预缓存（百万行文件 24MB+ 堆开销）
                let ln_wide_final: Vec<u16> = format!("{}", line_idx + 1)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let ln_rect_draw = D2D_RECT_F {
                    left: x + 5.0,
                    top: line_y,
                    right: x + line_number_width - 5.0,
                    bottom: line_y + line_height,
                };
                target.DrawText(
                    &ln_wide_final,
                    &ln_format,
                    &ln_rect_draw,
                    &ln_fg_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 代码文本（使用缓存的 tokens + DrawText）
                // 优化：合并相邻同色 token 段，减少 DrawText 调用次数
                if let Some(line_text) = cached_line {
                    let tokens: &[aether_core::lexer::LexemeSpan] = self
                        .content
                        .cached_tokens
                        .get(line_idx)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);
                    // P0-3: 应用水平滚动偏移；用 PushAxisAlignedClip 裁剪文本区域，
                    // 防止横向滚动后文本溢出到行号区域
                    let text_x = x + line_number_width + 5.0 - self.content.scroll_x;
                    let text_clip = D2D_RECT_F {
                        left: x + line_number_width,
                        top: line_y,
                        right: x + width,
                        bottom: line_y + line_height,
                    };
                    target.PushAxisAlignedClip(&text_clip, D2D1_ANTIALIAS_MODE_ALIASED);

                    let mut current_byte = 0usize;
                    let mut current_char = 0usize;
                    let mut token_idx = 0;

                    // 当前合并段的起始位置和颜色
                    let mut seg_start_byte = 0usize;
                    let mut seg_start_char = 0usize;
                    let mut seg_color = self.theme.text_default;
                    let mut seg_active = false;

                    while current_byte < line_text.len() {
                        let mut token_color = self.theme.text_default;
                        let token_len: usize;

                        if token_idx < tokens.len() {
                            let token = &tokens[token_idx];
                            // LexemeSpan 字段已压缩为 u32，比较/运算前转回 usize
                            let t_start = token.start as usize;
                            let t_end = t_start + token.len as usize;
                            if t_start <= current_byte && current_byte < t_end {
                                token_color = self.theme.color_for_token(token.kind);
                                token_len =
                                    (t_end - current_byte).min(line_text.len() - current_byte);
                                if current_byte + token_len >= t_end {
                                    token_idx += 1;
                                }
                            } else if t_start > current_byte {
                                token_len =
                                    (t_start - current_byte).min(line_text.len() - current_byte);
                            } else {
                                token_idx += 1;
                                continue;
                            }
                        } else {
                            token_len = line_text.len() - current_byte;
                        }

                        if !seg_active {
                            // 开始新段
                            seg_start_byte = current_byte;
                            seg_start_char = current_char;
                            seg_color = token_color;
                            seg_active = true;
                        } else if seg_color != token_color {
                            // 颜色变化：flush 前一段，开始新段
                            let segment = &line_text[seg_start_byte..current_byte];
                            if !segment.is_empty() {
                                draw_segment_cell_aligned(
                                    &mut self.render_ctx,
                                    target,
                                    segment,
                                    seg_start_char,
                                    text_x,
                                    line_y,
                                    char_width,
                                    line_height,
                                    font_size,
                                    &code_format,
                                    &seg_color,
                                );
                            }
                            seg_start_byte = current_byte;
                            seg_start_char = current_char;
                            seg_color = token_color;
                        }
                        // else: 颜色相同，继续累积当前段（无需 DrawText）

                        current_char += line_text[current_byte..current_byte + token_len]
                            .chars()
                            .map(unicode_char_width)
                            .sum::<usize>();
                        current_byte += token_len;
                    }

                    // flush 最后一段
                    if seg_active {
                        let segment = &line_text[seg_start_byte..current_byte];
                        if !segment.is_empty() {
                            draw_segment_cell_aligned(
                                &mut self.render_ctx,
                                target,
                                segment,
                                seg_start_char,
                                text_x,
                                line_y,
                                char_width,
                                line_height,
                                font_size,
                                &code_format,
                                &seg_color,
                            );
                        }
                    }
                    // P0-3: 配对 PopAxisAlignedClip，恢复渲染范围
                    target.PopAxisAlignedClip();
                }

                // ===== LSP 诊断波浪线 =====
                // 根据当前文件路径查找诊断，line_idx 0-based vs DiagnosticItem.line 1-based
                if let Some(path) = &self.content.file_path {
                    let path_str = path.to_string_lossy().to_string();
                    if let Some(diags) = self.diagnostics.get(&path_str) {
                        for diag in diags.iter() {
                            // 当前行（1-based -> 0-based 比较）
                            if diag.line.saturating_sub(1) != line_idx {
                                continue;
                            }
                            // 颜色：错误红色、警告黄色、信息蓝色、提示灰色
                            let wave_color = match diag.severity {
                                1 => color_f(0.9, 0.25, 0.25, 1.0),
                                2 => color_f(0.9, 0.75, 0.2, 1.0),
                                3 => color_f(0.35, 0.6, 0.95, 1.0),
                                _ => color_f(0.55, 0.55, 0.55, 1.0),
                            };
                            let wave_brush = self
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &wave_color)
                                .unwrap();
                            // 起始/结束字符列（1-based -> 0-based）
                            let start_char = diag.col.saturating_sub(1);
                            let end_char = if diag.end_line == diag.line && diag.end_col > diag.col
                            {
                                diag.end_col.saturating_sub(1)
                            } else {
                                // 跨行或无 end_col：取到行尾
                                cached_line
                                    .map(|t| t.chars().count())
                                    .unwrap_or(start_char + 1)
                            };
                            // 至少给 1 个字符宽度，避免空诊断不可见
                            let end_char = end_char.max(start_char + 1);
                            let wave_left = x + line_number_width + 5.0 - self.content.scroll_x
                                + start_char as f32 * char_width;
                            let wave_right = x + line_number_width + 5.0 - self.content.scroll_x
                                + end_char as f32 * char_width;
                            // 波浪线位于行底部，3px 高度区域
                            let wave_top = line_y + line_height - 3.0;
                            // 限制在可见区域
                            if wave_right <= x + line_number_width || wave_left >= x + width {
                                continue;
                            }
                            let clip_left = wave_left.max(x + line_number_width);
                            let clip_right = wave_right.min(x + width);
                            // 绘制简单波浪线（用小矩形拼接成锯齿状）
                            let seg_count =
                                ((clip_right - clip_left) / (char_width * 0.5)).ceil() as i32;
                            if seg_count <= 0 {
                                continue;
                            }
                            let seg_w = (clip_right - clip_left) / seg_count as f32;
                            for i in 0..seg_count {
                                let sx = clip_left + i as f32 * seg_w;
                                // 上下交替形成波浪
                                let offset = if i % 2 == 0 { 0.0 } else { 2.0 };
                                let seg_rect = D2D_RECT_F {
                                    left: sx,
                                    top: wave_top + offset,
                                    right: sx + seg_w + 0.5,
                                    bottom: wave_top + offset + 1.0,
                                };
                                target.FillRectangle(&seg_rect, &wave_brush);
                            }
                        }
                    }
                }
            }

            // P3.2: 在光标之前渲染内联补全幽灵文本
            self.render_inline_completion(
                target,
                x,
                y,
                width,
                height,
                start_line,
                line_height,
                char_width,
                line_number_width,
                &code_format,
            );

            // 光标：将字节列转换为字符列计算x坐标
            // UI-H04: 使用字符宽度累加而非简单 char count * char_width，
            // 支持 CJK 等双宽度字符的正确光标定位
            let cursor_char_col = if let Some(text) =
                self.content.cached_line(self.content.cursor_line)
            {
                let byte_pos = text.floor_char_boundary(self.content.cursor_col.min(text.len()));
                text[..byte_pos]
                    .chars()
                    .map(unicode_char_width)
                    .sum::<usize>()
            } else {
                0
            };
            // P0-3: 光标 x 减去水平滚动偏移
            let cursor_x = x + line_number_width + 5.0 - self.content.scroll_x
                + cursor_char_col as f32 * char_width;
            let cursor_y = y
                + (self.content.cursor_line.saturating_sub(start_line)) as f32 * line_height
                - (self.content.scroll_y % line_height);
            // UI-L02: 更新 IME 候选窗口位置到光标处
            // 文件树输入框激活时，IME 候选窗口定位到输入框附近而非编辑器光标
            // 终端聚焦时，定位到终端光标，否则用户看不到合成窗口会以为删除无效
            if self.terminal_panel.focused {
                let term_region = self.layout.bottom_panel_region();
                let (t_row, t_col) = self.terminal_panel.cursor_position();
                // 光标位置使用 DirectWrite HitTestTextPosition 获取精确前缀坐标（逻辑像素，最后再乘 DPI）
                let cell_w_logical = self
                    .render_ctx
                    .text_format_cache
                    .measure_text_width("M", 11.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                    .unwrap_or(7.0);
                let prefix_x_logical =
                    if let Some(line) = self.terminal_panel.output_lines.get(t_row) {
                        let char_count = line.chars().count();
                        let take = t_col.min(char_count);
                        let mut prefix_len = 0usize;
                        let mut prefix_utf16_len = 0usize;
                        for (idx, ch) in line.char_indices().take(take) {
                            prefix_len = idx + ch.len_utf8();
                            prefix_utf16_len += ch.encode_utf16(&mut [0; 2]).len();
                        }
                        let prefix = &line[..prefix_len];
                        let prefix_x = self
                            .render_ctx
                            .text_format_cache
                            .text_position_x(
                                prefix,
                                prefix_utf16_len,
                                11.0,
                                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            )
                            .unwrap_or(t_col as f32 * cell_w_logical);
                        let extra = (t_col.saturating_sub(char_count)) as f32 * cell_w_logical;
                        prefix_x + extra
                    } else {
                        t_col as f32 * cell_w_logical
                    };
                let line_h_logical = 14.0;
                let term_x_logical = term_region.x + 8.0 + prefix_x_logical;
                let term_y_logical = term_region.y + 24.0 + t_row as f32 * line_h_logical;
                self.ime.set_composition_window_position(
                    (term_x_logical * self.dpi_scale) as i32,
                    (term_y_logical * self.dpi_scale) as i32,
                );
                self.ime.set_candidate_window_position(
                    (term_x_logical * self.dpi_scale) as i32,
                    ((term_y_logical + line_h_logical) * self.dpi_scale) as i32,
                );
            } else if self.file_tree_input.is_some() {
                let sidebar = self.layout.sidebar_region();
                let ft_input_y = sidebar.y + 28.0 + 6.0; // header_h + margin
                let ft_value_x = sidebar.x + 10.0 + 6.0; // input_rect.left + padding
                                                         // 估算 value 宽度（近似，IME 候选窗口只需大致位置）
                let value_chars = self
                    .file_tree_input
                    .as_ref()
                    .map(|i| i.value.chars().count())
                    .unwrap_or(0);
                let ft_cursor_x = ft_value_x + value_chars as f32 * 7.0;
                self.ime.set_candidate_window_position(
                    (ft_cursor_x * self.dpi_scale) as i32,
                    ((ft_input_y + 26.0) * self.dpi_scale) as i32, // input_h
                );
            } else if self.ai_panel.input_focused {
                // AI 面板输入框聚焦时，IME 候选窗口定位到 AI 输入框
                let rp = self.layout.right_panel_region();
                let ai_input_y = rp.y + rp.height - 40.0 + 7.0; // 输入框顶部 + padding
                let ai_value_x = rp.x + 12.0 + 8.0; // margin + padding
                let ai_input_width = self
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(
                        &self.ai_panel.input,
                        11.0,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    )
                    .unwrap_or(0.0);
                let ai_cursor_x = ai_value_x + ai_input_width;
                self.ime.set_composition_window_position(
                    (ai_cursor_x * self.dpi_scale) as i32,
                    (ai_input_y * self.dpi_scale) as i32,
                );
                self.ime.set_candidate_window_position(
                    (ai_cursor_x * self.dpi_scale) as i32,
                    ((ai_input_y + 24.0) * self.dpi_scale) as i32,
                );
            } else {
                self.ime.set_composition_window_position(
                    (cursor_x * self.dpi_scale) as i32,
                    (cursor_y * self.dpi_scale) as i32,
                );
                self.ime.set_candidate_window_position(
                    (cursor_x * self.dpi_scale) as i32,
                    ((cursor_y + line_height) * self.dpi_scale) as i32,
                );
            }
            if cursor_y >= y && cursor_y <= y + height {
                // P0-2: 若存在 IME 合成串，渲染合成串文本 + 下划线，光标隐藏
                if let Some(comp) = self.composition.as_ref() {
                    if !comp.is_empty() {
                        // 合成串宽度（按字符宽度累加，CJK 字符 2 倍宽）
                        let comp_char_width: usize = comp.chars().map(unicode_char_width).sum();
                        let comp_pixel_width = comp_char_width as f32 * char_width;

                        // 渲染合成串文本（与代码格式一致）
                        let comp_utf16: Vec<u16> = comp.encode_utf16().collect();
                        let comp_rect = D2D_RECT_F {
                            left: cursor_x,
                            top: cursor_y,
                            right: cursor_x + comp_pixel_width + 4.0,
                            bottom: cursor_y + line_height,
                        };
                        target.DrawText(
                            &comp_utf16,
                            &code_format,
                            &comp_rect,
                            &cursor_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );

                        // 渲染下划线（光标颜色，距底部 2px，1px 高）
                        let underline_y = cursor_y + line_height - 2.0;
                        let underline_rect = D2D_RECT_F {
                            left: cursor_x,
                            top: underline_y,
                            right: cursor_x + comp_pixel_width,
                            bottom: underline_y + 1.0,
                        };
                        target.FillRectangle(&underline_rect, &cursor_brush);
                    } else {
                        // 合成串为空时显示普通光标
                        let cursor_rect = D2D_RECT_F {
                            left: cursor_x,
                            top: cursor_y,
                            right: cursor_x + 2.0,
                            bottom: cursor_y + line_height,
                        };
                        target.FillRectangle(&cursor_rect, &cursor_brush);
                    }
                } else {
                    let cursor_rect = D2D_RECT_F {
                        left: cursor_x,
                        top: cursor_y,
                        right: cursor_x + 2.0,
                        bottom: cursor_y + line_height,
                    };
                    target.FillRectangle(&cursor_rect, &cursor_brush);
                }
            }
        }
    }

    /// P3.2: 渲染内联补全幽灵文本
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_inline_completion(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        _width: f32,
        _height: f32,
        start_line: usize,
        line_height: f32,
        char_width: f32,
        line_number_width: f32,
        code_format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    ) {
        let Some(comp) = self.content.inline_completion.as_ref() else {
            return;
        };

        // 仅当建议触发位置与当前光标位置匹配时渲染，避免错位
        if comp.trigger_line != self.content.cursor_line
            || comp.trigger_col != self.content.cursor_col
        {
            return;
        }

        unsafe {
            let ghost_color = color_f(0.5, 0.5, 0.5, 0.6);
            let Ok(ghost_brush) = self.render_ctx.brush_cache.get_brush(target, &ghost_color)
            else {
                return;
            };

            let cursor_char_col = if let Some(text) =
                self.content.cached_line(self.content.cursor_line)
            {
                let byte_pos = text.floor_char_boundary(self.content.cursor_col.min(text.len()));
                text[..byte_pos]
                    .chars()
                    .map(unicode_char_width)
                    .sum::<usize>()
            } else {
                0
            };

            let ghost_x = x + line_number_width + 5.0 - self.content.scroll_x
                + cursor_char_col as f32 * char_width;
            let ghost_y = y
                + (self.content.cursor_line.saturating_sub(start_line)) as f32 * line_height
                - (self.content.scroll_y % line_height);

            let text_utf16: Vec<u16> = comp.text.encode_utf16().collect();
            let text_rect = windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                left: ghost_x,
                top: ghost_y,
                right: ghost_x + comp.text.len() as f32 * char_width + 10.0,
                bottom: ghost_y + line_height,
            };
            target.DrawText(
                &text_utf16,
                code_format,
                &text_rect,
                &ghost_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// P3.4: 渲染 hover tooltip（鼠标悬停提示框）
    ///
    /// 在鼠标附近绘制一个深色背景的提示框，显示文件树节点的绝对路径。
    /// 小字号单行优先；超宽时仅在路径分隔符处折行，绝不折断单个文件/文件夹名。
    pub(super) fn render_hover_tooltip(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    ) {
        let Some(tooltip) = self.hover.tooltip.as_ref() else {
            return;
        };
        if tooltip.is_empty() {
            return;
        }

        unsafe {
            // 小字号提示（VS Code 风格）
            let font_size = 11.0_f32;
            let line_height = 16.0_f32;
            let padding_x = 8.0_f32;
            let padding_y = 5.0_f32;
            let win_w = self.window_width as f32;
            let win_h = self.window_height as f32;
            // 可用最大文本宽度：窗口宽减去边距与内边距
            let max_text_w = (win_w - 16.0 - padding_x * 2.0).max(80.0);

            // DirectWrite 精确测宽（估算宽度会导致 DrawText 在名字中间折行）
            let measure =
                |cache: &aether_render::d2d::brush_cache::TextFormatCache, s: &str| -> f32 {
                    cache
                        .measure_text_width(s, font_size, 400)
                        .unwrap_or(s.chars().count() as f32 * font_size * 0.62)
                };

            // 整段能放下则单行；否则仅在路径分隔符（\ /）后折行，
            // 单段（单个文件/文件夹名）即使超宽也独占一行不折断
            let mut lines: Vec<String> = Vec::new();
            let mut line_widths: Vec<f32> = Vec::new();
            for raw_line in tooltip.text.split('\n') {
                let full_w = measure(&self.render_ctx.text_format_cache, raw_line);
                if full_w <= max_text_w {
                    lines.push(raw_line.to_string());
                    line_widths.push(full_w);
                    continue;
                }
                // 按分隔符拆段（分隔符留在段尾），贪心拼行
                let segments: Vec<&str> = raw_line.split_inclusive(['\\', '/']).collect();
                let mut current = String::new();
                for seg in segments {
                    if current.is_empty() {
                        current.push_str(seg);
                        continue;
                    }
                    let candidate_w = measure(
                        &self.render_ctx.text_format_cache,
                        &format!("{}{}", current, seg),
                    );
                    if candidate_w <= max_text_w {
                        current.push_str(seg);
                    } else {
                        let w = measure(&self.render_ctx.text_format_cache, &current);
                        lines.push(std::mem::take(&mut current));
                        line_widths.push(w);
                        current.push_str(seg);
                    }
                }
                if !current.is_empty() {
                    let w = measure(&self.render_ctx.text_format_cache, &current);
                    lines.push(current);
                    line_widths.push(w);
                }
            }

            let text_w = line_widths.iter().copied().fold(0.0_f32, f32::max);
            let text_h = lines.len() as f32 * line_height;
            let box_w = text_w + padding_x * 2.0 + 2.0; // +2 余量防亚像素舍入再折行
            let box_h = text_h + padding_y * 2.0;

            // 钳制到窗口范围内，避免 tooltip 超出右/下边界
            let tx = if tooltip.x + box_w > win_w {
                (win_w - box_w).max(0.0)
            } else {
                tooltip.x
            };
            let ty = if tooltip.y + box_h > win_h {
                (win_h - box_h).max(0.0)
            } else {
                tooltip.y
            };

            // 背景：半透明深色
            let bg_color = color_f(0.12, 0.12, 0.15, 0.95);
            let Ok(bg_brush) = self.render_ctx.brush_cache.get_brush(target, &bg_color) else {
                return;
            };
            // 边框：浅色
            let border_color = color_f(0.4, 0.4, 0.45, 1.0);
            let Ok(border_brush) = self.render_ctx.brush_cache.get_brush(target, &border_color)
            else {
                return;
            };
            // 文本：浅色
            let text_color = color_f(0.9, 0.9, 0.9, 1.0);
            let Ok(text_brush) = self.render_ctx.brush_cache.get_brush(target, &text_color) else {
                return;
            };

            let box_rect = windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                left: tx,
                top: ty,
                right: tx + box_w,
                bottom: ty + box_h,
            };
            target.FillRectangle(&box_rect, &bg_brush);
            target.DrawRectangle(&box_rect, &border_brush, 1.0, None);

            // 绘制文本（逐行，小字号）
            // DWRITE_TEXT_ALIGNMENT_LEADING=0, DWRITE_PARAGRAPH_ALIGNMENT_NEAR=0
            let tf = match self
                .render_ctx
                .text_format_cache
                .get_format(font_size, 400, 0, 0)
            {
                Ok(tf) => tf,
                Err(_) => return,
            };

            for (i, line) in lines.iter().enumerate() {
                let line_y = ty + padding_y + i as f32 * line_height;
                let line_rect = windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                    left: tx + padding_x,
                    top: line_y,
                    // 给足宽度：折行已在上面按分隔符完成，DrawText 不得再自动换行
                    right: tx + padding_x + text_w + 2.0,
                    bottom: line_y + line_height,
                };
                let utf16: Vec<u16> = line.encode_utf16().collect();
                target.DrawText(
                    &utf16,
                    &tf,
                    &line_rect,
                    &text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }
}

/// 按可视单元格网格绘制一段同色代码文本。
///
/// 纯窄字符（ASCII 等）段整体一次绘制（等宽字体下字形天然对齐网格，无性能回退）；
/// 含宽字符（CJK 等占 2 格）时按宽度类切分：窄字符连续区间整体绘制，
/// 宽字符逐个对齐到所属单元格起点。否则 DirectWrite 回退字体的 CJK 真实字宽
/// ≠ 2×char_width，段内字形逐渐漂离网格，导致光标/选区/点击命中（均按网格计算）
/// 与字形视觉位置对不上，中文越多偏差越大。
#[allow(clippy::too_many_arguments)]
unsafe fn draw_segment_cell_aligned(
    render_ctx: &mut crate::render_context::RenderContext,
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    segment: &str,
    start_cell: usize,
    text_x: f32,
    line_y: f32,
    char_width: f32,
    line_height: f32,
    font_size: f32,
    code_format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    color: &windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
) {
    let Ok(brush) = render_ctx.brush_cache.get_brush(target, color) else {
        return;
    };
    let draw_run = |run: &str, cell: usize, ctx: &mut crate::render_context::RenderContext| {
        if run.is_empty() {
            return;
        }
        if let Ok(layout) =
            ctx.text_layout_cache
                .get_or_create(run, code_format, line_height, font_size)
        {
            let point = D2D_POINT_2F {
                x: text_x + cell as f32 * char_width,
                y: line_y,
            };
            target.DrawTextLayout(point, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        }
    };

    // 快速路径：无宽字符 → 整段一次绘制（绝大多数代码行）
    if segment.chars().all(|c| unicode_char_width(c) < 2) {
        draw_run(segment, start_cell, render_ctx);
        return;
    }

    let mut run_start_byte = 0usize;
    let mut run_start_cell = start_cell;
    let mut cur_cell = start_cell;
    for (i, ch) in segment.char_indices() {
        let w = unicode_char_width(ch);
        if w >= 2 {
            // flush 之前的窄字符连续区间
            draw_run(&segment[run_start_byte..i], run_start_cell, render_ctx);
            // 宽字符对齐到自身单元格起点单独绘制（单字符 layout 命中缓存）
            let end = i + ch.len_utf8();
            draw_run(&segment[i..end], cur_cell, render_ctx);
            run_start_byte = end;
            cur_cell += w;
            run_start_cell = cur_cell;
        } else {
            cur_cell += w;
        }
    }
    // flush 尾部窄字符区间
    draw_run(&segment[run_start_byte..], run_start_cell, render_ctx);
}
