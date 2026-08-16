# Aether Studio AI 协作测试指南

> 本文档面向 AI 助手与开发者：如何用 `tests/framework/` 驱动 GUI 自动化测试、
> 诊断失败、生成新用例。框架沉淀了历次测试踩过的所有环境坑（见"常见陷阱"）。

## 1. 框架总览

```
tests/
├── framework/
│   ├── AetherTest.psm1   # 核心层：生命周期/窗口/输入/截图/日志/hit regions/断言/报告
│   └── AetherAi.psm1     # AI 协作层：诊断包/动作脚本/像素断言/UI 状态/日志断言/智能操作
├── cases/                # GUI 用例（*.tests.ps1，可独立运行）
│   └── _template.tests.ps1   # AI 生成用例的起点模板
├── run_tests.ps1         # 统一入口：unit / gui / coverage / all
├── reports/              # 用例 JSON 报告（机器可读）
├── screenshots/<case>/   # 用例截图
├── diag/<case>-<时间戳>/ # 失败诊断包（New-AetherDiagBundle 生成）
├── gui_hit_regions.jsonl # debug 构建自动记录每帧可点击区域
└── tools/                # coverage / selfcheck 等辅助脚本
```

## 2. 快速开始（AI 写第一个用例）

1. 复制 `tests/cases/_template.tests.ps1` 为 `<功能名>.tests.ps1`
2. 在文件头注释填写元数据：`Target`（被测功能）、`Feature`（需求条目）、`Scenario`、`Expect`
3. 用 `New-AetherTestWorkspace` 声明测试文件，用 `Invoke-TestStep` 组织步骤
4. 运行：`pwsh -File tests\cases\<功能名>.tests.ps1`
5. 报告输出到 `tests/reports/<功能名>.json`（含每步耗时与失败详情），截图在 `tests/screenshots/<功能名>/`

用例骨架（详见模板）：

```powershell
Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force
$L = Get-AetherLayoutConstants
Start-TestCase "case_name"
$ws = New-AetherTestWorkspace -Files @{ "main.rs" = 'fn main() {}' }
$proc = Start-AetherApp -Folder $ws          # 自动信任工作区 + new_window 隔离
$win = Get-AetherWindow -Process $proc -Isolate
$k = $win.Scale2
Invoke-TestStep "步骤名" { ...断言... }
Stop-AetherApp $proc
exit (Complete-TestCase)
```

## 3. 坐标与 DPI 约定（务必遵守）

- **物理坐标 = 名义值 × Scale²**（`$win.Scale2`，150% 屏为 2.25）。
  应用布局常量本身已乘一次 dpi_scale，渲染坐标是 dips，因此物理像素要乘平方。
- 布局名义常量：`Get-AetherLayoutConstants`（TITLE_BAR=28、ACTIVITY_W=40、SIDEBAR_W=200、
  HEADER_H=24、ROW_H=15、STATUS_H=16...），与 `crates/aether-win32/src/layout.rs` 同步。
- 文件树第 i 个节点行中心：
  `NX ($L.TITLE_BAR + $L.HEADER_H + 6 + $L.ROW_H * ($i + 1) + $L.ROW_H / 2)`（目录优先 + 字母序）。
- 状态栏 y ≈ `NX ($L.TITLE_BAR + 窗口内容高 - $L.STATUS_H)`。

## 4. 输入方式选择

### 4.1 鼠标操作

| 方式 | 函数 | 适用 |
|---|---|---|
| PostMessage 点击 | `Send-AetherClickMsg -Hwnd -X -Y [-Right]` | **首选**：不受前台锁定/窗口遮挡影响 |
| PostMessage 双击 | `Send-AetherDoubleClickMsg -Hwnd -X -Y [-Right]` | 双击选词、双击打开 |
| PostMessage 中键 | `Send-AetherMiddleClickMsg -Hwnd -X -Y` | 中键关闭标签 |
| PostMessage 滚轮 | `Send-AetherMouseWheel -Hwnd -X -Y -Delta [-Horizontal] [-Shift] [-Ctrl]` | 滚动/缩放/横向滚动 |
| PostMessage 移动 | `Send-AetherMouseMoveMsg -Hwnd -X -Y` | 触发 hover、拖拽过程 |
| PostMessage 拖拽 | `Send-AetherDrag -Hwnd -FromX -FromY -ToX -ToY [-Steps] [-Right]` | 文件拖拽、标签重排、面板调整 |
| 真实鼠标 | `Invoke-AetherClick -Window -X -Y [-Right]` | 需要系统级行为（拖拽到窗口外） |

### 4.2 键盘操作

