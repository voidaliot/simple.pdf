use crate::state::AppState;
use pdf_core::{AnnRect, Annotation, FormField, PageSize, TextSpan};
use serde::Serialize;
use shared_types::AppVersion;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// ── Version ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn app_version() -> AppVersion {
    AppVersion {
        name: "simple.pdf".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        pdfium_version: None,
    }
}

// ── Document lifecycle ─────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct OpenedDocument {
    pub id: String,
    pub path: String,
    pub title: String,
    pub page_count: u32,
}

#[tauri::command]
pub async fn open_document(
    path: String,
    state: State<'_, AppState>,
) -> Result<OpenedDocument, String> {
    let p = PathBuf::from(&path);
    let engine = state.engine.clone();
    let open_path = p.clone();
    let doc = run_pdfium(move || engine.open(&open_path).map_err(|e| e.to_string())).await?;
    let id = Uuid::new_v4();
    let page_count = doc.page_count;
    let title = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string();
    state.docs.lock().insert(id, Arc::new(doc));
    Ok(OpenedDocument { id: id.to_string(), path, title, page_count })
}

#[tauri::command]
pub async fn close_document(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let doc = state.docs.lock().remove(&uid);
    state.undo_stacks.lock().remove(&uid);
    if let Some(doc) = doc {
        run_pdfium(move || {
            drop(doc);
            Ok(())
        })
        .await?;
    }
    Ok(())
}

