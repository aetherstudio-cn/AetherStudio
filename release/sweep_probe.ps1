# 只读诊断探针：扫过 AI 输出栏抓帧并逐帧 diff，定位闪烁相位。
# 不修改任何应用状态，仅截图 + 移动鼠标。
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Sweep {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

$hwnd = [IntPtr]4131054
$r = New-Object Win32Sweep+RECT
if (-not [Win32Sweep]::GetWindowRect($hwnd, [ref]$r)) { Write-Host "HWND INVALID"; exit 1 }
[Win32Sweep]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 400
Write-Host "RECT: $($r.Left),$($r.Top),$($r.Right),$($r.Bottom)"

$dir = "d:\Application\牧羊人编辑器\release"
$frames = New-Object System.Collections.ArrayList

function Shot([string]$name) {
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
    $g.Dispose()
    $path = "$dir\_sw_$name.png"
    $bmp.Save($path)
    [void]$frames.Add($bmp)
}

# 相位 A：鼠标静止（屏幕角落），抓 3 帧基准
[Win32Sweep]::SetCursorPos(5, 5)
Start-Sleep -Milliseconds 300
for ($i = 0; $i -lt 3; $i++) { Shot "base_$i"; Start-Sleep -Milliseconds 150 }

# 相位 B：扫过窗口中部（智能体模式 AI 输出栏 = 中间内容区），多条水平线
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$ys = @( [int]($h * 0.35), [int]($h * 0.55), [int]($h * 0.75) )
$n = 0
foreach ($y in $ys) {
    for ($x = [int]($w * 0.15); $x -le [int]($w * 0.9); $x += [int]($w * 0.1)) {
        [Win32Sweep]::SetCursorPos($r.Left + $x, $r.Top + $y)
        Start-Sleep -Milliseconds 90
        Shot ("sweep_" + $n)
        $n++
    }
}

# 相位 C：悬停在 AI 面板中心静止 4 帧（若静止也闪，则是定时器驱动）
[Win32Sweep]::SetCursorPos($r.Left + [int]($w * 0.5), $r.Top + [int]($h * 0.55))
Start-Sleep -Milliseconds 250
for ($i = 0; $i -lt 4; $i++) { Shot "hold_$i"; Start-Sleep -Milliseconds 200 }

# 逐帧与基准帧 0 对比（跳过鼠标光标 16x16 区域）
function DiffTo([System.Drawing.Bitmap]$a, [System.Drawing.Bitmap]$b) {
    $step = 4; $diff = 0
    for ($y = 0; $y -lt $a.Height; $y += $step) {
        for ($x = 0; $x -lt $a.Width; $x += $step) {
            $pa = $a.GetPixel($x, $y); $pb = $b.GetPixel($x, $y)
            if ([Math]::Abs($pa.R - $pb.R) + [Math]::Abs($pa.G - $pb.G) + [Math]::Abs($pa.B - $pb.B) -gt 24) { $diff++ }
        }
    }
    return $diff
}

$base0 = $frames[0]
Write-Host "frame_count=$($frames.Count)"
for ($i = 1; $i -lt $frames.Count; $i++) {
    $d = DiffTo $base0 $frames[$i]
    Write-Host ("frame[$i] diff_vs_base0 = $d")
}
for ($i = 0; $i -lt $frames.Count; $i++) { $frames[$i].Dispose() }
Write-Host "DONE"
