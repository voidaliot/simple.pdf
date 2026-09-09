use std::path::{Path, PathBuf};

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

/// A second instance may have a different working directory from the first.
pub fn file_args(argv: Vec<String>, cwd: &Path) -> Vec<PathBuf> {
    argv.into_iter()
        .map(PathBuf::from)
        .filter(|path| is_supported(path))
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pdf_extensions_are_supported_case_insensitively() {
        for name in ["example.pdf", "example.PDF", "example.PdF"] {
            assert!(is_supported(Path::new(name)));
        }
        for name in [
            "notes.md",
            "notes.MARKDOWN",
            "flow.mmd",
            "flow.MERMAID",
            "class.puml",
            "class.PLANTUML",
            "class.pu",
            "class.uml",
            "file.pdf.exe",
            "notes.txt",
            "pdf",
            "folder.pdf/file",
        ] {
            assert!(!is_supported(Path::new(name)));
        }
    }

    #[test]
    fn cli_filters_non_pdf_files_and_resolves_paths_against_the_sending_instance() {
        let cwd = Path::new("C:\\documents");
        let paths = file_args(
            vec![
                "notes.md".into(),
                "--help".into(),
                "relative.PDF".into(),
                "E:\\document.pdf".into(),
                "flow.mmd".into(),
            ],
            cwd,
        );
        assert_eq!(
            paths,
            vec![cwd.join("relative.PDF"), PathBuf::from("E:\\document.pdf")]
        );
    }
}
