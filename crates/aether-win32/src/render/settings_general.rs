use super::app_theme;
use super::*;

/// 设置页内容最大宽度：超宽窗口下内容不再无限拉伸
pub(super) const SETTINGS_CONTENT_MAX_W: f32 = 680.0;
/// 设置行行高
pub(super) const SETTINGS_ROW_H: f32 = 36.0;

impl EditorState {
    /// 绘制设置分组卡片底（圆角 + 1px 描边，白底薄荷绿主题）
    pub(super) unsafe fn draw_settings_card(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        rect: &D2D_RECT_F,
    ) {
        let bg_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::surface())
            .unwrap();
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: *rect,
            radiusX: 10.0,
            radiusY: 10.0,
        };
        target.FillRoundedRectangle(&rounded, &bg_brush);
        let border_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::border())
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
            .get_brush(target, &app_theme::text_secondary())
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
                    (app_theme::accent_text(), app_theme::accent_soft())
                } else {
                    (app_theme::text_muted(), app_theme::surface_alt())
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
                    .get_brush(target, &app_theme::text_primary())
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
            .get_brush(target, &app_theme::text_secondary())
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
            .get_brush(target, &app_theme::separator())
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

    /// 渲染设置面板：左侧导航 + 右侧内容
    /// 设置独立整页：全窗覆盖（标题栏下方）+ 顶部「← 回到应用」返回条 + 设置内容。
    /// 几何与 [`EditorState::settings_page_region`] / 返回按钮命中区严格一致。
    pub(super) fn render_settings_fullpage(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    ) {
        unsafe {
            let layout = self.ui.layout.clone();
            let title_h = layout.title_bar_region().height;
            let status_h = layout.status_bar_region().height;
            let win_w = layout.window_width;
            let page_h = (layout.window_height - title_h - status_h).max(0.0);
            // 整页背景：白底 → 薄荷绿渐变
            let page_rect = D2D_RECT_F {
                left: 0.0,
                top: title_h,
                right: win_w,
                bottom: title_h + page_h,
            };
            app_theme::fill_vertical_gradient(
                target,
                &page_rect,
                app_theme::page_bg_top(),
                app_theme::page_bg_bottom(),
            );
            // 顶部返回条
            let bar = self.settings_back_bar_region(&layout);
            if let Ok(bb) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::surface())
            {
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: bar.x,
                        top: bar.y,
                        right: bar.x + bar.width,
                        bottom: bar.y + bar.height,
                    },
                    &bb,
                );
            }
            if let Ok(sep) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::border())
            {
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: 0.0,
                        top: bar.y + bar.height - 1.0,
                        right: win_w,
                        bottom: bar.y + bar.height,
                    },
                    &sep,
                );
            }
            // 「← 回到应用」按钮
            let btn = self.settings_back_button_region(&layout);
            let btn_rect = D2D_RECT_F {
                left: btn.x,
                top: btn.y,
                right: btn.x + btn.width,
                bottom: btn.y + btn.height,
            };
            if let Ok(bbg) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_soft())
            {
                let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: btn_rect,
                    radiusX: 8.0,
                    radiusY: 8.0,
                };
                target.FillRoundedRectangle(&rounded, &bbg);
            }
            if let Ok(btn_format) = self.win.render_ctx.text_format_cache.get_format(
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                let btn_text: Vec<u16> = "← 回到应用".encode_utf16().chain(Some(0)).collect();
                if let Ok(fg) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::accent_text())
                {
                    target.DrawText(
                        &btn_text,
                        &btn_format,
                        &btn_rect,
                        &fg,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
            crate::hit_test::register_hit_region(
                "settings:back_to_app",
                btn.x,
                btn.y,
                btn.width,
                btn.height,
            );
            // 设置内容（导航 + 当前页）
            let region = self.settings_page_region(&layout);
            if let Ok(tb) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_primary())
            {
                self.render_settings_sidebar(
                    target,
                    region.x,
                    region.y,
                    region.width,
                    region.height,
                    &tb,
                );
            }
        }
    }

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

            // 整体背景（右侧内容区）由 fullpage 层的白→薄荷渐变提供，这里不再覆盖

            // 左侧导航栏布局：钳制宽度，防止窄面板下导航占满整区、内容区算出负宽
            let nav_w = self
                .ui
                .settings_panel
                .nav_width
                .min((width - 160.0).max(96.0));
            let nav_x = x;
            let nav_y = y;
            let nav_h = height;

            // 导航栏背景（略偏薄荷白，与内容区渐变区分）
            let nav_bg = app_theme::nav_bg();
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
            let sep_color = app_theme::border();
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
                let handle_color = app_theme::accent();
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

                // 悬停 / 激活：内缩圆角色块（薄荷绿主题）
                if is_active || is_hover {
                    let item_bg = if is_active {
                        app_theme::accent_soft()
                    } else {
                        app_theme::surface_alt()
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
                    let accent = app_theme::accent();
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
                    app_theme::accent_text()
                } else {
                    app_theme::text_secondary()
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
                crate::settings::SettingsTab::Memory => {
                    ("记忆", "查看和管理 AetherStudio 在本机保存的长期记忆")
                }
                crate::settings::SettingsTab::Appearance => ("外观", "主题与界面自定义"),
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
                .get_brush(target, &app_theme::text_muted())
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
                crate::settings::SettingsTab::Update => {
                    self.render_update_settings(target, page_x, page_w, page_y);
                }
                crate::settings::SettingsTab::Ai => {}
                crate::settings::SettingsTab::Memory => {
                    let avail_h = (content_h - 96.0).max(60.0);
                    self.render_memory_settings(target, page_x, page_w, page_y, avail_h);
                }
            }

            target.PopAxisAlignedClip();
        }
    }

    // ============================================================================
    // 记忆页（全局记忆 + 按工作区隔离的项目记忆）
    // ============================================================================

    /// 重新加载记忆库缓存（记忆页数据源）
    pub(crate) fn reload_memory_cache(&mut self) {
        let entries = self
            .ai
            .ai_panel
            .warm_data_store
            .as_ref()
            .and_then(|w| w.list_all_memories().ok())
            .unwrap_or_default();
        self.ui.settings_panel.memory_entries = entries;
        self.ui.settings_panel.memory_dirty = false;
    }

    /// 记忆分组视图：全局记忆统计 + 每个工作区的项目记忆统计
    fn memory_group_views(
        &self,
    ) -> (
        (usize, u64),
        Vec<(crate::settings::MemoryDetailTarget, usize, u64, bool)>,
    ) {
        use aether_ai_panel::memory_store::MemoryScope;
        use std::collections::HashMap;

        let current_hash = self
            .ai
            .ai_panel
            .warm_data_store
            .as_ref()
            .map(|w| w.current_workspace_hash())
            .unwrap_or_default();
        let current_name = self
            .ai
            .ai_panel
            .warm_data_store
            .as_ref()
            .map(|w| w.current_workspace_name())
            .unwrap_or_default();

        let mut global_count = 0usize;
        let mut global_updated = 0u64;
        let mut groups: HashMap<String, (String, usize, u64)> = HashMap::new();
        for b in &self.ui.settings_panel.memory_entries {
            match b.scope {
                MemoryScope::Global => {
                    global_count += 1;
                    global_updated = global_updated.max(b.updated_at);
                }
                MemoryScope::Project => {
                    if b.workspace_hash.is_empty() {
                        // 无归属哈希的脏数据按全局展示，避免记忆“消失”
                        global_count += 1;
                        global_updated = global_updated.max(b.updated_at);
                        continue;
                    }
                    let entry = groups
                        .entry(b.workspace_hash.clone())
                        .or_insert_with(|| (String::new(), 0, 0));
                    if entry.0.is_empty() && !b.workspace_name.is_empty() {
                        entry.0 = b.workspace_name.clone();
                    }
                    entry.1 += 1;
                    entry.2 = entry.2.max(b.updated_at);
                }
            }
        }

        // 工作区名称兜底：当前工作区 → 最近项目 → 哈希短名
        let recent: Vec<(String, String)> = self
            .ui
            .recent_projects
            .list()
            .into_iter()
            .map(|p| {
                (
                    crate::editor::files::workspace_path_hash(std::path::Path::new(&p.path)),
                    p.name,
                )
            })
            .collect();
        let mut list: Vec<(crate::settings::MemoryDetailTarget, usize, u64, bool)> = groups
            .into_iter()
            .map(|(hash, (name, count, updated))| {
                let is_current = hash == current_hash;
                let resolved = if !name.is_empty() {
                    name
                } else if is_current && !current_name.is_empty() {
                    current_name.clone()
                } else if let Some((_, n)) = recent.iter().find(|(h, _)| *h == hash) {
                    n.clone()
                } else {
                    crate::settings::short_workspace_hash(&hash)
                };
                (
                    crate::settings::MemoryDetailTarget::Project {
                        hash,
                        name: resolved,
                    },
                    count,
                    updated,
                    is_current,
                )
            })
            .collect();
        list.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.title().cmp(&b.0.title()))
        });
        ((global_count, global_updated), list)
    }

    /// 渲染"记忆"标签页（列表视图 / 详情视图，支持滚动）
    pub(super) fn render_memory_settings(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
        avail_h: f32,
    ) {
        use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

        unsafe {
            if self.ui.settings_panel.memory_dirty {
                self.reload_memory_cache();
            }
            let scroll = self.ui.settings_panel.scroll_offset;
            let clip = D2D_RECT_F {
                left: x - 10.0,
                top: start_y - 4.0,
                right: x + width + 10.0,
                bottom: start_y + avail_h,
            };
            target.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);

            let detail = self.ui.settings_panel.memory_detail.clone();
            let end_y = match detail {
                Some(detail) => {
                    self.render_memory_detail(target, x, width, start_y - scroll, &detail)
                }
                None => self.render_memory_list(target, x, width, start_y - scroll),
            };

            target.PopAxisAlignedClip();

            // 滚动高度与滚动条（与 AI 表单页一致的手感）
            let total_content = (end_y + scroll) - start_y;
            let max_scroll = (total_content - avail_h).max(0.0);
            self.ui.settings_panel.content_height = max_scroll;
            if self.ui.settings_panel.scroll_offset > max_scroll {
                self.ui.settings_panel.scroll_offset = max_scroll;
            }
            if max_scroll > 0.0 && total_content > 0.0 {
                let sb_w = 6.0_f32;
                let sb_x = x + width + 4.0;
                let visible_ratio = (avail_h / total_content).clamp(0.1, 1.0);
                let thumb_h = (avail_h * visible_ratio).max(30.0);
                let scroll_ratio =
                    (self.ui.settings_panel.scroll_offset / max_scroll).clamp(0.0, 1.0);
                let thumb_y = start_y + (avail_h - thumb_h) * scroll_ratio;
                let thumb_color = color_f(0.55, 0.66, 0.60, 0.9);
                if let Ok(brush) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &thumb_color)
                {
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: sb_x,
                            top: thumb_y,
                            right: sb_x + sb_w,
                            bottom: thumb_y + thumb_h,
                        },
                        radiusX: 3.0,
                        radiusY: 3.0,
                    };
                    target.FillRoundedRectangle(&rounded, &brush);
                }
            }
        }
    }

    /// 记忆列表视图（记忆行为 / 全局记忆 / 项目记忆）
    unsafe fn render_memory_list(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
    ) -> f32 {
        use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
        use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD;

        let section_fmt = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                13.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let title_fmt = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                13.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let desc_fmt = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let title_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::text_primary())
            .unwrap();
        let desc_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::text_muted())
            .unwrap();

        let mut cy = start_y;

        // ---- 记忆行为 ----
        self.draw_memory_section_title(
            target,
            "记忆行为",
            x,
            width,
            cy,
            &section_fmt,
            &title_brush,
        );
        cy += 24.0;
        let behavior_h = 60.0 * 2.0;
        let behavior_rect = D2D_RECT_F {
            left: x,
            top: cy,
            right: x + width,
            bottom: cy + behavior_h,
        };
        self.draw_settings_card(target, &behavior_rect);

        let toggles = self.ui.app_settings.memory.clone();
        self.render_memory_behavior_row(
            target,
            x,
            cy,
            width,
            60.0,
            true,
            "全局记忆",
            "开启后，AetherStudio 会在所有项目中读取和更新通用经验；关闭后不再使用或沉淀全局记忆。",
            toggles.global_enabled,
            crate::settings::MemoryHit::ToggleGlobal,
            &title_fmt,
            &desc_fmt,
            &title_brush,
            &desc_brush,
        );
        // 行间分隔线
        let sep = app_theme::separator();
        if let Ok(brush) = self.win.render_ctx.brush_cache.get_brush(target, &sep) {
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x + 16.0,
                    top: cy + 60.0,
                    right: x + width - 16.0,
                    bottom: cy + 61.0,
                },
                &brush,
            );
        }
        self.render_memory_behavior_row(
            target,
            x,
            cy + 60.0,
            width,
            60.0,
            false,
            "项目记忆",
            "开启后，按工作区读写项目专属经验；关闭后不会读取或更新任何项目的记忆。",
            toggles.project_enabled,
            crate::settings::MemoryHit::ToggleProject,
            &title_fmt,
            &desc_fmt,
            &title_brush,
            &desc_brush,
        );
        cy += behavior_h + 26.0;

        // ---- 全局记忆 ----
        self.draw_memory_section_title(
            target,
            "全局记忆",
            x,
            width,
            cy,
            &section_fmt,
            &title_brush,
        );
        cy += 24.0;
        let ((global_count, global_updated), groups) = self.memory_group_views();
        cy = self.render_memory_card(
            target,
            x,
            cy,
            width,
            &crate::settings::MemoryDetailTarget::Global,
            global_count,
            global_updated,
            false,
            &title_fmt,
            &desc_fmt,
            &title_brush,
            &desc_brush,
        );
        cy += 26.0;

        // ---- 项目记忆 ----
        self.draw_memory_section_title(
            target,
            "项目记忆",
            x,
            width,
            cy,
            &section_fmt,
            &title_brush,
        );
        cy += 24.0;
        if groups.is_empty() {
            let hint_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + width,
                bottom: cy + 52.0,
            };
            self.draw_settings_card(target, &hint_rect);
            let hint_text: Vec<u16> =
                "暂无项目记忆。打开工作区并对话后，沉淀的经验会按工作区出现在这里。"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            target.DrawText(
                &hint_text,
                &desc_fmt,
                &D2D_RECT_F {
                    left: x + 16.0,
                    top: cy,
                    right: x + width - 16.0,
                    bottom: cy + 52.0,
                },
                &desc_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += 52.0;
        } else {
            for (target_ref, count, updated, is_current) in groups {
                cy = self.render_memory_card(
                    target,
                    x,
                    cy,
                    width,
                    &target_ref,
                    count,
                    updated,
                    is_current,
                    &title_fmt,
                    &desc_fmt,
                    &title_brush,
                    &desc_brush,
                );
                cy += 10.0;
            }
        }
        cy
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_memory_section_title(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        text: &str,
        x: f32,
        width: f32,
        y: f32,
        format: &IDWriteTextFormat,
        brush: &ID2D1SolidColorBrush,
    ) {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &wide,
            format,
            &D2D_RECT_F {
                left: x + 2.0,
                top: y,
                right: x + width,
                bottom: y + 20.0,
            },
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// 记忆行为开关行（图标 + 标题/描述 + 开关）
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_memory_behavior_row(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        is_global: bool,
        title: &str,
        desc: &str,
        enabled: bool,
        hit: crate::settings::MemoryHit,
        title_format: &IDWriteTextFormat,
        desc_format: &IDWriteTextFormat,
        title_brush: &ID2D1SolidColorBrush,
        desc_brush: &ID2D1SolidColorBrush,
    ) {
        // 图标块
        let icon_x = x + 16.0;
        let icon_y = y + height / 2.0 - 15.0;
        let tile = D2D_RECT_F {
            left: icon_x,
            top: icon_y,
            right: icon_x + 30.0,
            bottom: icon_y + 30.0,
        };
        if let Ok(bg) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::accent_soft())
        {
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: tile,
                radiusX: 8.0,
                radiusY: 8.0,
            };
            target.FillRoundedRectangle(&rounded, &bg);
        }
        if let Ok(icon_brush) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::accent())
        {
            if is_global {
                draw_memory_globe_icon(target, &icon_brush, icon_x + 15.0, icon_y + 15.0, 7.0);
            } else {
                draw_memory_folder_icon(
                    target,
                    &icon_brush,
                    icon_x + 9.0,
                    icon_y + 10.0,
                    12.0,
                    10.0,
                );
            }
        }

        // 标题 + 描述
        let text_x = icon_x + 30.0 + 12.0;
        let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &title_wide,
            title_format,
            &D2D_RECT_F {
                left: text_x,
                top: y + 12.0,
                right: x + width - 90.0,
                bottom: y + 30.0,
            },
            title_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        let desc_wide: Vec<u16> = desc.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &desc_wide,
            desc_format,
            &D2D_RECT_F {
                left: text_x,
                top: y + 30.0,
                right: x + width - 16.0,
                bottom: y + height - 6.0,
            },
            desc_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 开关
        let sw_x = x + width - 16.0 - 38.0;
        let sw_y = y + height / 2.0 - 10.0;
        draw_memory_switch(self, target, sw_x, sw_y, enabled);

        // 整行可点击
        self.ui
            .settings_panel
            .add_memory_region(hit, x + 8.0, y, width - 16.0, height);
    }

    /// 记忆卡片（全局 / 某工作区），返回底部 Y
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_memory_card(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        target_ref: &crate::settings::MemoryDetailTarget,
        count: usize,
        updated_at: u64,
        is_current: bool,
        title_format: &IDWriteTextFormat,
        desc_format: &IDWriteTextFormat,
        title_brush: &ID2D1SolidColorBrush,
        desc_brush: &ID2D1SolidColorBrush,
    ) -> f32 {
        let h = 64.0;
        let card_rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + width,
            bottom: y + h,
        };
        let hovered = self.ui.settings_panel.memory_hover.as_ref()
            == Some(&crate::settings::MemoryHit::OpenCard(target_ref.clone()));
        self.draw_settings_card(target, &card_rect);
        // 悬停：卡片描边加深
        if hovered {
            if let Ok(brush) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_border())
            {
                let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: card_rect,
                    radiusX: 10.0,
                    radiusY: 10.0,
                };
                target.DrawRoundedRectangle(&rounded, &brush, 1.2, None);
            }
        }

        // 图标块
        let icon_x = x + 16.0;
        let icon_y = y + 16.0;
        let tile = D2D_RECT_F {
            left: icon_x,
            top: icon_y,
            right: icon_x + 32.0,
            bottom: icon_y + 32.0,
        };
        let tile_bg = if target_ref.is_global() {
            app_theme::accent_soft()
        } else {
            let name = target_ref.title();
            memory_workspace_accent(&name)
        };
        if let Ok(bg) = self.win.render_ctx.brush_cache.get_brush(target, &tile_bg) {
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: tile,
                radiusX: 8.0,
                radiusY: 8.0,
            };
            target.FillRoundedRectangle(&rounded, &bg);
        }
        if let Ok(icon_brush) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::accent_text())
        {
            if target_ref.is_global() {
                draw_memory_globe_icon(target, &icon_brush, icon_x + 16.0, icon_y + 16.0, 7.5);
            } else {
                draw_memory_folder_icon(
                    target,
                    &icon_brush,
                    icon_x + 9.0,
                    icon_y + 11.0,
                    14.0,
                    11.0,
                );
            }
        }

        // 标题 + 统计
        let text_x = icon_x + 32.0 + 12.0;
        let mut title = target_ref.title();
        if is_current {
            title.push_str("（当前）");
        }
        let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &title_wide,
            title_format,
            &D2D_RECT_F {
                left: text_x,
                top: y + 14.0,
                right: x + width - 120.0,
                bottom: y + 32.0,
            },
            title_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        let meta = if count == 0 {
            "暂无记忆".to_string()
        } else {
            format!(
                "{} 条记忆 · 更新于 {}",
                count,
                format_memory_updated(updated_at)
            )
        };
        let meta_wide: Vec<u16> = meta.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &meta_wide,
            desc_format,
            &D2D_RECT_F {
                left: text_x,
                top: y + 34.0,
                right: x + width - 120.0,
                bottom: y + 50.0,
            },
            desc_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 右侧操作：查看（文档图标） / 清空（垃圾桶，需二次确认）
        let action_y = y + h / 2.0 - 14.0;
        let clear_hovered = self.ui.settings_panel.memory_hover.as_ref()
            == Some(&crate::settings::MemoryHit::ClearCard(target_ref.clone()));
        let confirming = self.ui.settings_panel.memory_confirm_clear.as_ref() == Some(target_ref);
        let trash_x = x + width - 16.0 - 28.0;
        let trash_rect = D2D_RECT_F {
            left: trash_x,
            top: action_y,
            right: trash_x + 28.0,
            bottom: action_y + 28.0,
        };
        if let Ok(bg) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::danger_soft())
        {
            if clear_hovered || confirming {
                let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: trash_rect,
                    radiusX: 6.0,
                    radiusY: 6.0,
                };
                target.FillRoundedRectangle(&rounded, &bg);
            }
        }
        if let Ok(brush) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::danger())
        {
            if confirming {
                let text: Vec<u16> = "确认清空".encode_utf16().chain(Some(0)).collect();
                let pill = D2D_RECT_F {
                    left: trash_x - 46.0,
                    top: action_y + 2.0,
                    right: trash_x + 28.0,
                    bottom: action_y + 26.0,
                };
                if let Ok(rounded) = self.win.render_ctx.text_format_cache.get_format(
                    10.5,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                ) {
                    target.DrawText(
                        &text,
                        &rounded,
                        &pill,
                        &brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            } else {
                draw_memory_trash_icon(target, &brush, trash_x + 8.0, action_y + 6.0);
            }
        }

        let view_x = trash_x - 34.0;
        let view_rect = D2D_RECT_F {
            left: view_x,
            top: action_y,
            right: view_x + 28.0,
            bottom: action_y + 28.0,
        };
        // 清空二次确认时隐藏“查看”图标，避免误触触发清空
        if !confirming {
            let view_hovered = self.ui.settings_panel.memory_hover.as_ref()
                == Some(&crate::settings::MemoryHit::OpenCard(target_ref.clone()));
            if let Ok(bg) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_soft())
            {
                if view_hovered {
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: view_rect,
                        radiusX: 6.0,
                        radiusY: 6.0,
                    };
                    target.FillRoundedRectangle(&rounded, &bg);
                }
            }
            if let Ok(brush) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent())
            {
                draw_memory_doc_icon(target, &brush, view_x + 8.0, action_y + 6.0);
            }
        }

        // 命中区：先卡片后按钮（hit_test 反向优先按钮）
        self.ui.settings_panel.add_memory_region(
            crate::settings::MemoryHit::OpenCard(target_ref.clone()),
            x,
            y,
            width,
            h,
        );
        if confirming {
            self.ui.settings_panel.add_memory_region(
                crate::settings::MemoryHit::ClearCard(target_ref.clone()),
                trash_x - 46.0,
                action_y,
                28.0 + 46.0,
                28.0,
            );
        } else {
            self.ui.settings_panel.add_memory_region(
                crate::settings::MemoryHit::ClearCard(target_ref.clone()),
                trash_x,
                action_y,
                28.0,
                28.0,
            );
        }

        y + h
    }

    /// 记忆详情视图：条目列表 + 单条删除 / 清空
    unsafe fn render_memory_detail(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
        detail: &crate::settings::MemoryDetailTarget,
    ) -> f32 {
        use windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD;

        let title_fmt = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                15.0,
                DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let body_fmt = self
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
        let meta_fmt = self
            .win
            .render_ctx
            .text_format_cache
            .get_format(
                10.5,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            )
            .unwrap();
        let title_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::text_primary())
            .unwrap();
        let body_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::text_primary())
            .unwrap();
        let meta_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::text_muted())
            .unwrap();
        let accent_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::accent_text())
            .unwrap();

        let entries: Vec<_> = self
            .ui
            .settings_panel
            .memory_entries
            .iter()
            .filter(|b| detail.to_filter().matches(b))
            .cloned()
            .collect();

        let mut cy = start_y;

        // 返回
        let back_text: Vec<u16> = "← 返回记忆列表".encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &back_text,
            &meta_fmt,
            &D2D_RECT_F {
                left: x + 2.0,
                top: cy,
                right: x + 160.0,
                bottom: cy + 20.0,
            },
            &accent_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        self.ui.settings_panel.add_memory_region(
            crate::settings::MemoryHit::Back,
            x,
            cy - 4.0,
            150.0,
            26.0,
        );
        cy += 30.0;

        // 标题 + 计数
        let title = detail.title();
        let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &title_wide,
            &title_fmt,
            &D2D_RECT_F {
                left: x + 2.0,
                top: cy,
                right: x + width - 180.0,
                bottom: cy + 26.0,
            },
            &title_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        let count_text: Vec<u16> = format!("共 {} 条", entries.len())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        target.DrawText(
            &count_text,
            &meta_fmt,
            &D2D_RECT_F {
                left: x + 2.0 + title.chars().count() as f32 * 15.0 + 12.0,
                top: cy + 4.0,
                right: x + width - 120.0,
                bottom: cy + 24.0,
            },
            &meta_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 清空按钮（二次确认）
        let clear_confirming = self.ui.settings_panel.memory_confirm_clear.as_ref() == Some(detail);
        let clear_w = if clear_confirming { 86.0 } else { 64.0 };
        let clear_rect = D2D_RECT_F {
            left: x + width - clear_w,
            top: cy + 2.0,
            right: x + width,
            bottom: cy + 26.0,
        };
        let clear_hovered = self.ui.settings_panel.memory_hover.as_ref()
            == Some(&crate::settings::MemoryHit::ClearScope(detail.clone()));
        if let Ok(bg) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::danger_soft())
        {
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: clear_rect,
                radiusX: 6.0,
                radiusY: 6.0,
            };
            target.FillRoundedRectangle(&rounded, &bg);
            if clear_hovered || clear_confirming {
                if let Ok(border) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::danger())
                {
                    target.DrawRoundedRectangle(&rounded, &border, 1.0, None);
                }
            }
        }
        if let Ok(brush) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &app_theme::danger())
        {
            if let Ok(fmt) = self.win.render_ctx.text_format_cache.get_format(
                11.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                let label: Vec<u16> = (if clear_confirming {
                    "确认清空全部？"
                } else {
                    "清空"
                })
                .encode_utf16()
                .chain(Some(0))
                .collect();
                target.DrawText(
                    &label,
                    &fmt,
                    &clear_rect,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
        self.ui.settings_panel.add_memory_region(
            crate::settings::MemoryHit::ClearScope(detail.clone()),
            clear_rect.left,
            clear_rect.top,
            clear_w,
            24.0,
        );
        cy += 38.0;

        if entries.is_empty() {
            let empty_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + width,
                bottom: cy + 56.0,
            };
            self.draw_settings_card(target, &empty_rect);
            let hint: Vec<u16> =
                "这份记忆库还没有内容。完成对话并归档后，可复用的经验会自动沉淀到这里。"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            target.DrawText(
                &hint,
                &meta_fmt,
                &D2D_RECT_F {
                    left: x + 16.0,
                    top: cy,
                    right: x + width - 16.0,
                    bottom: cy + 56.0,
                },
                &meta_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            return cy + 56.0;
        }

        // 条目列表卡片
        let row_h = 58.0_f32;
        let list_h = row_h * entries.len() as f32;
        let list_rect = D2D_RECT_F {
            left: x,
            top: cy,
            right: x + width,
            bottom: cy + list_h,
        };
        self.draw_settings_card(target, &list_rect);

        for (i, bullet) in entries.iter().enumerate() {
            let row_y = cy + i as f32 * row_h;
            if i > 0 {
                if let Ok(brush) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::separator())
                {
                    target.FillRectangle(
                        &D2D_RECT_F {
                            left: x + 16.0,
                            top: row_y,
                            right: x + width - 16.0,
                            bottom: row_y + 1.0,
                        },
                        &brush,
                    );
                }
            }

            // 分类标签
            let tag_text: Vec<u16> = memory_section_label(&bullet.section)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let tag_w = memory_section_label(&bullet.section).chars().count() as f32 * 11.0 + 14.0;
            let tag_rect = D2D_RECT_F {
                left: x + 16.0,
                top: row_y + 12.0,
                right: x + 16.0 + tag_w,
                bottom: row_y + 30.0,
            };
            if let Ok(bg) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_soft())
            {
                let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: tag_rect,
                    radiusX: 9.0,
                    radiusY: 9.0,
                };
                target.FillRoundedRectangle(&rounded, &bg);
            }
            if let Ok(fmt) = self.win.render_ctx.text_format_cache.get_format(
                10.0,
                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
            ) {
                target.DrawText(
                    &tag_text,
                    &fmt,
                    &tag_rect,
                    &accent_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // 内容（最多两行）
            let content: Vec<u16> = bullet.content.encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &content,
                &body_fmt,
                &D2D_RECT_F {
                    left: x + 16.0 + tag_w + 10.0,
                    top: row_y + 7.0,
                    right: x + width - 96.0,
                    bottom: row_y + 38.0,
                },
                &body_brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 反馈计数
            let meta: Vec<u16> = format!(
                "有效 {} · 无效 {}",
                bullet.helpful_count, bullet.harmful_count
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            target.DrawText(
                &meta,
                &meta_fmt,
                &D2D_RECT_F {
                    left: x + 16.0 + tag_w + 10.0,
                    top: row_y + 40.0,
                    right: x + width - 96.0,
                    bottom: row_y + row_h - 4.0,
                },
                &meta_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 删除按钮（二次确认）
            let confirming =
                self.ui.settings_panel.memory_confirm_delete.as_deref() == Some(bullet.id.as_str());
            let del_hovered = self.ui.settings_panel.memory_hover.as_ref()
                == Some(&crate::settings::MemoryHit::DeleteEntry(bullet.id.clone()));
            let del_rect = D2D_RECT_F {
                left: x + width - 16.0 - if confirming { 72.0 } else { 28.0 },
                top: row_y + row_h / 2.0 - 14.0,
                right: x + width - 16.0,
                bottom: row_y + row_h / 2.0 + 14.0,
            };
            if del_hovered || confirming {
                if let Ok(bg) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::danger_soft())
                {
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: del_rect,
                        radiusX: 6.0,
                        radiusY: 6.0,
                    };
                    target.FillRoundedRectangle(&rounded, &bg);
                }
            }
            if let Ok(brush) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::danger())
            {
                if confirming {
                    if let Ok(fmt) = self.win.render_ctx.text_format_cache.get_format(
                        10.5,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                        DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                        DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                    ) {
                        let label: Vec<u16> = "确认删除".encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &label,
                            &fmt,
                            &del_rect,
                            &brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                } else {
                    draw_memory_trash_icon(target, &brush, del_rect.left + 8.0, del_rect.top + 6.0);
                }
            }
            self.ui.settings_panel.add_memory_region(
                crate::settings::MemoryHit::DeleteEntry(bullet.id.clone()),
                del_rect.left,
                del_rect.top,
                del_rect.right - del_rect.left,
                del_rect.bottom - del_rect.top,
            );
        }

        cy + list_h
    }

    /// 渲染"通用"标签页内容（主题 / 字体大小 / 自动保存等概览）
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
                .get_brush(target, &app_theme::text_muted())
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
                .get_brush(target, &app_theme::text_muted())
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

// ============================================================================
// 记忆页绘制辅助（图标 / 时间 / 分组）
// ============================================================================

/// 记忆更新时间格式："11月23日 18:19"
fn format_memory_updated(ts: u64) -> String {
    if ts == 0 {
        return "暂无".to_string();
    }
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    match time::OffsetDateTime::from_unix_timestamp(ts as i64) {
        Ok(t) => {
            let t = t.to_offset(offset);
            format!(
                "{}月{}日 {:02}:{:02}",
                u8::from(t.month()),
                t.day(),
                t.hour(),
                t.minute()
            )
        }
        Err(_) => "未知".to_string(),
    }
}

/// 记忆分类展示名
fn memory_section_label(section: &str) -> String {
    match section {
        "tool_use" => "工具使用".to_string(),
        "coding_style" => "编码风格".to_string(),
        "pitfalls" => "踩坑".to_string(),
        "project_facts" => "项目事实".to_string(),
        "workflow" => "协作流程".to_string(),
        other if other.is_empty() => "经验".to_string(),
        other => other.to_string(),
    }
}

/// 工作区图标底色：按名称稳定散列到一组柔和主题色
fn memory_workspace_accent(name: &str) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    const PALETTE: [(f32, f32, f32); 6] = [
        (0.90, 0.95, 0.98),
        (0.96, 0.93, 0.99),
        (0.99, 0.95, 0.90),
        (0.93, 0.98, 0.93),
        (0.99, 0.93, 0.93),
        (0.90, 0.97, 0.99),
    ];
    let mut h: u32 = 0;
    for b in name.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    let (r, g, b) = PALETTE[(h as usize) % PALETTE.len()];
    color_f(r, g, b, 1.0)
}

