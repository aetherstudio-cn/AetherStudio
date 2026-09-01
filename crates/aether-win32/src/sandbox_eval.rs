//! 智能体沙盒评测面板。
//!
//! 用户输入一个主题并设定**并发智能体数量**与模式（定时 / 不定时），任务清单由
//! AI 规划器自主决定，多个智能体在同一个专属临时沙盒目录内并发领取并完成任务，
//! 人类只需监控进度并打分：
//! - 所有文件操作被严格限制在沙盒目录内（拒绝绝对路径 / `..` 逃逸）；
//! - 终端命令一律拦截（智能体不能调用沙盒外的任何工具与命令）；
//! - 允许联网搜索（`<<<<<<< AETHER_SEARCH 查询词` 单行指令，每任务一次）；
//! - 定时模式下剩余 5 分钟时向智能体注入"你还有 5 分钟"提醒；
//! - 任务完成后由人工逐个打分（1-10），自动计算平均分；
//! - 结果（沙盒产物 + 评测报告 + results.json）可打包导出为 zip。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use aether_ai::{AiClient, AiStreamEvent, ChatMessage};
use aether_shared::settings::AiSettings;

use crate::ai_panel::AiStreamState;

/// 联网搜索单行指令前缀：`<<<<<<< AETHER_SEARCH <查询词>`
pub const SEARCH_PREFIX: &str = "<<<<<<< AETHER_SEARCH";
/// 任务数量上限（与规划器 AETHER_PLAN 上限一致；任务数由智能体自主决定）
pub const MAX_TASKS: usize = 20;
/// 并发智能体数量上限
pub const MAX_AGENTS: usize = 8;
/// 定时模式的"最后冲刺"提醒阈值（秒）
pub const WARN_THRESHOLD_SECS: u64 = 5 * 60;
/// 并发智能体数量快捷选项
pub const AGENT_PRESETS: [usize; 4] = [1, 2, 3, 5];
/// 定时模式时长快捷选项（分钟）
pub const DURATION_PRESETS: [u32; 4] = [10, 15, 30, 60];

/// 评测阶段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxPhase {
    /// 配置阶段：输入主题、任务数量、模式
    Setup,
    /// 规划阶段：AI 正在把主题拆解为任务清单
    Planning,
    /// 执行阶段：智能体逐个完成任务
    Running,
    /// 打分阶段：人工为每个任务打分
    Scoring,
}

/// 任务模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxMode {
    /// 不定时模式：任务不设截止时间
    Untimed,
    /// 定时模式：整轮评测限时完成
    Timed,
}

impl SandboxMode {
    pub fn label(&self) -> &'static str {
        match self {
            SandboxMode::Untimed => "不定时模式",
            SandboxMode::Timed => "定时模式",
        }
    }
}

/// 单个任务状态
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxTaskStatus {
    Pending,
    Running,
    /// 正在联网搜索并等待续写
    Searching,
    Done,
    Failed,
    /// 定时模式下超时未完成
    TimedOut,
}

impl SandboxTaskStatus {
    pub fn label(&self) -> &'static str {
        match self {
            SandboxTaskStatus::Pending => "等待中",
            SandboxTaskStatus::Running => "执行中",
            SandboxTaskStatus::Searching => "联网搜索中",
            SandboxTaskStatus::Done => "已完成",
            SandboxTaskStatus::Failed => "失败",
            SandboxTaskStatus::TimedOut => "超时",
        }
    }
}

/// 单个评测任务
#[derive(Clone, Debug)]
pub struct SandboxTask {
    /// 任务标题 / 要求（来自规划器）
    pub title: String,
    pub status: SandboxTaskStatus,
    /// 完成后展示的文字总结（回复中剥离文件块后的文本，截断）
    pub summary: String,
    /// 本任务在沙盒内产出的文件（相对路径）
    pub files: Vec<String>,
    /// 本任务使用过的搜索查询
    pub searches: Vec<String>,
    /// 人工评分（1-10）
    pub score: Option<u8>,
    /// 任务耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否已用掉本任务唯一一次搜索机会
    pub search_used: bool,
    /// 执行该任务的智能体编号（1 起；None = 未领取）
    pub agent: Option<usize>,
    /// 任务开始时刻（领取时记录，用于结算耗时）
    pub started: Option<Instant>,
}

impl SandboxTask {
    pub fn new(title: String) -> Self {
        Self {
            title,
            status: SandboxTaskStatus::Pending,
            summary: String::new(),
            files: Vec::new(),
            searches: Vec::new(),
            score: None,
            elapsed_ms: 0,
            search_used: false,
            agent: None,
            started: None,
        }
    }

    /// 任务是否已结束（无论成败），结束后才允许打分
    pub fn is_finished(&self) -> bool {
        matches!(
            self.status,
            SandboxTaskStatus::Done | SandboxTaskStatus::Failed | SandboxTaskStatus::TimedOut
        )
    }
}

/// 可编辑字段
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxField {
    /// 主题输入框
    Topic,
    /// 自定义任务数量输入框
    CustomCount,
}

/// 执行日志条目类型（决定渲染颜色）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxLogKind {
    Info,
    File,
    Search,
    Warn,
    Error,
}

/// 执行日志条目
#[derive(Clone, Debug)]
pub struct SandboxLogEntry {
    /// 相对评测开始的 "mm:ss"
    pub time: String,
    pub kind: SandboxLogKind,
    pub text: String,
}

/// 单个并发智能体（worker）的运行状态。
/// 每个智能体拥有独立的流式通道与对话上下文，从共享任务队列领取任务。
pub struct AgentWorker {
    /// 智能体编号（从 1 开始，用于展示与归属记录）
    pub id: usize,
    /// 当前领取的任务下标（None = 空闲）
    pub task: Option<usize>,
    /// 是否有流式请求进行中
    pub generating: bool,
    pub stream_state: Arc<Mutex<AiStreamState>>,
    /// 当前任务的多轮对话上下文（系统提示 + 任务要求 + 搜索续写）
    conversation: Vec<ChatMessage>,
    /// 当前流式响应的累计文本
    current_response: String,
    /// 流式增量的尾部预览（UI 实时显示）
    pub live_tail: String,
    /// 是否收到过思考增量（预览提示用）
    pub live_thinking: bool,
}

impl AgentWorker {
    fn new(id: usize) -> Self {
        Self {
            id,
            task: None,
            generating: false,
            stream_state: Arc::new(Mutex::new(AiStreamState::default())),
            conversation: Vec::new(),
            current_response: String::new(),
            live_tail: String::new(),
            live_thinking: false,
        }
    }

    /// 空闲：无任务且无进行中的请求（可领取新任务）
    pub fn is_idle(&self) -> bool {
        self.task.is_none() && !self.generating
    }
}

/// 命中区域 (x, y, w, h)
pub type HitRect = (f32, f32, f32, f32);

