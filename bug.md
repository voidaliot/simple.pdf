# Render & scroll bugs — diagnosis and fix plan

Two reported issues:

1. Some PDFs render large regions as solid black (e.g. ASPICE PDF "Table of contents" page — entire ToC area is a black rectangle).
2. Scrolling is laggy compared to Chrome's PDF viewer / Adobe Reader. Pages don't appear instantly when scrolled into view.

The earlier `disable_native_text_rendering(true) + use_lcd_text_rendering(true)` change in [crates/pdf-core/src/render.rs](crates/pdf-core/src/render.rs) does **not** fix the blackout — that change only addresses GDI font failures, but the blackout in the ASPICE PDF is a *transparency-group* rendering bug, not a font bug.

---

## Bug 1 — Black rectangles where text/links should be

### Symptom
On the ASPICE PDF's ToC page, after the "Table of contents" heading, a large rectangular region renders as solid black. The region corresponds to the link/heading group with hyperlinks back to chapters.

### Root cause
PDFium's renderer has a known defect when rendering **transparency groups whose backdrop alpha is undefined**, into a destination bitmap that has an alpha channel (`FPDFBitmap_BGRA`). The group's compositing math collapses to fully-opaque black where the group covers the page.

We currently render with `PdfBitmapFormat::BGRA` (the pdfium-render default — see [pdfium-render's PdfBitmapFormat::default](https://github.com/ajrcarey/pdfium-render/blob/main/src/pdf/bitmap.rs)). Because the page is always displayed against an opaque white background (clear color = WHITE), there is no reason to keep an alpha channel in the output bitmap — and keeping it triggers the bug.

The Chrome PDF viewer and Adobe Reader both render PDF pages into an **opaque** bitmap (no alpha channel) for exactly this reason.

### Fix
In [crates/pdf-core/src/render.rs](crates/pdf-core/src/render.rs):

```rust
let config = PdfRenderConfig::new()
    .set_target_width(px_w)
    .set_target_height(px_h)
    .set_format(PdfBitmapFormat::BGRx)        // ← add: opaque RGB, no alpha
    .set_clear_color(PdfColor::WHITE)         // ← add: explicit, even though it's the default
    .disable_native_text_rendering(true)      // keep
    .use_lcd_text_rendering(true);            // keep
```

`BGRx` is a 32-bit format with the alpha byte ignored. PDFium pre-flattens transparency groups against the clear color when the output has no alpha, which avoids the blackout.

After the bitmap is produced, encode as JPEG (see Bug 2 fix below) — the alpha channel is no longer needed downstream either, so all consumers can drop the alpha path.

### Verification
- ASPICE ToC page renders with visible text and link-rect highlights.
- Other PDFs that have transparency-heavy art (figure overlays, watermarks, redactions) still render correctly — re-test the M1 sample PDFs in `samples/`.

---

## Bug 2 — Scrolling lag (not Chrome-smooth)

### Symptom
Scrolling reveals blank/skeleton placeholders for ~150–500 ms per newly-visible page before the page appears. Chrome and Adobe Reader show pages near-instantly.

### Root causes (multiple, compounding)

1. **PNG encoding on every render.** `image::ImageFormat::Png` runs zlib DEFLATE over a multi-megabyte RGBA buffer. For an A4 page at zoom=1, dpr=1.5 → ~2.5 MP × 4 bytes = 10 MB compressed to PNG, which is ~30–80 ms of pure CPU per page on commit `a197c92`. Chrome doesn't encode anything — it draws straight from PDFium's bitmap into a `<canvas>` via `putImageData`.

2. **base64 over Tauri IPC.** `render_page_b64` returns a base64 string, which Tauri serializes through JSON. base64 inflates the byte size by 33% and the JSON string round-trip adds another copy + UTF-8 validation. For a 1.5 MB PNG that's ~6 MB of string traffic per page.

3. **`data:image/png;base64,...` decoding in the WebView.** Each new `imgSrc` forces WebView2 to decode the PNG on the main thread of the renderer. PNG decoding is single-threaded and significantly slower than copying raw RGBA into a canvas.

4. **All renders serialized through one mutex.** [crates/pdf-core/src/lib.rs:54](crates/pdf-core/src/lib.rs:54) — every page render takes `inner: Arc<Mutex<PdfDocument>>`. PDFium itself is single-threaded per document, so this is correct, but it means scroll-driven prefetch (`idx-1`, `idx`, `idx+1`) cannot run in parallel — pages render strictly sequentially.

5. **No render cache.** Scrolling back to a page that was already rendered at the current zoom triggers a fresh PDFium render. Chrome keeps a page-bitmap LRU cache.

6. **IntersectionObserver `rootMargin: "200px 0px"` is too tight.** A flick-scroll past 200 px reveals pages that haven't started rendering yet. Should be at least 1 viewport (~800 px) ahead.

### Fixes (in priority order)

**P0 — Replace PNG with raw bitmap delivery.**
Two viable paths:
- **(A) Custom URI scheme delivering raw RGBA/JPEG bytes.** Re-introduce a `pdf://` scheme but registered correctly for both dev (HTTP origin) and prod. Body is `Content-Type: image/jpeg` (or raw RGBA + dimensions in headers). The earlier commit `a197c92` removed `pdf://` because of a WebView2 dev-server origin issue — fix the origin issue, don't fall back to base64.
- **(B) Encode as JPEG (quality 85) instead of PNG.** Keeps the IPC base64 path but cuts payload ~10× and encode time ~5×. Acceptable visual quality for screen viewing; matches what Chrome's print preview uses internally. Easiest first step.

Recommendation: do **(B)** first (5-line change), then **(A)** as a follow-up for the last bit of latency.

In [crates/pdf-core/src/render.rs](crates/pdf-core/src/render.rs:76):
```rust
DynamicImage::ImageRgb8(rgb_img)
    .write_to(&mut buf, image::ImageFormat::Jpeg(85))
```
Convert to RGB before encoding (alpha is gone after Bug 1 fix). Update the data URL prefix in [frontend/src/lib/ipc.ts:89](frontend/src/lib/ipc.ts:89) from `image/png` to `image/jpeg`.

**P1 — Add an LRU bitmap cache in `pdf-core`.**
Cache key: `(doc_id, page_index, scale_bucket)`. `scale_bucket` rounds the float scale to ~2 decimals so trivial dpr/zoom drift doesn't miss the cache. Capacity: ~30–50 pages worth of RGBA bytes (configurable, ~150 MB at A4 zoom=1). Use the [`lru`](https://crates.io/crates/lru) crate. Plug into `Document::render_page_png`/`render_page_jpeg`.

**P2 — Widen the IntersectionObserver prefetch window.**
[frontend/src/routes/Viewer.svelte:273](frontend/src/routes/Viewer.svelte:273):
```diff
- { root: container, rootMargin: "200px 0px", threshold: 0.01 }
+ { root: container, rootMargin: "1200px 0px", threshold: 0.01 }
```
And prefetch `idx-2 .. idx+2` instead of `idx-1 .. idx+1` ([Viewer.svelte:266-267](frontend/src/routes/Viewer.svelte:266)).

**P3 — Decouple visible-page render from prefetch render.**
Currently every visible page kicks off a render via `$effect` in [Page.svelte:73](frontend/src/components/Page.svelte:73). Add a small frontend queue that prioritises pages whose center is closest to the viewport center, so the page the user is *actually looking at* renders before its neighbours.

**P4 — Per-page render token to skip stale work.**
While zooming or fast-scrolling, multiple `renderPageB64` calls for the same page can pile up in IPC. Add an in-flight token per page on the Rust side (or cancel the old one on the frontend before issuing a new one) so we don't burn CPU on results that will be discarded.

**P5 — Pre-render at one fixed scale, CSS-zoom from there.**
Render every page at `dpr * 1.0` (i.e. 100 % zoom) and let CSS scale up/down for moderate zoom changes (0.6×–1.6×). Re-render only when zoom moves outside the band. This is exactly what Chrome's viewer does and would make zoom feel instant.

### Verification
- Open a 50-page PDF, scroll fast top-to-bottom: no blank skeleton frames visible after first viewport.
- Scroll back up: pages reappear instantly (cache hits).
- Memory (Task Manager) stays under ~250 MB for typical reading sessions.

---

## Implementation order

1. **Bug 1 fix** — `BGRx` + explicit clear color. ~5 lines, 1 file. Verifies blackout disappears.
2. **Bug 2 / P0(B)** — switch PNG → JPEG. ~10 lines across `render.rs` + `ipc.ts`. Cuts scroll latency immediately.
3. **Bug 2 / P2** — widen prefetch margin + range. ~3 lines in `Viewer.svelte`.
4. **Bug 2 / P1** — LRU bitmap cache in `pdf-core`. ~80 lines, new file `cache.rs`.
5. **Bug 2 / P0(A)** — custom URI scheme for raw bitmap. Larger change to `protocol.rs` + Tauri config.
6. **Bug 2 / P3, P4, P5** — render-priority queue, in-flight cancellation, fixed-scale base render. Follow-ups.
