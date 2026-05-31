//! API v1 ルーター。

use axum::Router;

use crate::state::AppState;

pub mod placeholders;
pub mod posts;
pub mod pages;
pub mod widgets;
pub mod media;
pub mod settings;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(placeholders::router())
        .merge(posts::router())
        .merge(pages::router())
        .merge(widgets::router())
        .merge(media::router())
        .merge(settings::router())
}
