use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFormat {
    Markdown,
}

pub fn text_format(path: &Path) -> Option<TextFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Some(TextFormat::Markdown),
        _ => None,
    }
}

pub fn is_supported(path: &Path) -> bool {
    text_format(path).is_some()
        || path
            .extension()
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

#[derive(Serialize)]
pub struct TextDocument {
    pub path: String,
    pub title: String,
    pub format: TextFormat,
    pub source: String,
}

fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    let source = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes[0] == 0xff;
        if bytes.len() % 2 != 0 {
            return Err("Document contains incomplete UTF-16 text".into());
        }
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| {
                let pair = [pair[0], pair[1]];
                if little_endian {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "Document is not valid UTF-16 text")?
    } else {
        String::from_utf8(bytes).map_err(|_| "Save the document as UTF-8 or UTF-16 text")?
    };
    if source.contains('\0') {
        return Err("Document contains binary data; expected Markdown text".into());
    }
    Ok(source
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn read_document(path: &Path) -> Result<TextDocument, String> {
    let format = text_format(path).ok_or("Unsupported Markdown file extension")?;
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    let file = std::fs::File::open(&canonical).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Choose a file, not a folder".into());
    }
    if metadata.len() > MAX_TEXT_BYTES {
        return Err("Markdown files are limited to 2 MB".into());
    }
    // Bound the read too: the file may grow after metadata was inspected.
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err("Markdown files are limited to 2 MB".into());
    }
    Ok(TextDocument {
        title: canonical
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled")
            .into(),
        path: crate::commands::user_display_path(&canonical),
        format,
        source: decode_text(bytes)?,
    })
}

#[tauri::command]
pub async fn open_text_document(path: String) -> Result<TextDocument, String> {
    tokio::task::spawn_blocking(move || read_document(Path::new(&path)))
        .await
        .map_err(|error| format!("Could not read document: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_are_case_insensitive_and_exact() {
        for extension in ["PDF", "md", "MARKDOWN"] {
            assert!(is_supported(Path::new(&format!("example.{extension}"))));
        }
        for name in [
            "diagram.puml.exe",
            "notes.txt",
            "md",
            "diagram",
            "flow.mmd",
            "flow.MERMAID",
            "class.puml",
            "class.PLANTUML",
            "class.pu",
            "class.uml",
        ] {
            assert!(!is_supported(Path::new(name)));
        }
    }

    #[test]
    fn resolves_cli_paths_against_the_sending_instance() {
        let cwd = Path::new("C:\\documents");
        let paths = file_args(
            vec![
                "notes.md".into(),
                "--help".into(),
                "E:\\document.pdf".into(),
            ],
            cwd,
        );
        assert_eq!(
            paths,
            vec![cwd.join("notes.md"), PathBuf::from("E:\\document.pdf")]
        );
    }

    #[test]
    fn decodes_boms_newlines_and_unicode() {
        assert_eq!(
            decode_text(b"\xef\xbb\xbfgraph TD\r\nA-->B\r".to_vec()).unwrap(),
            "graph TD\nA-->B\n"
        );
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in "Alice -> Bob : Grüß 👋".encode_utf16() {
                bytes.extend(if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(decode_text(bytes).unwrap(), "Alice -> Bob : Grüß 👋");
        }
        assert!(decode_text(vec![0xff, 0xfe, 0x41]).is_err());
        assert!(decode_text(vec![0xff]).is_err());
        assert!(decode_text(vec![0]).is_err());
    }

    #[test]
    fn rejects_oversized_files_without_reading_them() {
        let path =
            std::env::temp_dir().join(format!("simplepdf_text_test_{}.md", uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_TEXT_BYTES + 1).unwrap();
        drop(file);
        let result = read_document(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.err().unwrap().contains("2 MB"));
    }
}
