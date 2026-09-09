# Changelog

## 1.4.0 — 2026-09-09

- Add visible multiline text placement on PDF pages with adjustable font size, persistent saving, and annotation Undo/removal.
- Add a form editing control and keep edited text, checkbox, and radio values synchronized when controls reload.
- Add Print and Ctrl+P for the complete document, including current form values and annotations; connect recognized PDF print buttons to the same action.

- Fix native window closing by granting the required destroy capability, awaiting an explicit Cancel/Discard response for dirty tabs and windows, and testing these paths in the actual Windows executable.
- Handle PlantUML comment/prose preambles and trailing text, ship the OpenIconic and emoji assets required by the engine, and explain unsupported diagram types and styles-only files.
- Preserve Mermaid HTML labels, line breaks, and entities while producing valid sanitized SVG; add corpus diagnostics and rendering regressions.

- Render Mermaid and PlantUML locally from standalone files and Markdown code fences, with zoom, source inspection, reload, and safe SVG export.
- Integrate diagram documents with file/folder opening, drag-and-drop, CLI launches, tabs, recents, and installer file-type registration.
- Bound diagram source sizes and rendering work, sanitize Markdown and SVG output, and prevent remote include/data fetching.
- Show file-open errors, resolve second-instance paths against the sender's working directory, and remove the startup file-queue subscription race.
- Normalize Windows path aliases, validate stored recent files, preserve pins, and tolerate thumbnail storage exhaustion.
- Keep annotation Undo indexes correct after deletions, serialize changes with their history updates, bound history, preserve dirty state after Undo or edits during Save, and guard window close when documents have unsaved changes.
- Prevent unknown form buttons from clearing values, respect read-only fields, handle radio selections, and stop silently accepting unsupported field edits. Combo/list controls display their saved values as read-only with the pinned PDFium wrapper.
- Preserve portable user data during rebuilds and package only release files, including dependency notices.

## 1.3.0 — 2026-08-30

- Add a dual-page layout with spread-aware fit modes, navigation, virtualization, and scroll anchoring for mixed-size and rotated documents.
- Render visible regions in bounded full-resolution tiles so high zoom and high-DPI displays stay sharp without allocating oversized full-page bitmaps.
- Track monitor DPI changes and preserve the visible document position while rebuilding page rasters at the new device-pixel ratio.

## 1.2.0 — 2026-08-30

- Match Windows Explorer's compact 40-DIP tab/title bar while using a native WebView2 caption region for reliable touch and pen dragging, horizontal tab swiping, and a preserved blank grab area.
- Keep high-DPI and large-window page renders inside the renderer's exact rounded bitmap limits instead of showing a pixel-limit error.

## 1.1.0 — 2026-08-27

- Load each opened PDF from a complete in-memory source snapshot for seek-free page access while enforcing a process-wide resident-source budget.
- Make scrolling responsive with page virtualization, visible-page render priority, a bounded raster LRU, and deterministic canvas cleanup.
- Replace sequential frontend Find with cancellable native full-document search supporting phrases, normalized whitespace, case folding, and line-break hyphenation.
- Add clickable document chapters, internal page links, and safe HTTP(S)/mail links.
- Add a tab context menu with a keyboard-accessible **Copy path** action.
- Harden Windows resource handling with process-wide text-cache limits, one below-normal-priority index worker, fallible render allocations, ordered PDFium destruction, atomic-save cleanup, and streamed temporary downloads.

## 1.0.1 — 2026-08-26

- Replaced `mshta`, inline VBScript, and `cmd /c start` with native Windows dialogs and URI launching.
- Stopped directly claiming the `.pdf` extension. Settings now registers simple.pdf as a PDF-capable application and opens Windows Default Apps for the user's choice.
- Declared PDF support in the Tauri/NSIS bundle metadata.
- Restricted URL downloads to HTTP(S), five redirects, 60 seconds, and 100 MB.
- Removed unused Tauri shell and HTTP plugins from the executable.
- Added optional trusted-certificate signing support to the release scripts.

## 1.0.0 — 2026-08-26

- Initial public release.
