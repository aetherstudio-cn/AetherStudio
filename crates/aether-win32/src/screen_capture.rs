//! 桌面/窗口截图捕获（GDI BitBlt，窗口级 PrintWindow 优先、失败回退屏幕拷贝）。
//!
//! 技术选型：截图频率低（Agent 工具/命令面板手动触发），GDI 足够且无需
//! D3D 设备；PNG 编码复用已在依赖中的 image crate（png feature），零新增 crate。
//! 捕获结果为 RGBA 像素，落盘到 `%TEMP%\Aether\shots\shot-<时间戳>.png`，
//! 仅本地保存、不主动上传；附图回喂走现有附件隐私提示约束。

use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// 截图结果：RGBA 像素（行主序、自上而下）
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 捕获整个虚拟屏幕（含多显示器，物理像素，要求进程 DPI 感知）。
pub fn capture_virtual_screen() -> Result<Capture, String> {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if w <= 0 || h <= 0 {
            return Err("无法获取屏幕尺寸".to_string());
        }
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("获取屏幕 DC 失败".to_string());
        }
        let result = capture_from_dc(screen_dc, x, y, w, h, None);
        ReleaseDC(None, screen_dc);
        result
    }
}

/// 捕获指定窗口：优先 PrintWindow（可捕获被遮挡窗口），失败或全黑时
/// 回退到从屏幕 DC 按窗口矩形拷贝。
pub fn capture_window(hwnd: HWND) -> Result<Capture, String> {
    unsafe {
        let mut rect = std::mem::zeroed::<windows::Win32::Foundation::RECT>();
        if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_err() {
            return Err("获取窗口矩形失败".to_string());
        }
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w <= 0 || h <= 0 {
            return Err("窗口尺寸无效（可能已最小化）".to_string());
        }
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("获取屏幕 DC 失败".to_string());
        }
        let result = capture_from_dc(screen_dc, rect.left, rect.top, w, h, Some(hwnd));
        ReleaseDC(None, screen_dc);
        result
    }
}

/// 从屏幕 DC 捕获指定区域；`print_hwnd` 非空时先尝试 PrintWindow。
///
/// # Safety
/// 调用者保证 `screen_dc` 有效；本函数内部完成所有 GDI 对象的创建与释放。
unsafe fn capture_from_dc(
    screen_dc: windows::Win32::Graphics::Gdi::HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    print_hwnd: Option<HWND>,
) -> Result<Capture, String> {
    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_invalid() {
        return Err("创建兼容 DC 失败".to_string());
    }
    let bmp = CreateCompatibleBitmap(screen_dc, w, h);
    if bmp.is_invalid() {
        let _ = DeleteDC(mem_dc);
        return Err("创建兼容位图失败".to_string());
    }
    let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));

    let mut blt_ok = false;
    if let Some(hwnd) = print_hwnd {
        // PW_RENDERFULLCONTENT = 2：捕获 DirectComposition/WebView2 等非客户区内容
        let flags = windows::Win32::Storage::Xps::PRINT_WINDOW_FLAGS(2);
        if windows::Win32::Storage::Xps::PrintWindow(hwnd, mem_dc, flags).as_bool() {
            blt_ok = true;
        }
    }
    if !blt_ok {
        blt_ok = BitBlt(mem_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY | CAPTUREBLT).is_ok();
    }
    if !blt_ok {
        SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        return Err("屏幕拷贝失败".to_string());
    }

    // PrintWindow 对部分窗口返回成功但内容全黑 → 回退屏幕拷贝
    if print_hwnd.is_some() {
        if let Ok(cap) = read_bitmap_pixels(screen_dc, bmp, mem_dc, old, w, h) {
            if cap.rgba.iter().any(|&b| b != 0) {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                let _ = DeleteDC(mem_dc);
                return Ok(cap);
            }
        }
        // 全黑或读取失败：重新 BitBlt（此时 bmp 已被 read_bitmap_pixels 选出）
        let old2 = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        let ok = BitBlt(mem_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY | CAPTUREBLT).is_ok();
        if !ok {
            SelectObject(mem_dc, old2);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            return Err("窗口截图回退失败".to_string());
        }
        let cap = read_bitmap_pixels(screen_dc, bmp, mem_dc, old2, w, h);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        return cap;
    }

    let cap = read_bitmap_pixels(screen_dc, bmp, mem_dc, old, w, h);
    let _ = DeleteObject(HGDIOBJ(bmp.0));
    let _ = DeleteDC(mem_dc);
    cap
}

