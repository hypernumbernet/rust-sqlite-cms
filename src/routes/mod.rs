use axum::Router;

use crate::state::AppState;

pub mod admin;
pub mod api;
pub mod public;
pub mod url;

pub fn router(uploads_dir: std::path::PathBuf) -> Router<AppState> {
    Router::new()
        .merge(public::router())
        .merge(admin::router())
        .merge(api::router())
        .nest_service("/uploads", tower_http::services::ServeDir::new(uploads_dir))
        .fallback(public::serve_fallback)
}
