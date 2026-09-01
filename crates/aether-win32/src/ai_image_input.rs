//! AI 输入框图片输入：拖放（WM_DROPFILES 落点在 AI 面板）与 Ctrl+V 粘贴
//! （剪贴板位图 / 图片文件路径）统一附加到 `pending_images`，随下一条
//! 用户消息以 OpenAI 兼容 `image_url` base64 块发送。
//!
//! 与图片按钮（content_area.rs 文件对话框）共用同一条数据通路：
//! `std::fs::read` → `ChatImage::from_bytes`（魔数检测 + 32 MiB 上限）
//! → `AiPanel::add_pending_image`。多模态门禁在发送时统一校验，此处不做。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::editor::EditorState;

/// 当前活动 AI 面板区域（逻辑像素）：智能体模式在中间列，经典模式在右面板。
/// 右面板隐藏时返回 None（此时 AI 面板不可见，拖放不拦截）。
pub(crate) fn ai_panel_region(st: &EditorState) -> Option<crate::layout::Region> {
    if st.editor_mode.is_agent() {
        Some(st.ui.layout.editor_content_region(false))
    } else if st.ui.layout.right_panel_visible {
        Some(st.ui.layout.right_panel_region())
    } else {
        None
    }
}

/// 按扩展名粗筛图片文件（内容仍由 ChatImage::from_bytes 魔数校验兜底）。
fn is_image_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
            )
        })
        .unwrap_or(false)
}

/// 把图片字节附加到待发送列表。返回 Ok(文件名) 或 Err(状态栏提示)。
fn attach_image_bytes(
    st: &mut EditorState,
    filename: String,
    bytes: &[u8],
) -> Result<String, String> {
    match aether_ai::ChatImage::from_bytes(bytes) {
        Some(img) => {
            st.ai
                .ai_panel
                .add_pending_image(crate::ai_panel::AiImageAttachment {
                    filename: filename.clone(),
                    mime: img.mime,
                    data_b64: img.data_b64,
                });
            Ok(filename)
        }
        None => Err(
            "无法附加图片：仅支持 JPEG/PNG/GIF/WebP（按文件内容识别），且单张不超过 32 MiB"
                .to_string(),
        ),
    }
}

/// 从文件路径附加图片（拖放 / 剪贴板文件路径共用）。
fn attach_image_path(st: &mut EditorState, path: &Path) -> Result<String, String> {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    match std::fs::read(path) {
        Ok(bytes) => attach_image_bytes(st, filename, &bytes),
        Err(e) => Err(format!("读取图片失败：{}", e)),
    }
}

/// WM_DROPFILES：落点在 AI 面板区域时，把图片文件附加到待发送列表。
///
/// 返回 true 表示已消费该文件（不再走"打开文件"路径）；非图片 / 不在
/// AI 面板区域时返回 false，由调用方维持原有打开行为。
pub(crate) fn try_attach_dropped_image(
    state: &Rc<RefCell<EditorState>>,
    drop_client_logical: (f32, f32),
    path: &Path,
) -> bool {
    let region = {
        let st = state.borrow();
        ai_panel_region(&st)
    };
    let Some(region) = region else {
        return false;
    };
    if !region.contains(drop_client_logical.0, drop_client_logical.1) {
        return false;
    }
    if !is_image_ext(path) {
        return false;
    }
    let mut st = state.borrow_mut();
    match attach_image_path(&mut st, path) {
        Ok(name) => {
            st.ui.status_message = format!("已附加图片 {}，将随下一条消息发送", name);
        }
        Err(msg) => {
            st.ui.status_message = msg;
        }
    }
    st.mark_ai_panel_dirty();
    true
}

/// 剪贴板图片粘贴结果
pub(crate) enum PasteImageOutcome {
    /// 已附加图片（位图或文件路径）
    Attached,
    /// 剪贴板无图片内容，调用方应回退到文本粘贴
    NoImage,
}