pub fn rect_hit(r: &HitRect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

/// 渲染阶段回填的全部命中区域，供鼠标点击路由使用
#[derive(Clone, Debug, Default)]
pub struct SandboxRegions {
    pub topic_field: Option<HitRect>,
    pub custom_count_field: Option<HitRect>,
    /// (并发智能体数量, 区域)
    pub agent_chips: Vec<(usize, HitRect)>,
    /// (模式下标 0=不定时 1=定时, 区域)
    pub mode_chips: Vec<(usize, HitRect)>,
    /// (分钟数, 区域)
    pub duration_chips: Vec<(u32, HitRect)>,
    pub start_button: Option<HitRect>,
    pub stop_button: Option<HitRect>,
    /// (任务下标, 分数 1-10, 区域)
    pub score_chips: Vec<(usize, u8, HitRect)>,
    pub export_button: Option<HitRect>,
    pub restart_button: Option<HitRect>,
    pub open_dir_button: Option<HitRect>,
}

impl SandboxRegions {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// 智能体沙盒评测面板状态
pub struct SandboxEvalPanel {
    // ===== 配置输入 =====
    pub topic: String,
    /// 并发智能体数量（快捷选项之一；自定义输入非空时以其为准）
    pub agent_count: usize,
    /// 自定义并发智能体数量输入（1-8）
    pub custom_count: String,
    pub mode: SandboxMode,
    pub duration_min: u32,
    pub active_field: Option<SandboxField>,
    pub caret_visible: bool,
    /// 配置校验 / 运行错误提示
    pub error: Option<String>,
    /// 指定评测的模型 ID（从模型管理页"能力评测"按钮带入；None = 用全局激活模型）
    pub target_model_id: Option<String>,
    /// 指定评测的模型显示名称（仅供 UI 展示）
    pub target_model_name: Option<String>,

    // ===== 运行时 =====
    pub phase: SandboxPhase,
    pub tasks: Vec<SandboxTask>,
    /// 并发智能体（Running 阶段活跃；规划阶段为空）
    pub workers: Vec<AgentWorker>,
    /// 沙盒运行根目录（含 workspace / 报告）
    pub run_dir: Option<PathBuf>,
    /// 智能体唯一可写的工作目录（run_dir/workspace）
    pub sandbox_dir: Option<PathBuf>,
    /// 规划阶段流式请求进行中（Running 阶段以各 worker 的 generating 为准）
    pub is_generating: bool,
    /// 规划阶段的流式通道
    pub stream_state: Arc<Mutex<AiStreamState>>,
    /// 全局停止信号（终止/超时时通知所有智能体线程）
    pub should_stop: Arc<AtomicBool>,
    /// 整轮评测开始时刻
    pub started_at: Option<Instant>,
    /// 定时模式截止时刻
    pub deadline: Option<Instant>,
    /// 是否已发出"剩余 5 分钟"提醒（此后派发的任务提示词都会附带该提醒）
    pub five_min_warned: bool,
    /// 规划阶段的流式响应累计文本
    current_response: String,
    /// 规划阶段流式增量的尾部预览（UI 实时显示）
    pub live_tail: String,
    /// 规划阶段是否收到过思考增量（预览提示用）
    pub live_thinking: bool,
    pub log: Vec<SandboxLogEntry>,
    /// 整轮评测总耗时（进入打分阶段时结算，毫秒）
    pub total_elapsed_ms: u64,

    // ===== 打分 / 导出 =====
    pub export_message: Option<String>,

    // ===== 渲染回填 =====
    pub scroll_y: f32,
    pub content_height: f32,
    pub view_height: f32,
    pub regions: SandboxRegions,
}

impl SandboxEvalPanel {
    pub fn new() -> Self {
        Self {
            topic: String::new(),
            agent_count: 2,
            custom_count: String::new(),
            mode: SandboxMode::Untimed,
            duration_min: 30,
            active_field: None,
            caret_visible: false,
            error: None,
            target_model_id: None,
            target_model_name: None,
            phase: SandboxPhase::Setup,
            tasks: Vec::new(),
            workers: Vec::new(),
            run_dir: None,
            sandbox_dir: None,
            is_generating: false,
            stream_state: Arc::new(Mutex::new(AiStreamState::default())),
            should_stop: Arc::new(AtomicBool::new(false)),
            started_at: None,
            deadline: None,
            five_min_warned: false,
            current_response: String::new(),
            live_tail: String::new(),
            live_thinking: false,
            log: Vec::new(),
            total_elapsed_ms: 0,
            export_message: None,
            scroll_y: 0.0,
            content_height: 0.0,
            view_height: 0.0,
            regions: SandboxRegions::default(),
        }
    }

    // ===================== 输入处理 =====================

    pub fn input_char(&mut self, c: char) {
        match self.active_field {
            Some(SandboxField::Topic) => {
                if self.topic.chars().count() < 500 {
                    self.topic.push(c);
                }
            }
            Some(SandboxField::CustomCount)
                if c.is_ascii_digit() && self.custom_count.len() < 2 =>
            {
                self.custom_count.push(c);
            }
            _ => {}
        }
        self.error = None;
    }

    pub fn backspace(&mut self) {
        match self.active_field {
            Some(SandboxField::Topic) => {
                self.topic.pop();
            }
            Some(SandboxField::CustomCount) => {
                self.custom_count.pop();
            }
            None => {}
        }
    }

    pub fn paste_text(&mut self, text: &str) {
        match self.active_field {
            Some(SandboxField::Topic) => {
                for c in text.chars().filter(|c| *c != '\r' && *c != '\n') {
                    self.input_char(c);
                }
            }
            Some(SandboxField::CustomCount) => {
                for c in text.chars().filter(|c| c.is_ascii_digit()) {
                    self.input_char(c);
                }
            }
            None => {}
        }
    }

    /// 实际生效的并发智能体数量：自定义输入优先，钳制到 1..=MAX_AGENTS
    pub fn effective_agent_count(&self) -> usize {
        if let Ok(n) = self.custom_count.trim().parse::<usize>() {
            if n >= 1 {
                return n.min(MAX_AGENTS);
            }
        }
        self.agent_count.clamp(1, MAX_AGENTS)
    }

    /// 解析本次评测应使用的 AiSettings（target_model_id 优先，否则全局激活模型）。
    pub fn resolve_ai_settings(
        &self,
        app_settings: &aether_shared::settings::AppSettings,
    ) -> AiSettings {
        if let Some(id) = &self.target_model_id {
            if let Some(m) = app_settings.ai_models.iter().find(|m| &m.id == id) {
                return m.to_ai_settings();
            }
        }
        app_settings.active_ai_settings()
    }

    // ===================== 运行控制 =====================

    /// 开始评测：创建沙盒目录并发起规划请求。返回 Err 时停留在配置页。
    /// `app_settings` 用于解析目标模型的 AI 配置（target_model_id 优先，否则全局激活模型）。
    pub fn start(
        &mut self,
        app_settings: &aether_shared::settings::AppSettings,
    ) -> Result<(), String> {
        if self.topic.trim().is_empty() {
            return Err("请先输入评测主题".to_string());
        }
        let settings = self.resolve_ai_settings(app_settings);
        let agents = self.effective_agent_count();

        // 创建专属临时沙盒：%TEMP%\AetherSandbox\run-<时间戳>\workspace
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let run_dir = std::env::temp_dir()
            .join("AetherSandbox")
            .join(format!("run-{}", stamp));
        let sandbox_dir = run_dir.join("workspace");
        std::fs::create_dir_all(&sandbox_dir).map_err(|e| format!("创建沙盒目录失败: {}", e))?;

        self.tasks.clear();
        self.workers.clear();
        self.log.clear();
        self.error = None;
        self.export_message = None;
        self.five_min_warned = false;
        self.total_elapsed_ms = 0;
        self.scroll_y = 0.0;
        self.run_dir = Some(run_dir);
        self.sandbox_dir = Some(sandbox_dir.clone());
        self.started_at = Some(Instant::now());
        self.deadline = match self.mode {
            SandboxMode::Timed => {
                Some(Instant::now() + std::time::Duration::from_secs(self.duration_min as u64 * 60))
            }
            SandboxMode::Untimed => None,
        };
        self.should_stop = Arc::new(AtomicBool::new(false));
        self.active_field = None;
        self.phase = SandboxPhase::Planning;

        self.push_log(
            SandboxLogKind::Info,
            format!("已创建沙盒环境: {}", sandbox_dir.display()),
        );
        self.push_log(
            SandboxLogKind::Info,
            format!(
                "评测开始 · 主题「{}」· {} 个智能体并发 · {}",
                self.topic.trim(),
                agents,
                match self.mode {
                    SandboxMode::Timed => format!("定时 {} 分钟", self.duration_min),
                    SandboxMode::Untimed => "不限时".to_string(),
                },
            ),
        );

        // 规划请求：任务数量由规划器根据主题复杂度自主决定
        let system = format!(
            "你是「牧羊人编辑器」沙盒评测的任务规划器。\
             请把用户给出的主题拆解为一组相互独立、可以由 AI 智能体在沙盒内\
             通过创建文件完成的具体任务（如撰写文档、编写代码、整理资料等）。\
             任务数量由你根据主题复杂度自行决定（2 到 {} 个之间），\
             任务之间不得有先后依赖，因为它们会被 {} 个智能体并发执行。\
             每个任务独占一行，格式为：TASK 任务标题——具体要求。\
             输出必须且只能包裹在以下标记之间：\n\
             <<<<<<< AETHER_PLAN\nTASK ...\n>>>>>>> AETHER_END_PLAN\n\
             除标记块外不要输出任何其他内容。",
            MAX_TASKS, agents
        );
        let user = format!("评测主题：{}", self.topic.trim());
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
                images: Vec::new(),
            },
            ChatMessage::user(user),
        ];
        self.begin_stream(&settings, messages);
        Ok(())
    }

    /// 用户手动终止评测：已完成的任务保留可打分；一个都没完成则回到配置页。
    pub fn stop(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.is_generating = false;
        for w in self.workers.iter_mut() {
            w.generating = false;
            w.task = None;
        }
        for t in self.tasks.iter_mut() {
            if !t.is_finished() {
                t.status = SandboxTaskStatus::Failed;
                if t.summary.is_empty() {
                    t.summary = "评测被手动终止".to_string();
                }
            }
        }
        self.push_log(SandboxLogKind::Warn, "评测已手动终止".to_string());
        if self
            .tasks
            .iter()
            .any(|t| t.status == SandboxTaskStatus::Done)
        {
            self.enter_scoring();
        } else {
            self.phase = SandboxPhase::Setup;
        }
    }

    /// 重开一轮：保留主题与参数配置，清空运行状态
    pub fn reset_for_new_round(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.phase = SandboxPhase::Setup;
        self.tasks.clear();
        self.workers.clear();
        self.log.clear();
        self.run_dir = None;
        self.sandbox_dir = None;
        self.is_generating = false;
        self.started_at = None;
        self.deadline = None;
        self.five_min_warned = false;
        self.error = None;
        self.export_message = None;
        self.live_tail.clear();
        self.live_thinking = false;
        self.scroll_y = 0.0;
        self.total_elapsed_ms = 0;
    }

    /// 是否处于需要周期驱动（定时器）的活跃状态
    pub fn is_active(&self) -> bool {
        matches!(self.phase, SandboxPhase::Planning | SandboxPhase::Running)
    }

    /// 剩余秒数（仅定时模式），到期返回 0
    pub fn remaining_secs(&self) -> Option<u64> {
        let deadline = self.deadline?;
        let now = Instant::now();
        if now >= deadline {
            Some(0)
        } else {
            Some((deadline - now).as_secs())
        }
    }

    // ===================== 周期驱动 =====================

    /// 定时器每帧调用：消费流式增量、推进状态机、处理限时逻辑。
    /// 返回 true 表示状态有变化需要重绘。
    pub fn tick(&mut self, app_settings: &aether_shared::settings::AppSettings) -> bool {
        if !self.is_active() {
            return false;
        }
        // 提前解析一次（避免后续 &mut self 与 &self 冲突）
        let settings = self.resolve_ai_settings(app_settings);
        self.tick_with_settings(&settings)
    }

    fn tick_with_settings(&mut self, settings: &AiSettings) -> bool {
        let mut dirty = false;

        // ---- 限时检查（在消费流之前，保证超时立即生效）----
        if self.phase == SandboxPhase::Running {
            if let Some(remaining) = self.remaining_secs() {
                if remaining == 0 {
                    self.on_deadline_reached();
                    return true;
                }
                if remaining <= WARN_THRESHOLD_SECS && !self.five_min_warned {
                    self.five_min_warned = true;
                    self.push_log(
                        SandboxLogKind::Warn,
                        "已向所有智能体发出提醒：还有 5 分钟完成剩余任务".to_string(),
                    );
                }
                // 倒计时显示需要每秒刷新
                dirty = true;
            }
        }

        match self.phase {
            SandboxPhase::Planning => {
                if self.tick_planning(settings) {
                    dirty = true;
                }
            }
            SandboxPhase::Running => {
                for i in 0..self.workers.len() {
                    if self.drain_worker(i, settings) {
                        dirty = true;
                    }
                }
                if self.dispatch_tasks(settings) {
                    dirty = true;
                }
                // 全部任务结束且无智能体仍在生成 → 进入打分
                if self.phase == SandboxPhase::Running
                    && !self.tasks.is_empty()
                    && self.tasks.iter().all(|t| t.is_finished())
                    && self.workers.iter().all(|w| !w.generating)
                {
                    self.enter_scoring();
                    dirty = true;
                }
            }
            _ => {}
        }
        dirty
    }

    /// 规划阶段：消费规划器的流式增量
    fn tick_planning(&mut self, _settings: &AiSettings) -> bool {
        if !self.is_generating {
            return false;
        }
        let mut dirty = false;
        let (partial, reasoning, done, error) = drain_stream_state(&self.stream_state);
        if !reasoning.is_empty() {
            self.live_thinking = true;
            dirty = true;
        }
        if !partial.is_empty() {
            self.live_thinking = false;
            self.current_response.push_str(&partial);
            self.live_tail.push_str(&partial);
            trim_tail(&mut self.live_tail);
            dirty = true;
        }
        if let Some(err) = error {
            self.is_generating = false;
            self.push_log(SandboxLogKind::Error, format!("任务规划失败: {}", err));
            self.error = Some(format!("任务规划失败: {}", err));
            self.phase = SandboxPhase::Setup;
            return true;
        }
        if done {
            self.is_generating = false;
            let response = std::mem::take(&mut self.current_response);
            self.live_tail.clear();
            self.live_thinking = false;
            self.on_plan_response(&response);
            return true;
        }
        dirty
    }

    /// 消费单个智能体的流式增量；响应完成时处理搜索续写 / 任务收尾
    fn drain_worker(&mut self, wi: usize, settings: &AiSettings) -> bool {
        if !self.workers.get(wi).map(|w| w.generating).unwrap_or(false) {
            return false;
        }
        let mut dirty = false;
        let (partial, reasoning, done, error) = drain_stream_state(&self.workers[wi].stream_state);
        if !reasoning.is_empty() {
            self.workers[wi].live_thinking = true;
            dirty = true;
        }
        if !partial.is_empty() {
            let task_idx = self.workers[wi].task;
            let w = &mut self.workers[wi];
            w.live_thinking = false;
            w.current_response.push_str(&partial);
            w.live_tail.push_str(&partial);
            trim_tail(&mut w.live_tail);
            // 联网搜索中收到正文说明搜索续写已开始
            if let Some(ti) = task_idx {
                if let Some(t) = self.tasks.get_mut(ti) {
                    if t.status == SandboxTaskStatus::Searching {
                        t.status = SandboxTaskStatus::Running;
                    }
                }
            }
            dirty = true;
        }
        if let Some(err) = error {
            self.workers[wi].generating = false;
            self.on_worker_error(wi, err);
            return true;
        }
        if done {
            self.workers[wi].generating = false;
            let response = std::mem::take(&mut self.workers[wi].current_response);
            self.workers[wi].live_tail.clear();
            self.workers[wi].live_thinking = false;
            self.on_task_response(wi, &response, settings);
            return true;
        }
        dirty
    }

    /// 把待执行任务派发给空闲智能体（每次 tick 调用，天然避免响应处理路径重入）
    fn dispatch_tasks(&mut self, settings: &AiSettings) -> bool {
        if self.phase != SandboxPhase::Running {
            return false;
        }
        let mut dirty = false;
        for wi in 0..self.workers.len() {
            if !self.workers[wi].is_idle() {
                continue;
            }
            let Some(ti) = self
                .tasks
                .iter()
                .position(|t| t.status == SandboxTaskStatus::Pending)
            else {
                break;
            };
            self.assign_task(wi, ti, settings);
            dirty = true;
        }
        dirty
    }

    /// 限时到期：终止全部生成，标记未完成任务为超时，进入打分阶段
    fn on_deadline_reached(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.is_generating = false;
        for w in self.workers.iter_mut() {
            w.generating = false;
            w.task = None;
        }
        for t in self.tasks.iter_mut() {
            if !t.is_finished() {
                t.status = SandboxTaskStatus::TimedOut;
                if t.summary.is_empty() {
                    t.summary = "时间耗尽，任务未完成".to_string();
                }
            }
        }
        self.push_log(
            SandboxLogKind::Warn,
            "评测时间已用尽，剩余任务标记为超时".to_string(),
        );
        self.enter_scoring();
    }

    /// 智能体执行出错：抢救已接收文件块，标记任务失败，智能体转为空闲
    fn on_worker_error(&mut self, wi: usize, err: String) {
        let agent_id = self.workers.get(wi).map(|w| w.id).unwrap_or(0);
        let Some(task_idx) = self.workers.get(wi).and_then(|w| w.task) else {
            return;
        };
        self.push_log(
            SandboxLogKind::Error,
            format!(
                "智能体 #{} 执行任务 {} 出错: {}",
                agent_id,
                task_idx + 1,
                err
            ),
        );
        let salvaged = std::mem::take(&mut self.workers[wi].current_response);
        if !salvaged.is_empty() {
            self.apply_response_files(task_idx, &salvaged);
        }
        if let Some(t) = self.tasks.get_mut(task_idx) {
            t.status = SandboxTaskStatus::Failed;
            t.summary = format!("执行出错: {}", err);
            if let Some(start) = t.started {
                t.elapsed_ms = start.elapsed().as_millis() as u64;
            }
        }
        self.workers[wi].task = None;
    }

    /// 规划完成：解析任务清单、创建并发智能体（任务派发由下一次 tick 完成）
    fn on_plan_response(&mut self, response: &str) {
        let mut titles = parse_sandbox_plan(response);
        titles.truncate(MAX_TASKS);
        if titles.is_empty() {
            self.push_log(
                SandboxLogKind::Error,
                "规划器未返回有效任务清单".to_string(),
            );
            self.error = Some("任务规划失败：模型未返回有效任务清单，请重试".to_string());
            self.phase = SandboxPhase::Setup;
            return;
        }
        for t in &titles {
            self.tasks.push(SandboxTask::new(t.clone()));
        }
        let agents = self.effective_agent_count();
        self.workers = (1..=agents).map(AgentWorker::new).collect();
        self.push_log(
            SandboxLogKind::Info,
            format!(
                "任务规划完成：智能体自主拆解出 {} 个任务，{} 个智能体开始并发执行",
                self.tasks.len(),
                agents
            ),
        );
        self.phase = SandboxPhase::Running;
    }

    /// 智能体的任务响应到达：若包含搜索指令则执行搜索续写，否则落盘收尾并转为空闲
    fn on_task_response(&mut self, wi: usize, response: &str, settings: &AiSettings) {
        let agent_id = self.workers.get(wi).map(|w| w.id).unwrap_or(0);
        let Some(task_idx) = self.workers.get(wi).and_then(|w| w.task) else {
            return;
        };
        let search_allowed = self
            .tasks
            .get(task_idx)
            .map(|t| !t.search_used)
            .unwrap_or(false);
        if search_allowed {
            if let Some(query) = parse_search_query(response) {
                if let Some(t) = self.tasks.get_mut(task_idx) {
                    t.search_used = true;
                    t.searches.push(query.clone());
                    t.status = SandboxTaskStatus::Searching;
                }
                self.push_log(
                    SandboxLogKind::Search,
                    format!("智能体 #{} 联网搜索: {}", agent_id, query),
                );
                // 组装续写上下文：先前对话 + 本轮回复 + 搜索结果（线程内获取）
                self.workers[wi]
                    .conversation
                    .push(ChatMessage::assistant(response));
                let messages = self.workers[wi].conversation.clone();
                self.begin_worker_search_stream(wi, settings, messages, query);
                return;
            }
        }
        // 最终响应：应用文件块、拦截命令、记录总结
        self.apply_response_files(task_idx, response);
        let blocked = crate::ai_panel::parse_run_commands(response);
        for cmd in &blocked {
            self.push_log(
                SandboxLogKind::Warn,
                format!(
                    "已拦截智能体 #{} 的沙盒外命令: {}",
                    agent_id,
                    truncate_chars(cmd, 60)
                ),
            );
        }
        if let Some(t) = self.tasks.get_mut(task_idx) {
            t.status = SandboxTaskStatus::Done;
            t.summary = extract_summary(response);
            if let Some(start) = t.started {
                t.elapsed_ms = start.elapsed().as_millis() as u64;
            }
        }
        let file_count = self.tasks.get(task_idx).map(|t| t.files.len()).unwrap_or(0);
        self.push_log(
            SandboxLogKind::Info,
            format!(
                "智能体 #{} 完成任务 {}/{}，产出 {} 个文件",
                agent_id,
                task_idx + 1,
                self.tasks.len(),
                file_count
            ),
        );
        // 转为空闲，下一次 tick 的 dispatch_tasks 会自动领取新任务
        self.workers[wi].task = None;
    }

    /// 结算并进入打分阶段
    fn enter_scoring(&mut self) {
        if let Some(start) = self.started_at {
            self.total_elapsed_ms = start.elapsed().as_millis() as u64;
        }
        self.phase = SandboxPhase::Scoring;
        self.push_log(
            SandboxLogKind::Info,
            "全部任务执行结束，请为每个任务打分（1-10）".to_string(),
        );
    }

    /// 把第 task_idx 个任务派发给第 wi 个智能体并发起请求
    fn assign_task(&mut self, wi: usize, task_idx: usize, settings: &AiSettings) {
        let total = self.tasks.len();
        let agent_id = self.workers.get(wi).map(|w| w.id).unwrap_or(0);
        let Some(task) = self.tasks.get_mut(task_idx) else {
            return;
        };
        task.status = SandboxTaskStatus::Running;
        task.agent = Some(agent_id);
        task.started = Some(Instant::now());
        let title = task.title.clone();
        self.push_log(
            SandboxLogKind::Info,
            format!(
                "智能体 #{} 领取任务 {}/{}: {}",
                agent_id,
                task_idx + 1,
                total,
                truncate_chars(&title, 60)
            ),
        );

        let system = format!(
            "你是运行在「牧羊人编辑器」隔离沙盒中的评测智能体 #{}（共 {} 个智能体并发协作，\
             各自独立完成领取的任务）。必须遵守沙盒规则：\n\
             1. 你们共享一个临时沙盒工作目录，所有文件操作只能发生在该目录内，一律使用相对路径；\
             为避免与其他智能体冲突，产出文件请放入子目录 task{:02}/ 下；\n\
             2. 创建或修改文件必须使用如下标记（每个标记独占整行）：\n\
             <<<<<<< AETHER_FILE 相对路径\n\
             ======= AETHER_SEP\n\
             文件完整内容\n\
             >>>>>>> AETHER_END_FILE\n\
             3. 禁止执行任何终端命令、禁止访问沙盒外的路径或调用沙盒外的工具，此类请求会被系统拦截；\n\
             4. 如需最新网络资料，可联网搜索一次：单独一行输出 `<<<<<<< AETHER_SEARCH 查询词`，\
             然后停止输出等待搜索结果，收到结果后再继续完成任务；\n\
             5. 回复时先用两三句话简述完成思路，然后输出成果文件块。",
            agent_id,
            self.workers.len(),
            task_idx + 1
        );

        let mut time_note = String::new();
        if let Some(remaining) = self.remaining_secs() {
            time_note = format!(
                "\n本轮评测为定时模式，剩余时间约 {} 分 {:02} 秒。",
                remaining / 60,
                remaining % 60
            );
        }
        if self.five_min_warned {
            time_note.push_str(
                "\n⚠️ 时间提醒：你还有 5 分钟来完成以下任务，请优先产出核心成果并尽快收尾。",
            );
        }
        let user = format!(
            "评测主题：{}\n当前任务（第 {}/{} 个）：{}{}\n请在沙盒内完成该任务并输出成果文件。",
            self.topic.trim(),
            task_idx + 1,
            total,
            title,
            time_note
        );
        self.workers[wi].conversation = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
                images: Vec::new(),
            },
            ChatMessage::user(user),
        ];
        self.workers[wi].task = Some(task_idx);
        let messages = self.workers[wi].conversation.clone();
        self.begin_worker_stream(wi, settings, messages);
    }

    /// 解析响应中的文件块并落盘到沙盒（严格路径校验），归属到指定任务
    fn apply_response_files(&mut self, task_idx: usize, response: &str) {
        let Some(sandbox) = self.sandbox_dir.clone() else {
            return;
        };
        let edits = crate::ai_panel::parse_edits(response, None);
        let mut salvage = Vec::new();
        if let Some(partial) = crate::ai_panel::parse_trailing_create_block(response) {
            salvage.push(partial);
        }
        for edit in edits.into_iter().chain(salvage) {
            let rel = edit.path.to_string_lossy().replace('\\', "/");
            match safe_sandbox_path(&sandbox, &rel) {
                Ok(abs) => {
                    if edit.is_delete() {
                        let _ = std::fs::remove_file(&abs);
                        self.push_log(SandboxLogKind::File, format!("删除文件: {}", rel));
                        continue;
                    }
                    if let Some(parent) = abs.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let content = if edit.is_create_new() {
                        edit.replace.clone()
                    } else {
                        // 修改语义：沙盒内做简单 search/replace，找不到则整文件覆盖
                        match std::fs::read_to_string(&abs) {
                            Ok(old)
                                if !edit.search.trim().is_empty() && old.contains(&edit.search) =>
                            {
                                old.replacen(&edit.search, &edit.replace, 1)
                            }
                            _ => edit.replace.clone(),
                        }
                    };
                    match std::fs::write(&abs, content) {
                        Ok(()) => {
                            self.push_log(SandboxLogKind::File, format!("写入文件: {}", rel));
                            if let Some(t) = self.tasks.get_mut(task_idx) {
                                if !t.files.contains(&rel) {
                                    t.files.push(rel);
                                }
                            }
                        }
                        Err(e) => {
                            self.push_log(
                                SandboxLogKind::Error,
                                format!("写入失败 {}: {}", rel, e),
                            );
                        }
                    }
                }
                Err(reason) => {
                    self.push_log(
                        SandboxLogKind::Warn,
                        format!("已拦截沙盒外文件操作 {}（{}）", rel, reason),
                    );
                }
            }
        }
    }

    /// 启动规划阶段的流式请求（面板级流；不重建全局停止信号）
    fn begin_stream(&mut self, settings: &AiSettings, messages: Vec<ChatMessage>) {
        self.stream_state = Arc::new(Mutex::new(AiStreamState::default()));
        self.current_response.clear();
        self.live_tail.clear();
        self.live_thinking = false;
        self.is_generating = true;
        spawn_sandbox_stream(
            settings.clone(),
            messages,
            self.stream_state.clone(),
            self.should_stop.clone(),
        );
    }

    /// 为指定智能体启动一次流式请求
    fn begin_worker_stream(
        &mut self,
        wi: usize,
        settings: &AiSettings,
        messages: Vec<ChatMessage>,
    ) {
        let should_stop = self.should_stop.clone();
        let Some(w) = self.workers.get_mut(wi) else {
            return;
        };
        w.stream_state = Arc::new(Mutex::new(AiStreamState::default()));
        w.current_response.clear();
        w.live_tail.clear();
        w.live_thinking = false;
        w.generating = true;
        spawn_sandbox_stream(
            settings.clone(),
            messages,
            w.stream_state.clone(),
            should_stop,
        );
    }

    /// 为指定智能体启动"先搜索、后续写"的后台流程
    fn begin_worker_search_stream(
        &mut self,
        wi: usize,
        settings: &AiSettings,
        messages: Vec<ChatMessage>,
        query: String,
    ) {
        let should_stop = self.should_stop.clone();
        let Some(w) = self.workers.get_mut(wi) else {
            return;
        };
        w.stream_state = Arc::new(Mutex::new(AiStreamState::default()));
        w.current_response.clear();
        w.live_tail.clear();
        w.live_thinking = false;
        w.generating = true;
        spawn_search_then_stream(
            settings.clone(),
            messages,
            query,
            w.stream_state.clone(),
            should_stop,
        );
    }

    // ===================== 打分 =====================

    pub fn set_score(&mut self, task_idx: usize, score: u8) {
        if let Some(t) = self.tasks.get_mut(task_idx) {
            if t.is_finished() {
                t.score = Some(score.clamp(1, 10));
            }
        }
    }

    /// 已打分任务的平均分
    pub fn average_score(&self) -> Option<f32> {
        let scores: Vec<u8> = self.tasks.iter().filter_map(|t| t.score).collect();
        if scores.is_empty() {
            return None;
        }
        Some(scores.iter().map(|s| *s as f32).sum::<f32>() / scores.len() as f32)
    }

    pub fn all_scored(&self) -> bool {
        !self.tasks.is_empty() && self.tasks.iter().all(|t| t.score.is_some())
    }

    // ===================== 日志 =====================

    fn push_log(&mut self, kind: SandboxLogKind, text: String) {
        let time = match self.started_at {
            Some(start) => {
                let secs = start.elapsed().as_secs();
                format!("{:02}:{:02}", secs / 60, secs % 60)
            }
            None => "00:00".to_string(),
        };
        self.log.push(SandboxLogEntry { time, kind, text });
        if self.log.len() > 500 {
            self.log.drain(0..self.log.len() - 500);
        }
    }

    // ===================== 导出 =====================

    /// 生成评测报告与 results.json 并把整个运行目录打包为 zip
    pub fn export(&self, dest: &Path) -> Result<String, String> {
        let run_dir = self
            .run_dir
            .as_ref()
            .ok_or_else(|| "没有可导出的评测结果".to_string())?;

        // 1. 评测报告（Markdown）
        let report = self.build_report_markdown();
        std::fs::write(run_dir.join("评测报告.md"), report)
            .map_err(|e| format!("写入评测报告失败: {}", e))?;

        // 2. results.json（结构化结果）
        let json = self.build_results_json();
        std::fs::write(
            run_dir.join("results.json"),
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string()),
        )
        .map_err(|e| format!("写入 results.json 失败: {}", e))?;

        // 3. zip 打包
        let count = zip_dir(run_dir, dest)?;
        Ok(format!("已导出 {} 个文件到 {}", count, dest.display()))
    }

    fn build_report_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# 智能体沙盒评测报告\n\n");
        md.push_str(&format!("- 评测主题：{}\n", self.topic.trim()));
        md.push_str(&format!("- 任务模式：{}\n", self.mode.label()));
        if self.mode == SandboxMode::Timed {
            md.push_str(&format!("- 时间限制：{} 分钟\n", self.duration_min));
        }
        md.push_str(&format!("- 并发智能体数量：{}\n", self.workers.len()));
        md.push_str(&format!(
            "- 任务数量（智能体自主规划）：{}\n",
            self.tasks.len()
        ));
        let secs = self.total_elapsed_ms / 1000;
        md.push_str(&format!("- 总耗时：{} 分 {:02} 秒\n", secs / 60, secs % 60));
        match self.average_score() {
            Some(avg) => md.push_str(&format!("- **平均得分：{:.1} / 10**\n", avg)),
            None => md.push_str("- 平均得分：未打分\n"),
        }
        md.push_str("\n## 任务明细\n\n");
        for (i, t) in self.tasks.iter().enumerate() {
            md.push_str(&format!("### 任务 {}：{}\n\n", i + 1, t.title));
            md.push_str(&format!("- 状态：{}\n", t.status.label()));
            if let Some(agent) = t.agent {
                md.push_str(&format!("- 执行智能体：#{}\n", agent));
            }
            md.push_str(&format!(
                "- 得分：{}\n",
                t.score
                    .map(|s| format!("{} / 10", s))
                    .unwrap_or_else(|| "未打分".to_string())
            ));
            md.push_str(&format!("- 耗时：{:.1} 秒\n", t.elapsed_ms as f64 / 1000.0));
            if !t.files.is_empty() {
                md.push_str("- 产出文件：\n");
                for f in &t.files {
                    md.push_str(&format!("  - `workspace/{}`\n", f));
                }
            }
            if !t.searches.is_empty() {
                md.push_str("- 联网搜索：\n");
                for s in &t.searches {
                    md.push_str(&format!("  - {}\n", s));
                }
            }
            if !t.summary.is_empty() {
                md.push_str(&format!("- 智能体总结：{}\n", t.summary));
            }
            md.push('\n');
        }
        md.push_str("## 执行日志\n\n");
        for entry in &self.log {
            md.push_str(&format!("- [{}] {}\n", entry.time, entry.text));
        }
        md
    }

    fn build_results_json(&self) -> serde_json::Value {
        serde_json::json!({
            "topic": self.topic.trim(),
            "mode": match self.mode {
                SandboxMode::Timed => "timed",
                SandboxMode::Untimed => "untimed",
            },
            "duration_min": if self.mode == SandboxMode::Timed {
                Some(self.duration_min)
            } else {
                None
            },
            "task_count": self.tasks.len(),
            "agent_count": self.workers.len(),
            "total_elapsed_ms": self.total_elapsed_ms,
            "average_score": self.average_score(),
            "tasks": self.tasks.iter().enumerate().map(|(i, t)| {
                serde_json::json!({
                    "index": i + 1,
                    "title": t.title,
                    "status": t.status.label(),
                    "agent": t.agent,
                    "score": t.score,
                    "elapsed_ms": t.elapsed_ms,
                    "files": t.files,
                    "searches": t.searches,
                    "summary": t.summary,
                })
            }).collect::<Vec<_>>(),
            "log": self.log.iter().map(|e| {
                serde_json::json!({ "time": e.time, "text": e.text })
            }).collect::<Vec<_>>(),
        })
    }
}

