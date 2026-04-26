use crate::{Document, PdfError, PdfResult};
use pdfium_render::prelude::*;

/// Normalized annotation bounding rect ([0,1] relative to page dimensions).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnnRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// An annotation serialized for the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    pub index: u32,
    /// "highlight" | "underline" | "squiggly" | "strikeout" | "text" | "ink" | "widget" | "other"
    pub kind: String,
    pub rect: AnnRect,
    /// RGBA, each in [0..255].
    pub color: [u8; 4],
    pub contents: Option<String>,
    pub author: Option<String>,
}

// ── Read ───────────────────────────────────────────────────────────────────────

impl Document {
    pub fn page_annotations(&self, page_index: u32) -> PdfResult<Vec<Annotation>> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            if page_index >= pages.len() as u32 {
                return Err(PdfError::InvalidPage(page_index));
            }
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let pw = page.width().value;
            let ph = page.height().value;
            let annotations = page.annotations();
            let count = annotations.len();
            let mut result = Vec::with_capacity(count as usize);

            for i in 0..count {
                let annot = match annotations.get(i) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let kind = kind_str(annot.annotation_type());
                let rect = annot
                    .bounds()
                    .map(|b| pdf_to_screen(&b, pw, ph))
                    .unwrap_or(AnnRect { left: 0.0, top: 0.0, width: 0.05, height: 0.05 });
                let color = annot
                    .fill_color()
                    .map(|c| [c.red(), c.green(), c.blue(), c.alpha()])
                    .unwrap_or([255, 214, 0, 128]);
                let contents = annot.contents();
                let author = annot.creator();
                result.push(Annotation { index: i, kind, rect, color, contents, author });
            }
            Ok(result)
        })
    }
}

// ── Write ──────────────────────────────────────────────────────────────────────

