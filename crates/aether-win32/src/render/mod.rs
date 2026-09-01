pub(crate) use aether_core::char_width::char_width as unicode_char_width;
pub(crate) use aether_core::lexer::Language;
pub(crate) use aether_core::workspace::file_tree::{FileKind, FileTree};
pub(crate) use aether_render::d2d::factory::color_f;
pub(crate) use aether_render::d2d::glass;
pub(crate) use windows::Win32::Graphics::Direct2D::Common::{D2D_POINT_2F, D2D_RECT_F};
pub(crate) use windows::Win32::Graphics::Direct2D::{
    ID2D1SolidColorBrush, D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_DRAW_TEXT_OPTIONS_NONE,
};
pub(crate) use windows::Win32::Graphics::DirectWrite::{
    IDWriteTextFormat, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING,
};

pub(crate) use crate::editor::{BottomPanelTab, EditorState};
pub(crate) use crate::layout::{Region, ACTIVITY_BAR_BUTTON_SIZE};
pub(crate) use crate::settings::ProviderTemplateButton;

/// 绘制输入框的四条边框
pub(crate) unsafe fn draw_input_borders(
    target: &windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    brush: &ID2D1SolidColorBrush,
) {
    let top = D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + 1.0,
    };
    let bottom = D2D_RECT_F {
        left: x,
        top: y + h - 1.0,
        right: x + w,
        bottom: y + h,
    };
    let left = D2D_RECT_F {
        left: x,
        top: y,
        right: x + 1.0,
        bottom: y + h,
    };
    let right = D2D_RECT_F {
        left: x + w - 1.0,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    target.FillRectangle(&top, brush);
    target.FillRectangle(&bottom, brush);
    target.FillRectangle(&left, brush);
    target.FillRectangle(&right, brush);
}

