# Aether 测试框架自检（Smoke）— 验证核心链路可用
#
# 运行：pwsh -NoProfile -File tests\ai\selfcheck.ps1
# 覆盖：模块导入 → 布局常量 → 构建 → 启动（信任/隔离）→ PostMessage 点击 →
#        日志观测 → 截图 → 动作脚本 → 诊断包 → 报告
# AI 在开始写用例前运行一次，确认测试环境就绪。

$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$failures = 0

function Step([string]$Name, [scriptblock]$Body) {
    Write-Host "`n== $Name" -ForegroundColor Cyan
    try { & $Body; Write-Host "   ✓ $Name" -ForegroundColor Green }
    catch { $script:failures++; Write-Host "   ✗ $Name : $_" -ForegroundColor Red }
}

Step "导入框架模块" {
    Import-Module "$root\tests\framework\AetherTest.psm1" -Force
    Import-Module "$root\tests\framework\AetherAi.psm1" -Force
    if (-not (Get-Command Get-AetherLayoutConstants -ErrorAction SilentlyContinue)) { throw "核心模块函数缺失" }
    if (-not (Get-Command Invoke-AetherActionScript -ErrorAction SilentlyContinue)) { throw "AI 模块函数缺失" }
    # 验证新增函数导出
    $coreFns = @(
        'Send-AetherHotkey', 'Send-AetherDoubleClickMsg', 'Send-AetherMiddleClickMsg',
        'Send-AetherMouseWheel', 'Send-AetherMouseMoveMsg', 'Send-AetherDrag',
        'Resize-AetherWindow', 'Move-AetherWindow', 'Set-AetherWindowState', 'Close-AetherWindow'
    )
    foreach ($fn in $coreFns) {
        if (-not (Get-Command $fn -ErrorAction SilentlyContinue)) { throw "核心模块缺少函数: $fn" }
    }
    $aiFns = @(
        'Find-AetherHitRegion', 'Invoke-AetherSmartClick', 'Wait-AetherHitRegion', 'Get-AetherEditorState'
    )
    foreach ($fn in $aiFns) {
        if (-not (Get-Command $fn -ErrorAction SilentlyContinue)) { throw "AI 模块缺少函数: $fn" }
    }
    Write-Host "   核心层 $($coreFns.Count) 个新函数 + AI 层 $($aiFns.Count) 个新函数导出正常"
}

Step "布局常量" {
    $L = Get-AetherLayoutConstants
    if ($L.TITLE_BAR -ne 28.0 -or $L.ROW_H -ne 15.0 -or $L.SIDEBAR_W -ne 200.0) {
        throw "布局常量与 layout.rs 不一致: $($L | ConvertTo-Json -Compress)"
    }
    Write-Host "   TITLE_BAR=$($L.TITLE_BAR) ACTIVITY_W=$($L.ACTIVITY_W) SIDEBAR_W=$($L.SIDEBAR_W) HEADER_H=$($L.HEADER_H) ROW_H=$($L.ROW_H)"
}

Step "构建 debug 版" {
    Build-AetherApp
}

# 干净基线：删除历史 hit regions，保证后续断言只反映本次会话
$hrFile = "$root\tests\gui_hit_regions.jsonl"
if (Test-Path $hrFile) { Remove-Item $hrFile -Force }