/// AI 输入框聚焦时的 Ctrl+V：优先尝试图片（位图 → PNG；图片文件路径 → 读文件），
/// 无图片内容时返回 NoImage 由调用方走原有文本粘贴。
pub(crate) fn try_paste_clipboard_image(state: &Rc<RefCell<EditorState>>) -> PasteImageOutcome {
    // 1) 剪贴板位图（截图 / 图片编辑器复制）
    if let Some(png) = clipboard_bitmap_as_png() {
        let mut st = state.borrow_mut();
        match attach_image_bytes(&mut st, "clipboard.png".to_string(), &png) {
            Ok(name) => {
                st.ui.status_message = format!("已附加剪贴板图片 {}，将随下一条消息发送", name);
            }
            Err(msg) => {
                st.ui.status_message = msg;
            }
        }
        st.mark_ai_panel_dirty();
        return PasteImageOutcome::Attached;
    }
    // 2) 剪贴板文件路径（Explorer 复制的图片文件）
    if let Some(paths) = clipboard_file_paths() {
        let mut attached = 0usize;
        let mut last_err: Option<String> = None;
        {
            let mut st = state.borrow_mut();
            for p in &paths {
                if p.is_file() && is_image_ext(p) {
                    match attach_image_path(&mut st, p) {
                        Ok(_) => attached += 1,
                        Err(msg) => last_err = Some(msg),
                    }
                }
            }
            if attached > 0 {
                st.ui.status_message = format!("已附加 {} 张图片，将随下一条消息发送", attached);
                st.mark_ai_panel_dirty();
            } else if let Some(msg) = last_err {
                st.ui.status_message = msg;
            }
        }
        if attached > 0 {
            return PasteImageOutcome::Attached;
        }
    }
    PasteImageOutcome::NoImage
}

/// 读取剪贴板位图并编码为 PNG 字节。支持 CF_DIB（截图工具常用）与
/// CF_BITMAP（GDI 位图句柄）两种格式；失败返回 None。
fn clipboard_bitmap_as_png() -> Option<Vec<u8>> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    const CF_DIB: u32 = 8;
    const CF_BITMAP: u32 = 2;
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            if IsClipboardFormatAvailable(CF_DIB).is_ok() {
                if let Ok(handle) = GetClipboardData(CF_DIB) {
                    if let Some(bytes) = read_global_bytes(handle.0) {
                        return dib_bytes_to_png(&bytes);
                    }
                }
            }
            if IsClipboardFormatAvailable(CF_BITMAP).is_ok() {
                if let Ok(handle) = GetClipboardData(CF_BITMAP) {
                    return gdi_bitmap_to_png(windows::Win32::Graphics::Gdi::HBITMAP(handle.0));
                }
            }
            None
        })();
        let _ = CloseClipboard();
        result
    }
}

/// 锁定 HGLOBAL 并拷贝其字节内容
unsafe fn read_global_bytes(hglobal: *mut std::ffi::c_void) -> Option<Vec<u8>> {
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    let hg = windows::Win32::Foundation::HGLOBAL(hglobal);
    let size = GlobalSize(hg);
    if size == 0 {
        return None;
    }
    let ptr = GlobalLock(hg);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
    let _ = GlobalUnlock(hg);
    Some(bytes)
}

/// 读取剪贴板 CF_HDROP 文件路径列表（Explorer 复制文件时提供）
fn clipboard_file_paths() -> Option<Vec<PathBuf>> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    const CF_HDROP: u32 = 15;
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            if IsClipboardFormatAvailable(CF_HDROP).is_err() {
                return None;
            }
            let handle = GetClipboardData(CF_HDROP).ok()?;
            let hdrop = HDROP(handle.0);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None);
                if len == 0 {
                    continue;
                }
                let mut buf = vec![0u16; (len + 1) as usize];
                let _ = DragQueryFileW(hdrop, i, Some(&mut buf));
                if let Ok(s) = String::from_utf16(&buf[..len as usize]) {
                    paths.push(PathBuf::from(s));
                }
            }
            if paths.is_empty() {
                None
            } else {
                Some(paths)
            }
        })();
        let _ = CloseClipboard();
        result
    }
}

