use super::*;

impl EditorState {
    pub(super) fn render_file_tree_sidebar(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        let s = self.dpi_scale;
        unsafe {
            // 确保矢量图标几何已创建（FilePython / FileJava / FileText）
            self.icons.ensure_created_from_target(target);
            let ui_format = self
                .render_ctx
                .text_format_cache
                .get_format(
                    12.0 * s,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            // 章节标题：10px 加粗，紧凑风格
            let header_format = self
                .render_ctx
                .text_format_cache
                .get_format(
                    10.0 * s,
                    DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
                )
                .unwrap();
            let tree_format = self
                .render_ctx
                .text_format_cache
                .get_format(
                    11.0 * s,
                    DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            // 根目录行（工作区文件夹名）加粗显示（VS Code 风格）
            let tree_bold_format = self
                .render_ctx
                .text_format_cache
                .get_format(
                    11.0 * s,
                    DWRITE_FONT_WEIGHT_BOLD.0 as u32,
                    DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
                )
                .unwrap();
            let dir_color = color_f(0.9, 0.9, 0.9, 1.0);
            let dir_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &dir_color)
                .unwrap();
            let sel_color = if self.theme.glass_enabled {
                self.theme.glow_selection
            } else {
                color_f(0.0, 0.47, 0.83, 1.0)
            };
            let sel_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &sel_color)
                .unwrap();
            let hover_color = if self.theme.glass_enabled {
                color_f(0.25, 0.25, 0.27, 0.70)
            } else {
                color_f(0.2, 0.2, 0.2, 1.0)
            };
            let hover_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &hover_color)
                .unwrap();
            // 缩进参考线：白色 8% 细线（VS Code 风格）
            let guide_color = color_f(1.0, 1.0, 1.0, 0.08);
            let guide_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &guide_color)
                .unwrap();
            // 章节分隔线颜色
            let sep_color = color_f(0.2, 0.2, 0.2, 1.0);
            let sep_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &sep_color)
                .unwrap();
            let btn_hover_color = color_f(0.28, 0.28, 0.28, 1.0);
            let btn_hover_brush = self
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_hover_color)
                .unwrap();

            // 章节标题栏（与"源代码管理"风格一致，约 28px 高）
            let header_h = 28.0f32 * s;
            let header_text: Vec<u16> = "资源管理器".encode_utf16().chain(Some(0)).collect();
            let header_text_rect = D2D_RECT_F {
                left: x + 10.0 * s,
                top: y,
                right: x + width - 68.0 * s,
                bottom: y + header_h,
            };
            target.DrawText(
                &header_text,
                &header_format,
                &header_text_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 标题栏右侧：新建文件 / 打开文件夹按钮（紧凑小尺寸）
            let btn_size = 16.0f32 * s;
            let btn_margin = 4.0f32 * s;
            let new_file_rect = D2D_RECT_F {
                left: x + width - btn_size * 2.0 - btn_margin * 2.0,
                top: y + (header_h - btn_size) / 2.0,
                right: x + width - btn_size - btn_margin * 2.0,
                bottom: y + (header_h + btn_size) / 2.0,
            };
            let new_folder_rect = D2D_RECT_F {
                left: x + width - btn_size - btn_margin,
                top: y + (header_h - btn_size) / 2.0,
                right: x + width - btn_margin,
                bottom: y + (header_h + btn_size) / 2.0,
            };
            // 保存按钮区域供 hit test 使用
            self.file_tree_new_file_btn = Some(crate::layout::Region::new(
                new_file_rect.left,
                new_file_rect.top,
                new_file_rect.right - new_file_rect.left,
                new_file_rect.bottom - new_file_rect.top,
            ));
            self.file_tree_new_folder_btn = Some(crate::layout::Region::new(
                new_folder_rect.left,
                new_folder_rect.top,
                new_folder_rect.right - new_folder_rect.left,
                new_folder_rect.bottom - new_folder_rect.top,
            ));

            let nf_hover = self
                .file_tree_new_file_btn
                .as_ref()
                .map(|r| r.contains(self.hover.last_mouse_x, self.hover.last_mouse_y))
                .unwrap_or(false);
            let nfo_hover = self
                .file_tree_new_folder_btn
                .as_ref()
                .map(|r| r.contains(self.hover.last_mouse_x, self.hover.last_mouse_y))
                .unwrap_or(false);

            // 轻量化：常态不画背景色块，仅 hover 时显示浅色反馈
            if nf_hover {
                target.FillRectangle(&new_file_rect, &btn_hover_brush);
            }
            if nfo_hover {
                target.FillRectangle(&new_folder_rect, &btn_hover_brush);
            }

            // 矢量描边图标替代 emoji（➕/📁）：细线条、可缩放、与主题同色
            let icon_inset = 1.5f32 * s;
            self.icons.draw(
                target,
                crate::icons::IconKind::NewFile,
                new_file_rect.left + icon_inset,
                new_file_rect.top + icon_inset,
                btn_size - icon_inset * 2.0,
                btn_size - icon_inset * 2.0,
                text_brush,
            );
            self.icons.draw(
                target,
                crate::icons::IconKind::OpenFolder,
                new_folder_rect.left + icon_inset,
                new_folder_rect.top + icon_inset,
                btn_size - icon_inset * 2.0,
                btn_size - icon_inset * 2.0,
                text_brush,
            );

            // 标题下方的分隔线
            let sep_rect = D2D_RECT_F {
                left: x,
                top: y + header_h,
                right: x + width,
                bottom: y + header_h + 1.0 * s,
            };
            target.FillRectangle(&sep_rect, &sep_brush);

            // 文件树内联输入框（新建文件/文件夹时显示）
            // 该输入框的 y 偏移会通过 file_tree_list_start_y() 自动包含，
            // 此处仍需渲染输入框 UI。
            if let Some(input) = &self.file_tree_input {
                let input_y = y + header_h + 6.0 * s;
                let input_h = 26.0f32 * s;
                let input_rect = D2D_RECT_F {
                    left: x + 10.0 * s,
                    top: input_y,
                    right: x + width - 10.0 * s,
                    bottom: input_y + input_h,
                };
                let input_bg = color_f(0.12, 0.12, 0.12, 1.0);
                let input_bg_brush = self
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &input_bg)
                    .unwrap();
                let cursor_brush = self
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &self.theme.cursor_color)
                    .unwrap();
                target.FillRectangle(&input_rect, &input_bg_brush);
                target.DrawRectangle(&input_rect, &sep_brush, 1.0 * s, None);

                let value_text: Vec<u16> = input.value.encode_utf16().collect();
                let value_rect = D2D_RECT_F {
                    left: input_rect.left + 6.0 * s,
                    top: input_rect.top + 2.0 * s,
                    right: input_rect.right - 6.0 * s,
                    bottom: input_rect.bottom - 2.0 * s,
                };
                target.DrawText(
                    &value_text,
                    &ui_format,
                    &value_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                // 精确测量 value 文本宽度（支持 CJK 双宽字符）
                let ui_font_size = 13.0f32 * s;
                let value_width = self
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(
                        &input.value,
                        ui_font_size,
                        DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                    )
                    .unwrap_or(0.0);

                // IME 合成串（pre-edit text）显示在 value 之后
                let mut comp_width = 0.0f32;
                if let Some(comp) = &input.composition {
                    if !comp.is_empty() {
                        let comp_text: Vec<u16> = comp.encode_utf16().collect();
                        let comp_x = value_rect.left + value_width;
                        let comp_rect = D2D_RECT_F {
                            left: comp_x,
                            top: value_rect.top,
                            right: value_rect.right,
                            bottom: value_rect.bottom,
                        };
                        // 合成串用稍暗的颜色，带下划线效果
                        let comp_brush = self
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &color_f(1.0, 0.9, 0.4, 1.0))
                            .unwrap();
                        target.DrawText(
                            &comp_text,
                            &ui_format,
                            &comp_rect,
                            &comp_brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                        comp_width = self
                            .render_ctx
                            .text_format_cache
                            .measure_text_width(
                                comp,
                                ui_font_size,
                                DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                            )
                            .unwrap_or(0.0);
                    }
                }

                // 光标：使用精确测量的文本宽度定位
                if input.caret_visible {
                    let caret_x = value_rect.left + value_width + comp_width;
                    let caret_rect = D2D_RECT_F {
                        left: caret_x,
                        top: value_rect.top + 2.0 * s,
                        right: caret_x + 1.0 * s,
                        bottom: value_rect.bottom - 2.0 * s,
                    };
                    target.FillRectangle(&caret_rect, &cursor_brush);
                }
            }

            if self.file_tree.is_some() {
                let node_h = crate::layout::FILE_TREE_ROW_HEIGHT * s;
                let base_x = x + 10.0 * s;
                let arrow_w = 14.0 * s;
                // 根目录行：矢量 chevron + 加粗工作区文件夹名（与
                // handle_file_tree_click / update_local_tree_hover 共用同一公式，
                // 避免 dpi_scale / scroll / inline input 不一致时焦点错位）
                let root_top = y + self.file_tree_list_start_y();
                if self.hover_file_tree_root {
                    let hover_rect = D2D_RECT_F {
                        left: x,
                        top: root_top,
                        right: x + width,
                        bottom: root_top + node_h,
                    };
                    target.FillRectangle(&hover_rect, &hover_brush);
                }
                let chevron = if self.file_tree_root_expanded {
                    crate::icons::IconKind::ChevronDown
                } else {
                    crate::icons::IconKind::ChevronRight
                };
                let ch_size = 10.0 * s;
                self.icons.draw(
                    target,
                    chevron,
                    base_x + (arrow_w - ch_size) / 2.0,
                    root_top + (node_h - ch_size) / 2.0,
                    ch_size,
                    ch_size,
                    &dir_brush,
                );
                let root_name = self
                    .current_folder
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "工作区".to_string());
                let root_text_left = base_x + arrow_w + 5.0 * s;
                let max_text_w = (x + width - 10.0 * s - root_text_left).max(1.0);
                if let Ok(layout) = self.render_ctx.text_layout_cache.create_ellipsis_layout(
                    &root_name,
                    &tree_bold_format,
                    max_text_w,
                    node_h,
                ) {
                    let _ = layout.SetParagraphAlignment(
                        windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                    );
                    let point = D2D_POINT_2F {
                        x: root_text_left,
                        y: root_top,
                    };
                    target.DrawTextLayout(point, &layout, &dir_brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                }
            }

            if let Some(tree) = &self.file_tree {
                if self.file_tree_root_expanded {
                    // 节点列表从根目录行下方开始（公式与 file_tree_nodes_start_y 一致）
                    let mut current_y =
                        y + self.file_tree_list_start_y() + crate::layout::FILE_TREE_ROW_HEIGHT * s;
                    self.render_tree_nodes(
                        target,
                        tree,
                        u32::MAX,
                        x + 10.0 * s,
                        &mut current_y,
                        y,
                        height,
                        width,
                        &tree_format,
                        text_brush,
                        &dir_brush,
                        &sel_brush,
                        &hover_brush,
                        &guide_brush,
                    );
                }
            } else if self.file_tree_input.is_none() {
                let text: Vec<u16> = "按 Ctrl+K 打开文件夹"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let text_rect = D2D_RECT_F {
                    left: x + 10.0 * s,
                    top: y + header_h + 6.0 * s,
                    right: x + width - 10.0 * s,
                    bottom: y + header_h + 26.0 * s,
                };
                target.DrawText(
                    &text,
                    &ui_format,
                    &text_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_tree_nodes(
        &self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        tree: &FileTree,
        parent_idx: u32,
        base_x: f32,
        current_y: &mut f32,
        clip_y: f32,
        clip_height: f32,
        sidebar_width: f32,
        format: &windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
        dir_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
        sel_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
        hover_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
        guide_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) {
        let s = self.dpi_scale;
        let node_height = crate::layout::FILE_TREE_ROW_HEIGHT * s;
        // VS Code 风格两列布局：目录 = chevron + 名称（无文件夹图标），
        // 文件 = 类型图标（占据 chevron 列）+ 名称，同级名称对齐
        let arrow_w = 14.0f32 * s;
        let icon_size = 14.0f32 * s;
        let icon_gap = 4.0f32 * s;
        let mut child_idx = if parent_idx == u32::MAX {
            tree.first_root_node()
        } else {
            tree.get_node(parent_idx)
                .map(|n| n.first_child)
                .filter(|&c| c != u32::MAX)
        };

        while let Some(idx) = child_idx {
            if let Some(node) = tree.get_node(idx) {
                let next_sibling = if node.next_sibling != u32::MAX {
                    Some(node.next_sibling)
                } else {
                    None
                };

                if *current_y > clip_y + clip_height {
                    break;
                }

                if *current_y + node_height < clip_y {
                    *current_y += node_height;
                    if node.kind == FileKind::Directory && node.is_expanded {
                        self.skip_tree_nodes(tree, idx, current_y);
                    }
                    child_idx = next_sibling;
                    continue;
                }

                // 根目录行占据第 0 层，所有节点整体缩进一级（depth 0 → 16px）
                let indent = (node.depth as f32 + 1.0) * crate::layout::FILE_TREE_INDENT * s;
                let name = tree.get_name(node);

                let item_left = base_x + indent;
                let item_right = base_x + sidebar_width - 10.0 * s;
                // 高亮背景横跨整个侧边栏宽度（VS Code 风格），
                // 而非随缩进缩短，视觉上更整齐稳定
                let row_left = base_x - 10.0 * s;
                let row_right = row_left + sidebar_width;

                // 绘制悬停背景
                let is_hover = self.hover_file_node == Some(idx);
                if is_hover {
                    let hover_rect = D2D_RECT_F {
                        left: row_left,
                        top: *current_y,
                        right: row_right,
                        bottom: *current_y + node_height,
                    };
                    unsafe {
                        target.FillRectangle(&hover_rect, hover_brush);
                    }
                }

                // 绘制选中高亮背景（文件 + 目录都支持选中显示）
                let is_selected = self.selected_file_node == Some(idx);
                if is_selected {
                    let sel_rect = D2D_RECT_F {
                        left: row_left,
                        top: *current_y,
                        right: row_right,
                        bottom: *current_y + node_height,
                    };
                    unsafe {
                        target.FillRectangle(&sel_rect, sel_brush);
                    }
                }

                let brush = if node.kind == FileKind::Directory {
                    dir_brush
                } else {
                    text_brush
                };

                // 目录：Lucide 矢量 chevron（展开 v / 折叠 >，非三角形），不显示
                // 文件夹图标；文件：彩色类型图标占据 chevron 列，未命中扩展名
                // 时回退到通用 File 描边图标
                if node.kind == FileKind::Directory {
                    let chevron = if node.is_expanded {
                        crate::icons::IconKind::ChevronDown
                    } else {
                        crate::icons::IconKind::ChevronRight
                    };
                    let ch_size = 10.0 * s;
                    self.icons.draw(
                        target,
                        chevron,
                        item_left + (arrow_w - ch_size) / 2.0,
                        *current_y + (node_height - ch_size) / 2.0,
                        ch_size,
                        ch_size,
                        brush,
                    );
                } else {
                    let icon_kind = self
                        .get_file_vector_icon(name)
                        .unwrap_or(crate::icons::IconKind::File);
                    let icon_top = *current_y + (node_height - icon_size) / 2.0;
                    self.icons.draw(
                        target, icon_kind, item_left, icon_top, icon_size, icon_size, brush,
                    );
                }

                // 文件名：chevron/图标列之后，同级文件与目录名称对齐
                let text_left = item_left + arrow_w + icon_gap;

                unsafe {
                    // 单行 + 字符级“…”省略号：直接 IDWriteTextLayout 处理超长文件名
                    //（旧版用 DrawText 会在 text_rect 宽度不够时按字符换行，出现
                    // "project.private.config.js" 重叠堆叠成一坨的 bug）。
                    // 每次重绘重新创建 layout：节点数少、且 layout 轻量，
                    // 副作用是侧边栏拖动时省略号即时刷新（无缓存滞后）。
                    let max_text_w = (item_right - text_left).max(1.0);
                    let layout = self
                        .render_ctx
                        .text_layout_cache
                        .create_ellipsis_layout(name, format, max_text_w, node_height)
                        .unwrap();
                    // 文字在行内垂直居中（layout 高度 = 行高，默认顶对齐会偏上）
                    let _ = layout.SetParagraphAlignment(
                        windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                    );
                    let point = D2D_POINT_2F {
                        x: text_left,
                        y: *current_y,
                    };
                    target.DrawTextLayout(point, &layout, brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                }

                *current_y += node_height;

                if node.kind == FileKind::Directory && node.is_expanded {
                    let children_top = *current_y;
                    self.render_tree_nodes(
                        target,
                        tree,
                        idx,
                        base_x,
                        current_y,
                        clip_y,
                        clip_height,
                        sidebar_width,
                        format,
                        text_brush,
                        dir_brush,
                        sel_brush,
                        hover_brush,
                        guide_brush,
                    );

                    // 缩进参考线：子块左侧 1px 细线，对齐父目录 chevron 中心
                    //（根行占第 0 层，节点缩进整体 +1 级，公式对所有层级统一成立）
                    let child_indent =
                        (node.depth as f32 + 2.0) * crate::layout::FILE_TREE_INDENT * s;
                    // chevron 列宽 14，中心偏移 7 → child_indent - 16 + 7
                    let guide_x = base_x + child_indent - 9.0 * s;
                    let guide_top = children_top.max(clip_y);
                    let guide_bottom = (*current_y).min(clip_y + clip_height);
                    if guide_bottom > guide_top {
                        let guide_rect = D2D_RECT_F {
                            left: guide_x,
                            top: guide_top,
                            right: guide_x + 1.0 * s,
                            bottom: guide_bottom,
                        };
                        unsafe {
                            target.FillRectangle(&guide_rect, guide_brush);
                        }
                    }
                }

                child_idx = next_sibling;
            } else {
                break;
            }
        }
    }

    pub(super) fn skip_tree_nodes(&self, tree: &FileTree, parent_idx: u32, current_y: &mut f32) {
        let s = self.dpi_scale;
        let node_height = crate::layout::FILE_TREE_ROW_HEIGHT * s;
        let mut child_idx = tree
            .get_node(parent_idx)
            .map(|n| n.first_child)
            .filter(|&c| c != u32::MAX);
        while let Some(idx) = child_idx {
            if let Some(node) = tree.get_node(idx) {
                *current_y += node_height;
                if node.kind == FileKind::Directory && node.is_expanded {
                    self.skip_tree_nodes(tree, idx, current_y);
                }
                child_idx = if node.next_sibling != u32::MAX {
                    Some(node.next_sibling)
                } else {
                    None
                };
            } else {
                break;
            }
        }
    }
}
