# 只读诊断：点击图片按钮，截图验证文件对话框是否弹出
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Click {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, int e);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
}
"@
Add-Type -AssemblyName System.Drawing
[Win32Click]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null

$hwnd = [IntPtr]526854
[Win32Click]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 400

function Shot($name) {
    $bmp = New-Object System.Drawing.Bitmap 2560, 1368
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size)
    $g.Dispose()
    $bmp.Save("d:\Application\牧羊人编辑器\release\_click_$name.png")
    $bmp.Dispose()
}

# 点击图片按钮（物理坐标 2413, 1325，按几何链计算）
[Win32Click]::SetCursorPos(2413, 1325)
Start-Sleep -Milliseconds 200
[Win32Click]::mouse_event(0x0002, 0, 0, 0, 0)  # DOWN
Start-Sleep -Milliseconds 60
[Win32Click]::mouse_event(0x0004, 0, 0, 0, 0)  # UP
Start-Sleep -Milliseconds 900
Shot "after_img_btn"

# 检查是否弹出通用文件对话框（#32770 类）
$dlg = [Win32Click]::FindWindowW("#32770", $null)
Write-Host "DIALOG HWND: $($dlg.ToInt64())"
if ($dlg -ne [IntPtr]::Zero) {
    # 关闭对话框（ESC）
    [Win32Click]::SetForegroundWindow($dlg) | Out-Null
    Start-Sleep -Milliseconds 200
    [System.Windows.Forms.SendKeys]::SendWait("{ESC}")
}
Write-Host "DONE"
