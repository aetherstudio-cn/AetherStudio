//! 内置浏览器模块：借用系统自带的 WebView2（Edge Chromium 内核）实现智能体模式浏览器标签页。
//!
//! 架构：
//! - 每个窗口共享一个 ICoreWebView2Environment，首次打开浏览器标签时惰性异步创建
//! - 每个浏览器标签持有一个 ICoreWebView2Controller，父窗口为主窗口；
//!   webview 子窗口叠于 D2D 渲染面之上，边界/显隐随布局与活动标签每帧同步
//! - WebView2 的所有回调都由运行时 Post 回 UI 线程，回调内只把动作压入线程本地
//!   挂起队列并 PostMessage(WM_BROWSER_EVENT)，主循环在安全时机统一应用，
//!   避免回调期间与消息处理函数的 RefCell 借用冲突

use std::cell::RefCell;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Environment,
};
use webview2_com::{
    take_pwstr, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DocumentTitleChangedEventHandler,
    ExecuteScriptCompletedHandler, HistoryChangedEventHandler, NavigationCompletedEventHandler,
    NewWindowRequestedEventHandler, SourceChangedEventHandler,
};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{BOOL, E_POINTER, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::WinRT::EventRegistrationToken;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::layout::Region;

/// 浏览器工具栏高度（逻辑像素）
pub const BROWSER_TOOLBAR_HEIGHT: f32 = 42.0;
/// 默认主页
pub const DEFAULT_HOME_URL: &str = "https://www.bing.com";

/// 回喂给模型的网页正文最大字符数（控制上下文占用）
pub const PAGE_CONTENT_MAX_CHARS: usize = 6000;

/// 浏览器异步事件自定义消息（window.rs 中分发给 on_browser_event）
pub const WM_BROWSER_EVENT: u32 = WM_APP + 13;

/// 网页正文采集脚本（在页面上下文执行，返回 JSON 对象）：
/// `{title, url, text, entries:[{title,url,snippet}]}`
/// - 必应搜索结果页优先抽取条目列表（标题 + 链接 + 摘要），信息密度远高于整页文本；
/// - 其余页面取正文（优先 article/main，去空行），上限 6000 字符。
pub const PAGE_EXTRACT_SCRIPT: &str = r#"(function(){
function norm(s){return (s||'').split('\n').map(function(l){return l.replace(/[ \t]+/g,' ').trim();}).filter(function(l){return l;}).join('\n');}
function oneLine(s){return (s||'').replace(/\s+/g,' ').trim();}
var out={title:document.title||'',url:location.href,text:'',entries:[]};
try{
var isSearch=location.href.indexOf('bing.com/search')>=0||location.href.indexOf('duckduckgo.com')>=0;
var nodes=isSearch?document.querySelectorAll('li.b_algo, article[data-testid="result"]'):[];
for(var i=0;i<nodes.length&&out.entries.length<8;i++){
var li=nodes[i];
var a=li.querySelector('h2 a, a');
var h=li.querySelector('h2')||a;
var p=li.querySelector('.b_caption p')||li.querySelector('[data-result="snippet"]')||li.querySelector('p');
if(!h||!a)continue;
out.entries.push({title:oneLine(h.textContent),url:a.href,snippet:oneLine(p?p.textContent:'').slice(0,300)});
}
if(!out.entries.length){
var root=document.querySelector('article')||document.querySelector('main')||document.body;
out.text=norm(root?(root.innerText||root.textContent):'').slice(0,6000);
}
}catch(e){out.text='';}
return out;})()
"#;

thread_local! {
    /// WebView2 回调待应用动作队列（UI 线程内传递 COM 对象，无需 Send）
    static PENDING: RefCell<Vec<PendingAction>> = const { RefCell::new(Vec::new()) };
}

/// WebView2 回调产生的待应用动作
enum PendingAction {
    /// 环境创建完成
    EnvironmentReady(windows::core::Result<ICoreWebView2Environment>),
    /// 控制器创建完成（浏览器实例 id）
    ControllerReady(usize, windows::core::Result<ICoreWebView2Controller>),
    /// 导航完成（浏览器实例 id，是否成功）
    NavigationCompleted(usize, bool),
    /// 页面地址变化
    SourceChanged(usize, String),
    /// 历史记录变化（前进/后退可用性更新）
    HistoryChanged(usize),
    /// 页面标题变化
    TitleChanged(usize, String),
    /// 正文采集脚本执行完成（浏览器实例 id，Ok(JSON) / Err(失败原因)）
    ScriptCompleted(usize, Result<String, String>),
}

