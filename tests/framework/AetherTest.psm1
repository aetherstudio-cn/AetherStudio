# AetherTest.psm1 — Aether Studio GUI/进程测试框架核心模块（AI 协作版）
#
# 能力：应用生命周期（构建/启动/停止）、窗口管理、鼠标键盘模拟（真实 + 消息注入）、
#       截图、临时测试工作区、日志观测、hit regions 读取、断言与用例报告（JSON + 控制台）。
#
# 用例协议（tests/cases/*.tests.ps1）：
#   Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
#   Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force   # AI 协作层（可选）
#   Start-TestCase "case_name"
#   Invoke-TestStep "步骤描述" { ...; Assert-Condition ... }
#   exit (Complete-TestCase)
#
# 坐标约定：本应用渲染物理尺寸 = 名义值(布局常量) × dpi_scale²（150% 屏为 2.25 倍）。
#   窗口内相对坐标用 Scale2 换算；布局常量见 Get-AetherLayoutConstants。
#
# 可靠性约定（AI 生成用例时必须遵守，见 tests/ai/AI_TESTING.md）：
#   - 启动工作区用 -TrustFolder 预写信任列表，避免模态信任弹窗阻塞；
#   - 测试窗口与用户窗口可能重叠（持久化窗口矩形），用 Get-AetherWindow -Isolate 隔离；
#   - 模拟点击优先 Send-AetherClickMsg（PostMessage 注入，不受前台/遮挡影响）；
#   - 启动参数经 .NET ProcessStartInfo.ArgumentList 传递（Start-Process 会剥离 JSON 引号）。

Set-StrictMode -Version Latest

$script:ProjectRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
$script:AppExe = Join-Path $script:ProjectRoot "target\x86_64-pc-windows-msvc\debug\aether-app.exe"
$script:ScreenshotDir = Join-Path $script:ProjectRoot "tests\screenshots"
$script:ReportDir = Join-Path $script:ProjectRoot "tests\reports"
$script:HitRegionsFile = Join-Path $script:ProjectRoot "tests\gui_hit_regions.jsonl"
$script:LogDir = Join-Path $env:TEMP "Aether\logs"
$script:TrustFile = Join-Path $env:APPDATA "Aether\trusted_folders.txt"

Add-Type -AssemblyName System.Drawing

if (-not ("AetherWin32" -as [type])) {
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class AetherWin32 {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool SendMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern short GetKeyState(int nVirtKey);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr hWnd, ref POINT lpPoint);
    [DllImport("user32.dll")] public static extern IntPtr GetSystemMetrics(int nIndex);
    [DllImport("kernel32.dll")] public static extern IntPtr GetModuleHandleW(string lpModuleName);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    public struct POINT { public int X; public int Y; }
    public const uint LEFTDOWN = 0x0002, LEFTUP = 0x0004, RIGHTDOWN = 0x0008, RIGHTUP = 0x0010;
    public const uint MIDDLEDOWN = 0x0020, MIDDLEUP = 0x0040;
    public const uint WM_LBUTTONDOWN = 0x0201, WM_LBUTTONUP = 0x0202, WM_RBUTTONDOWN = 0x0204, WM_RBUTTONUP = 0x0205;
    public const uint WM_MBUTTONDOWN = 0x0207, WM_MBUTTONUP = 0x0208;
    public const uint WM_LBUTTONDBLCLK = 0x0203, WM_RBUTTONDBLCLK = 0x0206, WM_MBUTTONDBLCLK = 0x0209;
    public const uint WM_MOUSEMOVE = 0x0200, WM_MOUSEWHEEL = 0x020A, WM_MOUSEHWHEEL = 0x020E;
    public const uint WM_KEYDOWN = 0x0100, WM_KEYUP = 0x0101, WM_CHAR = 0x0102;
    public const uint WM_SYSKEYDOWN = 0x0104, WM_SYSKEYUP = 0x0105;
    public const uint WM_SIZE = 0x0005, WM_MOVE = 0x0003, WM_CLOSE = 0x0010;
    public const uint WM_DROPFILES = 0x0233;
    public const int SIZE_RESTORED = 0, SIZE_MINIMIZED = 1, SIZE_MAXIMIZED = 2;
    public const int SW_HIDE = 0, SW_SHOWNORMAL = 1, SW_SHOWMINIMIZED = 2, SW_SHOWMAXIMIZED = 3, SW_RESTORE = 9;
    // 虚拟键码
    public const int VK_SHIFT = 0x10, VK_CONTROL = 0x11, VK_MENU = 0x12;  // Alt
    public const int VK_LSHIFT = 0xA0, VK_RSHIFT = 0xA1, VK_LCONTROL = 0xA2, VK_RCONTROL = 0xA3;
    public const int VK_LMENU = 0xA4, VK_RMENU = 0xA5;  // Alt
    public const int VK_RETURN = 0x0D, VK_ESCAPE = 0x1B, VK_BACK = 0x08, VK_TAB = 0x09;
    public const int VK_DELETE = 0x2E, VK_INSERT = 0x2D;
    public const int VK_UP = 0x26, VK_DOWN = 0x28, VK_LEFT = 0x25, VK_RIGHT = 0x27;
    public const int VK_HOME = 0x24, VK_END = 0x23, VK_PRIOR = 0x21, VK_NEXT = 0x22;  // PageUp/PageDown
    public const int VK_SPACE = 0x20;
    public const int VK_F1 = 0x70, VK_F2 = 0x71, VK_F3 = 0x72, VK_F4 = 0x73, VK_F5 = 0x74, VK_F6 = 0x75;
    public const int VK_F7 = 0x76, VK_F8 = 0x77, VK_F9 = 0x78, VK_F10 = 0x79, VK_F11 = 0x7A, VK_F12 = 0x7B;
    public const int VK_A = 0x41, VK_B = 0x42, VK_C = 0x43, VK_D = 0x44, VK_E = 0x45, VK_F = 0x46;
    public const int VK_G = 0x47, VK_H = 0x48, VK_I = 0x49, VK_J = 0x4A, VK_K = 0x4B, VK_L = 0x4C;
    public const int VK_M = 0x4D, VK_N = 0x4E, VK_O = 0x4F, VK_P = 0x50, VK_Q = 0x51, VK_R = 0x52;
    public const int VK_S = 0x53, VK_T = 0x54, VK_U = 0x55, VK_V = 0x56, VK_W = 0x57, VK_X = 0x58;
    public const int VK_Y = 0x59, VK_Z = 0x5A;
    public const int VK_0 = 0x30, VK_1 = 0x31, VK_2 = 0x32, VK_3 = 0x33, VK_4 = 0x34;
    public const int VK_5 = 0x35, VK_6 = 0x36, VK_7 = 0x37, VK_8 = 0x38, VK_9 = 0x39;
    public const int VK_OEM_COMMA = 0xBC, VK_OEM_PERIOD = 0xBE, VK_OEM_2 = 0xBF;  // , . /
    public const int VK_OEM_3 = 0xC0;  // `
    public const int VK_OEM_PLUS = 0xBB, VK_OEM_MINUS = 0xBD;
}
"@
}
[AetherWin32]::SetProcessDPIAware() | Out-Null