impl EditorState {
    /// 后台任务泵：消费 AI 流式结果、Agent 动作、终端输出与 LSP 事件。
    /// 渲染路径每帧调用；冰冻态由 AI 定时器无头调用（不触发重绘），
    /// 保证最小化/长期空闲期间 AI 生成与 Agent 终端命令回环不中断。
    pub(crate) fn pump_background_tasks(&mut self) {
        // AI-H01: 轮询后台 AI 请求结果，不阻塞 UI 线程
        // 多会话并发：轮询所有会话（活动 + 后台），对本帧刚完成的每个会话处理 Agent 动作；
        // 对本帧因错误中断的会话抢救已接收的文件块（不执行 RUN 命令）
        let (ai_completed, ai_interrupted) = self.ai.ai_panel.poll_all_background();
        // 本帧是否有会话刚结束（完成/中断），用于后续脏标记判断
        let ai_just_finished = !ai_completed.is_empty() || !ai_interrupted.is_empty();
        self.ai.ai_panel.sync_active_title();
        for conv_idx in ai_completed {
            self.process_ai_agent_actions_for(conv_idx);
        }
        for conv_idx in ai_interrupted {
            self.salvage_ai_partial_edits(conv_idx);
        }

        // AI 流式生成中：标记 AI 面板所在区域脏，确保新 token 及时渲染。
        // 若缺少此标记，infer_from_state 返回 None（右侧面板可见性未变），
        // 依赖 on_paint 的全窗口防护导致每帧全量重绘，产生重影和性能浪费。
        // 智能体模式下生成中 UI（流式文本/停止按钮）在中间列，需标中间区域。
        // 完成/中断帧同样要标脏：此时 any_generating() 已变 false，若不标，
        // 思考块自动折叠（块高收缩、后续内容上移）那一帧不会重绘，
        // 旧的展开思考内容残留在屏幕上形成重影。
        if self.ai.ai_panel.any_generating() || ai_just_finished {
            if self.editor_mode.is_agent() {
                self.mark_ai_panel_dirty();
            } else if self.ui.layout.right_panel_visible {
                let rp = self.ui.layout.right_panel_region();
                if rp.width > 0.0 && rp.height > 0.0 {
                    self.win.dirty_tracker.mark_region(
                        rp.x,
                        rp.y,
                        rp.width,
                        rp.height,
                        crate::dirty_rect::DirtyRegionType::RightPanel,
                    );
                }
            }
        }

        // LSP: 轮询诊断事件，更新 diagnostics 字段
        // 诊断变化时标记编辑区脏矩形，确保波浪线及时出现/消失（否则残留为重影）
        if self
            .lsp
            .lsp
            .poll_events(&mut self.lsp.diagnostics, &mut self.ui.status_message)
        {
            let er = self.ui.layout.editor_content_region(self.show_tab_bar());
            self.win.dirty_tracker.mark_region(
                er.x,
                er.y,
                er.width,
                er.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
        }

        // 终端输出轮询：从读取线程拉取子进程 stdout/stderr 并写入输出缓存。
        // 此前未调用 flush_output 导致 shell 输出无法显示，现在每帧轮询保证实时性。
        if self.terminal.terminal_panel.running {
            self.terminal.terminal_panel.poll_startup();
            // 拉取到新输出时标脏底部面板区域，确保输出及时触发局部重绘（而非等全窗口）
            if self.terminal.terminal_panel.flush_output() {
                let bp = self.ui.layout.bottom_panel_region();
                self.win.dirty_tracker.mark_region(
                    bp.x,
                    bp.y,
                    bp.width,
                    bp.height,
                    crate::dirty_rect::DirtyRegionType::BottomPanel,
                );
            }
            // AI Agent 排队命令：终端就绪后自动发送执行
            self.terminal.terminal_panel.flush_pending_commands();
        }
        // AI Agent 命令完成回环：输出回传给 AI 继续推理（哨兵检测，含超时兕底）
        let agent_results = self.terminal.terminal_panel.poll_agent_results();
        if !agent_results.is_empty() {
            self.handle_agent_command_results(agent_results);
        }
    }

    pub fn render(&mut self) {
        // 避免0尺寸渲染
        if self.win.window_width == 0 || self.win.window_height == 0 {
            return;
        }
        // 冰冻且最小化：跳过渲染，防止杂散 WM_PAINT 重建已释放的 D2D 资源；
        // 后台任务由无头泵（AI 定时器）驱动，不依赖渲染路径
        if self.power.frozen && self.power.minimized {
            return;
        }

        // TEST: 每帧开始清除上一帧命中区域
        crate::hit_test::clear_hit_regions();

        // 后台任务泥已移至独立定时器（TERM_TIMER 每 33ms），
        // 渲染路径零 IO 阻塞，保证点击/编辑即时响应。

        // UI-L07: 降级为 trace，避免生产环境每帧日志噪声
        tracing::trace!(
            win_w = self.win.window_width,
            win_h = self.win.window_height,
            "render() start"
        );

        // 确保渲染目标存在（设备丢失后重建）
        if self.win.render_ctx.target.is_none() {
            let _ = self.init_render_target();
            // 渲染目标就绪后预初始化常用画笔和文本格式
            if let Some(rt) = &self.win.render_ctx.target {
                let target = rt.target().clone();
                let common_colors = [
                    self.win.theme.editor_bg,
                    self.win.theme.line_number_bg,
                    self.win.theme.line_number_fg,
                    self.win.theme.line_highlight_bg,
                    self.win.theme.selection_bg,
                    self.win.theme.cursor_color,
                    self.win.theme.sidebar_bg,
                    self.win.theme.statusbar_bg,
                    self.win.theme.text_default,
                    self.win.theme.tab_active_bg,
                    self.win.theme.tab_inactive_bg,
                    self.win.theme.titlebar_bg,
                    self.win.theme.activity_bar_bg,
                    self.win.theme.panel_border,
                    self.win.theme.shadow,
                    self.win.theme.glow_selection,
                    self.win.theme.command_palette_bg,
                    self.win.theme.submenu_bg,
                ];
                self.win
                    .render_ctx
                    .brush_cache
                    .init_common_brushes(&target, &common_colors);
                let font_size = self.win.text_renderer.font_size();
                self.win
                    .render_ctx
                    .text_format_cache
                    .init_common_formats(font_size);
                // 预构建 SVG 图标几何：避免在设置页/欢迎页等不渲染图标的页面停留后，
                // 首次切换回文件标签时由标题栏/活动栏/状态栏的懒加载触发 87 个
                // PathGeometry 的集中构建，导致该帧卡顿（表现为文件需点两次才打开）。
                self.ui.icons.ensure_created_from_target(&target);
            }
        }

        // 计算编辑器可见行范围，用于增量缓存重建
        let show_tab_bar = self.show_tab_bar();
        let editor_content_region = self.ui.layout.editor_content_region(show_tab_bar);
        let line_height = self.win.text_renderer.line_height();
        let total_lines = self.editor.content.buffer.len_lines().max(1);
        let visible_start = (self.editor.content.scroll_y / line_height) as usize;
        let visible_lines = (editor_content_region.height / line_height) as usize + 2;
        let visible_end = (visible_start + visible_lines).min(total_lines);

        self.rebuild_cache(visible_start, visible_end);

        // 使用布局管理器计算各区域
        let titlebar_region = self.ui.layout.title_bar_region();
        let menu_region = self.ui.layout.menu_bar_region();
        let activity_region = self.ui.layout.activity_bar_region();
        let sidebar_region = self.ui.layout.sidebar_region();
        let editor_region = self.ui.layout.editor_region();
        let tab_region = self.ui.layout.tab_bar_region(show_tab_bar);
        let status_region = self.ui.layout.status_bar_region();
        let right_panel_region = self.ui.layout.right_panel_region();

        // 预计算标签栏布局
        if show_tab_bar {
            self.update_tab_layouts(editor_region.x, editor_region.width, tab_region.height);
        }

        // 预计算菜单栏 item 位置（用于子菜单定位和 hover 检测）
        // 菜单项现在绘制在标题栏内，从左侧开始，避开窗口控制按钮区域
        // 优化：只在 layout_dirty 时重建，避免每帧分配
        if self.ui.menu_bar.layout_dirty {
            self.ui.menu_bar.item_widths.clear();
            self.ui
                .menu_bar
                .item_widths
                .reserve(self.ui.menu_bar.items.len());
            for item in &self.ui.menu_bar.items {
                // 优先用 DirectWrite 精确测量文本宽度，保证各菜单项间距均匀；
                // 测量失败时回退到字符宽度估算
                let text_width = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(&item.label, 13.0, DWRITE_FONT_WEIGHT_NORMAL.0 as u32)
                    .filter(|w| *w > 0.0)
                    .unwrap_or_else(|| {
                        item.label
                            .chars()
                            .map(|ch| if ch.is_ascii() { 8.0 } else { 13.0 })
                            .sum()
                    });
                let item_width = text_width + 16.0; // 左右各 8px padding（紧凑风格）
                self.ui.menu_bar.item_widths.push(item_width);
            }
            self.ui.menu_bar.layout_dirty = false;
        }
        // 每帧只需重新计算 x 位置（因为起始 x 可能随标题栏变化）
        {
            let mut item_x = titlebar_region.x + 8.0;
            self.ui.menu_bar.item_x_positions.clear();
            self.ui
                .menu_bar
                .item_x_positions
                .reserve(self.ui.menu_bar.items.len());
            for (i, _item) in self.ui.menu_bar.items.iter().enumerate() {
                let item_width = self.ui.menu_bar.item_widths.get(i).copied().unwrap_or(60.0);
                self.ui.menu_bar.item_x_positions.push(item_x);
                item_x += item_width;
            }
        }

        // P1.2: 先把事件队列中累积的事件转换为脏矩形
        self.flush_events_to_dirty_tracker();

        // 标签栏平滑滚动动画 tick
        if self.tick_tab_scroll() {
            // 动画进行中：标记标签栏区域脏，并请求下一帧
            let show_tab_bar = self.show_tab_bar();
            let tr = self.ui.layout.tab_bar_region(show_tab_bar);
            self.win.dirty_tracker.mark_region(
                tr.x,
                tr.y,
                tr.width,
                tr.height,
                crate::dirty_rect::DirtyRegionType::TabBar,
            );
            crate::window::invalidate_window(self.win.hwnd);
        }

        // 侧边栏宽度动画 tick：每帧 lerp，到达终态后布可见性并清除动画
        if let Some(anim) = self.ui.layout.sidebar_anim {
            let (width, done) = anim.tick();
            if done {
                if anim.end_width <= 0.0 {
                    // 收起终态：隐藏 + 宽度复位到默认（下次展开时使用）
                    self.ui.layout.sidebar_visible = false;
                    self.ui.layout.sidebar_width = crate::layout::SIDEBAR_WIDTH;
                } else {
                    self.ui.layout.sidebar_visible = true;
                    self.ui.layout.sidebar_width = anim.end_width;
                }
                self.ui.layout.sidebar_anim = None;
            } else {
                self.ui.layout.sidebar_width = width.max(0.0);
            }
            // 动画期间 + 终态帧均强制全窗口重绘，避免侧边栏区域残影
            self.win.dirty_tracker.mark_full_window();
            // 终态帧后再请求一次重绘，确保编辑器区域完全覆盖旧侧边栏位置
            crate::window::invalidate_window(self.win.hwnd);
        }

        // 脏矩形检测：对比上一帧状态，标记变化区域（兼容层）
        let cursor_moved = self.editor.content.cursor_line != self.input.prev.cursor_line
            || self.editor.content.cursor_col != self.input.prev.cursor_col;
        let scroll_changed = (self.editor.content.scroll_y - self.input.prev.scroll_y).abs() > 0.01;
        let selection_changed = self.editor.content.selection_start
            != self.input.prev.selection_start
            || self.editor.content.selection_end != self.input.prev.selection_end;
        let sidebar_changed = self.ui.sidebar_content != self.input.prev.sidebar_content;
        let sidebar_visible_changed =
            self.ui.layout.sidebar_visible != self.input.prev.sidebar_visible;
        let activity_bar_visible_changed =
            self.ui.layout.activity_bar_visible != self.input.prev.activity_bar_visible;
        let right_panel_changed =
            self.ui.layout.right_panel_visible != self.input.prev.right_panel_visible;
        let bottom_panel_changed =
            self.ui.layout.bottom_panel_visible != self.input.prev.bottom_panel_visible;
        let status_changed = self.ui.status_message != self.input.prev.status_message;
        let active_tab_changed = self.editor.tab_bar.active_tab != self.input.prev.active_tab;
        let dialog_visible = self.remote.ssh_dialog.visible
            || self.remote.clone_dialog.visible
            || self.ui.command_palette.visible;

        // P5-2: 标签页切换只影响标签栏高亮、编辑器内容、状态栏三个区域，
        // 侧边栏/活动栏/AI 面板与活动标签无关，改为精确标记省去 ~60% 绘制面积。
        // 仅当新旧标签涉及 Welcome/Settings 等特殊页类型时保留全窗口重绘
        //（欢迎页跳过侧边栏等面板渲染，局部裁剪会留下残影）
        if active_tab_changed {
            let is_special = |idx: usize| -> bool {
                self.editor
                    .tab_bar
                    .tabs
                    .get(idx)
                    .map(|t| !t.is_file())
                    .unwrap_or(true)
            };
            if is_special(self.input.prev.active_tab)
                || is_special(self.editor.tab_bar.active_tab)
                || self.show_welcome()
            {
                self.win.dirty_tracker.mark_full_window();
            } else {
                let show_tab_bar = self.show_tab_bar();
                let tab_region = self.ui.layout.tab_bar_region(show_tab_bar);
                self.win.dirty_tracker.mark_region(
                    tab_region.x,
                    tab_region.y,
                    tab_region.width,
                    tab_region.height,
                    crate::dirty_rect::DirtyRegionType::TabBar,
                );
                let editor_region = self.ui.layout.editor_region();
                self.win.dirty_tracker.mark_region(
                    editor_region.x,
                    editor_region.y,
                    editor_region.width,
                    editor_region.height,
                    crate::dirty_rect::DirtyRegionType::EditorContent,
                );
                let status_region = self.ui.layout.status_bar_region();
                self.win.dirty_tracker.mark_status_bar(
                    status_region.x,
                    status_region.y,
                    status_region.width,
                    status_region.height,
                );
            }
        }

        // 底部面板可见性变化属于重大布局变更，强制全量重绘以保证编辑器区域正确刷新
        if bottom_panel_changed {
            self.win.dirty_tracker.mark_full_window();
        }
        // REQ-P0-06: 侧边栏/活动栏可见性变化改为精确区域标记，
        // 避免不必要的全窗口重绘。标记侧边栏、活动栏和编辑器区域（布局位移）
        if sidebar_visible_changed || activity_bar_visible_changed {
            let activity_region = self.ui.layout.activity_bar_region();
            self.win.dirty_tracker.mark_region(
                activity_region.x,
                activity_region.y,
                activity_region.width,
                activity_region.height,
                crate::dirty_rect::DirtyRegionType::ActivityBar,
            );
            let sidebar_region = self.ui.layout.sidebar_region();
            self.win.dirty_tracker.mark_region(
                sidebar_region.x,
                sidebar_region.y,
                sidebar_region.width,
                sidebar_region.height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
            // 编辑器区域因布局位移需要重绘
            let editor_region = self.ui.layout.editor_region();
            self.win.dirty_tracker.mark_region(
                editor_region.x,
                editor_region.y,
                editor_region.width,
                editor_region.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
        }
        // REQ-P0-06: 侧边栏内容切换/右侧面板可见性变化改为精确区域标记
        if sidebar_changed {
            let sidebar_region = self.ui.layout.sidebar_region();
            self.win.dirty_tracker.mark_region(
                sidebar_region.x,
                sidebar_region.y,
                sidebar_region.width,
                sidebar_region.height,
                crate::dirty_rect::DirtyRegionType::Sidebar,
            );
        }
        if right_panel_changed {
            let right_panel_region = self.ui.layout.right_panel_region();
            self.win.dirty_tracker.mark_region(
                right_panel_region.x,
                right_panel_region.y,
                right_panel_region.width,
                right_panel_region.height,
                crate::dirty_rect::DirtyRegionType::RightPanel,
            );
            // 编辑器区域因布局位移需要重绘
            let editor_region = self.ui.layout.editor_region();
            self.win.dirty_tracker.mark_region(
                editor_region.x,
                editor_region.y,
                editor_region.width,
                editor_region.height,
                crate::dirty_rect::DirtyRegionType::EditorContent,
            );
        }

        // 根据状态变化推断最优渲染命令
        let render_cmd = crate::dirty_rect::RenderCommand::infer_from_state(
            cursor_moved,
            selection_changed,
            false,
            scroll_changed,
            sidebar_changed,
            right_panel_changed,
            bottom_panel_changed,
            status_changed,
            dialog_visible,
        );

        // 标记脏区域：根据状态变化自动推断，同时保留外部显式标记
        // 如果已经有全窗口标记，不需要再推断
        if !self.win.dirty_tracker.is_full_window() {
            match render_cmd {
                crate::dirty_rect::RenderCommand::EditorOnly => {
                    let line_height = self.win.text_renderer.line_height();
                    let editor_content_region = self.ui.layout.editor_content_region(show_tab_bar);
                    let cursor_y = editor_content_region.y
                        + self.editor.content.cursor_line as f32 * line_height
                        - self.editor.content.scroll_y;
                    self.win.dirty_tracker.mark_cursor(
                        editor_content_region.x,
                        cursor_y,
                        2.0,
                        line_height,
                    );
                }
                crate::dirty_rect::RenderCommand::EditorAndStatus => {
                    self.win.dirty_tracker.mark_region(
                        editor_region.x,
                        editor_region.y,
                        editor_region.width,
                        editor_region.height,
                        crate::dirty_rect::DirtyRegionType::EditorContent,
                    );
                    self.win.dirty_tracker.mark_status_bar(
                        status_region.x,
                        status_region.y,
                        status_region.width,
                        status_region.height,
                    );
                }
                crate::dirty_rect::RenderCommand::SidebarOnly => {
                    self.win.dirty_tracker.mark_region(
                        sidebar_region.x,
                        sidebar_region.y,
                        sidebar_region.width,
                        sidebar_region.height,
                        crate::dirty_rect::DirtyRegionType::Sidebar,
                    );
                }
                crate::dirty_rect::RenderCommand::RightPanelOnly => {
                    self.win.dirty_tracker.mark_region(
                        right_panel_region.x,
                        right_panel_region.y,
                        right_panel_region.width,
                        right_panel_region.height,
                        crate::dirty_rect::DirtyRegionType::RightPanel,
                    );
                }
                crate::dirty_rect::RenderCommand::BottomPanelOnly => {
                    let bottom_region = self.ui.layout.bottom_panel_region();
                    if bottom_region.height > 0.0 {
                        self.win.dirty_tracker.mark_region(
                            bottom_region.x,
                            bottom_region.y,
                            bottom_region.width,
                            bottom_region.height,
                            crate::dirty_rect::DirtyRegionType::BottomPanel,
                        );
                    }
                }
                crate::dirty_rect::RenderCommand::FullRedraw => {
                    self.win.dirty_tracker.mark_full_window();
                }
                // REQ-P0-06: 无状态变化时不标记任何脏区域
                crate::dirty_rect::RenderCommand::None => {}
            }
        }

        // REQ-P0-06: 如果没有脏区域，跳过渲染（避免无变化时的全窗口重绘）
        if !self.win.dirty_tracker.has_dirty() {
            // 仍需更新上一帧状态追踪，避免下一帧误检测到变化
            self.snapshot_prev_frame_state();
            return;
        }

        // 菜单 hover 快速路径：脏区全部为 Dialog 类型（仅菜单 hover 标记该类型）且有
        // 上下文菜单展开时，跳过整条渲染管线（面板遍历 + 几何裁剪层构建），只重绘
        // 不透明的菜单本体。菜单是最顶层不透明绘制，直接覆盖旧高亮即可，单帧成本
        // 从整管线降至若干矩形 + 几行文本，彻底消除快速滑动菜单时的卡顿。
        {
            let menus_open = self.ui.context_menus.explorer.is_open
                || self.ui.context_menus.file_node.is_open
                || self.ui.context_menus.tab.visible
                || self.ui.context_menus.activity_bar.visible;
            let dialog_only = !self.win.dirty_tracker.is_full_window()
                && self
                    .win
                    .dirty_tracker
                    .rects()
                    .iter()
                    .all(|r| r.region_type == crate::dirty_rect::DirtyRegionType::Dialog);
            if menus_open && dialog_only {
                let target = {
                    let Some(rt) = &self.win.render_ctx.target else {
                        return;
                    };
                    rt.target().clone()
                };
                // 裁剪到菜单本体矩形：菜单阴影为半透明绘制，若不裁剪，每次 hover
                // 重绘都会在旧阴影上再叠一层，数次后菜单右/下侧积出黑色偏移块。
                let clip_to = |x: f32, y: f32, w: f32, h: f32| {
                    windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + w,
                        bottom: y + h,
                    }
                };
                self.win.render_ctx.begin_draw();
                if self.ui.context_menus.explorer.is_open {
                    let r = clip_to(
                        self.ui.context_menus.explorer.origin_x,
                        self.ui.context_menus.explorer.origin_y,
                        self.ui.context_menus.explorer.menu_width(),
                        self.ui.context_menus.explorer.menu_height(),
                    );
                    unsafe { target.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_ALIASED) };
                    self.render_explorer_context_menu(&target);
                    unsafe { target.PopAxisAlignedClip() };
                }
                if self.ui.context_menus.file_node.is_open {
                    let r = clip_to(
                        self.ui.context_menus.file_node.origin_x,
                        self.ui.context_menus.file_node.origin_y,
                        self.ui.context_menus.file_node.menu_width(),
                        self.ui.context_menus.file_node.menu_height(),
                    );
                    unsafe { target.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_ALIASED) };
                    self.render_file_node_context_menu(&target);
                    unsafe { target.PopAxisAlignedClip() };
                }
                if self.ui.context_menus.tab.visible {
                    let r = clip_to(
                        self.ui.context_menus.tab.x,
                        self.ui.context_menus.tab.y,
                        self.ui.context_menus.tab.width,
                        self.ui.context_menus.tab.menu_height(),
                    );
                    unsafe { target.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_ALIASED) };
                    self.render_tab_context_menu(&target);
                    unsafe { target.PopAxisAlignedClip() };
                }
                if self.ui.context_menus.activity_bar.visible {
                    let r = clip_to(
                        self.ui.context_menus.activity_bar.x,
                        self.ui.context_menus.activity_bar.y,
                        self.ui.context_menus.activity_bar.width,
                        self.ui.context_menus.activity_bar.menu_height(),
                    );
                    unsafe { target.PushAxisAlignedClip(&r, D2D1_ANTIALIAS_MODE_ALIASED) };
                    self.render_activity_bar_context_menu(&target);
                    unsafe { target.PopAxisAlignedClip() };
                }
                if let Err(e) = self.win.render_ctx.end_draw() {
                    // 帧被丢弃（如批次外绘制残留导致 D2DERR_WRONG_STATE）：
                    // 标记全窗口并立即请求重绘。若只标记不重绘，要等到下一次
                    // 交互才恢复，表现为菜单 hover 高亮"隔次丢失"。
                    tracing::warn!(
                        hresult = format_args!("{:#010X}", e.code().0 as u32),
                        "菜单快速路径 EndDraw 失败，丢帧后全窗口重绘恢复"
                    );
                    self.recover_from_end_draw_failure();
                } else {
                    self.win.end_draw_fail_streak = 0;
                    self.win.dirty_tracker.clear();
                }
                return;
            }
        }

