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

    /// Render one page to JPEG bytes.
    ///
    /// Minimal config:
    ///
    /// - `BGRx` (no alpha) + explicit `WHITE` clear color — PDFium's transparency-
    ///   group compositor produces black rectangles when the destination has a
    ///   real alpha channel (BGRA). Using BGRx forces pre-flattening against the
    ///   clear color, which is what Chrome's PDF viewer and Adobe Reader do.
    ///
    /// - No `LCD_TEXT`, no `NO_NATIVETEXT` — empirically, these flags interact
    ///   badly with subset CID fonts in some PDFs (text renders as solid black
    ///   glyph boxes). Default flags are the most reliable across PDFs.
    ///
    /// - JPEG @ quality 85 — small payload, fast encode. The IPC base64 round-trip
    ///   is the dominant cost; PNG would more than double it.
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
                    "page {} has invalid dimensions: {page_w}×{page_h}",
                    req.page_index
                )));
            }

            let px_w = (page_w * req.scale).round().max(1.0) as i32;
            let px_h = (page_h * req.scale).round().max(1.0) as i32;

            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
                .set_format(PdfBitmapFormat::BGRx)
                .set_clear_color(PdfColor::WHITE);

            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| PdfError::Render(format!(
                    "pdfium render error (page {}, {}×{}): {}",
                    req.page_index, px_w, px_h, e
                )))?;

            let rgb = bitmap.as_image().into_rgb8();
            let mut buf = std::io::Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(rgb)
                .write_to(&mut buf, image::ImageFormat::Jpeg)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            Ok(buf.into_inner())
        })
    }
}
