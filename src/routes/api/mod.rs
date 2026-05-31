//! API ルートマウント用モジュール（routes 配下）。

use axum::Router;

use crate::api;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().nest("/api", api::router())
}
