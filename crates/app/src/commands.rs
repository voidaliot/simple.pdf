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
pub fn open_document(path: String, state: State<AppState>) -> Result<OpenedDocument, String> {
    let p = PathBuf::from(&path);
    let doc = state.engine.open(&p).map_err(|e| e.to_string())?;
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
pub fn close_document(id: String, state: State<AppState>) -> Result<(), String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state.docs.lock().remove(&uid);
    state.undo_stacks.lock().remove(&uid);
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

// ── Page data ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_page_sizes(id: String, state: State<AppState>) -> Result<Vec<PageSize>, String> {
    with_doc(&id, &state, |doc| doc.page_sizes().map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn get_page_text_spans(
    id: String,
    page_index: u32,
    state: State<AppState>,
) -> Result<Vec<TextSpan>, String> {
    with_doc(&id, &state, |doc| {
        doc.page_text_spans(page_index).map_err(|e| e.to_string())
    })
}

// ── Annotations ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_page_annotations(
    id: String,
    page_index: u32,
    state: State<AppState>,
) -> Result<Vec<Annotation>, String> {
    with_doc(&id, &state, |doc| {
        doc.page_annotations(page_index).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn add_highlight_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    opacity: f32,
    state: State<AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = doc
        .add_highlight(page_index, &rects, color, opacity)
        .map_err(|e| e.to_string())?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub fn add_underline_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    state: State<AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    // Underline reuses the highlight path with a different annotation type.
    // pdfium-render wraps FPDFPage_CreateAnnot(FPDF_ANNOT_UNDERLINE).
    let idx = doc
        .add_underline(page_index, &rects, color)
        .map_err(|e| e.to_string())?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub fn add_strikeout_annotation(
    id: String,
    page_index: u32,
    rects: Vec<AnnRect>,
    color: [u8; 3],
    state: State<AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = doc
        .add_strikeout(page_index, &rects, color)
        .map_err(|e| e.to_string())?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub fn add_text_annotation(
    id: String,
    page_index: u32,
    left: f32,
    top: f32,
    contents: String,
    author: Option<String>,
    color: [u8; 3],
    state: State<AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = doc
        .add_text_annotation(page_index, left, top, &contents, author.as_deref(), color)
        .map_err(|e| e.to_string())?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub fn add_ink_annotation(
    id: String,
    page_index: u32,
    paths: Vec<Vec<[f32; 2]>>,
    color: [u8; 3],
    width: f32,
    state: State<AppState>,
) -> Result<u32, String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let idx = doc
        .add_ink_annotation(page_index, &paths, color, width)
        .map_err(|e| e.to_string())?;
    state.undo_stacks.lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry { page_index, annot_index: idx });
    Ok(idx)
}

#[tauri::command]
pub fn remove_annotation(
    id: String,
    page_index: u32,
    annot_index: u32,
    state: State<AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, |doc| {
        doc.remove_annotation(page_index, annot_index)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn undo_annotation(id: String, state: State<AppState>) -> Result<bool, String> {
    let uid = parse_uuid(&id)?;
    let entry = state.undo_stacks.lock().get_mut(&uid).and_then(|s| s.pop());
    if let Some(e) = entry {
        let doc = get_doc(&uid, &state)?;
        doc.remove_annotation(e.page_index, e.annot_index)
            .map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// ── Forms (AcroForms) ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_form_type(id: String, state: State<AppState>) -> Result<String, String> {
    with_doc(&id, &state, |doc| doc.form_type().map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn get_form_fields(
    id: String,
    page_index: u32,
    state: State<AppState>,
) -> Result<Vec<FormField>, String> {
    with_doc(&id, &state, |doc| {
        doc.get_form_fields(page_index).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_field_text_value(
    id: String,
    page_index: u32,
    annot_index: u32,
    value: String,
    state: State<AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, |doc| {
        doc.set_field_text_value(page_index, annot_index, &value)
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_field_checked(
    id: String,
    page_index: u32,
    annot_index: u32,
    checked: bool,
    state: State<AppState>,
) -> Result<(), String> {
    with_doc(&id, &state, |doc| {
        doc.set_field_checked(page_index, annot_index, checked)
            .map_err(|e| e.to_string())
    })
}

// ── Save ──────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_document(id: String, state: State<AppState>) -> Result<(), String> {
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    let path = doc.path.clone();
    doc.save_to_path(&path).map_err(|e| e.to_string())
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

fn with_doc<T>(
    id: &str,
    state: &State<AppState>,
    f: impl FnOnce(&pdf_core::Document) -> Result<T, String>,
) -> Result<T, String> {
    let uid = parse_uuid(id)?;
    let doc = get_doc(&uid, state)?;
    f(&doc)
}
