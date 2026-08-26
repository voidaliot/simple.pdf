pub mod annotations;
pub mod error;
pub mod forms;
pub mod render;
pub mod text;

pub use annotations::{AnnRect, Annotation};
pub use error::{PdfError, PdfResult};
pub use forms::FormField;
pub use render::{PageSize, RenderRequest};
pub use text::TextSpan;

use parking_lot::Mutex;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
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

struct SharedPdfium(Pdfium);

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
        let pdfium = Arc::new(SharedPdfium(Pdfium::new(bindings)));
        *registered = Arc::downgrade(&pdfium);
        Ok(Self { pdfium: Some(pdfium) })
    }

    pub fn open(&self, path: &Path) -> PdfResult<Document> {
        let _pdfium_guard = PDFIUM_GATE.lock();
        let pdfium = self
            .pdfium
            .as_ref()
            .expect("PdfEngine::open() called while PdfEngine is being dropped");
        let doc = pdfium
            .0
            .load_pdf_from_file(path, None)
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
            pdfium: Some(Arc::clone(pdfium)),
        })
    }
}

pub struct Document {
    pub path: PathBuf,
    pub page_count: u32,
    inner: Mutex<Option<PdfDocument<'static>>>,
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

}

impl Drop for Document {
    fn drop(&mut self) {
        let _pdfium_guard = PDFIUM_GATE.lock();

        // PdfDocument::drop() calls FPDF_CloseDocument(), so it must run while
        // both the global gate and its exact Pdfium owner are still alive.
        drop(self.inner.get_mut().take());
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
