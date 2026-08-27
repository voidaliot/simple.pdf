use crate::state::AppState;
use pdf_core::{AnnRect, Annotation, FormField, OutlineItem, PageSize, SearchResults, TextSpan};
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
    let p = PathBuf::from(path);
    let requested_path = p.clone();
    let engine = state.engine.clone();
    let opened = run_pdfium(move || {
        let canonical = std::fs::canonicalize(&p).map_err(|e| e.to_string())?;
        let result = engine.open(&canonical).map_err(|e| e.to_string());
        Ok((canonical, result))
    })
    .await;
    let (p, doc) = match opened {
        Ok((canonical, Ok(doc))) => (canonical, doc),
        Ok((canonical, Err(error))) => {
            state.remove_temporary_download(&canonical);
            return Err(error);
        }
        Err(error) => {
            state.remove_temporary_download(&requested_path);
            return Err(error);
        }
    };
    let id = Uuid::new_v4();
    let page_count = doc.page_count;
    let title = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string();
    let doc = Arc::new(doc);
    state.docs.lock().insert(id, Arc::clone(&doc));
    state.text_indexer.enqueue(Arc::downgrade(&doc), page_count);
    Ok(OpenedDocument {
        id: id.to_string(),
        path: user_display_path(&p),
        title,
        page_count,
    })
}

