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

/// Maximum bitmap edge accepted by the renderer. This also stays within the
/// practical canvas limits of the WebView receiving the pixels.
const MAX_RENDER_DIMENSION: i32 = 16_384;

/// Maximum pixels in one rendered page (128 MiB for one four-channel bitmap).
/// The complete render/IPC pipeline temporarily holds more than one copy.
const MAX_RENDER_PIXELS: u64 = 32 * 1024 * 1024;

/// Validates a requested scale and converts page points to bounded pixel
/// dimensions without casting NaN/infinity to an arbitrary integer.
fn checked_render_dimensions(page_w: f32, page_h: f32, scale: f32) -> PdfResult<(i32, i32)> {
    if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0.0 || page_h <= 0.0 {
        return Err(PdfError::Render(format!(
            "invalid page dimensions ({page_w}×{page_h})"
        )));
    }

    if !scale.is_finite() || scale <= 0.0 {
        return Err(PdfError::Render(format!(
            "render scale must be finite and positive (got {scale})"
        )));
    }

    // Calculate in f64 so validation occurs before the narrowing integer cast.
    let scaled_w = (f64::from(page_w) * f64::from(scale)).round().max(1.0);
    let scaled_h = (f64::from(page_h) * f64::from(scale)).round().max(1.0);

    if !scaled_w.is_finite() || !scaled_h.is_finite() {
        return Err(PdfError::Render("render dimensions are not finite".into()));
    }

    let max_dimension = f64::from(MAX_RENDER_DIMENSION);
    if scaled_w > max_dimension || scaled_h > max_dimension {
        return Err(PdfError::Render(format!(
            "render dimensions {:.0}×{:.0} exceed the {} px edge limit",
            scaled_w, scaled_h, MAX_RENDER_DIMENSION
        )));
    }

    let px_w = scaled_w as i32;
    let px_h = scaled_h as i32;
    let pixels = (px_w as u64)
        .checked_mul(px_h as u64)
        .ok_or_else(|| PdfError::Render("render pixel count overflow".into()))?;

    if pixels > MAX_RENDER_PIXELS {
        return Err(PdfError::Render(format!(
            "render requires {pixels} pixels, exceeding the {MAX_RENDER_PIXELS} pixel limit"
        )));
    }

    Ok((px_w, px_h))
}

/// Convert a stride-padded opaque BGRx buffer to packed RGBA.
///
/// `raw` has length `stride * height`; only the first `width * 4` bytes
/// of each row are real pixels — the rest is alignment padding that must
/// be skipped.
fn bgrx_stride_to_rgba(
    raw: &[u8],
    width: usize,
    height: usize,
    stride: usize,
) -> PdfResult<Vec<u8>> {
    let row_bytes_len = width
        .checked_mul(4)
        .ok_or_else(|| PdfError::Render("bitmap row size overflow".into()))?;
    let expected_raw_len = stride
        .checked_mul(height)
        .ok_or_else(|| PdfError::Render("bitmap buffer size overflow".into()))?;
    let output_len = row_bytes_len
        .checked_mul(height)
        .ok_or_else(|| PdfError::Render("RGBA output size overflow".into()))?;

    if stride < row_bytes_len || raw.len() != expected_raw_len {
        return Err(PdfError::Render(format!(
            "invalid bitmap layout: {} bytes for {width}×{height} at stride {stride}",
            raw.len()
        )));
    }

    let mut rgba = Vec::with_capacity(output_len);
    for row in 0..height {
        let row_bytes = &raw[row * stride..row * stride + row_bytes_len];
        for chunk in row_bytes.chunks_exact(4) {
            // PDFium BGRx → web RGBA: swap B/R and do not interpret the
            // undefined x byte as alpha. The page is explicitly rendered
            // against opaque white, so output alpha is always 255.
            rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
        }
    }
    Ok(rgba)
}

