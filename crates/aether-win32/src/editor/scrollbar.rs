//! 编辑区覆盖式滚动条（VS Code 风格）：几何计算、渲染与鼠标交互。
//!
//! 垂直滚动条映射 `scroll_y` / 全文行高，水平滚动条映射 `scroll_x` / 文档最长行宽。
//! 滑块可拖拽；点击轨道跳转至点击位置并可继续拖拽。
//! 无内容溢出时对应滚动条不绘制、不命中。

use super::*;
use crate::layout::Region;
use aether_render::d2d::factory::color_f;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget;

/// 滚动条厚度（逻辑像素）
pub const THICKNESS: f32 = 10.0;
/// 滑块最小长度，保证可抓取
const THUMB_MIN: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// 命中测试结果
#[derive(Clone, Copy, Debug)]
pub enum Hit {
    /// 命中滑块，f32 = 鼠标距滑块顶/左边的抓取偏移
    Thumb(Axis, f32),
    /// 命中轨道（滑块外区域）
    Track(Axis),
}

/// 当前帧滚动条几何（None = 内容未溢出，不绘制该条）
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub v_track: Option<Region>,
    pub v_thumb: Option<Region>,
    pub h_track: Option<Region>,
    pub h_thumb: Option<Region>,
    pub v_max_scroll: f32,
    pub h_max_scroll: f32,
}

/// 计算滚动条几何。`region` 为编辑内容区域（与 render_editor 入参一致）。
pub fn geometry(state: &EditorState, region: &Region) -> Geometry {
    let line_height = state.win.text_renderer.line_height();
    let char_width = state.win.text_renderer.char_width();

    let total_h = state.editor.content.buffer.len_lines() as f32 * line_height;
    let view_h = region.height;
    let v_max = (total_h - view_h).max(0.0);

    // 水平可视宽与 scroll_horizontal 的钳制口径一致（行号 60 + 内边距 5）
    let content_w = state.editor.content.content_width_chars as f32 * char_width;
    let view_w = (region.width - 60.0 - 5.0).max(1.0);
    let h_max = (content_w - view_w).max(0.0);

    let v_visible = v_max > 0.0;
    let h_visible = h_max > 0.0;
    let mut g = Geometry {
        v_max_scroll: v_max,
        h_max_scroll: h_max,
        ..Default::default()
    };
    if !v_visible && !h_visible {
        return g;
    }

    // 两条同时存在时各自让出拐角，避免重叠
    let track_h = region.height - if h_visible { THICKNESS } else { 0.0 };
    let track_w = region.width - if v_visible { THICKNESS } else { 0.0 };

    if v_visible {
        let track = Region::new(
            region.x + region.width - THICKNESS,
            region.y,
            THICKNESS,
            track_h,
        );
        let thumb_h = (view_h / total_h * track_h).clamp(THUMB_MIN, track_h);
        let usable = (track_h - thumb_h).max(0.0);
        let ratio = (state.editor.content.scroll_y / v_max).clamp(0.0, 1.0);
        let thumb = Region::new(track.x, track.y + ratio * usable, THICKNESS, thumb_h);
        g.v_track = Some(track);
        g.v_thumb = Some(thumb);
    }
    if h_visible {
        let track = Region::new(
            region.x,
            region.y + region.height - THICKNESS,
            track_w,
            THICKNESS,
        );
        let thumb_w = (view_w / content_w * track_w).clamp(THUMB_MIN, track_w);
        let usable = (track_w - thumb_w).max(0.0);
        let ratio = (state.editor.content.scroll_x / h_max).clamp(0.0, 1.0);
        let thumb = Region::new(track.x + ratio * usable, track.y, thumb_w, THICKNESS);
        g.h_track = Some(track);
        g.h_thumb = Some(thumb);
    }
    g
}

/// 刷新文档最长行宽缓存（水平滚动条比例依据）。
///
/// 小文件：buffer_version 变化时全量扫描（精确）；
/// 大文件：仅按可见行单调递增更新，避免每次击键全量扫描的卡顿。
pub fn refresh_content_width(state: &mut EditorState) {
    let version = state.editor.content.buffer_version;
    if state.editor.content.content_width_version == version {
        return;
    }
    let mut max_chars = if state.editor.content.is_large_file {
        // 大文件保留历史最大值（单调），删除长行后的过期值在重新打开时复位
        state.editor.content.content_width_chars
    } else {
        0
    };
    if state.editor.content.is_large_file {
        let (start, end) = state.visible_line_range();
        for idx in start..end {
            if let Some(text) = state.editor.content.cached_line(idx) {
                let w: usize = text.chars().map(unicode_char_width).sum();
                if w > max_chars {
                    max_chars = w;
                }
            }
        }
    } else {
        let total = state.editor.content.buffer.len_lines();
        for idx in 0..total {
            if let Some(text) = state.editor.content.buffer.get_line(idx) {
                let w: usize = text.chars().map(unicode_char_width).sum();
                if w > max_chars {
                    max_chars = w;
                }
            }
        }
    }
    state.editor.content.content_width_chars = max_chars;
    state.editor.content.content_width_version = version;
}