/// 薄荷绿开关
unsafe fn draw_memory_switch(
    state: &mut EditorState,
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    x: f32,
    y: f32,
    on: bool,
) {
    use windows::Win32::Graphics::Direct2D::Common::{D2D_POINT_2F, D2D_RECT_F};
    use windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE;
    let (w, h) = (38.0_f32, 20.0_f32);
    let bg = if on {
        app_theme::accent()
    } else {
        app_theme::switch_off()
    };
    if let Ok(brush) = state.win.render_ctx.brush_cache.get_brush(target, &bg) {
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            radiusX: h / 2.0,
            radiusY: h / 2.0,
        };
        target.FillRoundedRectangle(&rounded, &brush);
    }
    let knob_cx = if on { x + w - 10.0 } else { x + 10.0 };
    if let Ok(brush) = state
        .win
        .render_ctx
        .brush_cache
        .get_brush(target, &app_theme::knob())
    {
        target.FillEllipse(
            &D2D1_ELLIPSE {
                point: D2D_POINT_2F {
                    x: knob_cx,
                    y: y + h / 2.0,
                },
                radiusX: 7.0,
                radiusY: 7.0,
            },
            &brush,
        );
    }
}

/// 地球图标（全局记忆）
unsafe fn draw_memory_globe_icon(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    cx: f32,
    cy: f32,
    r: f32,
) {
    use windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F;
    use windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE;
    target.DrawEllipse(
        &D2D1_ELLIPSE {
            point: D2D_POINT_2F { x: cx, y: cy },
            radiusX: r,
            radiusY: r,
        },
        brush,
        1.2,
        None,
    );
    target.DrawLine(
        D2D_POINT_2F { x: cx - r, y: cy },
        D2D_POINT_2F { x: cx + r, y: cy },
        brush,
        1.0,
        None,
    );
    target.DrawEllipse(
        &D2D1_ELLIPSE {
            point: D2D_POINT_2F { x: cx, y: cy },
            radiusX: r * 0.45,
            radiusY: r,
        },
        brush,
        1.0,
        None,
    );
}

