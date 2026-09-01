# 只读：光标类型扫描定位图片按钮精确坐标
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinScan {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorInfo(ref CURSORINFO pci);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct CURSORINFO { public int cbSize; public int flags; public IntPtr hCursor; public long pt; }
}
"@
[WinScan]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

# 枚举当前 aether 主窗口
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinE3 {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder sb, int max);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
"@
$tp = (Get-Process aether-app).Id
$script:hwnd = [IntPtr]::Zero
$cb = [WinE3+EnumProc]{ param($h,$l); $pidOut = 0; [WinE3]::GetWindowThreadProcessId($h, [ref]$pidOut) | Out-Null; if ($pidOut -eq $tp -and [WinE3]::IsWindowVisible($h)) { $cls = New-Object System.Text.StringBuilder 64; [WinE3]::GetClassNameW($h, $cls, 64) | Out-Null; if ($cls.ToString() -eq "AetherEditor") { $script:hwnd = $h; return $false } }; return $true }
[WinE3]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
[WinScan]::SetForegroundWindow($script:hwnd) | Out-Null
Start-Sleep -Milliseconds 500

function CurH {
    $ci = New-Object WinScan+CURSORINFO
    $ci.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf([type][WinScan+CURSORINFO])
    [WinScan]::GetCursorInfo([ref]$ci) | Out-Null
    return $ci.hCursor.ToInt64()
}

# 扫描右下角区域，打印每行 HAND/IBEAM 的 x 区间
Write-Host "=== 区域扫描 ==="
for ($y = 1150; $y -le 1330; $y += 4) {
    $handX = @(); $ibeamX = @()
    for ($x = 1700; $x -le 2200; $x += 3) {
        [WinScan]::SetCursorPos($x, $y)
        Start-Sleep -Milliseconds 15
        $h = CurH
        if ($h -eq 65543) { $handX += $x }
        elseif ($h -eq 65541) { $ibeamX += $x }
    }
    $hs = if ($handX.Count -gt 0) { "HAND x[$($handX[0])..$($handX[-1])]" } else { "" }
    $is = if ($ibeamX.Count -gt 0) { "IBEAM x[$($ibeamX[0])..$($ibeamX[-1])]" } else { "" }
    if ($hs -or $is) { Write-Host "y=$y  $hs  $is" }
}
Write-Host "=== 完成 ==="
