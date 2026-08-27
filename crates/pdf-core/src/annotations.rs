use crate::navigation::link_target;
use crate::{Document, LinkTarget, PdfError, PdfResult};
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};

/// Removes an incomplete save artifact on every error path. On success the
/// rename has already consumed the path, so the final removal is a no-op.
struct TemporarySave(PathBuf);

impl Drop for TemporarySave {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

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
    /// Click destination for link annotations. `None` for annotations without
    /// a supported local-page or URI action.
    pub link_target: Option<LinkTarget>,
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
                // Enumerate links through PdfPage::links() below. That API
                // exposes direct destinations and actions reliably, whereas
                // generic annotation metadata does not for every producer.
                if kind == "link" {
                    continue;
                }
                let rect = annot
                    .bounds()
                    .map(|b| pdf_to_screen(&b, pw, ph))
                    .unwrap_or(AnnRect {
                        left: 0.0,
                        top: 0.0,
                        width: 0.05,
                        height: 0.05,
                    });
                // pdfium-render 0.8.37's annotation color fallback casts an
                // annotation handle to a page-object handle when an appearance
                // stream exists. Read the real appearance object first to avoid
                // that invalid native call (and to report its actual color).
                let annotation_color = match annot.objects().get(0) {
                    Ok(object) => match kind.as_str() {
                        "highlight" => object.fill_color().or_else(|_| object.stroke_color()),
                        "underline" | "squiggly" | "strikeout" | "ink" => {
                            object.stroke_color().or_else(|_| object.fill_color())
                        }
                        _ => object.fill_color().or_else(|_| object.stroke_color()),
                    },
                    Err(_) => match kind.as_str() {
                        "highlight" | "underline" | "squiggly" | "strikeout" | "ink" => {
                            annot.stroke_color()
                        }
                        _ => annot.fill_color(),
                    },
                };
                let color = annotation_color
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
                    link_target: None,
                });
            }

            for (link_index, link) in page.links().iter().enumerate() {
                let Ok(bounds) = link.rect() else {
                    continue;
                };
                let Some(link_target) = link_target(&link) else {
                    continue;
                };
                result.push(Annotation {
                    index: link_index as u32,
                    kind: "link".into(),
                    rect: pdf_to_screen(&bounds, pw, ph),
                    // Link annotations are transparent hit targets; Pdfium
                    // remains the sole renderer of their visual appearance.
                    color: [0, 0, 0, 0],
                    contents: None,
                    author: None,
                    link_target: Some(link_target),
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
                PdfPoints::new((1.0 - top) * ph),
                PdfPoints::new((left + 0.03) * pw),
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
        width: f32,
    ) -> PdfResult<u32> {
        self.with_doc(|doc| {
            let pages = doc.pages();
            let mut page = pages
                .get(page_index as u16)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
            let (pw, ph) = page_dims(&page);

            let paths: Vec<&[[f32; 2]]> = paths
                .iter()
                .filter(|path| path.len() >= 2)
                .map(Vec::as_slice)
                .collect();
            if paths.is_empty() {
                return Err(PdfError::Render("no drawable ink paths supplied".into()));
            }

            let mut xmin = f32::MAX;
            let mut ymin = f32::MAX;
            let mut xmax = f32::MIN;
            let mut ymax = f32::MIN;
            for path in &paths {
                for [x, y] in path.iter() {
                    if !x.is_finite() || !y.is_finite() {
                        return Err(PdfError::Render(
                            "ink path contains a non-finite point".into(),
                        ));
                    }
                    let x = x.clamp(0.0, 1.0);
                    let y = y.clamp(0.0, 1.0);
                    xmin = xmin.min(x);
                    ymin = ymin.min(y);
                    xmax = xmax.max(x);
                    ymax = ymax.max(y);
                }
            }

            let stroke_width = width.clamp(0.5, 72.0);
            let padding_x = stroke_width / (2.0 * pw);
            let padding_y = stroke_width / (2.0 * ph);
            let rect = PdfRect::new(
                PdfPoints::new((1.0 - (ymax + padding_y).min(1.0)) * ph),
                PdfPoints::new((xmin - padding_x).max(0.0) * pw),
                PdfPoints::new((1.0 - (ymin - padding_y).max(0.0)) * ph),
                PdfPoints::new((xmax + padding_x).min(1.0) * pw),
            );

            let annots = page.annotations_mut();
            {
                let mut ink = annots
                    .create_ink_annotation()
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                ink.set_stroke_color(PdfColor::new(color[0], color[1], color[2], 255))
                    .map_err(|e| PdfError::Render(e.to_string()))?;

                // FPDFAnnot_AppendObject generates the annotation appearance stream
                // from these path objects. A bounds-only annotation is an empty box,
                // which was why freehand strokes and signatures vanished. Annotation
                // appearance streams are not page content, so this operation must not
                // run FPDFPage_GenerateContent() after appending each path.
                ink.set_bounds(rect)
                    .map_err(|e| PdfError::Render(e.to_string()))?;

                for points in paths {
                    let [first_x, first_y] = points[0];
                    let mut path = PdfPagePathObject::new(
                        doc,
                        PdfPoints::new(first_x.clamp(0.0, 1.0) * pw),
                        PdfPoints::new((1.0 - first_y.clamp(0.0, 1.0)) * ph),
                        Some(PdfColor::new(color[0], color[1], color[2], 255)),
                        Some(PdfPoints::new(stroke_width)),
                        None,
                    )
                    .map_err(|e| PdfError::Render(e.to_string()))?;
                    path.set_line_cap(PdfPageObjectLineCap::Round)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                    path.set_line_join(PdfPageObjectLineJoin::Round)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                    for [x, y] in points.iter().skip(1) {
                        path.line_to(
                            PdfPoints::new(x.clamp(0.0, 1.0) * pw),
                            PdfPoints::new((1.0 - y.clamp(0.0, 1.0)) * ph),
                        )
                        .map_err(|e| PdfError::Render(e.to_string()))?;
                    }
                    ink.objects_mut()
                        .add_path_object(path)
                        .map_err(|e| PdfError::Render(e.to_string()))?;
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
    pub fn save_to_path(&self, path: &Path) -> PdfResult<()> {
        let parent = path.parent().unwrap_or(Path::new("."));
        let tmp_name = format!(".simplepdf_{}.tmp", uuid::Uuid::new_v4().as_simple());
        let tmp = parent.join(&tmp_name);
        let _cleanup = TemporarySave(tmp.clone());

        self.with_doc(|doc| {
            doc.save_to_file(&tmp)
                .map_err(|e| PdfError::Render(e.to_string()))
        })?;

        // Pdfium has closed its output handle and released the process-wide
        // native gate. Flush and replace outside that critical section so
        // disk latency cannot hold up rendering in another tab.
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&tmp)?
            .sync_all()?;
        std::fs::rename(&tmp, path).map_err(PdfError::Io)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

enum MarkupKind {
    Highlight,
    Underline,
    Strikeout,
}

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
    let quad_points: Vec<PdfQuadPoints> = rects
        .iter()
        .map(|rect| screen_to_pdf_quad_points(rect, pw, ph))
        .collect();

    match kind {
        MarkupKind::Highlight => {
            let mut hl = annots
                .create_highlight_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            hl.set_bounds(pdf_rect)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            hl.set_stroke_color(color)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            for points in quad_points {
                hl.attachment_points_mut()
                    .create_attachment_point_at_end(points)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
            }
        }
        MarkupKind::Underline => {
            let mut ul = annots
                .create_underline_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            ul.set_bounds(pdf_rect)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            ul.set_stroke_color(color)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            for points in quad_points {
                ul.attachment_points_mut()
                    .create_attachment_point_at_end(points)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
            }
        }
        MarkupKind::Strikeout => {
            let mut so = annots
                .create_strikeout_annotation()
                .map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_bounds(pdf_rect)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            so.set_stroke_color(color)
                .map_err(|e| PdfError::Render(e.to_string()))?;
            for points in quad_points {
                so.attachment_points_mut()
                    .create_attachment_point_at_end(points)
                    .map_err(|e| PdfError::Render(e.to_string()))?;
            }
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
        PdfPageAnnotationType::Squiggly => "squiggly",
        PdfPageAnnotationType::Strikeout => "strikeout",
        PdfPageAnnotationType::Text => "text",
        PdfPageAnnotationType::Ink => "ink",
        PdfPageAnnotationType::Link => "link",
        PdfPageAnnotationType::Widget => "widget",
        PdfPageAnnotationType::Stamp => "stamp",
        PdfPageAnnotationType::FreeText => "freetext",
        _ => "other",
    }
    .to_owned()
}

fn pdf_to_screen(r: &PdfRect, pw: f32, ph: f32) -> AnnRect {
    let left = (r.left().value / pw).clamp(0.0, 1.0);
    let top_s = (1.0 - r.top().value / ph).clamp(0.0, 1.0);
    let w = ((r.right().value - r.left().value) / pw).abs().max(0.001);
    let h = ((r.top().value - r.bottom().value) / ph).abs().max(0.001);
    AnnRect {
        left,
        top: top_s,
        width: w.min(1.0),
        height: h.min(1.0),
    }
}

fn screen_to_pdf(r: &AnnRect, pw: f32, ph: f32) -> PdfRect {
    PdfRect::new(
        PdfPoints::new((1.0 - r.top - r.height).max(0.0) * ph),
        PdfPoints::new(r.left * pw),
        PdfPoints::new((1.0 - r.top).min(1.0) * ph),
        PdfPoints::new((r.left + r.width).min(1.0) * pw),
    )
}

fn screen_to_pdf_quad_points(r: &AnnRect, pw: f32, ph: f32) -> PdfQuadPoints {
    let rect = screen_to_pdf(r, pw, ph);
    // Text-markup QuadPoints use PDFium's Z-order: top-left, top-right,
    // bottom-left, bottom-right. This is intentionally not PdfQuadPoints::from_rect(),
    // whose generic counter-clockwise ordering does not describe text lines.
    PdfQuadPoints::new(
        rect.left(),
        rect.top(),
        rect.right(),
        rect.top(),
        rect.left(),
        rect.bottom(),
        rect.right(),
        rect.bottom(),
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
    Some(AnnRect {
        left: l,
        top: t,
        width: r - l,
        height: b - t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PdfEngine, RenderRequest, PDFIUM_GATE};

    static TEST_GATE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

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
    fn markup_and_ink_are_visible_and_survive_save() {
        let _test_guard = TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("annotations.pdf");
        let dll_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium");
        let engine = PdfEngine::new(&dll_dir).unwrap();
        create_blank_pdf(&engine, &path);

        let document = engine.open(&path).unwrap();
        let render = || {
            document
                .render_page_raw(RenderRequest {
                    page_index: 0,
                    scale: 0.5,
                })
                .unwrap()
                .rgba
        };
        let changed_bytes =
            |left: &[u8], right: &[u8]| left.iter().zip(right).filter(|(a, b)| a != b).count();

        let blank = render();
        document
            .add_highlight(
                0,
                &[AnnRect {
                    left: 0.10,
                    top: 0.10,
                    width: 0.25,
                    height: 0.04,
                }],
                [255, 214, 0],
                0.4,
            )
            .unwrap();
        let highlighted = render();
        assert!(
            changed_bytes(&highlighted, &blank) > 0,
            "highlight must change rendered pixels"
        );

        document
            .add_underline(
                0,
                &[AnnRect {
                    left: 0.10,
                    top: 0.20,
                    width: 0.25,
                    height: 0.04,
                }],
                [255, 0, 0],
            )
            .unwrap();
        let underlined = render();
        assert!(
            changed_bytes(&underlined, &highlighted) > 0,
            "underline must change rendered pixels"
        );

        document
            .add_strikeout(
                0,
                &[AnnRect {
                    left: 0.10,
                    top: 0.30,
                    width: 0.25,
                    height: 0.04,
                }],
                [0, 80, 255],
            )
            .unwrap();
        let struck = render();
        assert!(
            changed_bytes(&struck, &underlined) > 0,
            "strikeout must change rendered pixels"
        );

        document
            .add_ink_annotation(
                0,
                &[
                    vec![[0.45, 0.45], [0.55, 0.50], [0.65, 0.45]],
                    vec![[0.50, 0.52], [0.60, 0.53]],
                ],
                [0, 0, 0],
                3.0,
            )
            .unwrap();
        let ink_object_count = document.with_doc(|doc| {
            doc.pages()
                .get(0)
                .unwrap()
                .annotations()
                .get(3)
                .unwrap()
                .objects()
                .len()
        });
        assert_eq!(
            ink_object_count, 2,
            "each ink stroke needs an appearance path"
        );
        let inked = render();
        let immediate_ink_changes = changed_bytes(&inked, &struck);
        assert!(
            immediate_ink_changes > 0,
            "ink/signature must change rendered pixels immediately"
        );

        document.save_to_path(&path).unwrap();
        drop(document);

        let reopened = engine.open(&path).unwrap();
        let reopened_render = reopened
            .render_page_raw(RenderRequest {
                page_index: 0,
                scale: 0.5,
            })
            .unwrap()
            .rgba;
        let annotations = reopened.page_annotations(0).unwrap();
        assert_eq!(annotations.len(), 4);
        assert_eq!(annotations[0].kind, "highlight");
        assert_eq!(annotations[1].kind, "underline");
        assert_eq!(annotations[2].kind, "strikeout");
        assert_eq!(annotations[3].kind, "ink");
        assert!((annotations[0].rect.left - 0.10).abs() < 0.001);
        assert!((annotations[0].rect.top - 0.10).abs() < 0.001);
        assert!((annotations[0].rect.width - 0.25).abs() < 0.001);
        assert!((annotations[0].rect.height - 0.04).abs() < 0.001);
        let ink_rect = &annotations[3].rect;
        assert!(ink_rect.left <= 0.45);
        assert!(ink_rect.left + ink_rect.width >= 0.65);
        assert!(ink_rect.top <= 0.45);
        assert!(ink_rect.top + ink_rect.height >= 0.53);
        assert!(
            changed_bytes(&reopened_render, &struck) > 0,
            "ink/signature must change rendered pixels (immediate changes: {immediate_ink_changes})"
        );
        assert!(
            changed_bytes(&reopened_render, &blank) > 0,
            "saved annotations must still render after reopening"
        );
    }

    #[test]
    fn resident_document_releases_source_handle_and_renders_ipc_frame() {
        let _test_guard = TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.pdf");
        let moved = temp.path().join("moved.pdf");
        let dll_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium");
        let engine = PdfEngine::new(&dll_dir).unwrap();
        create_blank_pdf(&engine, &source);

        let document = engine.open(&source).unwrap();
        // In particular on Windows, this proves Pdfium is backed by the owned
        // resident byte buffer rather than a still-open source file handle.
        std::fs::rename(&source, &moved).unwrap();

        let frame = document
            .render_page_ipc(RenderRequest {
                page_index: 0,
                scale: 0.1,
            })
            .unwrap();
        let raw = document
            .render_page_raw(RenderRequest {
                page_index: 0,
                scale: 0.1,
            })
            .unwrap();
        assert!(frame.len() >= 8);
        let width = u32::from_le_bytes(frame[0..4].try_into().unwrap()) as usize;
        let height = u32::from_le_bytes(frame[4..8].try_into().unwrap()) as usize;
        assert!(width > 0 && height > 0);
        assert_eq!(frame.len(), 8 + width * height * 4);
        assert_eq!((raw.width as usize, raw.height as usize), (width, height));
        assert_eq!(&frame[8..], raw.rgba.as_slice());
        assert!(frame[8..].chunks_exact(4).all(|pixel| pixel[3] == 255));
    }

    #[test]
    fn failed_atomic_save_removes_its_temporary_file() {
        let _test_guard = TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.pdf");
        let destination_directory = temp.path().join("destination");
        std::fs::create_dir(&destination_directory).unwrap();
        let dll_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium");
        let engine = PdfEngine::new(&dll_dir).unwrap();
        create_blank_pdf(&engine, &source);
        let document = engine.open(&source).unwrap();

        assert!(document.save_to_path(&destination_directory).is_err());
        let leftovers = std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".simplepdf_")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn repeated_open_render_search_close_releases_resident_budgets() {
        let _test_guard = TEST_GATE.lock();
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("soak.pdf");
        let dll_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/pdfium");
        let engine = PdfEngine::new(&dll_dir).unwrap();
        create_blank_pdf(&engine, &source);
        let shared = engine.pdfium.as_ref().unwrap();

        for _ in 0..100 {
            let document = engine.open(&source).unwrap();
            document
                .render_page_ipc(RenderRequest {
                    page_index: 0,
                    scale: 0.05,
                })
                .unwrap();
            document.page_text_spans(0).unwrap();
            assert!(shared.source_bytes_budget.used() > 0);
            assert!(shared.text_cache_budget.used() > 0);
            drop(document);
            assert_eq!(shared.source_bytes_budget.used(), 0);
            assert_eq!(shared.text_cache_budget.used(), 0);
        }
    }
}
