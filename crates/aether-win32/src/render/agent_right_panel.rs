use super::*;

impl EditorState {
    /// 智能体模式右侧标签页面板：浏览器风格标签栏 + 内容区域
    ///
    /// 布局：
    /// - 顶部：标签栏（显示所有打开的标签页，带关闭按钮和"+"新建按钮）
    /// - 中间：内容区域（根据活动标签页类型渲染文件编辑器/终端/设置等）
    /// - 空状态：无标签页时显示快捷操作按钮
    pub(super) fn render_agent_right_panel(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        if width < 1.0 || height < 1.0 {
            return;
        }

        unsafe {
            // 背景
            let bg_color = if self.win.theme.glass_enabled {
                self.win.theme.sidebar_bg
            } else {
                color_f(0.13, 0.13, 0.14, 1.0)
            };
            let bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &bg_color)
                .unwrap();
            let bg_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&bg_rect, &bg_brush);

            // 左边框（与中间 AI 面板分隔）
            let border_color = if self.win.theme.glass_enabled {
                self.win.theme.panel_border
            } else {
                color_f(0.2, 0.2, 0.2, 1.0)
            };
            let border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &border_color)
                .unwrap();
            let border_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + 1.0,
                bottom: y + height,
            };
            target.FillRectangle(&border_rect, &border_brush);

            let has_tabs = !self.editor.tab_bar.tabs.is_empty();
            let tab_bar_height = if has_tabs {
                crate::layout::TAB_BAR_HEIGHT
            } else {
                0.0
            };

            // 标签栏
            if has_tabs {
                self.render_tab_bar(target, x, y, width, tab_bar_height);
            }

            // 内容区域
            let content_y = y + tab_bar_height;
            let content_h = height - tab_bar_height;

            if content_h > 1.0 {
                if !has_tabs || self.active_tab_is_new_tab() {
                    // 新标签页（NTP）：快捷搜索框 + 快捷操作按钮
                    self.render_new_tab_page(target, x, content_y, width, content_h);
                } else if self.active_tab_is_settings() {
                    let text_brush = match self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &self.win.theme.text_default)
                    {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    self.render_settings_sidebar(
                        target,
                        x,
                        content_y,
                        width,
                        content_h,
                        &text_brush,
                    );
                } else if self.active_tab_is_sandbox_eval() {
                    self.render_sandbox_eval_page(target, x, content_y, width, content_h);
                } else if self.active_tab_is_terminal() {
                    self.render_bottom_panel(target, x, content_y, width, content_h);
                } else if self.active_tab_is_browser() {
                    // 浏览器标签：D2D 只画顶部工具栏，网页区域由 WebView2 子窗口覆盖
                    self.render_browser_toolbar(target, x, content_y, width, content_h);
                } else if self.editor.content.language == Language::Image {
                    self.render_image_preview(target, x, content_y, width, content_h);
                } else if self.editor.markdown_preview
                    && self.editor.content.language == Language::Markdown
                {
                    self.render_markdown_preview(target, x, content_y, width, content_h);
                } else {
                    self.render_editor(target, x, content_y, width, content_h);
                }
            }
        }
    }

    /// 新标签页（NTP）起始页：快捷搜索框 + 快捷操作按钮（打开本地文件/新建文件/打开终端）。
    /// 经典模式与智能体模式共用，几何取自 new_tab_page_region（与点击/光标/IME 一致）。
    pub(crate) fn render_new_tab_page(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        _x: f32,
        _y: f32,
        _width: f32,
        _height: f32,
    ) {
        unsafe {
            let text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &self.win.theme.text_default)
                .unwrap();
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.5, 0.5, 0.5, 1.0))
                .unwrap();

            let btn_width = crate::render::NEW_TAB_BTN_WIDTH;
            let btn_height = crate::render::NEW_TAB_BTN_HEIGHT;
            let btn_gap = crate::render::NEW_TAB_BTN_GAP;
            let page_region = self.new_tab_page_region(&self.ui.layout);
            let search_box = crate::render::new_tab_page_geom_in(page_region);
            let btn_x = search_box.x;
            let start_y = search_box.y;

            let buttons = [
                ("搜索或输入网址", crate::icons::IconKind::Search),
                ("打开本地文件", crate::icons::IconKind::OpenFolder),
                ("新建文件", crate::icons::IconKind::NewFile),
                ("打开终端", crate::icons::IconKind::Terminal),
            ];

            let btn_format = self
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

            // 搜索框状态快照（避免渲染中持有 self 可变借用）
            let search_focused = self.browser.empty_search_focused;
            let search_text = self.browser.empty_search_text.clone();
            let search_comp = self.browser.empty_search_composition.clone();
            let search_caret = self.browser.empty_search_caret_visible;

            for (i, (label, icon)) in buttons.iter().enumerate() {
                let btn_y = start_y + i as f32 * (btn_height + btn_gap);
                let btn_rect = D2D_RECT_F {
                    left: btn_x,
                    top: btn_y,
                    right: btn_x + btn_width,
                    bottom: btn_y + btn_height,
                };

                // 背景（圆角矩形）：搜索框聚焦时加深并描高亮边
                let btn_bg = if i == 0 && search_focused {
                    color_f(0.13, 0.13, 0.16, 1.0)
                } else {
                    color_f(0.18, 0.18, 0.20, 1.0)
                };
                let btn_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &btn_bg)
                    .unwrap();
                let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: btn_rect,
                    radiusX: 6.0,
                    radiusY: 6.0,
                };
                target.FillRoundedRectangle(&rounded, &btn_bg_brush);
                if i == 0 && search_focused {
                    let accent_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &color_f(0.35, 0.55, 0.95, 1.0))
                        .unwrap();
                    target.DrawRoundedRectangle(&rounded, &accent_brush, 1.5, None);
                }

                // 图标
                let icon_size = 16.0f32;
                self.ui.icons.draw(
                    target,
                    *icon,
                    btn_x + 12.0,
                    btn_y + (btn_height - icon_size) / 2.0,
                    icon_size,
                    icon_size,
                    &dim_brush,
                );

                if i == 0 {
                    // 首行：快捷搜索框 —— 已输入文本（+内联合成串）与光标；未输入显示占位提示
                    let text_left = btn_x + 36.0;
                    let text_rect = D2D_RECT_F {
                        left: text_left,
                        top: btn_y,
                        right: btn_x + btn_width - 8.0,
                        bottom: btn_y + btn_height,
                    };
                    let display =
                        format!("{}{}", search_text, search_comp.as_deref().unwrap_or(""));
                    if display.is_empty() {
                        let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &label_wide,
                            &btn_format,
                            &text_rect,
                            &dim_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    } else {
                        let wide: Vec<u16> = display.encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &wide,
                            &btn_format,
                            &text_rect,
                            &text_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    // 光标（追加式编辑，固定在文本末尾）
                    if search_focused && search_caret {
                        let weight = DWRITE_FONT_WEIGHT_NORMAL.0 as u32;
                        let tw = self
                            .win
                            .render_ctx
                            .text_format_cache
                            .measure_text_width(&display, 13.0, weight)
                            .unwrap_or(0.0);
                        let caret_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &self.win.theme.text_default)
                            .unwrap();
                        let caret_rect = D2D_RECT_F {
                            left: text_left + tw + 1.0,
                            top: btn_y + 9.0,
                            right: text_left + tw + 2.5,
                            bottom: btn_y + btn_height - 9.0,
                        };
                        target.FillRectangle(&caret_rect, &caret_brush);
                    }
                    continue;
                }

                // 其余行：普通按钮文字
                let label_wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let text_rect = D2D_RECT_F {
                    left: btn_x + 36.0,
                    top: btn_y,
                    right: btn_x + btn_width - 8.0,
                    bottom: btn_y + btn_height,
                };
                target.DrawText(
                    &label_wide,
                    &btn_format,
                    &text_rect,
                    &text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    /// 浏览器标签页工具栏：后退/前进/刷新按钮 + 地址栏。
    ///
    /// 网页正文区域由 WebView2 子窗口覆盖（每帧由 sync_browser_webviews 同步边界），
    /// D2D 只绘制工具栏条带；同时记录命中布局供点击处理使用。
    pub(crate) fn render_browser_toolbar(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let toolbar_h = crate::browser::BROWSER_TOOLBAR_HEIGHT;
        if width < 10.0 || height < toolbar_h {
            self.browser.toolbar_layout = None;
            return;
        }

        // 读取活动浏览器实例的展示状态
        let (can_back, can_forward, address_text, editing) = self
            .active_browser_id()
            .and_then(|id| self.browser.get(id))
            .map(|i| {
                (
                    i.can_go_back,
                    i.can_go_forward,
                    i.address_text.clone(),
                    i.address_editing,
                )
            })
            .unwrap_or_default();

        unsafe {
            // 1. 工具栏背景 + 底部分隔线
            let bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.16, 0.16, 0.17, 1.0))
                .unwrap();
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + width,
                    bottom: y + toolbar_h,
                },
                &bg_brush,
            );
            let sep_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.23, 0.23, 0.24, 1.0))
                .unwrap();
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y + toolbar_h - 1.0,
                    right: x + width,
                    bottom: y + toolbar_h,
                },
                &sep_brush,
            );

            // 2. 网页正文区白底（WebView2 就绪前避免黑洞视觉）
            let white_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(1.0, 1.0, 1.0, 1.0))
                .unwrap();
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y + toolbar_h,
                    right: x + width,
                    bottom: y + height,
                },
                &white_brush,
            );

            // 3. 导航按钮（禁用态置灰）
            let btn_size = 28.0f32;
            let btn_y = y + (toolbar_h - btn_size) / 2.0;
            let back_x = x + 8.0;
            let forward_x = x + 40.0;
            let refresh_x = x + 72.0;
            let icon_size = 16.0f32;
            let enabled_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &self.win.theme.text_default)
                .unwrap();
            let disabled_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.38, 0.38, 0.40, 1.0))
                .unwrap();
            let back_brush = if can_back {
                &enabled_brush
            } else {
                &disabled_brush
            };
            let fwd_brush = if can_forward {
                &enabled_brush
            } else {
                &disabled_brush
            };
            self.ui.icons.draw(
                target,
                crate::icons::IconKind::Back,
                back_x + (btn_size - icon_size) / 2.0,
                btn_y + (btn_size - icon_size) / 2.0,
                icon_size,
                icon_size,
                back_brush,
            );
            self.ui.icons.draw(
                target,
                crate::icons::IconKind::Forward,
                forward_x + (btn_size - icon_size) / 2.0,
                btn_y + (btn_size - icon_size) / 2.0,
                icon_size,
                icon_size,
                fwd_brush,
            );
            self.ui.icons.draw(
                target,
                crate::icons::IconKind::Refresh,
                refresh_x + (btn_size - icon_size) / 2.0,
                btn_y + (btn_size - icon_size) / 2.0,
                icon_size,
                icon_size,
                &enabled_brush,
            );

            // 4. 地址栏（编辑态带光标与高亮边框）
            let addr_x = x + 108.0;
            let addr_w = (width - 108.0 - 12.0).max(40.0);
            let addr_h = 26.0f32;
            let addr_y = y + (toolbar_h - addr_h) / 2.0;
            let addr_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.11, 0.11, 0.12, 1.0))
                .unwrap();
            let addr_rect = D2D_RECT_F {
                left: addr_x,
                top: addr_y,
                right: addr_x + addr_w,
                bottom: addr_y + addr_h,
            };
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: addr_rect,
                radiusX: addr_h / 2.0,
                radiusY: addr_h / 2.0,
            };
            target.FillRoundedRectangle(&rounded, &addr_bg_brush);
            if editing {
                let focus_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.30, 0.51, 0.86, 1.0))
                    .unwrap();
                target.DrawRoundedRectangle(&rounded, &focus_brush, 1.5, None);
            }

            // 地址文本（编辑中显示编辑文本，否则当前 URL；均为空时占位提示）
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.45, 0.45, 0.47, 1.0))
                .unwrap();
            let addr_format = self
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
            let display_text = if address_text.is_empty() {
                "搜索或输入网址".to_string()
            } else {
                address_text.clone()
            };
            let text_brush = if address_text.is_empty() {
                &dim_brush
            } else {
                &enabled_brush
            };
            let text_wide: Vec<u16> = display_text.encode_utf16().chain(Some(0)).collect();
            let text_rect = D2D_RECT_F {
                left: addr_x + 12.0,
                top: addr_y,
                right: addr_x + addr_w - 8.0,
                bottom: addr_y + addr_h,
            };
            target.DrawText(
                &text_wide,
                &addr_format,
                &text_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 编辑光标（追加式输入，光标始终在文本末尾）
            if editing {
                let tw = if address_text.is_empty() {
                    0.0
                } else {
                    self.win
                        .render_ctx
                        .text_format_cache
                        .measure_text_width(&address_text, 12.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                        .unwrap_or(0.0)
                };
                let caret_x = addr_x + 12.0 + tw;
                let caret_rect = D2D_RECT_F {
                    left: caret_x,
                    top: addr_y + 6.0,
                    right: caret_x + 1.5,
                    bottom: addr_y + addr_h - 6.0,
                };
                target.FillRectangle(&caret_rect, &enabled_brush);
            }

            // 5. 记录命中布局（绝对逻辑坐标）
            self.browser.toolbar_layout = Some(crate::browser::BrowserToolbarLayout {
                back: crate::layout::Region::new(back_x, btn_y, btn_size, btn_size),
                forward: crate::layout::Region::new(forward_x, btn_y, btn_size, btn_size),
                refresh: crate::layout::Region::new(refresh_x, btn_y, btn_size, btn_size),
                address: crate::layout::Region::new(addr_x, addr_y, addr_w, addr_h),
            });
        }
    }
}

