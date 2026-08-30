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

/// A device-pixel rectangle rendered from a page at its full requested scale.
///
/// Keeping the output rectangle separate from the virtual full-page dimensions
/// lets the viewer render high zoom levels without allocating one enormous
/// bitmap and then stretching a lower-resolution fallback.
#[derive(Debug, Clone)]
pub struct RenderTileRequest {
    pub page_index: u32,
    pub scale: f32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
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

/// Largest virtual full-page edge accepted by tiled rendering. This is far
/// above the viewer's normal 400% zoom range while still bounding pathological
/// direct IPC requests and keeping device-pixel translations exact in f32.
const MAX_VIRTUAL_RENDER_DIMENSION: u32 = 262_144;

/// Validates page dimensions and returns the requested full-page bitmap size.
/// Unlike `checked_render_dimensions()`, this deliberately does not apply the
/// one-bitmap memory ceiling; tiled callers allocate only their output region.
fn checked_scaled_page_dimensions(page_w: f32, page_h: f32, scale: f32) -> PdfResult<(u32, u32)> {
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

    let virtual_limit = f64::from(MAX_VIRTUAL_RENDER_DIMENSION);
    if scaled_w > virtual_limit || scaled_h > virtual_limit {
        return Err(PdfError::Render(format!(
            "virtual render dimensions {scaled_w:.0}×{scaled_h:.0} exceed the {MAX_VIRTUAL_RENDER_DIMENSION} px edge limit"
        )));
    }

    Ok((scaled_w as u32, scaled_h as u32))
}

/// Validates a requested scale and converts page points to bounded pixel
/// dimensions without casting NaN/infinity to an arbitrary integer.
fn checked_render_dimensions(page_w: f32, page_h: f32, scale: f32) -> PdfResult<(i32, i32)> {
    let (scaled_w, scaled_h) = checked_scaled_page_dimensions(page_w, page_h, scale)?;

    if scaled_w > MAX_RENDER_DIMENSION as u32 || scaled_h > MAX_RENDER_DIMENSION as u32 {
        return Err(PdfError::Render(format!(
            "render dimensions {scaled_w}×{scaled_h} exceed the {} px edge limit",
            MAX_RENDER_DIMENSION
        )));
    }

    let pixels = u64::from(scaled_w)
        .checked_mul(u64::from(scaled_h))
        .ok_or_else(|| PdfError::Render("render pixel count overflow".into()))?;

    if pixels > MAX_RENDER_PIXELS {
        return Err(PdfError::Render(format!(
            "render requires {pixels} pixels, exceeding the {MAX_RENDER_PIXELS} pixel limit"
        )));
    }

    Ok((scaled_w as i32, scaled_h as i32))
}

fn checked_render_tile(
    page_w: f32,
    page_h: f32,
    request: &RenderTileRequest,
) -> PdfResult<(u32, u32)> {
    let (full_width, full_height) = checked_scaled_page_dimensions(page_w, page_h, request.scale)?;

    if request.width == 0 || request.height == 0 {
        return Err(PdfError::Render(
            "render tile dimensions must be positive".into(),
        ));
    }
    if request.width > MAX_RENDER_DIMENSION as u32 || request.height > MAX_RENDER_DIMENSION as u32 {
        return Err(PdfError::Render(format!(
            "render tile dimensions {}×{} exceed the {} px edge limit",
            request.width, request.height, MAX_RENDER_DIMENSION
        )));
    }

    let pixels = u64::from(request.width)
        .checked_mul(u64::from(request.height))
        .ok_or_else(|| PdfError::Render("render tile pixel count overflow".into()))?;
    if pixels > MAX_RENDER_PIXELS {
        return Err(PdfError::Render(format!(
            "render tile requires {pixels} pixels, exceeding the {MAX_RENDER_PIXELS} pixel limit"
        )));
    }

    let right = request
        .x
        .checked_add(request.width)
        .ok_or_else(|| PdfError::Render("render tile horizontal bounds overflow".into()))?;
    let bottom = request
        .y
        .checked_add(request.height)
        .ok_or_else(|| PdfError::Render("render tile vertical bounds overflow".into()))?;
    if right > full_width || bottom > full_height {
        return Err(PdfError::Render(format!(
            "render tile ({}, {}) {}×{} lies outside the {full_width}×{full_height} page",
            request.x, request.y, request.width, request.height
        )));
    }

    Ok((full_width, full_height))
}

#[derive(Debug, Clone, Copy)]
struct RenderRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    full_width: u32,
    full_height: u32,
}

fn checked_region_scale(page_size: &PageSize, region: RenderRegion) -> PdfResult<(f32, f32)> {
    let width_scale = region.full_width as f32 / page_size.width;
    let height_scale = region.full_height as f32 / page_size.height;

    if !width_scale.is_finite()
        || !height_scale.is_finite()
        || width_scale <= 0.0
        || height_scale <= 0.0
    {
        return Err(PdfError::Render(
            "render tile transform must be finite and positive".into(),
        ));
    }

    Ok((width_scale, height_scale))
}

impl RenderRegion {
    fn is_full_page(self) -> bool {
        self.x == 0
            && self.y == 0
            && self.width == self.full_width
            && self.height == self.full_height
    }
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