# ---------------------------------------------------------------- 布局常量

function Get-AetherLayoutConstants {
    <# 返回布局名义常量（与 crates/aether-win32/src/layout.rs 保持一致），AI 用例据此计算点击坐标 #>
    [pscustomobject]@{
        TITLE_BAR    = 28.0   # TITLE_BAR_HEIGHT
        MENU_BAR     = 0.0    # MENU_BAR_HEIGHT（合并到标题栏）
        ACTIVITY_W   = 40.0   # ACTIVITY_BAR_WIDTH
        SIDEBAR_W    = 200.0  # SIDEBAR_WIDTH
        HEADER_H     = 24.0   # FILE_TREE_HEADER_HEIGHT
        ROW_H        = 15.0   # FILE_TREE_ROW_HEIGHT
        TAB_BAR_H    = 30.0   # TAB_BAR_HEIGHT
        STATUS_H     = 16.0   # STATUS_BAR_HEIGHT
    }
}

# ---------------------------------------------------------------- 应用生命周期

function Build-AetherApp {
    <# 构建 debug 版 aether-app（增量，已最新时秒回） #>
    Push-Location $script:ProjectRoot
    try {
        cargo build -p aether-win32 2>&1 | Select-Object -Last 1 | Write-Host
        if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }
    } finally { Pop-Location }
}

function Add-AetherTrustedFolder {
    <# 将工作区写入信任列表（%APPDATA%\Aether\trusted_folders.txt）。
       未信任的工作区会在 open_folder 时弹模态确认框，阻塞启动流程。 #>
    param([Parameter(Mandatory)][string]$Folder)
    $key = $Folder.ToLower()
    $lines = @()
    if (Test-Path $script:TrustFile) {
        $lines = @(Get-Content $script:TrustFile | Where-Object { $_ -and $_.Trim() })
    }
    if ($lines -notcontains $key) {
        New-Item -ItemType Directory -Force -Path (Split-Path $script:TrustFile -Parent) | Out-Null
        Add-Content -Path $script:TrustFile -Value $key
    }
}

function Start-AetherApp {
    <# 启动应用并等待主窗口就绪。
       -Folder 指定启动时打开的工作区（自动信任 + new_window 隔离）。
       注意：经 .NET ProcessStartInfo.ArgumentList 传递参数，
       Start-Process -ArgumentList 会剥离 JSON 中的双引号导致工作区打不开。 #>
    param(
        [string]$Folder,
        [int]$TimeoutSec = 15,
        [switch]$SkipTrust
    )
    if (-not (Test-Path $script:AppExe)) { throw "未找到 $script:AppExe，请先 Build-AetherApp" }

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $script:AppExe
    if ($Folder) {
        if (-not $SkipTrust) { Add-AetherTrustedFolder -Folder $Folder }
        # new_window=true：避免单实例互斥把参数转发给用户已打开的窗口后本进程退出
        $launchJson = @{ paths = @($Folder); new_window = $true; goto = $null; wait = $false } | ConvertTo-Json -Compress
        $psi.ArgumentList.Add("--aether-launch-args")
        $psi.ArgumentList.Add($launchJson)
    }
    $proc = [System.Diagnostics.Process]::Start($psi)

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
        $proc.Refresh()
        if ($proc.HasExited) { throw "应用启动后立即退出 ExitCode=$($proc.ExitCode)" }
        if ($proc.MainWindowHandle -ne 0) {
            Start-Sleep -Milliseconds 1500   # 等首帧渲染稳定
            return $proc
        }
    }
    throw "等待主窗口超时（${TimeoutSec}s）"
}

