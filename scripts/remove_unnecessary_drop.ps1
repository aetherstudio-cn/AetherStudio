# 删除不必要的 drop(st) 调用
# 规则：drop(st) 后面紧跟 invalidate_window(hwnd) + return/} 的是不必要的
# 保留：drop(st) 后面需要再次 state.borrow_mut() 或调用需要 state 的函数

$srcDir = "d:\Application\牧羊人编辑器\crates\aether-win32\src"

$totalRemoved = 0
$filesModified = 0

Get-ChildItem -Path $srcDir -Recurse -Filter "*.rs" | ForEach-Object {
    $file = $_.FullName
    $lines = Get-Content $file -Encoding UTF8
    $newLines = [System.Collections.Generic.List[string]]::new()
    $removed = 0
    $i = 0

    while ($i -lt $lines.Count) {
        $line = $lines[$i]

        # 检测 drop(st); 行
        if ($line -match '^\s*drop\(st\);?\s*$') {
            # 查看后续行，判断是否必要
            $nextNonEmpty = $null
            $j = $i + 1
            while ($j -lt $lines.Count -and $lines[$j] -match '^\s*$') {
                $j++
            }
            if ($j -lt $lines.Count) {
                $nextNonEmpty = $lines[$j].Trim()
            }

            # 不必要的模式：drop(st) 后面紧跟 invalidate_window 或 return 或 }
            # 且 invalidate_window 后面紧跟 return 或 }
            $isUnnecessary = $false

            if ($nextNonEmpty -match '^invalidate_window') {
                # 看 invalidate_window 后面的行
                $k = $j + 1
                while ($k -lt $lines.Count -and $lines[$k] -match '^\s*$') {
                    $k++
                }
                if ($k -lt $lines.Count) {
                    $afterInvalidate = $lines[$k].Trim()
                    if ($afterInvalidate -match '^(return|Some\(|\})') {
                        $isUnnecessary = $true
                    }
                } else {
                    $isUnnecessary = $true
                }
            }

            if ($isUnnecessary) {
                $removed++
                $i++
                continue
            }
        }

        $newLines.Add($line)
        $i++
    }

    if ($removed -gt 0) {
        Set-Content -Path $file -Value $newLines -Encoding UTF8
        $filesModified++
        $totalRemoved += $removed
        Write-Output "$($_.Name): removed $removed drop(st)"
    }
}

Write-Output "`nTotal: removed $totalRemoved drop(st) from $filesModified files"
