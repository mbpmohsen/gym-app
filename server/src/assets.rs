//! Static files embedded in the executable: the built web UI (web/dist) and the
//! voice files (assets/voices). Release builds need `npm run build` in web/ first;
//! debug builds read from disk, so the UI can be rebuilt without recompiling.

use axum::{
    extract::Path,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web/dist"]
struct Web;

#[derive(RustEmbed)]
#[folder = "../assets/voices"]
struct Voices;

fn serve<E: RustEmbed>(path: &str, cache: &str) -> Option<Response> {
    let file = E::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(([(header::CONTENT_TYPE, mime.as_ref().to_string()), (header::CACHE_CONTROL, cache.to_string())], file.data).into_response())
}

/// SPA: real files as-is; any other non-API path gets index.html (client-side routing).
pub async fn web(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() {
        // Vite puts hashed files in assets/: cache them for long
        let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        if let Some(r) = serve::<Web>(path, cache) {
            return r;
        }
    }
    serve::<Web>("index.html", "no-cache")
        .unwrap_or_else(|| (StatusCode::NOT_FOUND, "web UI not built: run `npm run build` in web/").into_response())
}

/// GET /voices/{gender}/{name}  e.g. /voices/female/welcome.mp3
pub async fn voice(Path((gender, name)): Path<(String, String)>) -> Response {
    if !matches!(gender.as_str(), "male" | "female") || name.contains(['/', '\\']) || name.starts_with('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve::<Voices>(&format!("{gender}/{name}"), "public, max-age=86400")
        .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}
