# 验证 AI 面板悬停微动效与聚焦环：移动鼠标到 chips/输入框/发送按钮并截图。
param(
    [string]$OutDir = "d:\Application\牧羊人编辑器全部文件\AetherStudio\tests\screenshots"
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Mouse {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int n);
    public struct RECT { public int L; public int T; public int R; public int B; }
    public static void MoveTo(int x, int y) {
        int w = GetSystemMetrics(0), h = GetSystemMetrics(1);
        uint nx = (uint)((long)x * 65535 / (w - 1));
        uint ny = (uint)((long)y * 65535 / (h - 1));
        mouse_event(0x0001 | 0x8000, nx, ny, 0, UIntPtr.Zero);
    }
}
"@

[Win32Mouse]::SetProcessDPIAware() | Out-Null
$proc = Get-Process aether-app -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
# 后台进程的 SetForegroundWindow 会被前台锁拒绝；ShowWindow(SW_RESTORE) 带激活语义
function Focus-Aether {
    [Win32Mouse]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null
    [Win32Mouse]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 400
}
Focus-Aether
# 截图坐标 = 客户区坐标；注入屏幕坐标需加窗口原点偏移
$rect = New-Object Win32Mouse+RECT
[Win32Mouse]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$ox = $rect.L; $oy = $rect.T
Write-Output "window origin: $ox,$oy"
function Move-Client([int]$x, [int]$y) {
    [Win32Mouse]::MoveTo($ox + $x, $oy + $y)
}
function Click-Client([int]$x, [int]$y) {
    Move-Client $x $y
    Start-Sleep -Milliseconds 200
    [Win32Mouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [Win32Mouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
}

function Shot([string]$name) {
    $p = Join-Path $OutDir $name
    & powershell -WindowStyle Hidden -ExecutionPolicy Bypass -File "d:\Application\牧羊人编辑器全部文件\AetherStudio\tests\tools\capture_window.ps1" -OutPath $p | Out-Null
    Write-Output "saved: $p"
}

# 1. 悬停欢迎卡第一个 chip（解释代码）
Focus-Aether
Move-Client 610 205
Start-Sleep -Milliseconds 500
Shot "ui_hover_chip.png"

# 2. 点击 chip1 填入输入框（绕过 IME）→ 聚焦环 + 发送按钮启用
Focus-Aether
Click-Client 610 205
Start-Sleep -Milliseconds 600

# 3. 悬停发送按钮（启用态提亮）
Focus-Aether
Move-Client 1875 1140
Start-Sleep -Milliseconds 500
Shot "ui_focus_send.png"

# 4. 移开鼠标 → 聚焦环保持、发送悬停淡出
Focus-Aether
Move-Client 1200 600
Start-Sleep -Milliseconds 500
Shot "ui_focus_only.png"