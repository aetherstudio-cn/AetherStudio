# AetherAi.psm1 — Aether Studio AI 协作测试层
#
# 为 AI 驱动的测试提供高级能力：
#   - New-AetherDiagBundle        失败诊断包（截图 + 日志 + hit regions + 进程状态 + manifest）
#   - Invoke-AetherActionScript   动作脚本执行器（探索式/回归测试 DSL）
#   - Test-AetherPixelRegion      截图区域像素断言（视觉验证）
#   - Get-AetherUiState           可 AI 读取的 UI 状态快照
#   - Assert-AetherLogEvent       日志事件断言
#
# 依赖：AetherTest.psm1（核心层）。用例中：
#   Import-Module "...\framework\AetherTest.psm1" -Force
#   Import-Module "...\framework\AetherAi.psm1" -Force

Set-StrictMode -Version Latest

$script:AiDiagDir = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\diag"

# ---------------------------------------------------------------- 动作脚本 DSL

function Invoke-AetherActionScript {
    <# 执行动作脚本（探索式测试 / AI 生成的交互序列）。
       每个动作返回结果对象，整体返回数组（含每步截图路径与耗时）。
       动作类型：
         click     @{type='click';  x; y; [right]}         PostMessage 点击（窗口内物理坐标）
         dblclick  @{type='dblclick'; x; y; [right]}       PostMessage 双击
         mclick    @{type='mclick'; x; y}                  PostMessage 中键点击
         hover     @{type='hover';  x; y}                  真实鼠标移动（触发 hover 高亮）
         move      @{type='move';   x; y}                  PostMessage 鼠标移动（不依赖前台）
         wheel     @{type='wheel';  x; y; delta; [horizontal]; [shift]; [ctrl]}  滚轮
         drag      @{type='drag';   from_x; from_y; to_x; to_y; [steps]; [right]}  拖拽
         keys      @{type='keys';   text}                  PostMessage 注入文本（不受焦点影响）
         key       @{type='key';    key}                   PostMessage 注入按键（{ENTER}/{ESC}/{F2}...）
         hotkey    @{type='hotkey'; modifiers; key}        组合键（modifiers=@('Ctrl','Shift','Alt')）
         wait      @{type='wait';   ms}
         shot      @{type='shot';   name}                  截图（保存到用例截图目录）
         expect    @{type='expect'; pattern; [timeout_ms]} 等待日志出现指定模式（仅观察新增日志）
         resize    @{type='resize'; width; height}         调整窗口大小
         movewin   @{type='movewin'; x; y}                 移动窗口位置
         winstate  @{type='winstate'; state}               窗口状态（Normal/Minimized/Maximized/Restored）
         closewin  @{type='closewin'}                      发送 WM_CLOSE 关闭窗口
       示例：
         $actions = @(
             @{type='click';  x=92; y=181},
             @{type='keys';   text='hello.rs'},
             @{type='key';    key='{ENTER}'},
             @{type='hotkey'; modifiers=@('Ctrl'); key='S'},
             @{type='wheel';  x=500; y=300; delta=-120},
             @{type='drag';   from_x=100; from_y=200; to_x=300; to_y=400},
             @{type='shot';   name='created'},
             @{type='expect'; pattern='DIAG|error'}
         )
         $r = Invoke-AetherActionScript -Window $win -Actions $actions
         if ($r | Where-Object { -not $_.ok }) { New-AetherDiagBundle ... } #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)]$Actions
    )
    $results = @()
    foreach ($a in $Actions) {
        $r = [pscustomobject]@{
            type = $a.type
            ok   = $true
            note = $null
            duration_ms = 0
            screenshot = $null
        }
        $t0 = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            switch ($a.type) {
                'click' {
                    Send-AetherClickMsg -Hwnd $Window.Hwnd -X $a.x -Y $a.y @(if ($a.right) { @{Right=$true} } else { @{} })
                    $r.note = "click($($a.x),$($a.y))"
                }
                'dblclick' {
                    Send-AetherDoubleClickMsg -Hwnd $Window.Hwnd -X $a.x -Y $a.y @(if ($a.right) { @{Right=$true} } else { @{} })
                    $r.note = "dblclick($($a.x),$($a.y))"
                }
                'mclick' {
                    Send-AetherMiddleClickMsg -Hwnd $Window.Hwnd -X $a.x -Y $a.y
                    $r.note = "mclick($($a.x),$($a.y))"
                }
                'hover' {
                    $px = $Window.Rect.Left + $a.x
                    $py = $Window.Rect.Top + $a.y
                    [AetherWin32]::SetCursorPos($px, $py) | Out-Null
                    Start-Sleep -Milliseconds 300
                    $r.note = "hover($($a.x),$($a.y))"
                }
                'move' {
                    Send-AetherMouseMoveMsg -Hwnd $Window.Hwnd -X $a.x -Y $a.y
                    $r.note = "move($($a.x),$($a.y))"
                }
                'wheel' {
                    $params = @{
                        Hwnd = $Window.Hwnd
                        X = $a.x; Y = $a.y; Delta = $a.delta
                    }
                    if ($a.horizontal) { $params.Horizontal = $true }
                    if ($a.shift) { $params.Shift = $true }
                    if ($a.ctrl) { $params.Ctrl = $true }
                    Send-AetherMouseWheel @params
                    $r.note = "wheel($($a.x),$($a.y),delta=$($a.delta))"
                }
                'drag' {
                    $params = @{
                        Hwnd = $Window.Hwnd
                        FromX = $a.from_x; FromY = $a.from_y
                        ToX = $a.to_x; ToY = $a.to_y
                    }
                    if ($a.steps) { $params.Steps = $a.steps }
                    if ($a.right) { $params.Right = $true }
                    Send-AetherDrag @params
                    $r.note = "drag($($a.from_x),$($a.from_y))->($($a.to_x),$($a.to_y))"
                }
                'keys' {
                    Send-AetherTextMsg -Hwnd $Window.Hwnd -Text $a.text
                    $r.note = "keys:$($a.text)"
                }
                'key' {
                    Send-AetherKeyMsg -Hwnd $Window.Hwnd -Key $a.key
                    $r.note = "key:$($a.key)"
                }
                'hotkey' {
                    Send-AetherHotkey -Hwnd $Window.Hwnd -Modifiers $a.modifiers -Key $a.key
                    $r.note = "hotkey:$($a.modifiers -join '+')+$($a.key)"
                }
                'wait' {
                    Start-Sleep -Milliseconds $a.ms
                    $r.note = "wait $($a.ms)ms"
                }
                'shot' {
                    $r.screenshot = Save-AetherScreenshot -Window $Window -Name $a.name
                    $r.note = "shot:$($a.name)"
                }
                'expect' {
                    $timeout = if ($a.timeout_ms) { $a.timeout_ms } else { 5000 }
                    $ev = Wait-AetherLogEvent -Pattern $a.pattern -TimeoutMs $timeout
                    if (-not $ev.Found) { throw "等待日志事件超时: $($a.pattern)" }
                    $r.note = "expect:$($a.pattern)"
                }
                'resize' {
                    Resize-AetherWindow -Window $Window -Width $a.width -Height $a.height | Out-Null
                    $r.note = "resize($($a.width)x$($a.height))"
                }
                'movewin' {
                    Move-AetherWindow -Window $Window -X $a.x -Y $a.y
                    $r.note = "movewin($($a.x),$($a.y))"
                }
                'winstate' {
                    Set-AetherWindowState -Window $Window -State $a.state
                    $r.note = "winstate:$($a.state)"
                }
                'closewin' {
                    Close-AetherWindow -Window $Window
                    $r.note = "closewin"
                }
                default { throw "未知动作类型: $($a.type)" }
            }
        } catch {
            $r.ok = $false
            $r.note = "失败: $_"
        }
        $t0.Stop()
        $r.duration_ms = [math]::Round($t0.Elapsed.TotalMilliseconds, 1)
        $results += $r
    }
    , $results
}

