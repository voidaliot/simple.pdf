# App icons

Final icons land in M8 polish. For now this directory is a placeholder. The Tauri bundle config at `crates/app/tauri.conf.json` expects:

- `32x32.png`
- `128x128.png`
- `icon.ico`

Any 1024×1024 master PNG can be fed through `cargo tauri icon ..\..\resources\icons\source.png` to regenerate the full set.
