# 用例：标题栏齿轮打开设置标签页（回归：D2D 裁剪栈不配对毒化渲染目标致整窗冻结）
# =============================================================================
#   Target    : 智能体模式 → 标题栏设置按钮（齿轮）
#   Feature   : 缺陷修复回归。render_settings_sidebar 内容区 PushAxisAlignedClip
#               后误用 PopLayer 弹栈，毒化 D2D 渲染目标，EndDraw 永久返回
#               0x88990014（D2DERR_DISPLAY_STATE_INVALID）；恢复循环每帧重建
#               渲染目标但每帧被重新毒化 → 整窗永久冻结在旧画面，
#               用户感知为"点击齿轮没反应"。
#               修复点：crates/aether-win32/src/render/settings_general.rs
#               （target.PopLayer() → target.PopAxisAlignedClip()）
#   Scenario  : 打开工作区 → 点击标题栏齿轮 → 打开"设置"标签页并持续渲染
#   Expect    : 1. 点击后画面发生变化（设置页渲染出来，而非冻结在旧画面）
#               2. 设置页打开后窗口仍响应交互（再点侧栏开关按钮画面继续变化）
#               3. 本次会话日志无 "EndDraw 失败"（每帧丢帧是冻结的直接证据）
#   Layout    : 语义定位（hit region titlebar:settings / titlebar:left_sidebar），
#               不手算坐标，天然抗 DPI 差异
# =============================================================================
#
# 运行：pwsh -File tests\cases\settings_gear_tab.tests.ps1 [-SkipBuild]
param([switch]$SkipBuild)

Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force

function Save-AetherThemeScreenshot {
    param($Window, [string]$Name)
    [AetherWin32]::SetForegroundWindow($Window.Hwnd) | Out-Null
    Start-Sleep -Milliseconds 200
    return Save-AetherScreenshot -Window $Window -Name $Name -Direct
}

function Get-AetherMintRatio {
    param(
        [Parameter(Mandatory)][string]$Path,
        [double]$Left = 0.0,
        [double]$Top = 0.0,
        [double]$Right = 1.0,
        [double]$Bottom = 1.0
    )
    $bmp = [System.Drawing.Bitmap]::FromFile($Path)
    try {
        $x0 = [int]($bmp.Width * $Left)
        $y0 = [int]($bmp.Height * $Top)
        $x1 = [int]($bmp.Width * $Right)
        $y1 = [int]($bmp.Height * $Bottom)
        $mint = 0
        $total = 0
        for ($y = $y0; $y -lt $y1; $y += 4) {
            for ($x = $x0; $x -lt $x1; $x += 4) {
                $pixel = $bmp.GetPixel($x, $y)
                if ($pixel.G -gt $pixel.R + 2 -and $pixel.G -gt $pixel.B + 2 -and $pixel.G -gt 120) {
                    $mint++
                }
                $total++
            }
        }
        return $mint / $total
    } finally {
        $bmp.Dispose()
    }
}

Start-TestCase "settings_gear_tab"