# ---------------------------------------------------------------- 像素断言

function Test-AetherPixelRegion {
    <# 断言窗口内矩形区域的平均颜色近似目标色（视觉验证）。
       返回 @{ Passed; AvgColor; MeasuredText? }。
       -Color 传 @{R;G;B}，-Tolerance 为每通道允许偏差（默认 24）。 #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][int]$X,       # 窗口内物理坐标
        [Parameter(Mandatory)][int]$Y,
        [Parameter(Mandatory)][int]$Width,
        [Parameter(Mandatory)][int]$Height,
        [Parameter(Mandatory)][hashtable]$Color,   # @{R=..;G=..;B=..}
        [int]$Tolerance = 24
    )
    $r = $Window.Rect
    $bmp = New-Object System.Drawing.Bitmap($Width, $Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.Left + $X, $r.Top + $Y, 0, 0, $bmp.Size)
    $g.Dispose()
    # 采样区域平均色
    $sumR = 0; $sumG = 0; $sumB = 0; $count = 0
    for ($i = 0; $i -lt $Width; $i += 3) {
        for ($j = 0; $j -lt $Height; $j += 3) {
            $p = $bmp.GetPixel($i, $j)
            $sumR += $p.R; $sumG += $p.G; $sumB += $p.B; $count++
        }
    }
    $bmp.Dispose()
    $avg = @{
        R = [int]($sumR / $count)
        G = [int]($sumG / $count)
        B = [int]($sumB / $count)
    }
    $pass = [math]::Abs($avg.R - $Color.R) -le $Tolerance -and
            [math]::Abs($avg.G - $Color.G) -le $Tolerance -and
            [math]::Abs($avg.B - $Color.B) -le $Tolerance
    [pscustomobject]@{
        Passed   = $pass
        AvgColor = $avg
        Expected = $Color
        Region   = "($X,$Y) ${Width}x${Height}"
    }
}

