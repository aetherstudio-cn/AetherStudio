Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WiggleProbe {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
}
"@
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
function snap($n) {
    $bmp = New-Object System.Drawing.Bitmap $s.Width, $s.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($s.X, $s.Y, 0, 0, $s.Size)
    $g.Dispose()
    $bmp.Save("d:\Application\牧羊人编辑器\release\_wig_$n.png")
    $bmp.Dispose()
}

# 在 AI 输出列区域 (约 x 700-1400, y 300-600) 连续微动鼠标，交错抓帧
$job = Start-Job -ScriptBlock {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    for ($i = 0; $i -lt 10; $i++) {
        $bmp = New-Object System.Drawing.Bitmap $s.Width, $s.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($s.X, $s.Y, 0, 0, $s.Size)
        $g.Dispose()
        $bmp.Save("d:\Application\牧羊人编辑器\release\_wig_job_$i.png")
        $bmp.Dispose()
        Start-Sleep -Milliseconds 90
    }
}

# 主进程持续微动鼠标
for ($i = 0; $i -lt 40; $i++) {
    $x = 1000 + ($i % 2) * 6
    $y = 450 + ($i % 3) * 4
    [WiggleProbe]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 45
}
Wait-Job $job | Out-Null
Receive-Job $job | Out-Null
Remove-Job $job | Out-Null
Write-Host "wiggle done"
