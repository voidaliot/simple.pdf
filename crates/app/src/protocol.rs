use crate::state::AppState;
use pdf_core::RenderRequest;
use tauri::{AppHandle, Manager};
use tauri::http::{Request, Response};

pub fn handle_pdf_request(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match render(app, &request) {
        Ok(png) => Response::builder()
            .status(200)
            .header("Content-Type", "image/png")
            .header("Cache-Control", "private, max-age=3600")
            .body(png)
            .unwrap(),
        Err(e) => {
            tracing::warn!("pdf:// render error: {e}");
            Response::builder()
                .status(500)
                .header("Content-Type", "text/plain")
                .body(e.to_string().into_bytes())
                .unwrap()
        }
    }
}

fn render(app: &AppHandle, request: &Request<Vec<u8>>) -> Result<Vec<u8>, String> {
    // URL format: pdf://localhost/page/{docId}/{pageIndex}?scale={scale}
    let uri = request.uri();
    let path = uri.path();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    if segments.len() < 3 || segments[0] != "page" {
        return Err(format!("invalid pdf:// path: {path}"));
    }
    let doc_id = uuid::Uuid::parse_str(segments[1]).map_err(|e| e.to_string())?;
    let page_index: u32 = segments[2].parse().map_err(|e: std::num::ParseIntError| e.to_string())?;

    let scale: f32 = uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find(|p| p.starts_with("scale="))
                .and_then(|p| p["scale=".len()..].parse().ok())
        })
        .unwrap_or(1.5);

    let state = app.state::<AppState>();
    let doc = {
        let map = state.docs.lock();
        map.get(&doc_id)
            .cloned()
            .ok_or_else(|| format!("unknown doc {doc_id}"))?
    };

    doc.render_page_png(RenderRequest { page_index, scale })
        .map_err(|e| e.to_string())
}
