Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Win32Shot {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    public const uint DOWN = 0x0002, UP = 0x0004;
    public static void Click(int x, int y) {
        SetCursorPos(x, y);
        System.Threading.Thread.Sleep(30);
        mouse_event(DOWN, 0, 0, 0, 0);
        mouse_event(UP, 0, 0, 0, 0);
    }
}
"@
Add-Type -AssemblyName System.Drawing

$hwnd = [Win32Shot]::FindWindow($null, "Aether")
if ($hwnd -eq [IntPtr]::Zero) { Write-Host "WINDOW NOT FOUND"; exit 1 }
[Win32Shot]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object Win32Shot+RECT
[Win32Shot]::GetWindowRect($hwnd, [ref]$r) | Out-Null
Write-Host "RECT: $($r.Left),$($r.Top),$($r.Right),$($r.Bottom)"

function Shot([string]$name) {
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $g.Dispose()
    $bmp.Save("d:\Application\牧羊人编辑器\release\_flicker_$name.png")
    $bmp.Dispose()
}

# 连续 6 帧，间隔 120ms，捕获闪烁的两个相位
for ($i = 0; $i -lt 6; $i++) { Shot "frame_$i"; Start-Sleep -Milliseconds 120 }
Write-Host "frames saved"