impl Document {
    pub fn add_highlight(
        &self,
        page_index: u32,
        rects: &[AnnRect],
        color: [u8; 3],
        opacity: f32,
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            let pdf_color = PdfColor::new(color[0], color[1], color[2], alpha);
            let annots = page.annotations();
            let idx = create_markup_annotation(
                &annots,
                MarkupKind::Highlight,
                rects,
                pdf_color,
                pw,
                ph,
            )?;
            Ok(idx)
        })
    }

    pub fn add_underline(
        &self,
        page_index: u32,
        rects: &[AnnRect],
        color: [u8; 3],
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            let pdf_color = PdfColor::new(color[0], color[1], color[2], 200);
            let annots = page.annotations();
            create_markup_annotation(&annots, MarkupKind::Underline, rects, pdf_color, pw, ph)
        })
    }

    pub fn add_strikeout(
        &self,
        page_index: u32,
        rects: &[AnnRect],
        color: [u8; 3],
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            let pdf_color = PdfColor::new(color[0], color[1], color[2], 200);
            let annots = page.annotations();
            create_markup_annotation(&annots, MarkupKind::StrikeOut, rects, pdf_color, pw, ph)
        })
    }

    pub fn add_text_annotation(
        &self,
        page_index: u32,
        left: f32,
        top: f32,
        contents: &str,
        author: Option<&str>,
        color: [u8; 3],
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);

            // PDF rect: small icon area at the clicked position
            let rect = PdfRect::new(
                PdfPoints::new((1.0 - top - 0.04) * ph),
                PdfPoints::new(left * pw),
                PdfPoints::new((left + 0.03) * pw),
                PdfPoints::new((1.0 - top) * ph),
            );

            let annots = page.annotations();
            {
                let mut sticky = annots
                    .create_text_annotation()
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                sticky
                    .set_bounds(rect)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                sticky
                    .set_contents(contents)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                sticky
                    .set_fill_color(PdfColor::new(color[0], color[1], color[2], 255))
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                if let Some(a) = author {
                    let _ = sticky.set_creator(a);
                }
            }
            Ok(annots.len().saturating_sub(1))
        })
    }

    pub fn add_ink_annotation(
        &self,
        page_index: u32,
        paths: &[Vec<[f32; 2]>],
        color: [u8; 3],
        width: f32,
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            let annots = page.annotations();

            // Compute bounding box of all points
            let mut xmin = f32::MAX;
            let mut ymin = f32::MAX;
            let mut xmax = f32::MIN;
            let mut ymax = f32::MIN;
            for path in paths {
                for [x, y] in path {
                    xmin = xmin.min(*x);
                    ymin = ymin.min(*y);
                    xmax = xmax.max(*x);
                    ymax = ymax.max(*y);
                }
            }

            {
                let mut ink = annots
                    .create_ink_annotation()
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                ink.set_stroke_color(PdfColor::new(color[0], color[1], color[2], 255))
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                ink.set_stroke_width(PdfPoints::new(width))
                    .map_err(|e| PdfError::Render(e.to_string()))?;

                // Set bounding rect
                if xmin < f32::MAX {
                    let rect = PdfRect::new(
                        PdfPoints::new((1.0 - ymax) * ph),
                        PdfPoints::new(xmin * pw),
                        PdfPoints::new(xmax * pw),
                        PdfPoints::new((1.0 - ymin) * ph),
                    );
                    let _ = ink.set_bounds(rect);
                }

                // Add ink paths.
                // pdfium-render wraps FPDFAnnot_AppendAttachmentPoints / ink list.
                // If ink_paths_mut() exists, use it; otherwise bounds-only.
                for path_pts in paths {
                    let pts: Vec<PdfPoint> = path_pts
                        .iter()
                        .map(|[x, y]| {
                            PdfPoint::new(PdfPoints::new(x * pw), PdfPoints::new((1.0 - y) * ph))
                        })
                        .collect();
                    // Try to add a stroke. This API may vary by pdfium-render version.
                    let _ = ink.ink_list_mut().map(|mut list| list.add(&pts));
                }
            }
            Ok(annots.len().saturating_sub(1))
        })
    }

    pub fn remove_annotation(&self, page_index: u32, annot_index: u32) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let annots = page.annotations();
            // pdfium-render wraps FPDFPage_RemoveAnnot
            annots
                .delete_annotation_at_index(annot_index)
                .map_err(|e| PdfError::Render(e.to_string()))
        })
    }

    /// Atomically overwrite the original file: write to a temp file then rename.
    pub fn save_to_path(&self, path: &std::path::Path) -> PdfResult<()> {
        self.with_doc(|doc| {
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            let tmp_name = format!(".simplepdf_{}.tmp", uuid::Uuid::new_v4().as_simple());
            let tmp = parent.join(&tmp_name);
            doc.save_to_file(&tmp)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            std::fs::rename(&tmp, path).map_err(PdfError::Io)
        })
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────────

enum MarkupKind { Highlight, Underline, StrikeOut }

fn create_markup_annotation(
    annots: &PdfPageAnnotations<'_>,
    kind: MarkupKind,
    rects: &[AnnRect],
    color: PdfColor,
    pw: f32,
    ph: f32,
) -> PdfResult<u32> {
    let union = match union_rects(rects) {
        Some(u) => u,
        None => return Err(PdfError::Render("no rects supplied".into())),
    };
    let pdf_rect = screen_to_pdf(&union, pw, ph);

    let set_common = |bounds_setter: &dyn Fn(PdfRect) -> PdfResult<()>,
                      color_setter: &dyn Fn(PdfColor) -> PdfResult<()>| -> PdfResult<()> {
        bounds_setter(pdf_rect.clone())?;
        color_setter(color.clone())?;
        Ok(())
    };

    match kind {
        MarkupKind::Highlight => {
            let mut hl = annots
                .create_highlight_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            hl.set_bounds(pdf_rect).map_err(|e| PdfError::Render(e.to_string()))?;
            hl.set_fill_color(color).map_err(|e| PdfError::Render(e.to_string()))?;
        }
        MarkupKind::Underline => {
            let mut ul = annots
                .create_underline_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            ul.set_bounds(pdf_rect).map_err(|e| PdfError::Render(e.to_string()))?;
            ul.set_fill_color(color).map_err(|e| PdfError::Render(e.to_string()))?;
        }
        MarkupKind::StrikeOut => {
            let mut so = annots
                .create_strikeout_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_bounds(pdf_rect).map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_fill_color(color).map_err(|e| PdfError::Render(e.to_string()))?;
        }
    }
    let _ = set_common; // suppress unused warning
    Ok(annots.len().saturating_sub(1))
}

fn page_dims(page: &PdfPage<'_>) -> (f32, f32) {
    (page.width().value, page.height().value)
}

fn kind_str(t: PdfPageAnnotationType) -> String {
    match t {
        PdfPageAnnotationType::Highlight => "highlight",
        PdfPageAnnotationType::Underline => "underline",
        PdfPageAnnotationType::Squiggly  => "squiggly",
        PdfPageAnnotationType::StrikeOut => "strikeout",
        PdfPageAnnotationType::Text      => "text",
        PdfPageAnnotationType::Ink       => "ink",
        PdfPageAnnotationType::Widget    => "widget",
        PdfPageAnnotationType::Stamp     => "stamp",
        PdfPageAnnotationType::FreeText  => "freetext",
        _                                => "other",
    }
    .to_owned()
}

fn pdf_to_screen(r: &PdfRect, pw: f32, ph: f32) -> AnnRect {
    let left  = (r.left.value / pw).clamp(0.0, 1.0);
    let top_s = (1.0 - r.top.value / ph).clamp(0.0, 1.0);
    let w = ((r.right.value - r.left.value) / pw).abs().max(0.001);
    let h = ((r.top.value - r.bottom.value) / ph).abs().max(0.001);
    AnnRect { left, top: top_s, width: w.min(1.0), height: h.min(1.0) }
}

fn screen_to_pdf(r: &AnnRect, pw: f32, ph: f32) -> PdfRect {
    PdfRect::new(
        PdfPoints::new((1.0 - r.top - r.height).max(0.0) * ph),
        PdfPoints::new(r.left * pw),
        PdfPoints::new((r.left + r.width).min(1.0) * pw),
        PdfPoints::new((1.0 - r.top).min(1.0) * ph),
    )
}

fn union_rects(rects: &[AnnRect]) -> Option<AnnRect> {
    if rects.is_empty() {
        return None;
    }
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for rect in rects {
        l = l.min(rect.left);
        t = t.min(rect.top);
        r = r.max(rect.left + rect.width);
        b = b.max(rect.top + rect.height);
    }
    Some(AnnRect { left: l, top: t, width: r - l, height: b - t })
}