$script:ws = $null
$script:proc = $null
try {
    Step "启动应用（信任 + 隔离）" {
        $script:ws = New-AetherTestWorkspace -Files @{
            "main.rs" = 'fn main() { println!("hello"); }'
            "a.txt"   = 'line1'
            "b.txt"   = 'line2'
        }
        $script:proc = Start-AetherApp -Folder $script:ws
        $script:win = Get-AetherWindow -Process $script:proc -Isolate
        Write-Host "   dpi=$($script:win.Dpi) scale2=$($script:win.Scale2) fg=$($script:win.IsForeground)"
    }

    Step "PostMessage 点击文件树节点打开文件" {
        $win = $script:win
        $k = $win.Scale2
        $L = Get-AetherLayoutConstants
        $y = [int](($L.TITLE_BAR + $L.HEADER_H + 6 + $L.ROW_H * 1 + $L.ROW_H / 2) * $k)  # 第 0 个节点
        $x = [int]((40 + 10 + 12 + 30) * $k)
        Send-AetherClickMsg -Hwnd $win.Hwnd -X $x -Y $y
        # 状态栏语言区域变化是可靠的打开证据（status_message 不写日志）：
        # 点击 .rs 文件后状态栏出现 status:Rust
        $deadline = (Get-Date).AddSeconds(8)
        $found = $false
        while ((Get-Date) -lt $deadline) {
            Start-Sleep -Milliseconds 300
            if (@(Read-AetherHitRegions -ActionLike "status:Rust").Count -gt 0) { $found = $true; break }
        }
        if (-not $found) { throw "点击后状态栏未出现 Rust 语言（文件未打开）" }
        Write-Host "   状态栏已切换为 Rust 语言"
    }

    Step "日志观测 Get-AetherLog" {
        $log = Get-AetherLog -Tail 5 -Pattern "日志系统初始化"
        if ($log.Lines.Count -eq 0) { throw "日志过滤结果为空" }
        Write-Host "   匹配 $($log.Lines.Count) 行，文件: $(Split-Path $log.Path -Leaf)"
    }

    Step "截图" {
        $p = Save-AetherScreenshot -Window $script:win -Name "selfcheck"
        if (-not (Test-Path $p)) { throw "截图文件不存在" }
        Write-Host "   $p"
    }

    Step "hit regions 读取" {
        $regions = @(Read-AetherHitRegions)
        Write-Host "   已记录 $($regions.Count) 个可点击区域（debug 构建）"
    }

    Step "动作脚本执行器（探索式 DSL）" {
        $win = $script:win
        $k = $win.Scale2
        $L = Get-AetherLayoutConstants
        $y1 = [int](($L.TITLE_BAR + $L.HEADER_H + 6 + $L.ROW_H * 2 + $L.ROW_H / 2) * $k)  # 第 1 个节点
        $x = [int]((40 + 10 + 12 + 30) * $k)
        $actions = @(
            @{type='click'; x=$x; y=$y1},
            @{type='wait'; ms=800},
            @{type='shot'; name='selfcheck_after_click'}
        )
        $results = @(Invoke-AetherActionScript -Window $win -Actions $actions)
        foreach ($r in $results) { if (-not $r.ok) { throw "动作失败: $($r.type) $($r.note)" } }
        # 验证动作后状态栏语言（b.txt 点击后仍为 Rust？b.txt 是 txt——检查 JSON 语言？
        # 简化：验证标签栏出现（文件已打开）——用状态栏 Text 语言验证第 2 个文件打开
        $deadline = (Get-Date).AddSeconds(8)
        $found = $false
        while ((Get-Date) -lt $deadline) {
            Start-Sleep -Milliseconds 300
            if (@(Read-AetherHitRegions -ActionLike "status:Text").Count -gt 0) { $found = $true; break }
        }
        if (-not $found) { throw "第二个文件未打开（状态栏未出现 Text 语言）" }
        Write-Host "   $($results.Count) 个动作全部成功，第二个文件已打开"
    }

    Step "UI 状态快照" {
        $st = Get-AetherUiState -Process $script:proc
        if ($st.WorkingSetMB -le 0) { throw "进程状态异常" }
        Write-Host "   工作集 $($st.WorkingSetMB)MB，hit regions $($st.HitRegionCount) 个，日志尾部 $($st.LogTail.Count) 行"
    }

    Step "组合键注入（Ctrl+B 切换侧栏）" {
        $win = $script:win
        Send-AetherHotkey -Hwnd $win.Hwnd -Modifiers @('Ctrl') -Key 'B'
        Start-Sleep -Milliseconds 500
        Save-AetherScreenshot -Window $win -Name "selfcheck_ctrl_b" | Out-Null
        # 再按一次恢复
        Send-AetherHotkey -Hwnd $win.Hwnd -Modifiers @('Ctrl') -Key 'B'
        Write-Host "   Ctrl+B 组合键注入成功"
    }

    Step "鼠标滚轮注入" {
        $win = $script:win
        $k = $win.Scale2
        $L = Get-AetherLayoutConstants
        $editorX = [int](($L.ACTIVITY_W + $L.SIDEBAR_W + 100) * $k)
        $editorY = [int](($L.TITLE_BAR + $L.TAB_BAR_H + 100) * $k)
        Send-AetherMouseWheel -Hwnd $win.Hwnd -X $editorX -Y $editorY -Delta (-120)
        Write-Host "   滚轮注入成功"
    }

    Step "鼠标移动注入（hover）" {
        $win = $script:win
        $k = $win.Scale2
        $L = Get-AetherLayoutConstants
        $x = [int](($L.ACTIVITY_W + 10 + 12 + 30) * $k)
        $y = [int](($L.TITLE_BAR + $L.HEADER_H + 6 + $L.ROW_H * 1 + $L.ROW_H / 2) * $k)
        Send-AetherMouseMoveMsg -Hwnd $win.Hwnd -X $x -Y $y
        Write-Host "   鼠标移动注入成功"
    }

    Step "智能操作（hit region 查找）" {
        $region = Find-AetherHitRegion -ActionLike "status:*"
        if (-not $region) { throw "未找到状态栏 hit region" }
        Write-Host "   找到状态栏区域: $($region.action) @ ($($region.x),$($region.y))"
    }

    Step "编辑器状态摘要" {
        $state = Get-AetherEditorState -Process $script:proc
        Write-Host "   标签数: $($state.TabCount)，状态栏项: $($state.StatusBarItems.Count)"
    }

    Step "扩展动作脚本 DSL（hotkey/wheel/move）" {
        $win = $script:win
        $k = $win.Scale2
        $L = Get-AetherLayoutConstants
        $editorX = [int](($L.ACTIVITY_W + $L.SIDEBAR_W + 100) * $k)
        $editorY = [int](($L.TITLE_BAR + $L.TAB_BAR_H + 100) * $k)
        $actions = @(
            @{type='hotkey'; modifiers=@('Ctrl'); key='B'},
            @{type='wait';   ms=300},
            @{type='hotkey'; modifiers=@('Ctrl'); key='B'},
            @{type='wait';   ms=300},
            @{type='wheel';  x=$editorX; y=$editorY; delta=120},
            @{type='move';   x=$editorX; y=$editorY}
        )
        $results = @(Invoke-AetherActionScript -Window $win -Actions $actions)
        foreach ($r in $results) { if (-not $r.ok) { throw "动作失败: $($r.type) $($r.note)" } }
        Write-Host "   $($results.Count) 个扩展动作全部成功"
    }
} finally {
    if ($script:proc) { Stop-AetherApp $script:proc }
    if ($script:ws) { Remove-AetherTestWorkspace -Path $script:ws }
}

Step "诊断包生成" {
    $dir = New-AetherDiagBundle -CaseName "selfcheck"
    foreach ($f in @("manifest.json", "app.log")) {
        if (-not (Test-Path (Join-Path $dir $f))) { throw "诊断包缺少 $f" }
    }
    Write-Host "   $dir"
}

Step "报告产物（用例协议）" {
    Start-TestCase "selfcheck_case"
    Invoke-TestStep "占位步骤" { Assert-Condition $true "框架断言工作正常" }
    $failedSteps = Complete-TestCase
    if ($failedSteps -ne 0) { throw "用例协议报告失败" }
    if (-not (Test-Path "$root\tests\reports\selfcheck_case.json")) { throw "报告文件未生成" }
}

if ($failures -eq 0) {
    Write-Host "`n框架自检全部通过 ✔" -ForegroundColor Green
} else {
    Write-Host "`n框架自检存在 $failures 项失败" -ForegroundColor Red
}
exit $failures