        // 获取渲染目标，开始绘制
        let target = {
            let Some(rt) = &self.win.render_ctx.target else {
                return;
            };
            rt.target().clone()
        };
        self.win.render_ctx.begin_draw();

        // 设置裁剪区域（脏矩形优化）
        let use_clip =
            !self.win.dirty_tracker.is_full_window() && self.win.dirty_tracker.has_dirty();
        // REQ-P3-03: 使用多矩形并集裁剪，避免合并为单一包围盒导致的重绘面积膨胀
        let mut use_layer = false;
        if use_clip {
            let rects = self.win.dirty_tracker.rects();
            if !rects.is_empty() {
                let rect_tuples: Vec<(f32, f32, f32, f32)> = rects
                    .iter()
                    .map(|r| (r.x, r.y, r.width, r.height))
                    .collect();
                use_layer = self
                    .win
                    .render_ctx
                    .push_multi_clip(self.win.d2d_factory.factory(), &rect_tuples);
            }
        }

        // 全窗口清除只在全窗口重绘时执行
        if self.win.dirty_tracker.is_full_window() || !use_clip {
            // 欢迎页状态下使用深色背景（而非透明），避免面板区域出现黑色空洞
            // 透明色虽能让 DWM Mica/Acrylic 透出，但会导致未覆盖区域显示为黑色
            if self.show_welcome() {
                self.win.render_ctx.clear(
                    &windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                        r: 0.09,
                        g: 0.09,
                        b: 0.09,
                        a: 1.0,
                    },
                );
            } else {
                self.win.render_ctx.clear(&self.win.theme.editor_bg);
            }
        }

        // 欢迎页 + 脏矩形裁剪时，侧边栏/活动栏/右侧面板/底部面板区域不会被全窗口 clear 覆盖，
        // 且欢迎页逻辑跳过这些面板的渲染，导致这些区域显示为黑色。
        // 手动填充这些区域以保证背景色正确。
        if self.show_welcome() && use_clip {
            if self.ui.layout.activity_bar_visible && activity_region.width > 0.0 {
                self.win.render_ctx.fill_rect(
                    activity_region.x,
                    activity_region.y,
                    activity_region.width,
                    activity_region.height,
                    &self.win.theme.activity_bar_bg,
                );
            }
            if self.ui.layout.sidebar_visible && sidebar_region.width > 0.0 {
                self.win.render_ctx.fill_rect(
                    sidebar_region.x,
                    sidebar_region.y,
                    sidebar_region.width,
                    sidebar_region.height,
                    &self.win.theme.sidebar_bg,
                );
            }
            if self.ui.layout.right_panel_visible && right_panel_region.width > 0.0 {
                self.win.render_ctx.fill_rect(
                    right_panel_region.x,
                    right_panel_region.y,
                    right_panel_region.width,
                    right_panel_region.height,
                    &self.win.theme.sidebar_bg,
                );
            }
            if self.ui.layout.bottom_panel_visible {
                let bottom_region = self.ui.layout.bottom_panel_region();
                if bottom_region.height > 0.0 {
                    // 欢迎页状态下，底部面板需要覆盖整个窗口宽度（包括右侧面板下方），
                    // 避免右侧面板下方出现黑色空洞
                    let full_width = self.win.window_width as f32;
                    self.win.render_ctx.fill_rect(
                        0.0,
                        bottom_region.y,
                        full_width,
                        bottom_region.height,
                        &self.win.theme.statusbar_bg,
                    );
                }
            }
        }

        // 预提取菜单栏数据，避免借用冲突
        let item_x_positions = self.ui.menu_bar.item_x_positions.clone();
        let item_widths = self.ui.menu_bar.item_widths.clone();

        let showing_welcome = self.show_welcome();

        // 欢迎页左栏同步（不修改 visible 用户状态）：进入欢迎页瞬间重置
        // welcome_left_opened（侧边栏面板默认收起）；欢迎页期间用户可经
        // 活动栏图标/Ctrl+B/标题栏按钮调起面板；活动栏在欢迎页固定常驻。
        if showing_welcome && !self.ui.layout.welcome_active {
            self.ui.layout.welcome_left_opened = false;
        }
        self.ui.layout.welcome_active = showing_welcome;
        let suppress_sidebar = self.ui.layout.welcome_sidebar_suppressed();

        // 0. 标题栏（最先渲染，作为背景）
        if self.ui.layout.title_bar_visible {
            self.render_title_bar(&target, &titlebar_region);
        }

        // 1. 菜单栏
        if self.ui.layout.menu_bar_visible {
            self.render_menu_bar(&item_x_positions, &item_widths, &target, &menu_region);
        }

        // 2. 活动栏（欢迎页固定常驻：侧边栏功能按钮入口）
        if self.ui.layout.activity_bar_shown() {
            self.render_activity_bar(&target, &activity_region);
        }

        // 3. 侧边栏（欢迎页面板默认收起，用户可手动调起）
        // 智能体模式下：左侧渲染对话历史+工作区复合面板
        if self.ui.layout.sidebar_visible && !suppress_sidebar {
            if self.editor_mode.is_agent() {
                self.render_agent_sidebar(
                    &target,
                    sidebar_region.x,
                    sidebar_region.y,
                    sidebar_region.width,
                    sidebar_region.height,
                );
            } else {
                self.render_sidebar(&target, &sidebar_region);
            }
        }

        // 4. 标签栏（智能体模式下标签栏在右侧区域渲染，此处跳过）
        if show_tab_bar && !showing_welcome && !self.editor_mode.is_agent() {
            self.render_tab_bar(
                &target,
                tab_region.x,
                tab_region.y,
                tab_region.width,
                tab_region.height,
            );
        }

        // 5. 编辑器内容/欢迎页/图片预览/设置页
        // 智能体模式下：中间区域渲染 AI 对话面板，右侧区域渲染文件编辑区
        if showing_welcome && !self.editor_mode.is_agent() {
            tracing::trace!("render: before welcome_page");
            // 欢迎页在编辑器区域内居中：默认无左栏时即全屏宽，
            // 用户调起活动栏/侧边栏后自动避让（editor_region 已扣除左侧与右面板）
            let editor = self.ui.layout.editor_region();
            let welcome_x = editor.x;
            let welcome_width = editor.width;
            let welcome_y = self.ui.layout.top_offset();
            let mut welcome_height = self.win.window_height as f32 - welcome_y;
            if self.ui.layout.status_bar_visible {
                welcome_height -= self.ui.layout.status_bar_height;
            }
            if self.ui.layout.bottom_panel_visible {
                welcome_height -= self.ui.layout.bottom_panel_height;
            }
            welcome_height = welcome_height.max(200.0);
            self.render_welcome_page(&target, welcome_x, welcome_y, welcome_width, welcome_height);
            tracing::trace!("render: after welcome_page");
        } else if self.editor_mode.is_agent() {
            // 智能体模式：中间区域渲染 AI 对话面板（占据编辑器内容区域，包括欢迎页/空占位页场景）
            // 智能体模式标签栏在右侧面板渲染，中间区域不预留标签栏高度
            // 检测 AI 面板超时（与右侧面板路径一致，避免思考模式长等待请求永不收尾）
            if self.ai.ai_panel.check_timeout() {
                self.ai.ai_panel.handle_timeout();
            }
            let agent_center = self.ui.layout.editor_content_region(false);
            let text_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(&target, &self.win.theme.text_default)
            {
                Ok(b) => b,
                Err(_) => return,
            };
            self.render_ai_assistant_sidebar(
                &target,
                agent_center.x,
                agent_center.y,
                agent_center.width,
                agent_center.height,
                &text_brush,
            );
        } else if self.active_tab_is_new_tab() {
            // 新标签页（NTP）：编辑器内容区域渲染快捷搜索框 + 快捷操作按钮
            self.render_new_tab_page(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
            );
        } else if self.active_tab_is_browser() {
            // 浏览器标签：D2D 只画顶部工具栏，网页正文由 WebView2 子窗口覆盖
            self.render_browser_toolbar(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
            );
        } else if self.active_tab_is_settings() {
            // 设置页面：在编辑器内容区域渲染左侧导航+右侧内容
            let text_brush = match self
                .win
                .render_ctx
                .brush_cache
                .get_brush(&target, &self.win.theme.text_default)
            {
                Ok(b) => b,
                Err(_) => return,
            };
            self.render_settings_sidebar(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
                &text_brush,
            );
        } else if self.active_tab_is_sandbox_eval() {
            // 智能体沙盒评测页：在编辑器内容区域整页渲染
            unsafe {
                self.render_sandbox_eval_page(
                    &target,
                    editor_content_region.x,
                    editor_content_region.y,
                    editor_content_region.width,
                    editor_content_region.height,
                );
            }
        } else if self.editor.content.language == Language::Image {
            self.render_image_preview(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
            );
        } else if self.editor.markdown_preview && self.editor.content.language == Language::Markdown
        {
            self.render_markdown_preview(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
            );
        } else {
            self.render_editor(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
                editor_content_region.height,
            );
        }
        // 5.4 Markdown 预览切换按钮（编辑区右上角，仅 .md 文件显示）
        if self.editor.content.language == Language::Markdown && !showing_welcome {
            self.render_markdown_toggle_btn(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
            );
        } else {
            self.editor.markdown_toggle_btn = None;
        }

        // 5.5 查找替换框
        if self.editor.find.visible {
            self.render_find_replace(
                &target,
                editor_content_region.x,
                editor_content_region.y,
                editor_content_region.width,
            );
        }

        // 6. 右侧面板（AI面板等）
        // 智能体模式下：右侧区域渲染浏览器风格标签页容器（而非 AI 面板）
        {
            if self.editor_mode.is_agent() {
                let editor_x = right_panel_region.x;
                let editor_y = right_panel_region.y;
                let editor_w = right_panel_region.width;
                let editor_h = right_panel_region.height;
                self.render_agent_right_panel(&target, editor_x, editor_y, editor_w, editor_h);
            } else if self.ui.layout.right_panel_visible
                && right_panel_region.width > 1.0
                && right_panel_region.height > 1.0
            {
                tracing::trace!(region = ?right_panel_region, "render: before right_panel");
                self.render_right_panel(&target, &right_panel_region);
                tracing::trace!("render: after right_panel");
            }
        }

        // 7. 底部面板（终端、输出等）
        if self.ui.layout.bottom_panel_visible {
            let bottom_region = self.ui.layout.bottom_panel_region();
            self.render_bottom_panel(
                &target,
                bottom_region.x,
                bottom_region.y,
                bottom_region.width,
                bottom_region.height,
            );
        }

        // 8. 状态栏
        if self.ui.layout.status_bar_visible {
            self.render_statusbar(&target, &status_region);
        }

        // 8. 子菜单（最后渲染，避免被欢迎页/编辑器遮盖）
        // 预提取子菜单数据，避免借用冲突
        // REQ-P3-02: 测量并缓存子菜单宽度，hit_test 时复用
        let submenu_data = self.ui.menu_bar.active_index.and_then(|active_idx| {
            self.ui
                .menu_bar
                .items
                .get(active_idx)
                .filter(|item| item.expanded)
                .and_then(|item| {
                    let submenu_x = self.ui.menu_bar.item_x_positions.get(active_idx).copied()?;
                    Some((active_idx, submenu_x, item.clone()))
                })
        });
        if let Some((active_idx, submenu_x, item)) = submenu_data {
            // REQ-P3-02: 测量子菜单内容宽度并写回缓存，hit_test 时使用
            let measured = self.measure_submenu_width(&item);
            if let Some(item_ref) = self.ui.menu_bar.items.get_mut(active_idx) {
                item_ref.submenu_width = measured;
            }
            let item_for_render = crate::menu_bar::MenuBarItem {
                submenu_width: measured,
                ..item
            };
            // 子菜单从标题栏下方弹出
            let submenu_y = titlebar_region.y + titlebar_region.height;
            self.render_submenu(&target, submenu_x, submenu_y, &item_for_render);
        }

        // 8. 命令面板（最上层渲染）
        if self.ui.command_palette.visible {
            let palette_width = 600.0;
            let palette_x = (self.win.window_width as f32 - palette_width) / 2.0;
            let palette_y = titlebar_region.y + titlebar_region.height + 20.0;
            self.render_command_palette(&target, palette_x, palette_y, palette_width);
        }

        // 9. SSH 连接对话框
        if self.remote.ssh_dialog.visible {
            self.render_ssh_dialog(&target);
        }

        // 10. 克隆仓库对话框
        if self.remote.clone_dialog.visible {
            self.render_clone_dialog(&target);
        }

        // 11. 新建项目对话框
        if self.ui.new_project_dialog.visible {
            self.render_new_project_dialog(&target);
        }

        // 12. 用户下拉菜单（最后渲染，确保在所有 UI 之上）
        if self.ui.user_menu.is_open {
            let titlebar_h = self.ui.layout.title_bar_height;
            let window_w = self.win.window_width as f32;
            // 与标题栏按钮共用单一布局源，菜单精确锚定在用户头像按钮下方
            let tb = crate::layout::TitlebarButtons::compute(0.0, window_w);
            let user_btn_y = (titlebar_h - tb.user_btn_size) / 2.0;
            self.render_user_menu(&target, tb.user_btn_x, user_btn_y + tb.user_btn_size + 4.0);
        }

        // 13. 资源管理器空白区域上下文菜单（最上层渲染，覆盖所有内容）
        if self.ui.context_menus.explorer.is_open {
            self.render_explorer_context_menu(&target);
        }

        // 13b. 文件节点右键上下文菜单（最上层渲染）
        if self.ui.context_menus.file_node.is_open {
            self.render_file_node_context_menu(&target);
        }

        // 14. 标签右键上下文菜单（最顶层渲染，覆盖所有内容）
        if self.ui.context_menus.tab.visible {
            self.render_tab_context_menu(&target);
        }

        // 15. 活动栏右键上下文菜单（最顶层渲染，覆盖所有内容）
        if self.ui.context_menus.activity_bar.visible {
            self.render_activity_bar_context_menu(&target);
        }

        // 15c. AI 历史对话浮动窗口（可拖动，覆盖于所有面板/编辑器之上）
        if self.ai.ai_panel.history_open {
            self.render_history_float_window(&target);
        }

        // 16. P3.4: hover tooltip + UI Tooltip（最上层，必须在 EndDraw 之前）。
        // 此前二者在 EndDraw 之后调用：批次外命令被 D2D 静默丢弃（tooltip
        // 实际画不出来），且错误会锁存到下一次 EndDraw 返回 D2DERR_WRONG_STATE，
        // 导致下一帧（菜单 hover 快速路径）整帧被丢弃——单双数交替丢帧的根因。
        self.render_hover_tooltip(&target);
        if let Ok(tooltip_format) = self.win.render_ctx.text_format_cache.get_format(
            12.0,
            DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
            DWRITE_TEXT_ALIGNMENT_LEADING.0 as u32,
            DWRITE_PARAGRAPH_ALIGNMENT_NEAR.0 as u32,
        ) {
            self.render_tooltip(&target, &tooltip_format);
        }

        // 弹出裁剪区域（如果设置了）——必须在 end_draw 之前
        // REQ-P3-03: 根据 use_layer 标志选择 PopLayer 或 PopAxisAlignedClip
        if use_clip {
            self.win.render_ctx.pop_multi_clip(use_layer);
        }

        // 内置浏览器：同步 WebView2 子窗口边界/显隐（仅智能体模式活动浏览器标签可见）
        self.sync_browser_webviews();

        // IME 合成/候选窗口跟随当前聚焦输入框（每帧统一同步，覆盖编辑器未渲染场景）
        self.sync_ime_position();

        let mut end_draw_ok = true;
        match self.win.render_ctx.end_draw() {
            Ok(()) => {}
            Err(e) => {
                end_draw_ok = false;
                let hr = e.code().0 as u32;
                tracing::warn!(
                    hresult = format_args!("{:#010X}", hr),
                    "EndDraw 失败，本帧被丢弃"
                );
                // 可恢复的渲染目标失效错误，需重建渲染目标：
                // - 0x8899000C D2DERR_RECREATE_TARGET：设备丢失（驱动重置等）
                // - 0x88990014 D2DERR_DISPLAY_STATE_INVALID：锁屏/休眠/显示器
                //   切换后显示状态失效。若不重建，EndDraw 会永久失败，
                //   整个窗口冻结（点击任何 UI 都没有视觉响应）。
                let recoverable = hr == 0x8899000C || hr == 0x88990014;
                // 兜底：连续失败达到阈值时无论错误码都强制重建，
                // 防止未知错误码导致窗口永久冻结
                let force_rebuild = self.win.end_draw_fail_streak >= 3;
                if recoverable || force_rebuild {
                    if force_rebuild && !recoverable {
                        tracing::warn!(
                            streak = self.win.end_draw_fail_streak,
                            "EndDraw 连续失败，强制重建渲染目标"
                        );
                    }
                    self.win.render_ctx.handle_device_lost();
                    // P4-4: 同时清理 IconCache，确保下次绘制时从新 factory 重建几何
                    self.ui.icons.clear();
                    // 图片预览位图绑定旧设备，随渲染目标一起失效重建
                    self.win.image_bitmap = None;
                    // 重建渲染目标并重新预初始化
                    let _ = self.init_render_target();
                    if let Some(rt) = self.win.render_ctx.target_ref() {
                        let target = rt.target().clone();
                        let common_colors = [
                            self.win.theme.editor_bg,
                            self.win.theme.line_number_bg,
                            self.win.theme.line_number_fg,
                            self.win.theme.line_highlight_bg,
                            self.win.theme.selection_bg,
                            self.win.theme.cursor_color,
                            self.win.theme.sidebar_bg,
                            self.win.theme.statusbar_bg,
                            self.win.theme.text_default,
                            self.win.theme.tab_active_bg,
                            self.win.theme.tab_inactive_bg,
                            self.win.theme.titlebar_bg,
                            self.win.theme.activity_bar_bg,
                            self.win.theme.panel_border,
                            self.win.theme.shadow,
                            self.win.theme.glow_selection,
                            self.win.theme.command_palette_bg,
                            self.win.theme.submenu_bg,
                        ];
                        self.win
                            .render_ctx
                            .brush_cache
                            .init_common_brushes(&target, &common_colors);
                        let font_size = self.win.text_renderer.font_size();
                        self.win
                            .render_ctx
                            .text_format_cache
                            .init_common_formats(font_size);
                        // 同步预构建图标几何，避免重建后首个文件帧承担懒加载成本
                        self.ui.icons.ensure_created_from_target(&target);
                    }
                }
            }
        }

        // 更新上一帧状态追踪
        self.snapshot_prev_frame_state();

        // 清除脏矩形标记（渲染完成）；失败帧保留全窗口脏标记并立即重绘恢复
        if end_draw_ok {
            self.win.end_draw_fail_streak = 0;
            self.win.dirty_tracker.clear();
        } else {
            self.recover_from_end_draw_failure();
        }

        // TEST: 将本帧命中区域写入文件
        crate::hit_test::flush_hit_regions_to_file();
    }

    /// 快照本帧状态到 `prev`，供下一帧变化检测使用。
    fn snapshot_prev_frame_state(&mut self) {
        self.input.prev.cursor_line = self.editor.content.cursor_line;
        self.input.prev.cursor_col = self.editor.content.cursor_col;
        self.input.prev.scroll_y = self.editor.content.scroll_y;
        self.input.prev.selection_start = self.editor.content.selection_start;
        self.input.prev.selection_end = self.editor.content.selection_end;
        self.input.prev.sidebar_content = self.ui.sidebar_content.clone();
        self.input.prev.sidebar_visible = self.ui.layout.sidebar_visible;
        self.input.prev.activity_bar_visible = self.ui.layout.activity_bar_visible;
        self.input.prev.right_panel_visible = self.ui.layout.right_panel_visible;
        self.input.prev.bottom_panel_visible = self.ui.layout.bottom_panel_visible;
        self.input
            .prev
            .status_message
            .clone_from(&self.ui.status_message);
        self.input.prev.active_tab = self.editor.tab_bar.active_tab;
    }

    /// EndDraw 失败后的恢复：标记全窗口并立即请求重绘。
    ///
    /// 连续失败超过上限时停止自动重试（保留脏标记，交给下一次用户交互/
    /// 系统 WM_PAINT 驱动），避免设备持续丢失时 invalidate→重绘→再失败
    /// 的忙循环空转 CPU。
    fn recover_from_end_draw_failure(&mut self) {
        const MAX_AUTO_RETRY: u8 = 3;
        self.win.dirty_tracker.mark_full_window();
        self.win.end_draw_fail_streak = self.win.end_draw_fail_streak.saturating_add(1);
        if self.win.end_draw_fail_streak <= MAX_AUTO_RETRY {
            crate::window::invalidate_window(self.win.hwnd);
        } else {
            tracing::warn!(
                streak = self.win.end_draw_fail_streak,
                "EndDraw 连续失败，暂停自动重试以避免重绘忙循环"
            );
        }
    }

    /// 统一同步 IME 合成窗口/候选窗口位置到当前聚焦的输入目标（每帧渲染末尾调用）。
    ///
    /// 旧实现仅在 `render_editor` 内设置位置，智能体模式（AI 输入框在中间列、
    /// 编辑器未渲染）或浏览器/终端标签激活时 IME 窗口会停留在默认位置，
    /// 不跟随输入框。此处按聚焦优先级统一计算锚点：
    /// 终端 > 文件树输入 > 浏览器地址栏 > AI 输入框 > 编辑器光标。
    fn sync_ime_position(&mut self) {
        let weight = DWRITE_FONT_WEIGHT_NORMAL.0 as u32;

        // 终端聚焦：锚到终端光标（智能体模式终端在右面板标签，经典模式在底部面板）
        if self.terminal.terminal_panel.focused {
            let term_region = self.terminal_view_region(&self.ui.layout);
            let (t_row, t_col) = self.terminal.terminal_panel.cursor_position();
            let cell_w = self
                .win
                .render_ctx
                .text_format_cache
                .measure_text_width("M", 11.0, weight)
                .unwrap_or(7.0);
            let prefix_x = if let Some(line) = self.terminal.terminal_panel.output_lines.get(t_row)
            {
                // t_col 是 cell 制（中文占 2），用 split_line_at_cell 换算前缀，
                // 与光标方块绘制共用同一换算源
                let (byte_len, utf16_len, extra_cells) =
                    crate::terminal::TerminalPanel::split_line_at_cell(line, t_col);
                let prefix = &line[..byte_len];
                let base = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .text_position_x(prefix, utf16_len, 11.0, weight)
                    .unwrap_or(t_col as f32 * cell_w);
                base + extra_cells as f32 * cell_w
            } else {
                t_col as f32 * cell_w
            };
            // 几何与 render_bottom_panel 严格一致：内容区顶部 = 内层标签栏 + 间距，
            // 行位置必须按可见窗口起始行换算（t_row 是绝对行号，输出超出可视行数时
            // 直接乘行高会把 IME 窗口锚到屏幕外）
            let line_h = TERMINAL_LINE_H;
            let content_top = term_region.y + TERMINAL_CONTENT_TOP;
            let content_bottom = term_region.y + term_region.height - 6.0;
            let visible_lines = ((content_bottom - content_top) / line_h).floor().max(1.0) as usize;
            let total_lines = self.terminal.terminal_panel.output_lines.len();
            let scroll_off = self.terminal.terminal_panel.scroll_offset;
            let end_line = total_lines.saturating_sub(scroll_off);
            let start_line = end_line.saturating_sub(visible_lines);
            if t_row >= start_line && t_row < end_line {
                let display_row = t_row - start_line;
                let tx = term_region.x + TERMINAL_TEXT_LEFT + prefix_x;
                let ty = content_top + display_row as f32 * line_h;
                let (comp_x, comp_y) = self.client_to_screen(tx, ty);
                let (cand_x, cand_y) = self.client_to_screen(tx, ty + line_h);
                self.ui.ime.set_composition_window_position(comp_x, comp_y);
                self.ui.ime.set_candidate_window_position(cand_x, cand_y);
            }
            return;
        }

        // 文件树行内输入：锚到树内输入行（几何与渲染共用 file_tree_input_row_geom）
        if self.fs.file_tree_input.is_some() {
            let sidebar = self.ui.layout.sidebar_region();
            if let Some((top_rel, _, text_left_rel)) = self.file_tree_input_row_geom() {
                let s = self.win.dpi_scale;
                let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * s;
                // 估算 value 宽度（近似，IME 候选窗口只需大致位置）
                let value_chars = self
                    .fs
                    .file_tree_input
                    .as_ref()
                    .map(|i| i.value.chars().count())
                    .unwrap_or(0);
                let ft_x = sidebar.x + text_left_rel + value_chars as f32 * 6.0 * s;
                let (cand_x, cand_y) = self.client_to_screen(ft_x, sidebar.y + top_rel + row_h);
                self.ui.ime.set_candidate_window_position(cand_x, cand_y);
            }
            return;
        }

        // 浏览器地址栏编辑中：锚到地址栏文本末尾
        if let (Some(id), Some(tl)) = (
            self.active_browser_id(),
            self.browser.toolbar_layout.clone(),
        ) {
            let editing_text = self.browser.get(id).and_then(|inst| {
                if inst.address_editing {
                    Some(inst.address_text.clone())
                } else {
                    None
                }
            });
            if let Some(text) = editing_text {
                let tw = self
                    .win
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(&text, 12.0, weight)
                    .unwrap_or(0.0);
                let cx = tl.address.x + 12.0 + tw;
                let (comp_x, comp_y) = self.client_to_screen(cx, tl.address.y);
                let (cand_x, cand_y) = self.client_to_screen(cx, tl.address.y + tl.address.height);
                self.ui.ime.set_composition_window_position(comp_x, comp_y);
                self.ui.ime.set_candidate_window_position(cand_x, cand_y);
                return;
            }
        }

        // 新标签页快捷搜索框聚焦：锚到搜索框文本末尾（几何与渲染共用 new_tab_page_geom_in）
        if self.browser.empty_search_focused && self.ntp_active() {
            let box_region = new_tab_page_geom_in(self.new_tab_page_region(&self.ui.layout));
            let display = format!(
                "{}{}",
                self.browser.empty_search_text,
                self.browser
                    .empty_search_composition
                    .as_deref()
                    .unwrap_or("")
            );
            let tw = self
                .win
                .render_ctx
                .text_format_cache
                .measure_text_width(&display, 13.0, weight)
                .unwrap_or(0.0);
            let cx = box_region.x + 36.0 + tw;
            let (comp_x, comp_y) = self.client_to_screen(cx, box_region.y);
            let (cand_x, cand_y) =
                self.client_to_screen(cx, box_region.y + box_region.height + 2.0);
            self.ui.ime.set_composition_window_position(comp_x, comp_y);
            self.ui.ime.set_candidate_window_position(cand_x, cand_y);
            return;
        }

        // AI 面板输入框聚焦：锚到输入框光标（智能体模式中间列，经典模式右面板）
        if self.ai.ai_panel.input_focused {
            let region = if self.editor_mode.is_agent() {
                self.ui.layout.editor_content_region(false)
            } else {
                self.ui.layout.right_panel_region()
            };
            // 几何与 render/ai.rs 输入框渲染保持一致
            let margin = 10.0f32;
            let input_margin = 8.0f32;
            let text_input_h = self.ai.ai_panel.input_computed_height.max(36.0);
            let input_area_h = self.ai.ai_panel.input_area_height();
            let input_y = region.y + region.height - input_area_h;
            let text_input_y = input_y + 6.0;
            let text_left = region.x + margin + input_margin + 4.0;
            let caret_prefix = if self.ai.ai_panel.caret_pos <= self.ai.ai_panel.input.len() {
                self.ai.ai_panel.input[..self.ai.ai_panel.caret_pos].to_string()
            } else {
                self.ai.ai_panel.input.clone()
            };
            let tw = self
                .win
                .render_ctx
                .text_format_cache
                .measure_text_width(&caret_prefix, 11.0, weight)
                .unwrap_or(0.0);
            // IME 组合中：光标位于预编辑串之后（与 ai.rs 光标渲染一致）
            let comp_w = if let Some(c) = self
                .ai
                .ai_panel
                .composition
                .as_ref()
                .filter(|c| !c.is_empty())
            {
                self.win
                    .render_ctx
                    .text_format_cache
                    .measure_text_width(c, 11.0, weight)
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let cx = text_left + tw + comp_w;
            let (comp_x, comp_y) = self.client_to_screen(cx, text_input_y);
            let (cand_x, cand_y) = self.client_to_screen(cx, text_input_y + text_input_h);
            self.ui.ime.set_composition_window_position(comp_x, comp_y);
            self.ui.ime.set_candidate_window_position(cand_x, cand_y);
            return;
        }

        // 默认：编辑器光标（区域按模式区分：智能体模式编辑器在右面板内容区）
        let line_height = self.win.text_renderer.line_height();
        let char_width = self.win.text_renderer.char_width();
        let line_number_width = 40.0;
        let (ex, ey) = if self.editor_mode.is_agent() {
            let rp = self.ui.layout.right_panel_region();
            let tab_h = if self.editor.tab_bar.tabs.is_empty() {
                0.0
            } else {
                crate::layout::TAB_BAR_HEIGHT
            };
            (rp.x, rp.y + tab_h)
        } else {
            let r = self.ui.layout.editor_content_region(self.show_tab_bar());
            (r.x, r.y)
        };
        let (start_line, _) = self.visible_line_range();
        let cursor_char_col = if let Some(text) = self
            .editor
            .content
            .cached_line(self.editor.content.cursor_line)
        {
            let byte_pos = text.floor_char_boundary(self.editor.content.cursor_col.min(text.len()));
            text[..byte_pos]
                .chars()
                .map(unicode_char_width)
                .sum::<usize>()
        } else {
            0
        };
        let cursor_x = ex + line_number_width + 5.0 - self.editor.content.scroll_x
            + cursor_char_col as f32 * char_width;
        let cursor_y = ey
            + (self.editor.content.cursor_line.saturating_sub(start_line)) as f32 * line_height
            - (self.editor.content.scroll_y % line_height);
        let (comp_x, comp_y) = self.client_to_screen(cursor_x, cursor_y);
        let (cand_x, cand_y) = self.client_to_screen(cursor_x, cursor_y + line_height);
        self.ui.ime.set_composition_window_position(comp_x, comp_y);
        self.ui.ime.set_candidate_window_position(cand_x, cand_y);
    }
}

mod account;
mod agent_right_panel;
pub(crate) use agent_right_panel::{
    new_tab_page_geom_in, NEW_TAB_BTN_COUNT, NEW_TAB_BTN_GAP, NEW_TAB_BTN_HEIGHT, NEW_TAB_BTN_WIDTH,
};
mod agent_sidebar;
mod ai;
mod ai_history_window;
mod chrome;
mod dialogs;
mod editor_view;
mod find;
mod markdown_preview;
mod menus;
mod remote;
mod remote_dialogs;
mod sandbox_eval;
mod settings_ai;
mod settings_general;
mod settings_models;
mod settings_update;
mod sidebar;
mod sidebar_files;
mod sidebar_scm;
mod tabs;
mod terminal;
pub(crate) use terminal::{TERMINAL_CONTENT_TOP, TERMINAL_LINE_H, TERMINAL_TEXT_LEFT};
