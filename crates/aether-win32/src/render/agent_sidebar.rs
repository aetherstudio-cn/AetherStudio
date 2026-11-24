use super::*;

/// 智能体侧栏的工作区分组（渲染用）
struct SidebarWsGroup {
    name: String,
    path: String,
    hash: String,
    is_current: bool,
}

/// 侧栏会话行：打开中的标签（live_idx = Some）或历史库会话（None）
struct SidebarRow {
    conv_id: String,
    title: String,
    live_idx: Option<usize>,
}

/// 工作区图标底色：按名称稳定散列到一组主题色，形成可辨识的彩色项目图标
fn workspace_accent(name: &str) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    const PALETTE: [(f32, f32, f32); 6] = [
        (0.36, 0.56, 0.96),
        (0.30, 0.72, 0.50),
        (0.93, 0.58, 0.26),
        (0.88, 0.40, 0.40),
        (0.63, 0.47, 0.93),
        (0.20, 0.68, 0.72),
    ];
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let (r, g, b) = PALETTE[(h % PALETTE.len() as u64) as usize];
    color_f(r, g, b, 1.0)
}

impl EditorState {
    /// 智能体模式左侧边栏：工作区分组（工作区 → 会话）+ 新会话按钮
    ///
    /// 布局（从上到下）：
    /// 1. 新会话按钮
    /// 2. 「工作区」小节标题
    /// 3. 工作区分组：彩色图标块 + 名称，其下缩进列出该工作区的会话；
    ///    当前工作区展开打开中的标签并补充历史会话，其它最近工作区列出来自历史库的最近会话。
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
                left: x + width - 1.0,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            target.FillRectangle(&border_rect, &border_brush);

