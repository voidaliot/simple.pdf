use crate::{Document, PdfError, PdfResult, ResidentMemoryLease};
use std::sync::Arc;
use std::time::Duration;

/// Resident text/search data is bounded process-wide, independently from owned
/// source PDFs and transient render bitmaps. Background warming stops at the
/// soft limit, leaving headroom for a foreground search to reach the hard cap.
const SOFT_PREWARM_TEXT_CACHE_BYTES: usize = 96 * 1024 * 1024;
const MAX_TEXT_CACHE_BYTES: usize = 128 * 1024 * 1024;

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

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct SearchRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct SearchMatch {
    pub page_index: u32,
    /// One rectangle per word fragment covered by this occurrence. Keeping
    /// them separate avoids highlighting a large empty area for a multi-line
    /// phrase.
    pub rects: Vec<SearchRect>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
}

#[derive(Debug)]
struct SearchRun {
    start: usize,
    end: usize,
    span_index: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CacheStatus {
    Hit,
    Inserted,
    Uncached,
}

/// Immutable text/search data for one page. An `Arc` to this value is placed
/// in the document cache only after extraction has completed successfully.
pub(crate) struct CachedTextPage {
    spans: Vec<TextSpan>,
    normalized_text: String,
    runs: Vec<SearchRun>,
    // Returning/dropping a page Arc automatically returns its bytes to the
    // shared process budget, even if that Arc ever outlives its Document.
    _lease: Option<ResidentMemoryLease>,
}

impl CachedTextPage {
    fn new(spans: Vec<TextSpan>) -> Self {
        let (normalized_text, runs) = build_search_projection(&spans);
        Self {
            spans,
            normalized_text,
            runs,
            _lease: None,
        }
    }

    fn append_matches(
        &self,
        page_index: u32,
        query: &str,
        limit: usize,
        output: &mut Vec<SearchMatch>,
    ) -> bool {
        let mut cursor = 0;

        while cursor <= self.normalized_text.len() {
            let Some(relative_start) = self.normalized_text[cursor..].find(query) else {
                break;
            };
            let start = cursor + relative_start;
            let end = start + query.len();

            let first_run = self.runs.partition_point(|run| run.end <= start);
            let mut rects = Vec::new();
            let mut previous_span = None;
            for run in &self.runs[first_run..] {
                if run.start >= end {
                    break;
                }
                if run.end > start && previous_span != Some(run.span_index) {
                    let span = &self.spans[run.span_index];
                    rects.push(SearchRect {
                        left: span.left,
                        top: span.top,
                        width: span.width,
                        height: span.height,
                    });
                    previous_span = Some(run.span_index);
                }
            }

            // A match should always overlap at least one text run. Be
            // defensive around malformed/empty PDF text and omit it if not.
            if !rects.is_empty() {
                output.push(SearchMatch { page_index, rects });
                if output.len() >= limit {
                    return true;
                }
            }

            // Non-overlapping occurrences match the behaviour users expect
            // from browser/PDF find controls and bound the work for strings
            // such as "aaaaaaaa".
            cursor = end.max(start + 1);
        }

        false
    }

    fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(
                self.spans
                    .capacity()
                    .saturating_mul(std::mem::size_of::<TextSpan>()),
            )
            .saturating_add(self.spans.iter().fold(0usize, |total, span| {
                total.saturating_add(span.text.capacity())
            }))
            .saturating_add(self.normalized_text.capacity())
            .saturating_add(
                self.runs
                    .capacity()
                    .saturating_mul(std::mem::size_of::<SearchRun>()),
            )
    }
}

impl Document {
    /// Extract word-level text spans with normalized bounding boxes for one
    /// page. The immutable result is cached for the document lifetime; repeat
    /// calls do not re-open a Pdfium page or text-page handle.
    pub fn page_text_spans(&self, page_index: u32) -> PdfResult<Vec<TextSpan>> {
        Ok(self.cached_text_page(page_index)?.0.spans.clone())
    }

