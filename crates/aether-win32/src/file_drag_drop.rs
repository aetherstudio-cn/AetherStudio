//! 资源管理器文件树拖拽：
//! - 侧边栏内：把文件/文件夹拖到目标文件夹（或根目录行）完成移动；
//! - 拖出侧边栏：转交系统 OLE 拖放（DoDragDrop），同时提供
//!   CF_HDROP（Explorer 等收文件对象）与 CF_UNICODETEXT（输入框收绝对路径文本）。
//!
//! 状态管理参考标签拖拽模式（Task 8）：按下记录候选节点 → 移动超过阈值
//! 进入拖拽模式 → 拖拽中实时更新放置目标（高亮反馈）→ 释放执行移动或取消。
//! 按下候选与"是否已进入拖拽"记录在 `MousePressState`
//! （`file_tree_drag_node` / `file_tree_dragging`），放置目标与浮标绘制
//! 信息集中在本模块的 `FileDragDropState`。

use std::path::PathBuf;

use windows::core::{implement, Result as WinResult, HRESULT};
use windows::Win32::Foundation::{
    BOOL, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC,
    E_NOTIMPL, OLE_E_ADVISENOTSUPPORTED, POINT, S_OK,
};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY,
    DROPEFFECT_LINK, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;

use crate::editor::{file_tree_node_path, EditorState, FileKind};

/// 进入拖拽模式的位移阈值（逻辑像素，与标签拖拽的 3px 阈值同量级）
pub const DRAG_THRESHOLD: f32 = 4.0;

/// 拖拽放置目标
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// 放入某个目录节点
    Directory(u32),
    /// 放入工作区根目录（根目录行或树下方空白区域）
    Root,
}

/// 文件树拖拽的进行中状态（放置目标 + 浮标绘制信息）
#[derive(Default)]
pub struct FileDragDropState {
    /// 按下位置（窗口逻辑像素，用于阈值判定）
    pub press_x: f32,
    pub press_y: f32,
    /// 当前鼠标位置（窗口逻辑像素，用于绘制拖拽浮标）
    pub cur_x: f32,
    pub cur_y: f32,
    /// 当前放置目标（拖拽中实时更新；None 表示无效目标/不可放置）
    pub drop_target: Option<DropTarget>,
    /// 拖拽浮标显示的名称（按下时缓存，避免拖拽中反复取名）
    pub drag_label: String,
    /// 浮标文本宽度缓存（进入拖拽时测量一次，避免每帧 DirectWrite 测量）
    pub drag_label_width: f32,
    /// 上次标脏重绘时的浮标位置（微动过滤：亚像素抖动不触发重绘）
    pub last_paint_x: f32,
    pub last_paint_y: f32,
    /// 上次强制同步重绘时刻（UpdateWindow 节流至 ~120fps，
    /// 避免高回报率鼠标的消息洪流压垮渲染管线产生卡顿感）
    pub last_sync_paint: Option<std::time::Instant>,
}

impl EditorState {
    /// 文件树节点按下：记录拖拽候选（超过阈值后才进入拖拽模式）。
    /// 在 `lbd_sidebar` 命中文件/目录节点名称区域时调用。
    pub fn file_drag_begin_press(&mut self, node_idx: u32, mouse_x: f32, mouse_y: f32) {
        let label = self
    .fs.file_tree
            .as_ref()
            .and_then(|t| t.get_node(node_idx).map(|n| t.get_name(n).to_string()))
            .unwrap_or_default();
        self.input.mouse_press.file_tree_drag_node = Some(node_idx);
        self.input.mouse_press.file_tree_dragging = false;
        self.fs.file_drag = FileDragDropState {
            press_x: mouse_x,
            press_y: mouse_y,
            cur_x: mouse_x,
            cur_y: mouse_y,
            drop_target: None,
            drag_label: label,
            drag_label_width: 0.0,
            last_paint_x: mouse_x,
            last_paint_y: mouse_y,
            last_sync_paint: None,
        };
    }

