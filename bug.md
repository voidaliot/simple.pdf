# Active bugs (2026-04-27 update)

The previous BGRx + JPEG + LRU cache + wider-prefetch round (PR #3) did not
resolve the user's complaints. New observed issues, in priority order:

---

## Bug 1 — Text/regions still rendered as solid black rectangles

### Symptom
ASPICE PDF still shows large black rectangles where text should appear. The
ToC area and "List of Figures" area both render as opaque black blocks.

### Hypothesis
The remaining culprit is a **flag combination** problem, not a format problem:

- `FPDF_LCD_TEXT` (subpixel AA) requires reading the existing destination pixels
  to do its blending. With our BGRx (no real alpha) + reverse-byte-order
  combination it writes garbage into the page where text should be.
- `FPDF_NO_NATIVETEXT` forces FreeType, which can't decode some embedded
  subset CIDType2 fonts shipped in this PDF. PDFium then renders the fallback
  glyph rectangle as solid filled (this matches the visual pattern).

### Fix
Strip the render config back to the bare minimum that Chrome's PDF viewer
uses on Windows for problematic PDFs:

- Drop `use_lcd_text_rendering` — it's the most fragile flag.
- Drop `disable_native_text_rendering` — let PDFium pick its native path; it
  falls back gracefully for subset fonts.
- Keep `BGRx` + explicit white clear (transparency-group fix from PR #3).

If blackout persists after this strip-back, fall back to `BGR` (3-channel, no
alpha at all) — eliminates any remaining alpha-channel arithmetic.

- [x] Strip render flags to minimum, keep `BGRx` + WHITE clear
- [ ] If still black, switch to `BGR` 3-channel format

---

## Bug 2 — Scrolling slow, app freezes during annotation/comment loading

### Symptom
Scrolling stutters; loading existing annotations on a PDF locks up the UI for
seconds. The user explicitly asked: **no caching, render directly via PDFium,
no clever optimizations.**

### Root cause
1. The LRU cache adds a lock + clone per render — fine in isolation, but it
   hides the real problem: every IPC call (render, text spans, annotations,
   form fields) serializes through the same `Mutex<PdfDocument>`. When 5
   pages become visible the calls queue and the WebView main thread blocks
   waiting for the first one.
2. `loadAnnotations` is fired in an `$effect` per visible page. Combined with
   the wider prefetch (±2 pages × `rootMargin: 1200px`), one fast scroll can
   queue 7+ annotation-fetch IPCs.
3. Each IPC call goes through Tauri's command serializer (JSON), which is
   expensive when the result is a large list of spans/annotations.

### Fix
Per the user's instruction — strip the optimizations:

- [x] Remove the LRU render cache from `pdf-core` (Document struct + lib.rs)
- [x] Remove the `lru` dependency from `Cargo.toml` and `pdf-core/Cargo.toml`
- [x] Remove `with_doc_mutating` helper + all `invalidate_render_cache()` calls
- [x] Reset prefetch range to ±1 page; keep `rootMargin: "1200px"` (helps fast
      scroll without piling up too many concurrent IPCs)
- [ ] Keep render-on-demand simple: no eager whole-PDF prefetch (would freeze
      the UI even harder on large PDFs); but ensure the visible page renders
      before its neighbours by ordering the visible-set update so the entry
      under the viewport center fires its render first.

---

## Bug 3 — Ink/drawing tool draws ~100 px offset from cursor

### Symptom
When using the ink tool, the line is drawn well below and to the right of
where the cursor actually is. The offset scales with `devicePixelRatio`.

### Root cause
[Page.svelte:161-169](frontend/src/components/Page.svelte:161) sets the canvas
backing buffer to `cssW * dpr` device pixels, then applies
`ctx.scale(dpr, dpr)` so subsequent drawing uses CSS pixel coordinates.

But [Page.svelte:149-151](frontend/src/components/Page.svelte:149) draws the
strokes at:
```js
ctx.moveTo(path[0]![0] * inkCanvas!.width,  path[0]![1] * inkCanvas!.height);
```

`inkCanvas.width` is **device** pixels (`cssW * dpr`). After the `ctx.scale`,
that gets multiplied by `dpr` again — strokes land at `nx * cssW * dpr * dpr`
instead of `nx * cssW * dpr`. At dpr=1.5 that's a 1.5× overshoot which looks
like a ~100 px shift on a typical A4 page.

### Fix
Use CSS coordinates after the `ctx.scale(dpr, dpr)` is in effect:
```js
ctx.moveTo(path[0]![0] * cssW,  path[0]![1] * cssH);
```

- [x] Replace `inkCanvas.width` / `inkCanvas.height` in `drawInk` with the
      reactive `cssW` / `cssH` values.
- [x] Compute the clear-rect using device pixels (raw canvas dims) since
      `clearRect` operates pre-transform on canvas pixels.

---

## Bug 4 — Signature placement does nothing visible

### Symptom
After drawing a signature in the modal and clicking "Place on page", the
modal closes, but the signature does not appear on the page. No visual
feedback.

### Root cause(s)
1. **Stale render cache** — the LRU cache returns the old JPEG (without the
   ink annotation) even after `invalidate_render_cache` was called.
   Eliminated by Bug 2's removal of the cache.
2. **The Page component's `$effect` doesn't track annotation changes** —
   the render effect only depends on `docId`, `pageIndex`, `renderScale`.
   When annotations change, `refreshAnnotations` updates `annotsByPage` but
   the page bitmap doesn't re-render (PDFium renders annotations into the
   page bitmap, so the bitmap really must be re-fetched).
3. **`addInkAnnotation` is invoked with normalized [0,1] modal-canvas
   coordinates** — the modal's canvas is 480×200 px, paths are normalized
   to that. When the Rust side stores them as page-normalized coords, the
   signature ends up filling the entire page in PDF coordinate space, and
   may render off-screen or as a giant scribble. The user sees nothing.

### Fix
- [x] After (1) removes the cache, ensure Page re-renders on annotation
      changes by adding a reactive `annotationsVersion` prop that the
      Page's render `$effect` depends on.
- [x] When placing a signature: don't fill the whole page. Place the
      signature near the bottom-right of the current scroll viewport, scaled
      to a sensible default (~30 % page width). Map the modal-canvas paths
      into a target rectangle on the page, then send those page-normalized
      coords to `addInkAnnotation`.

---

## Implementation checklist

1. [x] Bug 2 — Remove LRU cache, lru dep, invalidate calls, with_doc_mutating
2. [x] Bug 1 — Strip render flags to BGRx + WHITE only
3. [x] Bug 3 — Fix ink coordinate math (use cssW/cssH after ctx.scale)
4. [x] Bug 4 — Add `annotationsVersion` prop + signature placement target box
5. [x] Build clean, no warnings introduced

Verification (manual, after restart):
- [ ] ASPICE ToC renders without black rectangles
- [ ] Scroll a 50-page PDF: no UI freezing, pages appear within ~200 ms
- [ ] Ink tool draws exactly under the cursor (no offset)
- [ ] Sticky note placed via "Add note" appears on the page after entering text
- [ ] Drawn signature appears as a visible signature-sized stroke on the page
