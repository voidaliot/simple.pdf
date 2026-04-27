use parking_lot::Mutex;
use pdf_core::{Document, PdfEngine};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// One undoable annotation add operation.
#[derive(Clone)]
pub struct UndoEntry {
    pub page_index: u32,
    pub annot_index: u32,
}

pub struct AppState {
    pub engine: PdfEngine,
    pub docs: Mutex<HashMap<Uuid, Arc<Document>>>,
    pub pending_files: Mutex<Vec<PathBuf>>,
    /// Per-document stack of added annotations (for Ctrl+Z).
    pub undo_stacks: Mutex<HashMap<Uuid, Vec<UndoEntry>>>,
}

pub fn init(
    app: &mut tauri::App,
    initial_args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .ok_or("no exe dir")?
        .to_path_buf();
    let engine = PdfEngine::new(&exe_dir)?;

    let pending: Vec<PathBuf> = initial_args
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map_or(false, |e| e.eq_ignore_ascii_case("pdf"))
        })
        .collect();

    app.manage(AppState {
        engine,
        docs: Mutex::new(HashMap::new()),
        pending_files: Mutex::new(pending),
        undo_stacks: Mutex::new(HashMap::new()),
    });
    Ok(())
}

pub fn enqueue_file_args(app: &AppHandle, argv: Vec<String>) {
    let state = app.state::<AppState>();
    let mut q = state.pending_files.lock();
    for a in argv {
        let p = PathBuf::from(a);
        if p.extension()
            .and_then(|e| e.to_str())
            .map_or(false, |e| e.eq_ignore_ascii_case("pdf"))
        {
            q.push(p);
        }
    }
    let _ = app.emit("files-queued", ());
}
