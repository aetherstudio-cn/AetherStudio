Add-Type @"
using System;
using System.Runtime.InteropServices;
public class HoverProbe {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$hwnd = [IntPtr]4131054
$r = New-Object HoverProbe+RECT
[HoverProbe]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$cx = [int](($r.L + $r.R) / 2) + 200   # 中间 AI 输出列
$cy = [int](($r.T + $r.B) / 2)
Write-Host "hover at $cx,$cy"

$s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
function snap($n) {
    $bmp = New-Object System.Drawing.Bitmap $s.Width, $s.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($s.X, $s.Y, 0, 0, $s.Size)
    $g.Dispose()
    $bmp.Save("d:\Application\牧羊人编辑器\release\_hov_$n.png")
    $bmp.Dispose()
}

# 移开鼠标先拍基准
[HoverProbe]::SetCursorPos(150, 800) | Out-Null
Start-Sleep -Milliseconds 600
snap "away_0"
snap "away_1"

# 悬停到 AI 输出列，连续拍 8 帧
[HoverProbe]::SetCursorPos($cx, $cy) | Out-Null
Start-Sleep -Milliseconds 150
for ($i = 0; $i -lt 8; $i++) { snap "hover_$i"; Start-Sleep -Milliseconds 110 }
Write-Host "done"
