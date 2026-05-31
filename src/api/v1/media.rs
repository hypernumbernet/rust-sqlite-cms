use axum::{Json, Router, routing::get};
use serde_json::json;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/media", get(list))
}

async fn list() -> Json<serde_json::Value> {
    Json(json!({ "data": [], "note": "API skeleton - implementation in progress" }))
}
