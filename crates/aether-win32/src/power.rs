//! 空闲内存优化：Frozen 冰冻态管理
//!
//! 两态模型（Active / Frozen）：
//! - 窗口最小化（WM_SIZE + SIZE_MINIMIZED）→ 立即进入 Frozen
//! - 无输入超过 [`IDLE_FREEZE_SECS`] 且窗口失焦 → 冰冻看门狗定时器进入 Frozen
//! - 任意输入 / 获焦 / 从最小化恢复 → 退出 Frozen
//!
//! Frozen 期间的内存回收：关停 LSP 子进程（rust-analyzer 是最大内存消耗者）、
//! 裁剪全部标签页渲染缓存、释放 D2D 渲染资源、收缩 SQLite 页缓存、裁剪工作集。
//! AI 生成与 Agent 终端命令回环通过无头泵（`pump_background_tasks`，由 AI 定时器
//! 以 [`HEADLESS_PUMP_MS`] 间隔驱动）保活，只消费后台结果不触发重绘。

use std::time::Instant;

use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::editor::EditorState;

/// 空闲冰冻阈值（秒）：无输入且窗口失焦超过该时长进入 Frozen
pub const IDLE_FREEZE_SECS: u64 = 600;
/// 冰冻看门狗检查间隔（毫秒）
pub const POWER_CHECK_MS: u32 = 30_000;
/// 冰冻态无头泵间隔（毫秒）：只消费 AI 流/Agent 结果，不重绘
pub const HEADLESS_PUMP_MS: u32 = 250;

/// 电源状态管理器（每窗口一份，存于 EditorState）
pub struct PowerManager {
    /// 是否处于冰冻态
    pub frozen: bool,
    /// 窗口是否最小化（由 WM_SIZE 维护）
    pub minimized: bool,
    /// 最近一次用户输入时间
    last_input_time: Instant,
}

impl PowerManager {
    pub fn new() -> Self {
        Self {
            frozen: false,
            minimized: false,
            last_input_time: Instant::now(),
        }
    }

    /// 用户输入（鼠标/键盘/IME）时刷新空闲计时
    pub fn note_input(&mut self) {
        self.last_input_time = Instant::now();
    }

    /// 当前空闲时长（秒）
    pub fn idle_secs(&self) -> u64 {
        self.last_input_time.elapsed().as_secs()
    }
}

