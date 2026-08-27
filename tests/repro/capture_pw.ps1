# 用 PrintWindow 截取指定进程主窗口内容（不受其它窗口遮挡影响）。
# 用法: pwsh -NoProfile -File tests/repro/capture_pw.ps1 [-OutPath xxx.png]
param(
    [string]$ProcessName = "aether-app",
    [string]$OutPath = "d:\Application\牧羊人编辑器\tests\screenshots\pw_capture.png"
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class PWCapture {
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

$proc = Get-Process $ProcessName -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Error "未找到窗口"; exit 1 }

# 声明 DPI 感知，确保 GetWindowRect 返回物理像素（否则虚拟化坐标导致截图被裁剪）
[PWCapture]::SetProcessDPIAware() | Out-Null

$rect = New-Object PWCapture+RECT
[PWCapture]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
if ($w -le 0 -or $h -le 0) { Write-Error "窗口尺寸无效"; exit 1 }

$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# PW_RENDERFULLCONTENT = 2，兼容 Direct2D 渲染面
$ok = [PWCapture]::PrintWindow($proc.MainWindowHandle, $hdc, 2)
$g.ReleaseHdc($hdc)
$g.Dispose()
$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "saved: $OutPath ($w x $h) print_ok=$ok"