impl Document {
    pub fn page_sizes(&self) -> PdfResult<Vec<PageSize>> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let mut sizes = Vec::with_capacity(pages.len() as usize);
            for i in 0..pages.len() {
                let page = pages.get(i).map_err(|e| PdfError::Render(e.to_string()))?;
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
    /// PDFium renders into an explicitly opaque white BGRx target in its native
    /// byte order. The display path intentionally uses the default screen
    /// rendering flags (no LCD subpixel or printing mode), then performs the
    /// BGRx-to-RGBA conversion here with a forced opaque alpha channel.
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

            let (px_w, px_h) = checked_render_dimensions(page_w, page_h, req.scale)?;

            // PdfRenderConfig defaults that matter here:
            //   clear_before_render = true   ← FPDFBitmap_FillRect called before FPDF_RenderPageBitmap
            //
            // Explicit settings deliberately keep the display render path
            // minimal. In particular, LCD_TEXT and PRINTING are not enabled:
            // they can change compositing/font behavior and are inappropriate
            // for a CSS-scaled screen bitmap.
            //
            // BGRx plus an opaque white clear gives PDFium an opaque target for
            // transparency groups. We leave byte order native and normalize it
            // ourselves after rendering.
            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
                .set_format(PdfBitmapFormat::BGRx)
                .set_clear_color(PdfColor::WHITE)
                .render_annotations(true)
                // Interactive widgets are drawn by the HTML form layer.
                .render_form_data(false)
                .set_reverse_byte_order(false);

            let bitmap = page.render_with_config(&config).map_err(|e| {
                PdfError::Render(format!(
                    "pdfium render error (page {}, {}×{}): {}",
                    req.page_index, px_w, px_h, e
                ))
            })?;

            let w = usize::try_from(bitmap.width())
                .map_err(|_| PdfError::Render("PDFium returned a negative bitmap width".into()))?;
            let h = usize::try_from(bitmap.height())
                .map_err(|_| PdfError::Render("PDFium returned a negative bitmap height".into()))?;
            if w == 0 || h == 0 {
                return Err(PdfError::Render(format!(
                    "PDFium returned an empty bitmap for page {}",
                    req.page_index
                )));
            }

            // `as_raw_bytes()` returns stride * height bytes; stride may exceed
            // width * 4 due to alignment padding.  Strip the padding per-row
            // while doing the BGRx→RGBA channel swap.
            let raw = bitmap.as_raw_bytes();
            if raw.len() % h != 0 {
                return Err(PdfError::Render(format!(
                    "PDFium returned a malformed bitmap buffer for page {}",
                    req.page_index
                )));
            }
            let stride = raw.len() / h;
            let rgba = bgrx_stride_to_rgba(&raw, w, h, stride)?;

            Ok(RawPage {
                rgba,
                width: w as u32,
                height: h as u32,
            })
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

            let (px_w, px_h) = checked_render_dimensions(page_w, page_h, req.scale)?;

            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
                .set_format(PdfBitmapFormat::BGRx)
                .set_clear_color(PdfColor::WHITE)
                .render_annotations(true)
                .set_reverse_byte_order(false);

            let bitmap = page.render_with_config(&config).map_err(|e| {
                PdfError::Render(format!(
                    "pdfium render error (page {}, {}×{}): {}",
                    req.page_index, px_w, px_h, e
                ))
            })?;

            let w = usize::try_from(bitmap.width())
                .map_err(|_| PdfError::Render("PDFium returned a negative bitmap width".into()))?;
            let h = usize::try_from(bitmap.height())
                .map_err(|_| PdfError::Render("PDFium returned a negative bitmap height".into()))?;
            if w == 0 || h == 0 {
                return Err(PdfError::Render(format!(
                    "PDFium returned an empty bitmap for page {}",
                    req.page_index
                )));
            }
            let raw = bitmap.as_raw_bytes();
            if raw.len() % h != 0 {
                return Err(PdfError::Render(format!(
                    "PDFium returned a malformed bitmap buffer for page {}",
                    req.page_index
                )));
            }
            let stride = raw.len() / h;
            let rgba = bgrx_stride_to_rgba(&raw, w, h, stride)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_dimensions_round_normally() {
        assert_eq!(
            checked_render_dimensions(612.0, 792.0, 1.5).unwrap(),
            (918, 1188)
        );
        assert_eq!(checked_render_dimensions(0.1, 0.1, 0.1).unwrap(), (1, 1));
    }

    #[test]
    fn checked_dimensions_reject_invalid_scale() {
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(checked_render_dimensions(612.0, 792.0, scale).is_err());
        }
    }

    #[test]
    fn checked_dimensions_reject_invalid_page_size() {
        assert!(checked_render_dimensions(0.0, 792.0, 1.0).is_err());
        assert!(checked_render_dimensions(-612.0, 792.0, 1.0).is_err());
        assert!(checked_render_dimensions(f32::NAN, 792.0, 1.0).is_err());
        assert!(checked_render_dimensions(612.0, f32::INFINITY, 1.0).is_err());
    }

    #[test]
    fn checked_dimensions_enforce_edge_and_pixel_limits() {
        assert!(checked_render_dimensions(1_000.0, 1_000.0, 20.0).is_err());
        assert!(checked_render_dimensions(10_000.0, 10_000.0, 1.0).is_err());

        // Exactly the configured pixel budget remains valid.
        assert_eq!(
            checked_render_dimensions(4_096.0, 8_192.0, 1.0).unwrap(),
            (4_096, 8_192)
        );
    }

    #[test]
    fn bgrx_conversion_skips_padding_and_forces_opaque_alpha() {
        let raw = [
            1, 2, 3, 0, 4, 5, 6, 17, 99, 99, 99, 99, // row 0 + padding
            7, 8, 9, 42, 10, 11, 12, 128, 88, 88, 88, 88, // row 1 + padding
        ];

        assert_eq!(
            bgrx_stride_to_rgba(&raw, 2, 2, 12).unwrap(),
            vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255,]
        );
    }

    #[test]
    fn bgrx_conversion_rejects_malformed_layout() {
        assert!(bgrx_stride_to_rgba(&[0; 8], 2, 1, 7).is_err());
        assert!(bgrx_stride_to_rgba(&[0; 7], 1, 2, 4).is_err());
    }
}