// ============================================================================
// 解析与工具函数
// ============================================================================

/// 从共享流状态中一次性取走增量（partial / reasoning / done / error）
fn drain_stream_state(
    stream_state: &Arc<Mutex<AiStreamState>>,
) -> (String, String, bool, Option<String>) {
    let mut s = match stream_state.lock() {
        Ok(s) => s,
        Err(_) => return (String::new(), String::new(), false, None),
    };
    let partial = std::mem::take(&mut s.partial);
    let reasoning = std::mem::take(&mut s.reasoning);
    let done = s.done;
    let error = s.error.take();
    (partial, reasoning, done, error)
}

/// 实时预览尾部截断：只保留最后 700 个字符
fn trim_tail(tail: &mut String) {
    if tail.chars().count() > 700 {
        let t: String = tail
            .chars()
            .rev()
            .take(700)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        *tail = t;
    }
}

/// 解析规划器输出：优先识别 AETHER_PLAN 块内的 `TASK ` 行；
/// 兜底识别全文中的 `TASK ` 行或 "1. xxx" 编号行。
pub fn parse_sandbox_plan(response: &str) -> Vec<String> {
    let lines: Vec<&str> = response.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_end() == crate::ai_panel::PLAN_HEADER);
    let end = lines
        .iter()
        .position(|l| l.trim_end() == crate::ai_panel::PLAN_FOOTER);
    let body: Vec<&str> = match (start, end) {
        (Some(s), Some(e)) if e > s => lines[s + 1..e].to_vec(),
        _ => lines.clone(),
    };
    let mut tasks = Vec::new();
    for line in &body {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("TASK") {
            let rest = rest.trim_start_matches([' ', ':', '：']).trim();
            if !rest.is_empty() {
                tasks.push(rest.to_string());
            }
        }
    }
    if tasks.is_empty() {
        // 兜底：识别 "1. xxx" / "1、xxx" 编号行
        for line in &body {
            let t = line.trim();
            let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                continue;
            }
            let rest = t[digits.len()..]
                .trim_start_matches(['.', '、', ')', '）', ' '])
                .trim();
            if !rest.is_empty() {
                tasks.push(rest.to_string());
            }
        }
    }
    tasks.truncate(MAX_TASKS);
    tasks
}

