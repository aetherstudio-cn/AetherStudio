# DPI 探针：确认截图真实尺寸 / PowerShell 默认 DPI 感知 / 应用窗口真实物理矩形
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$shot = 'd:\Application\牧羊人编辑器\tests\screenshots\ntp_repro\01_after_click_search.png'
$img = [System.Drawing.Image]::FromFile($shot)
Write-Host "PNG: $($img.Width)x$($img.Height)"
$img.Dispose()

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DpiProbe {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr c);
    [DllImport("user32.dll")] public static extern uint GetDpiForSystem();
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@

$p = Get-Process aether-app -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { Write-Host "aether-app 未运行"; exit 0 }
$h = $p.MainWindowHandle
$r = New-Object DpiProbe+RECT
[DpiProbe]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Host "pwsh默认感知: 窗口 W=$($r.R-$r.L) H=$($r.B-$r.T)  系统DPI=$([DpiProbe]::GetDpiForSystem())"

# 切换到 Per-Monitor V2 后再测
$prev = [DpiProbe]::SetProcessDpiAwarenessContext([IntPtr](-4))
Write-Host "切换感知结果 prev=0x$($prev.ToString('X'))"
[DpiProbe]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Host "PMv2感知:   窗口 W=$($r.R-$r.L) H=$($r.B-$r.T)"
$c = New-Object DpiProbe+RECT
[DpiProbe]::GetClientRect($h, [ref]$c) | Out-Null
Write-Host "PMv2感知:   客户区 W=$($c.R-$c.L) H=$($c.B-$c.T)"
