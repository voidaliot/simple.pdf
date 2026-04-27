# simple.pdf — Requirements

Authoritative list of what simple.pdf v1 must do and how well it must do it. Progress against these is tracked in [plan.md](plan.md).

**Status:** `[ ]` pending · `[x]` implemented

Each requirement has a stable ID (`<CATEGORY>-<AREA>-<NN>`). IDs never change once assigned; new requirements get the next free number.

---

## 1. Functional — Viewing (FR-VIEW)

- [ ] **FR-VIEW-01** — User can open a PDF via File → Open, drag-drop into the window, or `simple.pdf.exe <path>` CLI argument.
- [ ] **FR-VIEW-02** — User can navigate pages: mouse-wheel scroll, keyboard (PgUp/PgDn/Home/End), thumbnail sidebar, goto-page input.
- [ ] **FR-VIEW-03** — User can select text with the mouse and copy to clipboard (Ctrl+C).
- [ ] **FR-VIEW-04** — Find-in-page (Ctrl+F): live-highlight all matches, next/prev with Enter/Shift+Enter, match counter.
- [ ] **FR-VIEW-05** — Zoom 25%–400% plus fit-width and fit-page presets; pinch and Ctrl+Wheel to zoom.
- [ ] **FR-VIEW-06** — Rotate current page 90° / 180° / 270° for viewing (does not modify the PDF).

## 2. Functional — Tabs & Home (FR-TAB, FR-HOME)

- [ ] **FR-TAB-01** — Browser-style tab bar at the top; each open PDF gets a tab; `+` button opens a home tab.
- [ ] **FR-TAB-02** — Closing a tab with unsaved changes prompts the user (Save / Discard / Cancel).
- [ ] **FR-TAB-03** — Tabs reorderable by drag.
- [ ] **FR-TAB-04** — Middle-click on a tab closes it.
- [ ] **FR-HOME-01** — Home tab shows a grid of recent files with first-page thumbnails (configurable limit, default 50).
- [ ] **FR-HOME-02** — User can pin/unpin files; pinned section renders above recents.
- [ ] **FR-HOME-03** — Filter-as-you-type search over recents (filename, fuzzy).
- [ ] **FR-HOME-04** — Quick actions on home: Open File, Open Folder (lists PDFs in folder), Paste URL (download then open).
- [ ] **FR-HOME-05** — Right-click on a recent: Open, Remove from list, Reveal in Explorer, Copy path.

## 3. Functional — Annotations (FR-ANN)

- [ ] **FR-ANN-01** — User can highlight selected text. Color is configurable. Persisted as `/Highlight` annotation with appearance stream.
- [ ] **FR-ANN-02** — Underline selected text. Persisted as `/Underline`.
- [ ] **FR-ANN-03** — Strikethrough selected text. Persisted as `/StrikeOut`.
- [ ] **FR-ANN-04** — Sticky note anywhere on a page with author, timestamp, and text content. Persisted as `/Text` annotation.
- [ ] **FR-ANN-05** — Freehand ink with configurable color and stroke width. Persisted as `/Ink` annotation plus baked AP stream so other readers render identically.
- [ ] **FR-ANN-06** — User can edit the content / style of, or delete, any annotation authored in the current session. Annotations authored by others are read-only but deletable.
- [ ] **FR-ANN-07** — Undo/redo (Ctrl+Z / Ctrl+Y) across annotation actions, with a reasonable history depth (≥ 50 steps).

## 4. Functional — Forms (FR-FORM)

- [x] **FR-FORM-01** — Detect AcroForm fields and render interactive inputs over them for: text, multi-line text, checkbox, radio, combobox (dropdown), listbox, push button.
- [x] **FR-FORM-02** — On save, field values persist in the PDF and regenerated appearances are written so other readers display the filled form.
- [x] **FR-FORM-03** — Detect XFA-only forms (via `/XFA` in the catalog) and show a non-blocking banner: "This is an XFA form — some fields may not render. Open in Adobe Reader for full fidelity."
- [ ] **FR-FORM-04** — Respect form reset action buttons when the PDF defines them.

