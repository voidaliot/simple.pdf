use parking_lot::Mutex;
use pdf_core::{Document, PdfEngine};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

pub struct AppState {
    pub engine: PdfEngine,
    pub docs: Mutex<HashMap<Uuid, Arc<Document>>>,
    pub pending_files: Mutex<Vec<PathBuf>>,
}

pub fn init(app: &mut tauri::App, initial_args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .ok_or("no exe dir")?
        .to_path_buf();
    let engine = PdfEngine::new(&exe_dir)?;

    let pending: Vec<PathBuf> = initial_args
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case("pdf")))
        .collect();

    app.manage(AppState {
        engine,
        docs: Mutex::new(HashMap::new()),
        pending_files: Mutex::new(pending),
    });
    Ok(())
}

pub fn enqueue_file_args(app: &AppHandle, argv: Vec<String>) {
    let state = app.state::<AppState>();
    let mut q = state.pending_files.lock();
    for a in argv {
        let p = PathBuf::from(a);
        if p.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case("pdf")) {
            q.push(p);
        }
    }
    let _ = app.emit("files-queued", ());
}
