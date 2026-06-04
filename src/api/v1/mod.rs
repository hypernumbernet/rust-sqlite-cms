//! API v1 ルーター。

use axum::{middleware, Router};

use crate::auth::require_api_auth;
use crate::state::AppState;

pub mod layouts;
pub mod placeholders;
pub mod posts;
pub mod pages;
pub mod widgets;
pub mod media;
pub mod settings;
pub mod session;

pub fn router() -> Router<AppState> {
    let public = session::router();

    let protected = Router::new()
        .merge(layouts::router())
        .merge(placeholders::router())
        .merge(posts::router())
        .merge(pages::router())
        .merge(widgets::router())
        .merge(media::router())
        .merge(settings::router())
        .route_layer(middleware::from_fn(require_api_auth));

    public.merge(protected)
}