            let dim_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_muted())
                .unwrap();

            let mut cy = y + 10.0;

            // 1. 新会话按钮：强调色 tint 圆角胶囊 + 加号图标，悬停微动效提亮
            let new_chat_btn_h = 34.0;
            let new_chat_btn_rect = D2D_RECT_F {
                left: x + 8.0,
                top: cy,
                right: x + width - 8.0,
                bottom: cy + new_chat_btn_h,
            };
            let p_new = crate::ui_anim::quantize(self.ui.hover_anim.ai_btn_progress(0));
            let base = app_theme::accent_soft();
            let hover = app_theme::accent_soft_hover();
            let btn_bg = color_f(
                base.r + (hover.r - base.r) * p_new,
                base.g + (hover.g - base.g) * p_new,
                base.b + (hover.b - base.b) * p_new,
                1.0,
            );
            let btn_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_bg)
                .unwrap();
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: new_chat_btn_rect,
                radiusX: 10.0,
                radiusY: 10.0,
            };
            target.FillRoundedRectangle(&rounded, &btn_bg_brush);
            // 加号图标（左侧）+ 半粗体标题，形成明确的主操作锚点
            let icon_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent())
                .unwrap();
            self.ui.icons.draw(
                target,
                crate::icons::IconKind::Plus,
                x + 18.0,
                cy + (new_chat_btn_h - 14.0) / 2.0,
                14.0,
                14.0,
                &icon_brush,
            );

            let btn_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    13.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let new_chat_text: Vec<u16> = "新会话".encode_utf16().chain(Some(0)).collect();
            let new_chat_fg = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::accent_text())
                .unwrap();
            target.DrawText(
                &new_chat_text,
                &btn_format,
                &D2D_RECT_F {
                    left: new_chat_btn_rect.left + 14.0,
                    top: new_chat_btn_rect.top,
                    right: new_chat_btn_rect.right - 14.0,
                    bottom: new_chat_btn_rect.bottom,
                },
                &new_chat_fg,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            // 注册新会话按钮命中区域
            self.ai.ai_panel.agent_new_chat_region =
                Some((x + 8.0, cy, width - 16.0, new_chat_btn_h));
            cy += new_chat_btn_h + 14.0;

            // 2. 「工作区」小节标题
            let tabs_header_h = 22.0;
            let header_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    11.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let header_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &app_theme::text_muted())
                .unwrap();
            let section_title: Vec<u16> = "工作区".encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &section_title,
                &header_format,
                &D2D_RECT_F {
                    left: x + 10.0,
                    top: cy,
                    right: x + width - 10.0,
                    bottom: cy + tabs_header_h,
                },
                &header_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += tabs_header_h + 2.0;

            // 分组数据：当前工作区在前，随后是最近项目（去重）
            let current_path = self
                .fs
                .current_folder
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());
            let mut groups: Vec<SidebarWsGroup> = Vec::new();
            if let Some(path) = current_path.clone() {
                let name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "未打开工作区".to_string());
                let hash = crate::editor::files::workspace_path_hash(std::path::Path::new(&path));
                groups.push(SidebarWsGroup {
                    name,
                    path,
                    hash,
                    is_current: true,
                });
            }
            for proj in self.ui.recent_projects.list() {
                if current_path.as_deref() == Some(proj.path.as_str()) {
                    continue;
                }
                let hash =
                    crate::editor::files::workspace_path_hash(std::path::Path::new(&proj.path));
                groups.push(SidebarWsGroup {
                    name: proj.name,
                    path: proj.path,
                    hash,
                    is_current: false,
                });
            }

            // 工作区会话：当前工作区用打开中的标签 + 历史补充，其余工作区用历史库缓存。
            // 节流查库（缺失/过期时重建，避免每帧扫描 AetherDB）。
            let all_hashes: Vec<String> = groups.iter().map(|g| g.hash.clone()).collect();
            self.ensure_agent_workspace_conv_cache(&all_hashes);

            // 命中区与字体：渲染每帧重建
            self.ai.ai_panel.agent_tab_regions.clear();
            self.ai.ai_panel.agent_tab_close_regions.clear();
            self.ai.ai_panel.agent_workspace_regions.clear();
            self.ai.ai_panel.agent_ws_conv_regions.clear();

            let ws_name_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let conv_item_format = self
                .win
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();

            let ws_header_h = 26.0;
            let row_h = 28.0;
            let row_pitch = 30.0;
            let group_gap = 6.0;
            let bottom_limit = y + height - 8.0;
            let active_idx = self.ai.ai_panel.active;
            // 历史会话行的扁平命中下标（与 agent_ws_conv_regions 对齐）
            let mut ws_conv_flat: usize = 0;

            for group in groups.iter() {
                if cy + ws_header_h > bottom_limit {
                    break;
                }

                // —— 工作区分组头：彩色圆角图标块 + 名称 ——
                let accent_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &workspace_accent(&group.name))
                    .unwrap();
                let tile = 16.0f32;
                let tile_x = x + 10.0;
                let tile_y = cy + (ws_header_h - tile) / 2.0;
                let tile_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: tile_x,
                        top: tile_y,
                        right: tile_x + tile,
                        bottom: tile_y + tile,
                    },
                    radiusX: 4.0,
                    radiusY: 4.0,
                };
                target.FillRoundedRectangle(&tile_rounded, &accent_brush);
                let glyph_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(1.0, 1.0, 1.0, 0.92))
                    .unwrap();
                self.ui.icons.draw(
                    target,
                    crate::icons::IconKind::Folder,
                    tile_x + 3.0,
                    tile_y + 3.0,
                    10.0,
                    10.0,
                    &glyph_brush,
                );

                let ws_name_shown = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .truncate_with_ellipsis(
                        &group.name,
                        12.0,
                        windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_SEMI_BOLD.0
                            as u32,
                        width - 34.0 - 12.0,
                    );
                let name_wide: Vec<u16> = ws_name_shown.encode_utf16().chain(Some(0)).collect();
                let ws_name_color = if group.is_current {
                    app_theme::text_primary()
                } else {
                    app_theme::text_secondary()
                };
                let ws_name_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &ws_name_color)
                    .unwrap();
                target.DrawText(
                    &name_wide,
                    &ws_name_format,
                    &D2D_RECT_F {
                        left: x + 34.0,
                        top: cy,
                        right: x + width - 10.0,
                        bottom: cy + ws_header_h,
                    },
                    &ws_name_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                if !group.is_current {
                    self.ai.ai_panel.agent_workspace_regions.push((
                        group.path.clone(),
                        x + 8.0,
                        cy,
                        width - 16.0,
                        ws_header_h,
                    ));
                }
                cy += ws_header_h + 2.0;

                // —— 会话行：缩进展示，选中标签用中性灰圆角底突出 ——
                let mut rows: Vec<SidebarRow> = Vec::new();
                if group.is_current {
                    for (i, conv) in self.ai.ai_panel.conversations.iter().enumerate() {
                        rows.push(SidebarRow {
                            conv_id: conv.id.clone(),
                            title: conv.title.clone(),
                            live_idx: Some(i),
                        });
                    }
                    // 历史补充：当前工作区未打开的最近会话（与标签页去重）
                    let empty: Vec<crate::ai_panel::ConversationMeta> = Vec::new();
                    let history = self.ai.sidebar_ws_cache.get(&group.hash).unwrap_or(&empty);
                    for meta in history.iter() {
                        if self
                            .ai
                            .ai_panel
                            .conversations
                            .iter()
                            .any(|c| c.id == meta.id)
                        {
                            continue;
                        }
                        rows.push(SidebarRow {
                            conv_id: meta.id.clone(),
                            title: meta.title.clone(),
                            live_idx: None,
                        });
                    }
                } else {
                    let empty: Vec<crate::ai_panel::ConversationMeta> = Vec::new();
                    let history = self.ai.sidebar_ws_cache.get(&group.hash).unwrap_or(&empty);
                    for meta in history.iter() {
                        rows.push(SidebarRow {
                            conv_id: meta.id.clone(),
                            title: meta.title.clone(),
                            live_idx: None,
                        });
                    }
                }

                for row in rows.iter() {
                    if cy + row_h > bottom_limit {
                        break;
                    }
                    let row_rect = D2D_RECT_F {
                        left: x + 8.0,
                        top: cy,
                        right: x + width - 8.0,
                        bottom: cy + row_h,
                    };
                    let is_active = row.live_idx == Some(active_idx);
                    let live_hovered = row
                        .live_idx
                        .is_some_and(|i| self.ai.ai_panel.hover_tab == Some(i));
                    if is_active {
                        // 选中态：中性灰圆角底（参考 Qoder），降低视觉噪声
                        let active_bg = app_theme::accent_soft_hover();
                        let active_bg_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &active_bg)
                            .unwrap();
                        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                            rect: row_rect,
                            radiusX: 7.0,
                            radiusY: 7.0,
                        };
                        target.FillRoundedRectangle(&rounded, &active_bg_brush);
                    } else if let Some(i) = row.live_idx {
                        let p = crate::ui_anim::quantize(self.ui.hover_anim.ai_tab_progress(i));
                        if p > 0.0 {
                            let mut hover_bg = app_theme::accent_soft_hover();
                            hover_bg.a = p;
                            let hover_bg_brush = self
                                .win
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &hover_bg)
                                .unwrap();
                            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                                rect: row_rect,
                                radiusX: 7.0,
                                radiusY: 7.0,
                            };
                            target.FillRoundedRectangle(&rounded, &hover_bg_brush);
                        }
                    } else if self.ai.ai_panel.hover_ws_conv == Some(ws_conv_flat) {
                        let mut hover_bg = app_theme::accent_soft_hover();
                        hover_bg.a = 0.55;
                        let hover_bg_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &hover_bg)
                            .unwrap();
                        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                            rect: row_rect,
                            radiusX: 7.0,
                            radiusY: 7.0,
                        };
                        target.FillRoundedRectangle(&rounded, &hover_bg_brush);
                    }

                    // 会话标题（标签页过宽留出关闭按钮位置）
                    let title = if row.title.is_empty() {
                        "未命名对话"
                    } else {
                        row.title.as_str()
                    };
                    let title_right = if row.live_idx.is_some() {
                        x + width - 40.0
                    } else {
                        x + width - 14.0
                    };
                    let title_rect = D2D_RECT_F {
                        left: x + 38.0,
                        top: cy,
                        right: title_right,
                        bottom: cy + row_h,
                    };
                    let title_shown = self
                        .win
                        .render_ctx
                        .text_format_cache
                        .truncate_with_ellipsis(
                            title,
                            12.0,
                            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL.0
                                as u32,
                            title_rect.right - title_rect.left,
                        );
                    let title_wide: Vec<u16> = title_shown.encode_utf16().chain(Some(0)).collect();
                    let title_color = if is_active {
                        app_theme::accent_text()
                    } else if row.live_idx.is_some() {
                        self.win.theme.text_default
                    } else {
                        app_theme::text_secondary()
                    };
                    let title_draw_brush = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &title_color)
                        .unwrap();
                    target.DrawText(
                        &title_wide,
                        &conv_item_format,
                        &title_rect,
                        &title_draw_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );

                    if let Some(i) = row.live_idx {
                        // 关闭按钮：仅激活/悬停时清晰可见，其余淡出降低视觉噪声
                        let close_btn_size = 16.0;
                        let close_x = x + width - 8.0 - close_btn_size - 4.0;
                        let close_rect = D2D_RECT_F {
                            left: close_x,
                            top: cy + (row_h - close_btn_size) / 2.0,
                            right: close_x + close_btn_size,
                            bottom: cy + (row_h + close_btn_size) / 2.0,
                        };
                        let faint_close_brush = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &app_theme::text_secondary())
                            .unwrap();
                        let close_brush = if is_active || live_hovered {
                            &dim_brush
                        } else {
                            &faint_close_brush
                        };
                        let close_text: Vec<u16> = "×".encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &close_text,
                            &conv_item_format,
                            &close_rect,
                            close_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );

                        // 注册标签页命中区域
                        self.ai.ai_panel.agent_tab_regions.push((
                            i,
                            x + 8.0,
                            cy,
                            width - 16.0 - close_btn_size - 8.0,
                            row_h,
                        ));
                        self.ai.ai_panel.agent_tab_close_regions.push((
                            i,
                            close_x,
                            cy,
                            close_btn_size + 8.0,
                            row_h,
                        ));
                    } else {
                        // 注册历史会话命中区域（点击打开为标签页）
                        self.ai.ai_panel.agent_ws_conv_regions.push((
                            group.path.clone(),
                            row.conv_id.clone(),
                            x + 8.0,
                            cy,
                            width - 16.0,
                            row_h,
                        ));
                        ws_conv_flat += 1;
                    }
                    cy += row_pitch;
                }
                cy += group_gap;
            }
        }
    }
}
