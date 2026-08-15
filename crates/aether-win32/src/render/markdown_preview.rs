use super::*;

/// Markdown 预览渲染器
///
/// 解析 Markdown 文本并用 DirectWrite/Direct2D 渲染为格式化视图。
/// 支持：标题(H1-H4)、粗体、斜体、行内代码、代码块、无序/有序列表、
/// 链接文本、分割线、引用块。
impl EditorState {
    /// 渲染 Markdown 预览/编辑切换按钮（编辑区右上角，SVG 图标）
    pub(super) fn render_markdown_toggle_btn(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
    ) {
        let btn_size: f32 = 28.0;
        let btn_margin: f32 = 8.0;
        let btn_x = x + width - btn_size - btn_margin;
        let btn_y = y + btn_margin;

        // 保存按钮区域供点击命中检测
        self.editor.markdown_toggle_btn =
            Some(crate::layout::Region::new(btn_x, btn_y, btn_size, btn_size));

        let is_preview = self.editor.markdown_preview;
        let icon = if is_preview {
            crate::icons::IconKind::Pencil
        } else {
            crate::icons::IconKind::Eye
        };

        unsafe {
            // 按钮背景（半透明）
            let bg_color = if is_preview {
                color_f(0.25, 0.45, 0.75, 0.7)
            } else {
                color_f(0.3, 0.3, 0.35, 0.6)
            };
            let bg_brush = match self.win.render_ctx.brush_cache.get_brush(target, &bg_color) {
                Ok(b) => b,
                Err(_) => return,
            };
            target.FillRectangle(
                &D2D_RECT_F {
                    left: btn_x,
                    top: btn_y,
                    right: btn_x + btn_size,
                    bottom: btn_y + btn_size,
                },
                &bg_brush,
            );

            // SVG 图标
            let icon_brush = match self
    .win.render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.9, 0.9, 0.9, 1.0))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            let icon_size = 16.0;
            let icon_x = btn_x + (btn_size - icon_size) / 2.0;
            let icon_y = btn_y + (btn_size - icon_size) / 2.0;
            self.ui.icons.ensure_created_from_target(target);
            self.ui.icons.draw(
                target,
                icon,
                icon_x,
                icon_y,
                icon_size,
                icon_size,
                &icon_brush,
            );
        }
    }

    /// 渲染 Markdown 预览
    pub(super) fn render_markdown_preview(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        unsafe {
            // 背景
            let bg_brush = match self
    .win.render_ctx
                .brush_cache
                .get_brush(target, &self.win.theme.editor_bg)
            {
                Ok(b) => b,
                Err(_) => return,
            };
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + width,
                    bottom: y + height,
                },
                &bg_brush,
            );

            // 读取当前 buffer 文本
            let text = self
    .editor.content
                .buffer
                .get_text(0, self.editor.content.buffer.len_bytes());
            if text.is_empty() {
                self.render_markdown_empty(target, x, y, width, height);
                return;
            }

            // 解析 Markdown 为渲染行
            let lines = parse_markdown_lines(&text);

            // 渲染参数
            let padding: f32 = 24.0;
            let content_x = x + padding;
            let content_width = (width - padding * 2.0).max(100.0);
            let line_height = self.win.text_renderer.line_height();
            let scroll_y = self.editor.content.scroll_y;

            // 裁剪区域
            target.PushAxisAlignedClip(
                &D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + width,
                    bottom: y + height,
                },
                D2D1_ANTIALIAS_MODE_ALIASED,
            );

            let mut cy = y + padding - scroll_y;

            for line in &lines {
                let lh = match line {
                    MdRenderLine::Heading { level, .. } => line_height * heading_scale(*level),
                    MdRenderLine::CodeBlock { .. } => line_height * 1.1,
                    MdRenderLine::Divider => line_height * 0.8,
                    _ => line_height,
                };

                // 跳过不可见行
                if cy + lh < y {
                    cy += lh;
                    continue;
                }
                if cy > y + height {
                    break;
                }

                self.render_md_line(target, line, content_x, cy, content_width, lh);
                cy += lh;
            }

            target.PopAxisAlignedClip();
        }
    }

    /// 渲染空 Markdown 提示
    unsafe fn render_markdown_empty(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        let text_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.5, 0.5, 0.55, 1.0))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let format = match self.win.render_ctx.text_format_cache.get_format(
            14.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_CENTER.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let hint: Vec<u16> = "空 Markdown 文档".encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &hint,
            &format,
            &D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            },
            &text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// 渲染单行 Markdown 元素
    unsafe fn render_md_line(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        line: &MdRenderLine,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        match line {
            MdRenderLine::Heading { level, text } => {
                self.render_md_heading(target, *level, text, x, y, width, line_height);
            }
            MdRenderLine::Paragraph { segments } => {
                self.render_md_paragraph(target, segments, x, y, width, line_height);
            }
            MdRenderLine::UnorderedListItem { segments, indent } => {
                self.render_md_list_item(
                    target,
                    segments,
                    *indent,
                    false,
                    x,
                    y,
                    width,
                    line_height,
                );
            }
            MdRenderLine::OrderedListItem {
                number,
                segments,
                indent,
            } => {
                self.render_md_ordered_list_item(
                    target,
                    *number,
                    segments,
                    *indent,
                    x,
                    y,
                    width,
                    line_height,
                );
            }
            MdRenderLine::CodeBlock { text, .. } => {
                self.render_md_code_block(target, text, x, y, width, line_height);
            }
            MdRenderLine::Quote { segments } => {
                self.render_md_quote(target, segments, x, y, width, line_height);
            }
            MdRenderLine::Divider => {
                self.render_md_divider(target, x, y, width, line_height);
            }
            MdRenderLine::Empty => {
                // 空行，仅占空间
            }
        }
    }

    /// 渲染标题
    unsafe fn render_md_heading(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        level: u8,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        let font_size = heading_font_size(level);
        let text_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &self.win.theme.text_default)
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let format = match self.win.render_ctx.text_format_cache.get_format(
            font_size,
            DWRITE_FONT_WEIGHT_BOLD.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &wide,
            &format,
            &D2D_RECT_F {
                left: x,
                top: y,
                right: x + width,
                bottom: y + line_height,
            },
            &text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // H1/H2 下方绘制分割线
        if level <= 2 {
            let sep_brush = match self
    .win.render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.3, 0.3, 0.3, 0.5))
            {
                Ok(b) => b,
                Err(_) => return,
            };
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y + line_height - 1.0,
                    right: x + width,
                    bottom: y + line_height,
                },
                &sep_brush,
            );
        }
    }

    /// 渲染段落（含行内格式）
    unsafe fn render_md_paragraph(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        segments: &[MdSegment],
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        // 先拼接纯文本用于 TextLayout
        let plain: String = segments.iter().map(|s| s.text.as_str()).collect();
        if plain.is_empty() {
            return;
        }

        let text_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &self.win.theme.text_default)
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let format = match self.win.render_ctx.text_format_cache.get_format(
            13.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };

        let wide: Vec<u16> = plain.encode_utf16().chain(Some(0)).collect();

        // 使用 TextLayout 支持富文本范围样式
        let dwrite = self.win.text_renderer.dwrite_factory();
        let layout = match dwrite.CreateTextLayout(
            &wide[..wide.len() - 1],
            &format,
            width,
            line_height * 2.0,
        ) {
            Ok(l) => l,
            Err(_) => {
                // fallback: 纯文本绘制
                target.DrawText(
                    &wide,
                    &format,
                    &D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + width,
                        bottom: y + line_height,
                    },
                    &text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                return;
            }
        };

        // 应用行内样式
        let mut offset: u32 = 0;
        for seg in segments {
            let seg_len = seg.text.encode_utf16().count() as u32;
            if seg_len == 0 {
                continue;
            }
            let range = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE {
                startPosition: offset,
                length: seg_len,
            };
            if seg.bold {
                let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range);
            }
            if seg.italic {
                let _ = layout.SetFontStyle(
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_ITALIC,
                    range,
                );
            }
            if seg.code {
                // 行内代码用不同颜色绘制（通过 brush 区分，字体族名需要 PCWSTR，此处省略）
            }
            offset += seg_len;
        }

        target.DrawTextLayout(
            D2D_POINT_2F { x, y },
            &layout,
            &text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }

    /// 渲染无序列表项
    unsafe fn render_md_list_item(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        segments: &[MdSegment],
        indent: usize,
        _ordered: bool,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        let indent_px = indent as f32 * 20.0;
        let bullet_x = x + indent_px;

        // 绘制圆点
        let bullet_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &self.win.theme.text_default)
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let bullet_format = match self.win.render_ctx.text_format_cache.get_format(
            13.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let bullet: Vec<u16> = "\u{2022} ".encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &bullet,
            &bullet_format,
            &D2D_RECT_F {
                left: bullet_x,
                top: y,
                right: bullet_x + 20.0,
                bottom: y + line_height,
            },
            &bullet_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        // 绘制内容
        self.render_md_paragraph(
            target,
            segments,
            bullet_x + 20.0,
            y,
            width - indent_px - 20.0,
            line_height,
        );
    }

    /// 渲染有序列表项
    unsafe fn render_md_ordered_list_item(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        number: usize,
        segments: &[MdSegment],
        indent: usize,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        let indent_px = indent as f32 * 20.0;
        let num_x = x + indent_px;

        let num_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &self.win.theme.text_default)
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let num_format = match self.win.render_ctx.text_format_cache.get_format(
            13.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let num_text = format!("{}. ", number);
        let num_wide: Vec<u16> = num_text.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &num_wide,
            &num_format,
            &D2D_RECT_F {
                left: num_x,
                top: y,
                right: num_x + 30.0,
                bottom: y + line_height,
            },
            &num_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );

        self.render_md_paragraph(
            target,
            segments,
            num_x + 30.0,
            y,
            width - indent_px - 30.0,
            line_height,
        );
    }

    /// 渲染代码块
    unsafe fn render_md_code_block(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        // 代码块背景
        let code_bg = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.15, 0.15, 0.18, 1.0))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        target.FillRectangle(
            &D2D_RECT_F {
                left: x - 4.0,
                top: y,
                right: x + width + 4.0,
                bottom: y + line_height,
            },
            &code_bg,
        );

        let code_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.85, 0.85, 0.85, 1.0))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let format = match self.win.render_ctx.text_format_cache.get_format(
            12.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &wide,
            &format,
            &D2D_RECT_F {
                left: x + 8.0,
                top: y,
                right: x + width,
                bottom: y + line_height,
            },
            &code_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// 渲染引用块
    unsafe fn render_md_quote(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        segments: &[MdSegment],
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        // 左侧竖线
        let bar_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.4, 0.6, 0.9, 1.0))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        target.FillRectangle(
            &D2D_RECT_F {
                left: x,
                top: y,
                right: x + 3.0,
                bottom: y + line_height,
            },
            &bar_brush,
        );

        // 引用文本（灰色）
        let quote_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.6, 0.6, 0.65, 1.0))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let plain: String = segments.iter().map(|s| s.text.as_str()).collect();
        let format = match self.win.render_ctx.text_format_cache.get_format(
            13.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER.0 as u32,
        ) {
            Ok(f) => f,
            Err(_) => return,
        };
        let wide: Vec<u16> = plain.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &wide,
            &format,
            &D2D_RECT_F {
                left: x + 12.0,
                top: y,
                right: x + width,
                bottom: y + line_height,
            },
            &quote_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// 渲染分割线
    unsafe fn render_md_divider(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        line_height: f32,
    ) {
        let sep_brush = match self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.4, 0.4, 0.4, 0.6))
        {
            Ok(b) => b,
            Err(_) => return,
        };
        let mid_y = y + line_height / 2.0;
        target.FillRectangle(
            &D2D_RECT_F {
                left: x,
                top: mid_y,
                right: x + width,
                bottom: mid_y + 1.0,
            },
            &sep_brush,
        );
    }
}

