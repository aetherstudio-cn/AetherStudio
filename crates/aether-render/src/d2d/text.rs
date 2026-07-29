use windows::core::Result;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS,
};

/// 文本渲染器
pub struct TextRenderer {
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    font_size: f32,
    line_height: f32,
    char_width: f32,
    dpi_scale: f32,
}

impl TextRenderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            let font_size = 14.0;
            let text_format = dwrite_factory.CreateTextFormat(
                windows::core::w!("Consolas"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("zh-CN"),
            )?;

            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;

            // UI-M03: 使用 DirectWrite 实测等宽字体字符宽度，替代硬编码 font_size * 0.6
            let char_width = Self::measure_monospace_width(&dwrite_factory, &text_format)
                .unwrap_or(font_size * 0.6);
            let line_height = font_size * 1.5;

            Ok(Self {
                dwrite_factory,
                text_format,
                font_size,
                line_height,
                char_width,
                dpi_scale: 1.0,
            })
        }
    }

    /// UI-M03: 使用 IDWriteTextLayout 实测等宽字体单字符推进宽度
    fn measure_monospace_width(
        factory: &IDWriteFactory,
        format: &IDWriteTextFormat,
    ) -> Result<f32> {
        unsafe {
            let text: Vec<u16> = "W".encode_utf16().collect();
            let layout: IDWriteTextLayout =
                factory.CreateTextLayout(&text, format, f32::MAX, f32::MAX)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics)?;
            Ok(metrics.width)
        }
    }

    /// 设置 DPI 缩放因子，更新字体大小和测量值
    pub fn set_dpi_scale(&mut self, scale: f32) {
        if (self.dpi_scale - scale).abs() < 0.01 {
            return;
        }
        self.dpi_scale = scale;
        let scaled_font_size = self.font_size * scale;
        unsafe {
            // 重新创建 text_format 以应用新的字体大小
            let new_text_format = self.dwrite_factory.CreateTextFormat(
                windows::core::w!("Consolas"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                scaled_font_size,
                windows::core::w!("zh-CN"),
            );
            if let Ok(tf) = new_text_format {
                let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
                self.text_format = tf;
            }
        }
        // UI-M03: 使用 DirectWrite 实测新 DPI 下的字符宽度
        self.char_width = Self::measure_monospace_width(&self.dwrite_factory, &self.text_format)
            .unwrap_or(self.font_size * 0.6 * scale);
        self.line_height = self.font_size * 1.5 * scale;
    }

    /// P2-3: 设置基础字体大小（用户快捷键缩放），按当前 DPI 重新创建格式
    pub fn set_font_size(&mut self, font_size: f32) {
        // 限制在合理范围避免极端值导致渲染异常
        let clamped = font_size.clamp(8.0, 72.0);
        if (self.font_size - clamped).abs() < 0.01 {
            return;
        }
        self.font_size = clamped;
        let scaled_font_size = self.font_size * self.dpi_scale;
        unsafe {
            let new_text_format = self.dwrite_factory.CreateTextFormat(
                windows::core::w!("Consolas"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                scaled_font_size,
                windows::core::w!("zh-CN"),
            );
            if let Ok(tf) = new_text_format {
                let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
                self.text_format = tf;
            }
        }
        self.char_width = Self::measure_monospace_width(&self.dwrite_factory, &self.text_format)
            .unwrap_or(self.font_size * 0.6 * self.dpi_scale);
        self.line_height = self.font_size * 1.5 * self.dpi_scale;
    }

    pub fn dpi_scale(&self) -> f32 {
        self.dpi_scale
    }

    // 5.4: 原 render_line / render_visible_lines / color_for_token / Viewport 为死代码
    //（实际渲染路径在 aether-win32 的 editor_view.rs，且每 token 创建 COM 对象的
    // 实现存在每帧分配问题），按未使用方法清理规范删除。

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn char_width(&self) -> f32 {
        self.char_width
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn dwrite_factory(&self) -> &IDWriteFactory {
        &self.dwrite_factory
    }
}
