use super::*;

impl EditorState {
    /// 渲染一个参数滑块（轨道 + 填充 + 旋钮）；disabled 时灰化。
    /// 返回滑块轨道命中区 (x, y, w, h)。
    unsafe fn render_param_slider(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        track_x: f32,
        track_y: f32,
        track_w: f32,
        ratio: f32,
        disabled: bool,
    ) -> (f32, f32, f32, f32) {
        let track_h = 4.0_f32;
        let knob_cx = track_x + track_w * ratio.clamp(0.0, 1.0);
        let track_bg = color_f(0.30, 0.30, 0.33, 1.0);
        let track_bg_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &track_bg)
            .unwrap();
        target.FillRectangle(
            &D2D_RECT_F {
                left: track_x,
                top: track_y,
                right: track_x + track_w,
                bottom: track_y + track_h,
            },
            &track_bg_brush,
        );
        let track_fill = if disabled {
            color_f(0.40, 0.40, 0.43, 1.0)
        } else {
            color_f(0.0, 0.47, 0.83, 1.0)
        };
        let track_fill_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &track_fill)
            .unwrap();
        target.FillRectangle(
            &D2D_RECT_F {
                left: track_x,
                top: track_y,
                right: knob_cx,
                bottom: track_y + track_h,
            },
            &track_fill_brush,
        );
        let knob_r = 8.0_f32;
        let knob_color = if disabled {
            color_f(0.55, 0.55, 0.58, 1.0)
        } else {
            color_f(0.95, 0.95, 0.95, 1.0)
        };
        let knob_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &knob_color)
            .unwrap();
        target.FillEllipse(
            &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: knob_cx,
                    y: track_y + track_h / 2.0,
                },
                radiusX: knob_r,
                radiusY: knob_r,
            },
            &knob_brush,
        );
        (track_x, track_y, track_w, track_h)
    }

    /// 渲染一个胶囊开关 + 右侧标签；返回命中区（覆盖开关与标签一段）
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn render_pill_switch(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        checked: bool,
        label: &str,
        label_format: &IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) -> (f32, f32, f32, f32) {
        let sw_w = 38.0_f32;
        let sw_h = 20.0_f32;
        let sw_bg = if checked {
            color_f(0.0, 0.47, 0.83, 1.0)
        } else {
            color_f(0.34, 0.34, 0.37, 1.0)
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &sw_bg) {
            let sw_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + sw_w,
                    bottom: y + sw_h,
                },
                radiusX: sw_h / 2.0,
                radiusY: sw_h / 2.0,
            };
            target.FillRoundedRectangle(&sw_rounded, &b);
        }
        let knob_r = 7.0_f32;
        let knob_cx = if checked {
            x + sw_w - knob_r - 3.0
        } else {
            x + knob_r + 3.0
        };
        if let Ok(kb) = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(1.0, 1.0, 1.0, 1.0))
        {
            target.FillEllipse(
                &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                    point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                        x: knob_cx,
                        y: y + sw_h / 2.0,
                    },
                    radiusX: knob_r,
                    radiusY: knob_r,
                },
                &kb,
            );
        }
        let lbl: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &lbl,
            label_format,
            &D2D_RECT_F {
                left: x + sw_w + 10.0,
                top: y,
                right: x + sw_w + 10.0 + 360.0,
                bottom: y + sw_h,
            },
            text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        (x, y, sw_w + 10.0 + 280.0, sw_h)
    }

    /// 渲染一个单行文本输入字段（标签 + 输入框 + 占位符），注册命中区；
    /// 返回输入框底部 Y（调用方自行加 gap）
    #[allow(clippy::too_many_arguments)]
    unsafe fn render_dev_text_input(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        margin: f32,
        input_w: f32,
        label_h: f32,
        input_h: f32,
        cy: f32,
        label: &str,
        value: &str,
        placeholder: &str,
        field: crate::settings::SettingsField,
        valid: bool,
        label_format: &IDWriteTextFormat,
        input_format: &IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) -> f32 {
        let lbl: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &lbl,
            label_format,
            &D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + label_h,
            },
            text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        let box_y = cy + label_h;
        let focused = self.ui.settings_panel.active_field == Some(field);
        let bg_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.18, 0.18, 0.18, 1.0))
            .unwrap();
        let border = if !valid {
            color_f(0.85, 0.30, 0.30, 1.0)
        } else if focused {
            color_f(0.0, 0.47, 0.83, 1.0)
        } else {
            color_f(0.3, 0.3, 0.3, 1.0)
        };
        let border_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &border)
            .unwrap();
        target.FillRectangle(
            &D2D_RECT_F {
                left: x + margin,
                top: box_y,
                right: x + margin + input_w,
                bottom: box_y + input_h,
            },
            &bg_brush,
        );
        draw_input_borders(target, x + margin, box_y, input_w, input_h, &border_brush);
        let (display, is_placeholder) = if value.is_empty() {
            (placeholder.to_string(), true)
        } else {
            (value.to_string(), false)
        };
        let text_color = if is_placeholder {
            color_f(0.5, 0.5, 0.5, 1.0)
        } else {
            color_f(0.85, 0.85, 0.85, 1.0)
        };
        let value_brush = self
            .win
            .render_ctx
            .brush_cache
            .get_brush(target, &text_color)
            .unwrap();
        let value_wide: Vec<u16> = display.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &value_wide,
            input_format,
            &D2D_RECT_F {
                left: x + margin + 6.0,
                top: box_y,
                right: x + margin + input_w - 6.0,
                bottom: box_y + input_h,
            },
            &value_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        self.ui
            .settings_panel
            .add_field_region(field, x + margin, box_y, input_w, input_h);
        box_y + input_h
    }

    /// 渲染 AI 接口设置字段（provider / key / url / model / 保存 / 测试连接）
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_ai_settings_fields(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
        margin: f32,
        label_h: f32,
        input_h: f32,
        gap: f32,
        label_format: IDWriteTextFormat,
        input_format: IDWriteTextFormat,
        button_format: IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
        avail_h: f32,
    ) {
        // 居中表单容器：重定义局部 x/width/input_w 指向居中列，
        // 使后续所有字段自动居中并占满表单宽度（不再固定 460 贴左）。
        let content_left = x;
        let content_width = width;
        let form_w = width.min(560.0);
        let x = x + ((width - form_w) / 2.0).max(0.0);
        let width = form_w;
        let input_w = form_w;
        let scroll = self.ui.settings_panel.scroll_offset;
        let mut cy = start_y - scroll;
        unsafe {
            // 裁剪到可视内容区：滚动后超出上下边界的内容不会绘制到标题栏/边界外
            let clip_rect = D2D_RECT_F {
                left: content_left,
                top: start_y,
                right: content_left + content_width,
                bottom: start_y + avail_h,
            };
            target.PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_ALIASED);

            // 信息卡片：左侧强调色条 + 说明文字（对比度提高，两行自适应）
            let card_h = 56.0_f32;
            let card_bg = color_f(0.16, 0.18, 0.22, 1.0);
            let card_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &card_bg)
                .unwrap();
            let card_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + card_h,
            };
            target.FillRectangle(&card_rect, &card_bg_brush);
            let accent = color_f(0.0, 0.47, 0.83, 1.0);
            let accent_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &accent)
                .unwrap();
            let accent_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + 3.0,
                bottom: cy + card_h,
            };
            target.FillRectangle(&accent_rect, &accent_brush);
            let info_text = if self.ui.settings_panel.is_adding_model() {
                "配置 API 密钥后，AI 助手可在 Agent 模式下新建、修改、删除文件。点击「保存」时会自动验证密钥有效性并保存；新建的模型只有点击「保存」后才会真正保存。"
            } else {
                "配置 API 密钥后，AI 助手可在 Agent 模式下新建、修改、删除文件。点击「保存」时会自动验证密钥有效性，并将更改写入当前模型。"
            };
            let info_color = color_f(0.72, 0.74, 0.78, 1.0);
            let info_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &info_color)
                .unwrap();
            let info_wide: Vec<u16> = info_text.encode_utf16().chain(Some(0)).collect();
            let info_rect = D2D_RECT_F {
                left: x + margin + 14.0,
                top: cy + 8.0,
                right: x + margin + input_w - 12.0,
                bottom: cy + card_h - 8.0,
            };
            target.DrawText(
                &info_wide,
                &label_format,
                &info_rect,
                &info_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += card_h + gap;

            // 当前编辑模型指示（添加模式显示草稿提示；编辑模式显示目标模型名）
            let model_hint = if self.ui.settings_panel.is_adding_model() {
                "正在添加：新模型".to_string()
            } else {
                format!(
                    "正在编辑：{}",
                    self.ui.settings_panel.active_model_display()
                )
            };
            let hint_wide: Vec<u16> = model_hint.encode_utf16().chain(Some(0)).collect();
            let hint_color = color_f(0.60, 0.78, 0.95, 1.0);
            let hint_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &hint_color)
                .unwrap();
            target.DrawText(
                &hint_wide,
                &label_format,
                &D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + label_h,
                },
                &hint_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h + 4.0;

            // 厂商下拉
            let provider_label_text = self.ui.settings_panel.provider_display_label();
            let provider_items: Vec<String> =
                crate::settings::SettingsPanel::provider_dropdown_options()
                    .into_iter()
                    .map(|(_, name)| name.to_string())
                    .collect();
            cy = self.render_settings_dropdown(
                target,
                x,
                cy,
                margin,
                input_w,
                label_h,
                input_h,
                gap,
                "厂商",
                &provider_label_text,
                true,
                crate::settings::SettingsDropdownKind::Provider,
                provider_items,
                &label_format,
                &input_format,
                text_brush,
            );

            // API 密钥（必填）——带显示/隐藏切换
            let apikey_label: Vec<u16> = "API 密钥 *".encode_utf16().chain(Some(0)).collect();
            let apikey_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &apikey_label,
                &label_format,
                &apikey_label_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h;
            let apikey_focused =
                self.ui.settings_panel.active_field == Some(crate::settings::SettingsField::ApiKey);
            let apikey_bg = color_f(0.18, 0.18, 0.18, 1.0);
            let apikey_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &apikey_bg)
                .unwrap();
            let apikey_border = if apikey_focused {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.3, 0.3, 0.3, 1.0)
            };
            let apikey_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &apikey_border)
                .unwrap();
            let apikey_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + input_h,
            };
            target.FillRectangle(&apikey_rect, &apikey_bg_brush);
            draw_input_borders(
                target,
                x + margin,
                cy,
                input_w,
                input_h,
                &apikey_border_brush,
            );
            // 显示/隐藏 按钮（右侧）：切换明文 / 掩码。用文字避免图标字体缺失显示为方块。
            let eye_w = 48.0_f32;
            let eye_x = x + margin + input_w - eye_w;
            let eye_color = if self.ui.settings_panel.hover_api_key_toggle {
                color_f(0.55, 0.78, 1.0, 1.0)
            } else {
                color_f(0.60, 0.60, 0.62, 1.0)
            };
            let eye_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &eye_color)
                .unwrap();
            let eye_glyph = if self.ui.settings_panel.show_api_key {
                "隐藏"
            } else {
                "显示"
            };
            let eye_wide: Vec<u16> = eye_glyph.encode_utf16().chain(Some(0)).collect();
            let eye_rect = D2D_RECT_F {
                left: eye_x,
                top: cy,
                right: eye_x + eye_w,
                bottom: cy + input_h,
            };
            target.DrawText(
                &eye_wide,
                &button_format,
                &eye_rect,
                &eye_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.api_key_toggle_region = Some((eye_x, cy, eye_w, input_h));
            // 密钥文本或占位符
            let key_empty = self.ui.settings_panel.api_key.is_empty();
            let display_key = if key_empty {
                "sk-...（粘贴你的密钥）".to_string()
            } else {
                self.ui.settings_panel.display_api_key()
            };
            let apikey_text: Vec<u16> = display_key.encode_utf16().chain(Some(0)).collect();
            let apikey_text_rect = D2D_RECT_F {
                left: x + margin + 8.0,
                top: cy,
                right: eye_x - 6.0,
                bottom: cy + input_h,
            };
            let key_text_color = if key_empty {
                color_f(0.5, 0.5, 0.5, 1.0)
            } else {
                color_f(0.9, 0.9, 0.9, 1.0)
            };
            let key_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &key_text_color)
                .unwrap();
            target.DrawText(
                &apikey_text,
                &input_format,
                &apikey_text_rect,
                &key_text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.add_field_region(
                crate::settings::SettingsField::ApiKey,
                x + margin,
                cy,
                input_w - eye_w,
                input_h,
            );
            cy += input_h + gap;

            // 判断是否为自定义模式（预制模式自动填充 base_url 和 model）
            let is_custom = self.ui.settings_panel.provider == "custom";

            // Base URL（仅自定义模式显示，预制模式自动填充）
            if is_custom {
                let baseurl_label: Vec<u16> = "基础地址".encode_utf16().chain(Some(0)).collect();
                let baseurl_label_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + width - margin,
                    bottom: cy + label_h,
                };
                target.DrawText(
                    &baseurl_label,
                    &label_format,
                    &baseurl_label_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += label_h;
                let baseurl_bg = color_f(0.18, 0.18, 0.18, 1.0);
                let baseurl_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &baseurl_bg)
                    .unwrap();
                let baseurl_border = if self.ui.settings_panel.active_field
                    == Some(crate::settings::SettingsField::BaseUrl)
                {
                    color_f(0.0, 0.47, 0.83, 1.0)
                } else {
                    color_f(0.3, 0.3, 0.3, 1.0)
                };
                let baseurl_border_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &baseurl_border)
                    .unwrap();
                let baseurl_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + input_h,
                };
                target.FillRectangle(&baseurl_rect, &baseurl_bg_brush);
                let border_top = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + 1.0,
                };
                let border_bottom = D2D_RECT_F {
                    left: x + margin,
                    top: cy + input_h - 1.0,
                    right: x + margin + input_w,
                    bottom: cy + input_h,
                };
                let border_left = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + 1.0,
                    bottom: cy + input_h,
                };
                let border_right = D2D_RECT_F {
                    left: x + margin + input_w - 1.0,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + input_h,
                };
                target.FillRectangle(&border_top, &baseurl_border_brush);
                target.FillRectangle(&border_bottom, &baseurl_border_brush);
                target.FillRectangle(&border_left, &baseurl_border_brush);
                target.FillRectangle(&border_right, &baseurl_border_brush);
                let baseurl_text: Vec<u16> = self
                    .ui
                    .settings_panel
                    .base_url
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let baseurl_text_rect = D2D_RECT_F {
                    left: x + margin + 6.0,
                    top: cy,
                    right: x + margin + input_w - 6.0,
                    bottom: cy + input_h,
                };
                target.DrawText(
                    &baseurl_text,
                    &input_format,
                    &baseurl_text_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                self.ui.settings_panel.add_field_region(
                    crate::settings::SettingsField::BaseUrl,
                    x + margin,
                    cy,
                    input_w,
                    input_h,
                );
                cy += input_h + gap;
            } // end if is_custom

            // Model 下拉：对所有厂商显示，选项优先来自 /models 实时拉取（失败回退预置清单）
            {
                let model_value = if self.ui.settings_panel.model.is_empty() {
                    "选择模型".to_string()
                } else {
                    self.ui.settings_panel.model.clone()
                };
                let model_items: Vec<String> = self
                    .ui
                    .settings_panel
                    .model_dropdown_options()
                    .into_iter()
                    .map(|(_id, name)| name)
                    .collect();
                // 标签体现自动获取状态：获取中 / 失败原因
                let model_label = if self.ui.settings_panel.is_fetching_models {
                    "模型（正在获取…）".to_string()
                } else if !self.ui.settings_panel.models_fetch_status.is_empty() {
                    format!("模型（{}）", self.ui.settings_panel.models_fetch_status)
                } else {
                    "模型".to_string()
                };
                cy = self.render_settings_dropdown(
                    target,
                    x,
                    cy,
                    margin,
                    input_w,
                    label_h,
                    input_h,
                    gap,
                    &model_label,
                    &model_value,
                    true,
                    crate::settings::SettingsDropdownKind::Model,
                    model_items,
                    &label_format,
                    &input_format,
                    text_brush,
                );
            }

            // 自定义名称：用户可自由编写，用于区分不同厂商的相同模型
            {
                let display_name_value = self.ui.settings_panel.display_name.clone();
                cy = self.render_dev_text_input(
                    target,
                    x,
                    margin,
                    input_w,
                    label_h,
                    input_h,
                    cy,
                    "自定义名称",
                    &display_name_value,
                    "用于区分不同厂商的相同模型（可选）",
                    crate::settings::SettingsField::DisplayName,
                    true,
                    &label_format,
                    &input_format,
                    text_brush,
                );
                cy += gap;
            }

            // 深度思考开关（DeepSeek 专属：thinking enabled/disabled）——胶囊开关 + 标签
            if self.ui.settings_panel.provider == "deepseek" {
                let sw_w = 38.0_f32;
                let sw_h = 20.0_f32;
                let sw_x = x + margin;
                let sw_y = cy;
                let checked = self.ui.settings_panel.thinking;
                let sw_bg = if checked {
                    color_f(0.0, 0.47, 0.83, 1.0)
                } else {
                    color_f(0.34, 0.34, 0.37, 1.0)
                };
                if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &sw_bg) {
                    let sw_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: sw_x,
                            top: sw_y,
                            right: sw_x + sw_w,
                            bottom: sw_y + sw_h,
                        },
                        radiusX: sw_h / 2.0,
                        radiusY: sw_h / 2.0,
                    };
                    target.FillRoundedRectangle(&sw_rounded, &b);
                }
                let knob_r = 7.0_f32;
                let knob_cx = if checked {
                    sw_x + sw_w - knob_r - 3.0
                } else {
                    sw_x + knob_r + 3.0
                };
                if let Ok(kb) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(1.0, 1.0, 1.0, 1.0))
                {
                    target.FillEllipse(
                        &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                            point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                                x: knob_cx,
                                y: sw_y + sw_h / 2.0,
                            },
                            radiusX: knob_r,
                            radiusY: knob_r,
                        },
                        &kb,
                    );
                }
                let lbl_text = if checked {
                    "深度思考  已开启（输出思维链，回答更准确）"
                } else {
                    "深度思考  已关闭（直接回答，响应更快）"
                };
                let lbl: Vec<u16> = lbl_text.encode_utf16().chain(Some(0)).collect();
                let lbl_rect = D2D_RECT_F {
                    left: sw_x + sw_w + 10.0,
                    top: sw_y,
                    right: x + width - margin,
                    bottom: sw_y + sw_h,
                };
                target.DrawText(
                    &lbl,
                    &label_format,
                    &lbl_rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                // 命中区覆盖开关 + 标签一段，便于点击切换
                self.ui.settings_panel.thinking_toggle_region =
                    Some((sw_x, sw_y, sw_w + 10.0 + 240.0, sw_h));
                cy += sw_h + gap;

                // 思考强度分段（仅思考模式下显示）：high（默认）/ max
                if checked {
                    let effort_label: Vec<u16> = "思考强度".encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &effort_label,
                        &label_format,
                        &D2D_RECT_F {
                            left: x + margin,
                            top: cy,
                            right: x + margin + 80.0,
                            bottom: cy + 24.0,
                        },
                        text_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    let seg_h = 24.0_f32;
                    let seg_w = 84.0_f32;
                    let seg_x0 = x + margin + 80.0;
                    let segments: [(&'static str, &str); 2] =
                        [("high", "高（默认）"), ("max", "最大")];
                    for (i, (val, disp)) in segments.iter().enumerate() {
                        let seg_x = seg_x0 + i as f32 * (seg_w + 8.0);
                        let selected = self.ui.settings_panel.reasoning_effort == *val;
                        let hovered = self.ui.settings_panel.hover_effort == Some(*val);
                        let seg_bg = if selected {
                            color_f(0.0, 0.47, 0.83, 1.0)
                        } else if hovered {
                            color_f(0.24, 0.24, 0.27, 1.0)
                        } else {
                            color_f(0.18, 0.18, 0.20, 1.0)
                        };
                        let seg_rect = D2D_RECT_F {
                            left: seg_x,
                            top: cy,
                            right: seg_x + seg_w,
                            bottom: cy + seg_h,
                        };
                        let seg_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                            rect: seg_rect,
                            radiusX: 4.0,
                            radiusY: 4.0,
                        };
                        if let Ok(sb) = self.win.render_ctx.brush_cache.get_brush(target, &seg_bg) {
                            target.FillRoundedRectangle(&seg_rounded, &sb);
                        }
                        if !selected {
                            if let Ok(bb) = self
                                .win
                                .render_ctx
                                .brush_cache
                                .get_brush(target, &color_f(0.32, 0.32, 0.35, 1.0))
                            {
                                target.DrawRoundedRectangle(&seg_rounded, &bb, 1.0, None);
                            }
                        }
                        let seg_text_color = if selected {
                            color_f(1.0, 1.0, 1.0, 1.0)
                        } else {
                            color_f(0.78, 0.78, 0.80, 1.0)
                        };
                        if let Ok(tb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &seg_text_color)
                        {
                            let seg_wide: Vec<u16> = disp.encode_utf16().chain(Some(0)).collect();
                            target.DrawText(
                                &seg_wide,
                                &button_format,
                                &seg_rect,
                                &tb,
                                D2D1_DRAW_TEXT_OPTIONS_NONE,
                                DWRITE_MEASURING_MODE_NATURAL,
                            );
                        }
                        self.ui
                            .settings_panel
                            .effort_regions
                            .push((val, seg_x, cy, seg_w, seg_h));
                    }
                    cy += seg_h + gap;
                }
            }

            // 采样参数禁用态：DeepSeek 思考模式下 temperature/top_p 不生效（官方文档）
            let sampling_disabled = self.ui.settings_panel.sampling_disabled_by_thinking();
            let disabled_text_color = color_f(0.50, 0.50, 0.53, 1.0);
            let disabled_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &disabled_text_color)
                .unwrap();

            // 温度：滑块（0.0 - 2.0，步进 0.1）——比裸文本框更直观，且天然合法
            let temp_val = self
                .ui
                .settings_panel
                .temperature
                .trim()
                .parse::<f32>()
                .unwrap_or(0.7)
                .clamp(0.0, 2.0);
            let temp_label_str = if sampling_disabled {
                format!("温度  {:.1}   （思考模式下不生效）", temp_val)
            } else {
                format!("温度  {:.1}   （越低越严谨，越高越发散）", temp_val)
            };
            let temp_label: Vec<u16> = temp_label_str.encode_utf16().chain(Some(0)).collect();
            let temp_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &temp_label,
                &label_format,
                &temp_label_rect,
                if sampling_disabled {
                    &disabled_text_brush
                } else {
                    text_brush
                },
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h + 6.0;
            let track_x = x + margin + 8.0;
            let track_w = (input_w - 16.0).max(1.0);
            let track_y = cy + 8.0;
            let temp_region = self.render_param_slider(
                target,
                track_x,
                track_y,
                track_w,
                temp_val / 2.0,
                sampling_disabled,
            );
            // 禁用态不注册命中区，点击/拖拽自然失效
            self.ui.settings_panel.temp_slider_region = if sampling_disabled {
                None
            } else {
                Some(temp_region)
            };
            cy += 24.0 + gap;

            // Top-p：核采样滑块（0.0 - 1.0，步进 0.05），与温度二选一调节为宜
            let top_p_val = self
                .ui
                .settings_panel
                .top_p
                .trim()
                .parse::<f32>()
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let top_p_label_str = if sampling_disabled {
                format!("Top-p  {:.2}   （思考模式下不生效）", top_p_val)
            } else {
                format!("Top-p  {:.2}   （核采样，建议与温度二选一调整）", top_p_val)
            };
            let top_p_label: Vec<u16> = top_p_label_str.encode_utf16().chain(Some(0)).collect();
            let top_p_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &top_p_label,
                &label_format,
                &top_p_label_rect,
                if sampling_disabled {
                    &disabled_text_brush
                } else {
                    text_brush
                },
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h + 6.0;
            let top_p_track_y = cy + 8.0;
            let top_p_region = self.render_param_slider(
                target,
                track_x,
                top_p_track_y,
                track_w,
                top_p_val,
                sampling_disabled,
            );
            self.ui.settings_panel.top_p_slider_region = if sampling_disabled {
                None
            } else {
                Some(top_p_region)
            };
            cy += 24.0 + gap;

            // 最大输入 Token（上下文预算，正整数）——限制发送给模型的历史上下文量
            let maxin_valid = self.ui.settings_panel.max_input_tokens_valid();
            let maxin_label_text = if self.ui.settings_panel.provider == "deepseek" {
                "最大输入 Token（上下文预算，DeepSeek V4 上下文上限 1M）"
            } else {
                "最大输入 Token（上下文预算）"
            };
            let maxin_label: Vec<u16> = maxin_label_text.encode_utf16().chain(Some(0)).collect();
            let maxin_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &maxin_label,
                &label_format,
                &maxin_label_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h;
            let maxin_focused = self.ui.settings_panel.active_field
                == Some(crate::settings::SettingsField::MaxInputTokens);
            let maxin_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.18, 0.18, 0.18, 1.0))
                .unwrap();
            let maxin_border = if !maxin_valid {
                color_f(0.85, 0.30, 0.30, 1.0)
            } else if maxin_focused {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.3, 0.3, 0.3, 1.0)
            };
            let maxin_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &maxin_border)
                .unwrap();
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + input_h,
                },
                &maxin_bg_brush,
            );
            draw_input_borders(
                target,
                x + margin,
                cy,
                input_w,
                input_h,
                &maxin_border_brush,
            );
            let maxin_empty = self.ui.settings_panel.max_input_tokens.is_empty();
            let maxin_display = if maxin_empty {
                "如 24000".to_string()
            } else {
                self.ui.settings_panel.max_input_tokens.clone()
            };
            let maxin_text: Vec<u16> = maxin_display.encode_utf16().chain(Some(0)).collect();
            let maxin_text_color = if maxin_empty {
                color_f(0.5, 0.5, 0.5, 1.0)
            } else {
                color_f(0.9, 0.9, 0.9, 1.0)
            };
            let maxin_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &maxin_text_color)
                .unwrap();
            target.DrawText(
                &maxin_text,
                &input_format,
                &D2D_RECT_F {
                    left: x + margin + 8.0,
                    top: cy,
                    right: x + margin + input_w - 8.0,
                    bottom: cy + input_h,
                },
                &maxin_text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.add_field_region(
                crate::settings::SettingsField::MaxInputTokens,
                x + margin,
                cy,
                input_w,
                input_h,
            );
            cy += input_h;
            if !maxin_valid {
                let warn_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.90, 0.45, 0.45, 1.0))
                    .unwrap();
                let warn_text: Vec<u16> = "请输入 1 到 2000000 之间的整数"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                target.DrawText(
                    &warn_text,
                    &label_format,
                    &D2D_RECT_F {
                        left: x + margin,
                        top: cy + 2.0,
                        right: x + margin + input_w,
                        bottom: cy + 18.0,
                    },
                    &warn_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += 18.0;
            }
            cy += gap;

            // 最大输出 Token（回复长度，正整数）——带合法性校验
            let maxtok_valid = self.ui.settings_panel.max_tokens_valid();
            let maxtok_label_text = if self.ui.settings_panel.provider == "deepseek" {
                "最大输出 Token（回复长度，DeepSeek V4 输出上限 384K）"
            } else {
                "最大输出 Token（回复长度）"
            };
            let maxtok_label: Vec<u16> = maxtok_label_text.encode_utf16().chain(Some(0)).collect();
            let maxtok_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &maxtok_label,
                &label_format,
                &maxtok_label_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h;
            let maxtok_focused = self.ui.settings_panel.active_field
                == Some(crate::settings::SettingsField::MaxTokens);
            let maxtok_bg = color_f(0.18, 0.18, 0.18, 1.0);
            let maxtok_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &maxtok_bg)
                .unwrap();
            let maxtok_border = if !maxtok_valid {
                color_f(0.85, 0.30, 0.30, 1.0)
            } else if maxtok_focused {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.3, 0.3, 0.3, 1.0)
            };
            let maxtok_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &maxtok_border)
                .unwrap();
            let maxtok_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + input_h,
            };
            target.FillRectangle(&maxtok_rect, &maxtok_bg_brush);
            draw_input_borders(
                target,
                x + margin,
                cy,
                input_w,
                input_h,
                &maxtok_border_brush,
            );
            let maxtok_empty = self.ui.settings_panel.max_tokens.is_empty();
            let maxtok_display = if maxtok_empty {
                "如 2048".to_string()
            } else {
                self.ui.settings_panel.max_tokens.clone()
            };
            let maxtok_text: Vec<u16> = maxtok_display.encode_utf16().chain(Some(0)).collect();
            let maxtok_text_rect = D2D_RECT_F {
                left: x + margin + 8.0,
                top: cy,
                right: x + margin + input_w - 8.0,
                bottom: cy + input_h,
            };
            let maxtok_text_color = if maxtok_empty {
                color_f(0.5, 0.5, 0.5, 1.0)
            } else {
                color_f(0.9, 0.9, 0.9, 1.0)
            };
            let maxtok_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &maxtok_text_color)
                .unwrap();
            target.DrawText(
                &maxtok_text,
                &input_format,
                &maxtok_text_rect,
                &maxtok_text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.add_field_region(
                crate::settings::SettingsField::MaxTokens,
                x + margin,
                cy,
                input_w,
                input_h,
            );
            cy += input_h;
            if !maxtok_valid {
                let warn_color = color_f(0.90, 0.45, 0.45, 1.0);
                let warn_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &warn_color)
                    .unwrap();
                let warn_text: Vec<u16> = "请输入 1 到 1000000 之间的整数"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let warn_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy + 2.0,
                    right: x + margin + input_w,
                    bottom: cy + 18.0,
                };
                target.DrawText(
                    &warn_text,
                    &label_format,
                    &warn_rect,
                    &warn_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += 18.0;
            }
            cy += gap;

            // System Prompt
            let sysp_label: Vec<u16> = "系统提示词（可选）".encode_utf16().chain(Some(0)).collect();
            let sysp_label_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + width - margin,
                bottom: cy + label_h,
            };
            target.DrawText(
                &sysp_label,
                &label_format,
                &sysp_label_rect,
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            cy += label_h;
            let sysp_bg = color_f(0.18, 0.18, 0.18, 1.0);
            let sysp_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &sysp_bg)
                .unwrap();
            let sysp_border = if self.ui.settings_panel.active_field
                == Some(crate::settings::SettingsField::SystemPrompt)
            {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.3, 0.3, 0.3, 1.0)
            };
            let sysp_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &sysp_border)
                .unwrap();
            let sysp_h = input_h * 2.0;
            let sysp_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + sysp_h,
            };
            target.FillRectangle(&sysp_rect, &sysp_bg_brush);
            draw_input_borders(target, x + margin, cy, input_w, sysp_h, &sysp_border_brush);
            let sysp_display: String = if self.ui.settings_panel.system_prompt.is_empty() {
                "（留空使用默认）".to_string()
            } else {
                self.ui.settings_panel.system_prompt.clone()
            };
            let sysp_text: Vec<u16> = sysp_display.encode_utf16().chain(Some(0)).collect();
            let sysp_text_rect = D2D_RECT_F {
                left: x + margin + 6.0,
                top: cy + 4.0,
                right: x + margin + input_w - 6.0,
                bottom: cy + sysp_h - 4.0,
            };
            let sysp_text_color = if self.ui.settings_panel.system_prompt.is_empty() {
                color_f(0.5, 0.5, 0.5, 1.0)
            } else {
                color_f(0.85, 0.85, 0.85, 1.0)
            };
            let sysp_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &sysp_text_color)
                .unwrap();
            target.DrawText(
                &sysp_text,
                &input_format,
                &sysp_text_rect,
                &sysp_text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.add_field_region(
                crate::settings::SettingsField::SystemPrompt,
                x + margin,
                cy,
                input_w,
                sysp_h,
            );
            cy += sysp_h + gap + 8.0;

            // ---- 开发者参数（可折叠）----
            let dev_sep_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.28, 0.28, 0.30, 1.0))
                .unwrap();
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + 1.0,
                },
                &dev_sep_brush,
            );
            cy += 10.0;
            let dev_expanded = self.ui.settings_panel.dev_params_expanded;
            let header_h = 24.0_f32;
            // Lucide 风格矢量 chevron：折叠时 '>'，展开时 'v'（10px，1.5 描边）
            let chev_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.62, 0.62, 0.65, 1.0))
                .unwrap();
            let chev_cx = x + margin + 5.0;
            let chev_cy = cy + header_h / 2.0;
            if dev_expanded {
                target.DrawLine(
                    D2D_POINT_2F {
                        x: chev_cx - 4.0,
                        y: chev_cy - 2.0,
                    },
                    D2D_POINT_2F {
                        x: chev_cx,
                        y: chev_cy + 2.0,
                    },
                    &chev_brush,
                    1.5,
                    None,
                );
                target.DrawLine(
                    D2D_POINT_2F {
                        x: chev_cx,
                        y: chev_cy + 2.0,
                    },
                    D2D_POINT_2F {
                        x: chev_cx + 4.0,
                        y: chev_cy - 2.0,
                    },
                    &chev_brush,
                    1.5,
                    None,
                );
            } else {
                target.DrawLine(
                    D2D_POINT_2F {
                        x: chev_cx - 2.0,
                        y: chev_cy - 4.0,
                    },
                    D2D_POINT_2F {
                        x: chev_cx + 2.0,
                        y: chev_cy,
                    },
                    &chev_brush,
                    1.5,
                    None,
                );
                target.DrawLine(
                    D2D_POINT_2F {
                        x: chev_cx + 2.0,
                        y: chev_cy,
                    },
                    D2D_POINT_2F {
                        x: chev_cx - 2.0,
                        y: chev_cy + 4.0,
                    },
                    &chev_brush,
                    1.5,
                    None,
                );
            }
            let dev_title: Vec<u16> = "开发者参数（低频调参，一般无需修改）"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let dev_title_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.70, 0.70, 0.73, 1.0))
                .unwrap();
            target.DrawText(
                &dev_title,
                &label_format,
                &D2D_RECT_F {
                    left: x + margin + 16.0,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + header_h,
                },
                &dev_title_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.dev_params_toggle_region =
                Some((x + margin, cy, input_w, header_h));
            cy += header_h + 8.0;

            if dev_expanded {
                // 频率惩罚滑块（-2.0 ~ 2.0，步进 0.1）
                let freq_val = self
                    .ui
                    .settings_panel
                    .frequency_penalty
                    .trim()
                    .parse::<f32>()
                    .unwrap_or(0.0)
                    .clamp(-2.0, 2.0);
                let freq_label_str = if sampling_disabled {
                    format!("频率惩罚  {:.1}   （思考模式下不生效）", freq_val)
                } else {
                    format!("频率惩罚  {:.1}   （-2 ~ 2，越大越抑制逐字重复）", freq_val)
                };
                let freq_label: Vec<u16> = freq_label_str.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &freq_label,
                    &label_format,
                    &D2D_RECT_F {
                        left: x + margin,
                        top: cy,
                        right: x + width - margin,
                        bottom: cy + label_h,
                    },
                    if sampling_disabled {
                        &disabled_text_brush
                    } else {
                        text_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += label_h + 6.0;
                let freq_region = self.render_param_slider(
                    target,
                    track_x,
                    cy + 8.0,
                    track_w,
                    (freq_val + 2.0) / 4.0,
                    sampling_disabled,
                );
                self.ui.settings_panel.freq_slider_region = if sampling_disabled {
                    None
                } else {
                    Some(freq_region)
                };
                cy += 24.0 + gap;

                // 存在惩罚滑块（-2.0 ~ 2.0，步进 0.1）
                let pres_val = self
                    .ui
                    .settings_panel
                    .presence_penalty
                    .trim()
                    .parse::<f32>()
                    .unwrap_or(0.0)
                    .clamp(-2.0, 2.0);
                let pres_label_str = if sampling_disabled {
                    format!("存在惩罚  {:.1}   （思考模式下不生效）", pres_val)
                } else {
                    format!("存在惩罚  {:.1}   （-2 ~ 2，越大越鼓励新话题）", pres_val)
                };
                let pres_label: Vec<u16> = pres_label_str.encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &pres_label,
                    &label_format,
                    &D2D_RECT_F {
                        left: x + margin,
                        top: cy,
                        right: x + width - margin,
                        bottom: cy + label_h,
                    },
                    if sampling_disabled {
                        &disabled_text_brush
                    } else {
                        text_brush
                    },
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += label_h + 6.0;
                let pres_region = self.render_param_slider(
                    target,
                    track_x,
                    cy + 8.0,
                    track_w,
                    (pres_val + 2.0) / 4.0,
                    sampling_disabled,
                );
                self.ui.settings_panel.pres_slider_region = if sampling_disabled {
                    None
                } else {
                    Some(pres_region)
                };
                cy += 24.0 + gap;

                // 停止序列（逗号分隔，最多 16 个）
                let stop_value = self.ui.settings_panel.stop.clone();
                let stop_bottom = self.render_dev_text_input(
                    target,
                    x,
                    margin,
                    input_w,
                    label_h,
                    input_h,
                    cy,
                    "停止序列（逗号分隔，最多 16 个，可选）",
                    &stop_value,
                    "（无）",
                    crate::settings::SettingsField::Stop,
                    true,
                    &label_format,
                    &input_format,
                    text_brush,
                );
                cy = stop_bottom + gap;

                // 响应格式分段：文本 / JSON
                let fmt_label: Vec<u16> = "响应格式".encode_utf16().chain(Some(0)).collect();
                target.DrawText(
                    &fmt_label,
                    &label_format,
                    &D2D_RECT_F {
                        left: x + margin,
                        top: cy,
                        right: x + margin + 80.0,
                        bottom: cy + 24.0,
                    },
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                let fmt_seg_h = 24.0_f32;
                let fmt_seg_w = 84.0_f32;
                let fmt_seg_x0 = x + margin + 80.0;
                let fmt_segments: [(&'static str, &str); 2] =
                    [("text", "文本（默认）"), ("json_object", "JSON")];
                for (i, (val, disp)) in fmt_segments.iter().enumerate() {
                    let seg_x = fmt_seg_x0 + i as f32 * (fmt_seg_w + 8.0);
                    let selected = self.ui.settings_panel.response_format == *val;
                    let hovered = self.ui.settings_panel.hover_response_format == Some(*val);
                    let seg_bg = if selected {
                        color_f(0.0, 0.47, 0.83, 1.0)
                    } else if hovered {
                        color_f(0.24, 0.24, 0.27, 1.0)
                    } else {
                        color_f(0.18, 0.18, 0.20, 1.0)
                    };
                    let seg_rect = D2D_RECT_F {
                        left: seg_x,
                        top: cy,
                        right: seg_x + fmt_seg_w,
                        bottom: cy + fmt_seg_h,
                    };
                    let seg_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: seg_rect,
                        radiusX: 4.0,
                        radiusY: 4.0,
                    };
                    if let Ok(sb) = self.win.render_ctx.brush_cache.get_brush(target, &seg_bg) {
                        target.FillRoundedRectangle(&seg_rounded, &sb);
                    }
                    if !selected {
                        if let Ok(bb) = self
                            .win
                            .render_ctx
                            .brush_cache
                            .get_brush(target, &color_f(0.32, 0.32, 0.35, 1.0))
                        {
                            target.DrawRoundedRectangle(&seg_rounded, &bb, 1.0, None);
                        }
                    }
                    let seg_text_color = if selected {
                        color_f(1.0, 1.0, 1.0, 1.0)
                    } else {
                        color_f(0.78, 0.78, 0.80, 1.0)
                    };
                    if let Ok(tb) = self
                        .win
                        .render_ctx
                        .brush_cache
                        .get_brush(target, &seg_text_color)
                    {
                        let seg_wide: Vec<u16> = disp.encode_utf16().chain(Some(0)).collect();
                        target.DrawText(
                            &seg_wide,
                            &button_format,
                            &seg_rect,
                            &tb,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                    self.ui
                        .settings_panel
                        .response_format_regions
                        .push((val, seg_x, cy, fmt_seg_w, fmt_seg_h));
                }
                cy += fmt_seg_h + gap;

                // 用户标识 user_id
                let user_id_value = self.ui.settings_panel.user_id.clone();
                let uid_bottom = self.render_dev_text_input(
                    target,
                    x,
                    margin,
                    input_w,
                    label_h,
                    input_h,
                    cy,
                    "用户标识 user_id（内容安全/缓存隔离，可选）",
                    &user_id_value,
                    "（不下发）",
                    crate::settings::SettingsField::UserId,
                    true,
                    &label_format,
                    &input_format,
                    text_brush,
                );
                cy = uid_bottom + gap;
            }
            cy += 4.0;

            // 未保存更改提示
            if self.ui.settings_panel.is_dirty() {
                let dot_color = color_f(0.95, 0.65, 0.20, 1.0);
                let dot_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &dot_color)
                    .unwrap();
                target.FillEllipse(
                    &windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                        point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                            x: x + margin + 4.0,
                            y: cy + 9.0,
                        },
                        radiusX: 4.0,
                        radiusY: 4.0,
                    },
                    &dot_brush,
                );
                let dirty_text: Vec<u16> = "有未保存的更改，点击「保存」生效"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let dirty_rect = D2D_RECT_F {
                    left: x + margin + 16.0,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + 18.0,
                };
                target.DrawText(
                    &dirty_text,
                    &label_format,
                    &dirty_rect,
                    &dot_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += 24.0;
            }

            // 操作按钮：左「保存」（验证密钥后写入）+ 右「测试连接」（只测不存，便于调参）
            let btn_h = 34.0_f32;
            let is_testing = self.ui.settings_panel.is_testing;
            let btn_gap = 10.0_f32;
            let save_x = x + margin;
            let save_btn_w = (input_w - btn_gap) * 0.62;
            let test_btn_w = input_w - btn_gap - save_btn_w;
            let test_x = save_x + save_btn_w + btn_gap;

            // 保存设置（主按钮；保存时会自动先测试密钥有效性）
            let save_hover =
                self.ui.settings_panel.hover_button == Some(crate::settings::SettingsButton::Save);
            let save_bg = if is_testing {
                color_f(0.0, 0.30, 0.52, 1.0)
            } else if save_hover {
                color_f(0.0, 0.55, 0.95, 1.0)
            } else {
                color_f(0.0, 0.47, 0.83, 1.0)
            };
            let save_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &save_bg)
                .unwrap();
            let save_rect = D2D_RECT_F {
                left: save_x,
                top: cy,
                right: save_x + save_btn_w,
                bottom: cy + btn_h,
            };
            let save_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: save_rect,
                radiusX: 4.0,
                radiusY: 4.0,
            };
            target.FillRoundedRectangle(&save_rounded, &save_bg_brush);
            let save_label = if is_testing && self.ui.settings_panel.pending_save {
                "验证并保存中…"
            } else {
                "保存"
            };
            let save_text: Vec<u16> = save_label.encode_utf16().chain(Some(0)).collect();
            let btn_text_color = color_f(1.0, 1.0, 1.0, 1.0);
            let btn_text_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_text_color)
                .unwrap();
            target.DrawText(
                &save_text,
                &button_format,
                &save_rect,
                &btn_text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.ui.settings_panel.add_button_region(
                crate::settings::SettingsButton::Save,
                save_x,
                cy,
                save_btn_w,
                btn_h,
            );

            // 测试连接（次要描边按钮：只验证当前参数能否连通，不写入配置）
            let test_hover = self.ui.settings_panel.hover_button
                == Some(crate::settings::SettingsButton::TestConnection);
            let test_rect = D2D_RECT_F {
                left: test_x,
                top: cy,
                right: test_x + test_btn_w,
                bottom: cy + btn_h,
            };
            let test_rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: test_rect,
                radiusX: 4.0,
                radiusY: 4.0,
            };
            let test_bg = if (is_testing && !self.ui.settings_panel.pending_save) || test_hover {
                color_f(0.20, 0.24, 0.30, 1.0)
            } else {
                color_f(0.16, 0.16, 0.18, 1.0)
            };
            if let Ok(tb) = self.win.render_ctx.brush_cache.get_brush(target, &test_bg) {
                target.FillRoundedRectangle(&test_rounded, &tb);
            }
            let test_border = if test_hover {
                color_f(0.28, 0.56, 0.86, 1.0)
            } else {
                color_f(0.34, 0.34, 0.37, 1.0)
            };
            if let Ok(bb) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &test_border)
            {
                target.DrawRoundedRectangle(&test_rounded, &bb, 1.0, None);
            }
            let test_label = if is_testing && !self.ui.settings_panel.pending_save {
                "测试中…"
            } else {
                "测试连接"
            };
            let test_text: Vec<u16> = test_label.encode_utf16().chain(Some(0)).collect();
            let test_text_color = if test_hover {
                color_f(0.82, 0.90, 1.0, 1.0)
            } else {
                color_f(0.84, 0.86, 0.90, 1.0)
            };
            if let Ok(ttb) = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &test_text_color)
            {
                target.DrawText(
                    &test_text,
                    &button_format,
                    &test_rect,
                    &ttb,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            self.ui.settings_panel.add_button_region(
                crate::settings::SettingsButton::TestConnection,
                test_x,
                cy,
                test_btn_w,
                btn_h,
            );
            cy += btn_h + 12.0;

            // 状态消息卡片
            if !self.ui.settings_panel.test_status.is_empty() {
                let (status_bg, status_fg) = if self.ui.settings_panel.is_testing {
                    (
                        color_f(0.20, 0.20, 0.12, 1.0),
                        color_f(0.90, 0.85, 0.40, 1.0),
                    )
                } else if self.ui.settings_panel.test_status.starts_with('✓') {
                    (
                        color_f(0.12, 0.22, 0.14, 1.0),
                        color_f(0.40, 0.85, 0.45, 1.0),
                    )
                } else {
                    (
                        color_f(0.24, 0.14, 0.14, 1.0),
                        color_f(0.95, 0.50, 0.50, 1.0),
                    )
                };
                let status_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &status_bg)
                    .unwrap();
                let status_h = 34.0_f32;
                let status_rect = D2D_RECT_F {
                    left: x + margin,
                    top: cy,
                    right: x + margin + input_w,
                    bottom: cy + status_h,
                };
                target.FillRectangle(&status_rect, &status_bg_brush);
                let status_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &status_fg)
                    .unwrap();
                let status_format = self
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
                let status_text: Vec<u16> = self
                    .ui
                    .settings_panel
                    .test_status
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let status_text_rect = D2D_RECT_F {
                    left: x + margin + 10.0,
                    top: cy,
                    right: x + margin + input_w - 10.0,
                    bottom: cy + status_h,
                };
                target.DrawText(
                    &status_text,
                    &status_format,
                    &status_text_rect,
                    &status_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                cy += status_h;
            }
            cy += 8.0;

            // 结束裁剪，计算可滚动高度并绘制滚动条
            target.PopAxisAlignedClip();
            let total_content = (cy + scroll) - start_y;
            let max_scroll = (total_content - avail_h).max(0.0);
            self.ui.settings_panel.content_height = max_scroll;
            if self.ui.settings_panel.scroll_offset > max_scroll {
                self.ui.settings_panel.scroll_offset = max_scroll;
            }
            if max_scroll > 0.0 && total_content > 0.0 {
                let sb_w = 6.0_f32;
                let sb_x = content_left + content_width - sb_w - 2.0;
                let visible_ratio = (avail_h / total_content).clamp(0.1, 1.0);
                let thumb_h = (avail_h * visible_ratio).max(30.0);
                let scroll_ratio =
                    (self.ui.settings_panel.scroll_offset / max_scroll).clamp(0.0, 1.0);
                let thumb_y = start_y + (avail_h - thumb_h) * scroll_ratio;
                let thumb_color = color_f(0.4, 0.4, 0.45, 1.0);
                let thumb_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &thumb_color)
                    .unwrap();
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: sb_x,
                        top: thumb_y,
                        right: sb_x + sb_w,
                        bottom: thumb_y + thumb_h,
                    },
                    &thumb_brush,
                );
            }
        }
    }

    /// 渲染设置面板主编辑区的下拉字段（厂商 / 模型）
    /// 下拉项列表由调用方传入（settings_panel 上有多个下拉，items 集合各异）。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_settings_dropdown(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        cy: f32,
        margin: f32,
        input_w: f32,
        label_h: f32,
        input_h: f32,
        gap: f32,
        label: &str,
        value: &str,
        required: bool,
        kind: crate::settings::SettingsDropdownKind,
        items: Vec<String>,
        label_format: &IDWriteTextFormat,
        input_format: &IDWriteTextFormat,
        text_brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    ) -> f32 {
        unsafe {
            let is_open = self.ui.settings_panel.open_dropdown == Some(kind);
            // 标签
            let label_color = if required {
                color_f(0.92, 0.30, 0.30, 1.0)
            } else {
                color_f(0.85, 0.85, 0.85, 1.0)
            };
            let label_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &label_color)
                .unwrap();
            let prefix: Vec<u16> = "*".encode_utf16().chain(Some(0)).collect();
            let label_text: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
            let label_y = cy;
            if required {
                target.DrawText(
                    &prefix,
                    label_format,
                    &D2D_RECT_F {
                        left: x + margin,
                        top: label_y,
                        right: x + margin + 10.0,
                        bottom: label_y + label_h,
                    },
                    &label_brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            target.DrawText(
                &label_text,
                label_format,
                &D2D_RECT_F {
                    left: x + margin + (if required { 12.0 } else { 0.0 }),
                    top: label_y,
                    right: x + margin + input_w,
                    bottom: label_y + label_h,
                },
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let cy = cy + label_h + 4.0;

            // 下拉框背景
            let input_bg = color_f(0.18, 0.18, 0.18, 1.0);
            let input_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &input_bg)
                .unwrap();
            let input_border = if is_open {
                color_f(0.0, 0.47, 0.83, 1.0)
            } else {
                color_f(0.3, 0.3, 0.3, 1.0)
            };
            let input_border_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &input_border)
                .unwrap();
            let input_rect = D2D_RECT_F {
                left: x + margin,
                top: cy,
                right: x + margin + input_w,
                bottom: cy + input_h,
            };
            target.FillRectangle(&input_rect, &input_bg_brush);
            for (b_left, b_top, b_right, b_bottom) in [
                (x + margin, cy, x + margin + input_w, cy + 1.0),
                (
                    x + margin,
                    cy + input_h - 1.0,
                    x + margin + input_w,
                    cy + input_h,
                ),
                (x + margin, cy, x + margin + 1.0, cy + input_h),
                (
                    x + margin + input_w - 1.0,
                    cy,
                    x + margin + input_w,
                    cy + input_h,
                ),
            ] {
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: b_left,
                        top: b_top,
                        right: b_right,
                        bottom: b_bottom,
                    },
                    &input_border_brush,
                );
            }
            // 文本
            let value_color = if value.is_empty() || value.starts_with("选择") {
                color_f(0.55, 0.55, 0.55, 1.0)
            } else {
                color_f(0.95, 0.95, 0.95, 1.0)
            };
            let value_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &value_color)
                .unwrap();
            let value_wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &value_wide,
                input_format,
                &D2D_RECT_F {
                    left: x + margin + 10.0,
                    top: cy,
                    right: x + margin + input_w - 28.0,
                    bottom: cy + input_h,
                },
                &value_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            // 箭头
            let arrow = if is_open { "▴" } else { "▾" };
            let arrow_wide: Vec<u16> = arrow.encode_utf16().chain(Some(0)).collect();
            target.DrawText(
                &arrow_wide,
                input_format,
                &D2D_RECT_F {
                    left: x + margin + input_w - 24.0,
                    top: cy,
                    right: x + margin + input_w - 6.0,
                    bottom: cy + input_h,
                },
                text_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 保存触发区域
            self.ui.settings_panel.dropdown_trigger_regions.push((
                kind,
                x + margin,
                cy,
                input_w,
                input_h,
            ));

            let mut next_cy = cy + input_h + gap;

            // 如果展开，渲染下拉项
            if is_open {
                let item_h = 28.0f32;
                let item_bg = color_f(0.22, 0.22, 0.24, 1.0);
                let item_bg_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &item_bg)
                    .unwrap();
                let selected_color = color_f(0.14, 0.30, 0.45, 1.0);
                let selected_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &selected_color)
                    .unwrap();
                // 当前已选项的强调色（左侧竖条），与主题强调蓝一致
                let accent_color = color_f(0.0, 0.47, 0.83, 1.0);
                let accent_brush = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &accent_color)
                    .unwrap();
                for (i, item_label) in items.iter().enumerate() {
                    let iy = next_cy + i as f32 * item_h;
                    let is_hover = self.ui.settings_panel.hover_dropdown == Some(kind)
                        && self.ui.settings_panel.hover_dropdown_index == Some(i);
                    let is_selected = match kind {
                        crate::settings::SettingsDropdownKind::Provider => {
                            // dropdown_items() 顺序：DeepSeek, Kimi, 自定义
                            matches!(
                                (self.ui.settings_panel.current_provider_button(), i),
                                (Some(ProviderTemplateButton::DeepSeek), 0)
                                    | (Some(ProviderTemplateButton::Kimi), 1)
                                    | (Some(ProviderTemplateButton::Custom), 2)
                            )
                        }
                        crate::settings::SettingsDropdownKind::Model => {
                            self.ui.settings_panel.model == *item_label
                        }
                    };
                    // hover 与当前选中项统一显示预选中效果（蓝色高亮底 + 左侧强调竖条）
                    let highlighted = is_hover || is_selected;
                    let brush = if highlighted {
                        &selected_brush
                    } else {
                        &item_bg_brush
                    };
                    let item_rect = D2D_RECT_F {
                        left: x + margin,
                        top: iy,
                        right: x + margin + input_w,
                        bottom: iy + item_h,
                    };
                    target.FillRectangle(&item_rect, brush);
                    // 预选中标识：hover 或当前选中项都显示左侧强调竖条
                    if highlighted {
                        target.FillRectangle(
                            &D2D_RECT_F {
                                left: x + margin,
                                top: iy,
                                right: x + margin + 3.0,
                                bottom: iy + item_h,
                            },
                            &accent_brush,
                        );
                    }
                    let item_wide: Vec<u16> = item_label.encode_utf16().chain(Some(0)).collect();
                    target.DrawText(
                        &item_wide,
                        input_format,
                        &D2D_RECT_F {
                            left: x + margin + 14.0,
                            top: iy,
                            right: x + margin + input_w - 14.0,
                            bottom: iy + item_h,
                        },
                        text_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                    self.ui.settings_panel.dropdown_item_regions.push((
                        kind,
                        i,
                        x + margin,
                        iy,
                        input_w,
                        item_h,
                    ));
                }
                next_cy += items.len() as f32 * item_h + gap;
            }
            next_cy
        }
    }
}