## 5. Functional — Signing (FR-SIGN)

- [ ] **FR-SIGN-01** — Signature-capture modal: user can draw on a canvas with mouse/pen, or import a PNG/JPG. Output is a transparent-background bitmap plus vector paths (for ink).
- [ ] **FR-SIGN-02** — User places a captured signature as a stamp annotation (`/Stamp`) with a correct AP stream, resizable and movable before finalizing.
- [x] **FR-SIGN-03** — Manage saved signatures: list, rename, mark default, delete. Stored under the app's data folder.

## 6. Non-Functional (NFR)

- [ ] **NFR-PERF-01** — Cold start to first page paint ≤ 1.5 s for a 10 MB PDF on baseline hardware (4-core CPU, SSD, Win10 22H2).
- [ ] **NFR-PERF-02** — Scroll and zoom remain ≥ 60 fps on a 50-page text PDF at fit-width zoom on baseline hardware.
- [ ] **NFR-SIZE-01** — Shipped portable zip is ≤ 20 MB (exe + pdfium.dll + icons).
- [ ] **NFR-MEM-01** — Idle working set < 200 MB with one 50 MB PDF open.
- [ ] **NFR-STAB-01** — Opening a malformed or encrypted PDF never crashes the app; user sees an explanatory toast.

## 7. UX

- [ ] **UX-THEME-01** — App auto-follows Windows light/dark setting; user override persists across restarts.
- [ ] **UX-KBD-01** — Browser-like shortcuts: Ctrl+T (new tab), Ctrl+W (close tab), Ctrl+Tab / Ctrl+Shift+Tab (switch), Ctrl+O (open), Ctrl+F (find), Ctrl+S (save), Ctrl+P (print).
- [ ] **UX-DESIGN-01** — Minimalistic chrome: single top bar combining tabs + primary toolbar; optional collapsible sidebar for thumbnails/comments; no ribbons.
- [ ] **UX-ZOOM-01** — Zoom level persists per-document across sessions.
- [ ] **UX-A11Y-01** — All actions reachable by keyboard; visible focus ring; respects `prefers-reduced-motion`.

## 8. Distribution (DIST)

- [x] **DIST-PORT-01** — Portable zip runs from USB or network share without admin or install.
- [x] **DIST-INST-01** — NSIS installer available as an alternative for users who want file associations and Start Menu entries.
- [x] **DIST-ASSOC-01** — `.pdf` file association is opt-in via Settings; writes only to `HKCU` (per-user), never `HKLM`.
- [x] **DIST-WV2-01** — If WebView2 runtime is missing, app shows a clear message with a link to the Microsoft installer instead of a cryptic failure.

## 9. Security & Privacy (SEC)

- [ ] **SEC-NET-01** — Zero outbound network requests in default configuration. URL-paste feature requires an explicit user action per use.
- [ ] **SEC-JS-01** — PDF JavaScript execution is disabled (PDFium OSS default). Documents cannot run scripts.
- [ ] **SEC-SAVE-01** — Saves write to a temp file then atomic-rename over the original; original is preserved until success.
- [x] **SEC-LOG-01** — Logs are local-only under the app data folder. No telemetry without explicit opt-in. No personally identifying information in logs.

---

## Definitions

- **Baseline hardware**: 4-core x86-64 CPU ≥ 2 GHz, 8 GB RAM, NVMe SSD, Windows 10 22H2 or Windows 11 23H2, integrated GPU.
- **App data folder**: `./data/` next to the exe if `portable.txt` exists there; otherwise `%APPDATA%\simple.pdf\`.
- **Annotation fidelity**: an annotation is "fidelity-preserving" if opening the saved PDF in Adobe Reader DC and Microsoft Edge produces a visually equivalent rendering (ignoring subpixel antialiasing differences).
