pub mod annotations;
pub mod error;
pub mod forms;
pub mod navigation;
pub mod render;
pub mod text;

pub use annotations::{AnnRect, Annotation};
pub use error::{PdfError, PdfResult};
pub use forms::FormField;
pub use navigation::{LinkTarget, OutlineItem};
pub use render::{PageSize, RenderRequest};
pub use text::{SearchMatch, SearchRect, SearchResults, TextSpan};

use parking_lot::Mutex;
use pdfium_render::prelude::*;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};

/// Pdfium is not thread-safe, even when separate documents are being used.
///
/// Every call into Pdfium, including document/library destruction, must hold
/// this process-wide gate. Keeping the gate here (rather than one mutex per
/// document) also covers opening documents and callers using multiple
/// [`PdfEngine`] values.
static PDFIUM_GATE: Mutex<()> = Mutex::new(());

/// The one PDFium library instance shared by every engine in this process.
///
/// `FPDF_InitLibrary()` / `FPDF_DestroyLibrary()` are process-global. Merely
/// serializing two independently initialized `Pdfium` values is insufficient:
/// dropping either one would invalidate the other. The registry therefore
/// shares the exact same allocation until the final engine/document releases
/// it, at which point that release occurs while [`PDFIUM_GATE`] is held.
static PDFIUM_INSTANCE: OnceLock<Mutex<Weak<SharedPdfium>>> = OnceLock::new();

struct SharedPdfium {
    pdfium: Pdfium,
    source_bytes_budget: Arc<ResidentMemoryBudget>,
    text_cache_budget: Arc<ResidentMemoryBudget>,
}

pub(crate) struct ResidentMemoryBudget {
    used: AtomicUsize,
}

impl ResidentMemoryBudget {
    fn new() -> Self {
        Self {
            used: AtomicUsize::new(0),
        }
    }

    pub(crate) fn try_reserve(
        self: &Arc<Self>,
        bytes: usize,
        limit: usize,
    ) -> Option<ResidentMemoryLease> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|total| *total <= limit)
            })
            .ok()
            .map(|_| ResidentMemoryLease {
                budget: Arc::clone(self),
                bytes,
            })
    }

    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

pub(crate) struct ResidentMemoryLease {
    budget: Arc<ResidentMemoryBudget>,
    bytes: usize,
}

impl Drop for ResidentMemoryLease {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

// SAFETY: SharedPdfium never leaves this module and every access to its Pdfium
// value, including the final Arc drop, occurs while PDFIUM_GATE is held.
unsafe impl Send for SharedPdfium {}
unsafe impl Sync for SharedPdfium {}

#[derive(Clone)]
pub struct PdfEngine {
    // Kept in an Option so Drop can release the last Arc while PDFIUM_GATE is
    // held. Documents clone this exact Arc to keep their borrowed bindings
    // alive independently of the engine.
    pdfium: Option<Arc<SharedPdfium>>,
}

impl PdfEngine {
    pub fn new(dll_dir: &Path) -> PdfResult<Self> {
        let _pdfium_guard = PDFIUM_GATE.lock();
        let registry = PDFIUM_INSTANCE.get_or_init(|| Mutex::new(Weak::new()));
        let mut registered = registry.lock();

        if let Some(pdfium) = registered.upgrade() {
            return Ok(Self {
                pdfium: Some(pdfium),
            });
        }

        let bindings =
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dll_dir))
                .map_err(|e| PdfError::LoadLibrary(e.to_string()))?;
        let pdfium = Arc::new(SharedPdfium {
            pdfium: Pdfium::new(bindings),
            source_bytes_budget: Arc::new(ResidentMemoryBudget::new()),
            text_cache_budget: Arc::new(ResidentMemoryBudget::new()),
        });
        *registered = Arc::downgrade(&pdfium);
        Ok(Self {
            pdfium: Some(pdfium),
        })
    }

    pub fn open(&self, path: &Path) -> PdfResult<Document> {
        // Read the complete file before entering Pdfium. Apart from avoiding
        // disk seeks while scrolling, doing the I/O outside PDFIUM_GATE means
        // a slow drive cannot stall rendering in every other open document.
        let pdfium = self
            .pdfium
            .as_ref()
            .expect("PdfEngine::open() called while PdfEngine is being dropped");
        let (bytes, source_memory_lease) = read_pdf_into_memory(path, &pdfium.source_bytes_budget)?;

        let _pdfium_guard = PDFIUM_GATE.lock();
        let doc = pdfium
            .pdfium
            // PdfDocument owns this Vec and keeps it alive until after
            // FPDF_CloseDocument(), so Pdfium never reads a dangling buffer.
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|e| PdfError::OpenDocument(e.to_string()))?;
        let page_count = doc.pages().len() as u32;

        // SAFETY: `doc` borrows bindings stored inside the heap allocation
        // owned by `pdfium`. The Document below clones that exact Arc, so the
        // allocation cannot move or be destroyed before the PdfDocument. Its
        // custom Drop implementation closes the PdfDocument first and releases
        // the owner Arc second, both while PDFIUM_GATE is held.
        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };
        Ok(Document {
            path: path.to_path_buf(),
            inner: Mutex::new(Some(doc_static)),
            page_count,
            page_sizes_cache: Mutex::new(None),
            text_cache: (0..page_count).map(|_| Mutex::new(None)).collect(),
            text_cache_budget: Arc::clone(&pdfium.text_cache_budget),
            search_generation: AtomicU64::new(0),
            background_cancelled: AtomicBool::new(false),
            source_memory_lease: Some(source_memory_lease),
            pdfium: Some(Arc::clone(pdfium)),
        })
    }
}

