use std::path::PathBuf;

use axum::Router;
use tower_http::services::ServeDir;

use crate::state::AppState;

pub mod admin;
pub mod public;
pub mod url;

pub fn router(static_dir: PathBuf, uploads_dir: PathBuf) -> Router<AppState> {
    Router::new()
        .merge(public::router())
        .merge(admin::router())
        .nest_service("/static", ServeDir::new(static_dir))
        .nest_service("/uploads", ServeDir::new(uploads_dir))
        .fallback(public::serve_fallback)
}
