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

            let mut result = Vec::with_capacity(count);
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
                result.push(Annotation {
                    index: i as u32,
                    kind,
                    rect,
                    color,
                    contents,
                    author,
                });
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
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            create_markup_annotation(
                page.annotations_mut(),
                MarkupKind::Highlight,
                rects,
                PdfColor::new(color[0], color[1], color[2], alpha),
                pw,
                ph,
            )
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
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            create_markup_annotation(
                page.annotations_mut(),
                MarkupKind::Underline,
                rects,
                PdfColor::new(color[0], color[1], color[2], 200),
                pw,
                ph,
            )
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
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);
            create_markup_annotation(
                page.annotations_mut(),
                MarkupKind::Strikeout,
                rects,
                PdfColor::new(color[0], color[1], color[2], 200),
                pw,
                ph,
            )
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
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);

            let rect = PdfRect::new(
                PdfPoints::new((1.0 - top - 0.04) * ph),
                PdfPoints::new(left * pw),
                PdfPoints::new((left + 0.03) * pw),
                PdfPoints::new((1.0 - top) * ph),
            );

            let title = author.unwrap_or("Note");
            let annots = page.annotations_mut();
            {
                let mut sticky = annots
                    .create_text_annotation(title)
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
            }
            Ok(annots.len().saturating_sub(1) as u32)
        })
    }

    pub fn add_ink_annotation(
        &self,
        page_index: u32,
        paths: &[Vec<[f32; 2]>],
        color: [u8; 3],
        _width: f32,
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            let (pw, ph) = page_dims(&page);

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

            let annots = page.annotations_mut();
            {
                let mut ink = annots
                    .create_ink_annotation()
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                ink.set_stroke_color(PdfColor::new(color[0], color[1], color[2], 255))
                    .map_err(|e| PdfError::Render(e.to_string()))?;

                if xmin < f32::MAX {
                    let rect = PdfRect::new(
                        PdfPoints::new((1.0 - ymax) * ph),
                        PdfPoints::new(xmin * pw),
                        PdfPoints::new(xmax * pw),
                        PdfPoints::new((1.0 - ymin) * ph),
                    );
                    let _ = ink.set_bounds(rect);
                }
            }
            Ok(annots.len().saturating_sub(1) as u32)
        })
    }

    pub fn remove_annotation(&self, page_index: u32, annot_index: u32) -> PdfResult<()> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            // Get the annotation object (borrows 'a = library lifetime, not &page).
            // The temporary &PdfPageAnnotations borrow is released after .get() returns.
            let annot = page
                .annotations()
                .get(annot_index as usize)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            // Now mutably borrow annotations to delete — 'a is shared (library), no conflict.
            page.annotations_mut()
                .delete_annotation(annot)
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

// ── Helpers ────────────────────────────────────────────────────────────────────

enum MarkupKind { Highlight, Underline, Strikeout }

fn create_markup_annotation(
    annots: &mut PdfPageAnnotations<'_>,
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
        MarkupKind::Strikeout => {
            let mut so = annots
                .create_strikeout_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_bounds(pdf_rect).map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_fill_color(color).map_err(|e| PdfError::Render(e.to_string()))?;
        }
    }
    Ok(annots.len().saturating_sub(1) as u32)
}

fn page_dims(page: &PdfPage<'_>) -> (f32, f32) {
    (page.width().value, page.height().value)
}

fn kind_str(t: PdfPageAnnotationType) -> String {
    match t {
        PdfPageAnnotationType::Highlight => "highlight",
        PdfPageAnnotationType::Underline => "underline",
        PdfPageAnnotationType::Squiggly  => "squiggly",
        PdfPageAnnotationType::Strikeout => "strikeout",
        PdfPageAnnotationType::Text      => "text",
        PdfPageAnnotationType::Ink       => "ink",
        PdfPageAnnotationType::Link      => "link",
        PdfPageAnnotationType::Widget    => "widget",
        PdfPageAnnotationType::Stamp     => "stamp",
        PdfPageAnnotationType::FreeText  => "freetext",
        _                                => "other",
    }
    .to_owned()
}

fn pdf_to_screen(r: &PdfRect, pw: f32, ph: f32) -> AnnRect {
    let left  = (r.left().value / pw).clamp(0.0, 1.0);
    let top_s = (1.0 - r.top().value / ph).clamp(0.0, 1.0);
    let w = ((r.right().value - r.left().value) / pw).abs().max(0.001);
    let h = ((r.top().value - r.bottom().value) / ph).abs().max(0.001);
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
    if rects.is_empty() { return None; }
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for rect in rects {
        l = l.min(rect.left);
        t = t.min(rect.top);
        r = r.max(rect.left + rect.width);
        b = b.max(rect.top + rect.height);
    }
    Some(AnnRect { left: l, top: t, width: r - l, height: b - t })
}
