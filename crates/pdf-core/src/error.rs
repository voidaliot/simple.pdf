use thiserror::Error;

pub type PdfResult<T> = Result<T, PdfError>;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("failed to load pdfium library: {0}")]
    LoadLibrary(String),

    #[error("failed to open document: {0}")]
    OpenDocument(String),

    #[error("invalid page index {0}")]
    InvalidPage(u32),

    #[error("render failed: {0}")]
    Render(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