    /// Warms one page of the text cache. Intended for low-priority background
    /// indexing; each call enters and leaves the process-wide Pdfium gate on
    /// its own, allowing viewport renders to run between pages.
    pub fn preload_text_page(&self, page_index: u32) -> PdfResult<bool> {
        self.cached_text_page_with_limit(page_index, SOFT_PREWARM_TEXT_CACHE_BYTES)
            .map(|(_, status)| status != CacheStatus::Uncached)
    }

    /// Searches cached normalized text and extracts any missing page cache on
    /// demand. Search is case-insensitive, whitespace-normalized, phrase-aware
    /// across word spans, and joins words split by a line-ending hyphen.
    pub fn search_document(
        &self,
        query: &str,
        max_results: usize,
        generation: u64,
    ) -> PdfResult<SearchResults> {
        let query = normalize_query(query);
        if query.is_empty() || max_results == 0 {
            return Ok(SearchResults {
                matches: Vec::new(),
                truncated: false,
            });
        }

        let mut matches = Vec::new();
        for page_index in 0..self.page_count {
            // Closing a tab cancels both background prewarming and a search
            // whose frontend consumer has gone away. The partial result is
            // marked truncated and will normally be discarded by the caller.
            if self.background_work_cancelled() || !self.search_is_current(generation) {
                return Ok(SearchResults {
                    matches,
                    truncated: true,
                });
            }

            let (page, cache_status) = self.cached_text_page(page_index)?;
            if page.append_matches(page_index, &query, max_results, &mut matches) {
                return Ok(SearchResults {
                    matches,
                    truncated: true,
                });
            }

            // A foreground render waiting on PDFIUM_GATE should get a chance
            // between pages that this search had to extract. Cache-only search
            // remains a tight native string scan.
            if cache_status != CacheStatus::Hit {
                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        Ok(SearchResults {
            matches,
            truncated: false,
        })
    }

    fn cached_text_page(&self, page_index: u32) -> PdfResult<(Arc<CachedTextPage>, CacheStatus)> {
        self.cached_text_page_with_limit(page_index, MAX_TEXT_CACHE_BYTES)
    }

    fn cached_text_page_with_limit(
        &self,
        page_index: u32,
        cache_limit: usize,
    ) -> PdfResult<(Arc<CachedTextPage>, CacheStatus)> {
        let cache = self
            .text_cache
            .get(page_index as usize)
            .ok_or(PdfError::InvalidPage(page_index))?;
        let mut cached = cache.lock();
        if let Some(page) = cached.as_ref() {
            return Ok((Arc::clone(page), CacheStatus::Hit));
        }

        // Keep this per-page cache lock while extracting. Concurrent search,
        // viewport, and background requests for the same page then share one
        // native extraction instead of queuing duplicate Pdfium work.
        let spans = self.extract_page_text_spans(page_index)?;
        let mut page = CachedTextPage::new(spans);
        let estimated_bytes = page.estimated_bytes();
        if let Some(lease) = self
            .text_cache_budget
            .try_reserve(estimated_bytes, cache_limit)
        {
            page._lease = Some(lease);
            let page = Arc::new(page);
            *cached = Some(Arc::clone(&page));
            Ok((page, CacheStatus::Inserted))
        } else {
            Ok((Arc::new(page), CacheStatus::Uncached))
        }
    }

    pub(crate) fn clear_text_cache(&mut self) {
        for page in &mut self.text_cache {
            drop(page.get_mut().take());
        }
    }

    fn extract_page_text_spans(&self, page_index: u32) -> PdfResult<Vec<TextSpan>> {
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
            let mut bounds: Option<(f32, f32, f32, f32)> = None;
            let mut previous_char_bounds: Option<(f32, f32, f32, f32)> = None;

            for i in 0..n {
                let ch = match chars.get(i) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let uc = ch.unicode_char();
                let is_ws = uc.map(|c| c.is_whitespace()).unwrap_or(true);

                if is_ws {
                    flush(&mut spans, &mut word, &mut bounds, (pw, ph));
                    previous_char_bounds = None;
                    continue;
                }

                let char_bounds = ch.loose_bounds().ok().map(|b| {
                    (
                        b.left().value,
                        b.right().value,
                        b.top().value,
                        b.bottom().value,
                    )
                });

                // Some generated PDFs omit an explicit newline character
                // between text objects. Split on a clear visual line change
                // so search/highlight rectangles do not span unrelated rows.
                if let (Some(previous), Some(current)) = (previous_char_bounds, char_bounds) {
                    if should_split_visual_word(previous, current) {
                        flush(&mut spans, &mut word, &mut bounds, (pw, ph));
                    }
                }

                // Keep the Unicode character even if Pdfium cannot provide a
                // rectangle for that individual glyph; neighbouring glyphs
                // still provide a useful word highlight and searchable text.
                if let Some(c) = uc {
                    word.push(c);
                }

                if let Some((left, right, top, bottom)) = char_bounds {
                    bounds = Some(match bounds {
                        Some((wl, wr, wt, wb)) => {
                            (wl.min(left), wr.max(right), wt.max(top), wb.min(bottom))
                        }
                        None => (left, right, top, bottom),
                    });
                    previous_char_bounds = char_bounds;
                }
            }
            flush(&mut spans, &mut word, &mut bounds, (pw, ph));

            Ok(spans)
        })
    }
}