function Stop-AetherApp {
    param([Parameter(Mandatory)]$Process)
    if ($Process -and -not $Process.HasExited) {
        $Process.CloseMainWindow() | Out-Null
        Start-Sleep -Seconds 1
        if (-not $Process.HasExited) { $Process.Kill() }
    }
}

# ---------------------------------------------------------------- 窗口与几何

function Get-AetherWindow {
    <# 恢复并前置主窗口，返回 @{ Hwnd; Rect; ClientRect; Dpi; Scale; Scale2 } #>
    param(
        $Process,
        [switch]$Isolate
    )
    if (-not $Process) {
        $Process = Get-Process aether-app -ErrorAction Stop |
            Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    }
    $hwnd = $Process.MainWindowHandle
    [AetherWin32]::ShowWindow($hwnd, 9) | Out-Null   # SW_RESTORE
    [AetherWin32]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 800
    $rect = New-Object AetherWin32+RECT
    [AetherWin32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
    $client = New-Object AetherWin32+RECT
    [AetherWin32]::GetClientRect($hwnd, [ref]$client) | Out-Null
    $dpi = [AetherWin32]::GetDpiForWindow($hwnd)
    if ($dpi -le 0) { $dpi = 96 }
    $s = $dpi / 96.0

    if ($Isolate) {
        # 测试窗口可能恢复到了与用户窗口相同的位置（持久化窗口矩形），
        # 移动到 (40,40) 避免遮挡；PostMessage 点击不受位置影响，
        # 但真实鼠标点击与截图需要窗口在屏幕可见区域。
        [AetherWin32]::MoveWindow($hwnd, 40, 40, $rect.Right - $rect.Left, $rect.Bottom - $rect.Top, $true) | Out-Null
        Start-Sleep -Milliseconds 500
        [AetherWin32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
        [AetherWin32]::SetForegroundWindow($hwnd) | Out-Null
        Start-Sleep -Milliseconds 500
    }

    [pscustomobject]@{
        Hwnd        = $hwnd
        Rect        = $rect
        ClientRect  = $client
        Dpi         = $dpi
        Scale       = $s
        # 本应用布局常量的物理换算系数（渲染目标 DPI 与手动缩放叠加）
        Scale2      = $s * $s
        IsForeground = ([AetherWin32]::GetForegroundWindow() -eq $hwnd)
    }
}

# ---------------------------------------------------------------- 输入模拟

function Invoke-AetherClick {
    <# 真实鼠标单击（窗口内相对物理坐标）。-Right 右键。
       注意：若测试窗口不在前台（用户窗口遮挡/前台锁定），点击可能落入其他窗口，
       此时应改用 Send-AetherClickMsg（PostMessage 注入）。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y,
        [switch]$Right
    )
    $px = $Window.Rect.Left + $X
    $py = $Window.Rect.Top + $Y
    [AetherWin32]::SetCursorPos($px, $py) | Out-Null
    Start-Sleep -Milliseconds 120
    if ($Right) {
        [AetherWin32]::mouse_event([AetherWin32]::RIGHTDOWN, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
        [AetherWin32]::mouse_event([AetherWin32]::RIGHTUP, 0, 0, 0, [UIntPtr]::Zero)
    } else {
        [AetherWin32]::mouse_event([AetherWin32]::LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
        [AetherWin32]::mouse_event([AetherWin32]::LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 400
}

function Send-AetherClickMsg {
    <# 通过 PostMessage 向窗口注入点击（窗口内相对物理坐标），
       不受前台/遮挡/前台锁定影响，是自动化测试的首选输入方式。
       坐标 = 客户区物理像素 = 名义值 × Scale2。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y,
        [switch]$Right
    )
    $lParam = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))
    $down = if ($Right) { [AetherWin32]::WM_RBUTTONDOWN } else { [AetherWin32]::WM_LBUTTONDOWN }
    $up   = if ($Right) { [AetherWin32]::WM_RBUTTONUP }   else { [AetherWin32]::WM_LBUTTONUP }
    [AetherWin32]::PostMessageW($Hwnd, $down, [IntPtr]1, $lParam) | Out-Null
    Start-Sleep -Milliseconds 60
    [AetherWin32]::PostMessageW($Hwnd, $up, [IntPtr]0, $lParam) | Out-Null
    Start-Sleep -Milliseconds 400
}

function Send-AetherKeys {
    <# 发送 SendKeys 序列（{F2}/{ESC}/{ENTER} 等控制键原样传入） #>
    param([Parameter(Mandatory)][string]$Keys, [int]$DelayMs = 400)
    $shell = New-Object -ComObject WScript.Shell
    $shell.SendKeys($Keys)
    Start-Sleep -Milliseconds $DelayMs
}

function Send-AetherText {
    <# 发送字面文本：自动转义 SendKeys 元字符 +^%~(){}[]。
       注意：SendKeys 依赖前台窗口焦点，用户窗口在前台时文本可能丢失。
       自动化测试请优先使用 Send-AetherTextMsg（PostMessage 注入，不受焦点影响）。 #>
    param([Parameter(Mandatory)][string]$Text, [int]$DelayMs = 400)
    $escaped = ($Text.ToCharArray() | ForEach-Object {
        if ($_ -match '[+^%~(){}\[\]]') { "{$_}" } else { "$_" }
    }) -join ''
    Send-AetherKeys -Keys $escaped -DelayMs $DelayMs
}

function Send-AetherTextMsg {
    <# 通过 PostMessage 注入 WM_CHAR 逐字符输入文本（首选输入方式）：
       不依赖前台焦点，用户窗口在前台时依然可靠。
       文本进入应用后由 char_input 路由（文件树输入行/编辑器/AI 输入框等）。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][string]$Text,
        [int]$DelayMs = 40
    )
    foreach ($ch in $Text.ToCharArray()) {
        [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_CHAR, [IntPtr][int]$ch, [IntPtr]0) | Out-Null
        Start-Sleep -Milliseconds $DelayMs
    }
    Start-Sleep -Milliseconds 300
}

