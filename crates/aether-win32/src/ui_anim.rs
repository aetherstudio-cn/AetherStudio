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
}