// ── Markdown 解析 ──

/// 标题字号缩放系数
fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.8,
        2 => 1.5,
        3 => 1.3,
        4 => 1.15,
        _ => 1.0,
    }
}

/// 标题字号
fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => 24.0,
        2 => 20.0,
        3 => 17.0,
        4 => 15.0,
        _ => 13.0,
    }
}

/// Markdown 行内片段
#[derive(Clone, Debug)]
struct MdSegment {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    #[allow(dead_code)]
    link: Option<String>,
}

/// Markdown 渲染行
#[derive(Clone, Debug)]
enum MdRenderLine {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        segments: Vec<MdSegment>,
    },
    UnorderedListItem {
        segments: Vec<MdSegment>,
        indent: usize,
    },
    OrderedListItem {
        number: usize,
        segments: Vec<MdSegment>,
        indent: usize,
    },
    CodeBlock {
        text: String,
        #[allow(dead_code)]
        lang: String,
    },
    Quote {
        segments: Vec<MdSegment>,
    },
    Divider,
    Empty,
}

/// 将 Markdown 文本解析为渲染行列表
fn parse_markdown_lines(text: &str) -> Vec<MdRenderLine> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for raw_line in text.lines() {
        let line = raw_line;

        // 代码块围栏
        if line.trim_start().starts_with("```") {
            if in_code_block {
                in_code_block = false;
                code_lang.clear();
            } else {
                in_code_block = true;
                code_lang = line.trim_start().trim_start_matches('`').trim().to_string();
            }
            continue;
        }

        if in_code_block {
            lines.push(MdRenderLine::CodeBlock {
                text: line.to_string(),
                lang: code_lang.clone(),
            });
            continue;
        }

        let trimmed = line.trim();

        // 空行
        if trimmed.is_empty() {
            lines.push(MdRenderLine::Empty);
            continue;
        }

        // 分割线
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            lines.push(MdRenderLine::Divider);
            continue;
        }

        // 标题
        if let Some(h) = parse_heading(trimmed) {
            lines.push(h);
            continue;
        }

        // 引用
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let segments = parse_inline_segments(rest);
            lines.push(MdRenderLine::Quote { segments });
            continue;
        }
        if trimmed == ">" {
            lines.push(MdRenderLine::Quote { segments: vec![] });
            continue;
        }

        // 无序列表
        if let Some(item) = parse_unordered_list(line) {
            lines.push(item);
            continue;
        }

        // 有序列表
        if let Some(item) = parse_ordered_list(line) {
            lines.push(item);
            continue;
        }

        // 普通段落（含行内格式）
        let segments = parse_inline_segments(trimmed);
        lines.push(MdRenderLine::Paragraph { segments });
    }

    lines
}