    /// WM_MOUSEMOVE 中调用：阈值判定 + 放置目标/浮标位置更新。
    /// 返回 true 表示视觉状态有变化（需要重绘侧边栏）。
    pub fn file_drag_update(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        let Some(source_idx) = self.input.mouse_press.file_tree_drag_node else {
            return false;
        };
        if !self.input.mouse_press.file_tree_dragging {
            let dx = mouse_x - self.fs.file_drag.press_x;
            let dy = mouse_y - self.fs.file_drag.press_y;
            if dx * dx + dy * dy < DRAG_THRESHOLD * DRAG_THRESHOLD {
                return false;
            }
            self.input.mouse_press.file_tree_dragging = true;
            // 浮标文本宽度只在进入拖拽时测量一次（每帧测量是无谓的 CPU 开销）
            let label = self.fs.file_drag.drag_label.clone();
            self.fs.file_drag.drag_label_width = self
    .win.render_ctx
                .text_format_cache
                .measure_text_width(
                    &label,
                    9.5 * self.win.dpi_scale,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL.0 as u32,
                )
                .unwrap_or(0.0);
            // 捕获鼠标：拖出窗口仍能收到 WM_MOUSEMOVE / WM_LBUTTONUP，
            // 避免窗口外释放导致拖拽状态残留
            unsafe {
                SetCapture(self.win.hwnd);
            }
        }
        self.fs.file_drag.cur_x = mouse_x;
        self.fs.file_drag.cur_y = mouse_y;
        let new_target = self.file_drag_compute_target(source_idx, mouse_x, mouse_y);
        let target_changed = new_target != self.fs.file_drag.drop_target;
        self.fs.file_drag.drop_target = new_target;
        // 拖拽期间清除普通悬停，避免 hover 高亮与放置目标高亮叠加
        self.fs.hover_file_node = None;
        self.fs.hover_file_tree_root = false;
        // 微动过滤：目标未变且位移不足 1px 时跳过重绘，
        // 避免高回报率鼠标的亚像素抖动频繁刷帧
        let pdx = mouse_x - self.fs.file_drag.last_paint_x;
        let pdy = mouse_y - self.fs.file_drag.last_paint_y;
        if !target_changed && pdx * pdx + pdy * pdy < 1.0 {
            return false;
        }
        self.fs.file_drag.last_paint_x = mouse_x;
        self.fs.file_drag.last_paint_y = mouse_y;
        // 浮标/高亮跟随鼠标：整个侧边栏标脏（区域小，代价可接受）
        let sidebar = self.ui.layout.sidebar_region();
        self.win.dirty_tracker.mark_region(
            sidebar.x,
            sidebar.y,
            sidebar.width,
            sidebar.height,
            crate::dirty_rect::DirtyRegionType::Sidebar,
        );
        true
    }

    /// 拖拽中是否允许本次强制同步重绘（UpdateWindow 节流至约 120fps）。
    /// InvalidateRect 仍每次调用，未获得同步重绘的帧由后续 WM_PAINT 合并，
    /// 保证最终位置不丢帧、消息洪流不压垮渲染管线。
    pub fn file_drag_should_sync_paint(&mut self) -> bool {
        const MIN_FRAME: std::time::Duration = std::time::Duration::from_millis(8);
        let now = std::time::Instant::now();
        match self.fs.file_drag.last_sync_paint {
            Some(t) if now.duration_since(t) < MIN_FRAME => false,
            _ => {
                self.fs.file_drag.last_sync_paint = Some(now);
                true
            }
        }
    }

    /// WM_LBUTTONUP 中调用：拖拽中则以释放位置执行移动，否则仅清理候选。
    /// 返回 true 表示本次释放属于拖拽（调用方需要重绘）。
    pub fn file_drag_finish(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        let source = self.input.mouse_press.file_tree_drag_node.take();
        let was_dragging = std::mem::take(&mut self.input.mouse_press.file_tree_dragging);
        if !was_dragging {
            self.fs.file_drag.drop_target = None;
            return false;
        }
        unsafe {
            let _ = ReleaseCapture();
        }
        // 以释放位置重算目标（比 mouse_move 缓存的更准确）
        let target = source.and_then(|src| self.file_drag_compute_target(src, mouse_x, mouse_y));
        self.fs.file_drag.drop_target = None;
        self.fs.file_drag.drag_label.clear();
        match (source, target) {
            (Some(src), Some(t)) => self.file_drag_perform_move(src, t),
            _ => self.ui.status_message = "已取消移动".to_string(),
        }
        let sidebar = self.ui.layout.sidebar_region();
        self.win.dirty_tracker.mark_region(
            sidebar.x,
            sidebar.y,
            sidebar.width,
            sidebar.height,
            crate::dirty_rect::DirtyRegionType::Sidebar,
        );
        true
    }

