#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod commands;
mod protocol;
mod state;
mod thumb;

use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

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
            // page data
            commands::get_page_sizes,
            commands::get_page_text_spans,
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
            // network
            commands::download_url_to_temp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running simple.pdf");
}
