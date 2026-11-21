//! UI 悬停微动效（modern micro-interactions）
//!
//! 为活动栏 / 标签栏的悬停背景提供淡入淡出进度（0.0 → 1.0），
//! 由 `UI_ANIM_TIMER_ID` 定时器每帧驱动 `tick`，动画结束后自动停表。
//! 渲染侧用进度值调制背景透明度，得到柔和的过渡而非生硬的状态跳变。

/// 单帧步进量：16ms 定时器下约 6 帧（~100ms）完成淡入/淡出
const STEP: f32 = 0.18;

/// 渲染用进度量化档数：限制动画期间产生的不同画刷颜色数量，
/// 避免高频 alpha 变化撑爆画刷缓存（8 档在 100ms 内视觉上足够平滑）
pub const QUANTUM: f32 = 8.0;

/// 将动画进度量化到固定档位（渲染侧取色前调用）
pub fn quantize(p: f32) -> f32 {
    (p.clamp(0.0, 1.0) * QUANTUM).round() / QUANTUM
}

/// 活动栏 + 标签栏悬停淡入淡出进度表（按项索引对齐）
#[derive(Clone, Debug, Default)]
pub struct HoverAnim {
    /// 活动栏各项悬停进度
    pub activity: Vec<f32>,
    /// 标签栏各项悬停进度
    pub tabs: Vec<f32>,
    /// AI 欢迎卡快捷 chips 各项悬停进度
    pub ai_chips: Vec<f32>,
    /// AI 侧栏对话项各项悬停进度
    pub ai_tabs: Vec<f32>,
    /// AI 面板按钮悬停进度：0=新会话 1=模型选择 2=发送
    pub ai_btns: Vec<f32>,
    /// AI 输入框聚焦环进度（独立通道：聚焦与按钮悬停可同时存在）
    pub ai_focus: f32,
    /// 渲染侧上一帧见过的聚焦状态：变化时由渲染启动 UI_ANIM 定时器驱动聚焦环淡入淡出
    pub ai_focus_seen: bool,
}

impl HoverAnim {
    pub fn new() -> Self {
        Self::default()
    }

    /// 向目标状态推进一帧：悬停项 → 1.0，其余 → 0.0。
    /// 返回 true 表示仍有动画在途（调用方应保留定时器并继续重绘）。
    pub fn tick(
        &mut self,
        activity_hover: Option<usize>,
        activity_len: usize,
        tab_hover: Option<usize>,
        tab_len: usize,
    ) -> bool {
        Self::advance(&mut self.activity, activity_hover, activity_len)
            | Self::advance(&mut self.tabs, tab_hover, tab_len)
    }

    /// 推进单个进度表；项数变化时同步伸缩（新增项从 0 开始淡入）
    fn advance(progress: &mut Vec<f32>, hover: Option<usize>, len: usize) -> bool {
        progress.resize(len, 0.0);
        let mut animating = false;
        for (i, p) in progress.iter_mut().enumerate() {
            let target = if hover == Some(i) { 1.0 } else { 0.0 };
            if (*p - target).abs() < f32::EPSILON {
                continue;
            }
            *p = if *p < target {
                (*p + STEP).min(target)
            } else {
                (*p - STEP).max(target)
            };
            animating |= (*p - target).abs() > f32::EPSILON;
        }
        animating
    }

    /// 活动栏第 i 项的当前进度（越界返回 0）
    pub fn activity_progress(&self, i: usize) -> f32 {
        self.activity.get(i).copied().unwrap_or(0.0)
    }

    /// 标签栏第 i 项的当前进度（越界返回 0）
    pub fn tab_progress(&self, i: usize) -> f32 {
        self.tabs.get(i).copied().unwrap_or(0.0)
    }

    /// 推进 AI 面板各通道一帧（欢迎 chips / 侧栏对话项 / 按钮 / 输入聚焦环）。
    /// 返回 true 表示仍有动画在途（调用方应保留定时器并继续重绘）。
    pub fn tick_ai(
        &mut self,
        chip_hover: Option<usize>,
        chip_len: usize,
        tab_hover: Option<usize>,
        tab_len: usize,
        btn_hover: Option<usize>,
        input_focused: bool,
    ) -> bool {
        let mut animating = Self::advance(&mut self.ai_chips, chip_hover, chip_len)
            | Self::advance(&mut self.ai_tabs, tab_hover, tab_len)
            | Self::advance(&mut self.ai_btns, btn_hover, 3);
        let target = if input_focused { 1.0 } else { 0.0 };
        if (self.ai_focus - target).abs() > f32::EPSILON {
            self.ai_focus = if self.ai_focus < target {
                (self.ai_focus + STEP).min(target)
            } else {
                (self.ai_focus - STEP).max(target)
            };
            animating |= (self.ai_focus - target).abs() > f32::EPSILON;
        }
        animating
    }

