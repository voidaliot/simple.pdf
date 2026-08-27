use parking_lot::{Condvar, Mutex};
use pdf_core::{Document, PdfEngine};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// One undoable annotation add operation.
#[derive(Clone)]
pub struct UndoEntry {
    pub page_index: u32,
    pub annot_index: u32,
}

pub struct AppState {
    // Declared first so shutdown stops and joins the sole index worker before
    // the engine/document fields begin dropping.
    pub text_indexer: TextIndexer,
    pub engine: PdfEngine,
    pub docs: Mutex<HashMap<Uuid, Arc<Document>>>,
    pub pending_files: Mutex<Vec<PathBuf>>,
    /// Per-document stack of added annotations (for Ctrl+Z).
    pub undo_stacks: Mutex<HashMap<Uuid, Vec<UndoEntry>>>,
    /// URL downloads owned by this process. Their paths remain valid for Save
    /// and Copy path until the corresponding tab closes.
    temporary_downloads: Mutex<HashSet<PathBuf>>,
}

impl AppState {
    pub fn register_temporary_download(&self, path: PathBuf) {
        self.temporary_downloads.lock().insert(path);
    }

    /// Stop owning `path` and return it for deletion, if it is one of ours.
    pub fn take_temporary_download(&self, path: &std::path::Path) -> Option<PathBuf> {
        self.temporary_downloads.lock().take(path)
    }

    pub fn remove_temporary_download(&self, path: &std::path::Path) {
        if let Some(owned) = self.take_temporary_download(path) {
            let _ = std::fs::remove_file(owned);
        }
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        // The PDF source bytes are resident, so no Document holds these files
        // open. Best-effort cleanup is therefore safe even during shutdown.
        for path in self.temporary_downloads.get_mut().drain() {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct IndexJob {
    doc: Weak<Document>,
    next_page: u32,
    page_count: u32,
    not_before: Instant,
}

struct IndexQueue {
    // A newly opened document replaces older pending work. Search indexes any
    // non-active document on demand, so eagerly walking every background tab
    // would only waste memory and native-gate time.
    latest: Mutex<Option<IndexJob>>,
    wake: Condvar,
    shutdown: AtomicBool,
}

/// One process-wide, latest-document-only background text-index worker.
///
/// A worker per tab quickly exhausts Tokio's blocking pool and makes dozens of
/// threads compete for the single Pdfium gate. This scheduler instead indexes
/// at most one page at a time, keeps only Weak document references, and leaves
/// a deliberate idle slice between pages for viewport rendering.
pub struct TextIndexer {
    queue: Arc<IndexQueue>,
    worker: Option<JoinHandle<()>>,
}

impl TextIndexer {
    fn new() -> std::io::Result<Self> {
        let queue = Arc::new(IndexQueue {
            latest: Mutex::new(None),
            wake: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });
        let worker_queue = Arc::clone(&queue);
        let worker = std::thread::Builder::new()
            .name("simple-pdf-text-index".into())
            .spawn(move || run_text_indexer(worker_queue))?;
        Ok(Self {
            queue,
            worker: Some(worker),
        })
    }

    pub fn enqueue(&self, doc: Weak<Document>, page_count: u32) {
        let job = IndexJob {
            doc,
            next_page: 0,
            page_count,
            // First raster/layout should win decisively after document open.
            not_before: Instant::now() + Duration::from_millis(250),
        };
        *self.queue.latest.lock() = Some(job);
        self.queue.wake.notify_one();
    }
}

impl Drop for TextIndexer {
    fn drop(&mut self) {
        self.queue.shutdown.store(true, Ordering::Release);
        self.queue.wake.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_text_indexer(queue: Arc<IndexQueue>) {
    set_index_thread_priority();

    let mut active: Option<IndexJob> = None;
    loop {
        {
            let mut latest = queue.latest.lock();
            if queue.shutdown.load(Ordering::Acquire) {
                return;
            }
            if let Some(newest) = latest.take() {
                active = Some(newest);
            }

            let Some(job) = active.as_ref() else {
                queue.wake.wait(&mut latest);
                continue;
            };
            let now = Instant::now();
            if job.not_before > now {
                queue
                    .wake
                    .wait_for(&mut latest, job.not_before.saturating_duration_since(now));
                continue;
            }
        }

        let Some(mut job) = active.take() else {
            continue;
        };
        let Some(doc) = job.doc.upgrade() else {
            continue;
        };
        if doc.background_work_cancelled() {
            continue;
        }

        let retained = match doc.preload_text_page(job.next_page) {
            Ok(retained) => retained,
            Err(error) => {
                tracing::debug!(page_index = job.next_page, %error, "background text extraction failed");
                true
            }
        };
        drop(doc);

        job.next_page += 1;
        if retained && job.next_page < job.page_count {
            // A wake from enqueue() during this delay replaces `active` with
            // the newly opened document before another page is indexed.
            job.not_before = Instant::now() + Duration::from_millis(5);
            active = Some(job);
        } else if !retained {
            tracing::debug!("text cache reached its resident-memory limit");
        }

        // This is a global idle slice, not one sleep per tab. PDFIUM_GATE is
        // free throughout it, allowing render work to overtake the indexer.
        std::thread::yield_now();
    }
}

#[cfg(target_os = "windows")]
fn set_index_thread_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
    };

    // SAFETY: GetCurrentThread() returns a valid pseudo-handle for this worker;
    // SetThreadPriority() does not take ownership of it.
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(target_os = "windows"))]
fn set_index_thread_priority() {}

pub fn init(
    app: &mut tauri::App,
    initial_args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    scavenge_stale_temporary_downloads();

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
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        })
        .collect();

    app.manage(AppState {
        text_indexer: TextIndexer::new()?,
        engine,
        docs: Mutex::new(HashMap::new()),
        pending_files: Mutex::new(pending),
        undo_stacks: Mutex::new(HashMap::new()),
        temporary_downloads: Mutex::new(HashSet::new()),
    });
    Ok(())
}

fn scavenge_stale_temporary_downloads() {
    const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("simplepdf_") || !name.ends_with(".pdf") {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn enqueue_file_args(app: &AppHandle, argv: Vec<String>) {
    let state = app.state::<AppState>();
    let mut q = state.pending_files.lock();
    for a in argv {
        let p = PathBuf::from(a);
        if p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            q.push(p);
        }
    }
    let _ = app.emit("files-queued", ());
}
