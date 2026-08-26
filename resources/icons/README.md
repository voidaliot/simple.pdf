# App icons

`icon.svg` is the vector master for the desktop app. The Tauri bundle config at `crates/app/tauri.conf.json` expects:

- `32x32.png`
- `128x128.png`
- `icon.ico`

Regenerate every platform size with the repo-local CLI from the repository root:

```powershell
node frontend/node_modules/@tauri-apps/cli/tauri.js icon resources/icons/icon.svg --output resources/icons
```
