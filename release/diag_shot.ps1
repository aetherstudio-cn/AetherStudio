# 只读诊断：PMv2 感知截图 + 枚举 Aether 主窗口
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Win32Diag {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder sb, int max);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr hWnd, StringBuilder sb, int max);
    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing
[Win32Diag]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

$targetPid = 11252
$found = [IntPtr]::Zero
$cb = [Win32Diag+EnumProc]{
    param($h, $l)
    $pidOut = 0
    [Win32Diag]::GetWindowThreadProcessId($h, [ref]$pidOut) | Out-Null
    if ($pidOut -eq $targetPid -and [Win32Diag]::IsWindowVisible($h)) {
        $cls = New-Object System.Text.StringBuilder 64
        [Win32Diag]::GetClassNameW($h, $cls, 64) | Out-Null
        if ($cls.ToString() -eq "AetherEditor") { $script:found = $h; return $false }
    }
    return $true
}
[Win32Diag]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($found -eq [IntPtr]::Zero) { Write-Host "MAIN WINDOW NOT FOUND"; exit 1 }
Write-Host "HWND: $($found.ToInt64())"
[Win32Diag]::SetForegroundWindow($found) | Out-Null
Start-Sleep -Milliseconds 600
$r = New-Object Win32Diag+RECT
[Win32Diag]::GetWindowRect($found, [ref]$r) | Out-Null
Write-Host "RECT: $($r.Left),$($r.Top),$($r.Right),$($r.Bottom)"
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$g.Dispose()
$bmp.Save("d:\Application\牧羊人编辑器\release\_diag_full.png")
# 裁剪底部输入区（底部 200px）
$ch = [Math]::Min(300, $h)
$crop = New-Object System.Drawing.Bitmap $w, $ch
$g2 = [System.Drawing.Graphics]::FromImage($crop)
$g2.DrawImage($bmp, (New-Object System.Drawing.Rectangle 0,0,$w,$ch), (New-Object System.Drawing.Rectangle 0,($h-$ch),$w,$ch), [System.Drawing.GraphicsUnit]::Pixel)
$g2.Dispose()
$crop.Save("d:\Application\牧羊人编辑器\release\_diag_input.png")
$bmp.Dispose(); $crop.Dispose()
Write-Host "saved"