/// 回调动作入队并唤醒主循环
fn push_action(action: PendingAction, hwnd: HWND) {
    PENDING.with(|q| q.borrow_mut().push(action));
    unsafe {
        let _ = PostMessageW(hwnd, WM_BROWSER_EVENT, WPARAM(0), LPARAM(0));
    }
}

/// 读取视图当前 URL（失败返回空串）
unsafe fn view_source(view: &ICoreWebView2) -> String {
    let mut pw = PWSTR::null();
    if view.Source(&mut pw).is_ok() {
        take_pwstr(pw)
    } else {
        String::new()
    }
}

/// 导航到指定 URL（调用方保证 url 已规范化）
unsafe fn navigate_view(view: &ICoreWebView2, url: &str) {
    let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = view.Navigate(PCWSTR(wide.as_ptr()));
}

/// 在页面执行正文采集脚本，结果经 `WM_BROWSER_EVENT` 回传主循环
unsafe fn run_extract_script(view: &ICoreWebView2, id: usize, hwnd: HWND) {
    let script: Vec<u16> = PAGE_EXTRACT_SCRIPT
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(
        move |error_code: windows::core::Result<()>, result_json: String| {
            let action = match error_code {
                Ok(()) if !result_json.is_empty() => {
                    PendingAction::ScriptCompleted(id, Ok(result_json))
                }
                Ok(()) => PendingAction::ScriptCompleted(
                    id,
                    Err("页面未返回内容（可能为空白页或跨域限制）".to_string()),
                ),
                Err(e) => PendingAction::ScriptCompleted(id, Err(format!("{e:?}"))),
            };
            push_action(action, hwnd);
            Ok(())
        },
    ));
    if view
        .ExecuteScript(PCWSTR(script.as_ptr()), &handler)
        .is_err()
    {
        push_action(
            PendingAction::ScriptCompleted(id, Err("采集脚本提交失败".to_string())),
            hwnd,
        );
    }
}

/// 输入规范化：带协议直接导航；像域名补 https://；否则按必应搜索处理
pub fn normalize_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return DEFAULT_HOME_URL.to_string();
    }
    if s.contains("://") {
        return s.to_string();
    }
    let looks_like_domain = !s.contains(' ') && (s.contains('.') || s.starts_with("localhost"));
    if looks_like_domain {
        format!("https://{s}")
    } else {
        format!("https://www.bing.com/search?q={}", percent_encode(s))
    }
}

/// 简易百分号编码（仅保留 unreserved 字符）
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// 用系统默认浏览器打开 URL（无内嵌浏览器时的搜索/导航回退路径）
pub fn open_external_url(url: &str) {
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let _ = ShellExecuteW(
            None,
            windows::core::PCWSTR(operation.as_ptr()),
            windows::core::PCWSTR(wide.as_ptr()),
            windows::core::PCWSTR::null(),
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// 把采集脚本返回的 JSON 转成给模型阅读的纯文本。
///
/// 输入形如 `{"title":"...","url":"...","text":"...","entries":[{"title","url","snippet"}]}`；
/// 搜索结果页优先输出条目列表（标题 / 链接 / 摘要），普通页输出正文，
/// 正文按 `max_chars` 截断，避免上下文爆炸。解析失败时返回原始串（截断）以便排查。
pub fn format_page_content(json: &str, max_chars: usize) -> String {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => {
            let raw: String = json.chars().take(max_chars).collect();
            return raw;
        }
    };
    let title = value.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let url = value.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let mut out = String::new();
    out.push_str(&format!("标题：{}\n地址：{}\n", title, url));

    let entries = value
        .get("entries")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty());
    if let Some(list) = entries {
        out.push_str("搜索结果：\n");
        for (i, e) in list.iter().enumerate() {
            let t = e.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let u = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let s = e.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!("{}. {}\n   {}\n", i + 1, t, u));
            if !s.is_empty() {
                out.push_str(&format!("   {}\n", s));
            }
        }
    } else {
        let text = value.get("text").and_then(|v| v.as_str()).unwrap_or("");
        if text.is_empty() {
            out.push_str("（未提取到正文内容）\n");
        } else {
            let clipped: String = text.chars().take(max_chars).collect();
            out.push_str("正文：\n");
            out.push_str(&clipped);
            out.push('\n');
        }
    }
    out
}

/// 内置浏览器打开失败的原因
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserOpenError {
    /// 本机缺少 WebView2 Runtime（Edge 内核不可用）
    Unsupported,
}

impl BrowserOpenError {
    /// 面向用户/模型的中文说明
    pub fn message(self) -> &'static str {
        match self {
            BrowserOpenError::Unsupported => {
                "当前系统缺少 WebView2 运行库，内置浏览器不可用（安装 Microsoft Edge 或 WebView2 Runtime 后可用）"
            }
        }
    }
}