    /// Render one full-resolution page tile directly into the binary IPC
    /// response format used by `render_page_ipc()`.
    pub fn render_page_tile_ipc(&self, req: RenderTileRequest) -> PdfResult<Vec<u8>> {
        const HEADER_LEN: usize = 8;
        let (mut response, width, height) = self.render_page_tile_bgrx_buffer(req, HEADER_LEN)?;
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
        let width = px_w as u32;
        let height = px_h as u32;
        self.render_page_region_bgrx_buffer(
            req.page_index,
            &page_size,
            RenderRegion {
                x: 0,
                y: 0,
                width,
                height,
                full_width: width,
                full_height: height,
            },
            prefix_len,
        )
    }

    fn render_page_tile_bgrx_buffer(
        &self,
        req: RenderTileRequest,
        prefix_len: usize,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        let page_size = self.page_size(req.page_index)?;
        let (full_width, full_height) =
            checked_render_tile(page_size.width, page_size.height, &req)?;
        self.render_page_region_bgrx_buffer(
            req.page_index,
            &page_size,
            RenderRegion {
                x: req.x,
                y: req.y,
                width: req.width,
                height: req.height,
                full_width,
                full_height,
            },
            prefix_len,
        )
    }

    fn render_page_region_bgrx_buffer(
        &self,
        page_index: u32,
        page_size: &PageSize,
        region: RenderRegion,
        prefix_len: usize,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        let width = region.width;
        let height = region.height;
        let px_w = i32::try_from(width)
            .map_err(|_| PdfError::Render("render region width is too large".into()))?;
        let px_h = i32::try_from(height)
            .map_err(|_| PdfError::Render("render region height is too large".into()))?;
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
        let config = if region.is_full_page() {
            PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h)
        } else {
            // Match the independently rounded width/height of a full render,
            // then translate in device pixels so neighboring regions share the
            // same page-space geometry without a divide/multiply round trip.
            let (width_scale, height_scale) = checked_region_scale(page_size, region)?;
            PdfRenderConfig::new()
                .set_fixed_size(px_w, px_h)
                .transform(
                    width_scale,
                    0.0,
                    0.0,
                    height_scale,
                    -(region.x as f32),
                    -(region.y as f32),
                )
                .map_err(|error| {
                    PdfError::Render(format!("invalid render tile transform: {error}"))
                })?
        }
        .set_format(PdfBitmapFormat::BGRx)
        .set_clear_color(PdfColor::WHITE)
        .render_annotations(true)
        // Interactive widgets are drawn by the HTML form layer.
        .render_form_data(false)
        .set_reverse_byte_order(false);