    /// 计算鼠标位置对应的放置目标（窗口逻辑像素坐标）。
    /// 命中目录 → 该目录；命中文件 → 其父目录；根目录行/树下方空白 → 工作区根。
    /// 非法目标（自身/自身子孙/原父目录）返回 None。
    fn file_drag_compute_target(
        &mut self,
        source_idx: u32,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<DropTarget> {
        let sidebar = self.ui.layout.sidebar_region();
        if !sidebar.contains(mouse_x, mouse_y) {
            return None;
        }
        let rel_x = mouse_x - sidebar.x;
        let rel_y = mouse_y - sidebar.y;

        // 根目录行：放入工作区根目录
        let root_top = self.file_tree_list_start_y();
        let row_h = crate::layout::FILE_TREE_ROW_HEIGHT * self.win.dpi_scale;
        let raw_target = if rel_y >= root_top && rel_y < root_top + row_h {
            DropTarget::Root
        } else if rel_y < root_top || !self.fs.file_tree_root_expanded {
            // 标题栏/输入框区域不作为放置目标；树折叠时同理
            return None;
        } else {
            let start_y = self.file_tree_nodes_start_y();
            let sidebar_width = self.ui.layout.sidebar_width;
            match self.file_tree_hit_test(rel_x, rel_y, start_y, sidebar_width) {
                Some((idx, FileKind::Directory, _)) => DropTarget::Directory(idx),
                Some((idx, _, _)) => {
                    // 命中文件/符号链接 → 目标为其父目录
                    let parent = self
    .fs.file_tree
                        .as_ref()
                        .and_then(|t| t.get_node(idx))
                        .map(|n| n.parent_idx)?;
                    if parent == u32::MAX {
                        DropTarget::Root
                    } else {
                        DropTarget::Directory(parent)
                    }
                }
                // 树下方空白区域 → 工作区根目录
                None => DropTarget::Root,
            }
        };
        self.file_drag_validate_target(source_idx, raw_target)
            .then_some(raw_target)
    }

    /// 校验放置目标合法性：不能放入自身、自身子孙目录或原父目录（无操作）。
    fn file_drag_validate_target(&self, source_idx: u32, target: DropTarget) -> bool {
        let Some(tree) = self.fs.file_tree.as_ref() else {
            return false;
        };
        let Some(source) = tree.get_node(source_idx) else {
            return false;
        };
        match target {
            DropTarget::Root => source.parent_idx != u32::MAX,
            DropTarget::Directory(dir_idx) => {
                if source.parent_idx == dir_idx {
                    return false;
                }
                // 沿 parent 链上溯：目标目录是自身或自身子孙 → 非法
                let mut cur = dir_idx;
                loop {
                    if cur == source_idx {
                        return false;
                    }
                    match tree.get_node(cur) {
                        Some(n) if n.parent_idx != u32::MAX => cur = n.parent_idx,
                        _ => return true,
                    }
                }
            }
        }
    }

    /// 节点的绝对路径（工作区根 + 相对路径）
    fn file_drag_node_abs_path(&self, node_idx: u32) -> Option<PathBuf> {
        let folder = self.fs.current_folder.as_ref()?;
        let tree = self.fs.file_tree.as_ref()?;
        let rel = file_tree_node_path(tree, node_idx)?;
        Some(folder.join(rel))
    }

    /// 执行移动：fs::rename + 打开标签页路径同步 + 轻量刷新文件树。
    fn file_drag_perform_move(&mut self, source_idx: u32, target: DropTarget) {
        let Some(source_path) = self.file_drag_node_abs_path(source_idx) else {
            return;
        };
        let target_dir = match target {
            DropTarget::Root => match self.fs.current_folder.clone() {
                Some(p) => p,
                None => return,
            },
            DropTarget::Directory(dir_idx) => match self.file_drag_node_abs_path(dir_idx) {
                Some(p) => p,
                None => return,
            },
        };
        let Some(file_name) = source_path.file_name().map(|n| n.to_os_string()) else {
            return;
        };
        let dest_path = target_dir.join(&file_name);
        if dest_path.exists() {
            self.ui.status_message = format!("目标位置已存在: {}", file_name.to_string_lossy());
            return;
        }
        if let Err(e) = std::fs::rename(&source_path, &dest_path) {
            self.ui.status_message = format!("移动失败: {}", e);
            return;
        }

        // 同步已打开标签页的文件路径（文件精确匹配；目录移动做前缀替换）
        let remap = |path: &PathBuf| -> Option<PathBuf> {
            if path == &source_path {
                Some(dest_path.clone())
            } else {
                path.strip_prefix(&source_path)
                    .ok()
                    .map(|rest| dest_path.join(rest))
            }
        };
        for tab in &mut self.editor.tab_bar.tabs {
            if let Some(file_content) = tab.as_file_mut() {
                if let Some(new_path) = file_content.file_path.as_ref().and_then(remap) {
                    file_content.file_path = Some(new_path);
                }
            }
        }
        if let Some(new_path) = self.editor.content.file_path.as_ref().and_then(remap) {
            self.editor.content.file_path = Some(new_path);
        }

        // 展开目标目录，让用户在刷新后立即看到移动结果
        //（refresh_file_tree_light 依据旧树的展开状态重建）
        if let DropTarget::Directory(dir_idx) = target {
            if let Some(tree) = self.fs.file_tree.as_mut() {
                if let Some(node) = tree.get_node_mut(dir_idx) {
                    node.is_expanded = true;
                }
            }
        }
        // 移动后旧节点索引全部失效
        self.fs.selected_file_node = None;
        self.fs.hover_file_node = None;
        let target_name = match target {
            DropTarget::Root => "工作区根目录".to_string(),
            DropTarget::Directory(dir_idx) => self
    .fs.file_tree
                .as_ref()
                .and_then(|t| t.get_node(dir_idx).map(|n| t.get_name(n).to_string()))
                .unwrap_or_else(|| "目标文件夹".to_string()),
        };
        self.ui.status_message = format!("已移动 {} 到 {}", file_name.to_string_lossy(), target_name);
        self.refresh_file_tree_light();
    }

    /// 拖拽中源节点的绝对路径（供外部 OLE 拖放使用）
    pub fn file_drag_external_source_path(&self) -> Option<PathBuf> {
        self.input.mouse_press
            .file_tree_drag_node
            .and_then(|idx| self.file_drag_node_abs_path(idx))
    }

    /// 中止内部拖拽（转交外部 OLE 拖放前调用）：清状态 + 释放捕获 + 标脏。
    /// DoDragDrop 会自己捕获鼠标并运行模态消息循环，内部状态必须先清空，
    /// 避免重入的 WM_MOUSEMOVE/WM_LBUTTONUP 再走内部拖拽分支。
    pub fn file_drag_abort_internal(&mut self) {
        self.input.mouse_press.file_tree_drag_node = None;
        if std::mem::take(&mut self.input.mouse_press.file_tree_dragging) {
            unsafe {
                let _ = ReleaseCapture();
            }
        }
        self.fs.file_drag.drop_target = None;
        self.fs.file_drag.drag_label.clear();
        let sidebar = self.ui.layout.sidebar_region();
        self.win.dirty_tracker.mark_region(
            sidebar.x,
            sidebar.y,
            sidebar.width,
            sidebar.height,
            crate::dirty_rect::DirtyRegionType::Sidebar,
        );
    }
}

// ============ 外部 OLE 拖放（拖出应用：Explorer / 聊天框 / 输入框等） ============

/// 剪贴板格式常量（与 editing.rs 的 CF_UNICODETEXT 同源：WinUser.h 标准值）
const CF_UNICODETEXT_FMT: u16 = 13;
const CF_HDROP_FMT: u16 = 15;

/// DROPFILES 头（shellapi.h 布局）：后跟双空结尾的宽字符路径列表
#[repr(C)]
struct DropFilesHeader {
    p_files: u32,
    pt: POINT,
    f_nc: i32,
    f_wide: i32,
}

/// 构造 CF_HDROP 的 HGLOBAL：DROPFILES 头 + 双空结尾宽字符路径列表。
/// 所有权交给接收方（STGMEDIUM 释放）。
unsafe fn build_hdrop_hglobal(paths: &[PathBuf]) -> WinResult<windows::Win32::Foundation::HGLOBAL> {
    let mut wide: Vec<u16> = Vec::new();
    for p in paths {
        wide.extend(p.as_os_str().to_string_lossy().encode_utf16());
        wide.push(0);
    }
    wide.push(0); // 列表终止符（双空）
    let header_size = std::mem::size_of::<DropFilesHeader>();
    let total = header_size + wide.len() * 2;
    let hglobal = GlobalAlloc(GMEM_MOVEABLE, total)?;
    let ptr = GlobalLock(hglobal);
    if ptr.is_null() {
        let _ = GlobalUnlock(hglobal);
        return Err(windows::core::Error::from_win32());
    }
    let header = DropFilesHeader {
        p_files: header_size as u32,
        pt: POINT::default(),
        f_nc: 0,
        f_wide: 1,
    };
    std::ptr::copy_nonoverlapping(
        &header as *const _ as *const u8,
        ptr as *mut u8,
        header_size,
    );
    std::ptr::copy_nonoverlapping(
        wide.as_ptr() as *const u8,
        (ptr as *mut u8).add(header_size),
        wide.len() * 2,
    );
    let _ = GlobalUnlock(hglobal);
    Ok(hglobal)
}

/// 构造 CF_UNICODETEXT 的 HGLOBAL（空结尾宽字符串）
unsafe fn build_unicode_text_hglobal(text: &str) -> WinResult<windows::Win32::Foundation::HGLOBAL> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let hglobal = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
    let ptr = GlobalLock(hglobal);
    if ptr.is_null() {
        let _ = GlobalUnlock(hglobal);
        return Err(windows::core::Error::from_win32());
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, wide.len() * 2);
    let _ = GlobalUnlock(hglobal);
    Ok(hglobal)
}