/// 提取响应中的第一个联网搜索指令（单行：`<<<<<<< AETHER_SEARCH 查询词`）
pub fn parse_search_query(response: &str) -> Option<String> {
    for line in response.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(SEARCH_PREFIX) {
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                let q = rest.trim();
                if !q.is_empty() {
                    return Some(q.to_string());
                }
            }
        }
    }
    None
}

/// 从最终响应中提取文字总结（剥离文件/命令块后的 Text 部分，截断）
pub fn extract_summary(response: &str) -> String {
    let blocks = crate::ai_panel::parse_display_blocks(response);
    let mut text = String::new();
    for b in blocks {
        if let crate::ai_panel::AgentDisplayBlock::Text(t) = b {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(t.trim());
        }
    }
    truncate_chars(&text, 300)
}

/// 按字符数截断并追加省略号
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// 沙盒路径安全校验：拒绝绝对路径、盘符、`..` 逃逸，返回沙盒内绝对路径。
pub fn safe_sandbox_path(sandbox: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim();
    if rel.is_empty() {
        return Err("空路径".to_string());
    }
    let p = Path::new(rel);
    if p.is_absolute() || rel.contains(':') || rel.starts_with('/') || rel.starts_with('\\') {
        return Err("绝对路径被禁止".to_string());
    }
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            std::path::Component::CurDir => {}
            _ => return Err("路径包含越界成分".to_string()),
        }
    }
    Ok(sandbox.join(p))
}

