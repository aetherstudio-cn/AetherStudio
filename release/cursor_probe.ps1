# 只读诊断探针：采样鼠标光标句柄，验证"光标类型振荡"假设。
# GetCursorInfo 返回当前生效光标；标准光标（Arrow/IBeam/Hand）句柄唯一且稳定。
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Cur {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorInfo(ref CURSORINFO pci);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct CURSORINFO {
        public int cbSize; public int flags; public IntPtr hCursor; public long pt;
    }
}
"@

$hwnd = [IntPtr]4131054
$r = New-Object Win32Cur+RECT
if (-not [Win32Cur]::GetWindowRect($hwnd, [ref]$r)) { Write-Host "HWND INVALID"; exit 1 }
[Win32Cur]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 400

function CurHandle {
    $ci = New-Object Win32Cur+CURSORINFO
    $ci.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf([type][Win32Cur+CURSORINFO])
    [Win32Cur]::GetCursorInfo([ref]$ci) | Out-Null
    return $ci.hCursor
}

$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top

# 测试 1：静止悬停在 AI 面板中部，采样 80 次（30ms 间隔），看光标句柄是否自振荡
Write-Host "=== T1: stationary hover samples ==="
[Win32Cur]::SetCursorPos($r.Left + [int]($w * 0.5), $r.Top + [int]($h * 0.55))
Start-Sleep -Milliseconds 200
$t1 = @()
for ($i = 0; $i -lt 80; $i++) { $t1 += (CurHandle).ToInt64(); Start-Sleep -Milliseconds 30 }
$u1 = $t1 | Sort-Object -Unique
Write-Host ("distinct handles: " + ($u1 -join ','))
$trans = 0
for ($i = 1; $i -lt $t1.Count; $i++) { if ($t1[$i] -ne $t1[$i-1]) { $trans++ } }
Write-Host "transitions=$trans / $($t1.Count)"

# 测试 2：水平微扫（30 步横跨中部），每步采样光标句柄，看是否有密集交替区
Write-Host "=== T2: sweep cursor handles ==="
$seq = @()
for ($i = 0; $i -le 30; $i++) {
    $x = $r.Left + [int]($w * (0.1 + 0.8 * $i / 30.0))
    [Win32Cur]::SetCursorPos($x, $r.Top + [int]($h * 0.55))
    Start-Sleep -Milliseconds 50
    $seq += (CurHandle).ToInt64()
}
$sb = New-Object System.Text.StringBuilder
for ($i = 0; $i -lt $seq.Count; $i++) {
    if ($i -eq 0 -or $seq[$i] -ne $seq[$i-1]) { [void]$sb.Append("[$i]=" + $seq[$i] + " ") }
}
Write-Host $sb.ToString()
$trans2 = 0
for ($i = 1; $i -lt $seq.Count; $i++) { if ($seq[$i] -ne $seq[$i-1]) { $trans2++ } }
Write-Host "sweep transitions=$trans2 / $($seq.Count)"

# 测试 3：垂直微扫（自顶向下穿过 AI 面板）
Write-Host "=== T3: vertical sweep ==="
$seq3 = @()
for ($i = 0; $i -le 30; $i++) {
    $y = $r.Top + [int]($h * (0.05 + 0.9 * $i / 30.0))
    [Win32Cur]::SetCursorPos($r.Left + [int]($w * 0.5), $y)
    Start-Sleep -Milliseconds 50
    $seq3 += (CurHandle).ToInt64()
}
$sb3 = New-Object System.Text.StringBuilder
for ($i = 0; $i -lt $seq3.Count; $i++) {
    if ($i -eq 0 -or $seq3[$i] -ne $seq3[$i-1]) { [void]$sb3.Append("[$i]=" + $seq3[$i] + " ") }
}
Write-Host $sb3.ToString()
Write-Host "DONE"
