# 用例：欢迎页（无工作区）布局：活动栏固定 + 终端让位 + 侧边栏面板可开关
# =============================================================================
#   Target    : 欢迎页 + 底部面板（终端）/ 活动栏 / 侧边栏面板布局
#   Feature   : 1. 欢迎页活动栏固定常驻（侧边栏功能按钮入口，不可关闭），
#                  侧边栏面板默认收起，终端面板从活动栏右缘开始、无挤压空地
#               2. 欢迎页期间可经活动栏图标/标题栏按钮调起侧边栏面板，
#                  欢迎页内容避让、终端让位右侧；收起面板后活动栏仍在
#   Scenario  : 清空 last_workspace 以 new_window 启动（欢迎页）→ 调起终端
#               → 采样像素 → 调起侧边栏面板 → 采样 → 收起面板 → 采样
#   Expect    : 1. 启动后活动栏命中区存在、侧边栏面板命中区不存在
#               2. 默认终端左缘贴活动栏右缘（x=48），x=5 为活动栏非黑底
#               3. 调起面板后左缘为侧边栏底色(≈27)，终端让位右侧
#               4. 收起面板后终端恢复让位前布局，活动栏仍固定存在
#   Layout    : 语义定位（titlebar:*、activity:*）+ 像素采样
# =============================================================================
#
# 运行：pwsh -File tests\cases\welcome_terminal.tests.ps1 [-SkipBuild]
param([switch]$SkipBuild)

Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force

Add-Type -AssemblyName System.Drawing

Start-TestCase "welcome_terminal"

# ---- 清空 last_workspace，确保启动进入欢迎页（主窗口会自动恢复上次工作区）
$settingsFile = Join-Path $env:APPDATA "Aether\settings.json"
$backup = $null
if (Test-Path $settingsFile) {
    $backup = Get-Content $settingsFile -Raw
    $cfg = $backup | ConvertFrom-Json
    if ($cfg.ui) { $cfg.ui | Add-Member -NotePropertyName last_workspace -NotePropertyValue $null -Force }
    $cfg | ConvertTo-Json -Depth 20 | Set-Content $settingsFile -Encoding UTF8
}