/// 后台线程发起一次流式 AI 请求，把事件写入共享 stream_state。
fn spawn_sandbox_stream(
    settings: AiSettings,
    messages: Vec<ChatMessage>,
    stream_state: Arc<Mutex<AiStreamState>>,
    should_stop: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        run_stream_blocking(&settings, &messages, &stream_state, &should_stop);
    });
}

/// 后台线程：先联网搜索，再把结果拼进上下文续写。
fn spawn_search_then_stream(
    settings: AiSettings,
    mut messages: Vec<ChatMessage>,
    query: String,
    stream_state: Arc<Mutex<AiStreamState>>,
    should_stop: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let results_text = match web_search(&query, 5) {
            Ok(hits) if !hits.is_empty() => {
                let mut s = String::from("联网搜索结果（供参考，注意甄别）：\n");
                for (i, h) in hits.iter().enumerate() {
                    s.push_str(&format!(
                        "{}. {}\n   {}\n   来源: {}\n",
                        i + 1,
                        h.title,
                        h.snippet,
                        h.url
                    ));
                }
                s
            }
            Ok(_) => "联网搜索没有返回结果。".to_string(),
            Err(e) => format!("联网搜索失败（{}），请基于已有知识继续。", e),
        };
        if should_stop.load(Ordering::SeqCst) {
            if let Ok(mut s) = stream_state.lock() {
                s.done = true;
            }
            return;
        }
        messages.push(ChatMessage::user(format!(
            "{}\n请基于以上信息继续完成当前任务，直接输出成果文件块（不要再发起搜索）。",
            results_text
        )));
        run_stream_blocking(&settings, &messages, &stream_state, &should_stop);
    });
}

