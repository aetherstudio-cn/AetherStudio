//! AI 历史对话浮动窗口渲染（可拖动、带搜索、时间标签、标题编辑）。
//!
//! 浮窗在整个窗口层级的最顶层渲染（见 render/mod.rs），
//! 覆盖于编辑器/面板/菜单之上，不挤压任何布局。

use super::*;

impl EditorState {
    /// 渲染历史对话浮动窗口（仅在 history_open 时调用）。
    /// 返回浮窗在客户区中的矩形（供命中测试与外部点击关闭判定）。
    pub(super) fn render_history_float_window(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    ) {
        if !self.ai.ai_panel.history_open {
            self.ai.ai_panel.history_win_region = None;
            return;
        }

        // 浮窗专属 Vec 命中区每帧重建前显式清空。
        // 浮窗渲染不受右面板 visible 守卫（可拖出/居中于整个窗口），
        // 不能依赖 AI 面板的 clear_hit_regions（其仅在右面板渲染时调用），
        // 否则右面板隐藏时这些 Vec 会每帧无限累积（内存泄漏 + 命中错乱）。
        self.ai.ai_panel.history_item_regions.clear();
        self.ai.ai_panel.history_delete_regions.clear();
        self.ai.ai_panel.history_time_filter_regions.clear();

        // 确保垃圾桶矢量图标几何已创建
        self.ui.icons.ensure_created_from_target(target);

        unsafe {
            let (win_w, win_h) = self.ai.ai_panel.history_win_size;
            // 默认居中：窗口客户区中心减去浮窗一半尺寸
            let (px, py) = self.ai.ai_panel.history_win_pos.unwrap_or_else(|| {
                let cx = (self.win.window_width as f32 - win_w) / 2.0;
                let cy = (self.win.window_height as f32 - win_h) / 2.0;
                (cx.max(0.0), cy.max(0.0))
            });

            // 文本格式
            let title_format = match self.win.render_ctx.text_format_cache.get_format(
                12.0,
                DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            let text_format = match self.win.render_ctx.text_format_cache.get_format(
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };
            let small_format = match self.win.render_ctx.text_format_cache.get_format(
                10.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                Ok(f) => f,
                Err(_) => return,
            };

            // 画刷
            let bg_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.13, 0.13, 0.15, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let border_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.30, 0.30, 0.34, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let titlebar_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.17, 0.17, 0.20, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let white_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.92, 0.92, 0.92, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let dim_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.55, 0.58, 0.64, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let accent_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.0, 0.47, 0.83, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };

            let win_rect = D2D_RECT_F {
                left: px,
                top: py,
                right: px + win_w,
                bottom: py + win_h,
            };

            // 阴影（Glass 风格柔和投影）
            let _ = glass::draw_panel_shadow(
                target,
                &mut self.win.render_ctx.brush_cache,
                &win_rect,
                &self.win.theme.shadow,
                3.0,
            );
            // 主体背景 + 边框
            target.FillRectangle(&win_rect, &bg_brush);
            target.DrawRectangle(&win_rect, &border_brush, 1.0, None);

            // 注册浮窗整体命中区
            self.ai.ai_panel.history_win_region = Some((px, py, win_w, win_h));
            crate::hit_test::register_hit_region("ai:history_window", px, py, win_w, win_h);

            let titlebar_h = 32.0f32;
            let mut cy = py;

            // ===== 标题栏（拖动区 + 标题 + 关闭按钮）=====
            {
                let tb_rect = D2D_RECT_F {
                    left: px,
                    top: cy,
                    right: px + win_w,
                    bottom: cy + titlebar_h,
                };
                target.FillRectangle(&tb_rect, &titlebar_brush);
                // 标题栏下边框
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: px,
                        top: cy + titlebar_h - 1.0,
                        right: px + win_w,
                        bottom: cy + titlebar_h,
                    },
                    &border_brush,
                );
                // 标题文字
                let t: Vec<u16> = "历史对话".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &t,
                    &title_format,
                    &D2D_RECT_F {
                        left: px + 12.0,
                        top: cy,
                        right: px + win_w - 40.0,
                        bottom: cy + titlebar_h,
                    },
                    &white_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                // 标题栏命中区（拖动区，排除关闭按钮）
                self.ai.ai_panel.history_win_titlebar_region =
                    Some((px, cy, win_w - 40.0, titlebar_h));
                crate::hit_test::register_hit_region(
                    "ai:history_win_titlebar",
                    px,
                    cy,
                    win_w - 40.0,
                    titlebar_h,
                );
                // 关闭按钮（右上角 ×）
                let close_size = 28.0f32;
                let close_x = px + win_w - close_size - 4.0;
                let close_y = cy + (titlebar_h - close_size) / 2.0;
                let close_rect = D2D_RECT_F {
                    left: close_x,
                    top: close_y,
                    right: close_x + close_size,
                    bottom: close_y + close_size,
                };
                target.FillRectangle(&close_rect, &titlebar_brush);
                let x_text: Vec<u16> = "✕".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &x_text,
                    &text_format,
                    &D2D_RECT_F {
                        left: close_x,
                        top: close_y,
                        right: close_x + close_size,
                        bottom: close_y + close_size,
                    },
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                self.ai.ai_panel.history_win_close_region =
                    Some((close_x, close_y, close_size, close_size));
                crate::hit_test::register_hit_region(
                    "ai:history_win_close",
                    close_x,
                    close_y,
                    close_size,
                    close_size,
                );
                cy += titlebar_h;
            }