/// 将位图选出于 DC 后以 GetDIBits 读取 32bpp 像素（BGRA → RGBA）。
///
/// # Safety
/// GDI 对象由调用方创建；本函数负责把位图选出（恢复 old）以便 GetDIBits 读取。
unsafe fn read_bitmap_pixels(
    screen_dc: windows::Win32::Graphics::Gdi::HDC,
    bmp: windows::Win32::Graphics::Gdi::HBITMAP,
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    old: windows::Win32::Graphics::Gdi::HGDIOBJ,
    w: i32,
    h: i32,
) -> Result<Capture, String> {
    // GetDIBits 要求位图未被选入任何 DC
    SelectObject(mem_dc, old);
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // 负值 = 自上而下行序
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: (w * h * 4) as u32,
            ..std::mem::zeroed()
        },
        bmiColors: [std::mem::zeroed()],
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = GetDIBits(
        screen_dc,
        bmp,
        0,
        h as u32,
        Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    if lines != h {
        return Err(format!("读取像素失败（GetDIBits 返回 {}）", lines));
    }
    Ok(Capture {
        width: w as u32,
        height: h as u32,
        rgba: bgra_to_rgba(&buf),
    })
}

/// BGRA → RGBA（GDI 32bpp 输出为 BGRA 字节序）
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

/// RGBA 像素 → PNG 字节（image crate，复用现有依赖）
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgba.to_vec()).ok_or("像素数据尺寸不匹配")?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("PNG 编码失败: {}", e))?;
    Ok(out)
}

/// 截图落盘目录：`%TEMP%\Aether\shots`
pub fn shots_dir() -> PathBuf {
    std::env::temp_dir().join("Aether").join("shots")
}

/// 将捕获结果编码为 PNG 并落盘，返回文件路径。
pub fn save_shot(cap: &Capture) -> Result<PathBuf, String> {
    let dir = shots_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建截图目录失败: {}", e))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("shot-{}.png", ts));
    let png = encode_png(cap.width, cap.height, &cap.rgba)?;
    std::fs::write(&path, &png).map_err(|e| format!("写入截图失败: {}", e))?;
    Ok(path)
}

/// 捕获虚拟屏幕并落盘（命令面板/Agent 工具入口）。
pub fn capture_screen_to_file() -> Result<PathBuf, String> {
    save_shot(&capture_virtual_screen()?)
}

/// 捕获指定窗口并落盘（Agent 工具 window 作用域入口）。
pub fn capture_window_to_file(hwnd: HWND) -> Result<PathBuf, String> {
    save_shot(&capture_window(hwnd)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra_to_rgba() {
        let bgra = vec![10, 20, 30, 255, 40, 50, 60, 128];
        assert_eq!(bgra_to_rgba(&bgra), vec![30, 20, 10, 255, 60, 50, 40, 128]);
    }

    #[test]
    fn test_png_roundtrip() {
        // 2x2 渐变图：编码 → 解码后像素逐一对齐
        let rgba = vec![
            255, 0, 0, 255, // 红
            0, 255, 0, 255, // 绿
            0, 0, 255, 255, // 蓝
            255, 255, 0, 128, // 半透明黄
        ];
        let png = encode_png(2, 2, &rgba).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 2));
        assert_eq!(decoded.as_raw(), &rgba);
    }

    #[test]
    fn test_encode_png_size_mismatch() {
        assert!(encode_png(2, 2, &[0; 8]).is_err());
    }

    #[test]
    fn test_shots_dir_under_temp() {
        let dir = shots_dir();
        assert!(dir.starts_with(std::env::temp_dir()));
        assert!(dir.ends_with("shots"));
    }
}
