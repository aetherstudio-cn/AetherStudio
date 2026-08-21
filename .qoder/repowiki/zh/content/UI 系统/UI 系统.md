# UI 系统

<cite>
**本文引用的文件**   
- [main.rs](file://crates/aether-win32/src/main.rs)
- [window.rs](file://crates/aether-win32/src/window.rs)
- [window_setup.rs](file://crates/aether-win32/src/window/window_setup.rs)
- [render.rs](file://crates/aether-win32/src/render.rs)
- [render_context.rs](file://crates/aether-win32/src/render_context.rs)
- [dirty_rect.rs](file://crates/aether-win32/src/dirty_rect.rs)
- [hit_test.rs](file://crates/aether-win32/src/hit_test.rs)
- [input.rs](file://crates/aether-win32/src/input.rs)
- [ime.rs](file://crates/aether-win32/src/ime.rs)
- [keyboard_handler.rs](file://crates/aether-win32/src/window/keyboard_handler.rs)
- [mouse_handler.rs](file://crates/aether-win32/src/window/mouse_handler.rs)
- [theme.rs](file://crates/aether-win32/src/theme.rs)
- [aether-render_theme.rs](file://crates/aether-render/src/theme.rs)
- [aether-render_lib.rs](file://crates/aether-render/src/lib.rs)
- [user_menu.rs](file://crates/aether-win32/src/user_menu.rs)
- [account.rs](file://crates/aether-win32/src/render/account.rs)
- [welcome.rs](file://crates/aether-win32/src/welcome.rs)
- [bitmap_loader.rs](file://crates/aether-win32/src/bitmap_loader.rs)
- [browser.rs](file://crates/aether-win32/src/browser.rs)
- [agent_right_panel.rs](file://crates/aether-win32/src/render/agent_right_panel.rs)
- [agent_sidebar.rs](file://crates/aether-win32/src/render/agent_sidebar.rs)
- [chrome.rs](file://crates/aether-win32/src/render/chrome.rs)
- [layout.rs](file://crates/aether-win32/src/layout.rs)
- [ai_agent.rs](file://crates/aether-ai-panel/src/ai_agent.rs)
</cite>

## 更新摘要
**变更内容**   
- 新增嵌入式浏览器功能，基于 WebView2 实现智能体模式下的网页浏览能力
- 新增 Agent 右侧面板组件，支持标签页管理、新标签页快捷操作和浏览器工具栏
- 新增 Agent 侧边栏组件，提供对话历史管理和工作区文件列表
- 增强布局系统以支持智能体模式和开发者模式的动态切换
- 集成 AI Agent 工具标记协议，支持文件编辑、命令执行等高级功能

## 目录
1. [简介](#简介)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细组件分析](#详细组件分析)
6. [依赖关系分析](#依赖关系分析)
7. [性能考量](#性能考量)
8. [故障排查指南](#故障排查指南)
9. [结论](#结论)
10. [附录](#附录)

## 简介
本技术文档面向牧羊人编辑器的 UI 子系统，聚焦以下方面：
- Win32 窗口管理机制：窗口创建、消息循环与事件分发
- Direct2D/DirectWrite 渲染集成：绘制上下文管理、脏矩形优化与动画效果
- 输入事件处理：键盘映射、鼠标交互与输入法（IME）支持
- 主题系统：颜色管理、样式继承与动态切换
- **新增** 嵌入式浏览器功能：基于 WebView2 的智能体模式网页浏览
- **新增** Agent 模式支持：右侧面板和侧边栏组件的完整实现
- UI 组件开发最佳实践与性能优化技巧

**更新** 已新增完整的浏览器功能和Agent模式支持，包括嵌入式WebView2浏览器、Agent右侧面板和Agent侧边栏组件，显著增强了编辑器的智能化能力。

## 项目结构
UI 子系统主要位于 aether-win32 crate 中，围绕 window 模块组织；渲染相关能力由 aether-render crate 提供。**新增** 的浏览器和Agent模式功能分布在专门的模块中。关键入口与职责如下：
- main.rs：进程启动、单实例控制、调用 run(args)
- window.rs：注册窗口类、创建主窗口、消息循环、WndProc 分发
- window/window_setup.rs：DPI 感知、DWM 背景效果、窗口持久化、COPYDATA 处理
- render.rs：EditorState::render 主渲染流程、区域裁剪、设备丢失恢复
- render_context.rs：Direct2D 渲染目标封装、画刷/文本格式缓存、多矩形裁剪
- dirty_rect.rs：脏矩形追踪与渲染命令推断
- hit_test.rs：命中区域记录（调试/测试构建）
- input.rs：按键类型、快捷键绑定、动作枚举、多光标模型
- ime.rs：IMM32 集成，候选/合成窗口定位与 DPI 缩放
- window/keyboard_handler.rs、window/mouse_handler.rs：键盘/鼠标事件拆分处理
- theme.rs：UI 设置与默认主题工厂
- user_menu.rs：用户菜单和头像下拉菜单功能
- render/account.rs：账户相关的渲染逻辑（简化版）
- welcome.rs：欢迎屏幕界面，包含吉祥物图像显示
- bitmap_loader.rs：位图加载器，支持嵌入式PNG资源
- **新增** browser.rs：嵌入式浏览器管理器，基于 WebView2 实现
- **新增** agent_right_panel.rs：Agent模式右侧面板渲染
- **新增** agent_sidebar.rs：Agent模式左侧边栏渲染
- **新增** chrome.rs：状态栏、菜单栏、标题栏渲染增强
- **新增** layout.rs：布局管理器，支持智能体模式切换
- **新增** ai_agent.rs：AI Agent工具标记协议解析

```mermaid
graph TB
A["main.rs<br/>进程入口"] --> B["window.rs<br/>run()/WndProc"]
B --> C["window/window_setup.rs<br/>DPI/DWM/持久化/COPYDATA"]
B --> D["window/keyboard_handler.rs<br/>键盘处理"]
B --> E["window/mouse_handler.rs<br/>鼠标处理"]
B --> F["render.rs<br/>EditorState::render"]
F --> G["render_context.rs<br/>D2D 上下文/缓存/裁剪"]
F --> H["dirty_rect.rs<br/>脏矩形/渲染命令"]
F --> I["hit_test.rs<br/>命中区域(调试)"]
B --> J["ime.rs<br/>IME 集成"]
B --> K["input.rs<br/>键映射/动作/多光标"]
F --> L["theme.rs / aether-render/src/theme.rs<br/>主题/语法色"]
F --> M["user_menu.rs<br/>用户菜单/头像下拉"]
F --> N["render/account.rs<br/>账户渲染(简化)"]
F --> O["welcome.rs<br/>欢迎屏幕/吉祥物图像"]
O --> P["bitmap_loader.rs<br/>嵌入式PNG资源加载"]
F --> Q["browser.rs<br/>嵌入式浏览器"]
F --> R["agent_right_panel.rs<br/>Agent右侧面板"]
F --> S["agent_sidebar.rs<br/>Agent侧边栏"]
F --> T["chrome.rs<br/>状态栏/菜单栏/标题栏"]
R --> U["layout.rs<br/>布局管理"]
S --> U
Q --> U
U --> V["ai_agent.rs<br/>AI Agent工具协议"]
```

**图表来源** 
- [main.rs:1-52](file://crates/aether-win32/src/main.rs#L1-L52)
- [window.rs:114-173](file://crates/aether-win32/src/window.rs#L114-L173)
- [window_setup.rs:18-86](file://crates/aether-win32/src/window/window_setup.rs#L18-L86)
- [render.rs:62-780](file://crates/aether-win32/src/render.rs#L62-L780)
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)
- [hit_test.rs:1-245](file://crates/aether-win32/src/hit_test.rs#L1-L245)
- [ime.rs:1-255](file://crates/aether-win32/src/ime.rs#L1-L255)
- [input.rs:1-355](file://crates/aether-win32/src/input.rs#L1-L355)
- [theme.rs:1-26](file://crates/aether-win32/src/theme.rs#L1-L26)
- [aether-render_theme.rs:1-485](file://crates/aether-render/src/theme.rs#L1-L485)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [account.rs:1-50](file://crates/aether-win32/src/render/account.rs#L1-L50)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [chrome.rs:1-800](file://crates/aether-win32/src/render/chrome.rs#L1-L800)
- [layout.rs:1-400](file://crates/aether-win32/src/layout.rs#L1-L400)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

**章节来源**
- [main.rs:1-52](file://crates/aether-win32/src/main.rs#L1-L52)
- [window.rs:114-173](file://crates/aether-win32/src/window.rs#L114-L173)

## 核心组件
- 窗口与消息循环
  - 注册窗口类、创建主窗口、设置 DPI 感知与 DWM 背景效果
  - 全局窗口计数确保多窗口正确退出
  - WndProc 统一分发到各处理器
- 渲染管线
  - EditorState::render 负责布局计算、脏区推断、裁剪与绘制
  - RenderContext 封装 D2D 目标、画刷/文本格式缓存、多矩形裁剪
  - DirtyRectTracker 维护脏矩形并推断最小重绘范围
- 输入系统
  - KeyMap 将虚拟键码转换为编辑器动作
  - 鼠标/滚轮处理覆盖标签栏、侧边栏、终端等区域
  - IME 集成支持候选/合成窗口定位与 DPI 缩放
- 主题系统
  - Theme 提供玻璃/深色两套配色，SyntaxColors 集中管理语法着色
  - 通过 BrushCache/TextFormatCache 预初始化常用资源
- 用户界面组件
  - 简化的用户菜单系统，仅包含头像下拉菜单功能
  - 移除了复杂的账户设置页面，降低了UI架构复杂度
  - **新增** 欢迎屏幕吉祥物图像采用嵌入式PNG资源，确保跨平台一致性
- **新增** 嵌入式浏览器系统
  - 基于 WebView2 的嵌入式浏览器，支持标签页管理
  - 异步环境初始化，避免阻塞主线程
  - 智能体模式下叠于右面板，经典模式下叠于编辑器区域
- **新增** Agent 模式组件
  - 右侧面板：标签页管理、新标签页快捷操作、浏览器工具栏
  - 左侧边栏：对话历史管理、工作区文件列表
  - 支持智能体模式与开发者模式的动态切换

**更新** 新增了完整的浏览器功能和Agent模式支持，包括嵌入式WebView2浏览器、Agent右侧面板和Agent侧边栏组件，显著增强了编辑器的智能化能力。

**章节来源**
- [window.rs:175-297](file://crates/aether-win32/src/window.rs#L175-L297)
- [render.rs:62-780](file://crates/aether-win32/src/render.rs#L62-L780)
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)
- [input.rs:1-355](file://crates/aether-win32/src/input.rs#L1-L355)
- [ime.rs:1-255](file://crates/aether-win32/src/ime.rs#L1-L255)
- [theme.rs:1-26](file://crates/aether-win32/src/theme.rs#L1-L26)
- [aether-render_theme.rs:1-485](file://crates/aether-render/src/theme.rs#L1-L485)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [account.rs:1-50](file://crates/aether-win32/src/render/account.rs#L1-L50)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)

## 架构总览
下图展示从进程启动到渲染输出的关键路径，以及输入事件如何驱动状态变更与重绘。**新增** 了浏览器和Agent模式的完整集成路径。

```mermaid
sequenceDiagram
participant Main as "main.rs"
participant Win as "window.rs"
participant Setup as "window_setup.rs"
participant Proc as "WndProc"
participant Input as "键盘/鼠标/IME"
participant State as "EditorState"
participant Browser as "BrowserState"
participant Agent as "Agent面板"
participant Render as "render.rs"
participant RCtx as "render_context.rs"
participant Dirty as "dirty_rect.rs"
participant UserMenu as "user_menu.rs"
participant Welcome as "welcome.rs"
participant Bitmap as "bitmap_loader.rs"
Main->>Win : run(args)
Win->>Setup : set_dpi_awareness()
Win->>Setup : enable_dwm_acrylic(hwnd)
Win->>Win : create_editor_window()
Win->>Win : init_editor_state()
Win->>Win : 进入消息循环(GetMessageW/DispatchMessageW)
loop 每帧
Proc->>Input : 分发 WM_* 事件
Input->>State : 更新状态(光标/滚动/面板可见性等)
Input->>UserMenu : 处理用户菜单交互
Input->>Welcome : 处理欢迎屏幕交互
Input->>Browser : 处理浏览器事件
Input->>Agent : 处理Agent模式交互
Welcome->>Bitmap : 加载嵌入式PNG资源
Bitmap->>Welcome : 返回位图数据
Browser->>State : 同步浏览器状态
Agent->>State : 更新Agent面板状态
Input->>Win : invalidate_window(hwnd)
Win->>Render : WM_PAINT -> render()
Render->>Dirty : 推断/标记脏矩形
Render->>RCtx : begin_draw()/push_multi_clip()/clear()
Render->>RCtx : 绘制标题栏/菜单/侧边栏/编辑器/欢迎页→右侧面板→底部面板→状态栏→弹出菜单/对话框
Render->>Browser : 同步WebView2边界和可见性
Render->>Agent : 渲染Agent面板内容
Render->>RCtx : end_draw()
Render->>Dirty : clear()
end
```

**图表来源** 
- [main.rs:1-52](file://crates/aether-win32/src/main.rs#L1-L52)
- [window.rs:114-173](file://crates/aether-win32/src/window.rs#L114-L173)
- [window_setup.rs:18-86](file://crates/aether-win32/src/window/window_setup.rs#L18-L86)
- [render.rs:62-780](file://crates/aether-win32/src/render.rs#L62-L780)
- [render_context.rs:66-155](file://crates/aether-win32/src/render_context.rs#L66-L155)
- [dirty_rect.rs:387-426](file://crates/aether-win32/src/dirty_rect.rs#L387-L426)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)

## 详细组件分析

### Win32 窗口管理与消息循环
- 窗口类注册与创建
  - 使用 RegisterClassW 注册窗口类，CreateWindowExW 创建主窗口
  - 启用 CS_DBLCLKS 以接收双击消息
  - 应用 DWM 属性实现 Acrylic/Mica 背景与透明客户区穿透
- DPI 感知与尺寸恢复
  - 优先使用 Per-Monitor V2 DPI 感知，失败回退至 Per-Monitor
  - 根据持久化 settings 计算窗口矩形，校验显示器存在性
  - 最大化状态在创建后通过 ShowWindow(SW_MAXIMIZE) 恢复
- 消息循环与分发
  - GetMessageW/TranslateMessage/DispatchMessageW 标准循环
  - WndProc 内 catch_unwind 包裹，异常时回退 DefWindowProcW
  - 自定义消息用于低层键盘钩子投递（终端直接接收编辑键）
- 窗口生命周期与持久化
  - WM_DESTROY 中卸载键盘钩子、持久化窗口位置/最大化/工作区
  - 全局窗口计数保证所有窗口关闭才退出应用

```mermaid
flowchart TD
Start(["进程启动"]) --> Single["单实例检查"]
Single --> |已有实例| SendArgs["发送参数并退出"]
Single --> |首个实例| Run["run(args)"]
Run --> DPI["设置 DPI 感知"]
DPI --> Class["注册窗口类"]
Class --> Create["创建主窗口"]
Create --> DWM["启用 DWM 背景效果"]
Create --> InitState["初始化编辑器状态/渲染目标"]
InitState --> Loop["消息循环"]
Loop --> Dispatch["WndProc 分发"]
Dispatch --> Destroy{"WM_DESTROY?"}
Destroy --> |是| Persist["持久化窗口状态"]
Destroy --> |否| Loop
Persist --> Exit["关闭窗口计数归零则 PostQuitMessage"]
```

**图表来源** 
- [main.rs:1-52](file://crates/aether-win32/src/main.rs#L1-L52)
- [window.rs:114-173](file://crates/aether-win32/src/window.rs#L114-L173)
- [window_setup.rs:18-86](file://crates/aether-win32/src/window/window_setup.rs#L18-L86)
- [window_setup.rs:110-178](file://crates/aether-win32/src/window/window_setup.rs#L110-L178)
- [window_setup.rs:188-217](file://crates/aether-win32/src/window/window_setup.rs#L188-L217)
- [window.rs:299-373](file://crates/aether-win32/src/window.rs#L299-L373)

**章节来源**
- [window.rs:175-297](file://crates/aether-win32/src/window.rs#L175-L297)
- [window_setup.rs:18-86](file://crates/aether-win32/src/window/window_setup.rs#L18-L86)
- [window_setup.rs:110-178](file://crates/aether-win32/src/window/window_setup.rs#L110-L178)
- [window_setup.rs:188-217](file://crates/aether-win32/src/window/window_setup.rs#L188-L217)
- [window.rs:299-373](file://crates/aether-win32/src/window.rs#L299-L373)

### Direct2D/DirectWrite 渲染集成
- 绘制上下文管理
  - RenderContext 封装 ID2D1HwndRenderTarget、BrushCache、TextFormatCache、TextLayoutCache
  - 设备丢失时清理并重建资源，避免崩溃
- 脏矩形与裁剪
  - DirtyRectTracker 按区域类型合并重叠矩形，超过阈值降级为全窗口重绘
  - RenderContext::push_multi_clip 支持多矩形并集裁剪，失败回退包围盒
- 渲染流程
  - EditorState::render 先计算布局与可见行范围，重建增量缓存
  - 根据状态变化推断 RenderCommand，仅标记必要脏区域
  - 按层级绘制：标题栏→菜单栏→活动栏→侧边栏→标签栏→编辑器/欢迎页→右侧面板→底部面板→状态栏→弹出菜单/对话框
  - 最后清除脏标记并输出命中区域（调试）

**更新** 渲染流程已增强，新增了对嵌入式浏览器和Agent模式的支持。在智能体模式下，WebView2 子窗口会叠于右面板内容区域之上，提供更好的用户体验。

```mermaid
classDiagram
class RenderContext {
+target : Option<RenderTarget>
+brush_cache : BrushCache
+text_format_cache : TextFormatCache
+text_layout_cache : TextLayoutCache
+init_render_target(...)
+begin_draw()
+end_draw() Result
+clear(color)
+push_multi_clip(factory, rects) bool
+pop_multi_clip(use_layer)
+fill_rect(x,y,w,h,color)
+handle_device_lost()
}
class DirtyRectTracker {
+mark_full_window()
+mark_region(x,y,w,h,type)
+is_full_window() bool
+has_dirty() bool
+rects() &[DirtyRect]
+merge_all_rects()
+clear()
}
class EditorState {
+render()
+init_render_target()
+flush_events_to_dirty_tracker()
+sync_browser_webviews()
+render_agent_right_panel()
+render_agent_sidebar()
}
EditorState --> RenderContext : "使用"
EditorState --> DirtyRectTracker : "使用"
```

**图表来源** 
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)
- [render.rs:62-780](file://crates/aether-win32/src/render.rs#L62-L780)
- [browser.rs:608-651](file://crates/aether-win32/src/browser.rs#L608-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)

**章节来源**
- [render.rs:62-780](file://crates/aether-win32/src/render.rs#L62-L780)
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)

### 输入事件处理系统
- 键盘映射
  - KeyMap 将 VK 码与修饰键组合映射到 EditorAction
  - vk_to_char 使用 ToUnicode 获取当前键盘布局字符，支持多语言
- 鼠标交互
  - 左键按下/抬起/双击：选择、拖拽、标签重排、长按检测
  - 滚轮：标签栏横向滚动、Shift+滚轮横向滚动、终端面板滚动、侧边栏滚动
  - 水平滚轮：编辑器内容横向滚动
- IME 支持
  - IMM32 集成：ImmGetContext/ImmSetCandidateWindow/ImmSetCompositionWindow
  - 候选/合成窗口尺寸随 DPI 缩放，跟随光标定位
  - 支持临时解除 IME 关联以旁路系统级拦截（如终端删除汉字问题）

**更新** 输入系统已增强以支持新的浏览器和Agent模式功能。现在可以处理浏览器标签页的点击事件、Agent面板的交互操作，以及智能体模式下的特殊键盘快捷键。

```mermaid
sequenceDiagram
participant User as "用户"
participant Win as "WndProc"
participant Mouse as "mouse_handler.rs"
participant Key as "keyboard_handler.rs"
participant IME as "ime.rs"
participant UserMenu as "user_menu.rs"
participant Welcome as "welcome.rs"
participant Browser as "browser.rs"
participant Agent as "Agent面板"
participant State as "EditorState"
participant WinAPI as "invalidate_window()"
User->>Win : 鼠标/键盘/IME 事件
alt 鼠标事件
Win->>Mouse : on_l_button_down/up/dblclk/wheel/hwheel
Mouse->>State : 更新选择/滚动/面板拖拽/标签重排
Mouse->>UserMenu : 处理用户菜单点击
Mouse->>Welcome : 处理欢迎屏幕交互
Mouse->>Browser : 处理浏览器工具栏点击
Mouse->>Agent : 处理Agent面板交互
Mouse->>WinAPI : invalidate_window(hwnd)
else 键盘事件
Win->>Key : on_key_down/on_char
Key->>State : 执行动作(编辑/视图/多光标/AI)
Key->>Browser : 处理浏览器快捷键
Key->>Agent : 处理Agent模式快捷键
Key->>WinAPI : invalidate_window(hwnd)
else IME 事件
Win->>IME : 更新候选/合成窗口位置
IME->>State : 读取合成串/结果串
IME->>WinAPI : invalidate_window(hwnd)
end
```

**图表来源** 
- [window.rs:301-373](file://crates/aether-win32/src/window.rs#L301-L373)
- [mouse_handler.rs:1-277](file://crates/aether-win32/src/window/mouse_handler.rs#L1-L277)
- [keyboard_handler.rs:1-13](file://crates/aether-win32/src/window/keyboard_handler.rs#L1-L13)
- [ime.rs:1-255](file://crates/aether-win32/src/ime.rs#L1-L255)
- [input.rs:1-355](file://crates/aether-win32/src/input.rs#L1-L355)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)

**章节来源**
- [input.rs:1-355](file://crates/aether-win32/src/input.rs#L1-L355)
- [mouse_handler.rs:1-277](file://crates/aether-win32/src/window/mouse_handler.rs#L1-L277)
- [keyboard_handler.rs:1-13](file://crates/aether-win32/src/window/keyboard_handler.rs#L1-L13)
- [ime.rs:1-255](file://crates/aether-win32/src/ime.rs#L1-L255)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)

### 主题系统
- 颜色管理
  - Theme 包含编辑器背景、行号、选择高亮、侧边栏、状态栏、标签栏、标题栏、阴影、光晕等字段
  - SyntaxColors 统一管理关键字、字符串、注释、函数、类型名、语义令牌等颜色
  - color_for_token/color_for_semantic_token_index 提供通用 token 与语义令牌的颜色查找
- 样式继承与默认值
  - dark() 与 glass() 共享 SyntaxColors::shared()，减少重复
  - Default 实现返回 glass 主题
- 动态切换
  - 通过 brush_cache.init_common_brushes 与 text_format_cache.init_common_formats 预初始化常用资源
  - 设备丢失或主题切换时重建缓存

```mermaid
classDiagram
class Theme {
+editor_bg : D2D1_COLOR_F
+line_highlight_bg : D2D1_COLOR_F
+selection_bg : D2D1_COLOR_F
+cursor_color : D2D1_COLOR_F
+sidebar_bg : D2D1_COLOR_F
+statusbar_bg : D2D1_COLOR_F
+tab_active_bg : D2D1_COLOR_F
+tab_inactive_bg : D2D1_COLOR_F
+text_default : D2D1_COLOR_F
+titlebar_bg : D2D1_COLOR_F
+activity_bar_bg : D2D1_COLOR_F
+panel_border : D2D1_COLOR_F
+shadow : D2D1_COLOR_F
+glow_selection : D2D1_COLOR_F
+command_palette_bg : D2D1_COLOR_F
+submenu_bg : D2D1_COLOR_F
+glass_enabled : bool
+syntax : SyntaxColors
+color_for_token(kind) D2D1_COLOR_F
+color_for_semantic_token_index(idx, mods) D2D1_COLOR_F
+dark() Theme
+glass() Theme
}
class SyntaxColors {
+keyword : D2D1_COLOR_F
+string : D2D1_COLOR_F
+number : D2D1_COLOR_F
+comment : D2D1_COLOR_F
+function : D2D1_COLOR_F
+type_name : D2D1_COLOR_F
+operator : D2D1_COLOR_F
+variable : D2D1_COLOR_F
+preprocessor : D2D1_COLOR_F
+attribute : D2D1_COLOR_F
+macro_color : D2D1_COLOR_F
+lifetime : D2D1_COLOR_F
+regex : D2D1_COLOR_F
+format_string : D2D1_COLOR_F
+md_heading : D2D1_COLOR_F
+md_link : D2D1_COLOR_F
+md_code : D2D1_COLOR_F
+md_emphasis : D2D1_COLOR_F
+json_key : D2D1_COLOR_F
+toml_table : D2D1_COLOR_F
+find_highlight : D2D1_COLOR_F
+semantic_namespace : D2D1_COLOR_F
+semantic_type : D2D1_COLOR_F
+semantic_class : D2D1_COLOR_F
+semantic_enum : D2D1_COLOR_F
+semantic_interface : D2D1_COLOR_F
+semantic_struct : D2D1_COLOR_F
+semantic_type_parameter : D2D1_COLOR_F
+semantic_parameter : D2D1_COLOR_F
+semantic_variable_local : D2D1_COLOR_F
+semantic_variable_global : D2D1_COLOR_F
+semantic_property : D2D1_COLOR_F
+semantic_enum_member : D2D1_COLOR_F
+semantic_event : D2D1_COLOR_F
+semantic_function_declaration : D2D1_COLOR_F
+semantic_function_call : D2D1_COLOR_F
+semantic_method : D2D1_COLOR_F
+semantic_macro : D2D1_COLOR_F
+semantic_keyword_control : D2D1_COLOR_F
+semantic_modifier : D2D1_COLOR_F
+semantic_comment_doc : D2D1_COLOR_F
+semantic_string_format : D2D1_COLOR_F
+semantic_number_hex : D2D1_COLOR_F
+semantic_regexp : D2D1_COLOR_F
+semantic_operator_logical : D2D1_COLOR_F
+semantic_readonly : D2D1_COLOR_F
+semantic_deprecated : D2D1_COLOR_F
+semantic_async : D2D1_COLOR_F
+semantic_static : D2D1_COLOR_F
+semantic_abstract : D2D1_COLOR_F
+shared() SyntaxColors
}
Theme --> SyntaxColors : "包含"
```

**图表来源** 
- [aether-render_theme.rs:1-485](file://crates/aether-render/src/theme.rs#L1-L485)

**章节来源**
- [aether-render_theme.rs:1-485](file://crates/aether-render/src/theme.rs#L1-L485)
- [theme.rs:1-26](file://crates/aether-win32/src/theme.rs#L1-L26)
- [render_context.rs:189-217](file://crates/aether-win32/src/render_context.rs#L189-L217)

### 嵌入式浏览器系统

**新增** 基于 WebView2 的嵌入式浏览器系统，为智能体模式提供强大的网页浏览能力。

- WebView2 集成
  - 使用 Microsoft Edge Chromium 内核的 WebView2 控件
  - 每个窗口共享一个 ICoreWebView2Environment，首次打开时惰性异步创建
  - 每个浏览器标签持有一个 ICoreWebView2Controller，父窗口为主窗口
  - 所有回调通过 PostMessage 安全地传递到 UI 线程处理
- 浏览器实例管理
  - BrowserState 管理多个浏览器实例的生命周期
  - 支持标签页的创建、销毁、导航和历史记录管理
  - 自动处理 WebView2 环境的初始化和错误恢复
- 智能体模式集成
  - 在智能体模式下，浏览器叠于右面板内容区域之上
  - 在经典模式下，浏览器叠于中心编辑器内容区域
  - 每帧同步 WebView2 子窗口的边界和可见性
- 工具栏功能
  - 后退/前进按钮，支持历史记录导航
  - 刷新按钮，重新加载当前页面
  - 地址栏，支持 URL 输入和搜索查询
  - 自动 URL 规范化，支持域名补全和搜索查询转换

```mermaid
flowchart TD
BrowserInit["浏览器初始化"] --> EnvCheck{"WebView2环境就绪?"}
EnvCheck --> |否| CreateEnv["异步创建环境"]
EnvCheck --> |是| Ready["环境就绪"]
CreateEnv --> ControllerCheck{"控制器创建?"}
ControllerCheck --> |否| Wait["等待环境就绪"]
ControllerCheck --> |是| Active["浏览器实例激活"]
Ready --> ControllerCheck
Wait --> ControllerCheck
Active --> Sync["同步边界和可见性"]
Sync --> Navigate["导航到URL"]
Navigate --> Display["显示网页内容"]
```

**图表来源** 
- [browser.rs:237-651](file://crates/aether-win32/src/browser.rs#L237-L651)

**章节来源**
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)

### Agent 模式组件

**新增** 完整的 Agent 模式支持，包括右侧面板和左侧边栏组件。

- Agent 右侧面板
  - 标签页管理：支持多个标签页的创建、切换和关闭
  - 新标签页（NTP）：提供快捷搜索框和常用操作按钮
  - 浏览器工具栏：集成嵌入式浏览器的导航功能
  - 内容区域：根据活动标签页类型渲染不同的内容
- Agent 左侧边栏
  - 对话历史：显示和管理 AI 对话会话
  - 工作区文件：集成文件树浏览功能
  - 新会话按钮：快速创建新的 AI 对话
  - 可折叠设计：支持对话历史和文件列表的展开/收起
- 布局管理
  - 支持智能体模式和开发者模式的动态切换
  - 智能体模式下，AI 对话面板为主体，编辑器移至右侧
  - 开发者模式下，传统 IDE 布局，AI 面板在右侧
- AI Agent 工具协议
  - 支持文件编辑、命令执行、只读探查等工具标记
  - 解析 AI 回复中的结构化指令
  - 提供精确的代码定位和编辑功能

```mermaid
graph LR
subgraph "Agent 模式布局"
LeftSidebar["左侧边栏<br/>对话历史 + 文件列表"]
RightPanel["右侧面板<br/>标签页 + 内容区域"]
Editor["编辑器区域<br/>代码编辑"]
end
subgraph "右侧面板内容"
TabBar["标签栏"]
ContentArea["内容区域"]
BrowserToolbar["浏览器工具栏"]
NewTabPage["新标签页"]
end
LeftSidebar --> |管理| RightPanel
RightPanel --> |包含| TabBar
RightPanel --> |包含| ContentArea
ContentArea --> |渲染| BrowserToolbar
ContentArea --> |渲染| NewTabPage
RightPanel --> |嵌入| Editor
```

**图表来源** 
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [layout.rs:137-171](file://crates/aether-win32/src/layout.rs#L137-L171)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

**章节来源**
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [layout.rs:137-171](file://crates/aether-win32/src/layout.rs#L137-L171)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

### 用户界面组件简化

**更新** 用户界面组件已大幅简化，移除了复杂的账户设置页面功能。

- 简化的用户菜单
  - 仅保留头像下拉菜单功能，提供更简洁的用户交互体验
  - 移除了账户设置页面的复杂渲染逻辑和相关状态管理
  - 减少了UI组件间的耦合度，提高了系统的可维护性
- 账户渲染简化
  - account.rs 模块现在只处理基本的账户信息显示
  - 移除了设置页面的表单渲染、验证逻辑和数据持久化
  - 降低了内存占用和渲染开销

```mermaid
flowchart TD
OldUI["旧UI架构"] --> AccountPage["账户设置页面"]
OldUI --> ComplexMenu["复杂用户菜单"]
NewUI["新UI架构"] --> SimpleMenu["简化头像下拉菜单"]
NewUI --> BasicAccount["基础账户显示"]
ComplexMenu --> Removed["已移除"]
AccountPage --> Removed
```

**图表来源** 
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [account.rs:1-50](file://crates/aether-win32/src/render/account.rs#L1-L50)

**章节来源**
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [account.rs:1-50](file://crates/aether-win32/src/render/account.rs#L1-L50)

### 欢迎屏幕吉祥物图像处理改进

**更新** Windows编辑器欢迎屏幕吉祥物图像处理得到显著改进，采用嵌入式PNG资源确保跨构建环境的一致性。

- 嵌入式PNG资源管理
  - 吉祥物图像直接从二进制可执行文件中加载，不再依赖外部文件系统
  - 解决了CI构建环境和不同部署场景下的资源缺失问题
  - 确保了应用程序在所有环境下具有一致的视觉呈现
- 位图加载器优化
  - bitmap_loader.rs 模块支持从嵌入式资源加载PNG格式图像
  - 提供了统一的位图接口，简化了图像资源的访问和管理
  - 支持内存高效的图像解码和渲染
- 欢迎屏幕渲染增强
  - welcome.rs 模块集成了新的嵌入式图像加载机制
  - 保持了原有的欢迎屏幕布局和交互逻辑
  - 提升了图像加载的可靠性和性能

```mermaid
flowchart TD
OldMethod["旧方法：文件系统加载"] --> ExternalFile["外部PNG文件"]
ExternalFile --> LoadError["加载失败风险"]
LoadError --> Inconsistent["不一致的视觉呈现"]
NewMethod["新方法：嵌入式资源"] --> BinaryEmbedded["二进制内嵌PNG"]
BinaryEmbedded --> ReliableLoad["可靠的资源加载"]
ReliableLoad --> ConsistentVisual["一致的视觉呈现"]
ConsistentVisual --> CICompatible["CI构建兼容"]
```

**图表来源** 
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)

**章节来源**
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)

## 依赖关系分析
- 模块耦合
  - window.rs 依赖 window_setup、keyboard_handler、mouse_handler、render、ime、input
  - render.rs 依赖 render_context、dirty_rect、theme、layout、icons 等
  - render_context.rs 依赖 aether-render d2d factory/brush/text 缓存
- 外部依赖
  - Windows API：窗口、消息、DWM、GDI、HiDpi、IME、Direct2D、DirectWrite
  - aether-core：字符宽度、词法分析器语言枚举
  - aether-shared：AppSettings 持久化
- **新增** 浏览器和Agent模式依赖
  - webview2_com：WebView2 COM 接口
  - windows：Windows API 绑定
  - aether-ai-panel：AI Agent 功能模块

**更新** 由于新增了浏览器和Agent模式功能，依赖关系变得更加复杂。现在需要额外的 WebView2 运行时支持和 AI 面板模块。

```mermaid
graph LR
Win["window.rs"] --> Setup["window_setup.rs"]
Win --> Kbd["keyboard_handler.rs"]
Win --> Mse["mouse_handler.rs"]
Win --> Ime["ime.rs"]
Win --> Inp["input.rs"]
Win --> Rnd["render.rs"]
Rnd --> RCtx["render_context.rs"]
Rnd --> Dirty["dirty_rect.rs"]
Rnd --> Theme["theme.rs / aether-render/src/theme.rs"]
Rnd --> UserMenu["user_menu.rs"]
Rnd --> Welcome["welcome.rs"]
Welcome --> Bitmap["bitmap_loader.rs"]
Rnd --> Browser["browser.rs"]
Rnd --> AgentRight["agent_right_panel.rs"]
Rnd --> AgentSide["agent_sidebar.rs"]
AgentRight --> Layout["layout.rs"]
AgentSide --> Layout
Browser --> Layout
Layout --> AIAgent["ai_agent.rs"]
RCtx --> D2D["aether-render d2d"]
Browser --> WebView2["webview2_com"]
```

**图表来源** 
- [window.rs:1-373](file://crates/aether-win32/src/window.rs#L1-L373)
- [render.rs:1-800](file://crates/aether-win32/src/render.rs#L1-L800)
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)
- [aether-render_lib.rs:1-4](file://crates/aether-render/src/lib.rs#L1-L4)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [layout.rs:1-400](file://crates/aether-win32/src/layout.rs#L1-L400)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

**章节来源**
- [window.rs:1-373](file://crates/aether-win32/src/window.rs#L1-L373)
- [render.rs:1-800](file://crates/aether-win32/src/render.rs#L1-L800)
- [render_context.rs:1-226](file://crates/aether-win32/src/render_context.rs#L1-L226)
- [dirty_rect.rs:1-707](file://crates/aether-win32/src/dirty_rect.rs#L1-L707)
- [aether-render_lib.rs:1-4](file://crates/aether-render/src/lib.rs#L1-L4)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:1-651](file://crates/aether-win32/src/browser.rs#L1-L651)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [layout.rs:1-400](file://crates/aether-win32/src/layout.rs#L1-L400)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

## 性能考量
- 脏矩形优化
  - 按区域类型合并重叠矩形，数量超过阈值自动降级为全窗口重绘
  - 无状态变化时跳过渲染（RenderCommand::None），避免空转
- 裁剪策略
  - 多矩形并集裁剪减少无效绘制，失败回退包围盒保证稳定性
- 资源缓存
  - 画刷与文本格式缓存避免每帧创建 COM 对象
  - 设备丢失时快速重建并恢复缓存
- 命中区域记录
  - debug 构建下记录可点击区域，release 构建零开销
- **更新** 用户界面简化带来的性能提升
  - 移除了账户设置页面的复杂渲染逻辑，减少了渲染负载
  - 简化的用户菜单减少了事件处理和状态管理的开销
  - 整体UI架构的简化提高了响应性和内存效率
- **新增** 嵌入式图像资源的优势
  - 消除了文件系统I/O操作，提升了图像加载速度
  - 减少了运行时资源依赖，提高了应用程序的自包含性
  - 避免了CI构建环境中的资源路径解析问题
- **新增** 浏览器和Agent模式性能优化
  - WebView2 环境惰性初始化，避免启动时的性能开销
  - 浏览器实例按需创建，减少内存占用
  - Agent 面板组件采用增量渲染，只更新变化的部分
  - 智能体模式下的布局计算经过优化，减少重绘范围

[本节为通用指导，不直接分析具体文件]

## 故障排查指南
- 窗口未显示或黑屏
  - 检查 DPI 感知是否成功设置，DWM 背景效果是否启用
  - 确认渲染目标已初始化且尺寸有效
- 中文输入异常
  - 验证 IME 候选/合成窗口位置是否正确更新
  - 终端场景下确认是否临时解除了 IME 关联
- 频繁全量重绘
  - 检查脏矩形标记逻辑，确认是否误触发 FullRedraw
  - 观察是否有过多不相交脏矩形导致阈值触发
- 设备丢失导致崩溃
  - 捕获错误码并调用 handle_device_lost，重建渲染目标与缓存
- **更新** 用户菜单相关问题
  - 头像下拉菜单无法显示：检查用户菜单初始化逻辑
  - 菜单点击无响应：验证鼠标事件处理链路
- **更新** 欢迎屏幕图像问题
  - 吉祥物图像无法显示：检查嵌入式PNG资源是否正确编译到二进制文件
  - 图像显示异常：验证bitmap_loader的资源加载逻辑
  - CI构建环境问题：确认构建脚本正确包含了嵌入式资源
- **新增** 浏览器功能问题
  - WebView2 环境初始化失败：检查系统是否安装了 WebView2 Runtime
  - 浏览器标签页无法显示：验证控制器创建和边界同步逻辑
  - 网页加载缓慢：检查网络连接和 WebView2 配置
- **新增** Agent 模式问题
  - 模式切换无效：检查布局管理器的模式切换逻辑
  - Agent 面板渲染异常：验证右侧面板和侧边栏的渲染流程
  - AI 工具标记解析失败：检查 ai_agent.rs 中的解析逻辑

**章节来源**
- [window_setup.rs:18-86](file://crates/aether-win32/src/window/window_setup.rs#L18-L86)
- [render.rs:704-746](file://crates/aether-win32/src/render.rs#L704-L746)
- [render_context.rs:219-225](file://crates/aether-win32/src/render_context.rs#L219-L225)
- [ime.rs:208-243](file://crates/aether-win32/src/ime.rs#L208-L243)
- [dirty_rect.rs:387-426](file://crates/aether-win32/src/dirty_rect.rs#L387-L426)
- [user_menu.rs:1-100](file://crates/aether-win32/src/user_menu.rs#L1-L100)
- [welcome.rs:1-100](file://crates/aether-win32/src/welcome.rs#L1-L100)
- [bitmap_loader.rs:1-100](file://crates/aether-win32/src/bitmap_loader.rs#L1-L100)
- [browser.rs:324-350](file://crates/aether-win32/src/browser.rs#L324-L350)
- [agent_right_panel.rs:1-568](file://crates/aether-win32/src/render/agent_right_panel.rs#L1-L568)
- [agent_sidebar.rs:1-284](file://crates/aether-win32/src/render/agent_sidebar.rs#L1-L284)
- [layout.rs:137-171](file://crates/aether-win32/src/layout.rs#L137-L171)
- [ai_agent.rs:1-800](file://crates/aether-ai-panel/src/ai_agent.rs#L1-L800)

## 结论
牧羊人编辑器的 UI 系统采用清晰的 Win32 窗口管理与 Direct2D/DirectWrite 渲染分层设计，结合脏矩形与多矩形裁剪显著降低重绘成本。输入系统通过模块化键盘/鼠标/IME 处理提升可维护性，主题系统提供灵活的配色与语法着色方案。**最新更新** 通过移除账户设置页面功能、使用嵌入式PNG资源改进欢迎屏幕吉祥物图像处理，以及新增完整的浏览器功能和Agent模式支持，UI架构得到显著优化，进一步提升了系统的性能和跨构建环境的一致性。新增的 WebView2 嵌入式浏览器和 Agent 模式组件为编辑器带来了智能化的工作能力，使其能够更好地支持现代开发工作流程。遵循本文的最佳实践与性能建议，可进一步提升 UI 响应性与稳定性。

[本节为总结，不直接分析具体文件]

## 附录
- 术语
  - DPI：每英寸点数，影响 UI 缩放
  - DWM：桌面窗口管理器，提供 Acrylic/Mica 背景
  - IMM32：输入法管理器，用于 IME 集成
  - D2D/DWrite：Direct2D/DirectWrite，GPU 加速图形与文本渲染
  - PNG：便携式网络图形格式，用于高质量图像存储
  - WebView2：Microsoft Edge 内核的嵌入式浏览器控件
  - Agent 模式：AI 驱动的编程助手模式
  - 智能体模式：与 Agent 模式同义，强调 AI 代理的能力
- 参考路径
  - 窗口创建与设置：[window_setup.rs](file://crates/aether-win32/src/window/window_setup.rs)
  - 渲染主流程：[render.rs](file://crates/aether-win32/src/render.rs)
  - 渲染上下文：[render_context.rs](file://crates/aether-win32/src/render_context.rs)
  - 脏矩形系统：[dirty_rect.rs](file://crates/aether-win32/src/dirty_rect.rs)
  - 输入与 IME：[input.rs](file://crates/aether-win32/src/input.rs)、[ime.rs](file://crates/aether-win32/src/ime.rs)
  - 主题系统：[theme.rs](file://crates/aether-win32/src/theme.rs)、[aether-render/src/theme.rs](file://crates/aether-render/src/theme.rs)
  - **新增** 用户界面组件：[user_menu.rs](file://crates/aether-win32/src/user_menu.rs)、[account.rs](file://crates/aether-win32/src/render/account.rs)
  - **新增** 欢迎屏幕与图像处理：[welcome.rs](file://crates/aether-win32/src/welcome.rs)、[bitmap_loader.rs](file://crates/aether-win32/src/bitmap_loader.rs)
  - **新增** 嵌入式浏览器：[browser.rs](file://crates/aether-win32/src/browser.rs)
  - **新增** Agent 模式组件：[agent_right_panel.rs](file://crates/aether-win32/src/render/agent_right_panel.rs)、[agent_sidebar.rs](file://crates/aether-win32/src/render/agent_sidebar.rs)
  - **新增** 布局管理：[layout.rs](file://crates/aether-win32/src/layout.rs)
  - **新增** AI Agent 协议：[ai_agent.rs](file://crates/aether-ai-panel/src/ai_agent.rs)

[本节为附录，不直接分析具体文件]