            let content_left = px + 10.0;
            let content_right = px + win_w - 10.0;
            let content_w = content_right - content_left;

            // ===== 搜索框 =====
            let search_h = 30.0f32;
            {
                cy += 8.0;
                let focused = self.ai.ai_panel.history_search_focused;
                let box_rect = D2D_RECT_F {
                    left: content_left,
                    top: cy,
                    right: content_right,
                    bottom: cy + search_h,
                };
                let box_bg = match self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.10, 0.10, 0.12, 1.0))
                {
                    Ok(b) => b,
                    Err(_) => return,
                };
                target.FillRectangle(&box_rect, &box_bg);
                // 聚焦时高亮边框
                let border = if focused {
                    &accent_brush
                } else {
                    &border_brush
                };
                draw_input_borders(
                    target,
                    box_rect.left,
                    box_rect.top,
                    content_w,
                    search_h,
                    border,
                );
                // 搜索文本或占位符
                let search_text = self.ai.ai_panel.history_search.clone();
                let display = if search_text.is_empty() {
                    "搜索对话标题...".to_string()
                } else {
                    search_text.clone()
                };
                let brush = if search_text.is_empty() {
                    &dim_brush
                } else {
                    &white_brush
                };
                let st: Vec<u16> = display.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &st,
                    &text_format,
                    &D2D_RECT_F {
                        left: content_left + 8.0,
                        top: cy,
                        right: content_right - 8.0,
                        bottom: cy + search_h,
                    },
                    brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                // 聚焦时绘制光标
                if focused && self.ai.ai_panel.caret_visible {
                    // 用字节索引切片更安全
                    let byte_caret = self.ai.ai_panel.history_search_caret.min(search_text.len());
                    let before = &search_text[..byte_caret];
                    let approx_x =
                        content_left + 8.0 + measure_text_width(&title_format, before) as f32;
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: approx_x,
                            top: cy + 6.0,
                            right: approx_x + 1.5,
                            bottom: cy + search_h - 6.0,
                        },
                        &white_brush,
                    );
                }
                self.ai.ai_panel.history_search_region =
                    Some((content_left, cy, content_w, search_h));
                crate::hit_test::register_hit_region(
                    "ai:history_search",
                    content_left,
                    cy,
                    content_w,
                    search_h,
                );
                cy += search_h + 8.0;
            }

            // ===== 时间筛选标签行 =====
            let filter_h = 24.0f32;
            {
                let btn_h = 20.0f32;
                let mut fx = content_left;
                for (fi, f) in crate::ai_panel::HistoryTimeFilter::ALL.iter().enumerate() {
                    let bw = 44.0f32;
                    let active = self.ai.ai_panel.history_time_filter == *f;
                    let bg = if active {
                        color_f(0.0, 0.47, 0.83, 1.0)
                    } else {
                        color_f(0.20, 0.21, 0.24, 1.0)
                    };
                    if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
                        target.FillRectangle(
                            &D2D_RECT_F {
                                left: fx,
                                top: cy,
                                right: fx + bw,
                                bottom: cy + btn_h,
                            },
                            &b,
                        );
                    }
                    let t: Vec<u16> = f.label().encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &t,
                        &small_format,
                        &D2D_RECT_F {
                            left: fx,
                            top: cy,
                            right: fx + bw,
                            bottom: cy + btn_h,
                        },
                        &white_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    self.ai
                        .ai_panel
                        .history_time_filter_regions
                        .push((fi, fx, cy, bw, btn_h));
                    crate::hit_test::register_hit_region(
                        format!("ai:history_time_filter:{}", f.label()),
                        fx,
                        cy,
                        bw,
                        btn_h,
                    );
                    fx += bw + 6.0;
                }
                cy += filter_h + 4.0;
            }

            // ===== 列表区域（可滚动）=====
            let footer_h = 30.0f32;
            let list_top = cy;
            let list_bottom = py + win_h - footer_h - 8.0;
            let list_h = (list_bottom - list_top).max(0.0);
            let item_h = 44.0f32;
            let now = crate::ai_panel::now_secs();
            let page_indices = self.ai.ai_panel.history_page_indices();

            // 列表裁剪
            let list_clip = D2D_RECT_F {
                left: px,
                top: list_top,
                right: px + win_w,
                bottom: list_bottom,
            };
            target.PushAxisAlignedClip(&list_clip, D2D1_ANTIALIAS_MODE_ALIASED);

            let scroll = self.ai.ai_panel.history_scroll;
            let mut iy = list_top - scroll;

            if page_indices.is_empty() {
                let hint = if self.ai.ai_panel.history_search.trim().is_empty() {
                    "暂无历史对话"
                } else {
                    "无匹配的对话"
                };
                let t: Vec<u16> = hint.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &t,
                    &small_format,
                    &D2D_RECT_F {
                        left: content_left,
                        top: iy + 12.0,
                        right: content_right,
                        bottom: iy + 32.0,
                    },
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            for hi in page_indices.iter().copied() {
                let hmeta = match self.ai.ai_panel.history.get(hi) {
                    Some(m) => m.clone(),
                    None => continue,
                };
                let item_rect = D2D_RECT_F {
                    left: content_left,
                    top: iy,
                    right: content_right,
                    bottom: iy + item_h - 4.0,
                };
                // 悬停高亮
                if self.ai.ai_panel.hover_tab == Some(hi) {
                    if let Ok(hl) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &color_f(0.18, 0.20, 0.26, 1.0))
                    {
                        target.FillRectangle(&item_rect, &hl);
                    }
                }

                let editing =
                    self.ai.ai_panel.history_editing_id.as_deref() == Some(hmeta.id.as_str());
                let del_w = 30.0f32;

                if editing {
                    // 编辑态：渲染输入框 + 文本 + 光标
                    let edit_rect = D2D_RECT_F {
                        left: item_rect.left + 4.0,
                        top: iy + 4.0,
                        right: item_rect.right - del_w - 8.0,
                        bottom: iy + item_h - 8.0,
                    };
                    let edit_bg = match self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &color_f(0.10, 0.10, 0.12, 1.0))
                    {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    target.FillRectangle(&edit_rect, &edit_bg);
                    draw_input_borders(
                        target,
                        edit_rect.left,
                        edit_rect.top,
                        edit_rect.right - edit_rect.left,
                        edit_rect.bottom - edit_rect.top,
                        &accent_brush,
                    );
                    let edit_text = self.ai.ai_panel.history_editing_text.clone();
                    let et: Vec<u16> = edit_text.encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &et,
                        &text_format,
                        &D2D_RECT_F {
                            left: edit_rect.left + 4.0,
                            top: edit_rect.top,
                            right: edit_rect.right - 4.0,
                            bottom: edit_rect.bottom,
                        },
                        &white_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    // 光标
                    if self.ai.ai_panel.caret_visible {
                        let byte_caret =
                            self.ai.ai_panel.history_editing_caret.min(edit_text.len());
                        let before = &edit_text[..byte_caret];
                        let cx = edit_rect.left + 4.0 + measure_text_width(&title_format, before);
                        target.FillRectangle(
                            &D2D_RECT_F {
                                left: cx,
                                top: edit_rect.top + 4.0,
                                right: cx + 1.5,
                                bottom: edit_rect.bottom - 4.0,
                            },
                            &white_brush,
                        );
                    }
                } else {
                    // 正常态：标题 + 相对时间
                    let title_left = item_rect.left + 8.0;
                    let title_right = item_rect.right - del_w - 8.0;
                    // 显示层缩写：标题超出可用宽度时截断加 "…"，完整标题保留在存储层
                    let title_shown = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .truncate_with_ellipsis(
                            &hmeta.title,
                            11.0,
                            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            title_right - title_left,
                        );
                    let title_text: Vec<u16> = title_shown.encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &title_text,
                        &text_format,
                        &D2D_RECT_F {
                            left: title_left,
                            top: iy + 4.0,
                            right: title_right,
                            bottom: iy + 22.0,
                        },
                        &white_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    let time_text: Vec<u16> = crate::ai_panel::relative_time(hmeta.updated_at, now)
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    target.DrawText(
                        &time_text,
                        &small_format,
                        &D2D_RECT_F {
                            left: item_rect.left + 8.0,
                            top: iy + 22.0,
                            right: item_rect.right - del_w - 8.0,
                            bottom: iy + 40.0,
                        },
                        &dim_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }

                // 删除按钮（SVG 垃圾桶图标）
                let dx = item_rect.right - del_w - 4.0;
                let del_rect = D2D_RECT_F {
                    left: dx,
                    top: iy + (item_h - 4.0 - 22.0) / 2.0,
                    right: dx + del_w,
                    bottom: iy + (item_h - 4.0 - 22.0) / 2.0 + 22.0,
                };
                if let Ok(b) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.45, 0.16, 0.16, 1.0))
                {
                    target.FillRectangle(&del_rect, &b);
                }
                // 垃圾桶图标居中绘制（14x14 图标在 30x22 按钮内居中）
                let trash_size = 14.0f32;
                let trash_x = dx + (del_w - trash_size) / 2.0;
                let trash_y = del_rect.top + (22.0 - trash_size) / 2.0;
                self.ui.icons.draw(
                    target,
                    crate::icons::IconKind::Trash,
                    trash_x,
                    trash_y,
                    trash_size,
                    trash_size,
                    &white_brush,
                );
                self.ai.ai_panel.history_delete_regions.push((
                    hi,
                    del_rect.left,
                    del_rect.top,
                    del_w,
                    22.0,
                ));
                crate::hit_test::register_hit_region(
                    format!("ai:history_delete:{}", hi),
                    del_rect.left,
                    del_rect.top,
                    del_w,
                    22.0,
                );

                // 条目命中区（标题区，排除删除按钮）
                self.ai.ai_panel.history_item_regions.push((
                    hi,
                    item_rect.left,
                    iy,
                    item_rect.right - item_rect.left - del_w - 8.0,
                    item_h - 4.0,
                ));
                crate::hit_test::register_hit_region(
                    format!("ai:history_item:{}", hi),
                    item_rect.left,
                    iy,
                    item_rect.right - item_rect.left - del_w - 8.0,
                    item_h - 4.0,
                );

                iy += item_h;
            }
            target.PopAxisAlignedClip();

            // 列表滚动条
            let total_h = page_indices.len() as f32 * item_h;
            let max_scroll = (total_h - list_h).max(0.0);
            self.ai.ai_panel.history_max_scroll = max_scroll;
            if max_scroll > 1.0 {
                let sb_w = 4.0f32;
                let sb_x = px + win_w - sb_w - 3.0;
                let track_h = list_h.max(8.0);
                if let Ok(tb) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.20, 0.20, 0.23, 1.0))
                {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: sb_x,
                            top: list_top,
                            right: sb_x + sb_w,
                            bottom: list_top + track_h,
                        },
                        &tb,
                    );
                }
                let thumb_h = (track_h * list_h / total_h).max(16.0).min(track_h);
                let thumb_y = list_top + (track_h - thumb_h) * (scroll / max_scroll);
                if let Ok(tb) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.45, 0.46, 0.52, 1.0))
                {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: sb_x,
                            top: thumb_y,
                            right: sb_x + sb_w,
                            bottom: thumb_y + thumb_h,
                        },
                        &tb,
                    );
                }
            }

            // ===== 底部：分页 + 清空 =====
            {
                let fy = py + win_h - footer_h;
                // 顶部分隔线
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: px,
                        top: fy,
                        right: px + win_w,
                        bottom: fy + 1.0,
                    },
                    &border_brush,
                );
                let pc = self.ai.ai_panel.history_page_count().max(1);
                let page = self.ai.ai_panel.history_page + 1;
                let btn_h = 20.0f32;
                let btn_y = fy + (footer_h - btn_h) / 2.0;
                // 上一页
                let prev_enabled = self.ai.ai_panel.history_page > 0;
                let pw = 56.0f32;
                let px0 = content_left;
                let prev_bg = if prev_enabled {
                    color_f(0.20, 0.21, 0.24, 1.0)
                } else {
                    color_f(0.14, 0.14, 0.16, 1.0)
                };
                if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &prev_bg) {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: px0,
                            top: btn_y,
                            right: px0 + pw,
                            bottom: btn_y + btn_h,
                        },
                        &b,
                    );
                }
                let t: Vec<u16> = "‹ 上一页".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &t,
                    &small_format,
                    &D2D_RECT_F {
                        left: px0,
                        top: btn_y,
                        right: px0 + pw,
                        bottom: btn_y + btn_h,
                    },
                    if prev_enabled {
                        &white_brush
                    } else {
                        &dim_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                self.ai.ai_panel.history_page_prev_region = if prev_enabled {
                    crate::hit_test::register_hit_region(
                        "ai:history_page_prev",
                        px0,
                        btn_y,
                        pw,
                        btn_h,
                    );
                    Some((px0, btn_y, pw, btn_h))
                } else {
                    None
                };
                // 页码
                let pi: Vec<u16> = format!("{}/{}", page, pc)
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                target.DrawText(
                    &pi,
                    &small_format,
                    &D2D_RECT_F {
                        left: px0 + pw + 8.0,
                        top: btn_y,
                        right: content_right - pw - 60.0,
                        bottom: btn_y + btn_h,
                    },
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                // 下一页
                let next_enabled = page < pc;
                let nx = content_right - pw - 48.0;
                let next_bg = if next_enabled {
                    color_f(0.20, 0.21, 0.24, 1.0)
                } else {
                    color_f(0.14, 0.14, 0.16, 1.0)
                };
                if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &next_bg) {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: nx,
                            top: btn_y,
                            right: nx + pw,
                            bottom: btn_y + btn_h,
                        },
                        &b,
                    );
                }
                let t: Vec<u16> = "下一页 ›".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &t,
                    &small_format,
                    &D2D_RECT_F {
                        left: nx,
                        top: btn_y,
                        right: nx + pw,
                        bottom: btn_y + btn_h,
                    },
                    if next_enabled {
                        &white_brush
                    } else {
                        &dim_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                self.ai.ai_panel.history_page_next_region = if next_enabled {
                    crate::hit_test::register_hit_region(
                        "ai:history_page_next",
                        nx,
                        btn_y,
                        pw,
                        btn_h,
                    );
                    Some((nx, btn_y, pw, btn_h))
                } else {
                    None
                };
                // 清空按钮（最右侧）
                let has_history = !self.ai.ai_panel.history.is_empty();
                let cw = 44.0f32;
                let cx = content_right - cw;
                let btn_bg = if has_history {
                    color_f(0.45, 0.16, 0.16, 1.0)
                } else {
                    color_f(0.25, 0.25, 0.28, 0.5)
                };
                if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &btn_bg) {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: cx,
                            top: btn_y,
                            right: cx + cw,
                            bottom: btn_y + btn_h,
                        },
                        &b,
                    );
                }
                let ct: Vec<u16> = "清空".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &ct,
                    &small_format,
                    &D2D_RECT_F {
                        left: cx,
                        top: btn_y,
                        right: cx + cw,
                        bottom: btn_y + btn_h,
                    },
                    if has_history {
                        &white_brush
                    } else {
                        &dim_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                self.ai.ai_panel.history_clear_all_region = if has_history {
                    crate::hit_test::register_hit_region(
                        "ai:history_clear_all",
                        cx,
                        btn_y,
                        cw,
                        btn_h,
                    );
                    Some((cx, btn_y, cw, btn_h))
                } else {
                    None
                };
            }
        }
    }
}

/// 用 DirectWrite 测量文本宽度（近似，用于光标定位）。
/// 失败时退回按字符数估算（等宽近似）。
fn measure_text_width(format: &IDWriteTextFormat, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    unsafe {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut metrics: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS =
            std::mem::zeroed();
        // 需要 IDWriteFactory 创建 TextLayout；此处用 overhang metrics 不可行，
        // 退回字符宽度估算：ASCII 6px，CJK 11px（与 11px 字号近似）。
        let _ = &mut metrics;
        let _ = format;
        let _ = wide;
        text.chars()
            .map(|c| if c.is_ascii() { 6.0 } else { 11.0 })
            .sum()
    }
}