/// 智能体网页读取的在途任务：结果（或超时）由主循环轮询后回喂给对应会话。
#[derive(Clone, Debug)]
pub struct PendingPageRead {
    /// 浏览器实例 id
    pub id: usize,
    /// 目标会话索引（结果写回该会话，支持后台并发会话）
    pub conv_idx: usize,
    /// 提交时间（超时兜底）
    pub started: std::time::Instant,
    /// 展示用的目标地址
    pub url: String,
    /// 本轮同步探查的反馈文本（随首个网页结果一起回喂，避免重复发轮）
    pub extra_feedback: String,
}

/// 浏览器工具栏命中布局（绝对逻辑坐标，渲染时记录，点击处理使用）
#[derive(Clone, Debug)]
pub struct BrowserToolbarLayout {
    pub back: Region,
    pub forward: Region,
    pub refresh: Region,
    pub address: Region,
}

/// 单个浏览器标签实例
pub struct BrowserInstance {
    pub id: usize,
    /// 当前页面 URL
    pub url: String,
    /// 页面标题
    pub title: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// 正在加载
    pub loading: bool,
    /// 地址栏编辑模式
    pub address_editing: bool,
    /// 地址栏编辑文本
    pub address_text: String,
    controller: Option<ICoreWebView2Controller>,
    view: Option<ICoreWebView2>,
    /// 等待环境就绪后创建控制器
    pending_create: bool,
    /// 已请求正文采集，等待页面加载完成后执行脚本
    read_requested: bool,
    /// 采集目标 URL（None 表示当前页面；Some 时先导航再采集）
    read_target_url: Option<String>,
    /// 最近一次采集结果（JSON 字符串），由 AI 层取用后 `take`
    pub last_script_json: Option<String>,
    /// 最近一次采集失败原因（与成功结果互斥）
    pub last_script_error: Option<String>,
    /// 上次同步的边界（物理像素），避免重复 SetBounds
    last_bounds: Option<RECT>,
    /// 上次同步的可见性
    last_visible: Option<bool>,
}

impl BrowserInstance {
    /// 刷新前进/后退可用状态
    unsafe fn refresh_history_state(&mut self) {
        if let Some(view) = &self.view {
            let mut b = BOOL::default();
            if view.CanGoBack(&mut b).is_ok() {
                self.can_go_back = b.as_bool();
            }
            if view.CanGoForward(&mut b).is_ok() {
                self.can_go_forward = b.as_bool();
            }
        }
    }

    /// 从视图同步当前 URL 到状态
    unsafe fn refresh_source(&mut self) {
        if let Some(view) = &self.view {
            let url = view_source(view);
            if !url.is_empty() {
                self.url = url.clone();
                if !self.address_editing {
                    self.address_text = url;
                }
            }
        }
    }

    pub fn go_back(&self) {
        if let Some(v) = &self.view {
            let _ = unsafe { v.GoBack() };
        }
    }

    pub fn go_forward(&self) {
        if let Some(v) = &self.view {
            let _ = unsafe { v.GoForward() };
        }
    }

    pub fn reload(&self) {
        if let Some(v) = &self.view {
            let _ = unsafe { v.Reload() };
        }
    }

    /// 导航到新地址（规范化输入），并退出地址栏编辑模式
    pub fn navigate_input(&mut self, input: &str) {
        let url = normalize_url(input);
        if let Some(v) = &self.view {
            unsafe { navigate_view(v, &url) };
        }
        self.url = url.clone();
        self.address_text = url.clone();
        self.address_editing = false;
        self.loading = true;
    }

    /// 安排一次正文采集（`url` 为 None 表示当前页面）：
    /// - 目标与当前页面相同且已加载完 → 立即执行采集脚本；
    /// - 目标不同 → 先导航，导航完成后执行；
    /// - 视图尚未就绪（WebView2 环境/控制器创建中）→ 挂起，就绪后补做。
    ///
    /// 结果通过 `WM_BROWSER_EVENT` 回调写入 `last_script_json` / `last_script_error`。
    pub fn arm_read(&mut self, url: Option<String>, hwnd: HWND) {
        self.last_script_json = None;
        self.last_script_error = None;
        let view_ready = self.view.is_some();
        let need_nav = view_ready && url.as_deref().is_some_and(|u| u != self.url.as_str());
        if !view_ready || need_nav || self.loading {
            self.read_requested = true;
            self.read_target_url = url;
            if need_nav {
                self.loading = true;
                if let (Some(view), Some(u)) = (&self.view, self.read_target_url.as_deref()) {
                    unsafe { navigate_view(view, u) };
                }
            }
            return;
        }
        self.read_requested = false;
        self.read_target_url = None;
        if let Some(view) = &self.view {
            unsafe { run_extract_script(view, self.id, hwnd) };
        }
    }

