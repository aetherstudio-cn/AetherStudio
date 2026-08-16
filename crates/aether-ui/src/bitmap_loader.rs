//! 图片解码与 D2D 位图创建。
//!
//! - 欢迎页/空占位页 logo：使用 `load_png_to_bitmap`（PNG 字节 → ID2D1Bitmap）。
//! - 编辑器图片预览：使用 `decode_image_file` 解码常见位图格式为 RGBA8，
//!   再由渲染路径 `create_bitmap_from_rgba` 惰性创建 ID2D1Bitmap。
//!
//! 使用 `image` crate 解码，避免依赖系统 WIC 解码器（某些精简 Windows 环境可能缺少）。
//! 注意：D2D CreateBitmap 要求 PREMULTIPLIED alpha 的 BGRA8。

use std::path::Path;

use windows::Win32::Graphics::Direct2D::{
    Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_SIZE_U},
    ID2D1Bitmap, ID2D1RenderTarget, D2D1_BITMAP_PROPERTIES,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

/// 解码后的图片数据（设备无关，可随标签页状态保存/恢复）。
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// RGBA8 像素数据（未预乘），长度 = width * height * 4
    pub rgba: Vec<u8>,
    /// 人类可读的格式名（用于信息栏展示）
    pub format_name: &'static str,
}

/// 解码图片文件为 RGBA8。
///
/// 使用 `image::load_from_memory` 自动嗅探格式（PNG/JPEG/GIF/BMP/ICO/TIFF/WebP）。
/// GIF 动图返回首帧（编辑器预览定位，动画播放后续任务再实现）。
/// SVG/RAW/PSD 等 image crate 不支持的格式会返回 Err，由调用方落入占位提示。
pub fn decode_image_file(path: &Path) -> Result<DecodedImage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片失败: {}", e))?;
    let format_name = image::guess_format(&bytes)
        .map(image_format_name)
        .unwrap_or("未知");
    let img = image::load_from_memory(&bytes).map_err(|e| format!("解码图片失败: {}", e))?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Ok(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
        format_name,
    })
}

/// image::ImageFormat → 人类可读名
fn image_format_name(fmt: image::ImageFormat) -> &'static str {
    use image::ImageFormat as F;
    match fmt {
        F::Png => "PNG",
        F::Jpeg => "JPEG",
        F::Gif => "GIF",
        F::Bmp => "BMP",
        F::Ico => "ICO",
        F::Tiff => "TIFF",
        F::WebP => "WebP",
        _ => "图片",
    }
}

/// RGBA8 → 预乘 alpha 的 BGRA8（D2D CreateBitmap 要求）。
fn rgba_to_bgra_premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for chunk in rgba.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        let af = a as f32 / 255.0;
        bgra.push((b as f32 * af).round() as u8); // B 预乘
        bgra.push((g as f32 * af).round() as u8); // G 预乘
        bgra.push((r as f32 * af).round() as u8); // R 预乘
        bgra.push(a); // A 不变
    }
    bgra
}

/// 从 RGBA8 像素数据创建 ID2D1Bitmap（内部转为预乘 BGRA8）。
pub fn create_bitmap_from_rgba(
    target: &ID2D1RenderTarget,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<ID2D1Bitmap, String> {
    if width == 0 || height == 0 {
        return Err("图片尺寸为 0".to_string());
    }
    let bgra = rgba_to_bgra_premultiplied(rgba);

    let pixel_format = D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    };
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: pixel_format,
        dpiX: 96.0,
        dpiY: 96.0,
    };
    let size = D2D_SIZE_U { width, height };
    let pitch = width * 4;

    unsafe {
        match target.CreateBitmap(size, Some(bgra.as_ptr() as *const _), pitch, &props) {
            Ok(bmp) => Ok(bmp),
            Err(e) => {
                tracing::warn!(error = ?e, "BGRA8 PREMULTIPLIED 失败，尝试默认属性");
                let default_props = D2D1_BITMAP_PROPERTIES::default();
                target
                    .CreateBitmap(size, Some(bgra.as_ptr() as *const _), pitch, &default_props)
                    .map_err(|e2| {
                        tracing::error!(error = ?e2, "D2D CreateBitmap 默认属性也失败");
                        format!("D2D CreateBitmap 失败: {:?}", e2)
                    })
            }
        }
    }
}

/// 将 PNG 字节数据解码为 ID2D1Bitmap（欢迎页/空占位页 logo 专用）。
///
/// 使用 image crate 解码 PNG 为 RGBA8，再转换为预乘 alpha 的 BGRA8，
/// 最后通过 D2D CreateBitmap 从内存创建位图。
pub fn load_png_to_bitmap(
    target: &ID2D1RenderTarget,
    png_bytes: &[u8],
) -> Result<ID2D1Bitmap, String> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|e| format!("解码 PNG 失败: {}", e))?;
    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    create_bitmap_from_rgba(target, width, height, rgba.as_raw())
}