function Send-AetherKeyMsg {
    <# 通过 PostMessage 注入按键（WM_KEYDOWN/UP），不依赖前台焦点。
       -Key 支持 {ENTER}/{ESC}/{F1..F12}/{BACKSPACE}/{TAB}/{DELETE}/{UP}/{DOWN}/{LEFT}/{RIGHT}/{HOME}/{END}/{PAGEUP}/{PAGEDOWN}/{SPACE}/{INSERT}。
       WM_KEYDOWN 会经消息循环 TranslateMessage 转换为 WM_CHAR（如 ENTER），
       与真实键盘路径一致。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][string]$Key,
        [int]$DelayMs = 200
    )
    $vk = Get-AetherVirtualKeyCode -Key $Key
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYDOWN, [IntPtr]$vk, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds 30
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYUP, [IntPtr]$vk, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds $DelayMs
}

function Get-AetherVirtualKeyCode {
    <# 将按键名称转换为虚拟键码。内部辅助函数，不导出。 #>
    param([Parameter(Mandatory)][string]$Key)
    $k = $Key.ToUpper()
    # 处理 {XXX} 格式
    if ($k -match '^\{(.+)\}$') { $k = $Matches[1] }
    switch -Regex ($k) {
        '^ENTER$|^RETURN$'     { return [AetherWin32]::VK_RETURN }
        '^ESC(APE)?$'          { return [AetherWin32]::VK_ESCAPE }
        '^BACK(SPACE)?$'       { return [AetherWin32]::VK_BACK }
        '^TAB$'                { return [AetherWin32]::VK_TAB }
        '^DEL(ETE)?$'          { return [AetherWin32]::VK_DELETE }
        '^INS(ERT)?$'          { return [AetherWin32]::VK_INSERT }
        '^UP$'                 { return [AetherWin32]::VK_UP }
        '^DOWN$'               { return [AetherWin32]::VK_DOWN }
        '^LEFT$'               { return [AetherWin32]::VK_LEFT }
        '^RIGHT$'              { return [AetherWin32]::VK_RIGHT }
        '^HOME$'               { return [AetherWin32]::VK_HOME }
        '^END$'                { return [AetherWin32]::VK_END }
        '^PAGEUP$|^PGUP$'      { return [AetherWin32]::VK_PRIOR }
        '^PAGEDOWN$|^PGDN$'    { return [AetherWin32]::VK_NEXT }
        '^SPACE$'              { return [AetherWin32]::VK_SPACE }
        '^F(\d{1,2})$'         { return [AetherWin32]::VK_F1 + [int]$Matches[1] - 1 }
        '^SHIFT$'              { return [AetherWin32]::VK_SHIFT }
        '^CTRL$|^CONTROL$'     { return [AetherWin32]::VK_CONTROL }
        '^ALT$|^MENU$'         { return [AetherWin32]::VK_MENU }
        '^COMMA$|^,$'          { return [AetherWin32]::VK_OEM_COMMA }
        '^PERIOD$|^\.$'        { return [AetherWin32]::VK_OEM_PERIOD }
        '^SLASH$|^/$'          { return [AetherWin32]::VK_OEM_2 }
        '^BACKTICK$|^`$'       { return [AetherWin32]::VK_OEM_3 }
        '^PLUS$|^\+$|^=$'      { return [AetherWin32]::VK_OEM_PLUS }
        '^MINUS$|^-$'          { return [AetherWin32]::VK_OEM_MINUS }
        '^[A-Z]$'              { return [int][char]$k }
        '^[0-9]$'              { return [int][char]$k }
        default { throw "不支持的按键: $Key" }
    }
}