    /// 页面加载完成且本轮有采集请求时执行脚本（由导航完成回调驱动）
    unsafe fn maybe_extract(&mut self, hwnd: HWND) {
        if !self.read_requested || self.loading {
            return;
        }
        let Some(view) = self.view.clone() else {
            return;
        };
        self.read_requested = false;
        self.read_target_url = None;
        run_extract_script(&view, self.id, hwnd);
    }
}

impl Drop for BrowserInstance {
    fn drop(&mut self) {
        // WebView2 要求显式 Close 后再释放控制器
        if let Some(controller) = self.controller.take() {
            unsafe {
                let _ = controller.Close();
            }
        }
    }
}

/// 内置浏览器管理器（每个窗口一个，挂在 EditorState 上）
pub struct BrowserState {
    env_ready: bool,
    env_creating: bool,
    /// 环境初始化失败（如系统缺少 WebView2 Runtime）
    pub env_failed: bool,
    environment: Option<ICoreWebView2Environment>,
    /// 所有浏览器实例
    pub instances: Vec<BrowserInstance>,
    next_id: usize,
    /// 最近一次渲染记录的工具栏命中布局
    pub toolbar_layout: Option<BrowserToolbarLayout>,
    /// 智能体模式空状态快捷搜索框：直接输入回车即搜，无需先进浏览器
    pub empty_search_focused: bool,
    pub empty_search_text: String,
    pub empty_search_composition: Option<String>,
    pub empty_search_caret_visible: bool,
    /// 智能体网页读取在途任务（主循环轮询取结果回喂）
    pub pending_reads: Vec<PendingPageRead>,
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            env_ready: false,
            env_creating: false,
            env_failed: false,
            environment: None,
            instances: Vec::new(),
            next_id: 1,
            toolbar_layout: None,
            empty_search_focused: false,
            empty_search_text: String::new(),
            empty_search_composition: None,
            empty_search_caret_visible: false,
            pending_reads: Vec::new(),
        }
    }

    /// 重置空状态快捷搜索框（搜索执行/焦点转移后调用）
    pub fn reset_empty_search(&mut self) {
        self.empty_search_focused = false;
        self.empty_search_text.clear();
        self.empty_search_composition = None;
    }

    pub fn get(&self, id: usize) -> Option<&BrowserInstance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut BrowserInstance> {
        self.instances.iter_mut().find(|i| i.id == id)
    }

    /// 创建浏览器实例（返回 id），控制器在环境就绪后异步创建
    pub fn add_instance(&mut self, url: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.instances.push(BrowserInstance {
            id,
            url: url.to_string(),
            title: String::new(),
            can_go_back: false,
            can_go_forward: false,
            loading: true,
            address_editing: false,
            address_text: url.to_string(),
            controller: None,
            view: None,
            pending_create: true,
            read_requested: false,
            read_target_url: None,
            last_script_json: None,
            last_script_error: None,
            last_bounds: None,
            last_visible: None,
        });
        id
    }

    /// 移除并销毁指定实例（关闭标签页时调用）
    pub fn remove_instance(&mut self, id: usize) {
        if let Some(pos) = self.instances.iter().position(|i| i.id == id) {
            self.instances.remove(pos);
        }
    }

    /// 移除不再被任何标签引用的实例（标签批量关闭操作的统一清理）
    pub fn retain_referenced(&mut self, tabs: &[crate::tabs::Tab]) {
        let referenced: std::collections::HashSet<usize> =
            tabs.iter().filter_map(|t| t.browser_id()).collect();
        self.instances.retain(|i| referenced.contains(&i.id));
    }

    /// 确保 WebView2 环境正在创建（幂等，异步）
    pub fn ensure_environment(&mut self, hwnd: HWND) {
        if self.env_ready || self.env_creating || self.env_failed {
            return;
        }
        // WebView2 依赖 COM 单元线程模型；宿主未初始化时在此补齐（重复调用无害）
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        self.env_creating = true;
        let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |error_code: windows::core::Result<()>,
                  environment: Option<ICoreWebView2Environment>| {
                let res = error_code
                    .and_then(|_| environment.ok_or_else(|| windows::core::Error::from(E_POINTER)));
                push_action(PendingAction::EnvironmentReady(res), hwnd);
                Ok(())
            },
        ));
        unsafe {
            if CreateCoreWebView2EnvironmentWithOptions(None, None, None, &handler).is_err() {
                self.env_creating = false;
                self.env_failed = true;
                tracing::error!("内置浏览器: CreateCoreWebView2EnvironmentWithOptions 调用失败");
            }
        }
    }

    /// 请求为指定实例创建控制器；环境已就绪时立即发起，否则等环境回调后统一补创建
    pub fn request_controller(&mut self, id: usize, hwnd: HWND) {
        if let Some(inst) = self.get_mut(id) {
            inst.pending_create = true;
        }
        if self.env_ready {
            unsafe { self.start_pending_creates(hwnd) };
        }
    }

    /// 环境是否已确认不可用（系统缺少 WebView2 Runtime）
    pub fn is_unsupported(&self) -> bool {
        self.env_failed
    }

    /// 请求提取指定实例的网页正文：`url` 为 Some 时先导航到该页（输入按 URL/关键词规范化），
    /// 为 None 时提取当前页面。结果由 `take_read_result` 轮询取用。
    ///
    /// 返回 false 表示实例不存在或本机不支持内置浏览器。
    pub fn request_read(&mut self, id: usize, url: Option<&str>, hwnd: HWND) -> bool {
        if self.env_failed || self.get(id).is_none() {
            return false;
        }
        let target = url.map(normalize_url);
        // 视图尚未创建时补齐创建请求（就绪后由 on_controller_ready 接续采集）
        if self
            .get(id)
            .map(|inst| inst.controller.is_none())
            .unwrap_or(false)
        {
            self.ensure_environment(hwnd);
            self.request_controller(id, hwnd);
        }
        if let Some(inst) = self.get_mut(id) {
            inst.arm_read(target, hwnd);
        }
        true
    }

    /// 取出指定实例的采集结果（一次性消费）：
    /// `Some(Ok(json))` 采集成功、`Some(Err(msg))` 采集失败、`None` 仍在等待
    pub fn take_read_result(&mut self, id: usize) -> Option<Result<String, String>> {
        let inst = self.get_mut(id)?;
        if let Some(json) = inst.last_script_json.take() {
            return Some(Ok(json));
        }
        inst.last_script_error.take().map(Err)
    }

    /// 轮询在途网页读取任务：已就绪（成功/失败）或超时的一次性返回并从队列移除，
    /// 其余继续等待。由主循环每帧调用（与终端命令结果回环同构）。
    pub fn poll_pending_reads(
        &mut self,
        timeout: std::time::Duration,
    ) -> Vec<(PendingPageRead, Result<String, String>)> {
        if self.pending_reads.is_empty() {
            return Vec::new();
        }
        let mut done = Vec::new();
        let mut keep = Vec::new();
        for task in std::mem::take(&mut self.pending_reads) {
            match self.take_read_result(task.id) {
                Some(result) => done.push((task, result)),
                None => {
                    if task.started.elapsed() >= timeout {
                        done.push((
                            task,
                            Err("网页加载超时，未能提取正文（可稍后重试或改用其他来源）"
                                .to_string()),
                        ));
                    } else {
                        keep.push(task);
                    }
                }
            }
        }
        self.pending_reads = keep;
        done
    }

    /// 为所有等待中的实例发起控制器创建（仅在环境就绪后调用）
    unsafe fn start_pending_creates(&mut self, hwnd: HWND) {
        let Some(env) = self.environment.clone() else {
            return;
        };
        for inst in &mut self.instances {
            if !inst.pending_create || inst.controller.is_some() {
                continue;
            }
            inst.pending_create = false;
            let id = inst.id;
            let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                move |error_code: windows::core::Result<()>,
                      controller: Option<ICoreWebView2Controller>| {
                    let res = error_code.and_then(|_| {
                        controller.ok_or_else(|| windows::core::Error::from(E_POINTER))
                    });
                    push_action(PendingAction::ControllerReady(id, res), hwnd);
                    Ok(())
                },
            ));
            if env.CreateCoreWebView2Controller(hwnd, &handler).is_err() {
                tracing::error!("内置浏览器: CreateCoreWebView2Controller 调用失败 (id={id})");
            }
        }
    }

    /// 控制器就绪：配置初始状态并订阅页面事件
    unsafe fn on_controller_ready(
        &mut self,
        id: usize,
        controller: ICoreWebView2Controller,
        hwnd: HWND,
    ) {
        let Some(inst) = self.get_mut(id) else {
            // 实例已被关闭：立即释放控制器
            let _ = controller.Close();
            return;
        };
        // 初始隐藏，由 sync_visibility 按布局统一显示
        let _ = controller.SetIsVisible(false);
        let view = match controller.CoreWebView2() {
            Ok(v) => v,
            Err(_) => {
                let _ = controller.Close();
                return;
            }
        };
        // 禁用开发者工具；保留网页原生右键菜单
        if let Ok(settings) = view.Settings() {
            let _ = settings.SetAreDevToolsEnabled(false);
        }

        let mut token = EventRegistrationToken::default();

        // URL 变化
        let _ = view.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _args| {
                let url = sender.as_ref().map(|s| view_source(s)).unwrap_or_default();
                push_action(PendingAction::SourceChanged(id, url), hwnd);
                Ok(())
            })),
            &mut token,
        );

        // 历史变化（前进/后退可用性）
        let _ = view.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |_sender, _args| {
                push_action(PendingAction::HistoryChanged(id), hwnd);
                Ok(())
            })),
            &mut token,
        );

        // 导航完成
        let _ = view.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                let ok = args
                    .as_ref()
                    .map(|a| {
                        let mut b = BOOL::default();
                        a.IsSuccess(&mut b).map(|_| b.as_bool()).unwrap_or(false)
                    })
                    .unwrap_or(false);
                // 顺带取一次标题（部分页面标题早于导航完成事件到达）
                if let Some(s) = sender.as_ref() {
                    let mut pw = PWSTR::null();
                    if s.DocumentTitle(&mut pw).is_ok() {
                        let title = take_pwstr(pw);
                        if !title.is_empty() {
                            push_action(PendingAction::TitleChanged(id, title), hwnd);
                        }
                    }
                }
                push_action(PendingAction::NavigationCompleted(id, ok), hwnd);
                Ok(())
            })),
            &mut token,
        );

        // 页面标题变化
        let _ = view.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _args| {
                if let Some(s) = sender.as_ref() {
                    let mut pw = PWSTR::null();
                    if s.DocumentTitle(&mut pw).is_ok() {
                        push_action(PendingAction::TitleChanged(id, take_pwstr(pw)), hwnd);
                    }
                }
                Ok(())
            })),
            &mut token,
        );

        // 新窗口请求（target=_blank 等）：改为在当前视图内导航
        let _ = view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    let mut pw = PWSTR::null();
                    if args.Uri(&mut pw).is_ok() {
                        let url = take_pwstr(pw);
                        if !url.is_empty() {
                            if let Some(s) = sender.as_ref() {
                                navigate_view(s, &url);
                            }
                        }
                    }
                    let _ = args.SetHandled(true);
                }
                Ok(())
            })),
            &mut token,
        );

        inst.controller = Some(controller);
        inst.view = Some(view.clone());

        // 发起初始导航（若此前已排队等待采集某个页面，则直接导航到该页；
        // read_requested 保持不变，导航完成后由 maybe_extract 提取正文）
        let url = inst.read_target_url.clone().unwrap_or_else(|| {
            if inst.url.is_empty() {
                DEFAULT_HOME_URL.to_string()
            } else {
                inst.url.clone()
            }
        });
        navigate_view(&view, &url);
    }

    /// 应用 WebView2 回调累积的挂起动作（主循环 WM_BROWSER_EVENT 时排空）
    pub fn drain_pending(&mut self, hwnd: HWND) {
        let actions: Vec<PendingAction> = PENDING.with(|q| std::mem::take(&mut *q.borrow_mut()));
        let mut need_start = false;
        for action in actions {
            match action {
                PendingAction::EnvironmentReady(res) => {
                    self.env_creating = false;
                    match res {
                        Ok(env) => {
                            self.environment = Some(env);
                            self.env_ready = true;
                            need_start = true;
                        }
                        Err(e) => {
                            self.env_failed = true;
                            // 排队中的正文采集统一置失败，上层立即得到明确原因
                            for inst in &mut self.instances {
                                if inst.read_requested {
                                    inst.read_requested = false;
                                    inst.last_script_error = Some(
                                        "系统缺少 WebView2 Runtime，内置浏览器不可用".to_string(),
                                    );
                                }
                            }
                            tracing::error!("内置浏览器: WebView2 环境初始化失败 {e:?}（系统可能缺少 WebView2 Runtime）");
                        }
                    }
                }
                PendingAction::ControllerReady(id, res) => match res {
                    Ok(controller) => unsafe {
                        self.on_controller_ready(id, controller, hwnd);
                    },
                    Err(e) => {
                        tracing::error!("内置浏览器: 控制器创建失败 (id={id}) {e:?}");
                        if let Some(inst) = self.get_mut(id) {
                            inst.loading = false;
                            // 排队中的正文采集直接失败，避免上层等到超时
                            if inst.read_requested {
                                inst.read_requested = false;
                                inst.last_script_error = Some(format!("网页视图创建失败：{e:?}"));
                            }
                        }
                    }
                },
                PendingAction::NavigationCompleted(id, _ok) => {
                    if let Some(inst) = self.get_mut(id) {
                        inst.loading = false;
                        unsafe {
                            inst.refresh_history_state();
                            inst.refresh_source();
                            // 页面加载完成后补做排队的正文采集
                            inst.maybe_extract(hwnd);
                        }
                    }
                }
                PendingAction::SourceChanged(id, url) => {
                    if let Some(inst) = self.get_mut(id) {
                        inst.url = url.clone();
                        if !inst.address_editing {
                            inst.address_text = url;
                        }
                    }
                }
                PendingAction::HistoryChanged(id) => {
                    if let Some(inst) = self.get_mut(id) {
                        unsafe { inst.refresh_history_state() };
                    }
                }
                PendingAction::TitleChanged(id, title) => {
                    if let Some(inst) = self.get_mut(id) {
                        inst.title = title;
                    }
                }
                PendingAction::ScriptCompleted(id, result) => {
                    if let Some(inst) = self.get_mut(id) {
                        match result {
                            Ok(json) => inst.last_script_json = Some(json),
                            Err(e) => inst.last_script_error = Some(e),
                        }
                    }
                }
            }
        }
        if need_start {
            unsafe { self.start_pending_creates(hwnd) };
        }
    }

    /// 同步所有 WebView2 边界与显隐：仅活动实例在给定内容区域（逻辑像素，
    /// 工具栏下方）显示，其余全部隐藏
    pub fn sync_visibility(&mut self, dpi_scale: f32, active_id: Option<usize>, region: Region) {
        for inst in &mut self.instances {
            let Some(controller) = &inst.controller else {
                continue;
            };
            let show = active_id == Some(inst.id)
                && region.width > 1.0
                && region.height > BROWSER_TOOLBAR_HEIGHT + 1.0;
            unsafe {
                if show {
                    let rect = RECT {
                        left: (region.x * dpi_scale) as i32,
                        top: ((region.y + BROWSER_TOOLBAR_HEIGHT) * dpi_scale) as i32,
                        right: ((region.x + region.width) * dpi_scale) as i32,
                        bottom: ((region.y + region.height) * dpi_scale) as i32,
                    };
                    if inst.last_bounds != Some(rect) {
                        let _ = controller.SetBounds(rect);
                        inst.last_bounds = Some(rect);
                    }
                    if inst.last_visible != Some(true) {
                        let _ = controller.SetIsVisible(true);
                        inst.last_visible = Some(true);
                    }
                } else if inst.last_visible != Some(false) {
                    let _ = controller.SetIsVisible(false);
                    inst.last_visible = Some(false);
                }
            }
        }
    }
}

