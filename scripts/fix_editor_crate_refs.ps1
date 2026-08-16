# 修正 aether-editor crate 中的 crate:: 引用为外部 crate 路径

$srcDir = "d:\Application\牧羊人编辑器\crates\aether-editor\src"

# crate::xxx → aether_xxx::xxx 的映射
$crateMap = @{
    "crate::activity_bar" = "aether_ui::activity_bar"
    "crate::ai_panel" = "aether_ai_panel::ai_panel"
    "crate::command_palette" = "aether_ui::command_palette"
    "crate::dialogs" = "aether_ui::dialogs"
    "crate::git" = "aether_ui::git"
    "crate::input" = "aether_ui::input"
    "crate::layout" = "aether_ui::layout"
    "crate::menu_bar" = "aether_ui::menu_bar"
    "crate::ssh" = "aether_ui::ssh"
    "crate::status_bar" = "aether_ui::status_bar"
    "crate::terminal" = "aether_terminal::terminal"
    "crate::new_project_dialog" = "aether_ui::new_project_dialog"
    "crate::context_menu" = "aether_ui::context_menu"
    "crate::icons" = "aether_ui::icons"
    "crate::ime" = "aether_ui::ime"
    "crate::settings" = "aether_ui::settings"
    "crate::sandbox_eval" = "aether_ui::sandbox_eval"
    "crate::search_panel" = "aether_ui::search_panel"
    "crate::open_tabs" = "aether_ui::open_tabs"
    "crate::recent_projects" = "aether_ui::recent_projects"
    "crate::updater" = "aether_ui::updater"
    "crate::user_menu" = "aether_ui::user_menu"
    "crate::welcome" = "aether_ui::welcome"
    "crate::tooltip" = "aether_ui::tooltip"
    "crate::hit_test" = "aether_ui::hit_test"
    "crate::bitmap_loader" = "aether_ui::bitmap_loader"
    "crate::logging" = "aether_ui::logging"
    "crate::crash_guard" = "aether_ui::crash_guard"
    "crate::recycle_bin" = "aether_ui::recycle_bin"
    "crate::uia" = "aether_ui::uia"
    "crate::theme" = "aether_ui::theme"
    "crate::cursor" = "aether_ui::cursor"
    "crate::keyboard_hook" = "aether_ui::keyboard_hook"
    "crate::file_drag_drop" = "aether_ui::file_drag_drop"
    "crate::tab_context_menu" = "aether_ui::tab_context_menu"
    "crate::activity_bar_context_menu" = "aether_ui::activity_bar_context_menu"
    "crate::icons_svg" = "aether_ui::icons_svg"
    "crate::icons_svg_defs" = "aether_ui::icons_svg_defs"
    "crate::render_context" = "aether_render_win::render_context"
    "crate::window" = "crate::window"  # 保持在 aether-win32 中
    "crate::power" = "crate::power"    # 保持在 aether-win32 中
    "crate::render" = "aether_render_win::render"
    "crate::editor" = "crate::editor"  # 保持在当前 crate
    "crate::events" = "crate::events"
    "crate::auto_save" = "crate::auto_save"
    "crate::dirty_rect" = "crate::dirty_rect"
    "crate::focus_manager" = "crate::focus_manager"
    "crate::undo_delete" = "crate::undo_delete"
    "crate::inline_completion" = "crate::inline_completion"
    "crate::tabs" = "crate::tabs"
}

$totalReplacements = 0
$filesModified = 0

Get-ChildItem -Path $srcDir -Recurse -Filter "*.rs" | ForEach-Object {
    $file = $_.FullName
    $content = Get-Content $file -Raw -Encoding UTF8
    $original = $content
    $fileReplacements = 0

    foreach ($old in $crateMap.Keys) {
        $new = $crateMap[$old]
        if ($old -ne $new -and $content -match [regex]::Escape($old)) {
            $count = ([regex]::Matches($content, [regex]::Escape($old))).Count
            $content = $content -replace [regex]::Escape($old), $new
            $fileReplacements += $count
        }
    }

    if ($content -ne $original) {
        Set-Content -Path $file -Value $content -Encoding UTF8 -NoNewline
        $filesModified++
        $totalReplacements += $fileReplacements
        Write-Output "$($_.Name): $fileReplacements replacements"
    }
}

Write-Output "`nTotal: $totalReplacements replacements in $filesModified files"
