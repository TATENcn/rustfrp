//! Web UI static asset serving and SPA fallback
//!
//! Uses `rust-embed` to compile `plugins/webui/dist/` into the binary
//! at compile time. Serves `/assets/*` for static files and falls back
//! to `index.html` for all other non-API routes (SPA routing).

use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// Compile-time embedded web UI assets.
///
/// The path is relative to the daemon crate's Cargo.toml.
#[derive(RustEmbed)]
#[folder = "../../plugins/webui/dist/"]
pub struct WebAssets;

/// Serve static assets under `/assets/*` (JS, CSS, images, fonts).
///
/// Separated from the SPA fallback so that browser module script
/// requests (`<script type="module" src="/assets/index-xxx.js">`)
/// get the correct MIME type rather than `text/html`.
pub async fn serve_asset(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();

    if let Some(file) = WebAssets::get(&path) {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data))
            .unwrap()
    } else {
        (StatusCode::NOT_FOUND, "asset not found").into_response()
    }
}

/// SPA fallback: return `index.html` for all non-API, non-asset routes.
///
/// Vue Router handles client-side routing based on the URL hash/path,
/// so every non-API URL must serve the SPA shell.
pub async fn serve_index() -> impl IntoResponse {
    match WebAssets::get("index.html") {
        Some(file) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(file.data))
            .unwrap(),
        None => (
            StatusCode::NOT_FOUND,
            "Web UI not built. Run: cd plugins/webui && bun run build",
        )
            .into_response(),
    }
}
