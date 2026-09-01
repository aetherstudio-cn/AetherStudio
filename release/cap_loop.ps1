# 只读诊断：生成期间连续抓帧，检测"变暗/正常"交替相位。
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Cap {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Add-Type -AssemblyName System.Drawing

$hwnd = [IntPtr]4131054
$r = New-Object Win32Cap+RECT
if (-not [Win32Cap]::GetWindowRect($hwnd, [ref]$r)) { Write-Host "HWND INVALID"; exit 1 }
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top

# 只对比 AI 输出栏中心区域（避免光标/其他区域干扰）：取中部 600x300
$cx = [int]($w * 0.55); $cy = [int]($h * 0.45); $cw = 600; $ch = 300

$prev = $null
for ($i = 0; $i -lt 100; $i++) {
    $bmp = New-Object System.Drawing.Bitmap $cw, $ch
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left + $cx - [int]($cw/2), $r.Top + $cy - [int]($ch/2), 0, 0, $bmp.Size)
    $g.Dispose()
    if ($prev -ne $null) {
        $diff = 0; $sumA = 0; $sumB = 0; $n = 0
        for ($y = 0; $y -lt $ch; $y += 3) {
            for ($x = 0; $x -lt $cw; $x += 3) {
                $pa = $prev.GetPixel($x, $y); $pb = $bmp.GetPixel($x, $y)
                $sumA += $pa.R + $pa.G + $pa.B; $sumB += $pb.R + $pb.G + $pb.B; $n++
                if ([Math]::Abs($pa.R - $pb.R) + [Math]::Abs($pa.G - $pb.G) + [Math]::Abs($pa.B - $pb.B) -gt 24) { $diff++ }
            }
        }
        $avgA = [math]::Round($sumA / $n, 1); $avgB = [math]::Round($sumB / $n, 1)
        Write-Host ("f[$i] diff=$diff avgRGB prev=$avgA cur=$avgB delta=" + [math]::Round($avgB - $avgA, 1))
        if ($diff -gt 50) {
            $prev.Save("d:\Application\牧羊人编辑器\release\_cap_prev_$i.png")
            $bmp.Save("d:\Application\牧羊人编辑器\release\_cap_cur_$i.png")
        }
    }
    if ($prev -ne $null) { $prev.Dispose() }
    $prev = $bmp
    Start-Sleep -Milliseconds 500
}
$prev.Dispose()
Write-Host "CAP DONE"
