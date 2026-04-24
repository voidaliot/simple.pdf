# pdfium.dll

`pdfium.dll` is not checked in (see `.gitignore`). Fetch it with:

```powershell
pwsh scripts\fetch-pdfium.ps1
```

The script downloads the latest known-good Windows x64 build from [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries) and places it here. Tauri's `bundle.resources` config copies it next to `simple-pdf.exe` at build time, and the app calls `Pdfium::bind_to_library(exe_dir)` at startup.
