# 用例：资源管理器内联输入行（新建文件/文件夹/重命名）
#
# 覆盖场景：
#   1. 标题栏"新建文件"按钮 → 树内出现带焦点边框的空输入行
#   2. 输入文件名 → Enter 提交 → 磁盘上创建文件并在树中选中
#   3. 新建文件夹 → Enter 提交 → 磁盘上创建目录
#   4. F2 重命名 → 输入行覆盖原行 → Esc 取消不改名
#   5. 空名 Enter → 等效取消，不产生文件
#
# 使用独立临时工作区，测试结束自动清理，不污染真实项目。
# 运行：pwsh -File tests\cases\explorer_inline_input.tests.ps1 [-SkipBuild]
#
# 本用例使用 AI 协作测试框架（AetherTest + AetherAi）：
#   - Start-AetherApp 自动信任工作区 + new_window 隔离（不干扰用户窗口）
#   - Get-AetherWindow -Isolate 移动窗口避免重叠
#   - Send-AetherClickMsg（PostMessage 注入）不受前台锁定影响
param([switch]$SkipBuild)

Import-Module "$PSScriptRoot\..\framework\AetherTest.psm1" -Force
Import-Module "$PSScriptRoot\..\framework\AetherAi.psm1" -Force

# 布局名义常量（与 crates/aether-win32/src/layout.rs 保持一致）
$L = Get-AetherLayoutConstants

Start-TestCase "explorer_inline_input"

$ws = $null
$proc = $null
try {
    if (-not $SkipBuild) { Build-AetherApp }

    # hit regions 文件按帧累积：清空旧帧，避免历史会话的 sidebar:new_file
    # 坐标让 Wait-AetherHitRegion 提前返回（本进程文件树尚未就绪时点击落空）
    $hitFile = Join-Path (Resolve-Path "$PSScriptRoot\..\..").Path "tests\gui_hit_regions.jsonl"
    if (Test-Path $hitFile) { Remove-Item $hitFile -Force }

    $ws = New-AetherTestWorkspace -Files @{
        "main.rs"      = "fn main() {}"
        "zz_readme.md" = "# test"
    }
    $proc = Start-AetherApp -Folder $ws
    $win = Get-AetherWindow -Process $proc -Isolate
    $k = $win.Scale2   # 名义 → 物理换算系数

    # 名义坐标 → 窗口内物理像素
    function NX([double]$v) { [int]($v * $k) }
    # 侧栏头部按钮锚定侧栏右缘，侧栏宽度用户可调且持久化，手算坐标必偏，
    # 一律走语义点击（hit region sidebar:new_file / sidebar:new_folder）
    # 树行：根行 top = 标题栏 + 表头 24 + 间距 6；第 i 个节点行中心
    function RowCenterY([int]$i) { NX ($L.TITLE_BAR + $L.HEADER_H + 6 + $L.ROW_H * ($i + 1) + $L.ROW_H / 2) }
    $rowLabelX = NX ($L.ACTIVITY_W + 10 + 12 + 30)   # base 10 + 一级缩进 12 + 深入 label 区

    Invoke-TestStep "点击新建文件按钮出现空输入行" {
        Wait-AetherHitRegion -ActionLike "sidebar:new_file" -TimeoutMs 8000 | Out-Null
        Invoke-AetherSmartClick -Window $win -ActionLike "sidebar:new_file" | Out-Null
        Save-AetherScreenshot -Window $win -Name "1_newfile_empty_input" | Out-Null
    }

    Invoke-TestStep "输入文件名并 Enter 创建" {
        Send-AetherTextMsg -Hwnd $win.Hwnd -Text "hello_test.rs"
        Save-AetherScreenshot -Window $win -Name "2_newfile_typed" | Out-Null
        Send-AetherKeyMsg -Hwnd $win.Hwnd -Key "{ENTER}" -DelayMs 800
        Assert-PathExists (Join-Path $ws "hello_test.rs")
        Save-AetherScreenshot -Window $win -Name "3_newfile_created" | Out-Null
    }

    Invoke-TestStep "新建文件夹并 Enter 创建" {
        Invoke-AetherSmartClick -Window $win -ActionLike "sidebar:new_folder" | Out-Null
        Send-AetherTextMsg -Hwnd $win.Hwnd -Text "demo_dir"
        Send-AetherKeyMsg -Hwnd $win.Hwnd -Key "{ENTER}" -DelayMs 800
        Assert-PathExists (Join-Path $ws "demo_dir")
        Save-AetherScreenshot -Window $win -Name "4_newfolder_created" | Out-null
    }

    Invoke-TestStep "F2 重命名显示内联输入行且 Esc 不改名" {
        # 行序（目录优先 + 字母序）：demo_dir, hello_test.rs, main.rs, zz_readme.md
        Send-AetherClickMsg -Hwnd $win.Hwnd -X $rowLabelX -Y (RowCenterY 2)   # main.rs
        Send-AetherKeyMsg -Hwnd $win.Hwnd -Key "{F2}" -DelayMs 600
        Save-AetherScreenshot -Window $win -Name "5_rename_input" | Out-Null
        Send-AetherKeyMsg -Hwnd $win.Hwnd -Key "{ESC}" -DelayMs 500
        Assert-PathExists (Join-Path $ws "main.rs")
    }

    Invoke-TestStep "空名 Enter 等效取消" {
        $before = (Get-ChildItem $ws).Count
        Invoke-AetherSmartClick -Window $win -ActionLike "sidebar:new_file" | Out-Null
        Send-AetherKeyMsg -Hwnd $win.Hwnd -Key "{ENTER}" -DelayMs 600
        $after = (Get-ChildItem $ws).Count
        Assert-Condition ($before -eq $after) "空名提交未创建任何文件（$before → $after）"
        Save-AetherScreenshot -Window $win -Name "6_empty_commit_cancelled" | Out-Null
    }
} finally {
    if ($proc) { Stop-AetherApp -Process $proc }
    if ($ws) { Remove-AetherTestWorkspace -Path $ws }
}

exit (Complete-TestCase)