impl crate::editor::EditorState {
    /// 当前活动浏览器实例 id（智能体模式需右面板可见；活动标签为浏览器标签时）
    pub fn active_browser_id(&self) -> Option<usize> {
        if !self.ui.layout.right_panel_visible {
            return None;
        }
        match self.editor.tab_bar.tabs.get(self.editor.tab_bar.active_tab) {
            Some(crate::tabs::Tab::Browser(id)) => Some(*id),
            _ => None,
        }
    }

    /// 每帧同步内置浏览器 WebView2 显隐与边界（渲染末尾调用）：叠于右面板内容区
    pub fn sync_browser_webviews(&mut self) {
        if self.browser.instances.is_empty() {
            return;
        }
        let (active_id, region) = if let Some(id) = self.active_browser_id() {
            let rp = self.ui.layout.right_panel_region();
            let tab_h = if self.show_tab_bar() {
                crate::layout::TAB_BAR_HEIGHT
            } else {
                0.0
            };
            (
                Some(id),
                Region::new(rp.x, rp.y + tab_h, rp.width, rp.height - tab_h),
            )
        } else {
            (None, Region::new(0.0, 0.0, 0.0, 0.0))
        };
        self.browser
            .sync_visibility(self.win.dpi_scale, active_id, region);
    }

