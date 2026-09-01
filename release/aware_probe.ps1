# 只读诊断探针：以 Per-Monitor V2 感知身份查询窗口真实几何与 DPI。
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Aware {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetSystemMetrics(int idx);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

# DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
[Win32Aware]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

$hwnd = [IntPtr]4131054
$r = New-Object Win32Aware+RECT
[Win32Aware]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$dpi = [Win32Aware]::GetDpiForWindow($hwnd)
$sw = [Win32Aware]::GetSystemMetrics(0)
$sh = [Win32Aware]::GetSystemMetrics(1)
Write-Host "PHYS RECT: $($r.Left),$($r.Top),$($r.Right),$($r.Bottom)"
Write-Host "WINDOW DPI: $dpi"
Write-Host "SCREEN: ${sw}x${sh}"

$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$g.Dispose()
$bmp.Save("d:\Application\牧羊人编辑器\release\_aware_shot.png")
$bmp.Dispose()
Write-Host "saved _aware_shot.png"