fn should_split_visual_word(previous: (f32, f32, f32, f32), current: (f32, f32, f32, f32)) -> bool {
    let previous_mid = (previous.2 + previous.3) * 0.5;
    let current_mid = (current.2 + current.3) * 0.5;
    let previous_height = (previous.2 - previous.3).abs();
    let current_height = (current.2 - current.3).abs();
    if (previous_mid - current_mid).abs() > previous_height.max(current_height) * 0.75 {
        return true;
    }

    // Some PDFs position glyphs individually and omit literal space
    // characters. Detect a gap in either writing direction relative to the
    // neighbouring glyph widths, while allowing normal kerning/letterspacing.
    let previous_width = (previous.1 - previous.0).abs();
    let current_width = (current.1 - current.0).abs();
    let forward_gap = current.0 - previous.1;
    let reverse_gap = previous.0 - current.1;
    forward_gap.max(reverse_gap) > previous_width.max(current_width) * 0.75
}

fn flush(
    spans: &mut Vec<TextSpan>,
    word: &mut String,
    bounds: &mut Option<(f32, f32, f32, f32)>,
    page_size: (f32, f32),
) {
    let Some((wl, wr, wt, wb)) = bounds.take() else {
        word.clear();
        return;
    };
    if word.is_empty() {
        return;
    }

    let (pw, ph) = page_size;
    let left = wl / pw;
    // Convert from PDF y-up to screen y-down: top_screen = 1 - top_pdf/ph
    let top = 1.0 - wt / ph;
    let width = ((wr - wl).abs() / pw).max(0.001);
    let height = ((wt - wb).abs() / ph).max(0.001);
    // Clamp to [0, 1]. Invalid off-page geometry must not poison the text
    // overlay, but the rest of the page remains searchable.
    if left >= 0.0 && top >= 0.0 && left + width <= 1.01 && top + height <= 1.01 {
        spans.push(TextSpan {
            text: std::mem::take(word),
            left: left.clamp(0.0, 1.0),
            top: top.clamp(0.0, 1.0),
            width: width.min(1.0 - left.clamp(0.0, 1.0)),
            height: height.min(1.0 - top.clamp(0.0, 1.0)),
        });
    } else {
        word.clear();
    }
}

fn build_search_projection(spans: &[TextSpan]) -> (String, Vec<SearchRun>) {
    let mut text = String::new();
    let mut runs: Vec<SearchRun> = Vec::with_capacity(spans.len());
    let mut previous_span_index = None;

    for (span_index, span) in spans.iter().enumerate() {
        let token = normalize_fragment(&span.text);
        if token.is_empty() {
            continue;
        }

        if let Some(previous_index) = previous_span_index {
            let previous = &spans[previous_index];
            let new_line = visual_line_changed(previous, span);
            let trailing_soft_hyphen = previous.text.trim_end().ends_with('\u{00ad}');
            let trailing_visible_hyphen = text.chars().next_back().is_some_and(is_hyphenation_char);

            if new_line && (trailing_soft_hyphen || trailing_visible_hyphen) {
                if trailing_visible_hyphen {
                    remove_last_char(&mut text);
                    if let Some(run) = runs.last_mut() {
                        run.end = text.len();
                        if run.end == run.start {
                            runs.pop();
                        }
                    }
                }
            } else {
                text.push(' ');
            }
        }

        let start = text.len();
        text.push_str(&token);
        runs.push(SearchRun {
            start,
            end: text.len(),
            span_index,
        });
        previous_span_index = Some(span_index);
    }

    (text, runs)
}