function Send-AetherHotkey {
    <# 发送组合键（Ctrl/Shift/Alt + 键），通过 PostMessage 注入，不依赖前台焦点。
       -Modifiers 支持 @('Ctrl','Shift','Alt') 组合。
       -Key 支持 Send-AetherKeyMsg 的所有按键。
       示例：Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl') -Key 'S'   # Ctrl+S 保存
             Send-AetherHotkey -Hwnd $h -Modifiers @('Ctrl','Shift') -Key 'P'  # Ctrl+Shift+P 命令面板 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][string[]]$Modifiers,
        [Parameter(Mandatory)][string]$Key,
        [int]$DelayMs = 300
    )
    $vkCtrl  = [AetherWin32]::VK_CONTROL
    $vkShift = [AetherWin32]::VK_SHIFT
    $vkAlt   = [AetherWin32]::VK_MENU
    $hasCtrl  = $Modifiers -contains 'Ctrl' -or $Modifiers -contains 'Control'
    $hasShift = $Modifiers -contains 'Shift'
    $hasAlt   = $Modifiers -contains 'Alt' -or $Modifiers -contains 'Menu'
    $vkKey = Get-AetherVirtualKeyCode -Key $Key

    # 按下修饰键
    if ($hasCtrl)  { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYDOWN, [IntPtr]$vkCtrl, [IntPtr]0) | Out-Null; Start-Sleep -Milliseconds 20 }
    if ($hasShift) { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYDOWN, [IntPtr]$vkShift, [IntPtr]0) | Out-Null; Start-Sleep -Milliseconds 20 }
    if ($hasAlt)   { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYDOWN, [IntPtr]$vkAlt, [IntPtr]0) | Out-Null; Start-Sleep -Milliseconds 20 }

    # 按下并释放目标键
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYDOWN, [IntPtr]$vkKey, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds 30
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYUP, [IntPtr]$vkKey, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds 20

    # 释放修饰键（逆序）
    if ($hasAlt)   { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYUP, [IntPtr]$vkAlt, [IntPtr]0) | Out-Null; Start-Sleep -Milliseconds 20 }
    if ($hasShift) { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYUP, [IntPtr]$vkShift, [IntPtr]0) | Out-Null; Start-Sleep -Milliseconds 20 }
    if ($hasCtrl)  { [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_KEYUP, [IntPtr]$vkCtrl, [IntPtr]0) | Out-Null }

    Start-Sleep -Milliseconds $DelayMs
}

# ---------------------------------------------------------------- 鼠标扩展操作

function Send-AetherDoubleClickMsg {
    <# 通过 PostMessage 注入双击（WM_LBUTTONDBLCLK），不依赖前台焦点。
       坐标 = 客户区物理像素 = 名义值 × Scale2。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y,
        [switch]$Right
    )
    $lParam = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))
    $msg = if ($Right) { [AetherWin32]::WM_RBUTTONDBLCLK } else { [AetherWin32]::WM_LBUTTONDBLCLK }
    [AetherWin32]::PostMessageW($Hwnd, $msg, [IntPtr]1, $lParam) | Out-Null
    Start-Sleep -Milliseconds 400
}

function Send-AetherMiddleClickMsg {
    <# 通过 PostMessage 注入中键点击（WM_MBUTTONDOWN/UP），不依赖前台焦点。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y
    )
    $lParam = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_MBUTTONDOWN, [IntPtr]1, $lParam) | Out-Null
    Start-Sleep -Milliseconds 60
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_MBUTTONUP, [IntPtr]0, $lParam) | Out-Null
    Start-Sleep -Milliseconds 400
}

function Send-AetherMouseWheel {
    <# 通过 PostMessage 注入鼠标滚轮（WM_MOUSEWHEEL/WM_MOUSEHWHEEL），不依赖前台焦点。
       -Delta 滚轮增量：正数向上/右，负数向下/左。标准增量为 120 的倍数。
       -X/-Y 窗口内物理坐标（滚轮事件需要光标位置决定滚动目标区域）。
       -Horizontal 使用横向滚轮（WM_MOUSEHWHEEL）。
       -Shift/-Ctrl 修饰键状态（Shift+滚轮=横向滚动，Ctrl+滚轮=缩放）。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y,
        [Parameter(Mandatory)][int]$Delta,
        [switch]$Horizontal,
        [switch]$Shift,
        [switch]$Ctrl
    )
    # 将窗口内坐标转换为屏幕坐标（WM_MOUSEWHEEL 使用屏幕坐标）
    $pt = New-Object AetherWin32+POINT
    $pt.X = $X; $pt.Y = $Y
    [AetherWin32]::ClientToScreen($Hwnd, [ref]$pt) | Out-Null
    $lParam = [IntPtr](($pt.Y -shl 16) -bor ($pt.X -band 0xFFFF))

    # 构建 wParam：高 16 位 = delta，低 16 位 = 修饰键标志
    $modifierFlags = 0
    if ($Shift) { $modifierFlags = $modifierFlags -bor 0x0004 }  # MK_SHIFT
    if ($Ctrl)  { $modifierFlags = $modifierFlags -bor 0x0008 }  # MK_CONTROL
    $wParam = [IntPtr](($Delta -shl 16) -bor $modifierFlags)

    $msg = if ($Horizontal) { [AetherWin32]::WM_MOUSEHWHEEL } else { [AetherWin32]::WM_MOUSEWHEEL }
    [AetherWin32]::PostMessageW($Hwnd, $msg, $wParam, $lParam) | Out-Null
    Start-Sleep -Milliseconds 300
}

function Send-AetherMouseMoveMsg {
    <# 通过 PostMessage 注入鼠标移动（WM_MOUSEMOVE），不依赖前台焦点。
       用于触发 hover 效果、拖拽过程中的移动等。
       -X/-Y 窗口内物理坐标。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y,
        [int]$DelayMs = 50
    )
    $lParam = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))
    [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_MOUSEMOVE, [IntPtr]0, $lParam) | Out-Null
    Start-Sleep -Milliseconds $DelayMs
}

