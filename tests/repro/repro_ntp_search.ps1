# 复现：新标签页（NTP）快捷搜索框无反应
# 阶段 1：启动应用并截图基线（后续阶段按截图实测坐标点击）
$ErrorActionPreference = 'Stop'

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr i);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr c);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    public const uint DOWN = 0x0002, UP = 0x0004;
    public static void Click(int x, int y) {
        SetCursorPos(x, y);
        System.Threading.Thread.Sleep(200);
        mouse_event(DOWN, 0, 0, 0, IntPtr.Zero);
        System.Threading.Thread.Sleep(100);
        mouse_event(UP, 0, 0, 0, IntPtr.Zero);
    }
}
"@
# 切换为 Per-Monitor V2 感知：坐标与截图均使用物理像素，与真实用户操作一致
[Win]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null
Add-Type -AssemblyName System.Drawing

function Capture($hwnd, $path) {
    [Win]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 400
    $r = New-Object Win+RECT
    [Win]::GetWindowRect($hwnd, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $h = $r.B - $r.T
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size $w, $h))
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host "截图: $path (${w}x${h}) 窗口 L=$($r.L) T=$($r.T)"
}

$exe = "d:\Application\牧羊人编辑器\target\x86_64-pc-windows-msvc\debug\aether-app.exe"
$shotDir = "d:\Application\牧羊人编辑器\tests\screenshots\ntp_repro"
New-Item -ItemType Directory -Force -Path $shotDir | Out-Null
Remove-Item "$shotDir\*.png" -Force -ErrorAction SilentlyContinue

Get-Process aether-app -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
Start-Process -FilePath $exe | Out-Null

$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    $p = Get-Process aether-app -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($p -and $p.MainWindowHandle -ne [IntPtr]::Zero) { $hwnd = $p.MainWindowHandle; break }
}
if ($hwnd -eq [IntPtr]::Zero) { throw "未找到 aether-app 主窗口" }
[Win]::ShowWindow($hwnd, 9) | Out-Null
Start-Sleep -Seconds 3
[Win]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Seconds 2

Capture $hwnd "$shotDir\00_baseline.png"

# 阶段 2：点击搜索框视觉中心（物理坐标 = 逻辑(1026.5,408)×1.5），输入文本，回车
[Win]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 400
[Win]::Click(1540, 612)
Start-Sleep -Milliseconds 800
Capture $hwnd "$shotDir\01_after_click_search.png"

Add-Type -AssemblyName System.Windows.Forms
function FocusApp { [Win]::SetForegroundWindow($hwnd) | Out-Null; Start-Sleep -Milliseconds 400 }
FocusApp
[System.Windows.Forms.SendKeys]::SendWait('rust')
Start-Sleep -Milliseconds 800
Capture $hwnd "$shotDir\02_after_typing.png"

FocusApp
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 800
# 第一次回车可能被 IME 消费（确认合成），补发一次触发搜索/导航
FocusApp
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Milliseconds 3000
Capture $hwnd "$shotDir\03_after_enter.png"

# 输出本次新增日志（关注点击/搜索/浏览器路由）
$logFile = Get-ChildItem "$env:TEMP\Aether\logs\aether.*" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($logFile) {
    Write-Host "`n===== 最新日志尾部 ====="
    Get-Content $logFile.FullName -Tail 40
}