/// 阻塞消费一次流式请求（在后台线程内调用）
fn run_stream_blocking(
    settings: &AiSettings,
    messages: &[ChatMessage],
    stream_state: &Arc<Mutex<AiStreamState>>,
    should_stop: &Arc<AtomicBool>,
) {
    let client = AiClient::new(settings);
    match client.chat_completion_stream(messages) {
        Ok(rx) => {
            while let Ok(event) = rx.recv() {
                if should_stop.load(Ordering::SeqCst) {
                    if let Ok(mut s) = stream_state.lock() {
                        s.done = true;
                    }
                    break;
                }
                match event {
                    AiStreamEvent::Token(token) => {
                        if let Ok(mut s) = stream_state.lock() {
                            s.partial.push_str(&token);
                        }
                    }
                    AiStreamEvent::Reasoning(r) => {
                        if let Ok(mut s) = stream_state.lock() {
                            s.reasoning.push_str(&r);
                        }
                    }
                    AiStreamEvent::Done => {
                        if let Ok(mut s) = stream_state.lock() {
                            s.done = true;
                        }
                        break;
                    }
                    AiStreamEvent::Truncated(reason) => {
                        if let Ok(mut s) = stream_state.lock() {
                            s.truncated = Some(reason);
                        }
                    }
                    AiStreamEvent::Error(err) => {
                        if let Ok(mut s) = stream_state.lock() {
                            s.error = Some(crate::ai_panel::sanitize_error(&err));
                            s.done = true;
                        }
                        break;
                    }
                }
            }
        }
        Err(e) => {
            if let Ok(mut s) = stream_state.lock() {
                s.error = Some(e.safe_display());
                s.done = true;
            }
        }
    }
}