| 方式 | 函数 | 适用 |
|---|---|---|
| PostMessage 文本 | `Send-AetherTextMsg -Hwnd -Text` | **首选**：逐字符注入 WM_CHAR |
| PostMessage 按键 | `Send-AetherKeyMsg -Hwnd -Key` | 单键：{ENTER}/{ESC}/{F2}/{UP}... |
| PostMessage 组合键 | `Send-AetherHotkey -Hwnd -Modifiers -Key` | **首选**：Ctrl+S/Ctrl+Shift+P 等 |
| SendKeys（真实键盘） | `Send-AetherKeys` / `Send-AetherText` | 仅当需要系统级焦点行为时（慎用） |

**支持的按键名称**（`Send-AetherKeyMsg` / `Send-AetherHotkey`）：
- 控制键：`{ENTER}` `{ESC}` `{BACKSPACE}` `{TAB}` `{DELETE}` `{INSERT}` `{SPACE}`
- 导航键：`{UP}` `{DOWN}` `{LEFT}` `{RIGHT}` `{HOME}` `{END}` `{PAGEUP}` `{PAGEDOWN}`
- 功能键：`{F1}` ~ `{F12}`
- 字母键：`A` ~ `Z`（直接写字母）
- 数字键：`0` ~ `9`（直接写数字）
- 符号键：`,` `.` `/` `` ` `` `+` `-`

**组合键修饰符**（`Send-AetherHotkey -Modifiers`）：
- `@('Ctrl')` — Ctrl
- `@('Shift')` — Shift
- `@('Alt')` — Alt
- `@('Ctrl','Shift')` — Ctrl+Shift
- `@('Ctrl','Alt')` — Ctrl+Alt
- `@('Ctrl','Shift','Alt')` — Ctrl+Shift+Alt

**常用快捷键示例**：
```powershell
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'S'           # 保存
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl','Shift') -Key 'P'   # 命令面板
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'B'           # 切换侧栏
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key '`'           # 切换终端
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key ','           # 设置
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'F'           # 查找
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'Z'           # 撤销
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl','Shift') -Key 'Z'   # 重做
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'A'           # 全选
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'C'           # 复制
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'V'           # 粘贴
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'W'           # 关闭标签
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'Tab'         # 下一标签
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl','Shift') -Key 'Tab' # 上一标签
Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key '1'           # 跳转标签 1
```

### 4.3 窗口操作

| 函数 | 说明 |
|---|---|
| `Resize-AetherWindow -Window -Width -Height` | 调整窗口大小（触发 WM_SIZE） |
| `Move-AetherWindow -Window -X -Y` | 移动窗口位置 |
| `Set-AetherWindowState -Window -State` | Normal/Minimized/Maximized/Restored |
| `Close-AetherWindow -Window` | 发送 WM_CLOSE（触发正常关闭流程） |

**重要**：`SendKeys` 系列依赖前台窗口焦点——用户窗口在前台时输入会丢失。
全局键盘钩子能转发部分控制键（F2/ENTER/ESC），但普通文本字符不会，
因此自动化测试一律使用 PostMessage 注入（`Send-AetherClickMsg` +
`Send-AetherTextMsg` + `Send-AetherKeyMsg` + `Send-AetherHotkey`）。
注意：`Send-AetherClickMsg` 的坐标是**窗口内客户区物理坐标**；若用户正开着另一个
Aether 窗口且位置重叠，请先用 `-Isolate` 移动测试窗口。

## 5. 观测手段（AI 分析的输入）

- **日志**：`Get-AetherLog -Tail N [-Pattern X]`，位于 `%TEMP%\Aether\logs\aether.YYYY-MM-DD`
  （无 `.log` 扩展名，按天轮转，同一天多会话追加）。
  异步行为验证：`Wait-AetherLogEvent -Pattern "已打开" -TimeoutMs 5000`
  （只观察调用后**新增**的日志行，不会误匹配历史会话）。
- **hit regions**：`Read-AetherHitRegions -Contains @{X;Y}` —— debug 构建每帧把可点击
  区域写入 `tests/gui_hit_regions.jsonl`，可验证"点击位置预期落在哪个按钮/节点"。
  状态栏语言区域（`status:Rust` 等）是"文件打开成功"的可靠证据——
  **status_message（"已打开: xxx"）不写日志**，不要用日志断言它。
  注意 hit regions 是历史累计的，断言前可删除 jsonl 建立干净基线。
- **截图**：`Save-AetherScreenshot -Window -Name`；像素断言 `Test-AetherPixelRegion`。
- **UI 状态**：`Get-AetherUiState -Process` 返回日志尾部 + hit regions + CPU/内存，
  供探索式测试的"观察-决策"循环。
- **编辑器状态**：`Get-AetherEditorState -Process` 返回标签数、状态栏项、最近日志。
- **报告**：`tests/reports/<case>.json`，字段：Steps（含 duration_ms/error）、
  Screenshots、Env（OS/DPI/构建环境）。

## 6. 探索式测试（AI 自主交互）

用动作脚本 DSL 描述交互序列，`Invoke-AetherActionScript` 逐步执行并记录：

```powershell
$actions = @(
    @{type='click';   x=$rowLabelX; y=(RowCenterY 0)},
    @{type='keys';    text='hello.rs'},
    @{type='key';     key='{ENTER}'},
    @{type='hotkey';  modifiers=@('Ctrl'); key='S'},
    @{type='wheel';   x=500; y=300; delta=-120},
    @{type='drag';    from_x=100; from_y=200; to_x=300; to_y=400},
    @{type='shot';    name='created'},
    @{type='expect';  pattern='DIAG|error'},
    @{type='wait';    ms=800}
)
$results = Invoke-AetherActionScript -Window $win -Actions $actions
# 每步返回 @{ type; ok; note; duration_ms; screenshot }，失败可打包诊断
```

### 完整动作类型参考

| 类型 | 参数 | 说明 |
|---|---|---|
| `click` | `x; y; [right]` | PostMessage 点击 |
| `dblclick` | `x; y; [right]` | PostMessage 双击 |
| `mclick` | `x; y` | PostMessage 中键点击 |
| `hover` | `x; y` | 真实鼠标移动（触发 hover） |
| `move` | `x; y` | PostMessage 鼠标移动 |
| `wheel` | `x; y; delta; [horizontal]; [shift]; [ctrl]` | 滚轮（delta=±120 倍数） |
| `drag` | `from_x; from_y; to_x; to_y; [steps]; [right]` | 拖拽 |
| `keys` | `text` | PostMessage 文本注入 |
| `key` | `key` | PostMessage 按键（{ENTER} 等） |
| `hotkey` | `modifiers; key` | 组合键（modifiers=@('Ctrl','Shift')） |
| `wait` | `ms` | 等待 |
| `shot` | `name` | 截图 |
| `expect` | `pattern; [timeout_ms]` | 等待日志模式 |
| `resize` | `width; height` | 调整窗口大小 |
| `movewin` | `x; y` | 移动窗口 |
| `winstate` | `state` | 窗口状态（Normal/Minimized/Maximized/Restored） |
| `closewin` | — | 发送 WM_CLOSE |

## 7. 智能操作（AI 语义化交互）

框架提供基于 hit regions 的智能操作，AI 可以用语义名称而非坐标来交互：

```powershell
# 智能查找并点击（根据动作名称自动定位）
Invoke-AetherSmartClick -Window $win -ActionLike "new_file"
Invoke-AetherSmartClick -Window $win -ActionLike "tab:*" -Right   # 右键标签

