use super::*;

/// 设置页内容最大宽度：超宽窗口下内容不再无限拉伸
pub(super) const SETTINGS_CONTENT_MAX_W: f32 = 680.0;
/// 设置行行高
pub(super) const SETTINGS_ROW_H: f32 = 36.0;

impl EditorState {
    /// 绘制设置分组卡片底（圆角 + 1px 描边）
    pub(super) unsafe fn draw_settings_card(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        rect: &D2D_RECT_F,
    ) {
        let bg_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.155, 0.155, 0.17, 1.0))
            .unwrap();
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: *rect,
            radiusX: 6.0,
            radiusY: 6.0,
        };
        target.FillRoundedRectangle(&rounded, &bg_brush);
        let border_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.26, 0.26, 0.28, 1.0))
            .unwrap();
        target.DrawRoundedRectangle(&rounded, &border_brush, 1.0, None);
    }

    /// 绘制一行「标签左 / 值右」设置行；badge 为 Some 时值渲染为绿/灰徽章
    pub(super) unsafe fn draw_settings_row(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        label: &str,
        value: &str,
        rect: &D2D_RECT_F,
        badge: Option<bool>,
    ) {
        let label_format = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let label_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.62, 0.62, 0.65, 1.0))
            .unwrap();
        let padded_rect = D2D_RECT_F {
            left: rect.left + 16.0,
            top: rect.top,
            right: rect.right - 16.0,
            bottom: rect.bottom,
        };
        let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &label_wide,
            &label_format,
            &padded_rect,
            &label_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        match badge {
            Some(on) => {
                // 状态徽章：半透明底 pill + 彩色文字
                let (badge_color, badge_bg) = if on {
                    (
                        color_f(0.30, 0.80, 0.48, 1.0),
                        color_f(0.20, 0.72, 0.40, 0.16),
                    )
                } else {
                    (
                        color_f(0.62, 0.62, 0.65, 1.0),
                        color_f(0.55, 0.55, 0.58, 0.16),
                    )
                };
                let badge_w = value.chars().count() as f32 * 12.0 + 16.0;
                let badge_h = 20.0f32;
                let badge_cy = (rect.top + rect.bottom) / 2.0;
                let badge_rect = D2D_RECT_F {
                    left: padded_rect.right - badge_w,
                    top: badge_cy - badge_h / 2.0,
                    right: padded_rect.right,
                    bottom: badge_cy + badge_h / 2.0,
                };
                let badge_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &badge_bg)
                    .unwrap();
                let badge_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: badge_rect,
                    radiusX: 10.0,
                    radiusY: 10.0,
                };
                target.FillRoundedRectangle(&badge_rounded, &badge_bg_brush);
                let badge_text_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &badge_color)
                    .unwrap();
                let badge_format = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .get_format(
                        11.0,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                    )
                    .unwrap();
                let value_wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &value_wide,
                    &badge_format,
                    &badge_rect,
                    &badge_text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            None => {
                let value_format = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .get_format(
                        12.0,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        DWRITE_TEXT_ALIGNMENT_TRAILING.0 as u32,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                    )
                    .unwrap();
                let value_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.92, 0.92, 0.92, 1.0))
                    .unwrap();
                let value_wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &value_wide,
                    &value_format,
                    &padded_rect,
                    &value_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    /// 绘制计费统计模块
    pub(super) unsafe fn draw_billing_stats(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        w: f32,
        y: f32,
    ) -> f32 {
        let billing_history = &self.ui.app_settings.billing_history;

        // 计算总费用和总 token 数量
        let total_cost = billing_history.total_cost();
        let total_tokens = billing_history.total_tokens();
        let cache_hit_rate = billing_history.cache_hit_rate();

        // 绘制标题
        let title_format = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                14.0,
                DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let title_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.92, 0.92, 0.92, 1.0))
            .unwrap();
        let title_text: Vec<u16> = "计费统计".encode_utf16().chain(Some(0)).collect();
        let title_rect = D2D_RECT_F {
            left: x + 2.0,
            top: y,
            right: x + w,
            bottom: y + 20.0,
        };
        target.DrawText(
            &title_text,
            &title_format,
            &title_rect,
            &title_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        let mut current_y = y + 30.0;

        // 绘制总费用卡片
        let cost_card_rect = D2D_RECT_F {
            left: x,
            top: current_y,
            right: x + w,
            bottom: current_y + 80.0,
        };
        self.draw_settings_card(target, &cost_card_rect);

        // 绘制总费用
        let cost_format = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                24.0,
                DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let cost_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.30, 0.80, 0.48, 1.0))
            .unwrap();
        let cost_text = format!("¥ {:.2}", total_cost);
        let cost_wide: Vec<u16> = cost_text.encode_utf16().chain(Some(0)).collect();
        let cost_rect = D2D_RECT_F {
            left: cost_card_rect.left,
            top: cost_card_rect.top + 10.0,
            right: cost_card_rect.right,
            bottom: cost_card_rect.top + 50.0,
        };
        target.DrawText(
            &cost_wide,
            &cost_format,
            &cost_rect,
            &cost_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 绘制总费用标签
        let cost_label_format = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let cost_label_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.62, 0.62, 0.65, 1.0))
            .unwrap();
        let cost_label_text: Vec<u16> = "总费用（人民币）".encode_utf16().chain(Some(0)).collect();
        let cost_label_rect = D2D_RECT_F {
            left: cost_card_rect.left,
            top: cost_card_rect.top + 50.0,
            right: cost_card_rect.right,
            bottom: cost_card_rect.bottom - 10.0,
        };
        target.DrawText(
            &cost_label_text,
            &cost_label_format,
            &cost_label_rect,
            &cost_label_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        current_y += 90.0;

        // 绘制统计数据行
        let stats_rows = vec![
            ("总 Token 数量", format!("{}", total_tokens), None),
            (
                "缓存命中率",
                format!("{:.1}%", cache_hit_rate * 100.0),
                None,
            ),
        ];

        current_y = self.draw_settings_group(target, "统计数据", x, w, current_y, &stats_rows);

        current_y
    }

    /// 绘制一个设置分组：分组标题 + 卡片 + 若干设置行；返回卡片底部 Y
    pub(super) unsafe fn draw_settings_group(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        title: &str,
        x: f32,
        w: f32,
        y: f32,
        rows: &[(&str, String, Option<bool>)],
    ) -> f32 {
        // 分组标题（位于卡片上方）
        let group_title_format = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                12.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let group_title_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.55, 0.55, 0.58, 1.0))
            .unwrap();
        let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let title_rect = D2D_RECT_F {
            left: x + 2.0,
            top: y,
            right: x + w,
            bottom: y + 16.0,
        };
        target.DrawText(
            &title_wide,
            &group_title_format,
            &title_rect,
            &group_title_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 卡片底 + 描边
        let card_top = y + 24.0;
        let card_h = rows.len() as f32 * SETTINGS_ROW_H;
        let card_rect = D2D_RECT_F {
            left: x,
            top: card_top,
            right: x + w,
            bottom: card_top + card_h,
        };
        self.draw_settings_card(target, &card_rect);

        // 设置行 + 行间分隔线
        let row_sep_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.22, 0.22, 0.24, 1.0))
            .unwrap();
        for (i, (label, value, badge)) in rows.iter().enumerate() {
            let row_top = card_top + i as f32 * SETTINGS_ROW_H;
            let row_rect = D2D_RECT_F {
                left: x,
                top: row_top,
                right: x + w,
                bottom: row_top + SETTINGS_ROW_H,
            };
            self.draw_settings_row(target, label, value, &row_rect, *badge);
            if i + 1 < rows.len() {
                let sep_rect = D2D_RECT_F {
                    left: x + 16.0,
                    top: row_top + SETTINGS_ROW_H,
                    right: x + w - 16.0,
                    bottom: row_top + SETTINGS_ROW_H + 1.0,
                };
                target.FillRectangle(&sep_rect, &row_sep_brush);
            }
        }
        card_top + card_h
    }

    /// 渲染空态占位（外观 / 远程等尚未实现的页面）
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_settings_empty_state(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        title: &str,
        desc: &str,
    ) {
        unsafe {
            // 占位块整体高度：图标 40 + 间距 16 + 标题 20 + 间距 6 + 说明 16
            let block_h = 98.0f32;
            let top = (y + (h - block_h) / 2.0).max(y + 20.0);
            let cx = x + w / 2.0;

            // 矢量时钟图标：圆环 + 两根指针
            let icon_color = color_f(0.35, 0.35, 0.38, 1.0);
            let icon_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &icon_color)
                .unwrap();
            let icon_cy = top + 20.0;
            let ring = windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                point: D2D_POINT_2F { x: cx, y: icon_cy },
                radiusX: 20.0,
                radiusY: 20.0,
            };
            target.DrawEllipse(&ring, &icon_brush, 1.5, None);
            target.DrawLine(
                D2D_POINT_2F { x: cx, y: icon_cy },
                D2D_POINT_2F {
                    x: cx,
                    y: icon_cy - 9.0,
                },
                &icon_brush,
                1.5,
                None,
            );
            target.DrawLine(
                D2D_POINT_2F { x: cx, y: icon_cy },
                D2D_POINT_2F {
                    x: cx + 7.0,
                    y: icon_cy + 4.0,
                },
                &icon_brush,
                1.5,
                None,
            );

            // 标题
            let title_format = self
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
            let title_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.75, 0.75, 0.78, 1.0))
                .unwrap();
            let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            let title_rect = D2D_RECT_F {
                left: x,
                top: top + 56.0,
                right: x + w,
                bottom: top + 76.0,
            };
            target.DrawText(
                &title_wide,
                &title_format,
                &title_rect,
                &title_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 说明
            let desc_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let desc_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.52, 0.52, 0.55, 1.0))
                .unwrap();
            let desc_wide: Vec<u16> = desc.encode_utf16().chain(Some(0)).collect();
            let desc_rect = D2D_RECT_F {
                left: x,
                top: top + 82.0,
                right: x + w,
                bottom: top + 98.0,
            };
            target.DrawText(
                &desc_wide,
                &desc_format,
                &desc_rect,
                &desc_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// 渲染设置面板：左侧导航 + 右侧内容
    pub(super) fn render_settings_sidebar(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        unsafe {
            // 公共文本格式
            let nav_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let label_format = self
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
            let input_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let title_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    16.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            let button_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();

            // 整体背景（右侧内容区）
            let content_bg = color_f(0.12, 0.12, 0.12, 1.0);
            let content_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &content_bg)
                .unwrap();
            let content_bg_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&content_bg_rect, &content_bg_brush);

            // 左侧导航栏布局：钳制宽度，防止窄面板下导航占满整区、内容区算出负宽
            let nav_w = self
                .ui
                .settings_panel
                .nav_width
                .min((width - 160.0).max(96.0));
            let nav_x = x;
            let nav_y = y;
            let nav_h = height;

            // 导航栏背景（稍亮，与右侧区分）
            let nav_bg = color_f(0.10, 0.10, 0.10, 1.0);
            let nav_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &nav_bg)
                .unwrap();
            let nav_bg_rect = D2D_RECT_F {
                left: nav_x,
                top: nav_y,
                right: nav_x + nav_w,
                bottom: nav_y + nav_h,
            };
            target.FillRectangle(&nav_bg_rect, &nav_bg_brush);

            // 右侧分隔线
            let sep_color = color_f(0.2, 0.2, 0.2, 1.0);
            let sep_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &sep_color)
                .unwrap();
            let sep_rect = D2D_RECT_F {
                left: nav_x + nav_w,
                top: nav_y,
                right: nav_x + nav_w + 1.0,
                bottom: nav_y + nav_h,
            };
            target.FillRectangle(&sep_rect, &sep_brush);

            // 调整手柄：悬停或拖拽时高亮
            if self.ui.settings_panel.hover_nav_resize || self.ui.settings_panel.nav_resizing {
                let handle_color = color_f(0.0, 0.47, 0.83, 1.0);
                let handle_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &handle_color)
                    .unwrap();
                let handle_rect = D2D_RECT_F {
                    left: nav_x + nav_w - 1.0,
                    top: nav_y,
                    right: nav_x + nav_w + 1.0,
                    bottom: nav_y + nav_h,
                };
                target.FillRectangle(&handle_rect, &handle_brush);
            }

            // 导航标题
            let nav_title: Vec<u16> = "设置".encode_utf16().chain(Some(0)).collect();
            let nav_title_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    16.0,
                    DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let nav_title_rect = D2D_RECT_F {
                left: nav_x + 12.0,
                top: nav_y + 16.0,
                right: nav_x + nav_w,
                bottom: nav_y + 48.0,
            };
            target.DrawText(
                &nav_title,
                &nav_title_format,
                &nav_title_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 导航项
            self.ui.settings_panel.clear_regions();
            let tabs = crate::settings::SettingsTab::ALL;
            let nav_item_h = 32.0;
            let nav_item_start_y = nav_y + 60.0;
            for (i, tab) in tabs.iter().enumerate() {
                let item_y = nav_item_start_y + i as f32 * nav_item_h;
                let is_active = self.ui.settings_panel.active_tab == *tab;
                let is_hover = self.ui.settings_panel.hover_tab == Some(*tab);

                // 悬停 / 激活：内缩圆角色块（与全局菜单风格统一）
                if is_active || is_hover {
                    let item_bg = if is_active {
                        color_f(0.18, 0.30, 0.45, 1.0)
                    } else {
                        color_f(0.20, 0.20, 0.22, 1.0)
                    };
                    let item_bg_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &item_bg)
                        .unwrap();
                    let item_rect = D2D_RECT_F {
                        left: nav_x + 8.0,
                        top: item_y + 2.0,
                        right: nav_x + nav_w - 8.0,
                        bottom: item_y + nav_item_h - 2.0,
                    };
                    let item_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: item_rect,
                        radiusX: 4.0,
                        radiusY: 4.0,
                    };
                    target.FillRoundedRectangle(&item_rounded, &item_bg_brush);
                }

                // 激活状态左侧强调短条
                if is_active {
                    let accent = color_f(0.0, 0.47, 0.83, 1.0);
                    let accent_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &accent)
                        .unwrap();
                    let accent_rect = D2D_RECT_F {
                        left: nav_x + 8.0,
                        top: item_y + 8.0,
                        right: nav_x + 11.0,
                        bottom: item_y + nav_item_h - 8.0,
                    };
                    let accent_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: accent_rect,
                        radiusX: 1.5,
                        radiusY: 1.5,
                    };
                    target.FillRoundedRectangle(&accent_rounded, &accent_brush);
                }

                let item_text_color = if is_active {
                    color_f(1.0, 1.0, 1.0, 1.0)
                } else {
                    color_f(0.75, 0.75, 0.75, 1.0)
                };
                let item_text_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &item_text_color)
                    .unwrap();
                let item_text: Vec<u16> = tab.label().encode_utf16().chain(Some(0)).collect();
                let item_text_rect = D2D_RECT_F {
                    left: nav_x + 20.0,
                    top: item_y,
                    right: nav_x + nav_w - 8.0,
                    bottom: item_y + nav_item_h,
                };
                target.DrawText(
                    &item_text,
                    &nav_format,
                    &item_text_rect,
                    &item_text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                self.ui
                    .settings_panel
                    .add_tab_region(*tab, nav_x, item_y, nav_w, nav_item_h);
            }

            // 右侧内容区域
            let content_x = nav_x + nav_w + 1.0;
            let content_y = nav_y;
            let content_w = (width - nav_w - 1.0).max(0.0);
            let content_h = height;

            // 内容列：限制最大宽度防止超宽窗口下无限拉伸，空间充裕时整体居中
            let page_w = (content_w - 48.0).clamp(0.0, SETTINGS_CONTENT_MAX_W);
            let page_x = content_x + (content_w - page_w) / 2.0;

            // 标题栏：页面标题 + 一行灰色描述
            let (page_title, page_desc) = match self.ui.settings_panel.active_tab {
                crate::settings::SettingsTab::General => ("通用", "外观、字体与自动保存偏好"),
                crate::settings::SettingsTab::Models => {
                    if self.ui.settings_panel.model_editing {
                        if self.ui.settings_panel.is_adding_model() {
                            ("添加模型", "配置新模型的连接与参数")
                        } else {
                            ("编辑模型", "编辑模型连接与参数")
                        }
                    } else {
                        ("模型", "管理 AI 模型配置")
                    }
                }
                crate::settings::SettingsTab::Ai => ("AI", "AI 接口配置"),
                crate::settings::SettingsTab::Playbook => ("策略", "管理 AI 沉淀策略库"),
                crate::settings::SettingsTab::Appearance => ("外观", "主题与界面自定义"),
                crate::settings::SettingsTab::Remote => ("远程", "SSH 与容器远程连接"),
                crate::settings::SettingsTab::Update => ("更新", "版本与更新策略"),
            };
            let page_title_wide: Vec<u16> = page_title.encode_utf16().chain(Some(0)).collect();
            let page_title_rect = D2D_RECT_F {
                left: page_x,
                top: content_y + 24.0,
                right: page_x + page_w,
                bottom: content_y + 48.0,
            };
            target.DrawText(
                &page_title_wide,
                &title_format,
                &page_title_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 页面描述
            let page_desc_wide: Vec<u16> = page_desc.encode_utf16().chain(Some(0)).collect();
            let page_desc_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.52, 0.52, 0.55, 1.0))
                .unwrap();
            let page_desc_rect = D2D_RECT_F {
                left: page_x,
                top: content_y + 48.0,
                right: page_x + page_w,
                bottom: content_y + 64.0,
            };
            target.DrawText(
                &page_desc_wide,
                &label_format,
                &page_desc_rect,
                &page_desc_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 标题下方分隔线
            let title_sep_rect = D2D_RECT_F {
                left: page_x,
                top: content_y + 72.0,
                right: page_x + page_w,
                bottom: content_y + 73.0,
            };
            target.FillRectangle(&title_sep_rect, &sep_brush);

            // 渲染当前激活页面的内容（宽度约束到最大内容宽度）
            let page_y = content_y + 96.0;

            // 兜底裁剪：保证子页面元素（固定宽按钮/卡片、超长列表等）不画到设置区外
            let page_clip = D2D_RECT_F {
                left: content_x,
                top: content_y,
                right: content_x + content_w,
                bottom: content_y + content_h,
            };
            target.PushAxisAlignedClip(&page_clip, D2D1_ANTIALIAS_MODE_ALIASED);

            match self.ui.settings_panel.active_tab {
                crate::settings::SettingsTab::General => {
                    self.render_general_settings(target, page_x, page_w, page_y);
                }
                crate::settings::SettingsTab::Models => {
                    if self.ui.settings_panel.model_editing {
                        // 编辑/新建模型：顶部「返回模型列表」按钮 + 内嵌 AI 配置表单
                        let back_h = self.render_model_edit_back_button(
                            target,
                            page_x,
                            page_y,
                            &button_format,
                        );
                        let form_y = page_y + back_h + 10.0;
                        let form_avail = (content_h - 96.0 - back_h - 10.0).max(60.0);
                        self.render_ai_settings_fields(
                            target,
                            page_x,
                            page_w,
                            form_y,
                            0.0,
                            20.0,
                            32.0,
                            12.0,
                            label_format,
                            input_format,
                            button_format,
                            text_brush,
                            form_avail,
                        );
                    } else {
                        let label_format_clone = label_format.clone();
                        let input_format_clone = input_format.clone();
                        let button_format_clone = button_format.clone();
                        let title_format_clone = title_format.clone();
                        self.render_models_management(
                            target,
                            page_x,
                            page_w,
                            page_y,
                            0.0,
                            label_format_clone,
                            input_format_clone,
                            button_format_clone,
                            title_format_clone,
                            text_brush,
                        );
                    }
                }
                crate::settings::SettingsTab::Appearance => {
                    self.render_appearance_settings(
                        target,
                        page_x,
                        page_w,
                        page_y,
                        &label_format,
                        text_brush,
                    );
                }
                crate::settings::SettingsTab::Remote => {
                    self.render_settings_empty_state(
                        target,
                        page_x,
                        page_y,
                        page_w,
                        (content_h - 96.0).max(120.0),
                        "远程开发即将推出",
                        "SSH 与容器远程连接功能正在开发中",
                    );
                }
                crate::settings::SettingsTab::Update => {
                    self.render_update_settings(target, page_x, page_w, page_y);
                }
                crate::settings::SettingsTab::Ai => {}
                crate::settings::SettingsTab::Playbook => {
                    self.render_playbook_settings(target, page_x, page_w, page_y);
                }
            }

            target.PopAxisAlignedClip();
        }
    }

    /// 渲染"策略"标签页内容（AI 沉淀策略库管理）
    pub(super) fn render_playbook_settings(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
    ) {
        use aether_render::d2d::factory::color_f;
        use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
        use windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE;
        use windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL;

        unsafe {
            let mut cy = start_y;
            let small_format = self
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
            let white_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.9, 0.9, 0.9, 1.0))
                .unwrap();
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.55, 0.56, 0.60, 1.0))
                .unwrap();

            // 标题
            let header: Vec<u16> = format!(
                "已沉淀策略（共 {} 条）",
                self.ai.ai_panel.playbook_items.len()
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            let header_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + width,
                bottom: cy + 24.0,
            };
            target.DrawText(
                &header,
                &small_format,
                &header_rect,
                &white_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += 28.0;

            // 策略列表
            let item_h = 30.0f32;
            for bullet in self.ai.ai_panel.playbook_items.iter() {
                let line: Vec<u16> = format!(
                    "[{}] {}  (+{}/-{})",
                    bullet.section, bullet.content, bullet.helpful_count, bullet.harmful_count
                )
                .encode_utf16()
                .chain(Some(0))
                .collect();
                let line_rect = D2D_RECT_F {
                    left: x + 6.0,
                    top: cy + 3.0,
                    right: x + width - 6.0,
                    bottom: cy + item_h - 3.0,
                };
                target.DrawText(
                    &line,
                    &small_format,
                    &line_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += item_h;
            }

            if self.ai.ai_panel.playbook_items.is_empty() {
                let empty: Vec<u16> = "暂无沉淀策略，对话归档后会自动提炼"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let empty_rect = D2D_RECT_F {
                    left: x + 6.0,
                    top: cy + 3.0,
                    right: x + width - 6.0,
                    bottom: cy + item_h,
                };
                target.DrawText(
                    &empty,
                    &small_format,
                    &empty_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    /// 渲染"通用"标签页内容（主题 / 字体大小 / 自动保存等概览；
    /// 「默认启动模式」行可点击切换 开发者/智能体，持久化后重启生效）
    pub(super) fn render_general_settings(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
    ) {
        unsafe {
            let mut cy = start_y;

            // 分组「外观与字体」
            let theme_label = if self.ui.app_settings.ui.theme.is_empty() {
                "默认深色".to_string()
            } else {
                self.ui.app_settings.ui.theme.clone()
            };
            let font_size = if self.ui.app_settings.ui.font_size == 0 {
                14
            } else {
                self.ui.app_settings.ui.font_size
            };
            let appearance_rows = [
                ("主题", theme_label, None),
                ("编辑器字体大小", format!("{} px", font_size), None),
            ];
            cy = self.draw_settings_group(target, "外观与字体", x, width, cy, &appearance_rows);
            cy += 20.0;

            // 分组「编辑器模式」
            let mode_label = if self.editor_mode.is_agent() {
                "智能体模式"
            } else {
                "开发者模式"
            };
            let default_is_agent = self.ui.app_settings.ui.editor_mode == "agent";
            let mode_rows = [
                ("当前模式", mode_label.to_string(), None),
                (
                    "默认启动模式",
                    format!(
                        "{}（点击切换）",
                        if default_is_agent {
                            "智能体模式"
                        } else {
                            "开发者模式"
                        }
                    ),
                    None,
                ),
            ];
            // 「默认启动模式」是第 2 行：卡片顶 = 分组 y + 24，行高 SETTINGS_ROW_H。
            // 注册命中区供点击切换（与外观页任务栏开关同一交互模式）
            self.ui.settings_panel.default_mode_toggle_region =
                Some((x, cy + 24.0 + SETTINGS_ROW_H, width, SETTINGS_ROW_H));
            crate::hit_test::register_hit_region(
                "settings:default_mode",
                x,
                cy + 24.0 + SETTINGS_ROW_H,
                width,
                SETTINGS_ROW_H,
            );
            cy = self.draw_settings_group(target, "编辑器模式", x, width, cy, &mode_rows);
            cy += 20.0;

            // 分组「自动保存」
            let auto_save = &self.ui.app_settings.auto_save;
            let auto_save_rows = [
                (
                    "自动保存",
                    if auto_save.enabled {
                        "已启用".to_string()
                    } else {
                        "已禁用".to_string()
                    },
                    Some(auto_save.enabled),
                ),
                ("保存防抖", format!("{} ms", auto_save.debounce_ms), None),
                (
                    "失焦自动保存",
                    if auto_save.focus_loss_save {
                        "是".to_string()
                    } else {
                        "否".to_string()
                    },
                    Some(auto_save.focus_loss_save),
                ),
            ];
            cy = self.draw_settings_group(target, "自动保存", x, width, cy, &auto_save_rows);
            cy += 16.0;

            // 计费统计模块
            cy = self.draw_billing_stats(target, x, width, cy);
            cy += 16.0;

            // 提示
            let hint_format = self
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
            let hint_text: Vec<u16> = "更多通用选项（主题切换、字体调整等）将在后续版本提供"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let hint_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.50, 0.50, 0.53, 1.0))
                .unwrap();
            let hint_rect = D2D_RECT_F {
                left: x + 2.0,
                top: cy,
                right: x + width,
                bottom: cy + 16.0,
            };
            target.DrawText(
                &hint_text,
                &hint_format,
                &hint_rect,
                &hint_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// 渲染"外观"标签页内容（任务栏显示开关等）
    pub(super) fn render_appearance_settings(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
        label_format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        unsafe {
            let mut cy = start_y;

            // 分组标题「窗口」
            let group_title: Vec<u16> = "窗口".encode_utf16().chain(Some(0)).collect();
            let group_title_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            let group_title_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + width,
                bottom: cy + 22.0,
            };
            target.DrawText(
                &group_title,
                &group_title_format,
                &group_title_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += 28.0;

            // 任务栏开关
            let show_taskbar = self.ui.app_settings.ui.show_taskbar_when_maximized;
            let region = self.render_pill_switch(
                target,
                x,
                cy,
                show_taskbar,
                "最大化时显示 Windows 任务栏",
                label_format,
                text_brush,
            );
            self.ui.settings_panel.taskbar_toggle_region = Some(region);
            cy += 20.0 + 8.0;

            // 描述文字
            let desc_format = self
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
            let desc_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.50, 0.50, 0.53, 1.0))
                .unwrap();
            let desc_text: Vec<u16> =
                "开启后，窗口最大化时底部会保留 Windows 任务栏可见；关闭则全屏覆盖。"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            let desc_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + width,
                bottom: cy + 16.0,
            };
            target.DrawText(
                &desc_text,
                &desc_format,
                &desc_rect,
                &desc_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }
}