// ============================================================================
// 联网搜索（DuckDuckGo HTML，无需 API Key）
// ============================================================================

/// 搜索结果条目
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

/// 执行一次联网搜索，返回前 limit 条结果（在后台线程调用）。
pub fn web_search(query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        percent_encode(query)
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(12))
        .build();
    let body = agent
        .get(&url)
        .set(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AetherEditor/1.0",
        )
        .call()
        .map_err(|e| format!("请求失败: {}", e))?
        .into_string()
        .map_err(|e| format!("读取响应失败: {}", e))?;
    Ok(parse_ddg_results(&body, limit))
}

/// 解析 DuckDuckGo HTML 结果页（best-effort 字符串切分，避免引入 HTML 解析依赖）
fn parse_ddg_results(html: &str, limit: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut rest = html;
    while hits.len() < limit {
        // 结果链接锚点：<a rel="nofollow" class="result__a" href="...">标题</a>
        let Some(a_pos) = rest.find("class=\"result__a\"") else {
            break;
        };
        let after_a = &rest[a_pos..];
        let href = extract_between(after_a, "href=\"", "\"").unwrap_or_default();
        let title_html = extract_between(after_a, ">", "</a>").unwrap_or_default();
        // 摘要：class="result__snippet" ...>摘要</a>
        let snippet_html = after_a
            .find("result__snippet")
            .and_then(|p| extract_between(&after_a[p..], ">", "</a>"))
            .unwrap_or_default();
        let title = clean_html_text(&title_html);
        let snippet = truncate_chars(&clean_html_text(&snippet_html), 200);
        let url = resolve_ddg_href(&href);
        if !title.is_empty() {
            hits.push(SearchHit {
                title,
                snippet,
                url,
            });
        }
        // 前进到本条结果之后
        let advance = after_a
            .find("</a>")
            .map(|p| a_pos + p + 4)
            .unwrap_or(a_pos + 17);
        if advance >= rest.len() {
            break;
        }
        rest = &rest[advance..];
    }
    hits
}

