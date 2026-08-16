//! 鼠标光标语义化
//!
//! 根据 hover 区域返回不同的光标类型，由 `WM_SETCURSOR` 调用 `LoadCursorW` + `SetCursor`。
//! `mouse_move.rs` 仅暴露 `compute_cursor_for_pos` 计算光标类型，不直接调用 `SetCursor`。

/// 光标类型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorType {
    #[default]
    Arrow,
    IBeam,
    Hand,
    SizeWE,
    SizeNS,
    /// 西北-东南斜向（\ 方向），用于左下拐角手柄
    SizeNWSE,
    /// 东北-西南斜向（/ 方向），用于右下拐角手柄
    SizeNESW,
    /// 张开手形：可拖动（默认悬停状态）
    Grab,
    /// 握紧手形：正在拖动（按住鼠标时）
    Grabbing,
    /// 四向箭头：可移动 / 拖拽整体
    Move,
    /// 四向箭头加圆点：可向任意方向滚动
    AllScroll,
}

impl CursorType {
    /// 返回对应的 IDC_* 光标资源常量（windows crate 中的 PCWSTR）
    pub fn idc_cursor(self) -> windows::core::PCWSTR {
        use windows::Win32::UI::WindowsAndMessaging::*;
        match self {
            CursorType::Arrow => IDC_ARROW,
            CursorType::IBeam => IDC_IBEAM,
            CursorType::Hand => IDC_HAND,
            CursorType::SizeWE => IDC_SIZEWE,
            CursorType::SizeNS => IDC_SIZENS,
            CursorType::SizeNWSE => IDC_SIZENWSE,
            CursorType::SizeNESW => IDC_SIZENESW,
            // Win32 无内置 grab/grabbing/all-scroll 光标，用系统最近似替代：
            // Grab → 手形（IDC_HAND），Grabbing → 四向箭头（拖动中），
            // Move / AllScroll → 四向箭头（IDC_SIZEALL）。
            CursorType::Grab => IDC_HAND,
            CursorType::Grabbing => IDC_SIZEALL,
            CursorType::Move => IDC_SIZEALL,
            CursorType::AllScroll => IDC_SIZEALL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_arrow() {
        let c = CursorType::default();
        assert_eq!(c, CursorType::Arrow);
    }

    #[test]
    fn test_idc_cursor_mapping() {
        use windows::Win32::UI::WindowsAndMessaging::{
            IDC_ARROW, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
            IDC_SIZEWE,
        };
        assert_eq!(CursorType::Arrow.idc_cursor(), IDC_ARROW);
        assert_eq!(CursorType::IBeam.idc_cursor(), IDC_IBEAM);
        assert_eq!(CursorType::Hand.idc_cursor(), IDC_HAND);
        assert_eq!(CursorType::SizeWE.idc_cursor(), IDC_SIZEWE);
        assert_eq!(CursorType::SizeNS.idc_cursor(), IDC_SIZENS);
        assert_eq!(CursorType::SizeNWSE.idc_cursor(), IDC_SIZENWSE);
        assert_eq!(CursorType::SizeNESW.idc_cursor(), IDC_SIZENESW);
        assert_eq!(CursorType::Grab.idc_cursor(), IDC_HAND);
        assert_eq!(CursorType::Grabbing.idc_cursor(), IDC_SIZEALL);
        assert_eq!(CursorType::Move.idc_cursor(), IDC_SIZEALL);
        assert_eq!(CursorType::AllScroll.idc_cursor(), IDC_SIZEALL);
    }

    #[test]
    fn test_copy_and_eq() {
        let a = CursorType::IBeam;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(a, CursorType::Arrow);
    }
}
