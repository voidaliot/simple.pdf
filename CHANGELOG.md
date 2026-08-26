# Changelog

## 1.0.1 — 2026-08-26

- Replaced `mshta`, inline VBScript, and `cmd /c start` with native Windows dialogs and URI launching.
- Stopped directly claiming the `.pdf` extension. Settings now registers simple.pdf as a PDF-capable application and opens Windows Default Apps for the user's choice.
- Declared PDF support in the Tauri/NSIS bundle metadata.
- Restricted URL downloads to HTTP(S), five redirects, 60 seconds, and 100 MB.
- Removed unused Tauri shell and HTTP plugins from the executable.
- Added optional trusted-certificate signing support to the release scripts.

## 1.0.0 — 2026-08-26

- Initial public release.
