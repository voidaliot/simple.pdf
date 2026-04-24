use crate::{Document, PdfError, PdfResult};

#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub page_index: u32,
    pub scale: f32,
}

pub struct PageRender {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Document {
    pub fn render_page(&self, req: RenderRequest) -> PdfResult<PageRender> {
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
            let px_w = (page_w * req.scale).round().max(1.0) as i32;
            let px_h = (page_h * req.scale).round().max(1.0) as i32;

            use pdfium_render::prelude::*;
            let config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_maximum_height(px_h);

            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| PdfError::Render(e.to_string()))?;

            let img = bitmap.as_image().into_rgba8();
            let (w, h) = img.dimensions();
            let mut bgra = img.into_raw();
            for chunk in bgra.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }
            Ok(PageRender {
                width: w,
                height: h,
                bgra,
            })
        })
    }
}