fn hglobal_formatetc(cf: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: cf,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// OLE 拖放数据对象：同时提供 CF_HDROP（文件对象）与 CF_UNICODETEXT（绝对路径文本）
#[implement(IDataObject)]
struct FileDropDataObject {
    paths: Vec<PathBuf>,
}

impl FileDropDataObject {
    fn supports(&self, fmt: &FORMATETC) -> bool {
        (fmt.cfFormat == CF_HDROP_FMT || fmt.cfFormat == CF_UNICODETEXT_FMT)
            && fmt.dwAspect == DVASPECT_CONTENT.0
            && (fmt.tymed & TYMED_HGLOBAL.0 as u32) != 0
    }
}

impl IDataObject_Impl for FileDropDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> WinResult<STGMEDIUM> {
        let fmt = unsafe { &*pformatetcin };
        if !self.supports(fmt) {
            return Err(DV_E_FORMATETC.into());
        }
        let hglobal = unsafe {
            match fmt.cfFormat {
                CF_HDROP_FMT => build_hdrop_hglobal(&self.paths)?,
                _ => {
                    // 多个路径以换行分隔（当前单选拖拽，预留多选扩展）
                    let joined = self
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    build_unicode_text_hglobal(&joined)?
                }
            }
        };
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: hglobal },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *mut STGMEDIUM,
    ) -> WinResult<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        let fmt = unsafe { &*pformatetc };
        if self.supports(fmt) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        unsafe {
            (*pformatetcout).ptd = std::ptr::null_mut();
        }
        E_NOTIMPL
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: BOOL,
    ) -> WinResult<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> WinResult<IEnumFORMATETC> {
        if dwdirection == DATADIR_GET.0 as u32 {
            let formats = [
                hglobal_formatetc(CF_HDROP_FMT),
                hglobal_formatetc(CF_UNICODETEXT_FMT),
            ];
            unsafe { SHCreateStdEnumFmtEtc(&formats) }
        } else {
            Err(E_NOTIMPL.into())
        }
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Option<&IAdviseSink>,
    ) -> WinResult<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> WinResult<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> WinResult<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

/// OLE 拖放源：Esc 取消，松开左键落盘，光标用系统默认反馈
#[implement(IDropSource)]
struct FileDropSource;

impl IDropSource_Impl for FileDropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if (grfkeystate & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// 启动系统级 OLE 拖放（阻塞：内部运行模态消息循环直到落盘/取消）。
/// 调用前必须先 `file_drag_abort_internal` 并释放 EditorState 的 RefCell 借用，
/// 否则模态循环重入窗口过程时会双重可变借用 panic。
/// 返回目标接受的效果（None 表示取消）。
pub fn start_ole_file_drag(paths: Vec<PathBuf>) -> Option<DROPEFFECT> {
    unsafe {
        // 幂等：已初始化时返回 S_FALSE，不影响使用
        let _ = OleInitialize(None);
        let data: IDataObject = FileDropDataObject { paths }.into();
        let source: IDropSource = FileDropSource.into();
        let mut effect = DROPEFFECT_NONE;
        let hr = DoDragDrop(
            &data,
            &source,
            DROPEFFECT_COPY | DROPEFFECT_LINK,
            &mut effect,
        );
        if hr == DRAGDROP_S_DROP {
            Some(effect)
        } else {
            None
        }
    }
}