fn normalize_fragment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.chars() {
        if ch == '\u{00ad}' {
            continue;
        }
        if ch.is_whitespace() {
            pending_space = !output.is_empty();
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.extend(ch.to_lowercase());
    }
    output
}

fn normalize_query(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.trim().chars() {
        if ch == '\u{00ad}' {
            continue;
        }
        if ch.is_whitespace() {
            if matches!(ch, '\r' | '\n')
                && output.chars().next_back().is_some_and(is_hyphenation_char)
            {
                remove_last_char(&mut output);
                pending_space = false;
            } else {
                pending_space = !output.is_empty();
            }
            continue;
        }
        if pending_space {
            output.push(' ');
            pending_space = false;
        }
        output.extend(ch.to_lowercase());
    }
    output
}

fn visual_line_changed(previous: &TextSpan, current: &TextSpan) -> bool {
    let previous_mid = previous.top + previous.height * 0.5;
    let current_mid = current.top + current.height * 0.5;
    (previous_mid - current_mid).abs() > previous.height.max(current.height) * 0.75
}

fn is_hyphenation_char(ch: char) -> bool {
    matches!(ch, '-' | '\u{00ad}' | '\u{2010}' | '\u{2011}')
}

fn remove_last_char(value: &mut String) {
    if let Some((index, _)) = value.char_indices().next_back() {
        value.truncate(index);
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_query, CachedTextPage, SearchMatch, SearchRect, TextSpan};

    fn span(text: &str, left: f32, top: f32) -> TextSpan {
        TextSpan {
            text: text.into(),
            left,
            top,
            width: 0.1,
            height: 0.02,
        }
    }

    #[test]
    fn query_normalization_collapses_whitespace_and_case() {
        assert_eq!(normalize_query("  QUICK\tBrown  Fox "), "quick brown fox");
        assert_eq!(normalize_query("inter-\noperation"), "interoperation");
        assert_eq!(normalize_query("co\u{00ad}operate"), "cooperate");
    }

    #[test]
    fn phrase_search_returns_each_covered_word_rect() {
        let page = CachedTextPage::new(vec![
            span("Alpha", 0.1, 0.1),
            span("beta", 0.3, 0.1),
            span("gamma", 0.5, 0.1),
        ]);
        let mut matches = Vec::new();
        assert!(!page.append_matches(4, "alpha beta", 10, &mut matches));
        assert_eq!(
            matches,
            vec![SearchMatch {
                page_index: 4,
                rects: vec![
                    SearchRect {
                        left: 0.1,
                        top: 0.1,
                        width: 0.1,
                        height: 0.02,
                    },
                    SearchRect {
                        left: 0.3,
                        top: 0.1,
                        width: 0.1,
                        height: 0.02,
                    },
                ],
            }]
        );
    }

    #[test]
    fn visual_line_hyphenation_is_searchable_as_one_word() {
        let page = CachedTextPage::new(vec![span("reli-", 0.7, 0.10), span("able", 0.1, 0.14)]);
        let mut matches = Vec::new();
        assert!(!page.append_matches(0, "reliable", 10, &mut matches));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rects.len(), 2);
    }

    #[test]
    fn result_limit_marks_search_as_truncated() {
        let page = CachedTextPage::new(vec![span("one one one", 0.1, 0.1)]);
        let mut matches = Vec::new();
        assert!(page.append_matches(0, "one", 2, &mut matches));
        assert_eq!(matches.len(), 2);
    }
}