/// 解析标题行
fn parse_heading(line: &str) -> Option<MdRenderLine> {
    for level in (1..=4u8).rev() {
        let prefix = "#".repeat(level as usize) + " ";
        if let Some(text) = line.strip_prefix(&prefix) {
            return Some(MdRenderLine::Heading {
                level,
                text: text.to_string(),
            });
        }
    }
    None
}

/// 解析无序列表行
fn parse_unordered_list(line: &str) -> Option<MdRenderLine> {
    let trimmed = line.trim_start();
    let indent = (line.len() - trimmed.len()) / 2; // 每 2 空格一级缩进

    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;

    let segments = parse_inline_segments(rest);
    Some(MdRenderLine::UnorderedListItem { segments, indent })
}

/// 解析有序列表行
fn parse_ordered_list(line: &str) -> Option<MdRenderLine> {
    let trimmed = line.trim_start();
    let indent = (line.len() - trimmed.len()) / 2;

    let dot_pos = trimmed.find(". ")?;
    let num_str = &trimmed[..dot_pos];
    let number: usize = num_str.parse().ok()?;

    let rest = &trimmed[dot_pos + 2..];
    let segments = parse_inline_segments(rest);
    Some(MdRenderLine::OrderedListItem {
        number,
        segments,
        indent,
    })
}

