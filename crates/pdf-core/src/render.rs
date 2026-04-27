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

    /// Render one page and return JPEG bytes.
    ///
    /// Key rendering decisions:
    ///
    /// `BGRx` (no alpha) + explicit `WHITE` clear — PDFium has a known compositing
    /// defect when rendering transparency groups into a bitmap that has an alpha
    /// channel (BGRA): the group's backdrop-alpha calculation collapses to black,
    /// producing large solid-black rectangles for any page that uses transparency
    /// groups (e.g. ToC pages with link-rect overlays). Switching to BGRx forces
    /// PDFium to pre-flatten every transparency group against the clear color before
    /// compositing, which matches the behaviour of Chrome's PDF viewer and Adobe
    /// Reader (both render into opaque bitmaps for the same reason).
    ///
    /// `disable_native_text_rendering` (FPDF_NO_NATIVETEXT) — prevents PDFium from
    /// asking Windows GDI to draw text. GDI fails on some embedded/subset fonts and
    /// emits black rectangles instead of glyphs. Chrome sets this flag on Windows.
    ///
    /// `use_lcd_text_rendering` (FPDF_LCD_TEXT) — subpixel AA, matches Chrome's
    /// sharp-text look on standard-DPI screens.
    ///
    /// JPEG @ quality 85 — ~10× smaller payload and ~5× faster to encode than PNG.
    /// Acceptable for screen viewing; eliminates most of the IPC / decode latency
    /// that makes scroll feel slow.
    pub fn render_page_jpeg(&self, req: RenderRequest) -> PdfResult<Vec<u8>> {
        // Fast path: return cached JPEG bytes if present.
        let scale_bucket = (req.scale * 100.0).round() as u32;
        let cache_key = (req.page_index, scale_bucket);
        {
            let mut cache = self.render_cache.lock();
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let bytes = self.with_doc(|doc| {
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
                .set_clear_color(PdfColor::WHITE)
                .disable_native_text_rendering(true)
                .use_lcd_text_rendering(true);

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
        })?;

        // Store in cache after releasing the doc lock.
        self.render_cache.lock().put(cache_key, bytes.clone());
        Ok(bytes)
    }

    /// Invalidate all cached renders for this document (e.g. after saving annotations).
    pub fn invalidate_render_cache(&self) {
        self.render_cache.lock().clear();
    }
}
