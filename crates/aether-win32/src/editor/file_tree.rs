use super::*;

impl EditorState {
    /// 命中检测：侧边栏右侧的宽度调整手柄
    /// 仅当侧边栏可见、活动栏已渲染（侧边栏真实存在宽度）时返回 true
    pub fn hit_test_sidebar_resize(&self, mouse_x: f32, mouse_y: f32) -> bool {
        if !self.ui.layout.sidebar_visible {
            return false;
        }
        let sidebar = self.ui.layout.sidebar_region();
        if mouse_y < sidebar.y || mouse_y >= sidebar.y + sidebar.height {
            return false;
        }
        let handle_x = sidebar.x + sidebar.width;
        mouse_x >= handle_x - SIDEBAR_RESIZE_GRAB && mouse_x <= handle_x + SIDEBAR_RESIZE_GRAB
    }
    /// 文件树根目录行（工作区文件夹名）的起始 Y 坐标（相对侧边栏顶部），
    /// 与 render_tree_nodes 中的 `y + header_h + 6.0 - sidebar_scroll_y` 严格一致。
    ///
    /// 之前三处（render、handle_file_tree_click、update_local_tree_hover、rbutton_down）
    /// 各自硬编码 34.0，未考虑 dpi_scale 和 sidebar_scroll_y，
    /// 导致高 DPI / 滚动时点击/悬停位置与渲染节点错位，
    /// 表现为"焦点与选中状态分离"。
    /// 内联输入行已改为树内行（见 file_tree_input_row_geom），不再整体下移。
    /// 使用逻辑像素（与 TAB_BAR_HEIGHT 一致，不乘 dpi_scale，Direct2D 自动处理缩放）
    pub fn file_tree_list_start_y(&self) -> f32 {
        let header_h = crate::layout::FILE_TREE_HEADER_HEIGHT;
        header_h + 6.0 - self.fs.sidebar_scroll_y
    }

