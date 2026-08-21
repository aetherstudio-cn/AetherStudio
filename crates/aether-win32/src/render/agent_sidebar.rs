use super::*;

impl EditorState {
    /// 智能体模式左侧边栏：对话历史 + 工作区文件列表复合面板
    ///
    /// 布局（从上到下）：
    /// 1. 新会话按钮
    /// 2. 对话历史记录（可折叠）
    /// 3. 工作区文件列表（可折叠）
    pub(super) fn render_agent_sidebar(
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

            // 右边框（与中间 AI 面板分隔）
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
                left: x + width - 1.0,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&border_rect, &border_brush);

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

            let mut cy = y + 8.0;

            // 1. 新会话按钮
            let new_chat_btn_h = 32.0;
            let new_chat_btn_rect = D2D_RECT_F {
                left: x + 8.0,
                top: cy,
                right: x + width - 8.0,
                bottom: cy + new_chat_btn_h,
            };
            let btn_bg = color_f(0.18, 0.18, 0.20, 1.0);
            let btn_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_bg)
                .unwrap();
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: new_chat_btn_rect,
                radiusX: 6.0,
                radiusY: 6.0,
            };
            target.FillRoundedRectangle(&rounded, &btn_bg_brush);

            let btn_format = self
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
            let new_chat_text: Vec<u16> = "新会话".encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &new_chat_text,
                &btn_format,
                &new_chat_btn_rect,
                &text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            // 注册新会话按钮命中区域
            self.ai.ai_panel.agent_new_chat_region =
                Some((x + 8.0, cy, width - 16.0, new_chat_btn_h));
            cy += new_chat_btn_h + 12.0;

            // 2. 对话标签页列表（当前打开的对话）
            let tabs_header_h = 24.0;
            let tabs_header_rect = D2D_RECT_F {
                left: x + 8.0,
                top: cy,
                right: x + width - 8.0,
                bottom: cy + tabs_header_h,
            };
            let header_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0,
                    DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let tabs_title: Vec<u16> = "对话".encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &tabs_title,
                &header_format,
                &tabs_header_rect,
                &text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += tabs_header_h + 4.0;

            // 对话标签页列表
            let tab_item_height = 28.0;
            let conversations = &self.ai.ai_panel.conversations;
            let active_idx = self.ai.ai_panel.active;
            // 清空并重建标签页命中区域
            self.ai.ai_panel.agent_tab_regions.clear();
            self.ai.ai_panel.agent_tab_close_regions.clear();
            for (i, conv) in conversations.iter().enumerate() {
                let item_y = cy + i as f32 * tab_item_height;
                let item_rect = D2D_RECT_F {
                    left: x + 8.0,
                    top: item_y,
                    right: x + width - 8.0,
                    bottom: item_y + tab_item_height,
                };

                // 选中高亮或悬浮高亮
                let is_hovered = self.ai.ai_panel.hover_tab == Some(i);
                if i == active_idx {
                    let active_bg = color_f(0.18, 0.30, 0.45, 1.0);
                    let active_bg_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &active_bg)
                        .unwrap();
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: item_rect,
                        radiusX: 4.0,
                        radiusY: 4.0,
                    };
                    target.FillRoundedRectangle(&rounded, &active_bg_brush);
                } else if is_hovered {
                    // 悬浮高亮
                    let hover_bg = color_f(0.22, 0.24, 0.29, 1.0);
                    let hover_bg_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &hover_bg)
                        .unwrap();
                    let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: item_rect,
                        radiusX: 4.0,
                        radiusY: 4.0,
                    };
                    target.FillRoundedRectangle(&rounded, &hover_bg_brush);
                }

                // 对话标题
                let title = if conv.title.is_empty() {
                    "未命名对话"
                } else {
                    &conv.title
                };
                let title_wide: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
                let item_format = self
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
                let title_rect = D2D_RECT_F {
                    left: x + 16.0,
                    top: item_y,
                    right: x + width - 40.0,
                    bottom: item_y + tab_item_height,
                };
                target.DrawText(
                    &title_wide,
                    &item_format,
                    &title_rect,
                    &text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 关闭按钮
                let close_btn_size = 16.0;
                let close_x = x + width - 8.0 - close_btn_size - 4.0;
                let close_rect = D2D_RECT_F {
                    left: close_x,
                    top: item_y + (tab_item_height - close_btn_size) / 2.0,
                    right: close_x + close_btn_size,
                    bottom: item_y + (tab_item_height + close_btn_size) / 2.0,
                };
                let close_text: Vec<u16> = "×".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &close_text,
                    &item_format,
                    &close_rect,
                    &dim_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 注册命中区域
                self.ai.ai_panel.agent_tab_regions.push((
                    i,
                    x + 8.0,
                    item_y,
                    width - 16.0 - close_btn_size - 8.0,
                    tab_item_height,
                ));
                self.ai.ai_panel.agent_tab_close_regions.push((
                    i,
                    close_x,
                    item_y,
                    close_btn_size + 8.0,
                    tab_item_height,
                ));
            }
            cy += conversations.len() as f32 * tab_item_height + 8.0;

            // 2. 工作区文件列表（直接渲染文件树，文件树自带“工作区”标题栏与操作按钮）
            let remaining_h = y + height - cy - 8.0;
            if remaining_h > 30.0 {
                // 记录文件树区域的 y 偏移量（相对于侧边栏顶部），供点击处理使用
                self.ai.ai_panel.agent_file_tree_offset_y = cy - y;
                self.render_file_tree_sidebar(target, x, cy, width, remaining_h, &text_brush);
            }
        }
    }
}