#[tauri::command]
pub fn pending_open_files(state: State<AppState>) -> Vec<String> {
    state
        .pending_files
        .lock()
        .drain(..)
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

// ── Page rendering (IPC) ──────────────────────────────────────────────────────

/// Render one page and return raw RGBA pixels as a binary IPC response.
///
/// Response body layout (little-endian):
///   bytes 0–3  : u32  pixel width
///   bytes 4–7  : u32  pixel height
///   bytes 8+   : width × height × 4 bytes of RGBA (alpha=255 everywhere)
///
/// The frontend receives this as an `ArrayBuffer` and paints it with
/// `ctx.putImageData()` on a `<canvas>`.  No image codec is involved, so
/// there is no JPEG/PNG encoding loss and no transparency-group blackout.
#[tauri::command]
pub async fn render_page_pixels(
    id: String,
    page_index: u32,
    scale: f32,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    // Clone the document handle before entering the blocking pool so the Tauri
    // State guard is not captured by a 'static task. PDFium rendering and the
    // full-frame response copy both stay off the async/UI executor.
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let buf = run_pdfium(move || -> Result<Vec<u8>, String> {
        let raw = doc
            .render_page_raw(pdf_core::RenderRequest { page_index, scale })
            .map_err(|e| e.to_string())?;

        let response_len = raw
            .rgba
            .len()
            .checked_add(8)
            .ok_or_else(|| "render response size overflow".to_string())?;
        let mut buf = Vec::with_capacity(response_len);
        buf.extend_from_slice(&raw.width.to_le_bytes());
        buf.extend_from_slice(&raw.height.to_le_bytes());
        buf.extend_from_slice(&raw.rgba);
        Ok(buf)
    })
    .await?;

    Ok(tauri::ipc::Response::new(buf))
}

/// Render page 0 of an on-disk PDF at thumbnail size, returned as a data URL.
/// Thumbnails are small enough that JPEG+base64 is fine here.
#[tauri::command]
pub async fn render_thumb_b64(
    path: String,
    max_w: f32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use base64::Engine;
    let engine = state.engine.clone();
    let p = PathBuf::from(path);
    run_pdfium(move || {
        let doc = engine.open(&p).map_err(|e| e.to_string())?;
        let sizes = doc.page_sizes().map_err(|e| e.to_string())?;
        let page_w = sizes.first().map(|s| s.width).unwrap_or(612.0);
        let scale = (max_w / page_w).clamp(0.05, 1.0);
        let bytes = doc
            .render_page_jpeg(pdf_core::RenderRequest { page_index: 0, scale })
            .map_err(|e| e.to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
    })
    .await
}

// ── Page data ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_page_sizes(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<PageSize>, String> {
    with_doc(&id, &state, |doc| doc.page_sizes().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn get_page_text_spans(
    id: String,
    page_index: u32,
    state: State<'_, AppState>,
) -> Result<Vec<TextSpan>, String> {
    with_doc(&id, &state, move |doc| {
        doc.page_text_spans(page_index).map_err(|e| e.to_string())
    })
    .await
}

// ── Annotations ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_page_annotations(
    id: String,
    page_index: u32,
    state: State<'_, AppState>,
) -> Result<Vec<Annotation>, String> {
    with_doc(&id, &state, move |doc| {
        doc.page_annotations(page_index).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn add_highlight_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    opacity: f32,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = run_pdfium(move || {
        doc.add_highlight(page_index, &rects, color, opacity)
            .map_err(|e| e.to_string())
    })
    .await?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub async fn add_underline_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    // Underline reuses the highlight path with a different annotation type.
    // pdfium-render wraps FPDFPage_CreateAnnot(FPDF_ANNOT_UNDERLINE).
    let idx = run_pdfium(move || {
        doc.add_underline(page_index, &rects, color)
            .map_err(|e| e.to_string())
    })
    .await?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub async fn add_strikeout_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = run_pdfium(move || {
        doc.add_strikeout(page_index, &rects, color)
            .map_err(|e| e.to_string())
    })
    .await?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
// Tauri exposes these as named IPC arguments; grouping them would make the
// TypeScript call less explicit without simplifying the command boundary.
#[allow(clippy::too_many_arguments)]
pub async fn add_text_annotation(
    id: String,
    page_index: u32,
    left: f32,
    top: f32,
    contents: String,
    author: Option<String>,
    color: [u8; 3],
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = run_pdfium(move || {
        doc.add_text_annotation(page_index, left, top, &contents, author.as_deref(), color)
            .map_err(|e| e.to_string())
    })
    .await?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub async fn add_ink_annotation(
    id: String,
    page_index: u32,
    paths: Vec<Vec<[f32; 2]>>,
    color: [u8; 3],
    width: f32,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = run_pdfium(move || {
        doc.add_ink_annotation(page_index, &paths, color, width)
            .map_err(|e| e.to_string())
    })
    .await?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub async fn remove_annotation(
    id: String,
    page_index: u32,
    annot_index: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, move |doc| {
        doc.remove_annotation(page_index, annot_index)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn undo_annotation(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<u32>, String> {
    let uid = parse_uuid(&id)?;
    let entry = state.undo_stacks.lock().get_mut(&uid).and_then(|s| s.pop());
    if let Some(e) = entry {
        let doc = match get_doc(&uid, &state) {
            Ok(doc) => doc,
            Err(error) => {
                state.undo_stacks.lock().entry(uid).or_default().push(e);
                return Err(error);
            }
        };
        let page_index = e.page_index;
        let annot_index = e.annot_index;
        let result = run_pdfium(move || {
            doc.remove_annotation(page_index, annot_index)
                .map_err(|error| error.to_string())
        })
        .await;
        if let Err(error) = result {
            state.undo_stacks.lock().entry(uid).or_default().push(e);
            return Err(error);
        }
        Ok(Some(page_index))
    } else {
        Ok(None)
    }
}

// ── Forms (AcroForms) ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_form_type(id: String, state: State<'_, AppState>) -> Result<String, String> {
    with_doc(&id, &state, |doc| doc.form_type().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn get_form_fields(
    id: String,
    page_index: u32,
    state: State<'_, AppState>,
) -> Result<Vec<FormField>, String> {
    with_doc(&id, &state, move |doc| {
        doc.get_form_fields(page_index).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_field_text_value(
    id: String,
    page_index: u32,
    annot_index: u32,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, move |doc| {
        doc.set_field_text_value(page_index, annot_index, &value)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_field_checked(
    id: String,
    page_index: u32,
    annot_index: u32,
    checked: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, move |doc| {
        doc.set_field_checked(page_index, annot_index, checked)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn reset_form_fields(
    id: String,
    page_index: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, move |doc| {
        doc.reset_form_fields(page_index).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn reset_all_form_fields(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, |doc| {
        doc.reset_all_form_fields().map_err(|e| e.to_string())
    })
    .await
}

// ── Save ──────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn save_document(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    run_pdfium(move || {
        let path = doc.path.clone();
        doc.save_to_path(&path).map_err(|e| e.to_string())
    })
    .await
}

// ── File system helpers ────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_folder_pdfs(path: String) -> Result<Vec<String>, String> {
    let dir = std::path::Path::new(&path);
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut pdfs = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("pdf") {
                    pdfs.push(p.to_string_lossy().into_owned());
                }
            }
        }
    }
    pdfs.sort();
    Ok(pdfs)
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    // Use Windows Explorer with the /select flag to highlight the file.
    std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ── URL download ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn download_url_to_temp(url: String) -> Result<String, String> {
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    if bytes.len() < 5 || &bytes[0..5] != b"%PDF-" {
        return Err("URL did not return a valid PDF".into());
    }

    let tmp = std::env::temp_dir().join(format!(
        "simplepdf_{}.pdf",
        Uuid::new_v4().as_simple()
    ));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    Ok(tmp.to_string_lossy().into_owned())
}

// ── File association (Windows HKCU) ──────────────────────────────────────────

#[tauri::command]
pub fn get_pdf_association() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey("Software\\Classes\\.pdf") {
            let current: String = key.get_value("").unwrap_or_default();
            return Ok(current == "SimplePDF.Document");
        }
        Ok(false)
    }
    #[cfg(not(target_os = "windows"))]
    Ok(false)
}

#[tauri::command]
pub fn set_pdf_association(enable: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{KEY_SET_VALUE, HKEY_CURRENT_USER};
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" \"%1\"", exe.display());

            let (k, _) = hkcu
                .create_subkey("Software\\Classes\\.pdf")
                .map_err(|e| e.to_string())?;
            k.set_value("", &"SimplePDF.Document").map_err(|e| e.to_string())?;

            let (k, _) = hkcu
                .create_subkey("Software\\Classes\\SimplePDF.Document")
                .map_err(|e| e.to_string())?;
            k.set_value("", &"PDF Document").map_err(|e| e.to_string())?;

            let (k, _) = hkcu
                .create_subkey(
                    "Software\\Classes\\SimplePDF.Document\\shell\\open\\command",
                )
                .map_err(|e| e.to_string())?;
            k.set_value("", &cmd.as_str()).map_err(|e| e.to_string())?;

        } else {
            // Remove only if we set it
            if let Ok(k) = hkcu.open_subkey_with_flags(
                "Software\\Classes\\.pdf",
                KEY_SET_VALUE,
            ) {
                let cur: String = k.get_value("").unwrap_or_default();
                if cur == "SimplePDF.Document" {
                    let _ = hkcu.delete_subkey_all("Software\\Classes\\.pdf");
                }
            }
            let _ = hkcu.delete_subkey_all("Software\\Classes\\SimplePDF.Document");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    Err("File association is only supported on Windows".into())
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

fn parse_uuid(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|e| e.to_string())
}

fn get_doc(uid: &Uuid, state: &State<AppState>) -> Result<Arc<pdf_core::Document>, String> {
    state.docs.lock().get(uid).cloned().ok_or_else(|| "unknown doc id".into())
}

async fn run_pdfium<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("PDFium task failed: {e}"))?
}

async fn with_doc<T, F>(
    id: &str,
    state: &State<'_, AppState>,
    f: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&pdf_core::Document) -> Result<T, String> + Send + 'static,
{
    let uid = parse_uuid(id)?;
    let doc = get_doc(&uid, state)?;
    run_pdfium(move || f(&doc)).await
}
