pub mod annotations;
pub mod error;
pub mod render;
pub mod text;

pub use annotations::{AnnRect, Annotation, NewInkAnnotation, NewMarkupAnnotation, NewTextAnnotation};
pub use error::{PdfError, PdfResult};
pub use render::{PageSize, RenderRequest};
pub use text::TextSpan;

use parking_lot::Mutex;
use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct PdfEngine {
    // Stored in Arc so it outlives all Documents that borrow from it.
    pdfium: Arc<Pdfium>,
}

impl PdfEngine {
    pub fn new(dll_dir: &Path) -> PdfResult<Self> {
        let bindings = Pdfium::bind_to_library(
            Pdfium::pdfium_platform_library_name_at_path(dll_dir),
        )
        .map_err(|e| PdfError::LoadLibrary(e.to_string()))?;
        Ok(Self { pdfium: Arc::new(Pdfium::new(bindings)) })
    }

    pub fn open(&self, path: &Path) -> PdfResult<Document> {
        let doc = self
            .pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| PdfError::OpenDocument(e.to_string()))?;
        let page_count = doc.pages().len() as u32;

        // SAFETY: Pdfium is in an Arc stored in AppState for the app's entire
        // lifetime. AppState drops documents before dropping PdfEngine, so the
        // Pdfium borrow is always valid while any Document exists.
        let doc_static: PdfDocument<'static> = unsafe { std::mem::transmute(doc) };
        Ok(Document {
            path: path.to_path_buf(),
            inner: Arc::new(Mutex::new(doc_static)),
            page_count,
        })
    }
}

pub struct Document {
    pub path: PathBuf,
    pub page_count: u32,
    inner: Arc<Mutex<PdfDocument<'static>>>,
}

impl Document {
    pub fn with_doc<R>(&self, f: impl FnOnce(&PdfDocument<'static>) -> R) -> R {
        f(&self.inner.lock())
    }

    pub fn with_doc_mut<R>(&self, f: impl FnOnce(&mut PdfDocument<'static>) -> R) -> R {
        f(&mut self.inner.lock())
    }
}