    /// 智能体 `AETHER_OPEN`：在内置浏览器打开网址/域名/搜索词（用户可见，不回喂内容）。
    ///
    /// 成功返回实际导航的 URL；失败返回面向模型的说明文本（本机缺少 WebView2 Runtime）。
    pub fn agent_browser_open(&mut self, query: &str) -> Result<String, String> {
        let url = crate::browser::normalize_url(query);
        self.open_browser_tab(&url)
            .map_err(|e| e.message().to_string())?;
        Ok(url)
    }

    /// 智能体 `AETHER_READPAGE`：导航（可选）后排队提取网页正文，返回使用的实例 id。
    ///
    /// - `url_input` 为空：提取当前活动浏览器页面（无活动浏览器时报错）；
    /// - `url_input` 非空：已有活动浏览器标签则复用并导航（避免堆积标签），否则新开标签。
    ///
    /// 结果由主循环轮询 `browser.take_read_result(id)` 取用。
    pub fn agent_browser_read(&mut self, url_input: &str) -> Result<usize, String> {
        let hwnd = self.win.hwnd;
        let trimmed = url_input.trim();
        let id = if trimmed.is_empty() {
            self.active_browser_id().ok_or_else(|| {
                "当前没有已打开的网页，请先用 AETHER_OPEN 打开页面再读取。".to_string()
            })?
        } else {
            let url = crate::browser::normalize_url(trimmed);
            match self.active_browser_id() {
                Some(id) => {
                    if let Some(inst) = self.browser.get_mut(id) {
                        inst.navigate_input(&url);
                    }
                    id
                }
                None => {
                    self.open_browser_tab(&url)
                        .map_err(|e| e.message().to_string())?;
                    self.active_browser_id()
                        .ok_or_else(|| "内置浏览器标签创建失败".to_string())?
                }
            }
        };
        if !self.browser.request_read(id, None, hwnd) {
            return Err(crate::browser::BrowserOpenError::Unsupported
                .message()
                .to_string());
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url_keeps_protocol() {
        assert_eq!(
            normalize_url("https://example.com/a"),
            "https://example.com/a"
        );
    }

    #[test]
    fn test_normalize_url_domain_gets_https() {
        assert_eq!(normalize_url("baidu.com"), "https://baidu.com");
        assert_eq!(normalize_url("localhost:3000"), "https://localhost:3000");
    }

    #[test]
    fn test_normalize_url_keyword_becomes_search() {
        let u = normalize_url("rust async");
        assert!(u.starts_with("https://www.bing.com/search?q="), "实际: {u}");
        assert!(u.contains("rust%20async"), "空格需被编码: {u}");
    }

    #[test]
    fn test_normalize_url_empty_is_home() {
        assert_eq!(normalize_url("   "), DEFAULT_HOME_URL);
    }

    #[test]
    fn test_arm_read_queues_before_view_ready() {
        // 视图尚未创建（WebView2 异步初始化中）：只登记采集意图，不执行脚本
        let mut state = BrowserState::new();
        let id = state.add_instance("https://example.com");
        state.get_mut(id).unwrap().arm_read(
            Some("https://example.com".to_string()),
            HWND(std::ptr::null_mut()),
        );
        let inst = state.get(id).unwrap();
        assert!(inst.read_requested);
        assert_eq!(inst.read_target_url.as_deref(), Some("https://example.com"));
        assert!(inst.last_script_json.is_none());
        assert!(inst.last_script_error.is_none());
    }

    #[test]
    fn test_take_read_result_consumes_once() {
        let mut state = BrowserState::new();
        let id = state.add_instance("https://example.com");
        assert!(state.take_read_result(id).is_none());
        state.get_mut(id).unwrap().last_script_json = Some("{\"text\":\"hi\"}".to_string());
        assert!(matches!(
            state.take_read_result(id),
            Some(Ok(ref j)) if j.contains("hi")
        ));
        // 一次性消费：再次取用为空
        assert!(state.take_read_result(id).is_none());
        // 不存在的实例
        assert!(state.take_read_result(9999).is_none());
    }

    #[test]
    fn test_format_page_content_entries() {
        let json = r#"{"title":"Rust 异步 - 搜索","url":"https://www.bing.com/search?q=rust","text":"","entries":[{"title":"The Rust Programming Language","url":"https://rust-lang.org","snippet":"Rust 是一种拥抱内存安全的编程语言。"}]}"#;
        let out = format_page_content(json, 6000);
        assert!(out.contains("The Rust Programming Language"), "实际: {out}");
        assert!(out.contains("https://rust-lang.org"));
        assert!(out.contains("内存安全"));
        assert!(out.contains("搜索结果"));
    }

    #[test]
    fn test_format_page_content_body_text() {
        let json =
            r#"{"title":"示例","url":"https://example.com","text":"第一行\n第二行","entries":[]}"#;
        let out = format_page_content(json, 6000);
        assert!(out.contains("正文"), "实际: {out}");
        assert!(out.contains("第一行\n第二行"));
    }

    #[test]
    fn test_format_page_content_respects_char_limit() {
        let long = "啊".repeat(100);
        let json = format!(
            r#"{{"title":"t","url":"u","text":"{}","entries":[]}}"#,
            long
        );
        let out = format_page_content(&json, 10);
        assert_eq!(out.matches('啊').count(), 10, "正文应按字符数截断");
    }

    #[test]
    fn test_format_page_content_invalid_json_falls_back() {
        let out = format_page_content("不是 JSON", 6000);
        assert_eq!(out, "不是 JSON");
    }
}
