use crate::state::AppState;
use pdf_core::RenderRequest;
use tauri::{Manager, Runtime, UriSchemeContext};
use tauri::http::{Request, Response};

pub fn handle_thumb_request<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let app = ctx.app_handle();
    match render(app, &request) {
        Ok(png) => Response::builder()
            .status(200)
            .header("Content-Type", "image/png")
            .header("Cache-Control", "private, max-age=86400")
            .body(png)
            .unwrap(),
        Err(e) => {
            tracing::warn!("thumb:// render error: {e}");
            Response::builder()
                .status(500)
                .header("Content-Type", "text/plain")
                .body(e.into_bytes())
                .unwrap()
        }
    }
}

fn render<R: Runtime>(
    app: &tauri::AppHandle<R>,
    request: &Request<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let uri = request.uri();
    let query = uri.query().unwrap_or("");

    let path_encoded = query
        .split('&')
        .find(|p| p.starts_with("path="))
        .and_then(|p| p.strip_prefix("path="))
        .ok_or("missing path param")?;

    let path_str = urlencoding::decode(path_encoded).map_err(|e| e.to_string())?;
    let path = std::path::Path::new(path_str.as_ref());

    let max_w: f32 = query
        .split('&')
        .find(|p| p.starts_with("maxw="))
        .and_then(|p| p.strip_prefix("maxw=").and_then(|v| v.parse().ok()))
        .unwrap_or(240.0);

    let state = app.state::<AppState>();
    let doc = state.engine.open(path).map_err(|e| e.to_string())?;
    let sizes = doc.page_sizes().map_err(|e| e.to_string())?;
    let page_w = sizes.first().map(|s| s.width).unwrap_or(612.0);
    let scale = (max_w / page_w).min(1.0).max(0.05);
    doc.render_page_png(RenderRequest { page_index: 0, scale })
        .map_err(|e| e.to_string())
}