function Send-AetherDrag {
    <# 通过 PostMessage 注入完整拖拽操作（按下 → 移动 → 释放），不依赖前台焦点。
       -FromX/-FromY 起始坐标，-ToX/-ToY 目标坐标（窗口内物理像素）。
       -Steps 移动步数（越多越平滑，默认 10）。
       -Right 使用右键拖拽。 #>
    param(
        [Parameter(Mandatory)][IntPtr]$Hwnd,
        [Parameter(Mandatory)][int]$FromX,
        [Parameter(Mandatory)][int]$FromY,
        [Parameter(Mandatory)][int]$ToX,
        [Parameter(Mandatory)][int]$ToY,
        [int]$Steps = 10,
        [switch]$Right
    )
    $fromLParam = [IntPtr](($FromY -shl 16) -bor ($FromX -band 0xFFFF))
    $toLParam = [IntPtr](($ToY -shl 16) -bor ($ToX -band 0xFFFF))
    $downMsg = if ($Right) { [AetherWin32]::WM_RBUTTONDOWN } else { [AetherWin32]::WM_LBUTTONDOWN }
    $upMsg = if ($Right) { [AetherWin32]::WM_RBUTTONUP } else { [AetherWin32]::WM_LBUTTONUP }

    # 按下
    [AetherWin32]::PostMessageW($Hwnd, $downMsg, [IntPtr]1, $fromLParam) | Out-Null
    Start-Sleep -Milliseconds 80

    # 移动（分步模拟平滑拖拽）
    for ($i = 1; $i -le $Steps; $i++) {
        $cx = $FromX + [int](($ToX - $FromX) * $i / $Steps)
        $cy = $FromY + [int](($ToY - $FromY) * $i / $Steps)
        $moveLParam = [IntPtr](($cy -shl 16) -bor ($cx -band 0xFFFF))
        [AetherWin32]::PostMessageW($Hwnd, [AetherWin32]::WM_MOUSEMOVE, [IntPtr]1, $moveLParam) | Out-Null
        Start-Sleep -Milliseconds 30
    }

    # 释放
    [AetherWin32]::PostMessageW($Hwnd, $upMsg, [IntPtr]0, $toLParam) | Out-Null
    Start-Sleep -Milliseconds 400
}

# ---------------------------------------------------------------- 窗口操作

function Resize-AetherWindow {
    <# 调整窗口大小并触发 WM_SIZE。
       -Width/-Height 新的客户区尺寸（物理像素）。
       注意：实际窗口会收到 WM_SIZE 消息，应用会重新布局。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][int]$Width,
        [Parameter(Mandatory)][int]$Height
    )
    $r = $Window.Rect
    [AetherWin32]::MoveWindow($Window.Hwnd, $r.Left, $r.Top, $Width, $Height, $true) | Out-Null
    Start-Sleep -Milliseconds 500
    # 返回更新后的窗口信息
    Get-AetherWindow -Process (Get-Process -Id (Get-Process aether-app | Where-Object { $_.MainWindowHandle -eq $Window.Hwnd }).Id)
}

function Move-AetherWindow {
    <# 移动窗口到新位置。
       -X/-Y 新的屏幕坐标（左上角）。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][int]$X,
        [Parameter(Mandatory)][int]$Y
    )
    $r = $Window.Rect
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    [AetherWin32]::MoveWindow($Window.Hwnd, $X, $Y, $w, $h, $true) | Out-Null
    Start-Sleep -Milliseconds 500
}

function Set-AetherWindowState {
    <# 设置窗口状态：Normal/Minimized/Maximized/Restored。
       最小化会触发冰冻态（Frozen），还原会解冻。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][ValidateSet('Normal','Minimized','Maximized','Restored')][string]$State
    )
    $sw = switch ($State) {
        'Normal'    { [AetherWin32]::SW_SHOWNORMAL }
        'Minimized' { [AetherWin32]::SW_SHOWMINIMIZED }
        'Maximized' { [AetherWin32]::SW_SHOWMAXIMIZED }
        'Restored'  { [AetherWin32]::SW_RESTORE }
    }
    [AetherWin32]::ShowWindow($Window.Hwnd, $sw) | Out-Null
    Start-Sleep -Milliseconds 800
}

function Close-AetherWindow {
    <# 发送 WM_CLOSE 关闭窗口（等同于点击标题栏关闭按钮）。
       与 Stop-AetherApp 的区别：WM_CLOSE 会触发应用的正常关闭流程（如 dirty 检查）。 #>
    param([Parameter(Mandatory)]$Window)
    [AetherWin32]::PostMessageW($Window.Hwnd, [AetherWin32]::WM_CLOSE, [IntPtr]0, [IntPtr]0) | Out-Null
    Start-Sleep -Milliseconds 500
}

# ---------------------------------------------------------------- 文件拖放

