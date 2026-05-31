use axum::{
    extract::{Path, State},
    routing::{delete, get, patch},
    Json, Router,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::models::post::Post;
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct UpdatePostRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    excerpt: Option<String>,
    #[serde(default)]
    post_status: Option<String>,
    #[serde(default)]
    post_name: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/posts/{id}", get(get_one))
        .route("/posts/{id}", patch(update))
        .route("/posts/{id}", delete(delete_one))
}

async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Post>> {
    let post = services::posts::find(&state.pool, id).await?;
    Ok(Json(post))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePostRequest>,
) -> ApiResult<Json<Post>> {
    let current = services::posts::find(&state.pool, id).await?;

    let input = crate::models::post::PostInput {
        placeholder_id: current.placeholder_id.unwrap_or(0), // 更新時は使われない想定
        title: payload.title.unwrap_or(current.title),
        content: payload.content.unwrap_or(current.content),
        excerpt: payload.excerpt.unwrap_or(current.excerpt),
        post_status: payload.post_status.unwrap_or(current.post_status),
        post_name: payload.post_name.unwrap_or(current.post_name.unwrap_or_default()),
    };

    // メタ更新は現時点では未対応（必要なら後で拡張）
    services::posts::update(&state.pool, id, input, None)
        .await
        ?;

    let updated = services::posts::find(&state.pool, id).await?;
    Ok(Json(updated))
}

async fn delete_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    services::posts::delete(&state.pool, id).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
