# simple.pdf

A fast, small-footprint, modern PDF reader for Windows with annotations, AcroForms, and drawn-signature support.

<img width="1201" height="1550" alt="image" src="https://github.com/user-attachments/assets/b75c0cf1-97a9-42d9-9064-2d1370788953" />


## Status

Current release: 1.2.0. See [requirements.md](requirements.md) for the authoritative feature list and current implementation status, and [CHANGELOG.md](CHANGELOG.md) for release notes.

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

Building is supported on Windows x64. Install these prerequisites first:

- [Rust](https://rustup.rs/) with the MSVC toolchain. The repository's `rust-toolchain.toml` selects stable Rust and the `x86_64-pc-windows-msvc` target.
- Visual Studio 2022 Build Tools with **Desktop development with C++** and a Windows 10 or 11 SDK.
- [Node.js](https://nodejs.org/) 18 or newer and [pnpm](https://pnpm.io/installation).
- The Microsoft WebView2 Runtime (included with current Windows 10 and 11 installations).

From PowerShell in the repository root, install dependencies and fetch the pinned PDFium binary:

```powershell
pnpm --dir frontend install --frozen-lockfile
.\scripts\fetch-pdfium.ps1
```

Run the app in development mode:

```powershell
pnpm dev
```

Run the static and Rust checks:

```powershell
pnpm --dir frontend check
cargo test --workspace
```

Create release artifacts with the supplied scripts:

```powershell
# dist\simple.pdf-portable.zip
.\scripts\build-portable.ps1

# target\x86_64-pc-windows-msvc\release\bundle\nsis\*-setup.exe
# Requires NSIS 3.x on PATH.
.\scripts\build-installer.ps1
```

## Publishing releases

Pushing an annotated semantic-version tag runs the GitHub Actions release workflow. The workflow verifies that the tag matches the versions in `Cargo.toml`, `crates/app/tauri.conf.json`, and `frontend/package.json`; runs the frontend and Rust checks; builds the portable ZIP and NSIS installer; and publishes both assets to a GitHub release.

```powershell
git tag -a v1.2.0 -m "simple.pdf 1.2.0"
git push origin v1.2.0
```

The matching version section must already exist in `CHANGELOG.md`. To sign CI-built binaries, configure these GitHub Actions repository secrets:

- `WINDOWS_CERTIFICATE_BASE64` — base64-encoded PFX certificate
- `WINDOWS_CERTIFICATE_PASSWORD` — PFX password
- `WINDOWS_TIMESTAMP_URL` — Authenticode timestamp server URL

If none of these secrets are configured, the workflow publishes unsigned binaries with an explicit warning in the release notes. Supplying only some of them fails the release.

Public releases should be Authenticode-signed with a trusted code-signing certificate. The build scripts enable Tauri signing when both variables below are set; without them, they emit an explicit unsigned-build warning:

```powershell
$env:SIMPLE_PDF_CERTIFICATE_THUMBPRINT = "<certificate SHA-1 thumbprint>"
$env:SIMPLE_PDF_TIMESTAMP_URL = "<certificate provider timestamp URL>"
.\scripts\build-portable.ps1
.\scripts\build-installer.ps1
```

## License

MIT
