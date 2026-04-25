# simple.pdf — Progress Tracker

This file tracks implementation progress against the requirements in [requirements.md](requirements.md). Each milestone below lists concrete tasks; completed tasks are checked and flipped to `[x]`. Requirement IDs in parentheses link tasks to the authoritative requirement in `requirements.md` — tick the requirement there when the task lands.

**Status legend:** `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked

---

## Milestones

### M0 — Skeleton ✓ (2026-04-24, tag `m0-skeleton`)

- [x] Cargo workspace (`Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`)
- [x] `crates/pdf-core` — pdfium-render wired, `open_document()` returns page count
- [x] `crates/shared-types` — ts-rs emitter set up
- [x] `crates/app` — Tauri 2 binary with single window
- [x] `frontend/` — Svelte 5 + Vite + TypeScript scaffold; svelte-check 0 errors
- [ ] `resources/pdfium/pdfium.dll` downloaded, bundled next to exe in release
- [ ] IPC ping round-trip working (`invoke("version")`) — needs `cargo tauri dev`
- [x] `.gitignore`, `README.md`, license chosen
- [x] `git tag m0-skeleton`
- Note: `scripts/fetch-pdfium.ps1` provided; Rust toolchain install needed to build

### M1 — Render [~] (2026-04-25)

- [x] File → Open opens a PDF (FR-VIEW-01) — dialog + `open_document` IPC command
- [x] Page rendering: PDFium PNG → `<img>` via custom `pdf://` URI scheme (FR-VIEW-02)
- [x] Virtualized scroll — IntersectionObserver renders ±1 pages around viewport (NFR-PERF-02)
- [x] Zoom: fit-width / fit-page / custom % / Ctrl+Wheel (FR-VIEW-05, UX-ZOOM-01)
- [x] Toolbar: page-number input, zoom controls, doc title
- [ ] Text layer overlay for selection and copy (FR-VIEW-03) — next iteration
- [ ] Find-in-page (Ctrl+F) with next/prev and highlight-all (FR-VIEW-04) — next iteration
- [ ] Rotate view 90° / 180° / 270° (FR-VIEW-06) — next iteration
- Note: awaiting `cargo tauri dev` verification once Rust toolchain installed

### M2 — Tabs & Home

- [ ] Browser-style tab bar (FR-TAB-01)
- [ ] `+` button opens home tab
- [ ] Home: thumbnail grid of recents (FR-HOME-01)
- [ ] Pin/unpin recents; pinned section on top (FR-HOME-02)
- [ ] Live-filter search over recents (FR-HOME-03)
- [ ] Quick actions: open file, open folder, paste URL (FR-HOME-04)
- [ ] Right-click: remove from recents / reveal in Explorer (FR-HOME-05)
- [ ] Keyboard shortcuts — Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+O, Ctrl+F, Ctrl+S (UX-KBD-01)
- [ ] Drag to reorder tabs (FR-TAB-03)
- [ ] Middle-click closes tab (FR-TAB-04)
- [ ] Dirty-tab close confirmation (FR-TAB-02)

### M3 — Annotations (read)

- [ ] Parse existing annotations on page load
- [ ] Render highlight / underline / strike / sticky / ink from existing PDFs
- [ ] Comments sidebar: list + jump-to-annotation

### M4 — Annotations (write)

- [ ] Highlight tool (FR-ANN-01)
- [ ] Underline tool (FR-ANN-02)
- [ ] Strikethrough tool (FR-ANN-03)
- [ ] Sticky note tool with author + timestamp (FR-ANN-04)
- [ ] Freehand ink tool with `perfect-freehand` smoothing (FR-ANN-05)
- [ ] Edit / delete authored annotations (FR-ANN-06)
- [ ] Undo / redo (Ctrl+Z / Ctrl+Y) (FR-ANN-07)
- [ ] Save via `FPDF_SaveAsCopy` — temp file + atomic rename (SEC-SAVE-01)
- [ ] All written annotations include `FPDFAnnot_SetAP` appearance stream

### M5 — Forms (AcroForms)

- [ ] Enumerate form fields (text / check / radio / combo / list) (FR-FORM-01)
- [ ] Overlay positioned HTML inputs matching field geometry
- [ ] Write values back + regenerate appearances (FR-FORM-02)
- [ ] Detect `/XFA` and show warning banner (FR-FORM-03)
- [ ] Form reset support where PDF provides it (FR-FORM-04)

### M6 — Signing

- [ ] Signature capture modal — draw on canvas or import PNG/JPG (FR-SIGN-01)
- [ ] Place signature as stamp annotation with AP stream (FR-SIGN-02)
- [ ] Manage saved signatures — list / set default / delete (FR-SIGN-03)

### M7 — Integration

- [ ] CLI arg handling: `simple.pdf.exe file.pdf` opens file
- [ ] `tauri-plugin-single-instance` forwards second-launch args to running window
- [ ] Drag-drop PDFs onto window opens them as tabs
- [ ] Optional per-user `.pdf` association, HKCU only (DIST-ASSOC-01)

### M8 — Polish & Packaging

- [ ] Theme: auto-follow Windows + manual override persisted (UX-THEME-01)
- [ ] Settings page: theme, recents limit, associations, portable/roaming data folder
- [ ] Visible focus ring + full keyboard reachability (UX-A11Y-01)
- [ ] Portable zip build pipeline (exe + pdfium.dll + icons) (DIST-PORT-01)
- [ ] NSIS installer build pipeline (DIST-INST-01)
- [ ] WebView2 missing-runtime detection + install link (DIST-WV2-01)
- [ ] Perf pass: meet NFR-PERF-01, NFR-PERF-02, NFR-SIZE-01, NFR-MEM-01
- [ ] Local-only crash reporter (SEC-LOG-01)

---

## Verification gates

Each of these must pass before v1 is declared done. See `requirements.md` for per-item criteria.

- [ ] Clean-Win10-22H2 boot test: portable zip → double-click → app opens in < 1.5 s
- [ ] Fixture corpus: all 20 PDFs in `tests/fixtures/` render without crash
- [ ] Annotation round-trip: add one of each type → save → reopen → identical
- [ ] Compatibility: round-tripped file opens identically in Adobe Reader DC and Edge
- [ ] Forms round-trip: fill all field types → save → reopen in Adobe Reader → values preserved
- [ ] Signature flow: draw → place → save → reopen → stamp visible
- [ ] Tab persistence: open 10 PDFs → close → relaunch → all listed in recents with thumbs
- [ ] Association: enable in settings → double-click .pdf → opens as new tab in running window
- [ ] Size gate: `dist/portable` total ≤ 20 MB
- [ ] Perf gate: `hyperfine` cold-start ≤ 1.5 s median; working set ≤ 200 MB with 50 MB PDF

---

## Changelog

- 2026-04-24 — Project kicked off. Plan, requirements, and M0 skeleton scaffolded (Cargo workspace, Svelte 5 frontend, Tauri 2 app).
- 2026-04-25 — M1 render pipeline: `pdf://` URI scheme, PNG render via PDFium, IntersectionObserver virtualized scroll, fit-width/fit-page/custom zoom, toolbar. svelte-check 0 errors 0 warnings.