# 等待 UI 元素出现
$region = Wait-AetherHitRegion -ActionLike "status:Rust" -TimeoutMs 8000
if ($region) { Write-Host "文件已打开" }

# 获取编辑器状态摘要
$state = Get-AetherEditorState -Process $proc
Write-Host "标签数: $($state.TabCount), 状态栏: $($state.StatusBarItems -join ', ')"
```

## 8. 失败诊断流程

1. 用例失败后（`Invoke-TestStep` 已捕获异常，报告记录 error），
2. 执行 `New-AetherDiagBundle -CaseName <case>`，
3. 生成 `tests/diag/<case>-<时间戳>/`：manifest.json（环境+失败步骤+文件清单）、
   report.json、screenshots/、app.log（尾部 200 行）、hit_regions.jsonl、process.txt，
4. AI 据此分析：日志找时序/异常栈，截图看视觉状态，hit regions 验证命中区域。

## 9. 性能测试（回归基线）

- 步骤耗时自动记录在报告中（`duration_ms`），多次运行可对比趋势。
- 应用日志埋点约定：耗时数据用 `tracing::info!(ms=..., "DIAG <阶段>")` 输出，
  测试用 `Get-AetherLog -Pattern "DIAG"` 采集，`Wait-AetherLogEvent` 等待异步完成。
- 示例：验证"点击文件树切换标签零开销"——点击后断言 `switch_tab` 路径无重新解析
  （日志无新高亮请求），或直接测 `Get-AetherLog -Pattern "DIAG load_file"` 的 ms 值。

## 10. 常见陷阱（历次测试沉淀）

1. **Start-Process 剥离 JSON 引号**：`-Folder` 参数必须经 `Start-AetherApp` 的
   .NET ProcessStartInfo.ArgumentList 传递，否则工作区打不开（回退 last_workspace）。
2. **工作区信任弹窗**：未信任目录会在 open_folder 弹模态确认框阻塞流程。
   `Start-AetherApp -Folder` 自动写入 `%APPDATA%\Aether\trusted_folders.txt`。
3. **单实例互斥**：用户已开编辑器时，新实例会把参数转发给旧窗口后退出。
   框架强制 `new_window=true` 避免。
4. **窗口矩形持久化**：新窗口恢复到上次保存的位置，与用户窗口重叠。
   用 `Get-AetherWindow -Isolate` 移到 (40,40)。
5. **前台锁定与键盘焦点**：Windows 禁止后台进程抢占前台，真实鼠标点击可能落入其他窗口，
   SendKeys 文本会丢失。**一律用 PostMessage 注入**（`Send-AetherClickMsg` /
   `Send-AetherTextMsg` / `Send-AetherKeyMsg` / `Send-AetherHotkey`），不依赖前台焦点。
6. **冰冻态**：窗口最小化或失焦 10 分钟进入 Frozen（关停 LSP、裁剪缓存）。
   长用例注意防冻（周期发输入）；测冰冻恢复用例时以最小化触发。
   可用 `Set-AetherWindowState -State Minimized` 主动触发冰冻态测试。
7. **日志文件名无 .log 扩展名**：`Get-AetherLog` 已处理，勿手写 `*.log` 过滤。
8. **status_message 不写日志**："已打开: xxx" 等状态消息只在状态栏显示，
   验证文件打开用 hit regions 状态栏语言（`status:Rust`）。
9. **DPI 变化**：测试窗口 MoveWindow 到不同 DPI 显示器会收到 WM_DPICHANGED，
   重新 `Get-AetherWindow` 获取最新 Scale2。
10. **debug 构建才有 DIAG/hit regions**：性能与命中验证需 debug 构建；
    release 构建零开销（hit_test 空实现）。
11. **滚轮坐标是屏幕坐标**：`Send-AetherMouseWheel` 内部自动将窗口内坐标转为屏幕坐标，
    直接传窗口内物理坐标即可。
12. **组合键顺序**：`Send-AetherHotkey` 先按修饰键再按目标键，释放时逆序，
    与真实键盘行为一致。应用通过 `GetKeyState` 检测修饰键状态。
13. **WM_DROPFILES 无法 PostMessage**：文件拖放需要构造 HDROP 句柄，
    `Send-AetherDropFiles` 当前抛出异常提示替代方案（Ctrl+O / Start-AetherApp -Folder）。

## 11. 运行入口

```powershell
pwsh -File tests\run_tests.ps1 -Suite gui                  # 全部 GUI 用例
pwsh -File tests\run_tests.ps1 -Suite gui -Case explorer_inline_input
pwsh -File tests\run_tests.ps1 -Suite unit                 # cargo test --workspace
pwsh -File tests\run_tests.ps1 -Suite ai                   # 框架自检
pwsh -File tests\run_tests.ps1 -Suite all
```

## 12. 快捷键速查表（应用支持的全部快捷键）

| 快捷键 | 功能 |
|---|---|
| Ctrl+O | 打开文件 |
| Ctrl+K | 打开文件夹 |
| Ctrl+S | 保存 |
| Ctrl+Shift+S | 另存为 |
| Ctrl+N | 新建项目 |
| Ctrl+Space | LSP 补全 |
| Ctrl+B | 切换侧栏 |
| Ctrl+P | 命令面板 |
| Ctrl+Shift+P | 命令面板（> 前缀） |
| Ctrl+` | 切换终端 |
| Ctrl+J | 切换底部面板 |
| Ctrl+, | 设置 |
| Ctrl+Shift+E | 资源管理器视图 |
| Ctrl+Shift+G | 源代码管理视图 |
| Ctrl+Shift+V | Markdown 预览 |
| Ctrl+= / Ctrl+- / Ctrl+0 | 字体缩放/重置 |
| Ctrl+G | 命令面板（: 前缀） |
| Ctrl+C / Ctrl+X / Ctrl+V / Ctrl+A | 复制/剪切/粘贴/全选 |
| Ctrl+Shift+A | 切换 AI 面板 |
| Ctrl+F / Ctrl+H | 查找/替换 |
| Ctrl+Z / Ctrl+Y | 撤销/重做 |
| Ctrl+Shift+Z | 重做 |
| Ctrl+Tab / Ctrl+Shift+Tab | 下一/上一标签 |
| Ctrl+W / Ctrl+F4 | 关闭标签 |
| Ctrl+Shift+T | 恢复关闭的标签 |
| Ctrl+1 ~ Ctrl+9 | 跳转标签 1-9 |
| Ctrl+Left / Ctrl+Right | 词级移动 |
| Ctrl+Shift+Left/Right | 词级选择 |
| Ctrl+Home / Ctrl+End | 文件首/末 |
| Ctrl+D | 添加下一个相同词光标 |
| Ctrl+/ | 行注释 |
| Ctrl+Shift+I | 内联补全 |
| Ctrl+Alt+Up / Ctrl+Alt+Down | 列光标 |
| Ctrl+L | 终端清屏（终端聚焦时） |
| F2 | 重命名 |
| F3 | 查找下一个 |
