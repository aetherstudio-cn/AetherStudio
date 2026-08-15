# EditorState 字段路径批量迁移脚本 v3
# 处理跨行引用：self\n.field 和 self.field

$srcDir = "d:\Application\牧羊人编辑器\crates\aether-win32\src"

$fieldMap = @{
    "hwnd" = "win"; "d2d_factory" = "win"; "render_ctx" = "win"; "text_renderer" = "win"
    "theme" = "win"; "window_width" = "win"; "window_height" = "win"; "dpi_scale" = "win"
    "is_maximized" = "win"; "is_main_window" = "win"; "titlebar_hover_button" = "win"
    "titlebar_back_btn_x" = "win"; "titlebar_forward_btn_x" = "win"; "dirty_tracker" = "win"
    "end_draw_fail_streak" = "win"; "logo_bitmap" = "win"; "image_bitmap" = "win"
    "image_zoom" = "win"; "image_offset_x" = "win"; "image_offset_y" = "win"
    "gpu_highlighter" = "win"; "gpu_highlight_config" = "win"
    "content" = "editor"; "is_selecting" = "editor"; "tab_bar" = "editor"; "find" = "editor"
    "multi_cursor" = "editor"; "composition" = "editor"; "markdown_preview" = "editor"
    "markdown_toggle_btn" = "editor"; "inline_completion_service" = "editor"
    "file_tree" = "fs"; "current_folder" = "fs"; "folder_generation" = "fs"
    "selected_file_node" = "fs"; "hover_file_node" = "fs"; "file_tree_root_expanded" = "fs"
    "file_tree_visible_rows" = "fs"; "file_tree_rows_dirty" = "fs"; "file_tree_rows_tree_len" = "fs"
    "hover_file_tree_root" = "fs"; "file_tree_input" = "fs"; "file_tree_loading_nodes" = "fs"
    "file_tree_new_file_btn" = "fs"; "file_tree_new_folder_btn" = "fs"; "file_tree_open_folder_btn" = "fs"
    "is_loading_folder" = "fs"; "fs_watch_until" = "fs"; "fs_last_root_sig" = "fs"
    "sidebar_scroll_y" = "fs"; "delete_undo_stack" = "fs"; "file_drag" = "fs"
    "ai_panel" = "ai"; "workspace_ai_sessions" = "ai"; "current_workspace_ai_session" = "ai"
    "terminal_panel" = "terminal"; "saved_ime_himc" = "terminal"; "bottom_panel_tab" = "terminal"
    "diagnostics" = "lsp"; "lsp" = "lsp"
    "status_message" = "ui"; "key_map" = "ui"; "layout" = "ui"; "menu_bar" = "ui"
    "activity_bar" = "ui"; "status_bar" = "ui"; "activity_view" = "ui"; "sidebar_content" = "ui"
    "recent_projects" = "ui"; "command_palette" = "ui"; "search_panel" = "ui"
    "new_project_dialog" = "ui"; "update_available_version" = "ui"; "update_checking" = "ui"
    "hover_sidebar_resize" = "ui"; "welcome_hover_action" = "ui"; "welcome_focus_action" = "ui"
    "icons" = "ui"; "git_cloning" = "ui"; "app_settings" = "ui"; "settings_panel" = "ui"
    "sandbox_eval" = "ui"; "tabs_panel" = "ui"; "git_panel" = "ui"; "git" = "ui"
    "user_menu" = "ui"; "context_menus" = "ui"; "tooltip_state" = "ui"; "ime" = "ui"
    "mouse_press" = "input"; "hover" = "input"; "prev" = "input"
    "focus_manager" = "input"; "event_queue" = "input"
}

$sortedFields = $fieldMap.Keys | Sort-Object { $_.Length } -Descending

$totalReplacements = 0
$filesModified = 0

Get-ChildItem -Path $srcDir -Recurse -Filter "*.rs" | ForEach-Object {
    $file = $_.FullName
    $content = Get-Content $file -Raw -Encoding UTF8
    $original = $content

    if ($content -notmatch "impl\s+EditorState") {
        return
    }

    $fileReplacements = 0

    foreach ($field in $sortedFields) {
        $domain = $fieldMap[$field]
        # 匹配 self.field 或 self\n.field 或 self\n    .field 等跨行模式
        # 但不匹配 self.domain.field（已有域前缀）或 self.field_xxx（长字段名）
        $pattern = "(?<![.\w])self\s*\n\s*\.$field(?![\w])|(?<![.\w])self\.$field(?![\w])"
        $replacement = "self.$domain.$field"
        $newContent = [regex]::Replace($content, $pattern, { param($m)
            if ($m.Value -match "^\s*self\s*\n") {
                # 跨行：保持换行格式
                $m.Value -replace "self\s*\n\s*\.$field", "self`n    .$domain.$field"
            } else {
                "self.$domain.$field"
            }
        })
        if ($newContent -ne $content) {
            $count = ([regex]::Matches($content, $pattern)).Count
            $fileReplacements += $count
            $content = $newContent
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
