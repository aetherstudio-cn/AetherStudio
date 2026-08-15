//! 智能体沙盒评测页渲染（编辑器内容区整页，参考设置页模式）。
//!
//! 四个阶段各自的 UI：
//! - Setup：主题输入、任务数量、模式（定时/不定时）、时长选择、开始按钮、沙盒规则说明
//! - Planning：任务规划进度与实时输出预览
//! - Running：倒计时（定时模式）、任务列表进度、实时输出、执行日志、终止按钮
//! - Scoring：逐任务打分（1-10）、平均分、打包导出、再来一轮

use super::*;
use crate::sandbox_eval::{
    SandboxField, SandboxLogKind, SandboxMode, SandboxPhase, SandboxTaskStatus, AGENT_PRESETS,
    DURATION_PRESETS,
};

/// 页面内容最大宽度
const SB_CONTENT_MAX_W: f32 = 760.0;
/// 卡片内边距
const SB_PAD: f32 = 16.0;

// ===== 配色（与设置页风格一致的深色体系） =====
fn c_card_bg() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.155, 0.155, 0.17, 1.0)
}
fn c_card_border() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.26, 0.26, 0.28, 1.0)
}
fn c_text() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.92, 0.92, 0.92, 1.0)
}
fn c_text_dim() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.62, 0.62, 0.65, 1.0)
}
fn c_accent() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.25, 0.55, 0.95, 1.0)
}
fn c_accent_bg() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.25, 0.55, 0.95, 0.18)
}
fn c_green() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.30, 0.80, 0.48, 1.0)
}
fn c_orange() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.95, 0.65, 0.25, 1.0)
}
fn c_red() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.90, 0.35, 0.35, 1.0)
}
fn c_field_bg() -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    color_f(0.11, 0.11, 0.125, 1.0)
}

