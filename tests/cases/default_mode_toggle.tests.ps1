# 用例：默认启动模式切换 + 智能体模式设置统一走标签页
# =============================================================================
#   Target    : 设置页「通用 → 默认启动模式」行 / 智能体模式标题栏齿轮
#   Feature   : 1. 设置页「默认启动模式」行可点击：切换 开发者/智能体，
#                  立即持久化到 ui.editor_mode（重启后以该模式启动）
#               2. 智能体模式设置弹窗已取消：齿轮双模式统一打开设置标签页
#                  （智能体模式标签页内容渲染在右侧面板，打开时自动显示）
#   Scenario  : 以智能体模式启动 → 点齿轮开设置标签页 → 切到通用页
#               → 点击「默认启动模式」行 → 退出后校验 settings.json
#   Expect    : 1. 齿轮点击后 settings_tab:* 命中区出现（标签页而非弹窗）
#               2. settings:default_mode 命中区已注册
#               3. 退出后 ui.editor_mode 持久化为 developer（原为 agent）
#   Layout    : 语义定位（titlebar:settings / settings_tab:general /
#               settings:default_mode），不手算坐标
# =============================================================================
#
# 运行：pwsh -File tests\cases\default_mode_toggle.tests.ps1 [-SkipBuild]
param([switch]$SkipBuild)

Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force

Start-TestCase "default_mode_toggle"

# ---- 强制以智能体模式启动（备份原设置，结束还原）
$settingsFile = Join-Path $env:APPDATA "Aether\settings.json"
$backup = $null
if (Test-Path $settingsFile) { $backup = Get-Content $settingsFile -Raw }
try {
    $cfg = if ($backup) { $backup | ConvertFrom-Json } else { [pscustomobject]@{} }
    if (-not $cfg.ui) { $cfg | Add-Member -NotePropertyName ui -NotePropertyValue ([pscustomobject]@{}) }
    $cfg.ui | Add-Member -NotePropertyName editor_mode -NotePropertyValue "agent" -Force
    $cfg | ConvertTo-Json -Depth 20 | Set-Content $settingsFile -Encoding UTF8
} catch { Write-Host "[setup] settings.json 写入失败，按当前模式继续: $_" }

$ws = $null
$proc = $null
try {
    if (-not $SkipBuild) { Build-AetherApp }

    $ws = New-AetherTestWorkspace -Files @{ "main.rs" = "fn main() {}" }
    $proc = Start-AetherApp -Folder $ws
    $win = Get-AetherWindow -Process $proc -Isolate

    Invoke-TestStep "智能体模式：齿轮打开设置标签页（弹窗路径已删除）" {
        $region = Wait-AetherHitRegion -ActionLike "titlebar:settings" -TimeoutMs 8000
        Assert-Condition ($null -ne $region) "titlebar:settings 已注册"
        Invoke-AetherSmartClick -Window $win -ActionLike "titlebar:settings" | Out-Null
        # 设置页侧栏 tab 注册语义命中区即证明标签页已渲染；弹窗代码已删除，此为唯一可达路径
        $tab = Wait-AetherHitRegion -ActionLike "settings_tab:*" -TimeoutMs 8000
        Assert-Condition ($null -ne $tab) "设置标签页已打开（settings_tab hit region 出现）"
    }

    Invoke-TestStep "通用页：默认启动模式行注册命中区" {
        Invoke-AetherSmartClick -Window $win -ActionLike "settings_tab:general" | Out-Null
        Start-Sleep -Milliseconds 800
        $row = Wait-AetherHitRegion -ActionLike "settings:default_mode" -TimeoutMs 5000
        Assert-Condition ($null -ne $row) "settings:default_mode 命中区已注册"
    }

    Invoke-TestStep "点击切换：agent → developer（点击时即持久化）" {
        Invoke-AetherSmartClick -Window $win -ActionLike "settings:default_mode" | Out-Null
        Start-Sleep -Milliseconds 1000
    }
} finally {
    if ($proc) { Stop-AetherApp -Process $proc }
    Start-Sleep -Seconds 1
    # 应用退出后校验落盘结果（点击 handler 已同步 save，退出保存不会覆盖）。
    # 先还原用户设置再断言：断言失败抛错时不丢失 teardown。
    $after = (Get-Content $settingsFile -Raw | ConvertFrom-Json).ui.editor_mode
    if ($backup) { Set-Content -Path $settingsFile -Value $backup -Encoding UTF8 -NoNewline }
    Write-Host "[teardown] 已还原 settings.json"
    Assert-Condition ($after -eq "developer") "settings.json 已持久化为 developer（实际: $after）"
    if ($ws) { Remove-AetherTestWorkspace -Path $ws }
}

exit (Complete-TestCase)
