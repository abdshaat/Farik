//! The web app, built into the binary (`docs/SPEC.md` section 8.1).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use super::DaemonState;
use super::web::own_host;

/// What a farik whose web app was not built serves in its place.
const UNBUILT: &str = "The web app was not built into this farik. Run pnpm --filter @farik/web build, then build farik again.";

/// `GET` of any path the other routes do not have: the file of the embed `E` at the path, or else
/// its `index.html`, so that the app's own routes load on refresh. It checks `Host` alone, since a
/// navigation sends no `Origin`, and needs no session, since the app holds no secret. A daemon
/// without the browser routes answers 404.
pub(crate) async fn app_from<E: RustEmbed>(
    State(state): State<Arc<DaemonState>>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let Some(web) = state.web() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !own_host(&headers, web.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let asked = uri.path().trim_start_matches('/');
    let (path, file) = match E::get(asked) {
        Some(file) if !asked.is_empty() => (asked, Some(file)),
        _ => ("index.html", E::get("index.html")),
    };
    // Vite's `assetsDir`, whose file names carry a hash of their content.
    let cache = if path == "index.html" {
        "no-store"
    } else if path.starts_with("assets/") {
        "max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let (mime, body) = match file {
        // Vite writes UTF-8, and a text file without its charset is read as the browser guesses.
        Some(file) => match file.metadata.mimetype() {
            text if text.starts_with("text/") => {
                (format!("{text}; charset=utf-8"), file.data.into_owned())
            }
            other => (other.to_string(), file.data.into_owned()),
        },
        None => (
            "text/html; charset=utf-8".to_string(),
            UNBUILT.as_bytes().to_vec(),
        ),
    };
    let policy = format!(
        "default-src 'self'; connect-src 'self' ws://127.0.0.1:{}; img-src 'self' data:; font-src 'self' data:; style-src 'self'; frame-ancestors 'none'",
        web.port
    );
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache.to_string()),
            (header::CONTENT_SECURITY_POLICY, policy),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use rust_embed::RustEmbed;

    /// A built app of two files, so that no test depends on what `apps/web/dist` holds.
    #[derive(RustEmbed)]
    #[folder = "src/daemon/app-fixture/"]
    pub(crate) struct Fixture;

    /// An app that was not built: an empty folder.
    #[derive(RustEmbed)]
    #[folder = "src/daemon/app-fixture-empty/"]
    pub(crate) struct Unbuilt;
}

/// The built `apps/web/dist`. Debug builds read the folder at run time, release builds embed it,
/// and a folder that is not there is an empty app.
#[derive(RustEmbed)]
#[folder = "../../apps/web/dist"]
#[allow_missing = true]
pub(crate) struct WebApp;