/// 通知 OS 裁剪进程工作集（冷页换出到 pagefile，物理内存占用立即下降）
pub fn trim_working_set() {
    unsafe {
        // (usize::MAX, usize::MAX) 即 (SIZE_T)-1：让 OS 自行回收可换出页
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// 内存遥测：输出当前进程工作集/峰值工作集/提交内存到日志
pub fn log_memory_usage(tag: &str) {
    unsafe {
        let mut pmc = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb).as_bool() {
            tracing::info!(
                tag,
                working_set_mb = pmc.WorkingSetSize as f64 / 1048576.0,
                peak_working_set_mb = pmc.PeakWorkingSetSize as f64 / 1048576.0,
                commit_mb = pmc.PagefileUsage as f64 / 1048576.0,
                "内存遥测"
            );
        }
    }
}

impl EditorState {
    /// 进入冰冻态：落盘 → 定时器管控 → 关停 LSP → 裁剪标签缓存 →
    /// 释放渲染资源 → 收缩 SQLite → 裁剪工作集
    pub fn enter_frozen(&mut self) {
        if self.power.frozen {
            return;
        }
        log_memory_usage("enter_frozen:before");

        // 1. 数据安全：先落盘（复用失焦自动保存路径）
        self.autosave_on_focus_loss();

        // 2. 定时器管控：停掉纯 UI 定时器（AUTOSAVE_*/AI_ARCHIVE 保留）；
        //    AI/Agent 活跃时由 AI 定时器转为无头泵保活（回调内检测 frozen）
        unsafe {
            let hwnd = self.hwnd;
            let _ = KillTimer(hwnd, crate::window::UI_ANIM_TIMER_ID);
            let _ = KillTimer(hwnd, crate::window::HIGHLIGHT_TIMER_ID);
            let _ = KillTimer(hwnd, crate::window::CARET_TIMER_ID);
            let _ = KillTimer(hwnd, crate::window::HOVER_TIMER_ID);
            let _ = KillTimer(hwnd, crate::window::LP_TIMER_ID);
            // 冰冻态终端刷新交由无头泵驱动，独立定时器停止
            let _ = KillTimer(hwnd, crate::window::TERM_TIMER_ID);
            if self.ai_panel.any_generating()
                || self.settings_panel.is_testing
                || self.terminal_panel.has_agent_activity()
            {
                SetTimer(hwnd, crate::window::AI_TIMER_ID, HEADLESS_PUMP_MS, None);
            } else {
                let _ = KillTimer(hwnd, crate::window::AI_TIMER_ID);
            }
        }

        // 3. 关停 LSP 子进程（rust-analyzer 可达 GB 级），首次编辑时延迟重启
        self.freeze_lsp();

        // 4. 裁剪全部标签页渲染缓存（PieceTable 本体保留）
        self.trim_all_tab_caches();

        // 5. 释放 D2D 渲染资源（唤醒后由渲染路径经设备丢失恢复逻辑自动重建）
        self.render_ctx.release_for_suspend();
        self.icons.clear();
        self.logo_bitmap = None;

        // 6. 收缩 SQLite 页缓存
        if let Some(warm) = self.ai_panel.warm_data_store.as_ref() {
            warm.shrink_memory();
        }

        self.power.frozen = true;

        // 7. 工作集裁剪放最后，让上面释放的页立即归还 OS
        trim_working_set();
        log_memory_usage("enter_frozen:after");
    }

    /// 退出冰冻态：全量重绘（渲染路径懒重建 D2D 资源与标签缓存）+ 按需恢复定时器。
    /// LSP 不在此处重启，延迟到用户首次编辑/打开文件（见 `thaw_lsp_on_demand`）。
    pub fn exit_frozen(&mut self) {
        if !self.power.frozen {
            return;
        }
        self.power.frozen = false;
        self.power.note_input();

        // 全量重绘：D2D 资源在 render() 内经设备丢失恢复路径重建，
        // 同时保证欢迎页三区域等不出现黑块残影
        self.dirty_tracker.mark_full_window();
        // tree-sitter 语言：强制重新请求后台高亮（裁剪过的 cached_tokens 需重建）。
        // 非 tree-sitter 语言不能置 0：永远不会发请求，高亮定时器将无限空转
        let rearm_highlight = self.needs_bg_highlight();
        if rearm_highlight {
            self.hl_request_version = 0;
        }

        unsafe {
            let hwnd = self.hwnd;
            if self.ai_panel.any_generating() || self.settings_panel.is_testing {
                SetTimer(
                    hwnd,
                    crate::window::AI_TIMER_ID,
                    crate::window::AI_REFRESH_MS,
                    None,
                );
            }
            if self.layout.bottom_panel_visible && self.terminal_panel.running {
                SetTimer(
                    hwnd,
                    crate::window::TERM_TIMER_ID,
                    crate::window::TERM_REFRESH_MS,
                    None,
                );
            }
            // 周期重绘直到后台高亮结果到达并着色（结果消费后自动停止）；
            // 仅 tree-sitter 语言需要，否则停止条件永不满足会空转
            if rearm_highlight {
                SetTimer(
                    hwnd,
                    crate::window::HIGHLIGHT_TIMER_ID,
                    crate::window::HIGHLIGHT_REFRESH_MS,
                    None,
                );
            }
        }
        crate::window::invalidate_window(self.hwnd);
        log_memory_usage("exit_frozen");
    }

    /// 裁剪所有标签页（含活跃标签）的渲染缓存，输出释放量遥测
    fn trim_all_tab_caches(&mut self) {
        let mut total = self.content.cache_bytes();
        self.content.trim_caches();
        for tab in &mut self.tab_bar.tabs {
            if let crate::tabs::Tab::File(content) = tab {
                total += content.cache_bytes();
                content.trim_caches();
            }
        }
        tracing::info!(trimmed_kb = total / 1024, "标签页渲染缓存已裁剪");
    }
}
