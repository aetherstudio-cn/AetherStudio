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
    HistoryChangedEventHandler, NavigationCompletedEventHandler, NewWindowRequestedEventHandler,
    SourceChangedEventHandler,
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

/// 浏览器异步事件自定义消息（window.rs 中分发给 on_browser_event）
pub const WM_BROWSER_EVENT: u32 = WM_APP + 13;

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

/// 用系统默认浏览器打开 URL（经典模式无内嵌浏览器时的搜索/导航回退路径）
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

        // 发起初始导航
        let url = if inst.url.is_empty() {
            DEFAULT_HOME_URL.to_string()
        } else {
            inst.url.clone()
        };
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
                        }
                    }
                },
                PendingAction::NavigationCompleted(id, _ok) => {
                    if let Some(inst) = self.get_mut(id) {
                        inst.loading = false;
                        unsafe {
                            inst.refresh_history_state();
                            inst.refresh_source();
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
    /// 当前活动浏览器实例 id（智能体模式需右面板可见；经典模式随中心编辑区；活动标签为浏览器标签时）
    pub fn active_browser_id(&self) -> Option<usize> {
        if self.editor_mode.is_agent() && !self.ui.layout.right_panel_visible {
            return None;
        }
        match self.editor.tab_bar.tabs.get(self.editor.tab_bar.active_tab) {
            Some(crate::tabs::Tab::Browser(id)) => Some(*id),
            _ => None,
        }
    }

    /// 每帧同步内置浏览器 WebView2 显隐与边界（渲染末尾调用）：
    /// 智能体模式叠于右面板内容区，经典模式叠于中心编辑器内容区
    pub fn sync_browser_webviews(&mut self) {
        if self.browser.instances.is_empty() {
            return;
        }
        let (active_id, region) = if let Some(id) = self.active_browser_id() {
            if self.editor_mode.is_agent() {
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
                (
                    Some(id),
                    self.ui.layout.editor_content_region(self.show_tab_bar()),
                )
            }
        } else {
            (None, Region::new(0.0, 0.0, 0.0, 0.0))
        };
        self.browser
            .sync_visibility(self.win.dpi_scale, active_id, region);
    }
}
