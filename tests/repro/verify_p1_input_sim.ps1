# P1 端到端验证：WM_COPYDATA(dwData=1) 发模拟输入脚本 → 应用内定时器 → SendInput
# 脚本：点击输入框 → 输入文本，随后截图验证输入框出现文本
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W5 {
    [StructLayout(LayoutKind.Sequential)]
    public struct COPYDATASTRUCT { public IntPtr dwData; public int cbData; public IntPtr lpData; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string win);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, ref COPYDATASTRUCT cds);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
    public const uint WM_COPYDATA = 0x004A;
    public static void SendScript(IntPtr hwnd, string script) {
        IntPtr p = Marshal.StringToHGlobalUni(script);
        try {
            COPYDATASTRUCT cds = new COPYDATASTRUCT();
            cds.dwData = (IntPtr)1;
            cds.cbData = (script.Length + 1) * 2;
            cds.lpData = p;
            SendMessage(hwnd, WM_COPYDATA, IntPtr.Zero, ref cds);
        } finally { Marshal.FreeHGlobal(p); }
    }
}
"@
$p = Get-Process aether-app | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$hwnd = $p.MainWindowHandle
[W5]::ShowWindow($hwnd, 9) | Out-Null
[W5]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object W5+RECT
[W5]::GetWindowRect($hwnd, [ref]$r) | Out-Null
Write-Output "origin: $($r.L),$($r.T)"
$ix = $r.L + 1200
$iy = $r.T + 1085
$script = "click $ix,$iy`nsleep 250`ntype hello from aether input_sim`nsleep 300"
[W5]::SendScript($hwnd, $script)
Start-Sleep -Seconds 3
& powershell -WindowStyle Hidden -ExecutionPolicy Bypass -File "d:\Application\牧羊人编辑器全部文件\AetherStudio\tests\tools\capture_window.ps1" -OutPath "d:\Application\牧羊人编辑器全部文件\AetherStudio\tests\screenshots\p1_input_sim.png" | Out-Null