$proc = $null
try {
    if (-not $SkipBuild) { Build-AetherApp }

    # hit regions 文件按帧累积：清空旧帧，避免历史会话的 sidebar 区域污染"不应存在"断言
    $hitFile = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\gui_hit_regions.jsonl"
    if (Test-Path $hitFile) { Remove-Item $hitFile -Force }

    # new_window=true 绕过单实例转发；paths 为空 → 无工作区 → 欢迎页
    $appExe = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "target\x86_64-pc-windows-msvc\debug\aether-app.exe"
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $appExe
    $psi.ArgumentList.Add("--aether-launch-args")
    $psi.ArgumentList.Add('{"paths":[],"new_window":true,"goto":null,"wait":false}')
    $proc = [System.Diagnostics.Process]::Start($psi)
    $win = $null
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline -and -not $win) {
        Start-Sleep -Milliseconds 500
        try { $win = Get-AetherWindow -Process $proc -Isolate } catch { $win = $null }
    }
    Assert-Condition ($null -ne $win) "窗口 15s 内就绪"
    Start-Sleep -Milliseconds 1500
    $k = $win.Scale2   # 名义 → 物理换算系数（DPI 非 100% 时原始像素坐标必偏）
    function NX([double]$v) { [int]($v * $k) }

    Invoke-TestStep "启动后处于欢迎页（活动栏固定、侧边栏面板收起）" {
        Assert-Condition ($null -ne (Find-AetherHitRegion -ActionLike "activity:Explorer")) "活动栏命中区存在（固定常驻）"
        Assert-Condition (-not (Find-AetherHitRegion -ActionLike "sidebar:new_file")) "侧边栏面板命中区不存在（默认收起）"
    }

    Invoke-TestStep "调起终端后底部面板贴活动栏右缘（无挤压空地）" {
        $region = Wait-AetherHitRegion -ActionLike "titlebar:bottom_panel" -TimeoutMs 8000
        Assert-Condition ($null -ne $region) "titlebar:bottom_panel 已注册"
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:bottom_panel" | Out-Null
        Start-Sleep -Milliseconds 1500

        $shot = Save-AetherScreenshot -Window $win -Name "welcome_terminal"
        $img = [System.Drawing.Image]::FromFile((Resolve-Path $shot).Path)
        try {
            $ySample = [int]($img.Height * 0.8)
            $pxBar = $img.GetPixel((NX 5), $ySample)
            Write-Host "[samples] y=$ySample bar=($($pxBar.R),$($pxBar.G),$($pxBar.B))"
            # 活动栏固定常驻：x=5 为活动栏底色（非终端黑底）
            Assert-Condition (-not ($pxBar.R -le 15 -and $pxBar.G -le 15 -and $pxBar.B -le 15)) "x=5 为活动栏（实际 ($($pxBar.R),$($pxBar.G),$($pxBar.B))）"
            foreach ($x in @(100, 300)) {
                $px = $img.GetPixel((NX $x), $ySample)
                # 终端底色 color_f(0.02) ≈ (5,5,5)；终端从活动栏右缘(48)开始全宽铺开
                Assert-Condition ($px.R -le 15 -and $px.G -le 15 -and $px.B -le 15) "x=$x 为终端黑底（实际 ($($px.R),$($px.G),$($px.B))）"
            }
        } finally { $img.Dispose() }
    }

    Invoke-TestStep "欢迎页可调起侧边栏面板（终端让位右侧，无空地）" {
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:left_sidebar" | Out-Null
        Start-Sleep -Milliseconds 1000  # toggle_sidebar 200ms 动画
        $shot = Save-AetherScreenshot -Window $win -Name "welcome_sidebar_open"
        $img = [System.Drawing.Image]::FromFile((Resolve-Path $shot).Path)
        try {
            $y2 = [int]($img.Height * 0.8)
            $pxL = $img.GetPixel((NX 70), $y2)
            $pxR = $img.GetPixel((NX 700), $y2)
            Write-Host "[opened] (70)=($($pxL.R),$($pxL.G),$($pxL.B)) (700)=($($pxR.R),$($pxR.G),$($pxR.B))"
            # 面板区为侧边栏底色 #1B1B1C≈27（非终端黑底）；终端让位到面板右侧
            Assert-Condition (-not ($pxL.R -le 15 -and $pxL.G -le 15 -and $pxL.B -le 15)) "x=70 为侧边栏面板（实际 ($($pxL.R),$($pxL.G),$($pxL.B))）"
            Assert-Condition ($pxR.R -le 15 -and $pxR.G -le 15 -and $pxR.B -le 15) "x=700 为终端黑底（实际 ($($pxR.R),$($pxR.G),$($pxR.B))）"
        } finally { $img.Dispose() }
    }

    Invoke-TestStep "收起侧边栏面板后活动栏仍固定" {
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:left_sidebar" | Out-Null
        Start-Sleep -Milliseconds 1000
        $shot = Save-AetherScreenshot -Window $win -Name "welcome_sidebar_closed"
        $img = [System.Drawing.Image]::FromFile((Resolve-Path $shot).Path)
        try {
            $y3 = [int]($img.Height * 0.8)
            $pxBar = $img.GetPixel((NX 5), $y3)
            $pxPanel = $img.GetPixel((NX 70), $y3)
            Write-Host "[closed] (5)=($($pxBar.R),$($pxBar.G),$($pxBar.B)) (70)=($($pxPanel.R),$($pxPanel.G),$($pxPanel.B))"
            Assert-Condition (-not ($pxBar.R -le 15 -and $pxBar.G -le 15 -and $pxBar.B -le 15)) "x=5 活动栏仍在（实际 ($($pxBar.R),$($pxBar.G),$($pxBar.B))）"
            Assert-Condition ($pxPanel.R -le 15 -and $pxPanel.G -le 15 -and $pxPanel.B -le 15) "x=70 恢复终端黑底（实际 ($($pxPanel.R),$($pxPanel.G),$($pxPanel.B))）"
            Assert-Condition ($null -ne (Find-AetherHitRegion -ActionLike "activity:Explorer")) "活动栏命中区仍存在"
        } finally { $img.Dispose() }
    }
} finally {
    if ($proc) { Stop-AetherApp -Process $proc }
    if ($backup) { Set-Content -Path $settingsFile -Value $backup -Encoding UTF8 -NoNewline }
    Write-Host "[teardown] 已还原 settings.json"
}

exit (Complete-TestCase)