/// 新标签页（NTP）快捷面板几何常量（渲染/点击共用，保证命中一致）
pub(crate) const NEW_TAB_BTN_WIDTH: f32 = 200.0;
pub(crate) const NEW_TAB_BTN_HEIGHT: f32 = 36.0;
pub(crate) const NEW_TAB_BTN_GAP: f32 = 12.0;
pub(crate) const NEW_TAB_BTN_COUNT: usize = 4;

/// 新标签页首行（快捷搜索框）几何：给定内容区域内水平居中、四行垂直居中。
/// 渲染、点击命中、IME 定位必须一致使用本函数（区域取自 EditorState::new_tab_page_region）。
pub(crate) fn new_tab_page_geom_in(region: crate::layout::Region) -> crate::layout::Region {
    let btn_x = region.x + (region.width - NEW_TAB_BTN_WIDTH) / 2.0;
    let total_h = NEW_TAB_BTN_HEIGHT * NEW_TAB_BTN_COUNT as f32
        + NEW_TAB_BTN_GAP * (NEW_TAB_BTN_COUNT - 1) as f32;
    let start_y = region.y + (region.height - total_h) / 2.0;
    crate::layout::Region::new(btn_x, start_y, NEW_TAB_BTN_WIDTH, NEW_TAB_BTN_HEIGHT)
}
