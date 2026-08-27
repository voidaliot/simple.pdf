use crate::{Document, PdfError, PdfResult};
use image::DynamicImage;
use pdfium_render::prelude::*;
use std::sync::Arc;

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

/// Maximum pixels in one rendered page (24 MiB for one four-channel bitmap),
/// aligned with the frontend admission ceiling to bound Windows commit spikes
/// even for direct IPC callers.
const MAX_RENDER_PIXELS: u64 = 6_000_000;

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

/// Convert a stride-padded opaque BGRx buffer to packed RGB for JPEG encoding.
///
/// `raw` has length `stride * height`; only the first `width * 4` bytes
/// of each row are real pixels — the rest is alignment padding that must
/// be skipped.
fn bgrx_stride_to_rgb(
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
    let output_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| PdfError::Render("RGB output size overflow".into()))?;

    if stride < row_bytes_len || raw.len() != expected_raw_len {
        return Err(PdfError::Render(format!(
            "invalid bitmap layout: {} bytes for {width}×{height} at stride {stride}",
            raw.len()
        )));
    }

    let mut rgb = Vec::new();
    rgb.try_reserve_exact(output_len)
        .map_err(|_| PdfError::Render("unable to allocate JPEG RGB buffer".into()))?;
    for row in 0..height {
        let row_bytes = &raw[row * stride..row * stride + row_bytes_len];
        for chunk in row_bytes.chunks_exact(4) {
            rgb.extend_from_slice(&[chunk[2], chunk[1], chunk[0]]);
        }
    }
    Ok(rgb)
}

/// Normalize a tightly-packed BGRx bitmap to RGBA without allocating a second
/// full-page buffer. BGRx pixels are four-byte aligned, so their stride is
/// exactly `width * 4` even for odd page widths.
fn bgrx_to_rgba_in_place(pixels: &mut [u8]) -> PdfResult<()> {
    if pixels.len() % 4 != 0 {
        return Err(PdfError::Render(format!(
            "invalid tightly-packed BGRx buffer length ({})",
            pixels.len()
        )));
    }

    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        // BGRx's fourth byte is undefined. The bitmap is cleared to opaque
        // white before rendering, so expose a consistently opaque RGBA frame.
        pixel[3] = 255;
    }
    Ok(())
}

impl Document {
    /// Returns one page size, using the complete size cache when it has already
    /// been populated. Thumbnail generation can use this without walking every
    /// page in a large document.
    pub fn page_size(&self, page_index: u32) -> PdfResult<PageSize> {
        if page_index >= self.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }
        if let Some(sizes) = self.page_sizes_cache.lock().as_ref() {
            return sizes
                .get(page_index as usize)
                .cloned()
                .ok_or(PdfError::InvalidPage(page_index));
        }

        self.with_doc(|doc| {
            let size = doc
                .pages()
                .page_size(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            Ok(PageSize {
                width: size.width().value,
                height: size.height().value,
            })
        })
    }

    pub fn page_sizes(&self) -> PdfResult<Vec<PageSize>> {
        let mut cache = self.page_sizes_cache.lock();
        if let Some(sizes) = cache.as_ref() {
            return Ok(sizes.to_vec());
        }

        let sizes: Vec<PageSize> = self.with_doc(|doc| -> PdfResult<Vec<PageSize>> {
            doc.pages()
                .page_sizes()
                .map_err(|e| PdfError::Render(e.to_string()))
                .map(|sizes| {
                    sizes
                        .into_iter()
                        .map(|size| PageSize {
                            width: size.width().value,
                            height: size.height().value,
                        })
                        .collect()
                })
        })?;
        let sizes: Arc<[PageSize]> = sizes.into();
        *cache = Some(Arc::clone(&sizes));
        Ok(sizes.to_vec())
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
        let (mut rgba, width, height) = self.render_page_bgrx_buffer(req, 0)?;
        // This scan is pure Rust work. Keep it outside the process-wide
        // PDFium gate so the next native operation can start immediately.
        bgrx_to_rgba_in_place(&mut rgba)?;
        Ok(RawPage {
            rgba,
            width,
            height,
        })
    }

    /// Render directly into the final binary IPC response buffer.
    ///
    /// The first eight bytes contain little-endian width and height followed
    /// by tightly-packed RGBA pixels. Pdfium writes into the pixel portion of
    /// this allocation directly, avoiding the previous native-bitmap copy,
    /// RGBA allocation, and response-buffer copy for every page.
    pub fn render_page_ipc(&self, req: RenderRequest) -> PdfResult<Vec<u8>> {
        const HEADER_LEN: usize = 8;
        let (mut response, width, height) = self.render_page_bgrx_buffer(req, HEADER_LEN)?;
        bgrx_to_rgba_in_place(&mut response[HEADER_LEN..])?;
        response[0..4].copy_from_slice(&width.to_le_bytes());
        response[4..8].copy_from_slice(&height.to_le_bytes());
        Ok(response)
    }