/// DuckDuckGo 重定向链接（//duckduckgo.com/l/?uddg=<编码后URL>&...）还原为原始 URL
fn resolve_ddg_href(href: &str) -> String {
    if let Some(pos) = href.find("uddg=") {
        let encoded = &href[pos + 5..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        return percent_decode(encoded);
    }
    href.to_string()
}

/// 去除 HTML 标签并解码常见实体
fn clean_html_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// 提取 `start` 与 `end` 之间的内容
fn extract_between(s: &str, start: &str, end: &str) -> Option<String> {
    let a = s.find(start)? + start.len();
    let b = s[a..].find(end)? + a;
    Some(s[a..b].to_string())
}

/// URL 百分号编码（保守：字母数字与 -_.~ 之外全部编码）
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// URL 百分号解码（含 + → 空格）
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

// ============================================================================
// zip 打包（存储模式，无压缩依赖；CRC32 使用 flate2::Crc）
// ============================================================================

/// 把目录递归打包为 zip（stored 模式），返回打包的文件数
pub fn zip_dir(src_dir: &Path, dest: &Path) -> Result<usize, String> {
    let mut files = Vec::new();
    collect_files(src_dir, src_dir, &mut files).map_err(|e| format!("枚举沙盒文件失败: {}", e))?;
    if files.is_empty() {
        return Err("沙盒目录为空，没有可导出的内容".to_string());
    }
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let (dos_time, dos_date) = dos_datetime_now();
    for (abs, rel) in &files {
        let data = std::fs::read(abs).map_err(|e| format!("读取 {} 失败: {}", rel, e))?;
        let mut crc = flate2::Crc::new();
        crc.update(&data);
        let crc32 = crc.sum();
        let name_bytes = rel.as_bytes();
        let offset = out.len() as u32;
        // 本地文件头
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // 版本
        out.extend_from_slice(&(1u16 << 11).to_le_bytes()); // UTF-8 文件名标志
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&dos_time.to_le_bytes());
        out.extend_from_slice(&dos_date.to_le_bytes());
        out.extend_from_slice(&crc32.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&data);
        // 中央目录条目
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // made by
        central.extend_from_slice(&20u16.to_le_bytes()); // need
        central.extend_from_slice(&(1u16 << 11).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&dos_time.to_le_bytes());
        central.extend_from_slice(&dos_date.to_le_bytes());
        central.extend_from_slice(&crc32.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // 内部属性
        central.extend_from_slice(&0u32.to_le_bytes()); // 外部属性
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    // EOCD
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    std::fs::write(dest, out).map_err(|e| format!("写入 zip 失败: {}", e))?;
    Ok(files.len())
}

/// 递归收集目录下所有文件的 (绝对路径, zip 内相对路径)
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, rel));
        }
    }
    Ok(())
}

/// 当前本地时间的 DOS time/date（zip 条目时间戳）
fn dos_datetime_now() -> (u16, u16) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    // 简化：按 UTC 计算（zip 时间戳仅作展示用途）
    let days = secs / 86400;
    let tod = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    let hour = (tod / 3600) as u16;
    let min = ((tod % 3600) / 60) as u16;
    let sec = (tod % 60) as u16;
    let dos_time = (hour << 11) | (min << 5) | (sec / 2);
    let year = (y - 1980).max(0) as u16;
    let dos_date = (year << 9) | ((m as u16) << 5) | d as u16;
    (dos_time, dos_date)
}

/// 天数（自 1970-01-01）转公历 (年, 月, 日)（Howard Hinnant 算法）
fn civil_from_days(z: i64) -> (i64, u8, u8) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plan_task_lines() {
        let text = "<<<<<<< AETHER_PLAN\nTASK 写一篇介绍——不少于500字\nTASK 生成示例代码——含注释\n>>>>>>> AETHER_END_PLAN";
        let tasks = parse_sandbox_plan(text);
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].starts_with("写一篇介绍"));
    }

    #[test]
    fn test_parse_plan_numbered_fallback() {
        let text = "好的，任务如下：\n1. 撰写概述文档\n2、编写演示页面\n";
        let tasks = parse_sandbox_plan(text);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0], "撰写概述文档");
        assert_eq!(tasks[1], "编写演示页面");
    }

    #[test]
    fn test_parse_search_query() {
        let text = "我需要查资料：\n<<<<<<< AETHER_SEARCH Rust 2024 新特性\n";
        assert_eq!(
            parse_search_query(text),
            Some("Rust 2024 新特性".to_string())
        );
        assert_eq!(parse_search_query("普通文本"), None);
        // 前缀粘连（无空白分隔）不识别
        assert_eq!(parse_search_query("<<<<<<< AETHER_SEARCHX abc"), None);
    }

    #[test]
    fn test_safe_sandbox_path() {
        let root = Path::new("C:\\tmp\\sandbox");
        assert!(safe_sandbox_path(root, "a/b.txt").is_ok());
        assert!(safe_sandbox_path(root, "./a.txt").is_ok());
        assert!(safe_sandbox_path(root, "../escape.txt").is_err());
        assert!(safe_sandbox_path(root, "a/../../b.txt").is_err());
        assert!(safe_sandbox_path(root, "C:\\windows\\a.txt").is_err());
        assert!(safe_sandbox_path(root, "/etc/passwd").is_err());
        assert!(safe_sandbox_path(root, "").is_err());
    }

    #[test]
    fn test_percent_encode_decode() {
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        let s = "Rust 异步";
        assert_eq!(percent_decode(&percent_encode(s)), s);
    }

    #[test]
    fn test_clean_html_text() {
        assert_eq!(
            clean_html_text("<b>Hello</b> &amp; <i>world</i>"),
            "Hello & world"
        );
    }

    #[test]
    fn test_resolve_ddg_href() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc";
        assert_eq!(resolve_ddg_href(href), "https://example.com/page");
    }

    #[test]
    fn test_average_score() {
        let mut p = SandboxEvalPanel::new();
        p.tasks.push(SandboxTask::new("a".into()));
        p.tasks.push(SandboxTask::new("b".into()));
        p.tasks[0].status = SandboxTaskStatus::Done;
        p.tasks[1].status = SandboxTaskStatus::Done;
        assert_eq!(p.average_score(), None);
        p.set_score(0, 8);
        p.set_score(1, 5);
        assert!((p.average_score().unwrap() - 6.5).abs() < 0.001);
        assert!(p.all_scored());
    }

    #[test]
    fn test_effective_agent_count() {
        let mut p = SandboxEvalPanel::new();
        p.agent_count = 5;
        assert_eq!(p.effective_agent_count(), 5);
        p.custom_count = "99".to_string();
        assert_eq!(p.effective_agent_count(), MAX_AGENTS);
        p.custom_count = "7".to_string();
        assert_eq!(p.effective_agent_count(), 7);
        p.custom_count = "0".to_string();
        assert_eq!(p.effective_agent_count(), 5);
    }

    #[test]
    fn test_worker_idle_and_task_agent_field() {
        let w = AgentWorker::new(1);
        assert!(w.is_idle());
        let mut t = SandboxTask::new("x".into());
        assert_eq!(t.agent, None);
        t.agent = Some(2);
        t.status = SandboxTaskStatus::Running;
        assert!(!t.is_finished());
    }

    #[test]
    fn test_zip_dir_roundtrip_signature() {
        let dir = std::env::temp_dir().join(format!("aether_zip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        std::fs::write(dir.join("sub").join("b.txt"), "世界").unwrap();
        let dest = dir.join("out.zip");
        // 注意：out.zip 写在 dir 内会被 collect 到吗？先收集再写入，
        // collect 在 zip_dir 内部先执行，此时 out.zip 尚不存在，不会自包含。
        let n = zip_dir(&dir, &dest).unwrap();
        assert_eq!(n, 2);
        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        // EOCD 签名存在
        assert!(bytes.windows(4).any(|w| w == [0x50, 0x4b, 0x05, 0x06]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
    }

    #[test]
    fn test_extract_summary_strips_file_blocks() {
        let text = "完成思路说明。\n<<<<<<< AETHER_FILE a.txt\n======= AETHER_SEP\n内容\n>>>>>>> AETHER_END_FILE\n收尾。";
        let s = extract_summary(text);
        assert!(s.contains("完成思路说明"));
        assert!(s.contains("收尾"));
        assert!(!s.contains("AETHER_FILE"));
    }
}