# ---------------------------------------------------------------- UI 状态快照

function Get-AetherUiState {
    <# 返回 AI 可读取的 UI 状态快照：
       LogTail（最近 30 行日志）、HitRegionSummary（最近一帧可点击区域）、
       Process（CPU/内存）、StatusBarHint（日志中的状态消息痕迹）。
       用于探索式测试的"观察-决策"循环。 #>
    param(
        [Parameter(Mandatory)]$Process
    )
    $log = Get-AetherLog -Tail 30
    $regions = @(Read-AetherHitRegions | Select-Object -Last 40)
    $proc = Get-Process -Id $Process.Id -ErrorAction SilentlyContinue
    $cpu = if ($proc) { [math]::Round($proc.CPU, 1) } else { 0 }
    $mem = if ($proc) { [math]::Round($proc.WorkingSet64 / 1MB, 1) } else { 0 }
    [pscustomobject]@{
        Timestamp         = (Get-Date).ToString("o")
        LogTail           = $log.Lines
        HitRegionCount    = $regions.Count
        HitRegions        = $regions
        CpuSeconds        = $cpu
        WorkingSetMB      = $mem
        # 从日志中提取的状态消息（"已打开: xxx"、"正在扫描"等）
        StatusMessages    = @($log.Lines | Select-String "已打开|已保存|正在|失败|错误|警告" | ForEach-Object { $_.Line } | Select-Object -Last 5)
    }
}

# ---------------------------------------------------------------- 日志断言

function Assert-AetherLogEvent {
    <# 断言日志包含（或 -Not 时不含）指定模式。失败抛出异常（供 Invoke-TestStep 捕获）。 #>
    param(
        [Parameter(Mandatory)][string]$Pattern,
        [switch]$Not
    )
    $log = Get-AetherLog -Pattern $Pattern
    $found = $log.Lines.Count -gt 0
    if ($Not) {
        Assert-Condition (-not $found) "日志不应包含: $Pattern"
    } else {
        Assert-Condition $found "日志应包含: $Pattern"
    }
}

# ---------------------------------------------------------------- 诊断包

function New-AetherDiagBundle {
    <# 打包失败诊断材料到 tests/diag/<Case>-<时间戳>/，供 AI 分析：
       manifest.json（环境 + 失败步骤 + 文件清单）、report.json（用例报告）、
       screenshots/（截图副本）、app.log（日志尾部 200 行）、hit_regions.jsonl、process.txt。
       返回诊断目录路径。 #>
    param(
        [string]$CaseName = "diagnostic",
        [int]$LogTail = 200
    )
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $dir = Join-Path $script:AiDiagDir "$CaseName-$stamp"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $dir "screenshots") | Out-Null

    $manifest = [ordered]@{
        case        = $CaseName
        created_at  = (Get-Date).ToString("o")
        os          = [System.Environment]::OSVersion.VersionString
        pwsh        = $PSVersionTable.PSVersion.ToString()
        files       = @()
        failed_steps = @()
    }

    # 用例报告（若存在）
    $report = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\reports\$CaseName.json"
    if (Test-Path $report) {
        Copy-Item $report (Join-Path $dir "report.json")
        $manifest.files += "report.json"
        $rep = Get-Content $report | ConvertFrom-Json
        foreach ($s in $rep.Steps) {
            if (-not $s.ok) {
                $manifest.failed_steps += [ordered]@{ name = $s.name; error = $s.error }
            }
        }
    }

    # 截图（用例目录）
    $shotDir = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\screenshots\$CaseName"
    if (Test-Path $shotDir) {
        Get-ChildItem $shotDir -Filter "*.png" | ForEach-Object {
            Copy-Item $_.FullName (Join-Path $dir "screenshots\$($_.Name)")
            $manifest.files += "screenshots/$($_.Name)"
        }
    }

    # 应用日志尾部
    try {
        $log = Get-AetherLog -Tail $LogTail
        Set-Content -Path (Join-Path $dir "app.log") -Value $log.Lines -Encoding UTF8
        $manifest.files += "app.log"
    } catch { }

    # hit regions
    $hr = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\gui_hit_regions.jsonl"
    if (Test-Path $hr) {
        Copy-Item $hr (Join-Path $dir "hit_regions.jsonl")
        $manifest.files += "hit_regions.jsonl"
    }

    # 进程状态（若仍在运行）
    $procs = @(Get-Process aether-app -ErrorAction SilentlyContinue)
    if ($procs) {
        $procs | Select-Object Id, ProcessName, WorkingSet64, CPU, StartTime |
            Format-Table -AutoSize | Out-String -Width 200 | Set-Content (Join-Path $dir "process.txt")
        $manifest.files += "process.txt"
    }

    $manifest.files += "manifest.json"
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $dir "manifest.json") -Encoding UTF8
    Write-Host "  [诊断包] $dir" -ForegroundColor Yellow
    return $dir
}