impl EditorState {
    /// 沙盒评测页入口：在编辑器内容区渲染
    pub(super) unsafe fn render_sandbox_eval_page(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        // 背景
        let bg = self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.10, 0.10, 0.11, 1.0));
        if let Ok(bg) = bg {
            target.FillRectangle(
                &D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                &bg,
            );
        }
        // 裁剪到内容区，滚动内容不越界
        target.PushAxisAlignedClip(
            &D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            D2D1_ANTIALIAS_MODE_ALIASED,
        );

        self.ui.sandbox_eval.regions.clear();
        self.ui.sandbox_eval.view_height = h;

        let content_w = w.min(SB_CONTENT_MAX_W + 48.0) - 48.0;
        let cx = x + (w - content_w) / 2.0;
        let mut cy = y + 24.0 - self.ui.sandbox_eval.scroll_y;

        // ===== 页头 =====
        self.sb_text(
            target,
            "智能体沙盒评测",
            cx,
            cy,
            content_w,
            30.0,
            20.0,
            true,
            c_text(),
        );
        cy += 32.0;
        self.sb_text(
            target,
            "智能体在专属临时沙盒中完成你设定的任务：只允许沙盒内文件操作与联网搜索，任何沙盒外命令都会被拦截。",
            cx,
            cy,
            content_w,
            20.0,
            12.0,
            false,
            c_text_dim(),
        );
        cy += 26.0;
        // 沙盒路径（运行后显示）
        if let Some(dir) = self.ui.sandbox_eval.sandbox_dir.clone() {
            let phase_label = match self.ui.sandbox_eval.phase {
                SandboxPhase::Setup => "未开始",
                SandboxPhase::Planning => "任务规划中",
                SandboxPhase::Running => "执行中",
                SandboxPhase::Scoring => "待打分 / 已结束",
            };
            self.sb_text(
                target,
                &format!("状态：{}    沙盒目录：{}", phase_label, dir.display()),
                cx,
                cy,
                content_w,
                18.0,
                11.0,
                false,
                c_text_dim(),
            );
            cy += 24.0;
        }
        // 评测目标模型（有指定时显示）
        if let Some(model_name) = self.ui.sandbox_eval.target_model_name.clone() {
            self.sb_text(
                target,
                &format!("评测模型：{}", model_name),
                cx,
                cy,
                content_w,
                18.0,
                12.0,
                true,
                c_accent(),
            );
            cy += 22.0;
        }
        cy += 4.0;

        match self.ui.sandbox_eval.phase {
            SandboxPhase::Setup => {
                cy = self.sb_render_setup(target, cx, cy, content_w);
            }
            SandboxPhase::Planning => {
                cy = self.sb_render_planning(target, cx, cy, content_w);
                cy = self.sb_render_log(target, cx, cy, content_w);
            }
            SandboxPhase::Running => {
                cy = self.sb_render_running(target, cx, cy, content_w);
                cy = self.sb_render_log(target, cx, cy, content_w);
            }
            SandboxPhase::Scoring => {
                cy = self.sb_render_scoring(target, cx, cy, content_w);
                cy = self.sb_render_log(target, cx, cy, content_w);
            }
        }

        // 内容总高度（用于滚轮钳制）
        self.ui.sandbox_eval.content_height = (cy + self.ui.sandbox_eval.scroll_y - y) + 24.0;

        target.PopAxisAlignedClip();
    }

    // =====================================================================
    // Setup 阶段
    // =====================================================================

    unsafe fn sb_render_setup(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        cx: f32,
        mut cy: f32,
        content_w: f32,
    ) -> f32 {
        // ---- 配置卡片 ----
        let is_timed = self.ui.sandbox_eval.mode == SandboxMode::Timed;
        let card_h = if is_timed { 316.0 } else { 262.0 };
        self.sb_card(target, cx, cy, content_w, card_h);
        let ix = cx + SB_PAD;
        let iw = content_w - SB_PAD * 2.0;
        let mut iy = cy + SB_PAD;

        self.sb_text(target, "评测配置", ix, iy, iw, 20.0, 14.0, true, c_text());
        iy += 30.0;

        // 主题输入
        self.sb_text(
            target,
            "评测主题",
            ix,
            iy,
            200.0,
            18.0,
            12.0,
            false,
            c_text_dim(),
        );
        iy += 22.0;
        let topic_active = self.ui.sandbox_eval.active_field == Some(SandboxField::Topic);
        let topic = self.ui.sandbox_eval.topic.clone();
        let caret = topic_active && self.ui.sandbox_eval.caret_visible;
        self.sb_input_field(
            target,
            ix,
            iy,
            iw,
            34.0,
            &topic,
            "例如：制作一份 Rust 异步编程学习资料",
            topic_active,
            caret,
        );
        self.ui.sandbox_eval.regions.topic_field = Some((ix, iy, iw, 34.0));
        iy += 46.0;

        // 并发智能体数量
        self.sb_text(
            target,
            "并发智能体数量",
            ix,
            iy,
            200.0,
            18.0,
            12.0,
            false,
            c_text_dim(),
        );
        iy += 22.0;
        let mut chip_x = ix;
        let custom_active = self.ui.sandbox_eval.active_field == Some(SandboxField::CustomCount);
        let custom_set = !self.ui.sandbox_eval.custom_count.trim().is_empty();
        for n in AGENT_PRESETS {
            let label = format!("{} 个", n);
            let selected = !custom_set && self.ui.sandbox_eval.agent_count == n;
            let cw = 56.0;
            self.sb_chip(target, chip_x, iy, cw, 26.0, &label, selected);
            self.ui.sandbox_eval
                .regions
                .agent_chips
                .push((n, (chip_x, iy, cw, 26.0)));
            chip_x += cw + 8.0;
        }
        // 自定义数量输入
        let custom = self.ui.sandbox_eval.custom_count.clone();
        let caret2 = custom_active && self.ui.sandbox_eval.caret_visible;
        self.sb_input_field(
            target,
            chip_x,
            iy,
            96.0,
            26.0,
            &custom,
            "自定义 1-20",
            custom_active || custom_set,
            caret2,
        );
        self.ui.sandbox_eval.regions.custom_count_field = Some((chip_x, iy, 96.0, 26.0));
        iy += 40.0;

        // 任务模式
        self.sb_text(
            target,
            "任务模式",
            ix,
            iy,
            200.0,
            18.0,
            12.0,
            false,
            c_text_dim(),
        );
        iy += 22.0;
        let mut mx = ix;
        for (i, mode) in [SandboxMode::Untimed, SandboxMode::Timed]
            .iter()
            .enumerate()
        {
            let selected = self.ui.sandbox_eval.mode == *mode;
            let cw = 96.0;
            self.sb_chip(target, mx, iy, cw, 26.0, mode.label(), selected);
            self.ui.sandbox_eval
                .regions
                .mode_chips
                .push((i, (mx, iy, cw, 26.0)));
            mx += cw + 8.0;
        }
        if is_timed {
            iy += 40.0;
            self.sb_text(
                target,
                "时间限制（剩余 5 分钟时将提醒智能体抓紧收尾）",
                ix,
                iy,
                iw,
                18.0,
                12.0,
                false,
                c_text_dim(),
            );
            iy += 22.0;
            let mut dx = ix;
            for mins in DURATION_PRESETS {
                let label = format!("{} 分钟", mins);
                let selected = self.ui.sandbox_eval.duration_min == mins;
                let cw = 72.0;
                self.sb_chip(target, dx, iy, cw, 26.0, &label, selected);
                self.ui.sandbox_eval
                    .regions
                    .duration_chips
                    .push((mins, (dx, iy, cw, 26.0)));
                dx += cw + 8.0;
            }
        }
        iy += 40.0;

        // 错误提示 + 开始按钮
        if let Some(err) = self.ui.sandbox_eval.error.clone() {
            self.sb_text(
                target,
                &err,
                ix,
                iy + 4.0,
                iw - 140.0,
                18.0,
                12.0,
                false,
                c_red(),
            );
        }
        let btn_w = 120.0;
        let btn_x = ix + iw - btn_w;
        self.sb_button(target, btn_x, iy, btn_w, 32.0, "开始评测", true);
        self.ui.sandbox_eval.regions.start_button = Some((btn_x, iy, btn_w, 32.0));
        cy += card_h + 16.0;

        // ---- 沙盒规则说明卡片 ----
        let rules = [
            "◆ 每轮评测创建一个只属于智能体的临时工作目录（沙盒）",
            "◆ 智能体只能在沙盒内创建 / 修改文件，路径越界与终端命令一律拦截",
            "◆ 每个任务允许一次联网搜索，获取最新资料",
            "◆ 定时模式下时间用尽将立即终止，未完成任务记为超时",
            "◆ 任务结束后由你逐个打分（1-10 分），自动计算平均分",
            "◆ 全部产物与评测报告可一键打包导出（zip）",
        ];
        let rules_h = SB_PAD * 2.0 + 26.0 + rules.len() as f32 * 22.0;
        self.sb_card(target, cx, cy, content_w, rules_h);
        let mut ry = cy + SB_PAD;
        self.sb_text(
            target,
            "沙盒规则",
            cx + SB_PAD,
            ry,
            200.0,
            20.0,
            14.0,
            true,
            c_text(),
        );
        ry += 28.0;
        for r in rules {
            self.sb_text(
                target,
                r,
                cx + SB_PAD,
                ry,
                content_w - SB_PAD * 2.0,
                18.0,
                12.0,
                false,
                c_text_dim(),
            );
            ry += 22.0;
        }
        cy += rules_h + 16.0;
        cy
    }

    // =====================================================================
    // Planning 阶段
    // =====================================================================

    unsafe fn sb_render_planning(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        cx: f32,
        mut cy: f32,
        content_w: f32,
    ) -> f32 {
        let live = self.ui.sandbox_eval.live_tail.clone();
        let thinking = self.ui.sandbox_eval.live_thinking;
        let preview_h = if live.is_empty() { 0.0 } else { 110.0 };
        let card_h = SB_PAD * 2.0 + 30.0 + 24.0 + preview_h + 44.0;
        self.sb_card(target, cx, cy, content_w, card_h);
        let ix = cx + SB_PAD;
        let iw = content_w - SB_PAD * 2.0;
        let mut iy = cy + SB_PAD;
        self.sb_text(
            target,
            "正在规划任务…",
            ix,
            iy,
            iw,
            22.0,
            14.0,
            true,
            c_accent(),
        );
        iy += 30.0;
        let hint = if thinking {
            "模型正在深度思考，请稍候…"
        } else {
            "正在把主题拆解为任务清单"
        };
        self.sb_text(target, hint, ix, iy, iw, 18.0, 12.0, false, c_text_dim());
        iy += 24.0;
        if !live.is_empty() {
            self.sb_preview_box(target, ix, iy, iw, preview_h, &live);
            iy += preview_h;
        }
        iy += 8.0;
        self.sb_button(target, ix, iy, 96.0, 30.0, "终止评测", false);
        self.ui.sandbox_eval.regions.stop_button = Some((ix, iy, 96.0, 30.0));
        cy += card_h + 16.0;
        cy
    }

    // =====================================================================
    // Running 阶段
    // =====================================================================

    unsafe fn sb_render_running(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        cx: f32,
        mut cy: f32,
        content_w: f32,
    ) -> f32 {
        // ---- 状态卡片：进度 + 倒计时 + 终止 ----
        let card_h = 96.0;
        self.sb_card(target, cx, cy, content_w, card_h);
        let ix = cx + SB_PAD;
        let iw = content_w - SB_PAD * 2.0;
        let mut iy = cy + SB_PAD;
        let total = self.ui.sandbox_eval.tasks.len();
        let done = self
    .ui.sandbox_eval
            .tasks
            .iter()
            .filter(|t| t.is_finished())
            .count();
        let running_count = self
    .ui.sandbox_eval
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    SandboxTaskStatus::Running | SandboxTaskStatus::Searching
                )
            })
            .count();
        self.sb_text(
            target,
            &format!(
                "任务进度：{} / {}    正在执行：{}",
                done, total, running_count
            ),
            ix,
            iy,
            iw * 0.6,
            22.0,
            14.0,
            true,
            c_text(),
        );
        // 倒计时（定时模式）
        if let Some(remaining) = self.ui.sandbox_eval.remaining_secs() {
            let warned = self.ui.sandbox_eval.five_min_warned;
            let color = if warned { c_red() } else { c_text() };
            self.sb_text_aligned(
                target,
                &format!("剩余 {:02}:{:02}", remaining / 60, remaining % 60),
                ix + iw * 0.6,
                iy,
                iw * 0.4,
                22.0,
                16.0,
                true,
                color,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
            if warned {
                self.sb_text_aligned(
                    target,
                    "⚠ 已提醒智能体：还有 5 分钟",
                    ix + iw * 0.4,
                    iy + 24.0,
                    iw * 0.6,
                    18.0,
                    11.0,
                    false,
                    c_orange(),
                    DWRITE_TEXT_ALIGNMENT_TRAILING,
                );
            }
        } else {
            self.sb_text_aligned(
                target,
                "不限时",
                ix + iw * 0.6,
                iy,
                iw * 0.4,
                22.0,
                13.0,
                false,
                c_text_dim(),
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
        }
        iy += 34.0;
        self.sb_button(target, ix, iy, 96.0, 28.0, "终止评测", false);
        self.ui.sandbox_eval.regions.stop_button = Some((ix, iy, 96.0, 28.0));
        cy += card_h + 16.0;

        // ---- 任务列表 ----
        let n = self.ui.sandbox_eval.tasks.len();
        for i in 0..n {
            let (title, status, files, elapsed_ms, _agent) = {
                let t = &self.ui.sandbox_eval.tasks[i];
                (
                    t.title.clone(),
                    t.status,
                    t.files.len(),
                    t.elapsed_ms,
                    t.agent,
                )
            };
            // 查找正在执行此任务的 worker 以展示其实时输出
            let (live, thinking) = self
    .ui.sandbox_eval
                .workers
                .iter()
                .find(|w| w.task == Some(i))
                .map(|w| (w.live_tail.clone(), w.live_thinking))
                .unwrap_or_default();
            let is_active = matches!(
                status,
                SandboxTaskStatus::Running | SandboxTaskStatus::Searching
            );
            let preview_h = if is_active && !live.is_empty() {
                96.0
            } else {
                0.0
            };
            let think_h = if is_active && live.is_empty() && thinking {
                20.0
            } else {
                0.0
            };
            let row_h = 58.0 + preview_h + think_h;
            self.sb_card(target, cx, cy, content_w, row_h);
            let tx = cx + SB_PAD;
            let tw = content_w - SB_PAD * 2.0;
            let mut ty = cy + 10.0;
            // 状态徽章 + 标题
            let (badge, badge_color) = sb_status_badge(status);
            self.sb_badge(target, tx, ty, &badge, badge_color);
            self.sb_text(
                target,
                &crate::sandbox_eval::truncate_chars(&format!("{}. {}", i + 1, title), 60),
                tx + 88.0,
                ty,
                tw - 200.0,
                20.0,
                13.0,
                true,
                c_text(),
            );
            // 右侧信息
            let info = if status == SandboxTaskStatus::Done {
                format!("{} 个文件 · {:.1}s", files, elapsed_ms as f64 / 1000.0)
            } else if files > 0 {
                format!("{} 个文件", files)
            } else {
                String::new()
            };
            if !info.is_empty() {
                self.sb_text_aligned(
                    target,
                    &info,
                    tx + tw - 160.0,
                    ty,
                    160.0,
                    20.0,
                    11.0,
                    false,
                    c_text_dim(),
                    DWRITE_TEXT_ALIGNMENT_TRAILING,
                );
            }
            ty += 28.0;
            if think_h > 0.0 {
                self.sb_text(
                    target,
                    "模型思考中…",
                    tx + 88.0,
                    ty,
                    tw - 100.0,
                    18.0,
                    11.0,
                    false,
                    c_text_dim(),
                );
            }
            if preview_h > 0.0 {
                self.sb_preview_box(target, tx, ty, tw, preview_h, &live);
            }
            cy += row_h + 8.0;
        }
        cy += 8.0;
        cy
    }

    // =====================================================================
    // Scoring 阶段
    // =====================================================================

    unsafe fn sb_render_scoring(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        cx: f32,
        mut cy: f32,
        content_w: f32,
    ) -> f32 {
        // ---- 结果总览卡片 ----
        let card_h = 132.0;
        self.sb_card(target, cx, cy, content_w, card_h);
        let ix = cx + SB_PAD;
        let iw = content_w - SB_PAD * 2.0;
        let mut iy = cy + SB_PAD;
        self.sb_text(
            target,
            "评测结果",
            ix,
            iy,
            200.0,
            22.0,
            14.0,
            true,
            c_text(),
        );
        // 平均分（大号）
        let avg = self.ui.sandbox_eval.average_score();
        let avg_text = match avg {
            Some(a) => format!("{:.1}", a),
            None => "—".to_string(),
        };
        self.sb_text_aligned(
            target,
            &format!("平均分  {}", avg_text),
            ix + iw - 260.0,
            iy - 2.0,
            260.0,
            30.0,
            22.0,
            true,
            if avg.is_some() {
                c_green()
            } else {
                c_text_dim()
            },
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        );
        iy += 30.0;
        let total = self.ui.sandbox_eval.tasks.len();
        let done = self
    .ui.sandbox_eval
            .tasks
            .iter()
            .filter(|t| t.status == SandboxTaskStatus::Done)
            .count();
        let secs = self.ui.sandbox_eval.total_elapsed_ms / 1000;
        let scored = self
    .ui.sandbox_eval
            .tasks
            .iter()
            .filter(|t| t.score.is_some())
            .count();
        self.sb_text(
            target,
            &format!(
                "完成 {}/{} 个任务 · 总耗时 {} 分 {:02} 秒 · 已打分 {}/{}",
                done,
                total,
                secs / 60,
                secs % 60,
                scored,
                total
            ),
            ix,
            iy,
            iw,
            18.0,
            12.0,
            false,
            c_text_dim(),
        );
        iy += 26.0;
        // 导出 / 再来一轮 / 打开沙盒目录
        self.sb_button(target, ix, iy, 130.0, 32.0, "打包导出结果", true);
        self.ui.sandbox_eval.regions.export_button = Some((ix, iy, 130.0, 32.0));
        self.sb_button(target, ix + 142.0, iy, 110.0, 32.0, "再来一轮", false);
        self.ui.sandbox_eval.regions.restart_button = Some((ix + 142.0, iy, 110.0, 32.0));
        self.sb_button(target, ix + 264.0, iy, 130.0, 32.0, "打开沙盒目录", false);
        self.ui.sandbox_eval.regions.open_dir_button = Some((ix + 264.0, iy, 130.0, 32.0));
        // 导出结果 / 提示信息
        if let Some(msg) = self.ui.sandbox_eval.export_message.clone() {
            self.sb_text_aligned(
                target,
                &crate::sandbox_eval::truncate_chars(&msg, 60),
                ix + 404.0,
                iy + 7.0,
                iw - 404.0,
                18.0,
                11.0,
                false,
                c_green(),
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
        } else if !self.ui.sandbox_eval.all_scored() {
            self.sb_text_aligned(
                target,
                "为每个任务打分后可得到平均分",
                ix + 404.0,
                iy + 7.0,
                iw - 404.0,
                18.0,
                11.0,
                false,
                c_text_dim(),
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
        }
        cy += card_h + 16.0;

        // ---- 逐任务打分卡片 ----
        let n = self.ui.sandbox_eval.tasks.len();
        for i in 0..n {
            let (title, status, files, searches, summary, score, elapsed_ms) = {
                let t = &self.ui.sandbox_eval.tasks[i];
                (
                    t.title.clone(),
                    t.status,
                    t.files.clone(),
                    t.searches.len(),
                    t.summary.clone(),
                    t.score,
                    t.elapsed_ms,
                )
            };
            let files_lines = files.len().min(3);
            let summary_h = if summary.is_empty() { 0.0 } else { 38.0 };
            let row_h = 66.0
                + summary_h
                + files_lines as f32 * 20.0
                + if status == SandboxTaskStatus::Done
                    || status == SandboxTaskStatus::Failed
                    || status == SandboxTaskStatus::TimedOut
                {
                    40.0
                } else {
                    0.0
                };
            self.sb_card(target, cx, cy, content_w, row_h);
            let tx = cx + SB_PAD;
            let tw = content_w - SB_PAD * 2.0;
            let mut ty = cy + 10.0;
            let (badge, badge_color) = sb_status_badge(status);
            self.sb_badge(target, tx, ty, &badge, badge_color);
            self.sb_text(
                target,
                &crate::sandbox_eval::truncate_chars(&format!("{}. {}", i + 1, title), 56),
                tx + 88.0,
                ty,
                tw - 240.0,
                20.0,
                13.0,
                true,
                c_text(),
            );
            self.sb_text_aligned(
                target,
                &format!("{} 次搜索 · {:.1}s", searches, elapsed_ms as f64 / 1000.0),
                tx + tw - 150.0,
                ty,
                150.0,
                20.0,
                11.0,
                false,
                c_text_dim(),
                DWRITE_TEXT_ALIGNMENT_TRAILING,
            );
            ty += 26.0;
            if !summary.is_empty() {
                self.sb_text(
                    target,
                    &crate::sandbox_eval::truncate_chars(&summary, 120),
                    tx,
                    ty,
                    tw,
                    34.0,
                    11.0,
                    false,
                    c_text_dim(),
                );
                ty += summary_h;
            }
            for f in files.iter().take(3) {
                self.sb_text(
                    target,
                    &format!("📄 workspace/{}", f),
                    tx,
                    ty,
                    tw,
                    18.0,
                    11.0,
                    false,
                    c_accent(),
                );
                ty += 20.0;
            }
            // 打分行（已结束的任务才能打分）
            if status == SandboxTaskStatus::Done
                || status == SandboxTaskStatus::Failed
                || status == SandboxTaskStatus::TimedOut
            {
                self.sb_text(
                    target,
                    "打分",
                    tx,
                    ty + 5.0,
                    40.0,
                    18.0,
                    12.0,
                    false,
                    c_text_dim(),
                );
                let mut sx = tx + 44.0;
                for s in 1..=10u8 {
                    let selected = score == Some(s);
                    let cw = 30.0;
                    self.sb_score_chip(target, sx, ty, cw, 26.0, s, selected);
                    self.ui.sandbox_eval
                        .regions
                        .score_chips
                        .push((i, s, (sx, ty, cw, 26.0)));
                    sx += cw + 6.0;
                }
                if let Some(s) = score {
                    self.sb_text(
                        target,
                        &format!("{} 分", s),
                        sx + 6.0,
                        ty + 5.0,
                        60.0,
                        18.0,
                        12.0,
                        true,
                        c_green(),
                    );
                }
            }
            cy += row_h + 8.0;
        }
        cy += 8.0;
        cy
    }

    // =====================================================================
    // 执行日志卡片
    // =====================================================================

    unsafe fn sb_render_log(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        cx: f32,
        mut cy: f32,
        content_w: f32,
    ) -> f32 {
        if self.ui.sandbox_eval.log.is_empty() {
            return cy;
        }
        // 只显示最近 14 条
        let entries: Vec<(String, SandboxLogKind, String)> = self
    .ui.sandbox_eval
            .log
            .iter()
            .rev()
            .take(14)
            .rev()
            .map(|e| (e.time.clone(), e.kind, e.text.clone()))
            .collect();
        let card_h = SB_PAD * 2.0 + 26.0 + entries.len() as f32 * 20.0;
        self.sb_card(target, cx, cy, content_w, card_h);
        let ix = cx + SB_PAD;
        let iw = content_w - SB_PAD * 2.0;
        let mut iy = cy + SB_PAD;
        self.sb_text(
            target,
            "执行日志",
            ix,
            iy,
            200.0,
            20.0,
            14.0,
            true,
            c_text(),
        );
        iy += 28.0;
        for (time, kind, text) in entries {
            let color = match kind {
                SandboxLogKind::Info => c_text_dim(),
                SandboxLogKind::File => c_accent(),
                SandboxLogKind::Search => c_green(),
                SandboxLogKind::Warn => c_orange(),
                SandboxLogKind::Error => c_red(),
            };
            self.sb_text(
                target,
                &crate::sandbox_eval::truncate_chars(&format!("[{}] {}", time, text), 110),
                ix,
                iy,
                iw,
                18.0,
                11.0,
                false,
                color,
            );
            iy += 20.0;
        }
        cy += card_h + 16.0;
        cy
    }

    // =====================================================================
    // 基础绘制辅助
    // =====================================================================

    /// 圆角卡片（底 + 描边）
    unsafe fn sb_card(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 6.0,
            radiusY: 6.0,
        };
        if let Ok(bg) = self.win.render_ctx.brush_cache.get_brush(target, &c_card_bg()) {
            target.FillRoundedRectangle(&rounded, &bg);
        }
        if let Ok(border) = self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &c_card_border())
        {
            target.DrawRoundedRectangle(&rounded, &border, 1.0, None);
        }
    }

    /// 左对齐文本
    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_text(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        bold: bool,
        color: windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
    ) {
        self.sb_text_aligned(
            target,
            text,
            x,
            y,
            w,
            h,
            size,
            bold,
            color,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        );
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_text_aligned(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        bold: bool,
        color: windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
        align: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
    ) {
        let weight = if bold {
            DWRITE_FONT_WEIGHT_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let Ok(format) = self.win.render_ctx.text_format_cache.get_format(
            size,
            weight.0 as u32,
            align.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
        ) else {
            return;
        };
        let Ok(brush) = self.win.render_ctx.brush_cache.get_brush(target, &color) else {
            return;
        };
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        target.DrawText(
            &wide,
            &format,
            &rect,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// 输入框：圆角底 + 激活边框 + 文本 / 占位符 + 末尾光标
    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_input_field(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        value: &str,
        placeholder: &str,
        active: bool,
        caret: bool,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 5.0,
            radiusY: 5.0,
        };
        if let Ok(bg) = self.win.render_ctx.brush_cache.get_brush(target, &c_field_bg()) {
            target.FillRoundedRectangle(&rounded, &bg);
        }
        let border_color = if active { c_accent() } else { c_card_border() };
        if let Ok(border) = self.win.render_ctx.brush_cache.get_brush(target, &border_color) {
            target.DrawRoundedRectangle(&rounded, &border, 1.0, None);
        }
        // 文本内容（超长显示尾部）
        let font_size = if h < 30.0 { 11.0 } else { 12.5 };
        let max_chars = ((w - 20.0) / (font_size * 0.75)).max(4.0) as usize;
        let shown: String = if value.chars().count() > max_chars {
            let tail: String = value
                .chars()
                .rev()
                .take(max_chars)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("…{}", tail)
        } else {
            value.to_string()
        };
        let text_y = y + (h - font_size - 4.0) / 2.0;
        if value.is_empty() && !active {
            self.sb_text(
                target,
                placeholder,
                x + 10.0,
                text_y,
                w - 20.0,
                h,
                font_size,
                false,
                color_f(0.45, 0.45, 0.48, 1.0),
            );
        } else {
            let display = if caret {
                format!("{}▏", shown)
            } else {
                shown
            };
            self.sb_text(
                target,
                &display,
                x + 10.0,
                text_y,
                w - 20.0,
                h,
                font_size,
                false,
                c_text(),
            );
        }
    }

    /// 选择项 chip（选中态高亮）
    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_chip(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        selected: bool,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 13.0,
            radiusY: 13.0,
        };
        let bg = if selected {
            c_accent_bg()
        } else {
            c_field_bg()
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
            target.FillRoundedRectangle(&rounded, &b);
        }
        let border = if selected {
            c_accent()
        } else {
            c_card_border()
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &border) {
            target.DrawRoundedRectangle(&rounded, &b, 1.0, None);
        }
        let color = if selected { c_accent() } else { c_text_dim() };
        self.sb_text_aligned(
            target,
            label,
            x,
            y + (h - 15.0) / 2.0,
            w,
            h,
            11.5,
            selected,
            color,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
    }

    /// 分数 chip（1-10）
    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_score_chip(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        score: u8,
        selected: bool,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 5.0,
            radiusY: 5.0,
        };
        let bg = if selected {
            color_f(0.20, 0.72, 0.40, 0.25)
        } else {
            c_field_bg()
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
            target.FillRoundedRectangle(&rounded, &b);
        }
        let border = if selected { c_green() } else { c_card_border() };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &border) {
            target.DrawRoundedRectangle(&rounded, &b, 1.0, None);
        }
        let color = if selected { c_green() } else { c_text_dim() };
        self.sb_text_aligned(
            target,
            &score.to_string(),
            x,
            y + (h - 15.0) / 2.0,
            w,
            h,
            11.5,
            selected,
            color,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
    }

    /// 按钮（primary = 蓝色实底）
    #[allow(clippy::too_many_arguments)]
    unsafe fn sb_button(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        primary: bool,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 6.0,
            radiusY: 6.0,
        };
        let bg = if primary {
            c_accent()
        } else {
            color_f(0.22, 0.22, 0.24, 1.0)
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
            target.FillRoundedRectangle(&rounded, &b);
        }
        if !primary {
            if let Ok(b) = self
    .win.render_ctx
                .brush_cache
                .get_brush(target, &c_card_border())
            {
                target.DrawRoundedRectangle(&rounded, &b, 1.0, None);
            }
        }
        let color = if primary {
            color_f(1.0, 1.0, 1.0, 1.0)
        } else {
            c_text()
        };
        self.sb_text_aligned(
            target,
            label,
            x,
            y + (h - 16.0) / 2.0,
            w,
            h,
            12.0,
            true,
            color,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
    }

    /// 状态徽章（固定 80 宽）
    unsafe fn sb_badge(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        label: &str,
        color: windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
    ) {
        let w = 80.0;
        let h = 20.0;
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 10.0,
            radiusY: 10.0,
        };
        let bg = windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
            r: color.r,
            g: color.g,
            b: color.b,
            a: 0.16,
        };
        if let Ok(b) = self.win.render_ctx.brush_cache.get_brush(target, &bg) {
            target.FillRoundedRectangle(&rounded, &b);
        }
        self.sb_text_aligned(
            target,
            label,
            x,
            y + 3.0,
            w,
            h,
            10.5,
            false,
            color,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        );
    }

    /// 实时输出预览盒（深色底、截尾多行文本）
    unsafe fn sb_preview_box(
        &mut self,
        target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
    ) {
        let rect = D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h - 6.0,
        };
        let rounded = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect,
            radiusX: 5.0,
            radiusY: 5.0,
        };
        if let Ok(b) = self
    .win.render_ctx
            .brush_cache
            .get_brush(target, &color_f(0.08, 0.08, 0.09, 1.0))
        {
            target.FillRoundedRectangle(&rounded, &b);
        }
        // 只保留最后几行
        let max_lines = ((h - 22.0) / 16.0).max(1.0) as usize;
        let lines: Vec<&str> = text.lines().collect();
        let shown: Vec<&str> = lines.iter().rev().take(max_lines).rev().copied().collect();
        let mut ty = y + 8.0;
        for line in shown {
            self.sb_text(
                target,
                &crate::sandbox_eval::truncate_chars(line, 100),
                x + 10.0,
                ty,
                w - 20.0,
                16.0,
                10.5,
                false,
                color_f(0.72, 0.78, 0.72, 1.0),
            );
            ty += 16.0;
        }
    }
}

/// 状态 → (徽章文字, 颜色)
fn sb_status_badge(
    status: SandboxTaskStatus,
) -> (
    String,
    windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F,
) {
    match status {
        SandboxTaskStatus::Pending => ("等待中".to_string(), color_f(0.62, 0.62, 0.65, 1.0)),
        SandboxTaskStatus::Running => ("执行中".to_string(), color_f(0.25, 0.55, 0.95, 1.0)),
        SandboxTaskStatus::Searching => ("联网搜索中".to_string(), color_f(0.30, 0.80, 0.48, 1.0)),
        SandboxTaskStatus::Done => ("已完成".to_string(), color_f(0.30, 0.80, 0.48, 1.0)),
        SandboxTaskStatus::Failed => ("失败".to_string(), color_f(0.90, 0.35, 0.35, 1.0)),
        SandboxTaskStatus::TimedOut => ("超时".to_string(), color_f(0.95, 0.65, 0.25, 1.0)),
    }
}
