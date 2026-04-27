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
- [x] `resources/pdfium/pdfium.dll` placed in `resources/pdfium/`
- [~] IPC ping round-trip verified (`invoke("app_version")`) — code complete; run `cargo tauri dev` to confirm
- [x] `.gitignore`, `README.md`, license chosen
- [x] `git tag m0-skeleton`

### M1 — Render [~] (2026-04-25)

- [x] File → Open opens a PDF (FR-VIEW-01) — dialog + `open_document` IPC command
- [x] Page rendering: PDFium PNG → `<img>` via custom `pdf://` URI scheme (FR-VIEW-02)
- [x] Virtualized scroll — IntersectionObserver renders ±1 pages around viewport (NFR-PERF-02)
- [x] Zoom: fit-width / fit-page / custom % / Ctrl+Wheel (FR-VIEW-05, UX-ZOOM-01)
- [x] Toolbar: page-number input, zoom controls, doc title
- [x] Skeleton shimmer while page loads; error overlay on render failure
- [x] First-run smoke test: `cargo tauri dev` → open a PDF → pages render correctly
- [x] Text layer overlay for selection and copy (FR-VIEW-03)
- [x] Find-in-page (Ctrl+F) with next/prev and highlight-all (FR-VIEW-04)
- [x] Rotate view 90° / 180° / 270° (FR-VIEW-06)

### M2 — Tabs & Home [~]

- [x] Browser-style tab bar — `TabBar.svelte` renders tab list, active state, close × (FR-TAB-01)
- [x] `+` button opens home tab — `tabs.openHome()` wired in TabBar
- [x] Middle-click closes tab (FR-TAB-04)
- [x] Drag to reorder tabs — HTML5 drag events wired to `tabs.reorder()` with drop-indicator (FR-TAB-03)
- [x] Global keyboard shortcuts — Ctrl+T, Ctrl+W, Ctrl+Tab/Shift+Tab, Ctrl+O in `App.svelte` (UX-KBD-01)
- [x] Pending files on startup — `drainPending()` on mount + `files-queued` event listener
- [x] Recents persistence — `localStorage`-backed `recents` store (`stores/recents.svelte.ts`)
- [x] Home: recents grid, pinned section first, filter derived from search input (FR-HOME-01..03)
- [x] Pin/unpin via right-click context menu; pinned badge on card (FR-HOME-02)
- [x] Right-click: remove from recents / pin (FR-HOME-05)
- [x] Open file quick action calls shared `lib/open.ts` which adds to recents (FR-HOME-04)
- [x] Home thumbnail — `thumb://` protocol renders page 0; cached as data URL in localStorage
- [x] Open folder quick action — `list_folder_pdfs` command + picker dialog (FR-HOME-04)
- [x] Paste URL quick action — `download_url_to_temp` Rust command via reqwest (FR-HOME-04)
- [x] Reveal in Explorer + Copy path context menu (FR-HOME-05)
- [x] Dirty-tab close confirmation dialog (FR-TAB-02)

### M3 — Annotations (read) ✓

- [x] Parse existing annotations on page load (`get_page_annotations` IPC)
- [x] Render highlight / underline / strike / sticky / ink from existing PDFs (overlays in Page.svelte)
- [x] Comments sidebar: list + jump-to-annotation (collapsible sidebar in Viewer.svelte)

### M4 — Annotations (write) ✓

- [x] Highlight tool (FR-ANN-01)
- [x] Underline tool (FR-ANN-02)
- [x] Strikethrough tool (FR-ANN-03)
- [x] Sticky note tool (FR-ANN-04)
- [x] Freehand ink tool — canvas pointer events (FR-ANN-05)
- [x] Delete annotations — double-click or Delete key (FR-ANN-06)
- [x] Undo Ctrl+Z — per-doc added-annotation stack (FR-ANN-07)
- [x] Save via temp file + atomic rename Ctrl+S (SEC-SAVE-01)
- [~] All written annotations include appearance stream (FPDFAnnot_SetAP) — deferred: not in pdfium-render 0.8.37 bindings; needs ffi.rs + libloading

### M5 — Forms (AcroForms)

- [x] Enumerate form fields (text / check / radio / combo / list) (FR-FORM-01)
- [x] Overlay positioned HTML inputs matching field geometry
- [x] Write values back (text + checkbox); AP regen deferred with AP stream task (FR-FORM-02)
- [x] Detect `/XFA` and show warning banner (FR-FORM-03)
- [ ] Form reset support where PDF provides it (FR-FORM-04)

### M6 — Signing [~]

