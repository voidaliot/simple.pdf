# Changelog

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
