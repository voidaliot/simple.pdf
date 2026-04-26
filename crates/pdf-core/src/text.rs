use crate::{Document, PdfError, PdfResult};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextSpan {
    pub text: String,
    /// Left edge, normalized [0, 1] relative to page width.
    pub left: f32,
    /// Top edge, normalized [0, 1] relative to page height (0 = page top).
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Document {
    /// Extract word-level text spans with normalized bounding boxes for one page.
    pub fn page_text_spans(&self, page_index: u32) -> PdfResult<Vec<TextSpan>> {
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
            if pw <= 0.0 || ph <= 0.0 {
                return Ok(vec![]);
            }

            let text = page.text().map_err(|e| PdfError::Render(e.to_string()))?;
            let chars = text.chars();
            let n = chars.len();

            let mut spans: Vec<TextSpan> = Vec::new();
            let mut word = String::new();
            // PDF coordinate system: y increases upward from bottom-left.
            let mut wl = 0f32;
            let mut wr = 0f32;
            let mut wt = 0f32; // max y in PDF coords (= visual top of word)
            let mut wb = 0f32; // min y in PDF coords (= visual bottom of word)

            for i in 0..n {
                let ch = match chars.get(i) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let uc = ch.unicode_char();
                let is_ws = uc.map(|c| c.is_whitespace()).unwrap_or(true);

                if is_ws {
                    if !word.is_empty() {
                        flush(&mut spans, &word, wl, wr, wt, wb, pw, ph);
                        word.clear();
                    }
                    continue;
                }

                let b = match ch.loose_bounds() {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                let bl = b.left().value;
                let br = b.right().value;
                let bt = b.top().value;
                let bb = b.bottom().value;

                if word.is_empty() {
                    wl = bl;
                    wr = br;
                    wt = bt;
                    wb = bb;
                } else {
                    wl = wl.min(bl);
                    wr = wr.max(br);
                    wt = wt.max(bt);
                    wb = wb.min(bb);
                }

                if let Some(c) = uc {
                    word.push(c);
                }
            }
            if !word.is_empty() {
                flush(&mut spans, &word, wl, wr, wt, wb, pw, ph);
            }

            Ok(spans)
        })
    }
}

fn flush(
    spans: &mut Vec<TextSpan>,
    text: &str,
    wl: f32,
    wr: f32,
    wt: f32,
    wb: f32,
    pw: f32,
    ph: f32,
) {
    let left = wl / pw;
    // Convert from PDF y-up to screen y-down: top_screen = 1 - top_pdf/ph
    let top = 1.0 - wt / ph;
    let width = ((wr - wl).abs() / pw).max(0.001);
    let height = ((wt - wb).abs() / ph).max(0.001);
    // Clamp to [0, 1]
    if left >= 0.0 && top >= 0.0 && left + width <= 1.01 && top + height <= 1.01 {
        spans.push(TextSpan {
            text: text.to_owned(),
            left: left.clamp(0.0, 1.0),
            top: top.clamp(0.0, 1.0),
            width: width.min(1.0 - left.clamp(0.0, 1.0)),
            height: height.min(1.0 - top.clamp(0.0, 1.0)),
        });
    }
}