- [x] Signature capture modal — canvas drawing (FR-SIGN-01)
- [x] Place signature as ink annotation on current page (FR-SIGN-02)
- [x] Manage saved signatures — list / set default / delete (FR-SIGN-03)

### M7 — Integration [~]

- [x] CLI arg handling: `simple.pdf.exe file.pdf` opens file
- [x] `tauri-plugin-single-instance` forwards second-launch args to running window
- [x] Drag-drop PDFs onto window (`tauri://file-drop` listener)
- [x] Optional per-user `.pdf` association, HKCU only (DIST-ASSOC-01)

### M8 — Polish & Packaging [~]

- [x] Theme: auto-follow OS + manual override via `data-theme` + localStorage (UX-THEME-01)
- [x] Settings page: theme selector (FR-TAB-01, expandable)
- [x] Visible focus ring (:focus-visible CSS) + ARIA labels throughout (UX-A11Y-01)
- [x] Portable zip build pipeline (exe + pdfium.dll + icons) (DIST-PORT-01)
- [x] NSIS installer build pipeline (DIST-INST-01)
- [x] WebView2 missing-runtime detection + install link (DIST-WV2-01)
- [ ] Perf pass: meet NFR-PERF-01, NFR-PERF-02, NFR-SIZE-01, NFR-MEM-01
- [x] Local-only crash reporter (SEC-LOG-01)

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
- 2026-04-25 — Rust toolchain installed; `pdfium.dll` placed in `resources/pdfium/`. M0 now buildable. M2 tab bar, Home route, and viewer store partially scaffolded. First `cargo tauri dev` run is the immediate next step.
- 2026-04-25 — M2 batch: recents store (localStorage), `lib/open.ts` shared utility, keyboard shortcuts (Ctrl+T/W/Tab/O), pending-files drain on startup, Home recents grid with filter + pin/unpin context menu, drag-to-reorder tabs. Dev workflow fixed: `pnpm dev` from repo root via `package.json` shim.
- 2026-04-25 — M1 completion: text layer overlay (`get_page_text_spans` IPC + transparent word-level spans in Page.svelte), find-in-page bar (Ctrl+F, Enter/Shift+Enter next/prev, match counter, highlight overlays), rotation 90°/180°/270° (CSS transform with axis-swapping wrapper, fit-width adapts to rotated dims). svelte-check 0 errors.
- 2026-04-25 — M2–M8 batch: thumbnails (`thumb://` protocol + data-URL cache), open folder, paste URL (reqwest), reveal-in-explorer + copy-path, dirty-tab confirm dialog. M3–M4 annotations: read/render existing annotations, write highlight/underline/strikeout/sticky/ink, delete, Ctrl+Z undo, Ctrl+S atomic save. M6 signature canvas. M7 drag-drop. M8 theme store + Settings page + ARIA labels. Deps: tauri-plugin-shell, tauri-plugin-http, reqwest, urlencoding. svelte-check 0 errors 0 warnings.
- 2026-04-26 — M1 smoke test fixed: build.rs now copies pdfium.dll to target/{triple}/{profile}/ so cargo tauri dev works without manual DLL placement. M5 AcroForms: backend get_form_fields/set_field_text_value/set_field_checked commands (pdfium-render form API), frontend form-layer overlay in Page.svelte (text/multiline/checkbox/radio/combo/list inputs), XFA warning banner in Viewer.svelte. M6 signatures management: signatures.svelte.ts store (localStorage), SignatureCapture redesigned with Draw/Saved tabs, thumbnail preview, set-default star, delete. M7 file association: winreg HKCU write/delete for SimplePDF.Document ProgID, Settings.svelte toggle. M4 AP streams deferred: FPDFAnnot_GenerateAP absent from pdfium-render 0.8.37 bindings — needs ffi.rs + libloading (tracked as in-progress). 0 Rust errors, 0 svelte-check errors.
- 2026-04-27 — M8 distribution & crash logging: WebView2 detection in main.rs (winreg HKLM/HKCU check before Tauri init, mshta dialog + download link on failure); file-based logging via tracing-appender rolling::never to app_data_dir()/logs/simple-pdf.log; panic hook writes crash.txt to same dir; app_data_dir() implements portable.txt sentinel (./data/ vs %APPDATA%\simple.pdf); scripts/build-portable.ps1 (cargo tauri build --no-bundle → dist/portable/ zip); scripts/build-installer.ps1 (cargo tauri build --bundles nsis). M5 form overlay checkbox corrected (was already implemented in Page.svelte).