function Send-AetherDropFiles {
    <# 模拟文件拖放（WM_DROPFILES）。
       -Paths 文件或文件夹路径数组。
       注意：需要应用以拖拽目标注册（DragAcceptFiles），且需要 shell32.dll 的 DragQueryFileW 配合。
       由于 PostMessage 无法直接构造 HDROP，此函数通过真实拖拽模拟实现。
       更可靠的方式是使用 Windows 自动化 API 或直接在测试中调用应用内部接口。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][string[]]$Paths
    )
    # WM_DROPFILES 需要构造 HDROP 句柄，这无法通过 PostMessage 直接完成。
    # 替代方案：使用 SendMessage + 内存映射文件，或使用 UI Automation。
    # 当前实现：抛出异常提示使用替代方案。
    throw @"
WM_DROPFILES 无法通过 PostMessage 直接模拟（需要构造 HDROP 句柄）。
替代方案：
1. 使用应用内部命令（如 Ctrl+O 打开文件对话框）
2. 使用 Start-AetherApp -Folder 直接打开工作区
3. 使用 Windows UI Automation API
"@
}

# ---------------------------------------------------------------- 截图

function Save-AetherScreenshot {
    <# 截取窗口区域，保存到 tests/screenshots/<用例名>/<Name>.png，返回路径 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][string]$Name
    )
    $sub = if ($script:Case) { Join-Path $script:ScreenshotDir $script:Case.Name } else { $script:ScreenshotDir }
    New-Item -ItemType Directory -Force -Path $sub | Out-Null
    $r = $Window.Rect
    $w = $r.Right - $r.Left
    $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { throw "窗口尺寸无效 ${w}x${h}" }
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $g.Dispose()
    $path = Join-Path $sub "$Name.png"
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    if ($script:Case) { $script:Case.Screenshots += $path }
    Write-Host "  [截图] $path"
    return $path
}

# ---------------------------------------------------------------- 临时工作区

function New-AetherTestWorkspace {
    <# 创建临时工作区目录；-Files 哈希表 @{ '相对路径' = '内容' } #>
    param([hashtable]$Files = @{})
    $dir = Join-Path $env:TEMP ("aether_test_ws_" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    foreach ($rel in $Files.Keys) {
        $full = Join-Path $dir $rel
        New-Item -ItemType Directory -Force -Path (Split-Path $full -Parent) | Out-Null
        Set-Content -Path $full -Value $Files[$rel] -Encoding UTF8 -NoNewline
    }
    return $dir
}

function Remove-AetherTestWorkspace {
    param([Parameter(Mandatory)][string]$Path)
    # 只清理本框架创建的临时目录，防止误删
    if ($Path -like (Join-Path $env:TEMP "aether_test_ws_*")) {
        Remove-Item -Recurse -Force $Path -ErrorAction SilentlyContinue
    }
}

# ---------------------------------------------------------------- 日志观测

function Get-AetherLog {
    <# 返回应用最新日志文件路径。
       日志位于 %TEMP%\Aether\logs\aether.YYYY-MM-DD（无 .log 扩展名，按天轮转，
       同一天多个会话追加写同一文件）。-Tail 返回末尾 N 行；-Pattern 过滤。 #>
    param(
        [int]$Tail = 0,
        [string]$Pattern
    )
    if (-not (Test-Path $script:LogDir)) { throw "日志目录不存在: $script:LogDir" }
    $log = Get-ChildItem $script:LogDir -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $log) { throw "未找到日志文件: $script:LogDir" }
    $lines = Get-Content $log.FullName
    if ($Pattern) { $lines = $lines | Select-String $Pattern | ForEach-Object { $_.Line } }
    if ($Tail -gt 0 -and $lines) { $lines = $lines | Select-Object -Last $Tail }
    [pscustomobject]@{
        Path  = $log.FullName
        Lines = @($lines)
    }
}

function Wait-AetherLogEvent {
    <# 轮询等待日志中出现指定正则（如异步行为完成：高亮着色、LSP 诊断）。
       只观察调用时刻之后**新增**的日志行（不会误匹配历史会话/旧日志）。
       返回 @{ Found; Matches }。超时返回 Found=$false。 #>
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [int]$TimeoutMs = 5000,
        [int]$PollMs = 100
    )
    # 记录观察起点：当前最新日志文件的行数，只匹配之后追加的行
    $startLen = 0
    $startLog = $null
    if (Test-Path $script:LogDir) {
        $startLog = Get-ChildItem $script:LogDir -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($startLog) { $startLen = @(Get-Content $startLog.FullName).Count }
    }
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        try {
            $log = Get-ChildItem $script:LogDir -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($log) {
                $newLines = @(Get-Content $log.FullName) | Select-Object -Skip $startLen
                $matchedLines = @($newLines | Select-String $Pattern | ForEach-Object { $_.Line })
                if ($matchedLines.Count -gt 0) {
                    return [pscustomobject]@{ Found = $true; Matches = $matchedLines }
                }
            }
        } catch { /* 日志尚未生成 */ }
        Start-Sleep -Milliseconds $PollMs
    }
    [pscustomobject]@{ Found = $false; Matches = @() }
}

# ---------------------------------------------------------------- hit regions

