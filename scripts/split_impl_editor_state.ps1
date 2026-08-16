# 自动将 impl EditorState 方法拆分为自由函数 + 薄壳委托
# 用法: pwsh -File scripts\split_impl_editor_state.ps1 <file_path>

param(
    [Parameter(Mandatory=$true)]
    [string]$FilePath
)

$content = Get-Content $FilePath -Raw -Encoding UTF8

# 检查文件是否包含 impl EditorState
if ($content -notmatch 'impl\s+EditorState\s*\{') {
    Write-Output "No impl EditorState found in $FilePath"
    exit 0
}

# 提取 impl EditorState 块中的方法
# 匹配模式：pub fn method_name(&mut self, ...) 或 pub(crate) fn method_name(&self, ...)
$methodPattern = '(?s)(pub(?:\([^)]*\))?\s+(?:fn\s+\w+\s*\([^)]*\)(?:\s*->\s*[^{]+)?)\s*\{)'

# 简单策略：在 impl EditorState { 之前插入自由函数，在 impl 块中替换方法体为委托
# 由于 Rust 语法复杂，这里采用手动辅助的方式：
# 1. 找到 impl EditorState { 的位置
# 2. 在其前面生成自由函数
# 3. 在 impl 块中将方法体替换为委托调用

Write-Output "File contains impl EditorState: $FilePath"
Write-Output "Manual conversion recommended for complex files."
Write-Output "Pattern: fn method(&mut self, args) -> pub fn method(state: &mut EditorState, args)"