$ws = $null
$proc = $null
try {
    if (-not $SkipBuild) { Build-AetherApp }

    $hitFile = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\gui_hit_regions.jsonl"
    if (Test-Path $hitFile) { Remove-Item $hitFile -Force }

    $ws = New-AetherTestWorkspace -Files @{ "main.rs" = "fn main() {}" }
    $proc = Start-AetherApp -Folder $ws
    $win = Get-AetherWindow -Process $proc -Isolate

    # 截图目录（Save-AetherScreenshot 按用例名固定存放）
    $shotDir = Join-Path (Resolve-Path "$PSScriptRoot\..").Path "screenshots\settings_gear_tab"

    # 日志基线：只观察点击之后的新增日志（避免同天早先会话污染断言）
    $logMark = Get-AetherLogMark

    Invoke-TestStep "点击标题栏齿轮打开设置页" {
        $region = Wait-AetherHitRegion -ActionLike "titlebar:settings" -TimeoutMs 8000
        Assert-Condition ($null -ne $region) "hit region titlebar:settings 已注册（齿轮可见）"
        Save-AetherThemeScreenshot -Window $win -Name "1_before" | Out-Null
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:settings" | Out-Null
        Start-Sleep -Milliseconds 1500   # 等待设置页首帧渲染稳定
        Save-AetherThemeScreenshot -Window $win -Name "2_settings_opened" | Out-Null
    }

    Invoke-TestStep "点击后画面变化（冻结回归核心断言）" {
        # 冻结时 EndDraw 永久失败，画面停留在点击前 → 两图逐像素一致。
        # 正常打开设置页会替换整个内容区，变化占比远超 2%。
        Assert-AetherUiChanged `
            -Before (Join-Path $shotDir "1_before.png") `
            -After  (Join-Path $shotDir "2_settings_opened.png") `
            -MinChangedRatio 0.02 `
            -Message "点击齿轮后设置页渲染出来（窗口未冻结）"
    }

    Invoke-TestStep "功能界面与设置页共用薄荷绿渐变" {
        $regions = @(
            @{ Name = "左侧栏"; Left = 0.02; Right = 0.20 },
            @{ Name = "AI 面板"; Left = 0.22; Right = 0.55 },
            @{ Name = "功能面板"; Left = 0.58; Right = 0.98 }
        )
        foreach ($shot in @("1_before.png", "2_settings_opened.png")) {
            foreach ($region in $regions) {
                $ratio = Get-AetherMintRatio -Path (Join-Path $shotDir $shot) `
                    -Left $region.Left -Top 0.15 -Right $region.Right -Bottom 0.85
                Assert-Condition ($ratio -ge 0.35) `
                    "$shot $($region.Name) 薄荷绿像素占比 $($ratio.ToString('P1')) >= 35%"
            }
        }
    }

    Invoke-TestStep "返回应用后保持主题并继续响应交互" {
        Save-AetherThemeScreenshot -Window $win -Name "3_before_back" | Out-Null
        Invoke-AetherSmartClick -Window $win -ActionLike "settings:back_to_app" | Out-Null
        Start-Sleep -Milliseconds 800
        Save-AetherThemeScreenshot -Window $win -Name "4_after_back" | Out-Null
        Assert-AetherUiChanged `
            -Before (Join-Path $shotDir "3_before_back.png") `
            -After  (Join-Path $shotDir "4_after_back.png") `
            -MinChangedRatio 0.02 `
            -Message "返回应用生效（窗口持续响应）"
        $returned = Compare-AetherScreenshot `
            -Before (Join-Path $shotDir "1_before.png") `
            -After  (Join-Path $shotDir "4_after_back.png")
        Assert-Condition ($returned.ChangedRatio -lt 0.15) `
            "返回后恢复主界面，无设置页残留（差异像素 $($returned.ChangedRatio.ToString('P1')) < 15%）"
        $latestActions = @(
            Get-Content $hitFile | Select-Object -Last 80 | ForEach-Object {
                ($_ | ConvertFrom-Json).action
            }
        )
        Assert-Condition ($latestActions -contains "ntp:open_files") "返回后显示应用新标签页"
        Assert-Condition (@($latestActions | Where-Object { $_ -like "settings_tab:*" }).Count -eq 0) `
            "返回后设置导航已卸载"
        foreach ($region in $regions) {
            $ratio = Get-AetherMintRatio -Path (Join-Path $shotDir "4_after_back.png") `
                -Left $region.Left -Top 0.15 -Right $region.Right -Bottom 0.85
            Assert-Condition ($ratio -ge 0.35) `
                "返回后 $($region.Name) 薄荷绿像素占比 $($ratio.ToString('P1')) >= 35%"
        }
    }

    Invoke-TestStep "本次会话无 EndDraw 失败（丢帧即冻结前兆）" {
        $bad = @(Get-AetherLogSince -Mark $logMark -Pattern "EndDraw 失败")
        Assert-Condition ($bad.Count -eq 0) "无 EndDraw 失败日志（匹配 $($bad.Count) 条）"
    }
} finally {
    if ($proc) { Stop-AetherApp -Process $proc }
    if ($ws) { Remove-AetherTestWorkspace -Path $ws }
}

exit (Complete-TestCase)
