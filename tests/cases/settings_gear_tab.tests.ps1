# 用例：标题栏齿轮打开设置标签页（回归：D2D 裁剪栈不配对毒化渲染目标致整窗冻结）
# =============================================================================
#   Target    : 开发者模式 → 标题栏设置按钮（齿轮）
#   Feature   : 缺陷修复回归。render_settings_sidebar 内容区 PushAxisAlignedClip
#               后误用 PopLayer 弹栈，毒化 D2D 渲染目标，EndDraw 永久返回
#               0x88990014（D2DERR_DISPLAY_STATE_INVALID）；恢复循环每帧重建
#               渲染目标但每帧被重新毒化 → 整窗永久冻结在旧画面，
#               用户感知为"点击齿轮没反应"。
#               修复点：crates/aether-win32/src/render/settings_general.rs
#               （target.PopLayer() → target.PopAxisAlignedClip()）
#   Scenario  : 开发者模式打开工作区 → 点击标题栏齿轮 → 打开"设置"标签页并持续渲染
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

Start-TestCase "settings_gear_tab"

# ---- 模式防御：齿轮行为依赖开发者模式布局。
# ---- 模式持久化在 %APPDATA%\Aether\settings.json 的 ui.editor_mode，
# ---- 若用户上次停在智能体模式会污染本用例：临时改为 developer，结束后还原。
$settingsFile = Join-Path $env:APPDATA "Aether\settings.json"
$settingsBackup = $null
if (Test-Path $settingsFile) {
    try {
        $cfg = Get-Content $settingsFile -Raw | ConvertFrom-Json
        if ($cfg.ui -and $cfg.ui.editor_mode -eq "agent") {
            $settingsBackup = Get-Content $settingsFile -Raw
            $cfg.ui.editor_mode = "developer"
            $cfg | ConvertTo-Json -Depth 20 | Set-Content $settingsFile -Encoding UTF8
            Write-Host "[setup] 临时切换 editor_mode=developer（用例结束自动还原）"
        }
    } catch {
        Write-Host "[setup] settings.json 读取失败，按默认模式继续: $_"
    }
}

$ws = $null
$proc = $null
try {
    if (-not $SkipBuild) { Build-AetherApp }

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
        Save-AetherScreenshot -Window $win -Name "1_before" | Out-Null
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:settings" | Out-Null
        Start-Sleep -Milliseconds 1500   # 等待设置页首帧渲染稳定
        Save-AetherScreenshot -Window $win -Name "2_settings_opened" | Out-Null
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

    Invoke-TestStep "设置页打开后窗口仍响应交互" {
        # 再触发一次可见 UI 变化：切换左侧栏。冻结的窗口对此也不会有任何反应。
        Save-AetherScreenshot -Window $win -Name "3_before_sidebar_toggle" | Out-Null
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:left_sidebar" | Out-Null
        Start-Sleep -Milliseconds 800
        Save-AetherScreenshot -Window $win -Name "4_after_sidebar_toggle" | Out-Null
        Assert-AetherUiChanged `
            -Before (Join-Path $shotDir "3_before_sidebar_toggle.png") `
            -After  (Join-Path $shotDir "4_after_sidebar_toggle.png") `
            -MinChangedRatio 0.005 `
            -Message "侧栏切换生效（窗口持续响应）"
        # 还原侧栏状态，避免影响后续用例的持久化布局
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:left_sidebar" | Out-Null
        Start-Sleep -Milliseconds 500
    }

    Invoke-TestStep "本次会话无 EndDraw 失败（丢帧即冻结前兆）" {
        $bad = @(Get-AetherLogSince -Mark $logMark -Pattern "EndDraw 失败")
        Assert-Condition ($bad.Count -eq 0) "无 EndDraw 失败日志（匹配 $($bad.Count) 条）"
    }
} finally {
    if ($proc) { Stop-AetherApp -Process $proc }
    # 应用退出后再还原用户设置（应用退出时会落盘 settings.json，先停进程再写回）
    if ($settingsBackup) {
        Set-Content -Path $settingsFile -Value $settingsBackup -Encoding UTF8 -NoNewline
        Write-Host "[teardown] 已还原用户 settings.json"
    }
    if ($ws) { Remove-AetherTestWorkspace -Path $ws }
}

exit (Complete-TestCase)