/// 解析行内格式（粗体、斜体、行内代码、链接）
fn parse_inline_segments(text: &str) -> Vec<MdSegment> {
    let mut segments = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut current = String::new();

    while i < chars.len() {
        // 行内代码 `code`
        if chars[i] == '`' {
            if !current.is_empty() {
                segments.push(MdSegment {
                    text: std::mem::take(&mut current),
                    bold: false,
                    italic: false,
                    code: false,
                    link: None,
                });
            }
            if let Some(end) = find_char(&chars, i + 1, '`') {
                let code_text: String = chars[i + 1..end].iter().collect();
                segments.push(MdSegment {
                    text: code_text,
                    bold: false,
                    italic: false,
                    code: true,
                    link: None,
                });
                i = end + 1;
                continue;
            }
        }

        // 粗体 **text**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if !current.is_empty() {
                segments.push(MdSegment {
                    text: std::mem::take(&mut current),
                    bold: false,
                    italic: false,
                    code: false,
                    link: None,
                });
            }
            if let Some(end) = find_double_star(&chars, i + 2) {
                let bold_text: String = chars[i + 2..end].iter().collect();
                segments.push(MdSegment {
                    text: bold_text,
                    bold: true,
                    italic: false,
                    code: false,
                    link: None,
                });
                i = end + 2;
                continue;
            }
        }

        // 斜体 *text*
        if chars[i] == '*' && (i + 1 >= chars.len() || chars[i + 1] != '*') {
            if !current.is_empty() {
                segments.push(MdSegment {
                    text: std::mem::take(&mut current),
                    bold: false,
                    italic: false,
                    code: false,
                    link: None,
                });
            }
            if let Some(end) = find_char(&chars, i + 1, '*') {
                let italic_text: String = chars[i + 1..end].iter().collect();
                if !italic_text.is_empty() {
                    segments.push(MdSegment {
                        text: italic_text,
                        bold: false,
                        italic: true,
                        code: false,
                        link: None,
                    });
                    i = end + 1;
                    continue;
                }
            }
        }

        // 链接 [text](url)
        if chars[i] == '[' {
            if let Some(close) = find_char(&chars, i + 1, ']') {
                if close + 1 < chars.len() && chars[close + 1] == '(' {
                    if let Some(paren_end) = find_char(&chars, close + 2, ')') {
                        if !current.is_empty() {
                            segments.push(MdSegment {
                                text: std::mem::take(&mut current),
                                bold: false,
                                italic: false,
                                code: false,
                                link: None,
                            });
                        }
                        let link_text: String = chars[i + 1..close].iter().collect();
                        let url: String = chars[close + 2..paren_end].iter().collect();
                        segments.push(MdSegment {
                            text: link_text,
                            bold: false,
                            italic: false,
                            code: false,
                            link: Some(url),
                        });
                        i = paren_end + 1;
                        continue;
                    }
                }
            }
        }

        current.push(chars[i]);
        i += 1;
    }

    if !current.is_empty() {
        segments.push(MdSegment {
            text: current,
            bold: false,
            italic: false,
            code: false,
            link: None,
        });
    }

    segments
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_double_star(chars: &[char], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        let lines = parse_markdown_lines("# Hello\n## World\n### Sub\n#### H4");
        assert!(matches!(&lines[0], MdRenderLine::Heading { level: 1, text } if text == "Hello"));
        assert!(matches!(&lines[1], MdRenderLine::Heading { level: 2, text } if text == "World"));
        assert!(matches!(&lines[2], MdRenderLine::Heading { level: 3, text } if text == "Sub"));
        assert!(matches!(&lines[3], MdRenderLine::Heading { level: 4, text } if text == "H4"));
    }

    #[test]
    fn test_parse_code_block() {
        let lines = parse_markdown_lines("```rust\nfn main() {}\n```");
        assert!(
            matches!(&lines[0], MdRenderLine::CodeBlock { text, .. } if text == "fn main() {}")
        );
    }

    #[test]
    fn test_parse_unordered_list() {
        let lines = parse_markdown_lines("- item 1\n- item 2");
        assert!(matches!(&lines[0], MdRenderLine::UnorderedListItem { .. }));
        assert!(matches!(&lines[1], MdRenderLine::UnorderedListItem { .. }));
    }

    #[test]
    fn test_parse_ordered_list() {
        let lines = parse_markdown_lines("1. first\n2. second");
        assert!(matches!(
            &lines[0],
            MdRenderLine::OrderedListItem { number: 1, .. }
        ));
        assert!(matches!(
            &lines[1],
            MdRenderLine::OrderedListItem { number: 2, .. }
        ));
    }

    #[test]
    fn test_parse_divider() {
        let lines = parse_markdown_lines("---\n***\n___");
        assert!(matches!(&lines[0], MdRenderLine::Divider));
        assert!(matches!(&lines[1], MdRenderLine::Divider));
        assert!(matches!(&lines[2], MdRenderLine::Divider));
    }

    #[test]
    fn test_parse_quote() {
        let lines = parse_markdown_lines("> quoted text");
        assert!(matches!(&lines[0], MdRenderLine::Quote { .. }));
    }

    #[test]
    fn test_parse_bold() {
        let segs = parse_inline_segments("hello **world** end");
        assert_eq!(segs.len(), 3);
        assert!(!segs[0].bold);
        assert!(segs[1].bold);
        assert_eq!(segs[1].text, "world");
        assert!(!segs[2].bold);
    }

    #[test]
    fn test_parse_inline_code() {
        let segs = parse_inline_segments("use `println!` macro");
        assert_eq!(segs.len(), 3);
        assert!(segs[1].code);
        assert_eq!(segs[1].text, "println!");
    }

    #[test]
    fn test_parse_link() {
        let segs = parse_inline_segments("visit [Rust](https://rust-lang.org) site");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[1].text, "Rust");
        assert_eq!(segs[1].link.as_deref(), Some("https://rust-lang.org"));
    }

    #[test]
    fn test_parse_empty_line() {
        let lines = parse_markdown_lines("hello\n\nworld");
        assert_eq!(lines.len(), 3);
        assert!(matches!(&lines[1], MdRenderLine::Empty));
    }

    #[test]
    fn test_parse_italic() {
        let segs = parse_inline_segments("this is *italic* text");
        assert_eq!(segs.len(), 3);
        assert!(segs[1].italic);
        assert_eq!(segs[1].text, "italic");
    }
}
