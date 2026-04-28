#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod commands;
mod protocol;
mod state;
mod thumb;

use std::io::Write;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() {
    // Fail fast with a helpful dialog when WebView2 is missing (Windows release only).
    #[cfg(target_os = "windows")]
    if !webview2_present() {
        show_webview2_missing();
        return;
    }

    let _log_guard = init_logging();

    let initial_args: Vec<String> = std::env::args().skip(1).collect();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            state::enqueue_file_args(app, argv.into_iter().skip(1).collect());
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .register_uri_scheme_protocol("pdf", protocol::handle_pdf_request)
        .register_uri_scheme_protocol("thumb", thumb::handle_thumb_request)
        .setup(move |app| {
            state::init(app, initial_args.clone())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // version
            commands::app_version,
            // document lifecycle
            commands::open_document,
            commands::close_document,
            commands::pending_open_files,
            // page rendering — raw RGBA pixels via binary IPC (no image codec)
            commands::render_page_pixels,
            commands::render_thumb_b64,
            // page data
            commands::get_page_sizes,
            commands::get_page_text_spans,
            // forms
            commands::get_form_type,
            commands::get_form_fields,
            commands::set_field_text_value,
            commands::set_field_checked,
            commands::reset_form_fields,
            // annotations
            commands::get_page_annotations,
            commands::add_highlight_annotation,
            commands::add_underline_annotation,
            commands::add_strikeout_annotation,
            commands::add_text_annotation,
            commands::add_ink_annotation,
            commands::remove_annotation,
            commands::undo_annotation,
            commands::save_document,
            // file system
            commands::list_folder_pdfs,
            commands::reveal_in_explorer,
            // file association
            commands::get_pdf_association,
            commands::set_pdf_association,
            // network
            commands::download_url_to_temp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running simple.pdf");
}

// ── App data directory ─────────────────────────────────────────────────────────

/// Returns the data directory for logs and settings.
/// Uses `./data/` next to the exe when `portable.txt` is present; otherwise
/// `%APPDATA%\simple.pdf` on Windows.
fn app_data_dir() -> std::path::PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    if exe_dir.join("portable.txt").exists() {
        return exe_dir.join("data");
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(appdata).join("simple.pdf")
    }
    #[cfg(not(target_os = "windows"))]
    exe_dir.join("data")
}

// ── Logging ───────────────────────────────────────────────────────────────────

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = app_data_dir().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::never(&log_dir, "simple-pdf.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    // Write panics to a separate crash.txt so users can report them.
    let crash_path = log_dir.join("crash.txt");
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.to_string();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_path)
        {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(f, "[{ts}] {msg}");
        }
        tracing::error!("PANIC: {msg}");
    }));

    guard
}

// ── WebView2 detection (Windows) ──────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn webview2_present() -> bool {
    use winreg::{enums::*, RegKey};
    let guid = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let lm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let cu = RegKey::predef(HKEY_CURRENT_USER);
    lm.open_subkey(format!(
        "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{guid}"
    ))
    .is_ok()
        || lm
            .open_subkey(format!(
                "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{guid}"
            ))
            .is_ok()
        || cu
            .open_subkey(format!(
                "Software\\Microsoft\\EdgeUpdate\\Clients\\{guid}"
            ))
            .is_ok()
}

#[cfg(target_os = "windows")]
fn show_webview2_missing() {
    // mshta is available on all Windows versions; no extra deps needed.
    let _ = std::process::Command::new("mshta")
        .arg(concat!(
            "vbscript:msgbox(",
            r#""WebView2 Runtime is required to run simple.pdf."#,
            r#" & chr(10) & chr(10) & "Click OK to open the download page.","#,
            "64,",
            r#""simple.pdf — missing component""#,
            "):close"
        ))
        .status();
    let _ = std::process::Command::new("cmd")
        .args([
            "/c",
            "start",
            "",
            "https://developer.microsoft.com/en-us/microsoft-edge/webview2/",
        ])
        .spawn();
}