/// CF_DIB（BITMAPINFOHEADER + 像素数据，无 BITMAPFILEHEADER）→ PNG
fn dib_bytes_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    // BITMAPINFOHEADER 至少 40 字节
    if dib.len() < 40 {
        return None;
    }
    let width = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let height_raw = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let planes = u16::from_le_bytes([dib[12], dib[13]]);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
    // 仅支持未压缩 24/32bpp（截图工具与多数应用输出的格式）
    if planes != 1 || compression != 0 || !(bpp == 24 || bpp == 32) {
        return None;
    }
    if width <= 0 || height_raw == 0 {
        return None;
    }
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let w = width as usize;
    let h = height as usize;
    let bytes_per_px = (bpp / 8) as usize;
    let row_stride = (w * bytes_per_px + 3) & !3; // DIB 行 4 字节对齐
    let header_size = 40usize;
    if dib.len() < header_size + row_stride.checked_mul(h)? {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        // 底朝上 DIB：第 0 行像素在数据末尾
        let src_row = if top_down { row } else { h - 1 - row };
        let src_off = header_size + src_row * row_stride;
        for col in 0..w {
            let s = src_off + col * bytes_per_px;
            let d = (row * w + col) * 4;
            rgba[d] = dib[s + 2]; // R
            rgba[d + 1] = dib[s + 1]; // G
            rgba[d + 2] = dib[s]; // B
            rgba[d + 3] = if bpp == 32 { dib[s + 3] } else { 255 };
        }
    }
    encode_rgba_png(&rgba, w as u32, h as u32)
}

/// CF_BITMAP（GDI 位图句柄）→ PNG
fn gdi_bitmap_to_png(hbitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{GetBitmapBits, GetObjectW, BITMAP};
    unsafe {
        let mut bmp = BITMAP::default();
        if GetObjectW(
            hbitmap,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut std::ffi::c_void),
        ) == 0
        {
            return None;
        }
        let (w, h) = (bmp.bmWidth, bmp.bmHeight);
        if w <= 0 || h <= 0 {
            return None;
        }
        let bpp = bmp.bmBitsPixel as usize;
        if bpp != 24 && bpp != 32 {
            return None;
        }
        let bytes_per_px = bpp / 8;
        let row_stride = (w as usize * bytes_per_px + 3) & !3;
        let mut raw = vec![0u8; row_stride * h as usize];
        if GetBitmapBits(
            hbitmap,
            raw.len() as i32,
            raw.as_mut_ptr() as *mut std::ffi::c_void,
        ) == 0
        {
            return None;
        }
        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        for row in 0..h as usize {
            for col in 0..w as usize {
                let s = row * row_stride + col * bytes_per_px;
                let d = (row * w as usize + col) * 4;
                rgba[d] = raw[s + 2];
                rgba[d + 1] = raw[s + 1];
                rgba[d + 2] = raw[s];
                rgba[d + 3] = if bpp == 32 { raw[s + 3] } else { 255 };
            }
        }
        encode_rgba_png(&rgba, w as u32, h as u32)
    }
}

/// RGBA 像素 → PNG 编码（image crate，已在依赖中）
fn encode_rgba_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    use image::{ImageBuffer, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba.to_vec())?;
    let mut out = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut out),
        image::ImageOutputFormat::Png,
    )
    .ok()?;
    Some(out)
}

/// WM_DROPFILES 落点（客户区物理像素）→ 逻辑像素。
/// 必须在持有 hdrop（DragFinish 之前）时调用。
pub(crate) fn drop_point_logical(
    hdrop: windows::Win32::UI::Shell::HDROP,
    dpi_scale: f32,
) -> (f32, f32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Shell::DragQueryPoint;
    let mut pt = POINT::default();
    unsafe {
        let _ = DragQueryPoint(hdrop, &mut pt);
    }
    let scale = if dpi_scale > 0.01 { dpi_scale } else { 1.0 };
    (pt.x as f32 / scale, pt.y as f32 / scale)
}