/// 文件夹图标（项目记忆）
unsafe fn draw_memory_folder_icon(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
    use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
    // 顶部标签
    target.FillRectangle(
        &D2D_RECT_F {
            left: x,
            top: y,
            right: x + w * 0.45,
            bottom: y + h * 0.28,
        },
        brush,
    );
    // 主体
    let body = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x,
            top: y + h * 0.2,
            right: x + w,
            bottom: y + h,
        },
        radiusX: 1.5,
        radiusY: 1.5,
    };
    target.FillRoundedRectangle(&body, brush);
}

/// 文档图标（查看记忆）
unsafe fn draw_memory_doc_icon(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    x: f32,
    y: f32,
) {
    use windows::Win32::Graphics::Direct2D::Common::{D2D_POINT_2F, D2D_RECT_F};
    use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
    let body = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x,
            top: y,
            right: x + 11.0,
            bottom: y + 14.0,
        },
        radiusX: 1.5,
        radiusY: 1.5,
    };
    target.DrawRoundedRectangle(&body, brush, 1.0, None);
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 3.0,
            y: y + 5.0,
        },
        D2D_POINT_2F {
            x: x + 8.0,
            y: y + 5.0,
        },
        brush,
        1.0,
        None,
    );
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 3.0,
            y: y + 8.0,
        },
        D2D_POINT_2F {
            x: x + 8.0,
            y: y + 8.0,
        },
        brush,
        1.0,
        None,
    );
}

