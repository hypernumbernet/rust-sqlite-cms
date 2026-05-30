use axum::Router;

use crate::state::AppState;

pub mod admin;
pub mod public;

pub fn router() -> Router<AppState> {
    Router::new().merge(public::router()).merge(admin::router())
}