/// 命中检测：滑块优先于轨道，垂直优先于水平。
pub fn hit_test(g: &Geometry, mx: f32, my: f32) -> Option<Hit> {
    if let Some(ref t) = g.v_thumb {
        if t.contains(mx, my) {
            return Some(Hit::Thumb(Axis::Vertical, my - t.y));
        }
    }
    if let Some(ref t) = g.h_thumb {
        if t.contains(mx, my) {
            return Some(Hit::Thumb(Axis::Horizontal, mx - t.x));
        }
    }
    if let Some(ref t) = g.v_track {
        if t.contains(mx, my) {
            return Some(Hit::Track(Axis::Vertical));
        }
    }
    if let Some(ref t) = g.h_track {
        if t.contains(mx, my) {
            return Some(Hit::Track(Axis::Horizontal));
        }
    }
    None
}

/// 悬停轴（用于高亮），未命中返回 None。
pub fn hover_axis(g: &Geometry, mx: f32, my: f32) -> Option<Axis> {
    match hit_test(g, mx, my) {
        Some(Hit::Thumb(axis, _)) | Some(Hit::Track(axis)) => Some(axis),
        None => None,
    }
}

/// 拖拽应用：由鼠标位置反推滚动偏移（grab_offset 为按下时鼠标距滑块边缘的偏移）。
pub fn apply_scroll(
    state: &mut EditorState,
    region: &Region,
    axis: Axis,
    mx: f32,
    my: f32,
    grab_offset: f32,
) {
    let g = geometry(state, region);
    match axis {
        Axis::Vertical => {
            if let (Some(track), Some(thumb)) = (g.v_track, g.v_thumb) {
                let usable = (track.height - thumb.height).max(0.0);
                let rel = (my - grab_offset - track.y).clamp(0.0, usable);
                let ratio = if usable > 0.0 { rel / usable } else { 0.0 };
                state.editor.content.scroll_y = ratio * g.v_max_scroll;
                state.emit_event(crate::events::EditorEvent::Scrolled);
            }
        }
        Axis::Horizontal => {
            if let (Some(track), Some(thumb)) = (g.h_track, g.h_thumb) {
                let usable = (track.width - thumb.width).max(0.0);
                let rel = (mx - grab_offset - track.x).clamp(0.0, usable);
                let ratio = if usable > 0.0 { rel / usable } else { 0.0 };
                state.editor.content.scroll_x = ratio * g.h_max_scroll;
                state.emit_event(crate::events::EditorEvent::Scrolled);
            }
        }
    }
}

/// 轨道点击：滑块居中到点击位置并返回新的抓取偏移（滑块半长），可继续拖拽。
pub fn jump_and_grab(
    state: &mut EditorState,
    region: &Region,
    axis: Axis,
    mx: f32,
    my: f32,
) -> f32 {
    let grab = {
        let g = geometry(state, region);
        match axis {
            Axis::Vertical => g.v_thumb.map(|t| t.height / 2.0).unwrap_or(0.0),
            Axis::Horizontal => g.h_thumb.map(|t| t.width / 2.0).unwrap_or(0.0),
        }
    };
    apply_scroll(state, region, axis, mx, my, grab);
    grab
}

/// 左键按下统一入口：命中滚动条则进入拖拽并捕获鼠标，返回 true 表示事件已消费。
/// 经典模式与智能体模式的编辑器点击处理器共用。
///
/// # Safety
/// 内部调用 Win32 `SetCapture`，须在窗口消息处理的 unsafe 上下文中调用。
pub unsafe fn try_begin_drag(state: &mut EditorState, hwnd: HWND, mx: f32, my: f32) -> bool {
    let Some(region) = state.active_code_editor_region() else {
        return false;
    };
    let g = geometry(state, &region);
    let Some(hit) = hit_test(&g, mx, my) else {
        return false;
    };
    let (axis, grab) = match hit {
        Hit::Thumb(axis, off) => (axis, off),
        Hit::Track(axis) => (axis, jump_and_grab(state, &region, axis, mx, my)),
    };
    state.editor.scrollbar_drag = Some(axis);
    state.editor.scrollbar_drag_offset = grab;
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
    true
}

/// 鼠标移动统一入口的处理结果
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveOutcome {
    /// 拖拽中：已应用滚动，调用方应 invalidate 并消费事件
    Dragged,
    /// 悬停高亮变化：调用方应 invalidate
    HoverChanged,
    /// 与滚动条无关
    Ignored,
}