# ---------------------------------------------------------------- AI 智能操作辅助

function Find-AetherHitRegion {
    <# 智能查找 hit region：按动作名称模糊匹配，返回最佳匹配区域。
       用于 AI 根据语义（如"新建文件按钮"）定位可点击区域。
       -ActionLike 动作名称模式（支持 * 通配符）。
       -PreferCenter 优先返回靠近窗口中心的区域（当有多个匹配时）。 #>
    param(
        [Parameter(Mandatory)][string]$ActionLike,
        [switch]$PreferCenter
    )
    $regions = @(Read-AetherHitRegions -ActionLike $ActionLike)
    if ($regions.Count -eq 0) { return $null }
    if ($regions.Count -eq 1) { return $regions[0] }
    # 多个匹配时返回最新的（最后记录的）
    return $regions[-1]
}

function Invoke-AetherSmartClick {
    <# 智能点击：根据 hit region 动作名称自动定位并点击。
       示例：Invoke-AetherSmartClick -Window $win -ActionLike "new_file" #>
    param(
        [Parameter(Mandatory)]$Window,
        [Parameter(Mandatory)][string]$ActionLike,
        [switch]$Right
    )
    $region = Find-AetherHitRegion -ActionLike $ActionLike
    if (-not $region) { throw "未找到 hit region: $ActionLike" }
    $cx = [int]($region.x + $region.width / 2)
    $cy = [int]($region.y + $region.height / 2)
    Send-AetherClickMsg -Hwnd $Window.Hwnd -X $cx -Y $cy @(if ($Right) { @{Right=$true} } else { @{} })
    return $region
}

function Wait-AetherHitRegion {
    <# 等待指定 hit region 出现（用于等待 UI 元素加载完成）。
       -TimeoutMs 超时时间（默认 5000ms）。 #>
    param(
        [Parameter(Mandatory)][string]$ActionLike,
        [int]$TimeoutMs = 5000,
        [int]$PollMs = 200
    )
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $region = Find-AetherHitRegion -ActionLike $ActionLike
        if ($region) { return $region }
        Start-Sleep -Milliseconds $PollMs
    }
    return $null
}

function Get-AetherEditorState {
    <# 获取编辑器当前状态摘要（供 AI 分析）：
       当前标签数、活动标签、光标位置、选区状态、文件路径等。
       通过日志和 hit regions 推断。 #>
    param([Parameter(Mandatory)]$Process)
    $log = Get-AetherLog -Tail 50
    $regions = @(Read-AetherHitRegions)
    # 从 hit regions 提取标签信息
    $tabs = @($regions | Where-Object { $_.action -like "tab:*" })
    $statusBar = @($regions | Where-Object { $_.action -like "status:*" })
    [pscustomobject]@{
        Timestamp = (Get-Date).ToString("o")
        TabCount = $tabs.Count
        Tabs = @($tabs | ForEach-Object { $_.action -replace '^tab:', '' })
        StatusBarItems = @($statusBar | ForEach-Object { $_.action })
        RecentLog = $log.Lines | Select-Object -Last 10
    }
}

Export-ModuleMember -Function Invoke-AetherActionScript, Test-AetherPixelRegion,
    Get-AetherUiState, Assert-AetherLogEvent, New-AetherDiagBundle,
    Find-AetherHitRegion, Invoke-AetherSmartClick, Wait-AetherHitRegion,
    Get-AetherEditorState