/// 垃圾桶图标（清空/删除记忆）
unsafe fn draw_memory_trash_icon(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    x: f32,
    y: f32,
) {
    use windows::Win32::Graphics::Direct2D::Common::{D2D_POINT_2F, D2D_RECT_F};
    use windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT;
    // 盖子
    target.DrawLine(
        D2D_POINT_2F { x, y: y + 3.0 },
        D2D_POINT_2F {
            x: x + 12.0,
            y: y + 3.0,
        },
        brush,
        1.1,
        None,
    );
    // 提手
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 4.0,
            y: y + 3.0,
        },
        D2D_POINT_2F {
            x: x + 4.0,
            y: y + 1.0,
        },
        brush,
        1.0,
        None,
    );
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 8.0,
            y: y + 3.0,
        },
        D2D_POINT_2F {
            x: x + 8.0,
            y: y + 1.0,
        },
        brush,
        1.0,
        None,
    );
    // 桶身
    let body = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x + 1.5,
            top: y + 4.5,
            right: x + 10.5,
            bottom: y + 14.0,
        },
        radiusX: 1.5,
        radiusY: 1.5,
    };
    target.DrawRoundedRectangle(&body, brush, 1.0, None);
    // 内线
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 4.5,
            y: y + 7.0,
        },
        D2D_POINT_2F {
            x: x + 4.5,
            y: y + 11.5,
        },
        brush,
        1.0,
        None,
    );
    target.DrawLine(
        D2D_POINT_2F {
            x: x + 7.5,
            y: y + 7.0,
        },
        D2D_POINT_2F {
            x: x + 7.5,
            y: y + 11.5,
        },
        brush,
        1.0,
        None,
    );
}