/// 鼠标移动统一入口：拖拽中应用滚动；否则更新悬停高亮。
/// 经典模式与智能体模式的 WM_MOUSEMOVE 处理器共用。
pub fn handle_move(state: &mut EditorState, mx: f32, my: f32) -> MoveOutcome {
    let Some(region) = state.active_code_editor_region() else {
        if state.editor.scrollbar_hover.is_some() {
            state.editor.scrollbar_hover = None;
            return MoveOutcome::HoverChanged;
        }
        return MoveOutcome::Ignored;
    };
    if let Some(axis) = state.editor.scrollbar_drag {
        let grab = state.editor.scrollbar_drag_offset;
        apply_scroll(state, &region, axis, mx, my, grab);
        return MoveOutcome::Dragged;
    }
    let g = geometry(state, &region);
    let hover = hover_axis(&g, mx, my);
    if hover != state.editor.scrollbar_hover {
        state.editor.scrollbar_hover = hover;
        return MoveOutcome::HoverChanged;
    }
    MoveOutcome::Ignored
}

impl EditorState {
    /// 当前视图是否渲染代码编辑器（滚动条仅在该场景出现）。
    /// 与 render 主流程（经典中间区 / 智能体右侧标签面板）的分支条件保持一致。
    pub fn scrollbars_active(&self) -> bool {
        !self.show_welcome()
            && !self.ntp_active()
            && !self.active_tab_is_new_tab()
            && !self.active_tab_is_browser()
            && !self.active_tab_is_settings()
            && !self.active_tab_is_sandbox_eval()
            && !self.active_tab_is_terminal()
            && !self.editor.markdown_preview
            && self.editor.content.language != aether_core::lexer::Language::Image
            && !self.editor.tab_bar.tabs.is_empty()
    }

    /// 当前活动代码编辑器的渲染区域（当前视图非代码编辑器时 None）。
    ///
    /// 经典模式 = 中间编辑内容区；智能体模式 = 右侧标签面板内容区（扣除标签栏）。
    /// 滚动条渲染 / 命中 / 拖拽 / 滚轮路由的统一几何入口，与渲染几何保持一致。
    pub fn active_code_editor_region(&self) -> Option<Region> {
        if !self.scrollbars_active() {
            return None;
        }
        if self.editor_mode.is_agent() {
            let r = self.ui.layout.right_panel_region();
            let tab_h = if self.editor.tab_bar.tabs.is_empty() {
                0.0
            } else {
                crate::layout::TAB_BAR_HEIGHT
            };
            Some(Region::new(
                r.x,
                r.y + tab_h,
                r.width,
                (r.height - tab_h).max(0.0),
            ))
        } else {
            Some(self.ui.layout.editor_content_region(self.show_tab_bar()))
        }
    }

    /// 绘制编辑区覆盖式滚动条（在编辑内容裁剪弹出后调用，浮于文本之上）。
    pub(crate) fn render_scrollbars(
        &mut self,
        target: &ID2D1HwndRenderTarget,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        if !self.scrollbars_active() {
            return;
        }
        refresh_content_width(self);
        let region = Region::new(x, y, width, height);
        let g = geometry(self, &region);
        if g.v_track.is_none() && g.h_track.is_none() {
            return;
        }

        let hover = self.editor.scrollbar_hover;
        let drag = self.editor.scrollbar_drag;
        let thumb_alpha = |axis: Axis| -> f32 {
            if drag == Some(axis) {
                0.6
            } else if hover == Some(axis) {
                0.5
            } else {
                0.35
            }
        };

        unsafe {
            let track_brush = self
                .win
                .render_ctx
                .brush_cache
                .get_brush(target, &color_f(0.5, 0.5, 0.5, 0.08))
                .unwrap();
            let mut fill = |target: &ID2D1HwndRenderTarget, r: &Region, alpha: f32| {
                if let Ok(brush) = self
                    .win
                    .render_ctx
                    .brush_cache
                    .get_brush(target, &color_f(0.62, 0.62, 0.66, alpha))
                {
                    let rect = D2D_RECT_F {
                        left: r.x,
                        top: r.y,
                        right: r.x + r.width,
                        bottom: r.y + r.height,
                    };
                    target.FillRectangle(&rect, &brush);
                }
            };

            if let Some(track) = g.v_track {
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: track.x,
                        top: track.y,
                        right: track.x + track.width,
                        bottom: track.y + track.height,
                    },
                    &track_brush,
                );
            }
            if let Some(track) = g.h_track {
                target.FillRectangle(
                    &D2D_RECT_F {
                        left: track.x,
                        top: track.y,
                        right: track.x + track.width,
                        bottom: track.y + track.height,
                    },
                    &track_brush,
                );
            }
            if let Some(thumb) = g.v_thumb {
                fill(target, &thumb, thumb_alpha(Axis::Vertical));
            }
            if let Some(thumb) = g.h_thumb {
                fill(target, &thumb, thumb_alpha(Axis::Horizontal));
            }
        }
    }
}
