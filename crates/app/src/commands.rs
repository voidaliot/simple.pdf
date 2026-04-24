use crate::state::AppState;
use pdf_core::RenderRequest;
use serde::Serialize;
use shared_types::AppVersion;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{ipc::Response, State};
use uuid::Uuid;

#[tauri::command]
pub fn app_version() -> AppVersion {
    AppVersion {
        name: "simple.pdf".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        pdfium_version: None,
    }
}

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
    Ok(OpenedDocument {
        id: id.to_string(),
        path,
        title,
        page_count,
    })
}

#[tauri::command]
pub fn close_document(id: String, state: State<AppState>) -> Result<(), String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state.docs.lock().remove(&uid);
    Ok(())
}

#[tauri::command]
pub fn render_page(
    id: String,
    page_index: u32,
    scale: f32,
    state: State<AppState>,
) -> Result<Response, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let doc = {
        let map = state.docs.lock();
        map.get(&uid).cloned().ok_or("unknown doc id")?
    };
    let out = doc
        .render_page(RenderRequest { page_index, scale })
        .map_err(|e| e.to_string())?;
    let header_len = 8u32;
    let mut buf = Vec::with_capacity(8 + out.bgra.len());
    buf.extend_from_slice(&out.width.to_le_bytes());
    buf.extend_from_slice(&out.height.to_le_bytes());
    buf.extend_from_slice(&out.bgra);
    let _ = header_len;
    Ok(Response::new(buf))
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
