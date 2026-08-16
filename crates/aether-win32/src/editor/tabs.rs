use super::*;

impl EditorState {
    /// REQ-P1-09: 交换活动标签页内容（替代原 sync_to_tab/sync_from_tab 的字段逐个同步）
    ///
    /// 将 `self.editor.content` 与 `self.editor.tab_bar.tabs[index].content` 原子交换，
    /// 消除手动字段同步，保证状态归属单一。
    pub(super) fn swap_tab_content(&mut self, index: usize) {
        if let Some(crate::tabs::Tab::File(content)) = self.editor.tab_bar.tabs.get_mut(index) {
            std::mem::swap(&mut self.editor.content, content);
        }
        // 同步 Markdown 预览模式到 EditorState 级别（渲染分支读取）
        self.editor.markdown_preview = self.editor.content.markdown_preview;
    }
    /// 获取当前活动标签页（只读）
    pub fn current_tab(&self) -> &Tab {
        &self.editor.tab_bar.tabs[self.editor.tab_bar.active_tab]
    }
    /// 获取当前标签页数量
    pub fn tab_count(&self) -> usize {
        self.editor.tab_bar.tabs.len()
    }
    /// 是否显示标签栏：只要有标签页就显示（仅欢迎页例外）。
    /// 修复：原先单个未关联文件的 File tab 会导致标签栏消失，用户误以为"全关了"，
    /// 但 self.editor.content 仍持有内容可编辑。现在保证空 File tab 也显示标签栏。
    pub fn show_tab_bar(&self) -> bool {
        !self.editor.tab_bar.tabs.is_empty() && !self.active_tab_is_welcome()
    }
    /// 是否显示空占位页（tabs 为空时的默认状态）
    pub fn show_empty_placeholder(&self) -> bool {
        self.editor.tab_bar.tabs.is_empty()
    }
    /// 当前活动标签页是否是文件 tab
    pub fn active_tab_is_file(&self) -> bool {
        self.editor.tab_bar
            .tabs
            .get(self.editor.tab_bar.active_tab)
            .map(|t| t.is_file())
            .unwrap_or(false)
    }
    /// 当前活动标签页是否是设置 tab
    pub fn active_tab_is_settings(&self) -> bool {
        self.editor.tab_bar
            .tabs
            .get(self.editor.tab_bar.active_tab)
            .map(|t| t.is_settings())
            .unwrap_or(false)
    }
    /// 当前活动标签页是否是欢迎 tab
    pub fn active_tab_is_welcome(&self) -> bool {
        self.editor.tab_bar
            .tabs
            .get(self.editor.tab_bar.active_tab)
            .map(|t| t.is_welcome())
            .unwrap_or(false)
    }
    /// 当前活动标签页是否是沙盒评测 tab
    pub fn active_tab_is_sandbox_eval(&self) -> bool {
        self.editor.tab_bar
            .tabs
            .get(self.editor.tab_bar.active_tab)
            .map(|t| t.is_sandbox_eval())
            .unwrap_or(false)
    }
    /// 当前活动文件标签页的文件路径
    pub fn active_file_path(&self) -> Option<&std::path::PathBuf> {
        self.editor.tab_bar
            .tabs
            .get(self.editor.tab_bar.active_tab)
            .and_then(|t| t.file_path())
    }
    /// 切换 Markdown 预览模式（仅当当前标签页是 Markdown 文件时有效）
    pub fn toggle_markdown_preview(&mut self) {
        if self.editor.content.language == Language::Markdown {
            self.editor.content.markdown_preview = !self.editor.content.markdown_preview;
            self.editor.markdown_preview = self.editor.content.markdown_preview;
            self.ui.status_message = if self.editor.markdown_preview {
                "Markdown 预览模式".to_string()
            } else {
                "Markdown 编辑模式".to_string()
            };
        }
    }
    /// 查找设置 tab 的索引
    pub fn find_settings_tab(&self) -> Option<usize> {
        self.editor.tab_bar.tabs.iter().position(|t| t.is_settings())
    }
    /// 查找欢迎 tab 的索引
    pub fn find_welcome_tab(&self) -> Option<usize> {
        self.editor.tab_bar.tabs.iter().position(|t| t.is_welcome())
    }
    /// 查找沙盒评测 tab 的索引
    pub fn find_sandbox_eval_tab(&self) -> Option<usize> {
        self.editor.tab_bar.tabs.iter().position(|t| t.is_sandbox_eval())
    }
    /// 打开沙盒评测标签页（作为通用 tab 插入到标签栏）
    pub fn open_sandbox_eval_tab(&mut self) {
        if let Some(idx) = self.find_sandbox_eval_tab() {
            self.switch_tab(idx);
        } else {
            // 保存当前文件 tab 的内容到 tabs[active_tab]（如果是文件 tab）
            self.swap_tab_content(self.editor.tab_bar.active_tab);
            self.editor.tab_bar.tabs.push(crate::tabs::Tab::SandboxEval);
            self.editor.tab_bar.active_tab = self.editor.tab_bar.tabs.len() - 1;
            // 沙盒评测 tab 无 content，self.editor.content 保持空即可
            self.editor.content = crate::tabs::TabContent::new();
            self.ai.ai_panel.input_focused = false;
            self.ui.status_message = "已打开智能体沙盒评测".to_string();
            self.emit_event(crate::events::EditorEvent::TabChanged);
        }
    }
    /// 切换到指定标签页
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.editor.tab_bar.tabs.len() && index != self.editor.tab_bar.active_tab {
            // REQ-P1-09: 仅文件 tab 需要 swap content；设置/欢迎等通用 tab 无 content
            self.swap_tab_content(self.editor.tab_bar.active_tab);
            self.editor.tab_bar.active_tab = index;
            self.swap_tab_content(self.editor.tab_bar.active_tab);
            self.editor.is_selecting = false;

            // 无感切换优化：预先计算新标签页的可见范围并更新缓存签名，
            // 避免切换后第一帧因签名不匹配而强制重建缓存。
            // 这样切换回之前打开的标签页时，如果滚动位置没变，缓存立即命中。
            if self.active_tab_is_file() {
                let line_height = self.win.text_renderer.line_height();
                let editor_region = self.ui.layout.editor_content_region(self.show_tab_bar());
                let visible_start = (self.editor.content.scroll_y / line_height) as usize;
                let visible_lines = (editor_region.height / line_height) as usize + 2;
                let total_lines = self.editor.content.buffer.len_lines().max(1);
                let visible_end = (visible_start + visible_lines).min(total_lines);
                let cache_start = visible_start.saturating_sub(2);
                let cache_end = (visible_end + 2).min(total_lines).max(cache_start);
                let window_len = cache_end - cache_start;
                // 预更新签名，使下一帧 rebuild_cache 能立即命中
                self.editor.content.last_cache_signature = (
                    self.editor.content.buffer_version,
                    visible_start,
                    visible_end,
                    total_lines,
                );
                // 确保 cached_tokens 长度与当前文件匹配
                if self.editor.content.cached_tokens.len() != total_lines {
                    self.editor.content
                        .cached_tokens
                        .resize_with(total_lines, Vec::new);
                }
                // 如果行文本缓存窗口不匹配，需要重建（但保留已有缓存数据）
                if self.editor.content.cache_window_start != cache_start
                    || self.editor.content.cached_lines.len() != window_len
                {
                    // 使用 slide_cache_window 保留重叠部分
                    self.editor.content.slide_cache_window(cache_start, window_len);
                }
            }

            // 异步延迟同步文件树选择，避免阻塞切换
            let need_sync = self.active_tab_is_file() && self.editor.content.file_path.is_some();
            if need_sync {
                // 立即执行（文件树遍历通常很快），但如果项目极大可改为异步
                self.sync_file_tree_selection();
            }

            // 0延迟切换：标记刚切换过来的标签页
            // 这样 rebuild_cache 首帧会跳过所有工作，直接渲染已有缓存
            self.editor.content.just_switched = true;

            // 图片预览位图绑定旧标签的图像，切换后需按新标签的 image_data 重建
            self.win.image_bitmap = None;
            // 重置图片缩放状态
            self.win.image_zoom = 1.0;
            self.win.image_offset_x = 0.0;
            self.win.image_offset_y = 0.0;

            let title = self.editor.tab_bar.tabs[self.editor.tab_bar.active_tab].title();
            self.ui.status_message = format!("切换到: {}", title);
            self.emit_event(crate::events::EditorEvent::TabChanged);
            // 显式标记局部脏区域，避免标签切换触发全窗口重绘导致卡顿
            let editor_region = self.ui.layout.editor_region();
            let tab_region = self.ui.layout.tab_bar_region(self.show_tab_bar());
            let status_region = self.ui.layout.status_bar_region();
            self.win.dirty_tracker.mark_region(
                editor_region.x,
                editor_region.y,
                editor_region.width,
                editor_region.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
            self.win.dirty_tracker.mark_region(
                tab_region.x,
                tab_region.y,
                tab_region.width,
                tab_region.height,
                crate::dirty_rect::DirtyRegionType::TabBar,
            );
            self.win.dirty_tracker.mark_region(
                status_region.x,
                status_region.y,
                status_region.width,
                status_region.height,
                crate::dirty_rect::DirtyRegionType::StatusBar,
            );
            let sidebar_region = self.ui.layout.sidebar_region();
            if sidebar_region.width > 0.0 {
                self.win.dirty_tracker.mark_region(
                    sidebar_region.x,
                    sidebar_region.y,
                    sidebar_region.width,
                    sidebar_region.height,
                    crate::dirty_rect::DirtyRegionType::Sidebar,
                );
            }
        }
    }
    /// 关闭当前标签页，返回是否还有标签页
    pub fn close_current_tab(&mut self) -> bool {
        if self.editor.tab_bar.tabs.len() <= 1 {
            // 最后一个标签页：保存内容并清空 tabs（渲染层根据 tabs.is_empty() 显示欢迎页/空占位页）
            // 文件 tab 才需要保存 last_closed_tab；设置/欢迎等不保存
            if self.active_tab_is_file() {
                self.editor.tab_bar.last_closed_tab =
                    Some(std::mem::replace(&mut self.editor.content, TabContent::new()));
            }
            self.editor.tab_bar.tabs.clear();
            self.editor.tab_bar.active_tab = 0;
            self.editor.is_selecting = false;
            self.ui.status_message = "已关闭".to_string();
            return true;
        }
        // 保存 self.editor.content 中的最新内容到 last_closed_tab（文件 tab 才需要）
        if self.active_tab_is_file() {
            self.editor.tab_bar.last_closed_tab =
                Some(std::mem::replace(&mut self.editor.content, TabContent::new()));
        }
        // 从 tabs 中移除活动标签页
        let _removed = self.editor.tab_bar.tabs.remove(self.editor.tab_bar.active_tab);
        if self.editor.tab_bar.active_tab >= self.editor.tab_bar.tabs.len() {
            self.editor.tab_bar.active_tab = self.editor.tab_bar.tabs.len() - 1;
        }
        // 将新活动标签页的内容交换到 self.editor.content
        // swap 后文件 tabs[active_tab].content 持有空 TabContent（安全，不会被误匹配路径）
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        self.editor.is_selecting = false;
        self.ui.status_message = format!("已关闭，剩余 {} 个标签页", self.editor.tab_bar.tabs.len());
        !self.editor.tab_bar.tabs.is_empty()
    }
    /// P2-8: 带保存确认的关闭标签页。
    /// 返回值：true 表示已关闭（用户确认或无需保存），false 表示用户取消。
    pub fn close_current_tab_checked(&mut self) -> bool {
        // self 上的 is_dirty / buffer / file_path 即为当前文件活动标签页的实时状态
        // （编辑操作直接作用于 self，仅在切换标签页时通过 swap 交换）
        if self.active_tab_is_file() && self.editor.content.is_dirty {
            let file_name = self
    .editor.content
                .file_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".to_string());
            let msg = format!("{} 有未保存的修改，是否保存并关闭？", file_name);
            let confirmed = Dialogs::confirm_yes_no(self.win.hwnd, "关闭标签页", &msg);
            if !confirmed {
                self.ui.status_message = "已取消关闭".to_string();
                return false;
            }
            let saved = self.save_file();
            if !saved {
                self.ui.status_message = "保存失败，已取消关闭".to_string();
                return false;
            }
        }
        self.close_current_tab();
        true
    }
    /// SubTask 7.1: 中键关闭指定索引的标签页（带 dirty 检查，与关闭按钮行为一致）。
    /// 返回 true 表示已关闭，false 表示用户取消或索引越界。
    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.editor.tab_bar.tabs.len() {
            return false;
        }
        if index == self.editor.tab_bar.active_tab {
            return self.close_current_tab_checked();
        }
        // 非活动标签页：检查 is_dirty（与 handle_tab_bar_click 中关闭按钮逻辑一致）
        let tab_dirty = self
    .editor.tab_bar
            .tabs
            .get(index)
            .map(|t| t.is_dirty())
            .unwrap_or(false);
        if tab_dirty {
            let tab_name = self
    .editor.tab_bar
                .tabs
                .get(index)
                .and_then(|t| t.file_path())
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".to_string());
            let msg = format!("{} 有未保存的修改，是否丢弃修改并关闭？", tab_name);
            if !Dialogs::confirm_yes_no(self.win.hwnd, "关闭标签页", &msg) {
                self.ui.status_message = "已取消关闭".to_string();
                return false;
            }
        }
        // Task 13.3: 保存关闭的标签内容以支持 Ctrl+Shift+T 恢复
        remove_tab_saving_content(
            &mut self.editor.tab_bar.tabs,
            index,
            &mut self.editor.tab_bar.last_closed_tab,
        );
        if index < self.editor.tab_bar.active_tab {
            self.editor.tab_bar.active_tab -= 1;
        }
        self.ui.status_message = format!("已关闭，剩余 {} 个标签页", self.editor.tab_bar.tabs.len());
        true
    }
    /// SubTask 9.4: 关闭除指定索引外的所有标签页。
    ///
    /// 保留 `keep_idx`，移除其他所有标签页。若保留的标签页不是当前活动标签页，
    /// 切换到保留的标签页。索引越界时返回 false。
    pub fn close_other_tabs(&mut self, keep_idx: usize) -> bool {
        if keep_idx >= self.editor.tab_bar.tabs.len() {
            return false;
        }
        if self.editor.tab_bar.tabs.len() == 1 {
            return true;
        }
        // 取出保留标签，剩余标签中最后一个是 last_closed 候选
        let kept = self.editor.tab_bar.tabs.remove(keep_idx);
        let last_closed = self.editor.tab_bar.tabs.pop();
        self.editor.tab_bar.tabs.clear();
        if let Some(crate::tabs::Tab::File(content)) = last_closed {
            self.editor.tab_bar.last_closed_tab = Some(content);
        }
        self.editor.tab_bar.tabs.push(kept);
        self.editor.tab_bar.active_tab = 0;
        // 让 self.editor.content 反映保留标签
        if self.editor.tab_bar.tabs[0].is_file() {
            if let crate::tabs::Tab::File(content) = &mut self.editor.tab_bar.tabs[0] {
                self.editor.content = std::mem::replace(content, TabContent::new());
            }
        } else {
            self.editor.content = TabContent::new();
        }
        self.editor.is_selecting = false;
        self.ui.status_message = "已关闭其他标签页".to_string();
        true
    }
    /// SubTask 9.4: 关闭指定索引右侧的所有标签页。
    ///
    /// 保留 `idx` 及其左侧的标签页，移除 `idx+1..` 的所有标签页。
    /// 索引越界时返回 false。
    pub fn close_tabs_to_the_right(&mut self, idx: usize) -> bool {
        if idx >= self.editor.tab_bar.tabs.len() {
            return false;
        }
        if self.editor.tab_bar.tabs.len() <= idx + 1 {
            return true;
        }
        let active_in_closed = self.editor.tab_bar.active_tab > idx;
        if active_in_closed {
            // 活动标签页在被关闭的右侧：保存 self.editor.content 中的最新内容（文件 tab 才需要）
            if self.active_tab_is_file() {
                self.editor.tab_bar.last_closed_tab =
                    Some(std::mem::replace(&mut self.editor.content, TabContent::new()));
            }
        } else {
            // 活动标签页不在右侧：保存最后一个被关闭标签的内容
            let last_closed = self.editor.tab_bar.tabs.pop();
            if let Some(crate::tabs::Tab::File(content)) = last_closed {
                self.editor.tab_bar.last_closed_tab = Some(content);
            }
        }
        self.editor.tab_bar.tabs.truncate(idx + 1);
        if active_in_closed {
            self.editor.tab_bar.active_tab = idx;
            // 将保留标签内容加载到 self.editor.content
            self.swap_tab_content(self.editor.tab_bar.active_tab);
            self.editor.is_selecting = false;
        }
        self.ui.status_message = format!(
            "已关闭右侧标签页，剩余 {} 个标签页",
            self.editor.tab_bar.tabs.len()
        );
        true
    }
    /// SubTask 9.4: 关闭所有标签页，并创建一个新的空标签页。
    pub fn close_all_tabs(&mut self) {
        // 保存 self.editor.content 中的最新内容（文件 tab 才需要）以支持 Ctrl+Shift+T 恢复
        if self.active_tab_is_file() {
            self.editor.tab_bar.last_closed_tab =
                Some(std::mem::replace(&mut self.editor.content, TabContent::new()));
        }
        self.editor.tab_bar.tabs.clear();
        self.editor.tab_bar.tabs.push(Tab::new());
        self.editor.tab_bar.active_tab = 0;
        // self.editor.content 已空，tabs[0] 也是空，swap 保持架构一致
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        self.editor.is_selecting = false;
        self.ui.status_message = "已关闭所有标签页".to_string();
    }
    /// Task 13.3: 恢复最后关闭的标签页（Ctrl+Shift+T）。
    /// 如果存在 `last_closed_tab`，则将其作为新标签页恢复并切换为活动标签页。
    /// 返回 true 表示已恢复，false 表示没有可恢复的标签。
    pub fn reopen_last_closed_tab(&mut self) -> bool {
        let Some(content) = self.editor.tab_bar.last_closed_tab.take() else {
            self.ui.status_message = "没有可恢复的标签".to_string();
            return false;
        };
        // 将当前 self.editor.content swap 回当前活动标签，再 push 新文件标签并切换
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        self.editor.tab_bar.tabs.push(crate::tabs::Tab::File(content));
        self.editor.tab_bar.active_tab = self.editor.tab_bar.tabs.len() - 1;
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        self.editor.is_selecting = false;
        self.ui.status_message = "已恢复最后关闭的标签".to_string();
        true
    }
    /// 新建标签页
    pub fn new_tab(&mut self) -> usize {
        // REQ-P1-09: save current state to old tab, push new (empty) tab, swap it in
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        let tab = Tab::new();
        self.editor.tab_bar.tabs.push(tab);
        self.editor.tab_bar.active_tab = self.editor.tab_bar.tabs.len() - 1;
        self.swap_tab_content(self.editor.tab_bar.active_tab);
        self.editor.is_selecting = false;
        self.editor.tab_bar.active_tab
    }
    /// 切换到下一个标签页
    pub fn next_tab(&mut self) {
        if self.editor.tab_bar.tabs.len() > 1 {
            let next = (self.editor.tab_bar.active_tab + 1) % self.editor.tab_bar.tabs.len();
            self.switch_tab(next);
        }
    }
    /// 切换到上一个标签页
    pub fn prev_tab(&mut self) {
        if self.editor.tab_bar.tabs.len() > 1 {
            let prev =
                (self.editor.tab_bar.active_tab + self.editor.tab_bar.tabs.len() - 1) % self.editor.tab_bar.tabs.len();
            self.switch_tab(prev);
        }
    }
    /// 跳转到指定标签页（1-based index）
    pub fn goto_tab(&mut self, index: usize) {
        if index > 0 && index <= self.editor.tab_bar.tabs.len() {
            self.switch_tab(index - 1);
        }
    }
    /// 处理标签栏点击，返回是否处理了点击
    /// mouse_y 是相对于标签栏的 y 坐标（已由调用方转换）
    pub fn handle_tab_bar_click(&mut self, mouse_x: f32, _mouse_y: f32, editor_x: f32) -> bool {
        // P3-2: 使用 layout 常量而非硬编码 30.0，确保布局常量变更时此处理同步
        let tab_bar_height = if self.show_tab_bar() {
            TAB_BAR_HEIGHT
        } else {
            0.0
        };
        if tab_bar_height == 0.0 {
            return false;
        }
        // mouse_y 已由调用方检查，这里只检查 x 坐标
        if mouse_x < editor_x {
            return false;
        }
        // SubTask 7.2: 检测 "+" 新建按钮点击
        if let Some((pl, pt, pr, pb)) = self.editor.tab_bar.plus_button_rect {
            if mouse_x >= pl && mouse_x < pr && _mouse_y >= pt && _mouse_y < pb {
                self.new_tab();
                self.ui.status_message = "已新建标签页".to_string();
                return true;
            }
        }
        let rel_x = mouse_x - editor_x + self.editor.tab_bar.tab_scroll_x;
        for layout in &self.editor.tab_bar.tab_layouts {
            if rel_x >= layout.x && rel_x < layout.x + layout.width {
                // 检测关闭按钮点击
                if rel_x >= layout.close_x && rel_x < layout.close_x + layout.close_width {
                    // 复用 close_tab：统一活动/非活动标签页的 dirty 检查与
                    // last_closed_tab 保存逻辑，确保 Ctrl+Shift+T 可恢复。
                    self.close_tab(layout.index);
                    return true;
                }
                // 切换标签页
                self.switch_tab(layout.index);
                return true;
            }
        }
        false
    }
    /// 更新鼠标悬停标签
    pub fn update_hover_tab(&mut self, mouse_x: f32, mouse_y: f32, editor_x: f32) {
        // P3-2: 使用 layout 常量而非硬编码 30.0
        let tab_bar_height = if self.show_tab_bar() {
            TAB_BAR_HEIGHT
        } else {
            0.0
        };
        // SubTask 7.2: 更新 "+" 按钮悬停状态（独立于下方 y 检查，确保按钮响应）
        let mut on_plus = false;
        if let Some((pl, pt, pr, pb)) = self.editor.tab_bar.plus_button_rect {
            on_plus = mouse_x >= pl && mouse_x < pr && mouse_y >= pt && mouse_y < pb;
        }
        self.editor.tab_bar.plus_button_hover = on_plus;
        if tab_bar_height == 0.0 || mouse_y < 0.0 || mouse_y > tab_bar_height || mouse_x < editor_x
        {
            self.editor.tab_bar.hover_tab = None;
            return;
        }
        let rel_x = mouse_x - editor_x + self.editor.tab_bar.tab_scroll_x;
        for layout in &self.editor.tab_bar.tab_layouts {
            if rel_x >= layout.x && rel_x < layout.x + layout.width {
                self.editor.tab_bar.hover_tab = Some(layout.index);
                return;
            }
        }
        self.editor.tab_bar.hover_tab = None;
    }
    /// Task 8.2: 命中检测——返回鼠标所在标签体的索引。
    ///
    /// 仅当点击落在标签体内（非关闭按钮、非 "+" 按钮）时返回 `Some(idx)`。
    /// 用于拖拽重排：点击标签体时延迟切换，等待拖拽判定。
    ///
    /// `mouse_y` 是窗口绝对坐标，`tab_y` 是标签栏顶部 y 偏移（来自 layout）。
    /// 修复：原先将绝对 mouse_y 与 TAB_BAR_HEIGHT 直接比较，标签栏位于
    /// y=32..62 时总是返回 None，导致标签拖拽/点击切换失效。
    pub(crate) fn tab_body_hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        editor_x: f32,
        tab_y: f32,
    ) -> Option<usize> {
        let tab_bar_height = if self.show_tab_bar() {
            TAB_BAR_HEIGHT
        } else {
            0.0
        };
        if tab_bar_height == 0.0
            || mouse_y < tab_y
            || mouse_y >= tab_y + tab_bar_height
            || mouse_x < editor_x
        {
            return None;
        }
        // "+" 按钮区域不算标签体
        if let Some((pl, pt, pr, pb)) = self.editor.tab_bar.plus_button_rect {
            if mouse_x >= pl && mouse_x < pr && mouse_y >= pt && mouse_y < pb {
                return None;
            }
        }
        let rel_x = mouse_x - editor_x + self.editor.tab_bar.tab_scroll_x;
        for layout in &self.editor.tab_bar.tab_layouts {
            if rel_x >= layout.x && rel_x < layout.x + layout.width {
                // 关闭按钮区域不算标签体
                if rel_x >= layout.close_x && rel_x < layout.close_x + layout.close_width {
                    return None;
                }
                return Some(layout.index);
            }
        }
        None
    }
    /// Task 8.3: 根据鼠标 x 位置计算拖拽放置目标索引（0..=tabs.len()）。
    ///
    /// 基于标签中点：鼠标在标签左半部分 → 插入到该标签前；
    /// 右半部分 → 插入到该标签后。超出范围则 clamp 到 0 或 tabs.len()。
    pub(crate) fn tab_drop_index_at(&self, mouse_x: f32, editor_x: f32) -> usize {
        let rel_x = mouse_x - editor_x + self.editor.tab_bar.tab_scroll_x;
        for layout in &self.editor.tab_bar.tab_layouts {
            let mid = layout.x + layout.width / 2.0;
            if rel_x < mid {
                return layout.index;
            }
        }
        self.editor.tab_bar.tabs.len()
    }
    /// Task 8.4: 执行标签重排——将 drag_idx 处的标签移动到 drop_idx 位置。
    ///
    /// `drop_idx` 语义：插入到该索引之前（与 `tab_drop_index_at` 一致）。
    /// 自动调整 `active_tab` 索引以跟随移动的标签。
    pub fn reorder_tabs(&mut self, drag_idx: usize, drop_idx: usize) {
        reorder_tabs_with_active(
            &mut self.editor.tab_bar.tabs,
            &mut self.editor.tab_bar.active_tab,
            drag_idx,
            drop_idx,
        );
    }
    /// SubTask 7.5: 计算标签栏最大水平滚动偏移。
    ///
    /// `max_scroll = total_tabs_width - tab_bar_visible_width`，下限为 0。
    /// 基于 `tab_layouts` 中最后一个标签的右边界计算（包含尾部 gap）。
    pub(crate) fn tab_bar_max_scroll(&self, tab_bar_width: f32) -> f32 {
        let gap = 2.0;
        let left_padding = 4.0;
        // "+" 按钮区域（8px gap + 28px 按钮）也需预留可见空间
        let plus_area = 8.0 + 28.0;
        let total_tabs_width = self
    .editor.tab_bar
            .tab_layouts
            .last()
            .map(|l| l.x + l.width + gap)
            .unwrap_or(0.0);
        let visible_width = (tab_bar_width - left_padding - plus_area).max(0.0);
        (total_tabs_width - visible_width).max(0.0)
    }
    /// 滚动标签栏水平偏移（平滑滚动版）。
    ///
    /// 滚轮事件只更新 `tab_scroll_target`，实际渲染值 `tab_scroll_x` 在动画帧中
    /// 通过指数衰减（lerp）向 target 逼近，实现平滑滚动效果。
    /// 返回是否 target 发生了变化（用于决定是否启动动画定时器）。
    pub(crate) fn scroll_tab_bar(&mut self, delta: f32, tab_bar_width: f32) -> bool {
        let old = self.editor.tab_bar.tab_scroll_target;
        let max_scroll = self.tab_bar_max_scroll(tab_bar_width);
        // 将滚轮 delta 转换为像素增量（每格 120 → 约 3 个标签宽度）
        let pixel_delta = delta / 120.0 * 200.0;
        self.editor.tab_bar.tab_scroll_target =
            (self.editor.tab_bar.tab_scroll_target + pixel_delta).clamp(0.0, max_scroll);
        (self.editor.tab_bar.tab_scroll_target - old).abs() > 0.01
    }

    /// 动画帧中调用：将 tab_scroll_x 向 tab_scroll_target 逼近。
    /// 返回是否仍在动画中（需要继续请求下一帧）。
    pub(crate) fn tick_tab_scroll(&mut self) -> bool {
        let diff = self.editor.tab_bar.tab_scroll_target - self.editor.tab_bar.tab_scroll_x;
        if diff.abs() < 0.5 {
            // 到达目标，直接对齐
            self.editor.tab_bar.tab_scroll_x = self.editor.tab_bar.tab_scroll_target;
            return false;
        }
        // 指数衰减：每帧移动剩余距离的 25%（约 60fps 时 ~5 帧到达）
        self.editor.tab_bar.tab_scroll_x += diff * 0.25;
        true
    }

    /// 打开设置标签页（作为通用 tab 插入到标签栏）
    pub fn open_settings_tab(&mut self) {
        self.ui.settings_panel.apply_settings(&self.ui.app_settings);
        if let Some(idx) = self.find_settings_tab() {
            self.switch_tab(idx);
        } else {
            // 保存当前文件 tab 的内容到 tabs[active_tab]（如果是文件 tab）
            self.swap_tab_content(self.editor.tab_bar.active_tab);
            self.editor.tab_bar.tabs.push(crate::tabs::Tab::Settings);
            self.editor.tab_bar.active_tab = self.editor.tab_bar.tabs.len() - 1;
            // 设置 tab 无 content，self.editor.content 保持空即可
            self.editor.content = crate::tabs::TabContent::new();
            self.ai.ai_panel.input_focused = false;
            self.ui.status_message = "已打开设置标签页".to_string();
            self.emit_event(crate::events::EditorEvent::TabChanged);
        }
    }
    /// 关闭设置标签页
    pub fn close_settings_tab(&mut self) {
        if let Some(idx) = self.find_settings_tab() {
            if idx == self.editor.tab_bar.active_tab {
                // 关闭活动设置 tab，切换到最近的文件 tab
                self.close_current_tab();
            } else {
                // 非活动设置 tab 直接移除
                self.editor.tab_bar.tabs.remove(idx);
                if self.editor.tab_bar.active_tab > idx {
                    self.editor.tab_bar.active_tab -= 1;
                }
                self.ui.status_message = "已关闭设置标签页".to_string();
            }
        }
        self.ui.settings_panel.active_field = None;
    }
    /// 切换设置标签页
    pub fn toggle_settings_tab(&mut self) {
        if self.active_tab_is_settings() {
            self.close_settings_tab();
        } else {
            self.open_settings_tab();
        }
    }
    pub(super) fn create_new_file_tab(&mut self, path: &Path) {
        let lang = Language::from_path(path);
        let tab = Tab::File(TabContent::with_loaded_buffer(
            Some(path.to_path_buf()),
            PieceTable::from_string(String::new()),
            lang,
            true,
        ));
        self.open_in_new_tab(tab);
    }
}