    /// Return caller-owned BGRx pixels after releasing the global PDFium gate.
    fn render_page_bgrx_buffer(
        &self,
        req: RenderRequest,
        prefix_len: usize,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        let page_size = self.page_size(req.page_index)?;
        let (px_w, px_h) = checked_render_dimensions(page_size.width, page_size.height, req.scale)?;
        let width =
            u32::try_from(px_w).map_err(|_| PdfError::Render("negative render width".into()))?;
        let height =
            u32::try_from(px_h).map_err(|_| PdfError::Render("negative render height".into()))?;
        let pixel_len = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| PdfError::Render("render buffer size overflow".into()))?;
        let buffer_len = prefix_len
            .checked_add(pixel_len)
            .ok_or_else(|| PdfError::Render("render response size overflow".into()))?;

        // Reserve and commit the frame outside PDFIUM_GATE. Zero-filling a
        // 24 MiB buffer can otherwise make unrelated visible-page work wait
        // even though it does not touch the native library.
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(buffer_len).map_err(|_| {
            PdfError::Render(format!(
                "unable to allocate {} MiB page buffer",
                buffer_len.div_ceil(1024 * 1024)
            ))
        })?;
        buffer.resize(buffer_len, 0);

        // PdfRenderConfig defaults that matter here:
        //   clear_before_render = true   ← FPDFBitmap_FillRect called before FPDF_RenderPageBitmap
        //
        // BGRx plus an opaque white clear gives PDFium an opaque target for
        // transparency groups. We normalize byte order after releasing the
        // native gate.
        let config = PdfRenderConfig::new()
            .set_target_width(px_w)
            .set_target_height(px_h)
            .set_format(PdfBitmapFormat::BGRx)
            .set_clear_color(PdfColor::WHITE)
            .render_annotations(true)
            // Interactive widgets are drawn by the HTML form layer.
            .render_form_data(false)
            .set_reverse_byte_order(false);

        self.with_doc(|doc| {
            let pages = doc.pages();
            if req.page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(req.page_index));
            }
            let page = pages
                .get(req.page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            {
                let pixels = &mut buffer[prefix_len..];
                // SAFETY: checked_render_dimensions() guarantees positive,
                // bounded dimensions; pixel_len is exactly width*height*4 for
                // BGRx. The Vec cannot reallocate while PdfBitmap borrows this
                // slice, and the bitmap is dropped before the buffer is moved
                // or color-normalized. PDFium does not free caller memory when
                // FPDFBitmap_Destroy() closes an externally backed bitmap.
                let mut bitmap = unsafe {
                    PdfBitmap::from_bytes(
                        px_w,
                        px_h,
                        PdfBitmapFormat::BGRx,
                        pixels,
                        page.bindings(),
                    )
                }
                .map_err(|e| PdfError::Render(format!("failed to allocate PDFium bitmap: {e}")))?;

                page.render_into_bitmap_with_config(&mut bitmap, &config)
                    .map_err(|e| {
                        PdfError::Render(format!(
                            "pdfium render error (page {}, {}×{}): {}",
                            req.page_index, px_w, px_h, e
                        ))
                    })?;
            }

            Ok(())
        })?;

        Ok((buffer, width, height))
    }

    /// Render one page to JPEG bytes (thumbnails only — small, lossy is fine).
    pub fn render_page_jpeg(&self, req: RenderRequest) -> PdfResult<Vec<u8>> {
        let (raw, w, h, stride) = self.with_doc(|doc| {
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
            Ok((raw, w, h, stride))
        })?;

        // Conversion and JPEG encoding do not touch PDFium. Keeping them out
        // of the native-library critical section prevents thumbnails from
        // delaying visible-page renders and document search.
        let rgb = bgrx_stride_to_rgb(&raw, w, h, stride)?;
        let rgb_img = image::RgbImage::from_raw(w as u32, h as u32, rgb)
            .ok_or_else(|| PdfError::Render("failed to create image".into()))?;

        let mut buf = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(rgb_img)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| PdfError::Render(e.to_string()))?;

        Ok(buf.into_inner())
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
            checked_render_dimensions(2_000.0, 3_000.0, 1.0).unwrap(),
            (2_000, 3_000)
        );
    }

    #[test]
    fn bgrx_conversion_skips_padding() {
        let raw = [
            1, 2, 3, 0, 4, 5, 6, 17, 99, 99, 99, 99, // row 0 + padding
            7, 8, 9, 42, 10, 11, 12, 128, 88, 88, 88, 88, // row 1 + padding
        ];

        assert_eq!(
            bgrx_stride_to_rgb(&raw, 2, 2, 12).unwrap(),
            vec![3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]
        );
    }

    #[test]
    fn bgrx_conversion_rejects_malformed_layout() {
        assert!(bgrx_stride_to_rgb(&[0; 8], 2, 1, 7).is_err());
        assert!(bgrx_stride_to_rgb(&[0; 7], 1, 2, 4).is_err());
    }

    #[test]
    fn in_place_bgrx_conversion_handles_odd_pixel_counts() {
        let mut pixels = vec![
            1, 2, 3, 0, // BGRx pixel 1
            4, 5, 6, 17, // BGRx pixel 2
            7, 8, 9, 128, // BGRx pixel 3
        ];
        bgrx_to_rgba_in_place(&mut pixels).unwrap();
        assert_eq!(pixels, vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255]);
    }

    #[test]
    fn in_place_bgrx_conversion_rejects_partial_pixels() {
        assert!(bgrx_to_rgba_in_place(&mut [0; 3]).is_err());
        assert!(bgrx_to_rgba_in_place(&mut [0; 5]).is_err());
    }
}