        self.with_doc(|doc| {
            let pages = doc.pages();
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            {
                let pixels = &mut buffer[prefix_len..];
                // SAFETY: region validation guarantees positive, bounded
                // dimensions; pixel_len is exactly width*height*4 for BGRx.
                // The Vec cannot reallocate while PdfBitmap borrows this
                // slice, and PDFium does not free caller-owned memory.
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
                            "pdfium render error (page {page_index}, region {},{} {}×{}): {e}",
                            region.x, region.y, px_w, px_h
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
    use crate::{PdfEngine, DOCUMENT_TEST_GATE, PDFIUM_GATE};

    fn create_blank_pdf(engine: &PdfEngine, path: &std::path::Path) {
        let _pdfium_guard = PDFIUM_GATE.lock();
        let pdfium = &engine.pdfium.as_ref().unwrap().pdfium;
        let mut pdf = pdfium.create_new_pdf().unwrap();
        pdf.pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .unwrap();
        pdf.save_to_file(path).unwrap();
    }

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
    fn tile_validation_keeps_full_scale_while_bounding_each_allocation() {
        let request = RenderTileRequest {
            page_index: 0,
            scale: 10.0,
            x: 4_000,
            y: 7_000,
            width: 1_024,
            height: 1_024,
        };

        assert!(checked_render_dimensions(595.0, 842.0, request.scale).is_err());
        assert_eq!(
            checked_render_tile(595.0, 842.0, &request).unwrap(),
            (5_950, 8_420)
        );

        let outside = RenderTileRequest {
            x: 5_900,
            ..request.clone()
        };
        assert!(checked_render_tile(595.0, 842.0, &outside).is_err());

        let empty = RenderTileRequest {
            width: 0,
            ..request.clone()
        };
        assert!(checked_render_tile(595.0, 842.0, &empty).is_err());

        let oversized = RenderTileRequest {
            x: 0,
            y: 0,
            width: 3_000,
            height: 3_000,
            ..request
        };
        assert!(checked_render_tile(595.0, 842.0, &oversized).is_err());

        assert_eq!(
            checked_scaled_page_dimensions(1.0, 1.0, MAX_VIRTUAL_RENDER_DIMENSION as f32).unwrap(),
            (MAX_VIRTUAL_RENDER_DIMENSION, MAX_VIRTUAL_RENDER_DIMENSION)
        );
        assert!(checked_scaled_page_dimensions(
            1.0,
            1.0,
            MAX_VIRTUAL_RENDER_DIMENSION as f32 + 1.0,
        )
        .is_err());
    }

    #[test]
    fn rendered_tile_matches_full_page_geometry_and_color_values() {
        let _test_guard = DOCUMENT_TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tile.pdf");
        let dll_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium");
        let engine = PdfEngine::new(&dll_dir).unwrap();
        create_blank_pdf(&engine, &path);
        let document = engine.open(&path).unwrap();
        document
            .add_ink_annotation(0, &[vec![[0.05, 0.05], [0.95, 0.95]]], [20, 80, 200], 12.0)
            .unwrap();

        let scale = 0.75;
        let full = document
            .render_page_raw(RenderRequest {
                page_index: 0,
                scale,
            })
            .unwrap();
        let request = RenderTileRequest {
            page_index: 0,
            scale,
            x: 80,
            y: 110,
            width: 240,
            height: 300,
        };
        let frame = document.render_page_tile_ipc(request.clone()).unwrap();
        assert_eq!(
            u32::from_le_bytes(frame[0..4].try_into().unwrap()),
            request.width
        );
        assert_eq!(
            u32::from_le_bytes(frame[4..8].try_into().unwrap()),
            request.height
        );

        let tile_pixels = &frame[8..];
        let full_stride = full.width as usize * 4;
        let tile_stride = request.width as usize * 4;
        let mut expected = Vec::with_capacity(tile_pixels.len());
        for row in request.y as usize..(request.y + request.height) as usize {
            let start = row * full_stride + request.x as usize * 4;
            expected.extend_from_slice(&full.rgba[start..start + tile_stride]);
        }
        assert!(
            expected
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [255, 255, 255]),
            "the comparison tile must cross visible page content"
        );
        let content_bounds = |pixels: &[u8]| {
            let mut left = request.width;
            let mut top = request.height;
            let mut right = 0;
            let mut bottom = 0;
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                if pixel[..3] == [255, 255, 255] {
                    continue;
                }
                let x = index as u32 % request.width;
                let y = index as u32 / request.width;
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
            (left, top, right, bottom)
        };
        assert_eq!(content_bounds(tile_pixels), content_bounds(&expected));
        let changed_channels = tile_pixels
            .iter()
            .zip(&expected)
            .filter(|(actual, expected)| actual != expected)
            .count();
        let max_delta = tile_pixels
            .iter()
            .zip(&expected)
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .unwrap_or(0);
        // PDFium may round a transformed tile's anti-aliased edge channels by
        // one level compared with a crop from a monolithic bitmap. Geometry
        // and all visible channel values must otherwise agree.
        assert!(
            max_delta <= 1,
            "tile content {:?}, expected {:?}; {changed_channels} changed channels, max delta {max_delta}",
            content_bounds(tile_pixels),
            content_bounds(&expected)
        );
        assert!(
            changed_channels <= tile_pixels.len() / 1_000,
            "tile changed {changed_channels} of {} channels",
            tile_pixels.len()
        );

        // Neighboring regions must stitch to the same geometry as one larger
        // region, including visible ink that crosses their shared boundary.
        let split_width = request.width / 2;
        let left_request = RenderTileRequest {
            width: split_width,
            ..request.clone()
        };
        let right_request = RenderTileRequest {
            x: request.x + split_width,
            width: request.width - split_width,
            ..request.clone()
        };
        let left = document.render_page_tile_ipc(left_request).unwrap();
        let right = document.render_page_tile_ipc(right_request).unwrap();
        let left_stride = split_width as usize * 4;
        let right_stride = (request.width - split_width) as usize * 4;
        let mut stitched = Vec::with_capacity(tile_pixels.len());
        for row in 0..request.height as usize {
            let left_start = 8 + row * left_stride;
            stitched.extend_from_slice(&left[left_start..left_start + left_stride]);
            let right_start = 8 + row * right_stride;
            stitched.extend_from_slice(&right[right_start..right_start + right_stride]);
        }

        let seam_has_content = (0..request.height as usize).any(|row| {
            let first_x = split_width.saturating_sub(2) as usize;
            (first_x..(split_width + 2).min(request.width) as usize).any(|x| {
                let offset = (row * request.width as usize + x) * 4;
                stitched[offset..offset + 3] != [255, 255, 255]
            })
        });
        assert!(
            seam_has_content,
            "the fixture must draw across the tile seam"
        );
        assert_eq!(content_bounds(&stitched), content_bounds(tile_pixels));
        let stitched_changed_channels = stitched
            .iter()
            .zip(tile_pixels)
            .filter(|(actual, expected)| actual != expected)
            .count();
        let stitched_max_delta = stitched
            .iter()
            .zip(tile_pixels)
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .unwrap_or(0);
        // PDFium applies clipping before anti-aliasing, so independently
        // clipped regions can differ by a couple of channel levels at vector
        // edges. Keep that variation both visually negligible and sparse.
        assert!(stitched_max_delta <= 2);
        assert!(
            stitched_changed_channels <= tile_pixels.len() / 1_000,
            "stitched neighboring tiles changed {stitched_changed_channels} of {} channels",
            tile_pixels.len()
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
