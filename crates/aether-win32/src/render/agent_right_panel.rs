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
            let bg_rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            app_theme::fill_app_gradient(target, &bg_rect, self.win.window_height as f32);

            let border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::border())
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
                    if self.right_panel_tree_open {
                        // 展开态：工作区文件树（由「打开工作区文件」卡片触发）
                        self.ai.ai_panel.agent_file_tree_offset_y = 0.0;
                        if let Ok(tb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &self.win.theme.text_default)
                        {
                            self.render_file_tree_sidebar(
                                target, x, content_y, width, content_h, &tb,
                            );
                        }
                    } else {
                        // 默认态：Qoder 式快捷卡片页
                        self.render_new_tab_page(target, x, content_y, width, content_h);
                    }
                } else if self.active_tab_is_settings() {
                    // 设置为独立整页：由 render/mod.rs 的覆盖层统一渲染（含返回条），
                    // 此处不重复渲染，避免命中区重复注册与几何冲突。
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

    /// 新标签页（NTP）起始页：Qoder 式居中快捷卡片列表（图标块 + 标题 + 快捷键）。
    /// 「打开工作区文件」卡片展开文件树；其余卡片打开浏览器/终端/审阅等。
    pub(crate) fn render_new_tab_page(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        _x: f32,
        _y: f32,
        _width: f32,
        _height: f32,
    ) {
        unsafe {
            self.ui.icons.ensure_created_from_target(target);
            let page = self.new_tab_page_region(&self.ui.layout);
            if page.width < 80.0 || page.height < 80.0 {
                return;
            }
            let title_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_primary())
                .unwrap();
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_muted())
                .unwrap();
            let icon_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent())
                .unwrap();

            let title_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            let sub_format = self
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

            // (标题, 副标题/快捷键, 图标, 命中区动作)
            let cards = [
                (
                    "打开工作区文件",
                    "Ctrl Alt E",
                    crate::icons::IconKind::OpenFolder,
                    "ntp:open_files",
                ),
                (
                    "打开内置浏览器",
                    "Ctrl T",
                    crate::icons::IconKind::Search,
                    "ntp:browser",
                ),
                (
                    "打开终端",
                    "Ctrl Shift J",
                    crate::icons::IconKind::Terminal,
                    "ntp:terminal",
                ),
                (
                    "打开审阅",
                    "Ctrl Shift G",
                    crate::icons::IconKind::GitBranch,
                    "ntp:review",
                ),
                ("更多工具", "", crate::icons::IconKind::Settings, "ntp:more"),
            ];

            let margin = 16.0f32;
            let card_w = (page.width - margin * 2.0).min(360.0);
            let card_h = 64.0f32;
            let gap = 12.0f32;
            let total_h = cards.len() as f32 * card_h + (cards.len() as f32 - 1.0) * gap;
            let start_y = page.y + ((page.height - total_h) / 2.0).max(8.0);
            let card_x = page.x + (page.width - card_w) / 2.0;

            for (i, (title, sub, icon, action)) in cards.iter().enumerate() {
                let cy = start_y + i as f32 * (card_h + gap);
                let card_rect = D2D_RECT_F {
                    left: card_x,
                    top: cy,
                    right: card_x + card_w,
                    bottom: cy + card_h,
                };
                // 卡片背景（圆角 + 柔和边框）
                if let Ok(bg) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::surface())
                {
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: card_rect,
                        radiusX: 10.0,
                        radiusY: 10.0,
                    };
                    target.FillRoundedRectangle(&rounded, &bg);
                    if let Ok(border) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &app_theme::border())
                    {
                        target.DrawRoundedRectangle(&rounded, &border, 1.0, None);
                    }
                }
                // 左侧图标块
                let tile = 40.0f32;
                let tile_x = card_x + 12.0;
                let tile_y = cy + (card_h - tile) / 2.0;
                if let Ok(tbg) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &app_theme::accent_soft())
                {
                    let tr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: tile_x,
                            top: tile_y,
                            right: tile_x + tile,
                            bottom: tile_y + tile,
                        },
                        radiusX: 8.0,
                        radiusY: 8.0,
                    };
                    target.FillRoundedRectangle(&tr, &tbg);
                }
                self.ui.icons.draw(
                    target,
                    *icon,
                    tile_x + 10.0,
                    tile_y + 10.0,
                    20.0,
                    20.0,
                    &icon_brush,
                );
                // 标题 + 副标题
                let text_x = tile_x + tile + 12.0;
                let text_right = card_x + card_w - 12.0;
                let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                let title_y = if sub.is_empty() {
                    cy + (card_h - 18.0) / 2.0
                } else {
                    cy + 14.0
                };
                target.DrawText(
                    &title_wide,
                    &title_format,
                    &D2D_RECT_F {
                        left: text_x,
                        top: title_y,
                        right: text_right,
                        bottom: title_y + 18.0,
                    },
                    &title_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                if !sub.is_empty() {
                    let sub_wide: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &sub_wide,
                        &sub_format,
                        &D2D_RECT_F {
                            left: text_x,
                            top: cy + 34.0,
                            right: text_right,
                            bottom: cy + 50.0,
                        },
                        &dim_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
                crate::hit_test::register_hit_region(*action, card_x, cy, card_w, card_h);
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
                .get_brush(target, &app_theme::surface_alt())
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
                .get_brush(target, &app_theme::border())
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
                .get_brush(target, &app_theme::text_muted())
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
                .get_brush(target, &app_theme::input_bg())
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
                    .get_brush(target, &app_theme::accent())
                    .unwrap();
                target.DrawRoundedRectangle(&rounded, &focus_brush, 1.5, None);
            }

            // 地址文本（编辑中显示编辑文本，否则当前 URL；均为空时占位提示）
            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_muted())
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