    /// 欢迎卡第 i 个 chip 的当前进度（越界返回 0）
    pub fn ai_chip_progress(&self, i: usize) -> f32 {
        self.ai_chips.get(i).copied().unwrap_or(0.0)
    }

    /// 侧栏第 i 个对话项的当前进度（越界返回 0）
    pub fn ai_tab_progress(&self, i: usize) -> f32 {
        self.ai_tabs.get(i).copied().unwrap_or(0.0)
    }

    /// AI 面板第 i 个按钮的当前进度（越界返回 0）
    pub fn ai_btn_progress(&self, i: usize) -> f32 {
        self.ai_btns.get(i).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_fades_in_hovered_item() {
        let mut anim = HoverAnim::new();
        // 首帧启动动画
        assert!(anim.tick(Some(1), 3, None, 0));
        assert!(anim.activity_progress(1) > 0.0);
        assert_eq!(anim.activity_progress(0), 0.0);
        // 持续 tick 直到完成
        for _ in 0..20 {
            if !anim.tick(Some(1), 3, None, 0) {
                break;
            }
        }
        assert_eq!(anim.activity_progress(1), 1.0);
        // 完成后不再报告动画在途
        assert!(!anim.tick(Some(1), 3, None, 0));
    }

    #[test]
    fn test_tick_fades_out_after_hover_leaves() {
        let mut anim = HoverAnim::new();
        for _ in 0..20 {
            anim.tick(Some(0), 2, None, 0);
        }
        assert_eq!(anim.activity_progress(0), 1.0);
        // 悬停移走后应淡出
        assert!(anim.tick(None, 2, None, 0));
        for _ in 0..20 {
            if !anim.tick(None, 2, None, 0) {
                break;
            }
        }
        assert_eq!(anim.activity_progress(0), 0.0);
    }

    #[test]
    fn test_tick_resizes_with_item_count() {
        let mut anim = HoverAnim::new();
        anim.tick(Some(0), 2, Some(1), 3);
        assert_eq!(anim.activity.len(), 2);
        assert_eq!(anim.tabs.len(), 3);
        // 关闭标签后进度表收缩
        anim.tick(Some(0), 2, None, 1);
        assert_eq!(anim.tabs.len(), 1);
    }

    #[test]
    fn test_quantize_bounds() {
        assert_eq!(quantize(0.0), 0.0);
        assert_eq!(quantize(1.0), 1.0);
        assert_eq!(quantize(0.5), 0.5);
        assert_eq!(quantize(-0.3), 0.0);
        assert_eq!(quantize(1.7), 1.0);
        // 量化结果必为 1/8 的整数倍
        let q = quantize(0.37);
        assert!((q * QUANTUM - (q * QUANTUM).round()).abs() < f32::EPSILON);
    }

    #[test]
    fn test_progress_out_of_range_is_zero() {
        let anim = HoverAnim::new();
        assert_eq!(anim.activity_progress(99), 0.0);
        assert_eq!(anim.tab_progress(99), 0.0);
    }

    #[test]
    fn test_tick_ai_channels_independent() {
        let mut anim = HoverAnim::new();
        // chip 1 悬停 + 对话项 0 悬停 + 发送按钮(2) 悬停 + 聚焦：四通道并行
        assert!(anim.tick_ai(Some(1), 4, Some(0), 2, Some(2), true));
        for _ in 0..20 {
            if !anim.tick_ai(Some(1), 4, Some(0), 2, Some(2), true) {
                break;
            }
        }
        assert_eq!(anim.ai_chip_progress(1), 1.0);
        assert_eq!(anim.ai_chip_progress(0), 0.0);
        assert_eq!(anim.ai_tab_progress(0), 1.0);
        assert_eq!(anim.ai_btn_progress(2), 1.0);
        assert_eq!(anim.ai_focus, 1.0);
        // 全部移走/失焦后淡出归零
        for _ in 0..20 {
            if !anim.tick_ai(None, 4, None, 2, None, false) {
                break;
            }
        }
        assert_eq!(anim.ai_chip_progress(1), 0.0);
        assert_eq!(anim.ai_tab_progress(0), 0.0);
        assert_eq!(anim.ai_btn_progress(2), 0.0);
        assert_eq!(anim.ai_focus, 0.0);
        assert!(!anim.tick_ai(None, 4, None, 2, None, false));
    }

    #[test]
    fn test_tick_ai_focus_independent_of_btn_hover() {
        let mut anim = HoverAnim::new();
        // 聚焦环与按钮悬停互不干扰：失焦但悬停发送时聚焦环仍淡出
        for _ in 0..20 {
            anim.tick_ai(None, 0, None, 0, Some(2), true);
        }
        assert_eq!(anim.ai_focus, 1.0);
        for _ in 0..20 {
            if !anim.tick_ai(None, 0, None, 0, Some(2), false) {
                break;
            }
        }
        assert_eq!(anim.ai_focus, 0.0);
        assert_eq!(anim.ai_btn_progress(2), 1.0);
    }
}