    /// 树节点列表的起始 Y 坐标：根目录行之下一行（根行高度 = 节点行高）
    pub fn file_tree_nodes_start_y(&self) -> f32 {
        self.file_tree_list_start_y() + crate::layout::FILE_TREE_ROW_HEIGHT * self.win.dpi_scale
    }
    /// 开始文件树内联输入（新建文件/文件夹/重命名）。
    /// 新建时基于当前选中项定位目标目录（VS Code 行为）：
    /// 选中目录 → 其内；选中文件 → 其父目录；无选中 → 工作区根。
    pub fn start_file_tree_input(&mut self, kind: FileTreeInputKind) {
        let parent = self.file_tree_new_target_dir();
        self.start_file_tree_input_in(kind, parent);
    }
    /// 新建入口的目标父目录：选中目录 → 该目录；选中文件 → 其父目录；
    /// 无选中/顶层文件 → 工作区根（None）
    pub(crate) fn file_tree_new_target_dir(&self) -> Option<u32> {
        let idx = self.fs.selected_file_node?;
        let tree = self.fs.file_tree.as_ref()?;
        let node = tree.get_node(idx)?;
        if node.kind == FileKind::Directory {
            Some(idx)
        } else if node.parent_idx != u32::MAX {
            Some(node.parent_idx)
        } else {
            None
        }
    }
    /// 开始文件树内联输入，可指定父文件夹节点（新建时在其内创建；
    /// None 则在工作区根目录）。重命名忽略 parent_node，仍使用选中节点。
    pub fn start_file_tree_input_in(&mut self, kind: FileTreeInputKind, parent_node: Option<u32>) {
        // 未打开文件夹时无处创建，直接提示而不进入输入态
        if self.fs.current_folder.is_none() {
            self.ui.status_message = "请先打开文件夹".to_string();
            return;
        }
        let (default_name, target_node) = match kind {
            // VS Code 行为：新建从空名开始（免去先删默认名的操作），
            // 空名提交视为取消
            FileTreeInputKind::NewFile | FileTreeInputKind::NewFolder => {
                (String::new(), parent_node)
            }
            FileTreeInputKind::Rename => {
                let node_idx = self.fs.selected_file_node;
                let name = node_idx.and_then(|idx| {
                    self.fs.file_tree.as_ref().and_then(|tree| {
                        tree.get_node(idx)
                            .map(|node| tree.get_name(node).to_string())
                    })
                });
                (name.unwrap_or_default(), node_idx)
            }
        };
        // 新建时输入行渲染在目标目录子列表开头：确保目标目录已加载并展开
        if !matches!(kind, FileTreeInputKind::Rename) {
            self.fs.file_tree_root_expanded = true;
            if let Some(p) = target_node {
                let _ = self.ensure_node_loaded(p);
                if let Some(tree) = self.fs.file_tree.as_mut() {
                    if let Some(node) = tree.get_node_mut(p) {
                        node.is_expanded = true;
                    }
                }
            }
            self.mark_file_tree_rows_dirty();
        }
        self.fs.file_tree_input = Some(FileTreeInput {
            kind,
            value: default_name,
            caret_visible: true,
            composition: None,
            target_node,
        });
        // 启动光标闪烁定时器（此前遗漏，输入行光标不会闪烁）
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
                self.win.hwnd,
                crate::window::CARET_TIMER_ID,
                530,
                None,
            );
        }
        self.win.dirty_tracker.mark_region(
            self.ui.layout.sidebar_region().x,
            self.ui.layout.sidebar_region().y,
            self.ui.layout.sidebar_region().width,
            self.ui.layout.sidebar_region().height,
            crate::dirty_rect::DirtyRegionType::Sidebar,
        );
    }
    /// 确认文件树内联输入，执行新建操作
    pub fn confirm_file_tree_input(&mut self) {
        let Some(input) = self.fs.file_tree_input.take() else {
            return;
        };
        let Some(base_path) = self.fs.current_folder.clone() else {
            self.ui.status_message = "请先打开文件夹".to_string();
            self.win.dirty_tracker.mark_region(
                self.ui.layout.sidebar_region().x,
                self.ui.layout.sidebar_region().y,
                self.ui.layout.sidebar_region().width,
                self.ui.layout.sidebar_region().height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            return;
        };

        let name = input.value.trim();
        if name.is_empty() {
            // VS Code 行为：空名提交视为取消，不提示错误
            self.win.dirty_tracker.mark_region(
                self.ui.layout.sidebar_region().x,
                self.ui.layout.sidebar_region().y,
                self.ui.layout.sidebar_region().width,
                self.ui.layout.sidebar_region().height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            return;
        }

        // 验证文件名不含 Windows 非法字符
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        if name.contains(INVALID_CHARS) {
            let bad: String = name.chars().filter(|c| INVALID_CHARS.contains(c)).collect();
            self.ui.status_message = format!("文件名不能包含: {}", bad);
            // 验证失败时保留输入框，让用户修改后重试
            self.fs.file_tree_input = Some(input);
            self.win.dirty_tracker.mark_region(
                self.ui.layout.sidebar_region().x,
                self.ui.layout.sidebar_region().y,
                self.ui.layout.sidebar_region().width,
                self.ui.layout.sidebar_region().height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            return;
        }

        // 新建目标基准目录：指定了父文件夹节点则在其内创建，否则在工作区根目录
        let create_base = if matches!(input.kind, FileTreeInputKind::Rename) {
            base_path.clone()
        } else {
            input
                .target_node
                .and_then(|idx| self.get_node_path(idx))
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| base_path.clone())
        };
        let target = create_base.join(name);
        // 重命名的重名检查在 Rename 分支内基于旧路径父目录完成，
        // 此处仅对新建操作检查，避免子目录重命名被根目录同名项误拦
        if !matches!(input.kind, FileTreeInputKind::Rename) && target.exists() {
            self.ui.status_message = format!("{} 已存在", name);
            self.win.dirty_tracker.mark_region(
                self.ui.layout.sidebar_region().x,
                self.ui.layout.sidebar_region().y,
                self.ui.layout.sidebar_region().width,
                self.ui.layout.sidebar_region().height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            return;
        }

        match input.kind {
            FileTreeInputKind::NewFile => {
                if let Err(e) = std::fs::write(&target, "") {
                    self.ui.status_message = format!("创建文件失败: {}", e);
                } else {
                    self.ui.status_message = format!("已创建文件: {}", name);
                    // 轻量刷新：保留展开状态，不重启 LSP / 不重开 README
                    self.refresh_file_tree_light();
                    self.select_node_by_path(&target);
                    self.load_file(target);
                }
            }
            FileTreeInputKind::NewFolder => {
                if let Err(e) = std::fs::create_dir(&target) {
                    self.ui.status_message = format!("创建文件夹失败: {}", e);
                } else {
                    self.ui.status_message = format!("已创建文件夹: {}", name);
                    self.refresh_file_tree_light();
                    self.select_node_by_path(&target);
                }
            }
            FileTreeInputKind::Rename => {
                if let Some(node_idx) = input.target_node {
                    if let Some(old_path) = self.get_node_path(node_idx) {
                        let parent = old_path.parent();
                        let new_path = parent
                            .map(|p| p.join(name))
                            .unwrap_or_else(|| base_path.join(name));
                        if old_path == new_path {
                            // 名称未改变，无需操作
                        } else if new_path.exists() {
                            self.ui.status_message = format!("{} 已存在", name);
                            self.fs.file_tree_input = Some(input);
                            self.win.dirty_tracker.mark_region(
                                self.ui.layout.sidebar_region().x,
                                self.ui.layout.sidebar_region().y,
                                self.ui.layout.sidebar_region().width,
                                self.ui.layout.sidebar_region().height,
                                crate::dirty_rect::DirtyRegionType::Sidebar,
                            );
                            return;
                        } else if let Err(e) = std::fs::rename(&old_path, &new_path) {
                            self.ui.status_message = format!("重命名失败: {}", e);
                        } else {
                            self.ui.status_message = format!("已重命名为: {}", name);
                            // 如果重命名的文件当前已打开，更新标签页路径
                            let old_path_str = old_path.to_string_lossy().to_string();
                            for tab in &mut self.editor.tab_bar.tabs {
                                if let Some(file_content) = tab.as_file_mut() {
                                    if let Some(ref fp) = file_content.file_path {
                                        if fp.to_string_lossy() == old_path_str {
                                            file_content.file_path = Some(new_path.clone());
                                        }
                                    }
                                }
                            }
                            if let Some(ref active_path) = self.editor.content.file_path {
                                if active_path.to_string_lossy() == old_path_str {
                                    self.editor.content.file_path = Some(new_path.clone());
                                }
                            }
                            self.refresh_file_tree_light();
                            self.select_node_by_path(&new_path);
                        }
                    }
                }
            }
        }
    }
    /// 取消文件树内联输入
    pub fn cancel_file_tree_input(&mut self) {
        if self.fs.file_tree_input.take().is_some() {
            self.win.dirty_tracker.mark_region(
                self.ui.layout.sidebar_region().x,
                self.ui.layout.sidebar_region().y,
                self.ui.layout.sidebar_region().width,
                self.ui.layout.sidebar_region().height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
        }
    }
    /// 按绝对路径查找节点并选中（刷新树后定位新建/重命名的节点）
    fn select_node_by_path(&mut self, path: &std::path::Path) {
        let n = self
            .fs
            .file_tree
            .as_ref()
            .map(|t| t.len() as u32)
            .unwrap_or(0);
        for idx in 0..n {
            if self.get_node_path(idx).as_deref() == Some(path) {
                self.fs.selected_file_node = Some(idx);
                return;
            }
        }
    }
    /// 内联输入行几何（相对侧边栏左上角，已含 dpi 缩放与滚动偏移）：
    /// 返回 (行顶 y, 图标列左缘 x, 文本框左缘 x)。
    /// 新建：目标目录子列表第一行；重命名：目标节点自身行。
    /// 行序公式与 render_tree_nodes / skip_tree_nodes 的占位逻辑严格一致。
    /// 依赖 file_tree_visible_rows：渲染帧会先 ensure_file_tree_rows()，
    /// 鼠标路径读到的至多是上一帧布局（误差一帧，可接受）。
    pub(crate) fn file_tree_input_row_geom(&self) -> Option<(f32, f32, f32)> {
        let input = self.fs.file_tree_input.as_ref()?;
        let s = self.win.dpi_scale;
        let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * s;
        let nodes_top = self.file_tree_nodes_start_y();
        let (row, depth) = match (input.kind, input.target_node) {
            (FileTreeInputKind::Rename, Some(idx)) => {
                let i = self
                    .fs
                    .file_tree_visible_rows
                    .iter()
                    .position(|&r| r == idx)?;
                let depth = self.fs.file_tree.as_ref()?.get_node(idx)?.depth as f32;
                (i as f32, depth)
            }
            (FileTreeInputKind::Rename, None) => return None,
            (_, Some(p)) => {
                let i = self
                    .fs
                    .file_tree_visible_rows
                    .iter()
                    .position(|&r| r == p)?;
                let depth = self.fs.file_tree.as_ref()?.get_node(p)?.depth as f32 + 1.0;
                (i as f32 + 1.0, depth)
            }
            (_, None) => (0.0, 0.0),
        };
        let top = nodes_top + row * row_h;
        // 根目录行占第 0 层，节点整体缩进一级（与 render_tree_nodes 一致）
        let item_left = 10.0 * s + (depth + 1.0) * crate::layout::FILE_TREE_INDENT * s;
        let text_left = item_left + crate::layout::FILE_TREE_ARROW_COL * s + 4.0 * s;
        Some((top, item_left, text_left))
    }
    /// 刷新文件树（重新扫描当前文件夹）
    pub fn refresh_file_tree(&mut self) {
        if let Some(path) = self.fs.current_folder.clone() {
            // 信任检查在 open_folder 之前（不持有 RefCell 借用，避免模态框重入 panic）
            if crate::editor::files::check_workspace_trust(self.win.hwnd, &path) {
                self.open_folder(path);
            } else {
                self.ui.status_message = "已取消打开不受信任的工作区".to_string();
            }
        }
    }

    /// 轻量刷新文件树：同步重建（根 + 已展开子目录），并**保留展开状态**。
    /// 不重启 LSP、不保存设置、不显示加载 spinner、不自动打开 README。
    /// 用于 AI 新建/删除文件后即时同步资源管理器，避免用户手动刷新。
    pub fn refresh_file_tree_light(&mut self) {
        let Some(folder) = self.fs.current_folder.clone() else {
            return;
        };
        let expanded = self.capture_expanded_dir_paths();
        let mut tree = FileTree::new();
        Self::rebuild_tree_level(&mut tree, &folder, u32::MAX, 0, &folder, &expanded);
        self.fs.file_tree = Some(tree);
        self.mark_file_tree_rows_dirty();
        // 文件可能变化，刷新 Git 状态
        self.ui.git.detect(&folder);
        self.win.dirty_tracker.mark_full_window();
    }

    /// 收集当前已展开目录的相对路径集合（刷新后据此恢复展开状态）
    fn capture_expanded_dir_paths(&self) -> std::collections::HashSet<PathBuf> {
        let mut set = std::collections::HashSet::new();
        let (Some(tree), Some(root)) =
            (self.fs.file_tree.as_ref(), self.fs.current_folder.as_ref())
        else {
            return set;
        };
        let n = tree.len() as u32;
        for i in 0..n {
            let Some(node) = tree.get_node(i) else {
                continue;
            };
            if node.kind == FileKind::Directory && node.is_expanded {
                if let Some(abs) = self.get_node_path(i) {
                    if let Ok(rel) = abs.strip_prefix(root) {
                        set.insert(rel.to_path_buf());
                    }
                }
            }
        }
        set
    }

    /// 同步重建某一层目录；仅对"之前已展开"的子目录递归展开并加载（有界，避免全盘扫描）。
    fn rebuild_tree_level(
        tree: &mut FileTree,
        abs_dir: &std::path::Path,
        parent_idx: u32,
        depth: u8,
        root: &std::path::Path,
        expanded: &std::collections::HashSet<PathBuf>,
    ) {
        for entry in scan_file_tree_entries(&abs_dir.to_path_buf()) {
            let idx = tree.add_node(&entry.name, entry.kind, parent_idx, depth);
            if entry.kind == FileKind::Directory {
                let is_exp = entry
                    .path
                    .strip_prefix(root)
                    .ok()
                    .map(|rel| expanded.contains(rel))
                    .unwrap_or(false);
                if let Some(node) = tree.get_node_mut(idx) {
                    node.is_expanded = is_exp;
                    node.is_loaded = is_exp;
                }
                if is_exp {
                    Self::rebuild_tree_level(tree, &entry.path, idx, depth + 1, root, expanded);
                }
            }
        }
    }

    /// 工作区根目录的轻量签名（子项名称+类型+修改时间哈希）。
    /// 用于 AI 终端命令后检测文件变化，仅在变化时才刷新，避免无谓重建/闪烁。
    /// 除根目录外，当前已展开的子目录也纳入签名——否则 `mkdir src\utils` 这类
    /// 嵌套变化不会反映到根目录签名上，文件树无法自动刷新。
    pub fn workspace_root_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(folder) = self.fs.current_folder.as_ref() {
            Self::hash_dir_level(&mut hasher, folder);
            // 已展开目录的子项也参与签名（排序保证签名稳定）
            let mut expanded: Vec<PathBuf> =
                self.capture_expanded_dir_paths().into_iter().collect();
            expanded.sort();
            for rel in expanded {
                let abs = folder.join(&rel);
                rel.hash(&mut hasher); // 分隔不同目录的子项序列
                Self::hash_dir_level(&mut hasher, &abs);
            }
        }
        hasher.finish()
    }

    /// 把某目录一层的子项（名称+类型+mtime）写入哈希（排序保证确定性）
    fn hash_dir_level(hasher: &mut impl std::hash::Hasher, dir: &std::path::Path) {
        use std::hash::Hash;
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut items: Vec<(String, bool, u64)> = Vec::new();
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                items.push((name, is_dir, mtime));
            }
            items.sort();
            for it in items {
                it.hash(hasher);
            }
        }
    }

    /// 在 Windows 文件资源管理器中打开当前工作区文件夹。
    /// 通过 ShellExecuteW 调用系统 explorer.exe，无纯 Rust 依赖。
    pub fn open_in_file_explorer(&mut self) {
        let Some(folder) = self.fs.current_folder.clone() else {
            self.ui.status_message = "请先打开文件夹".to_string();
            return;
        };
        let path_str = folder.to_string_lossy().to_string();
        let wide: Vec<u16> = path_str.encode_utf16().chain(Some(0)).collect();
        unsafe {
            use windows::Win32::UI::Shell::ShellExecuteW;
            let operation: Vec<u16> = "open\0".encode_utf16().collect();
            let _ = ShellExecuteW(
                None,
                windows::core::PCWSTR(operation.as_ptr()),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
        self.ui.status_message = format!("已在文件资源管理器中打开: {}", path_str);
    }
    /// 复制当前工作区文件夹的绝对路径到剪贴板。
    pub fn copy_folder_path(&mut self) {
        let Some(folder) = self.fs.current_folder.clone() else {
            self.ui.status_message = "请先打开文件夹".to_string();
            return;
        };
        let path_str = folder.to_string_lossy().to_string();
        if Self::set_clipboard_text(&path_str) {
            self.ui.status_message = format!("已复制路径: {}", path_str);
        } else {
            self.ui.status_message = "复制路径失败".to_string();
        }
    }
    /// 复制文件节点的绝对路径到剪贴板。
    pub fn copy_node_path(&mut self, node_idx: u32) {
        let Some(path) = self.get_node_path(node_idx) else {
            self.ui.status_message = "无法获取文件路径".to_string();
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        if Self::set_clipboard_text(&path_str) {
            self.ui.status_message = format!("已复制路径: {}", path_str);
        } else {
            self.ui.status_message = "复制路径失败".to_string();
        }
    }
    /// 复制文件节点相对工作区根目录的路径到剪贴板（无法取相对则回退绝对路径）。
    pub fn copy_node_relative_path(&mut self, node_idx: u32) {
        let Some(path) = self.get_node_path(node_idx) else {
            self.ui.status_message = "无法获取文件路径".to_string();
            return;
        };
        let rel = self
            .fs
            .current_folder
            .as_ref()
            .and_then(|root| path.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if Self::set_clipboard_text(&rel) {
            self.ui.status_message = format!("已复制相对路径: {}", rel);
        } else {
            self.ui.status_message = "复制路径失败".to_string();
        }
    }
    /// 在文件资源管理器中打开/定位指定节点。
    /// - 文件：用 `explorer /select` 打开其所在文件夹并选中该文件
    ///   （不能用默认程序打开文件本身，否则等同于双击运行）；
    /// - 文件夹：直接用 explorer 打开该文件夹内容。
    pub fn open_node_in_explorer(&mut self, node_idx: u32) {
        let Some(path) = self.get_node_path(node_idx) else {
            self.ui.status_message = "无法获取文件路径".to_string();
            return;
        };
        let path_str = path.to_string_lossy().to_string();
        let mut cmd = std::process::Command::new("explorer.exe");
        if path.is_dir() {
            // 文件夹：打开其内容
            cmd.arg(&path_str);
        } else {
            // 文件：在所在目录中定位并选中该文件
            cmd.arg("/select,").arg(&path_str);
        }
        match cmd.spawn() {
            Ok(_) => {
                self.ui.status_message = format!("已在文件资源管理器中打开: {}", path_str);
            }
            Err(e) => {
                self.ui.status_message = format!("打开文件资源管理器失败: {}", e);
            }
        }
    }
    /// 删除文件节点（文件或文件夹），删除前弹窗确认防误操作。
    pub fn delete_file_node(&mut self, node_idx: u32) {
        let Some(path) = self.get_node_path(node_idx) else {
            self.ui.status_message = "无法获取文件路径".to_string();
            return;
        };
        let Some(tree) = self.fs.file_tree.as_ref() else {
            return;
        };
        let Some(node) = tree.get_node(node_idx) else {
            return;
        };
        let name = tree.get_name(node).to_string();

        // 直接移至回收站，无确认对话框（VS Code 行为）。
        // 用户可通过 Ctrl+Z 撤销或从系统回收站还原。

        // 文件暂存：删除前先收集受影响标签页（含目录前缀匹配）并快照内容，
        // 供 Ctrl+Z 从内存直接回写恢复。
        // 注意：活动标签页内容存放在 self.editor.content，tabs[active] 条目已被 swap 空，
        // 需分别检查两处。
        let mut affected_paths: Vec<PathBuf> = Vec::new();
        if self
            .editor
            .content
            .file_path
            .as_ref()
            .is_some_and(|fp| fp.starts_with(&path))
        {
            affected_paths.push(self.editor.content.file_path.clone().unwrap());
        }
        for tab in self.editor.tab_bar.tabs.iter() {
            if let Some(fp) = tab.file_path() {
                if fp.starts_with(&path) {
                    affected_paths.push(fp.clone());
                }
            }
        }
        let mut records: Vec<(PathBuf, Option<String>)> = Vec::new();
        for p in &affected_paths {
            let content = if self.editor.content.file_path.as_ref() == Some(p) {
                self.editor.content.buffer.get_all_text()
            } else {
                self.editor
                    .tab_bar
                    .tabs
                    .iter()
                    .find_map(|t| {
                        if t.file_path() == Some(p) {
                            t.as_file().map(|c| c.buffer.get_all_text())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            };
            records.push((p.clone(), Some(content)));
        }
        // 被删节点本身无对应标签页（目录或文件未打开）：文件则从磁盘快照，目录无快照
        if !affected_paths.iter().any(|p| p == &path) {
            let content = if path.is_file() {
                const MAX_SNAPSHOT_BYTES: u64 = 2 * 1024 * 1024;
                match std::fs::metadata(&path) {
                    Ok(meta) if meta.len() <= MAX_SNAPSHOT_BYTES => {
                        std::fs::read_to_string(&path).ok()
                    }
                    _ => None,
                }
            } else {
                None
            };
            records.push((path.clone(), content));
        }

        // 使用 Windows 回收站 API（可由系统回收站恢复）
        match crate::recycle_bin::move_to_recycle_bin(&path) {
            Ok(()) => {
                self.ui.status_message = format!("已删除: {} (Ctrl+Z 可撤销)", name);
                // 记录删除操作以支持 Ctrl+Z 撤销（含内容快照）
                for (p, content) in records {
                    self.fs
                        .delete_undo_stack
                        .push(crate::undo_delete::DeleteRecord {
                            original_path: p,
                            timestamp: std::time::Instant::now(),
                            content,
                        });
                }
                // 淡汰超过 20 条的旧记录
                if self.fs.delete_undo_stack.len() > 20 {
                    let extra = self.fs.delete_undo_stack.len() - 20;
                    self.fs.delete_undo_stack.drain(0..extra);
                }
                // 文件暂存：不关闭标签页，仅标记 deleted_from_disk，
                // 内容继续缓存在内存中，标签标题画横线提示已删除
                if self
                    .editor
                    .content
                    .file_path
                    .as_ref()
                    .is_some_and(|fp| fp.starts_with(&path))
                {
                    self.editor.content.deleted_from_disk = true;
                }
                for tab in self.editor.tab_bar.tabs.iter_mut() {
                    if tab.file_path().is_some_and(|fp| fp.starts_with(&path)) {
                        tab.set_deleted(true);
                    }
                }
                self.fs.selected_file_node = None;
                // 轻量刷新：保留展开状态，不重启 LSP
                self.refresh_file_tree_light();
            }
            Err(e) => {
                self.ui.status_message = format!("删除失败: {}", e);
            }
        }
    }
    /// Ctrl+Z 撤销最近一次文件删除（回退机制）。
    ///
    /// 恢复优先级：
    /// 1. 内存缓存：存在标记 `deleted_from_disk` 的对应标签页 → 直接从缓冲区
    ///    回写磁盘（保留删除后的未保存编辑）并清除删除标记；
    /// 2. 删除快照：标签页已关闭时用 `DeleteRecord.content` 回写；
    /// 3. 回收站兜底：无内容可用（如目录删除）时提示用户从回收站还原。
    pub fn undo_last_file_delete(&mut self) {
        let Some(record) = crate::undo_delete::pop_last_delete(&mut self.fs.delete_undo_stack)
        else {
            self.ui.status_message = "没有可撤销的删除操作".to_string();
            return;
        };
        let path = record.original_path;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        // 文件已重新存在（外部已还原）：同步状态即可
        if path.exists() {
            self.clear_deleted_mark(&path);
            self.refresh_file_tree_light();
            self.ui.status_message = format!("已恢复: {}", name);
            return;
        }

        // 优先级 1：从内存缓存的标签页缓冲区回写（活动标签页内容在 self.editor.content）
        let mut restore_content: Option<String> = None;
        if self.editor.content.deleted_from_disk
            && self.editor.content.file_path.as_deref() == Some(path.as_path())
        {
            restore_content = Some(self.editor.content.buffer.get_all_text());
        } else {
            for tab in self.editor.tab_bar.tabs.iter() {
                if tab.is_deleted() && tab.file_path() == Some(&path) {
                    if let Some(c) = tab.as_file() {
                        restore_content = Some(c.buffer.get_all_text());
                    }
                    break;
                }
            }
        }
        // 优先级 2：删除时的内容快照
        let content = match restore_content.or(record.content) {
            Some(c) => c,
            None => {
                self.ui.status_message =
                    format!("已撤销删除: {} (内容无缓存，请从回收站还原)", name);
                return;
            }
        };

        // 回写磁盘：父目录可能被一并删除，需先重建
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, &content) {
            Ok(()) => {
                self.clear_deleted_mark(&path);
                self.refresh_file_tree_light();
                self.ui.status_message = format!("已恢复: {}", name);
            }
            Err(e) => {
                self.ui.status_message = format!("恢复失败: {} ({})", name, e);
            }
        }
    }
    /// 清除与路径匹配的所有标签页的删除标记（撤销恢复或外部还原后调用）。
    fn clear_deleted_mark(&mut self, path: &std::path::Path) {
        if self.editor.content.file_path.as_deref() == Some(path) {
            self.editor.content.deleted_from_disk = false;
        }
        for tab in self.editor.tab_bar.tabs.iter_mut() {
            if tab.file_path().is_some_and(|fp| fp.as_path() == path) {
                tab.set_deleted(false);
            }
        }
    }
    /// 执行文件节点上下文菜单项对应的动作。
    /// 返回 true 表示动作已处理（调用方负责重绘）。
    pub fn execute_file_node_context_action(
        &mut self,
        item: crate::context_menu::FileNodeContextMenuItem,
        node_idx: u32,
    ) -> bool {
        use crate::context_menu::FileNodeContextMenuItem as Item;
        match item {
            Item::NewFileInside => {
                self.fs.selected_file_node = Some(node_idx);
                self.start_file_tree_input_in(FileTreeInputKind::NewFile, Some(node_idx));
                true
            }
            Item::NewFolderInside => {
                self.fs.selected_file_node = Some(node_idx);
                self.start_file_tree_input_in(FileTreeInputKind::NewFolder, Some(node_idx));
                true
            }
            Item::Rename => {
                self.fs.selected_file_node = Some(node_idx);
                self.start_file_tree_input(FileTreeInputKind::Rename);
                true
            }
            Item::Delete => {
                self.delete_file_node(node_idx);
                true
            }
            Item::RevealInExplorer => {
                self.open_node_in_explorer(node_idx);
                true
            }
            Item::CopyPath => {
                self.copy_node_path(node_idx);
                true
            }
            Item::CopyRelativePath => {
                self.copy_node_relative_path(node_idx);
                true
            }
            _ => false,
        }
    }
    /// 执行资源管理器空白区域上下文菜单项对应的动作。
    /// 返回 true 表示动作已处理（调用方负责重绘）。
    pub fn execute_explorer_context_action(
        &mut self,
        item: crate::context_menu::ExplorerContextMenuItem,
    ) -> bool {
        use crate::context_menu::ExplorerContextMenuItem as Item;
        match item {
            Item::NewFile => {
                self.start_file_tree_input(FileTreeInputKind::NewFile);
                true
            }
            Item::NewFolder => {
                self.start_file_tree_input(FileTreeInputKind::NewFolder);
                true
            }
            Item::Refresh => {
                self.refresh_file_tree();
                true
            }
            Item::RevealInExplorer => {
                self.open_in_file_explorer();
                true
            }
            Item::CopyPath => {
                self.copy_folder_path();
                true
            }
            _ => false,
        }
    }
    pub fn handle_sidebar_click(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        match &self.ui.sidebar_content {
            crate::layout::SidebarContent::FileTree => {
                self.handle_file_tree_click(mouse_x, mouse_y)
            }
            crate::layout::SidebarContent::SourceControlPanel => {
                self.handle_git_panel_click(mouse_x, mouse_y)
            }
            crate::layout::SidebarContent::RemoteFileTree => {
                self.handle_remote_tree_click(mouse_x, mouse_y)
            }
            _ => false,
        }
    }
    pub(super) fn handle_file_tree_click(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        // 优先检测标题栏按钮点击。按钮区域存的是窗口绝对坐标，
        // 而本函数收到的是侧边栏相对坐标，需先换算（此前直接比较
        // 导致左键点击新建按钮永远未命中，按钮形同虚设）。
        let sidebar = self.ui.layout.sidebar_region();
        let abs_x = mouse_x + sidebar.x;
        let abs_y = mouse_y + sidebar.y;
        if let Some(rect) = self.fs.file_tree_new_file_btn.clone() {
            if rect.contains(abs_x, abs_y) {
                // 正在输入时先失焦提交，再开始新的输入
                if self.fs.file_tree_input.is_some() {
                    self.confirm_file_tree_input();
                }
                self.start_file_tree_input(FileTreeInputKind::NewFile);
                return true;
            }
        }
        if let Some(rect) = self.fs.file_tree_new_folder_btn.clone() {
            if rect.contains(abs_x, abs_y) {
                if self.fs.file_tree_input.is_some() {
                    self.confirm_file_tree_input();
                }
                self.start_file_tree_input(FileTreeInputKind::NewFolder);
                return true;
            }
        }

        // 正在内联输入：点击输入行自身保持输入态，
        // 点击其他区域视为失焦提交（VS Code 行为；空名等效取消）
        if self.fs.file_tree_input.is_some() {
            if let Some((top, _, text_left)) = self.file_tree_input_row_geom() {
                let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * self.win.dpi_scale;
                if mouse_y >= top
                    && mouse_y < top + row_h
                    && mouse_x >= text_left - 3.0 * self.win.dpi_scale
                {
                    return true;
                }
            }
            self.confirm_file_tree_input();
            return true;
        }

        if self.fs.file_tree.is_none() {
            return false;
        }

        // 根目录行（工作区文件夹名）：点击切换整棵树的展开/折叠
        let root_top = self.file_tree_list_start_y();
        let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * self.win.dpi_scale;
        if mouse_y >= root_top && mouse_y < root_top + row_h {
            self.fs.file_tree_root_expanded = !self.fs.file_tree_root_expanded;
            self.emit_event(crate::events::EditorEvent::SidebarChanged);
            return true;
        }
        if !self.fs.file_tree_root_expanded {
            return false;
        }

        if self.fs.file_tree.is_none() {
            return false;
        }

        let start_y = self.file_tree_nodes_start_y();
        let sidebar_width = self.ui.layout.sidebar_width;
        let result = self.file_tree_hit_test(mouse_x, mouse_y, start_y, sidebar_width);

        if let Some((node_idx, kind, part)) = result {
            match kind {
                FileKind::Directory => {
                    // 读取当前展开状态以决定是否需要懒加载
                    let will_expand = self
                        .fs
                        .file_tree
                        .as_ref()
                        .and_then(|t| t.get_node(node_idx))
                        .map(|n| !n.is_expanded)
                        .unwrap_or(false);
                    // 展开前确保子节点已加载（使用异步加载避免阻塞 UI）
                    if will_expand {
                        let _ = self.ensure_node_loaded_async(node_idx);
                    }
                    if let Some(tree) = self.fs.file_tree.as_mut() {
                        if let Some(node) = tree.get_node_mut(node_idx) {
                            node.is_expanded = !node.is_expanded;
                        }
                    }
                    // P5-1: 展开状态变化，可见行数组失效
                    self.mark_file_tree_rows_dirty();
                    // 点击名称/图标区域时同时选中该目录
                    if part == FileTreeClickPart::Label {
                        self.fs.selected_file_node = Some(node_idx);
                    }
                    self.emit_event(crate::events::EditorEvent::SidebarChanged);
                    return true;
                }
                FileKind::File => {
                    // 仅点击文件名称/图标区域才打开文件
                    if part == FileTreeClickPart::Label {
                        self.fs.selected_file_node = Some(node_idx);
                        self.emit_event(crate::events::EditorEvent::SidebarChanged);
                        if let Some(path) = self.get_node_path(node_idx) {
                            // 检查该文件是否已在某个标签页中打开
                            // REQ-P1-09: 活动标签页的 file_path 在 self.editor.content 中
                            let active_path = self.editor.content.file_path.clone();
                            let active_idx = self.editor.tab_bar.active_tab;
                            if let Some(existing_tab) = self
                                .editor
                                .tab_bar
                                .tabs
                                .iter()
                                .enumerate()
                                .position(|(i, tab)| {
                                    if i == active_idx {
                                        active_path.as_ref() == Some(&path)
                                    } else {
                                        tab.file_path() == Some(&path)
                                    }
                                })
                            {
                                // 切换到已打开的标签页
                                self.switch_tab(existing_tab);
                            } else {
                                self.load_file(path);
                            }
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    /// 更新文件树悬停状态，返回是否需要重绘
    pub fn update_file_tree_hover(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        match &self.ui.sidebar_content {
            crate::layout::SidebarContent::FileTree => {
                self.update_local_tree_hover(mouse_x, mouse_y)
            }
            crate::layout::SidebarContent::RemoteFileTree => self.update_remote_tree_hover(mouse_y),
            _ => {
                let old = self.fs.hover_file_node.take();
                let old_root = std::mem::take(&mut self.fs.hover_file_tree_root);
                old.is_some() || old_root
            }
        }
    }
    pub(super) fn update_local_tree_hover(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        if self.fs.file_tree.is_none() {
            let old = self.fs.hover_file_node.take();
            let old_root = std::mem::take(&mut self.fs.hover_file_tree_root);
            return old.is_some() || old_root;
        }

        // 内联输入激活时禁用行悬停：输入行占位使行序偏移一行，
        // 且悬停高亮会干扰输入焦点（VS Code 同样不高亮）
        if self.fs.file_tree_input.is_some() {
            let old = self.fs.hover_file_node.take();
            let old_root = std::mem::take(&mut self.fs.hover_file_tree_root);
            return old.is_some() || old_root;
        }

        // 根目录行悬停检测（与节点悬停互斥）
        let root_top = self.file_tree_list_start_y();
        let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * self.win.dpi_scale;
        let new_root_hover = mouse_y >= root_top && mouse_y < root_top + row_h;

        let new_hover = if new_root_hover || !self.fs.file_tree_root_expanded {
            None
        } else {
            let start_y = self.file_tree_nodes_start_y();
            let sidebar_width = self.ui.layout.sidebar_width;
            self.file_tree_hit_test(mouse_x, mouse_y, start_y, sidebar_width)
                .map(|(idx, _, _)| idx)
        };

        let changed =
            self.fs.hover_file_node != new_hover || self.fs.hover_file_tree_root != new_root_hover;
        self.fs.hover_file_node = new_hover;
        self.fs.hover_file_tree_root = new_root_hover;
        changed
    }
    /// 根据当前打开的文件路径同步文件树选中状态
    pub fn sync_file_tree_selection(&mut self) {
        if let Some(ref path) = self.editor.content.file_path {
            if let Some(ref folder) = self.fs.current_folder {
                if let Some(ref tree) = self.fs.file_tree {
                    // 尝试找到匹配当前文件路径的节点
                    if let Some(matched) = Self::find_node_by_path(tree, path, folder) {
                        self.fs.selected_file_node = Some(matched);
                    }
                }
            }
        }
    }
    pub(super) fn find_node_by_path(tree: &FileTree, target: &Path, base: &Path) -> Option<u32> {
        // 获取相对于 base 的路径
        let rel_path = target.strip_prefix(base).ok()?;
        let components: Vec<_> = rel_path.components().collect();
        if components.is_empty() {
            return None;
        }

        let mut current_idx = tree.first_root_node()?;
        for (i, comp) in components.iter().enumerate() {
            let comp_name = comp.as_os_str().to_string_lossy();
            let mut found = None;
            let mut child_idx = tree
                .get_node(current_idx)
                .map(|n| n.first_child)
                .filter(|&c| c != u32::MAX);

            while let Some(idx) = child_idx {
                if let Some(node) = tree.get_node(idx) {
                    let name = tree.get_name(node);
                    if name == comp_name.as_ref() {
                        found = Some(idx);
                        break;
                    }
                    child_idx = if node.next_sibling != u32::MAX {
                        Some(node.next_sibling)
                    } else {
                        None
                    };
                } else {
                    break;
                }
            }

            if let Some(idx) = found {
                if i == components.len() - 1 {
                    return Some(idx);
                }
                current_idx = idx;
            } else {
                return None;
            }
        }
        None
    }
    pub(super) fn get_node_path(&self, node_idx: u32) -> Option<PathBuf> {
        let folder = self.fs.current_folder.as_ref()?;
        let tree = self.fs.file_tree.as_ref()?;
        let mut path_parts = Vec::new();

        let mut current_idx = Some(node_idx);
        while let Some(idx) = current_idx {
            let node = tree.get_node(idx)?;
            let name = tree.get_name(node).to_string();
            path_parts.push(name);

            if node.parent_idx == u32::MAX {
                break;
            }
            current_idx = Some(node.parent_idx);
        }

        path_parts.reverse();
        let mut path = folder.clone();
        for part in path_parts {
            path = path.join(part);
        }

        Some(path)
    }
    /// 懒加载：确保目录节点的子项已扫描
    /// 若节点未加载（is_loaded=false），扫描其磁盘子目录一层并标记为已加载
    /// 返回 true 表示本次实际执行了加载
    pub(super) fn ensure_node_loaded(&mut self, node_idx: u32) -> bool {
        // 先读取需要的信息，避免跨方法借用
        let (already_loaded, dir_path, depth) = {
            let tree = match self.fs.file_tree.as_ref() {
                Some(t) => t,
                None => return false,
            };
            let node = match tree.get_node(node_idx) {
                Some(n) => n,
                None => return false,
            };
            if node.kind != FileKind::Directory || node.is_loaded {
                return false;
            }
            let path = match self.get_node_path(node_idx) {
                Some(p) => p,
                None => return false,
            };
            (node.is_loaded, path, node.depth)
        };

        let _ = already_loaded; // 已通过上面的判断保证为 false
        let child_depth = depth.saturating_add(1);
        if let Some(tree) = self.fs.file_tree.as_mut() {
            let _ = populate_children_one_level(tree, &dir_path, node_idx, child_depth);
            if let Some(node) = tree.get_node_mut(node_idx) {
                node.is_loaded = true;
            }
            return true;
        }
        false
    }

    /// 异步懒加载：启动后台线程扫描目录子项
    /// 立即返回，不阻塞 UI 线程；扫描完成后通过 WM_APP+12 消息通知
    /// 返回 true 表示成功启动了异步加载
    pub(super) fn ensure_node_loaded_async(&mut self, node_idx: u32) -> bool {
        // 检查是否已在加载中
        if self.fs.file_tree_loading_nodes.contains(&node_idx) {
            return false;
        }

        // 先读取需要的信息，避免跨方法借用
        let (dir_path, depth) = {
            let tree = match self.fs.file_tree.as_ref() {
                Some(t) => t,
                None => return false,
            };
            let node = match tree.get_node(node_idx) {
                Some(n) => n,
                None => return false,
            };
            if node.kind != FileKind::Directory || node.is_loaded || node.is_loading {
                return false;
            }
            let path = match self.get_node_path(node_idx) {
                Some(p) => p,
                None => return false,
            };
            (path, node.depth)
        };

        // 标记节点为加载中状态
        if let Some(tree) = self.fs.file_tree.as_mut() {
            if let Some(node) = tree.get_node_mut(node_idx) {
                node.is_loading = true;
            }
        }
        self.fs.file_tree_loading_nodes.insert(node_idx);

        // 启动后台线程扫描
        let hwnd = self.win.hwnd;
        let send_hwnd = SendHwnd(hwnd.0 as usize);
        let child_depth = depth.saturating_add(1);
        let dir_path_clone = dir_path.clone();

        std::thread::spawn(move || {
            let entries = scan_file_tree_entries(&dir_path_clone);
            let result = SubdirScanResult {
                node_idx,
                entries,
                dir_path: dir_path_clone,
                child_depth,
            };
            let ptr = Box::into_raw(Box::new(result));
            let hwnd = windows::Win32::Foundation::HWND(send_hwnd.0 as *mut std::ffi::c_void);
            unsafe {
                post_boxed_message_lparam(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::WM_APP + 12,
                    ptr,
                );
            }
        });

        true
    }

    /// 处理子目录异步扫描结果（由 WM_APP+12 消息触发）
    pub(crate) fn on_subdir_scan_result(&mut self, result: &SubdirScanResult) {
        let node_idx = result.node_idx;

        // 移除加载中标记
        self.fs.file_tree_loading_nodes.remove(&node_idx);

        // 验证节点仍然有效（可能在扫描期间被刷新）
        let should_update = {
            if let Some(tree) = self.fs.file_tree.as_ref() {
                if let Some(node) = tree.get_node(node_idx) {
                    // 验证节点仍是目录且路径匹配
                    node.kind == FileKind::Directory
                        && self.get_node_path(node_idx).as_ref() == Some(&result.dir_path)
                } else {
                    false
                }
            } else {
                false
            }
        };

        if !should_update {
            return;
        }

        // 更新文件树：添加扫描到的子项
        if let Some(tree) = self.fs.file_tree.as_mut() {
            // 先清除旧的子节点（如果有）
            // 注意：这里简化处理，直接添加新节点
            // 如果需要支持刷新，需要先移除旧子节点
            for entry in &result.entries {
                tree.add_node(&entry.name, entry.kind, node_idx, result.child_depth);
            }

            // 标记节点为已加载、非加载中
            if let Some(node) = tree.get_node_mut(node_idx) {
                node.is_loaded = true;
                node.is_loading = false;
            }
        }

        // 标记可见行数组需要重建
        self.mark_file_tree_rows_dirty();
        // 新加载的子节点中可能包含已展开但未加载的嵌套目录，立即触发预加载
        self.preload_expanded_dirs();
        // 触发重绘
        self.emit_event(crate::events::EditorEvent::SidebarChanged);
    }
    /// 渲染前预扫描：加载所有 is_expanded 但未加载的目录节点
    /// 使用异步加载避免阻塞 UI 线程
    pub(crate) fn preload_expanded_dirs(&mut self) {
        let mut to_load: Vec<u32> = Vec::new();

        // 收集需要加载的节点（已展开但未加载且未在加载中的目录）
        if let Some(tree) = self.fs.file_tree.as_ref() {
            for (i, node) in tree.nodes_iter().enumerate() {
                if node.kind == FileKind::Directory
                    && node.is_expanded
                    && !node.is_loaded
                    && !node.is_loading
                {
                    to_load.push(i as u32);
                }
            }
        }

        // 启动异步加载（不阻塞 UI 线程）
        for idx in to_load {
            let _ = self.ensure_node_loaded_async(idx);
        }
    }
    /// P5-1: 标记可见行数组需要重建（展开/折叠等不改变节点数的变更后调用）
    pub(crate) fn mark_file_tree_rows_dirty(&mut self) {
        self.fs.file_tree_rows_dirty = true;
    }

    /// P5-1: 确保可见行数组与当前树状态一致（脏标志或节点数变化时重建）
    pub(crate) fn ensure_file_tree_rows(&mut self) {
        let tree_len = self.fs.file_tree.as_ref().map(|t| t.len()).unwrap_or(0);
        if !self.fs.file_tree_rows_dirty && self.fs.file_tree_rows_tree_len == tree_len {
            return;
        }
        self.fs.file_tree_visible_rows.clear();
        if let Some(tree) = self.fs.file_tree.as_ref() {
            Self::collect_visible_rows(tree, u32::MAX, &mut self.fs.file_tree_visible_rows);
        }
        self.fs.file_tree_rows_tree_len = tree_len;
        self.fs.file_tree_rows_dirty = false;
    }

    /// 按显示顺序（DFS，仅展开目录递归）收集可见节点索引
    fn collect_visible_rows(tree: &FileTree, parent_idx: u32, out: &mut Vec<u32>) {
        let mut child_idx = if parent_idx == u32::MAX {
            tree.first_root_node()
        } else {
            tree.get_node(parent_idx)
                .map(|n| n.first_child)
                .filter(|&c| c != u32::MAX)
        };
        while let Some(idx) = child_idx {
            let Some(node) = tree.get_node(idx) else {
                break;
            };
            out.push(idx);
            if node.kind == FileKind::Directory && node.is_expanded {
                Self::collect_visible_rows(tree, idx, out);
            }
            child_idx = if node.next_sibling != u32::MAX {
                Some(node.next_sibling)
            } else {
                None
            };
        }
    }

    /// P5-1: O(1) 文件树命中测试 — 基于可见行数组按行高直接索引，
    /// 替代逐节点递归遍历（每次鼠标移动都执行的热路径）。
    /// 横向命中规则与 render_tree_nodes 保持一致（根目录行占第 0 层，
    /// 节点整体缩进一级；chevron 列宽复用 FILE_TREE_ARROW_COL）。
    pub(crate) fn file_tree_hit_test(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        start_y: f32,
        sidebar_width: f32,
    ) -> Option<(u32, FileKind, FileTreeClickPart)> {
        self.ensure_file_tree_rows();
        let s = self.win.dpi_scale;
        let node_height = crate::layout::FILE_TREE_ROW_HEIGHT * s;
        if mouse_y < start_y || node_height <= 0.0 {
            return None;
        }
        let row = ((mouse_y - start_y) / node_height) as usize;
        let idx = *self.fs.file_tree_visible_rows.get(row)?;
        let tree = self.fs.file_tree.as_ref()?;
        let node = tree.get_node(idx)?;

        let base_x = 10.0 * s;
        let indent = (node.depth as f32 + 1.0) * crate::layout::FILE_TREE_INDENT * s;
        let item_left = base_x + indent;
        let item_right = sidebar_width - 10.0 * s;

        // x 超出节点有效区域视为未命中（避免点击滚动条或空白处误触发）
        if mouse_x < item_left - 4.0 * s || mouse_x > item_right {
            return None;
        }

        // 判断点击的是目录展开箭头还是名称/图标区域
        let part = if node.kind == FileKind::Directory {
            let arrow_right = item_left + crate::layout::FILE_TREE_ARROW_COL * s;
            if mouse_x < arrow_right {
                FileTreeClickPart::Arrow
            } else {
                FileTreeClickPart::Label
            }
        } else {
            FileTreeClickPart::Label
        };

        Some((idx, node.kind, part))
    }
    pub(super) fn format_file_tree(&self, tree: &FileTree) -> String {
        let mut lines = Vec::new();
        let max_files = 200;
        for (idx, node) in tree.nodes_iter().enumerate() {
            if idx >= max_files {
                lines.push("...".to_string());
                break;
            }
            if node.kind != FileKind::File {
                continue;
            }
            if let Some(path) = file_tree_node_path(tree, idx as u32) {
                lines.push(path);
            }
        }
        if lines.is_empty() {
            "(空)".to_string()
        } else {
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EditorState;
    use std::hash::Hasher;

    fn dir_sig(dir: &std::path::Path) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        EditorState::hash_dir_level(&mut hasher, dir);
        hasher.finish()
    }

    #[test]
    fn test_hash_dir_level_detects_new_file() {
        let dir = std::env::temp_dir().join(format!(
            "aether_sig_test_{}",
            aether_ai_panel::memory_store::new_id("d")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sig1 = dir_sig(&dir);
        // 新文件产生 → 签名必须变化
        std::fs::write(dir.join("new_file.txt"), "hello").unwrap();
        let sig2 = dir_sig(&dir);
        assert_ne!(sig1, sig2);
        // 无变化 → 签名稳定
        assert_eq!(sig2, dir_sig(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }
}