function Read-AetherHitRegions {
    <# 读取 tests/gui_hit_regions.jsonl（debug 构建自动记录每帧可点击区域）。
       -Contains 传入 @{X;Y} 过滤包含该屏幕坐标的区域；-ActionLike 按名称模糊过滤。
       返回区域数组 @{ action; x; y; width; height }。 #>
    param(
        [hashtable]$Contains,
        [string]$ActionLike
    )
    if (-not (Test-Path $script:HitRegionsFile)) { return @() }
    $regions = Get-Content $script:HitRegionsFile | Where-Object { $_ } | ForEach-Object {
        try { $_ | ConvertFrom-Json } catch { $null }
    } | Where-Object { $_ }
    if ($Contains) {
        $regions = @($regions | Where-Object {
            $_.x -le $Contains.X -and ($_.x + $_.width) -ge $Contains.X -and
            $_.y -le $Contains.Y -and ($_.y + $_.height) -ge $Contains.Y
        })
    }
    if ($ActionLike) { $regions = @($regions | Where-Object { $_.action -like $ActionLike }) }
    , $regions
}

# ---------------------------------------------------------------- 用例与断言

$script:Case = $null

function Start-TestCase {
    param([Parameter(Mandatory)][string]$Name)
    $script:Case = [pscustomobject]@{
        Name        = $Name
        StartedAt   = (Get-Date).ToString("o")
        Steps       = @()
        Screenshots = @()
        Failed      = 0
        Env         = [pscustomobject]@{
            OS          = [System.Environment]::OSVersion.VersionString
            PSHost      = $PSVersionTable.PSVersion.ToString()
            DPI         = [AetherWin32]::GetDpiForWindow([IntPtr]::Zero)
            ProcessId   = $PID
        }
    }
    Write-Host "`n=== 用例: $Name ===" -ForegroundColor Cyan
}

function Invoke-TestStep {
    <# 执行一个测试步骤；异常记为失败但不中断后续步骤。
       记录步骤耗时（毫秒）到报告，供性能回归与 AI 分析。 #>
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Body
    )
    Write-Host "-- $Name" -ForegroundColor Yellow
    $t0 = [System.Diagnostics.Stopwatch]::StartNew()
    $step = @{ name = $Name; ok = $true; error = $null; duration_ms = 0 }
    try {
        & $Body
    } catch {
        $step.ok = $false
        $step.error = "$_"
        $script:Case.Failed++
        Write-Host "   失败: $_" -ForegroundColor Red
    }
    $t0.Stop()
    $step.duration_ms = [math]::Round($t0.Elapsed.TotalMilliseconds, 1)
    $script:Case.Steps += $step
}

function Assert-Condition {
    param(
        [Parameter(Mandatory)][bool]$Condition,
        [Parameter(Mandatory)][string]$Message
    )
    if (-not $Condition) { throw "断言失败: $Message" }
    Write-Host "   ✓ $Message" -ForegroundColor Green
}

function Assert-PathExists {
    param([Parameter(Mandatory)][string]$Path)
    Assert-Condition (Test-Path $Path) "路径存在: $Path"
}

function Assert-PathMissing {
    param([Parameter(Mandatory)][string]$Path)
    Assert-Condition (-not (Test-Path $Path)) "路径不存在: $Path"
}

function Complete-TestCase {
    <# 输出 JSON 报告与汇总，返回失败步骤数（用作退出码） #>
    $c = $script:Case
    if (-not $c) { return 0 }
    New-Item -ItemType Directory -Force -Path $script:ReportDir | Out-Null
    $report = Join-Path $script:ReportDir "$($c.Name).json"
    $c | Add-Member -NotePropertyName FinishedAt -NotePropertyValue (Get-Date).ToString("o") -Force
    $c | Add-Member -NotePropertyName DurationMs -NotePropertyValue 0 -Force
    # 计算总耗时
    $totalMs = 0
    foreach ($s in $c.Steps) { $totalMs += [double]$s.duration_ms }
    $c.DurationMs = [math]::Round($totalMs, 1)
    $c | ConvertTo-Json -Depth 6 | Set-Content -Path $report -Encoding UTF8
    $total = $c.Steps.Count
    $failed = $c.Failed
    $color = if ($failed -eq 0) { "Green" } else { "Red" }
    Write-Host "=== $($c.Name): $($total - $failed)/$total 步通过，耗时 ${totalMs}ms，报告 $report ===" -ForegroundColor $color
    $script:Case = $null
    return $failed
}

Export-ModuleMember -Function Build-AetherApp, Start-AetherApp, Stop-AetherApp,
    Get-AetherWindow, Get-AetherLayoutConstants, Add-AetherTrustedFolder,
    Invoke-AetherClick, Send-AetherClickMsg, Send-AetherKeys, Send-AetherText,
    Send-AetherTextMsg, Send-AetherKeyMsg, Send-AetherHotkey,
    Send-AetherDoubleClickMsg, Send-AetherMiddleClickMsg,
    Send-AetherMouseWheel, Send-AetherMouseMoveMsg, Send-AetherDrag,
    Resize-AetherWindow, Move-AetherWindow, Set-AetherWindowState, Close-AetherWindow,
    Send-AetherDropFiles,
    Save-AetherScreenshot, New-AetherTestWorkspace, Remove-AetherTestWorkspace,
    Get-AetherLog, Wait-AetherLogEvent, Read-AetherHitRegions,
    Start-TestCase, Invoke-TestStep, Assert-Condition, Assert-PathExists,
    Assert-PathMissing, Complete-TestCase
