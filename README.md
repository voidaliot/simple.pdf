# simple.pdf

A fast, small-footprint, modern PDF reader for Windows with annotations, AcroForms, and drawn-signature support.

<img width="1201" height="1550" alt="image" src="https://github.com/user-attachments/assets/b75c0cf1-97a9-42d9-9064-2d1370788953" />


## Status

Wirking first version available.

## Design goals

- **Small footprint** — portable zip ≤ 20 MB, cold start ≤ 1.5 s.
- **Modern UX** — browser-like tabbed UI, minimalistic chrome, auto light/dark theme.
- **Useful editing** — highlight / underline / strike / sticky / ink annotations, AcroForms fill & save, drawn signatures.
- **Zero install** — portable single-folder distribution (exe + `pdfium.dll`), Windows 10+.

## Stack

- **Rust + Tauri 2** (native shell, WebView2 frontend host)
- **PDFium** (BSD) via the `pdfium-render` crate (rendering, annotations, forms)
- **Svelte 5 + Vite + TypeScript** (frontend)

## Repository layout

```
crates/
  app/           Tauri binary, commands, single-instance, menus
  pdf-core/      PDFium wrapper: render, annotations, forms, raw-FFI escape hatch
  shared-types/  ts-rs types shared with the TS frontend
frontend/        Svelte 5 + Vite frontend
resources/
  pdfium/        pdfium.dll (bundled next to exe at build time)
  icons/         App icon assets
tests/
  fixtures/      Sample PDFs for integration tests
```

## Build

(Instructions come online with M0 — see [plan.md](plan.md).)

## License

MIT
