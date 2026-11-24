//! 全应用共享的浅色薄荷绿主题。

use aether_render::d2d::factory::color_f;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

// ---- 文字 ----
pub(super) fn text_primary() -> D2D1_COLOR_F {
    color_f(0.10, 0.16, 0.13, 1.0)
}
pub(super) fn text_secondary() -> D2D1_COLOR_F {
    color_f(0.36, 0.44, 0.40, 1.0)
}
pub(super) fn text_muted() -> D2D1_COLOR_F {
    color_f(0.52, 0.60, 0.56, 1.0)
}

// ---- 主题色（薄荷绿）----
pub(super) fn accent() -> D2D1_COLOR_F {
    color_f(0.07, 0.45, 0.33, 1.0)
}
pub(super) fn accent_hover() -> D2D1_COLOR_F {
    color_f(0.05, 0.37, 0.26, 1.0)
}
pub(super) fn accent_soft() -> D2D1_COLOR_F {
    color_f(0.895, 0.963, 0.930, 1.0)
}
pub(super) fn accent_soft_hover() -> D2D1_COLOR_F {
    color_f(0.845, 0.940, 0.895, 1.0)
}
pub(super) fn accent_border() -> D2D1_COLOR_F {
    color_f(0.60, 0.83, 0.72, 1.0)
}
pub(super) fn accent_text() -> D2D1_COLOR_F {
    color_f(0.05, 0.38, 0.27, 1.0)
}

// ---- 背景与面 ----
pub(super) fn page_bg_top() -> D2D1_COLOR_F {
    color_f(1.0, 1.0, 1.0, 1.0)
}
pub(super) fn page_bg_bottom() -> D2D1_COLOR_F {
    color_f(0.870, 0.958, 0.920, 1.0)
}
pub(super) fn titlebar_bg_top() -> D2D1_COLOR_F {
    accent_soft()
}
pub(super) fn titlebar_bg_bottom() -> D2D1_COLOR_F {
    surface()
}
pub(super) fn statusbar_bg_top() -> D2D1_COLOR_F {
    page_bg_bottom()
}
pub(super) fn statusbar_bg_bottom() -> D2D1_COLOR_F {
    color_f(0.820, 0.940, 0.880, 1.0)
}
pub(super) fn nav_bg() -> D2D1_COLOR_F {
    color_f(0.985, 0.996, 0.990, 1.0)
}
pub(super) fn surface() -> D2D1_COLOR_F {
    color_f(1.0, 1.0, 1.0, 1.0)
}
pub(super) fn surface_alt() -> D2D1_COLOR_F {
    color_f(0.965, 0.988, 0.977, 1.0)
}
pub(super) fn input_bg() -> D2D1_COLOR_F {
    color_f(1.0, 1.0, 1.0, 1.0)
}
pub(super) fn border() -> D2D1_COLOR_F {
    color_f(0.845, 0.910, 0.880, 1.0)
}
pub(super) fn border_strong() -> D2D1_COLOR_F {
    color_f(0.72, 0.84, 0.79, 1.0)
}
pub(super) fn separator() -> D2D1_COLOR_F {
    color_f(0.895, 0.940, 0.920, 1.0)
}

// ---- 控件 ----
pub(super) fn switch_off() -> D2D1_COLOR_F {
    color_f(0.780, 0.855, 0.820, 1.0)
}
pub(super) fn track_bg() -> D2D1_COLOR_F {
    color_f(0.870, 0.925, 0.897, 1.0)
}
pub(super) fn track_disabled() -> D2D1_COLOR_F {
    color_f(0.905, 0.940, 0.922, 1.0)
}
pub(super) fn knob() -> D2D1_COLOR_F {
    color_f(1.0, 1.0, 1.0, 1.0)
}
pub(super) fn danger() -> D2D1_COLOR_F {
    color_f(0.78, 0.24, 0.24, 1.0)
}
pub(super) fn danger_soft() -> D2D1_COLOR_F {
    color_f(0.988, 0.933, 0.930, 1.0)
}
pub(super) fn warning() -> D2D1_COLOR_F {
    color_f(0.80, 0.52, 0.06, 1.0)
}
pub(super) fn warning_soft() -> D2D1_COLOR_F {
    color_f(0.994, 0.958, 0.882, 1.0)
}

pub(super) unsafe fn fill_vertical_gradient(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    rect: &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F,
    top: D2D1_COLOR_F,
    bottom: D2D1_COLOR_F,
) {
    fill_gradient(target, rect, rect.top, rect.bottom, top, bottom);
}

pub(super) unsafe fn fill_app_gradient(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    rect: &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F,
    window_height: f32,
) {
    fill_gradient(
        target,
        rect,
        0.0,
        window_height.max(1.0),
        titlebar_bg_top(),
        statusbar_bg_bottom(),
    );
}

unsafe fn fill_gradient(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    rect: &windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F,
    start_y: f32,
    end_y: f32,
    top: D2D1_COLOR_F,
    bottom: D2D1_COLOR_F,
) {
    use windows::Win32::Graphics::Direct2D::Common::{D2D1_GRADIENT_STOP, D2D_POINT_2F};
    use windows::Win32::Graphics::Direct2D::{
        D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    };
    let stops = [
        D2D1_GRADIENT_STOP {
            position: 0.0,
            color: top,
        },
        D2D1_GRADIENT_STOP {
            position: 1.0,
            color: bottom,
        },
    ];
    if let Ok(collection) =
        target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    {
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F {
                x: rect.left,
                y: start_y,
            },
            endPoint: D2D_POINT_2F {
                x: rect.left,
                y: end_y,
            },
        };
        if let Ok(brush) = target.CreateLinearGradientBrush(&props, None, &collection) {
            target.FillRectangle(rect, &brush);
        }
    }
}
