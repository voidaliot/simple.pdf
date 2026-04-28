use crate::{Document, PdfError, PdfResult};
use image::DynamicImage;
use pdfium_render::prelude::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub page_index: u32,
    pub scale: f32,
}

/// Raw RGBA pixel data for one rendered page, width × height × 4 bytes.
/// Alpha is 255 everywhere (page fully composited against opaque white).
pub struct RawPage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Convert a stride-padded BGRA buffer to a packed RGBA buffer.
///
/// `raw` has length `stride * height`; only the first `width * 4` bytes
/// of each row are real pixels — the rest is alignment padding that must
/// be skipped.
fn bgra_stride_to_rgba(raw: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let row_bytes = &raw[row * stride..row * stride + width * 4];
        for chunk in row_bytes.chunks_exact(4) {
            // PDFium BGRA → web RGBA: swap B (byte 0) ↔ R (byte 2)
            rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
        }
    }
    rgba
}

impl Document {
    pub fn page_sizes(&self) -> PdfResult<Vec<PageSize>> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let mut sizes = Vec::with_capacity(pages.len() as usize);
            for i in 0..pages.len() {
                let page = pages
                    .get(i)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                sizes.push(PageSize {
                    width: page.width().value,
                    height: page.height().value,
                });
            }
            Ok(sizes)
        })
    }

    /// Render one page to raw RGBA pixel data.
    ///
    /// ## Why `set_reverse_byte_order(false)` is the key fix
    ///
    /// When `FPDF_REVERSE_BYTE_ORDER` is active (pdfium-render's default),
    /// PDFium writes rendered pixels in RGBA order but still *reads* the
    /// destination bitmap in BGRA order during Porter-Duff compositing of
    /// transparency groups and soft masks.  This read/write mismatch corrupts
    /// the compositor and produces solid-black areas wherever transparency
    /// groups or blend modes are used (common in ASPICE PDFs, scanned docs,
    /// many modern-designed PDFs).
    ///
    /// Disabling the flag keeps the entire pipeline — pre-fill, transparency
    /// group compositing, final write — in native BGRA, eliminating the
    /// mismatch.  We do the BGRA→RGBA byte swap ourselves in Rust after
    /// rendering, where it is straightforward and cheap.
    ///
    /// ## Other flags
    ///
    /// * `use_lcd_text_rendering` (FPDF_LCD_TEXT): enables LCD subpixel
    ///   antialiasing for text on screen.
    /// * `use_print_quality` (FPDF_PRINTING): uses the same rendering path
    ///   as Chromium's PDF plugin for transparency group flattening.
    ///
    /// ## Output
    ///
    /// Flat RGBA `Vec<u8>` of `width × height × 4` bytes, alpha = 255
    /// everywhere (fully composited against the opaque white background).
    pub fn render_page_raw(&self, req: RenderRequest) -> PdfResult<RawPage> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if req.page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(req.page_index));
            }
            let page = pages
                .get(req.page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            let page_w = page.width().value;
            let page_h = page.height().value;

            if page_w <= 0.0 || page_h <= 0.0 {
                return Err(PdfError::Render(format!(
                    "page {} has zero dimensions ({page_w}×{page_h})",
                    req.page_index
                )));
            }

            let px_w = (page_w * req.scale).round().max(1.0) as i32;
            let px_h = (page_h * req.scale).round().max(1.0) as i32;

            // PdfRenderConfig defaults that matter here:
            //   format              = BGRA   ← real alpha channel (not BGRx)
            //   clear_before_render = true   ← FPDFBitmap_FillRect called before FPDF_RenderPageBitmap
            //   clear_color         = WHITE  ← 0xFFFFFFFF: opaque white
            //   render_annotations  = true   ← FPDF_ANNOT
            //
            // Explicit overrides:
            //   set_reverse_byte_order(false) ← NO FPDF_REVERSE_BYTE_ORDER
            //   use_lcd_text_rendering(true)  ← FPDF_LCD_TEXT
            //   use_print_quality(true)        ← FPDF_PRINTING
            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
                .use_lcd_text_rendering(true)
                .use_print_quality(true)
                .set_reverse_byte_order(false);

            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| PdfError::Render(format!(
                    "pdfium render error (page {}, {}×{}): {}",
                    req.page_index, px_w, px_h, e
                )))?;

            let w = bitmap.width() as usize;
            let h = bitmap.height() as usize;

            // `as_raw_bytes()` returns stride * height bytes; stride may exceed
            // width * 4 due to alignment padding.  Strip the padding per-row
            // while doing the BGRA→RGBA channel swap.
            let raw = bitmap.as_raw_bytes();
            let stride = if h > 0 { raw.len() / h } else { w * 4 };
            let rgba = bgra_stride_to_rgba(&raw, w, h, stride);

            Ok(RawPage { rgba, width: w as u32, height: h as u32 })
        })
    }

    /// Render one page to JPEG bytes (thumbnails only — small, lossy is fine).
    pub fn render_page_jpeg(&self, req: RenderRequest) -> PdfResult<Vec<u8>> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if req.page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(req.page_index));
            }
            let page = pages
                .get(req.page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            let page_w = page.width().value;
            let page_h = page.height().value;

            if page_w <= 0.0 || page_h <= 0.0 {
                return Err(PdfError::Render(format!(
                    "page {} has zero dimensions ({page_w}×{page_h})",
                    req.page_index
                )));
            }

            let px_w = (page_w * req.scale).round().max(1.0) as i32;
            let px_h = (page_h * req.scale).round().max(1.0) as i32;

            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
                .use_print_quality(true)
                .set_reverse_byte_order(false);

            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| PdfError::Render(format!(
                    "pdfium render error (page {}, {}×{}): {}",
                    req.page_index, px_w, px_h, e
                )))?;

            let w = bitmap.width() as usize;
            let h = bitmap.height() as usize;
            let raw = bitmap.as_raw_bytes();
            let stride = if h > 0 { raw.len() / h } else { w * 4 };
            let rgba = bgra_stride_to_rgba(&raw, w, h, stride);

            // RGBA → RGB (drop alpha, all pixels are fully opaque anyway) → JPEG
            let rgb_img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
                .map(DynamicImage::ImageRgba8)
                .ok_or_else(|| PdfError::Render("failed to create image".into()))?
                .into_rgb8();

            let mut buf = std::io::Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(rgb_img)
                .write_to(&mut buf, image::ImageFormat::Jpeg)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            Ok(buf.into_inner())
        })
    }
}
