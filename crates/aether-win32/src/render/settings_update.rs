use super::*;

impl EditorState {
    /// 渲染"更新"标签页内容（版本信息 + 策略选择 + 检查按钮）
    pub(super) fn render_update_settings(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        width: f32,
        start_y: f32,
    ) {
        unsafe {
            let mut cy = start_y;

            // 分组「版本信息」
            let last_check = self.ui.app_settings.update.last_check_ts;
            let last_check_text = if last_check == 0 {
                "从未".to_string()
            } else {
                // 简单格式化时间戳
                format!("{} 秒前", {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    now.saturating_sub(last_check)
                })
            };
            let mut version_rows: Vec<(&str, String, Option<bool>)> = vec![
                ("当前版本", crate::updater::APP_VERSION.to_string(), None),
                ("上次检查", last_check_text, None),
            ];
            if let Some(ref ver) = self.ui.update_available_version {
                version_rows.push(("发现新版本", ver.clone(), Some(true)));
            }
            cy = self.draw_settings_group(target, "版本信息", x, width, cy, &version_rows);
            cy += 20.0;

            // 分组「更新策略」
            let policy = &self.ui.app_settings.update.policy;
            let policy_label = match policy {
                aether_shared::settings::UpdatePolicy::AutoInstall => "自动下载并安装（推荐）",
                aether_shared::settings::UpdatePolicy::NotifyOnly => "仅通知，手动下载",
                aether_shared::settings::UpdatePolicy::Disabled => "关闭自动更新",
            };
            let suppress = self.ui.app_settings.update.suppress_days;
            let suppress_label = match suppress {
                0 => "每次启动检查".to_string(),
                1 => "1 天内不再提醒".to_string(),
                7 => "7 天内不再提醒".to_string(),
                30 => "30 天内不再提醒".to_string(),
                n => format!("{} 天内不再提醒", n),
            };
            let policy_rows = [
                ("更新策略", policy_label.to_string(), None),
                ("检查频率", suppress_label, None),
            ];
            cy = self.draw_settings_group(target, "更新策略", x, width, cy, &policy_rows);
            cy += 20.0;

            // "立即检查更新"按钮 / 检查中状态
            let is_checking = self.ui.update_checking;
            let btn_text = if is_checking {
                "正在检查..."
            } else {
                "立即检查更新"
            };
            let btn_wide: Vec<u16> = btn_text.encode_utf16().chain(Some(0)).collect();
            let btn_w = 132.0f32;
            let btn_h = 30.0f32;
            let btn_rect = D2D_RECT_F {
                left: x,
                top: cy,
                right: x + btn_w,
                bottom: cy + btn_h,
            };
            let btn_bg = if is_checking {
                color_f(0.35, 0.35, 0.35, 1.0)
            } else {
                color_f(0.0, 0.47, 0.83, 1.0)
            };
            let btn_bg_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &btn_bg)
                .unwrap();
            let white_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(1.0, 1.0, 1.0, 1.0))
                .unwrap();
            let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: btn_rect,
                radiusX: 4.0,
                radiusY: 4.0,
            };
            target.FillRoundedRectangle(&rounded, &btn_bg_brush);
            let btn_format = self
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
            target.DrawText(
                &btn_wide,
                &btn_format,
                &btn_rect,
                &white_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            // 注册按钮命中区域供点击检测
            self.ui.settings_panel.button_regions.push((
                crate::settings::SettingsButton::CheckUpdate,
                btn_rect.left,
                btn_rect.top,
                btn_w,
                btn_h,
            ));
        }
    }
}