#[tauri::command]
pub async fn close_document(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let doc = state.docs.lock().remove(&uid);
    state.undo_stacks.lock().remove(&uid);
    if let Some(doc) = doc {
        let temporary_download = state.take_temporary_download(&doc.path);
        doc.cancel_background_work();
        let result = run_pdfium(move || {
            drop(doc);
            Ok(())
        })
        .await;
        if let Some(path) = temporary_download {
            let _ = std::fs::remove_file(path);
        }
        result?;
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
    let buf = run_pdfium(move || {
        doc.render_page_ipc(pdf_core::RenderRequest { page_index, scale })
            .map_err(|e| e.to_string())
    })
    .await?;

    Ok(tauri::ipc::Response::new(buf))
}

/// Render page 0 of an on-disk PDF at thumbnail size, returned as a data URL.
/// Thumbnails are small enough that JPEG+base64 is fine here.
#[tauri::command]
pub async fn render_thumb_b64(
    id: String,
    max_w: f32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use base64::Engine;
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    run_pdfium(move || {
        let page_w = doc.page_size(0).map(|size| size.width).unwrap_or(612.0);
        let scale = (max_w / page_w).clamp(0.05, 1.0);
        let bytes = doc
            .render_page_jpeg(pdf_core::RenderRequest {
                page_index: 0,
                scale,
            })
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
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    // Viewer mount is also the active-tab signal. Resume low-priority text
    // warming for this document so Find is hot after switching older tabs.
    state
        .text_indexer
        .enqueue(Arc::downgrade(&doc), doc.page_count);
    run_pdfium(move || doc.page_sizes().map_err(|e| e.to_string())).await
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

const DEFAULT_SEARCH_RESULTS: usize = 10_000;
const MAX_SEARCH_RESULTS: usize = 10_000;
const MAX_SEARCH_QUERY_CHARS: usize = 1_024;

#[tauri::command]
pub async fn search_document(
    id: String,
    query: String,
    max_results: Option<usize>,
    state: State<'_, AppState>,
) -> Result<SearchResults, String> {
    if query.chars().count() > MAX_SEARCH_QUERY_CHARS {
        return Err(format!(
            "search query exceeds the {MAX_SEARCH_QUERY_CHARS} character limit"
        ));
    }
    let uid = parse_uuid(&id)?;
    let doc = get_doc(&uid, &state)?;
    // Bump synchronously, before entering the blocking pool, so a freshly
    // debounced query cancels stale extraction immediately.
    let generation = doc.begin_search();
    let max_results = max_results
        .unwrap_or(DEFAULT_SEARCH_RESULTS)
        .clamp(1, MAX_SEARCH_RESULTS);
    run_pdfium(move || {
        doc.search_document(&query, max_results, generation)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub fn cancel_search(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uid = parse_uuid(&id)?;
    get_doc(&uid, &state)?.cancel_search();
    Ok(())
}

#[tauri::command]
pub async fn get_document_outline(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<OutlineItem>, String> {
    with_doc(&id, &state, |doc| {
        doc.document_outline().map_err(|e| e.to_string())
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
    state
        .undo_stacks
        .lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry {
            page_index,
            annot_index: idx,
        });
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
    state
        .undo_stacks
        .lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry {
            page_index,
            annot_index: idx,
        });
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
    state
        .undo_stacks
        .lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry {
            page_index,
            annot_index: idx,
        });
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
    state
        .undo_stacks
        .lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry {
            page_index,
            annot_index: idx,
        });
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
    state
        .undo_stacks
        .lock()
        .entry(uid)
        .or_default()
        .push(crate::state::UndoEntry {
            page_index,
            annot_index: idx,
        });
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
    with_doc(&id, &state, |doc| {
        doc.form_type().map_err(|e| e.to_string())
    })
    .await
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
pub async fn reset_all_form_fields(id: String, state: State<'_, AppState>) -> Result<(), String> {
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

#[tauri::command]
pub fn open_external_uri(uri: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&uri).map_err(|_| "PDF link is not a valid URI")?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err("Only HTTP, HTTPS, and mail links can be opened".into());
    }

    #[cfg(target_os = "windows")]
    {
        crate::windows_integration::open_uri(parsed.as_str())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = parsed;
        Err("Opening external PDF links is only supported on Windows".into())
    }
}

// ── URL download ──────────────────────────────────────────────────────────────

struct TemporaryDownloadGuard {
    path: PathBuf,
    keep: bool,
}

impl TemporaryDownloadGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }

    fn persist(mut self) {
        self.keep = true;
    }
}

impl Drop for TemporaryDownloadGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[tauri::command]
pub async fn download_url_to_temp(
    url: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    const MAX_DOWNLOAD_BYTES: usize = 100 * 1024 * 1024;

    let parsed = reqwest::Url::parse(&url).map_err(|_| "Enter a valid HTTP or HTTPS URL")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS URLs are supported".into());
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES as u64)
    {
        return Err("PDF download exceeds the 100 MB limit".into());
    }

    let tmp = std::env::temp_dir().join(format!("simplepdf_{}.pdf", Uuid::new_v4().as_simple()));
    let guard = TemporaryDownloadGuard::new(tmp.clone());
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    let mut total = 0usize;
    let mut header = [0u8; 5];
    let mut header_len = 0usize;

    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        total = total
            .checked_add(chunk.len())
            .filter(|total| *total <= MAX_DOWNLOAD_BYTES)
            .ok_or_else(|| "PDF download exceeds the 100 MB limit".to_string())?;

        if header_len < header.len() {
            let copy_len = (header.len() - header_len).min(chunk.len());
            header[header_len..header_len + copy_len].copy_from_slice(&chunk[..copy_len]);
            header_len += copy_len;
            if header_len == header.len() && &header != b"%PDF-" {
                return Err("URL did not return a valid PDF".into());
            }
        }
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }

    if header_len < header.len() {
        return Err("URL did not return a valid PDF".into());
    }

    file.flush().await.map_err(|e| e.to_string())?;
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);

    let canonical = tokio::fs::canonicalize(&tmp)
        .await
        .map_err(|e| e.to_string())?;
    state.register_temporary_download(canonical.clone());
    guard.persist();
    Ok(canonical.to_string_lossy().into_owned())
}

// ── File association (Windows HKCU) ──────────────────────────────────────────

#[tauri::command]
pub fn get_pdf_association() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        crate::windows_integration::is_default_pdf_handler()
    }
    #[cfg(not(target_os = "windows"))]
    Ok(false)
}

#[tauri::command]
pub fn configure_pdf_association() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        crate::windows_integration::configure_pdf_handler()
    }
    #[cfg(not(target_os = "windows"))]
    Err("File association is only supported on Windows".into())
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

fn parse_uuid(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|e| e.to_string())
}

fn get_doc(uid: &Uuid, state: &State<AppState>) -> Result<Arc<pdf_core::Document>, String> {
    state
        .docs
        .lock()
        .get(uid)
        .cloned()
        .ok_or_else(|| "unknown doc id".into())
}

fn user_display_path(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        // std::fs::canonicalize() returns verbatim paths on Windows. Remove
        // that implementation prefix for display/copy while retaining the
        // canonical path inside Document for saving.
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = path.strip_prefix(r"\\?\") {
            return rest.into();
        }
    }
    path.into_owned()
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

async fn with_doc<T, F>(id: &str, state: &State<'_, AppState>, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&pdf_core::Document) -> Result<T, String> + Send + 'static,
{
    let uid = parse_uuid(id)?;
    let doc = get_doc(&uid, state)?;
    run_pdfium(move || f(&doc)).await
}