/// Reads one stable, complete snapshot of a PDF into an owned buffer.
///
/// Reserving fallibly first is important on Windows: a very large or malformed
/// input should produce a normal application error instead of reaching the
/// allocator's process-abort path. The file handle is closed before Pdfium is
/// entered, so an open document does not pin or lock the source file.
fn read_pdf_into_memory(
    path: &Path,
    budget: &Arc<ResidentMemoryBudget>,
) -> PdfResult<(Vec<u8>, ResidentMemoryLease)> {
    const MAX_RESIDENT_SOURCE_BYTES: usize = 1024 * 1024 * 1024;

    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let len = usize::try_from(file_len).map_err(|_| {
        PdfError::OpenDocument(format!(
            "{} is too large for this process address space ({file_len} bytes)",
            path.display()
        ))
    })?;
    // Keep one spare byte so read_to_end() can observe EOF without attempting
    // an additional infallible growth after the fallible reservation.
    let capacity = len.checked_add(1).ok_or_else(|| {
        PdfError::OpenDocument(format!(
            "{} is too large for this process address space ({file_len} bytes)",
            path.display()
        ))
    })?;
    let source_memory_lease = budget
        .try_reserve(capacity, MAX_RESIDENT_SOURCE_BYTES)
        .ok_or_else(|| {
            PdfError::OpenDocument(format!(
                "opening {} would exceed the 1 GiB process-wide resident PDF limit ({} MiB already in use)",
                path.display(),
                budget.used() / (1024 * 1024)
            ))
        })?;

    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|error| {
        PdfError::OpenDocument(format!(
            "could not reserve {file_len} bytes for {}: {error}",
            path.display()
        ))
    })?;
    file.take(file_len.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() != len {
        return Err(PdfError::OpenDocument(format!(
            "{} changed size while it was being loaded",
            path.display()
        )));
    }
    Ok((bytes, source_memory_lease))
}

pub struct Document {
    pub path: PathBuf,
    pub page_count: u32,
    inner: Mutex<Option<PdfDocument<'static>>>,
    pub(crate) page_sizes_cache: Mutex<Option<Arc<[PageSize]>>>,
    pub(crate) text_cache: Vec<Mutex<Option<Arc<text::CachedTextPage>>>>,
    pub(crate) text_cache_budget: Arc<ResidentMemoryBudget>,
    search_generation: AtomicU64,
    background_cancelled: AtomicBool,
    // Released immediately after PdfDocument drops its owned source Vec.
    source_memory_lease: Option<ResidentMemoryLease>,
    // Must remain alive until `inner` has been closed. See Document::drop().
    pdfium: Option<Arc<SharedPdfium>>,
}

// SAFETY: pdfium-render deliberately leaves Pdfium/PdfDocument !Send and
// !Sync because Pdfium is not thread-safe. This wrapper enforces the stronger
// invariant that every engine/document call and destructor is serialized by
// the single process-wide PDFIUM_GATE. The exact Pdfium allocation borrowed by
// each PdfDocument is also retained by that Document.
unsafe impl Send for Document {}
unsafe impl Sync for Document {}

impl Document {
    pub(crate) fn with_doc<R>(&self, f: impl FnOnce(&PdfDocument<'static>) -> R) -> R {
        // Lock order is always global gate, then document mutex.
        let _pdfium_guard = PDFIUM_GATE.lock();
        let inner = self.inner.lock();
        f(inner
            .as_ref()
            .expect("Document accessed after its PdfDocument was closed"))
    }

    /// Stops best-effort background work associated with this document.
    /// Foreground calls already holding an `Arc<Document>` remain valid.
    pub fn cancel_background_work(&self) {
        self.background_cancelled.store(true, Ordering::Release);
        self.cancel_search();
    }

    pub fn background_work_cancelled(&self) -> bool {
        self.background_cancelled.load(Ordering::Acquire)
    }

    /// Invalidates older searches immediately, before the new search enters
    /// the blocking worker pool.
    pub fn begin_search(&self) -> u64 {
        self.search_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    pub fn cancel_search(&self) {
        self.search_generation.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn search_is_current(&self, generation: u64) -> bool {
        self.search_generation.load(Ordering::Acquire) == generation
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        // Release pure-Rust caches before entering the native destructor gate,
        // and return their process-wide budget only after their allocations
        // have actually been freed.
        self.clear_text_cache();
        drop(self.page_sizes_cache.get_mut().take());

        let _pdfium_guard = PDFIUM_GATE.lock();

        // PdfDocument::drop() calls FPDF_CloseDocument(), so it must run while
        // both the global gate and its exact Pdfium owner are still alive.
        drop(self.inner.get_mut().take());
        drop(self.source_memory_lease.take());
        drop(self.pdfium.take());
    }
}

impl Drop for PdfEngine {
    fn drop(&mut self) {
        let _pdfium_guard = PDFIUM_GATE.lock();

        // If this is the final Arc, Pdfium::drop() calls
        // FPDF_DestroyLibrary() while the process-wide gate is held.
        drop(self.pdfium.take());
    }
}

#[cfg(test)]
mod memory_budget_tests {
    use super::ResidentMemoryBudget;
    use std::sync::Arc;

    #[test]
    fn resident_memory_leases_enforce_the_limit_and_release_exactly() {
        let budget = Arc::new(ResidentMemoryBudget::new());
        let first = budget.try_reserve(7, 10).expect("first reservation");
        assert_eq!(budget.used(), 7);
        assert!(budget.try_reserve(4, 10).is_none());
        assert_eq!(budget.used(), 7);

        let second = budget.try_reserve(3, 10).expect("remaining capacity");
        assert_eq!(budget.used(), 10);
        drop(first);
        assert_eq!(budget.used(), 3);
        drop(second);
        assert_eq!(budget.used(), 0);
    }
}
