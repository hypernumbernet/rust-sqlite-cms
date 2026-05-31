use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::models::placeholder::{Placeholder, PlaceholderInput};
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct CreatePlaceholderRequest {
    name: String,
    widget_type_id: i64,
}

#[derive(Debug, Deserialize)]
struct UpdatePlaceholderRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    widget_type_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreatePostRequest {
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default = "default_publish_status")]
    post_status: String,
    #[serde(default)]
    post_name: String,
    /// image widget など向けの追加メタ（media_id, float, margin など）
    #[serde(default)]
    meta: std::collections::HashMap<String, String>,
}

fn default_publish_status() -> String {
    "draft".to_string()
}

#[derive(serde::Serialize)]
struct PlaceholderListResponse {
    items: Vec<Placeholder>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/placeholders", get(list))
        .route("/placeholders", post(create))
        .route("/placeholders/{id}", get(get_one))
        .route("/placeholders/{id}", patch(update))
        .route("/placeholders/{id}", delete(delete_one))
        .route("/placeholders/{id}/posts", get(list_posts))
        .route("/placeholders/{id}/posts", post(create_post))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<PlaceholderListResponse>> {
    let items = services::placeholders::list_all(&state.pool).await?;
    Ok(Json(PlaceholderListResponse { items }))
}

async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Placeholder>> {
    let p = services::placeholders::find(&state.pool, id).await.map_err(ApiError::from)?;
    Ok(Json(p))
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreatePlaceholderRequest>,
) -> ApiResult<Json<Placeholder>> {
    let input = PlaceholderInput {
        name: payload.name,
        widget_type_id: payload.widget_type_id,
    };

    let created = services::placeholders::create(&state.pool, input)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(created))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePlaceholderRequest>,
) -> ApiResult<Json<Placeholder>> {
    let current = services::placeholders::find(&state.pool, id).await.map_err(ApiError::from)?;

    let input = PlaceholderInput {
        name: payload.name.unwrap_or(current.name),
        widget_type_id: payload.widget_type_id.unwrap_or(current.widget_type_id),
    };

    services::placeholders::update(&state.pool, id, input)
        .await
        .map_err(ApiError::from)?;

    let updated = services::placeholders::find(&state.pool, id).await.map_err(ApiError::from)?;
    Ok(Json(updated))
}

async fn delete_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    services::placeholders::delete(&state.pool, id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

async fn list_posts(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    // プレースホルダー存在確認
    let _ = services::placeholders::find(&state.pool, id).await.map_err(ApiError::from)?;

    let posts = services::posts::list_for_placeholder(&state.pool, id).await.map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "placeholder_id": id, "items": posts })))
}

async fn create_post(
    State(state): State<AppState>,
    Path(placeholder_id): Path<i64>,
    Json(payload): Json<CreatePostRequest>,
) -> ApiResult<Json<crate::models::post::Post>> {
    // プレースホルダー存在確認（widget_type も後で使えるように）
    let _ = services::placeholders::find(&state.pool, placeholder_id)
        .await
        .map_err(ApiError::from)?;

    let input = crate::models::post::PostInput {
        placeholder_id,
        title: payload.title,
        content: payload.content,
        excerpt: payload.excerpt,
        post_status: payload.post_status,
        post_name: payload.post_name,
    };

    let meta = if payload.meta.is_empty() { None } else { Some(payload.meta) };

    let created = services::posts::create(&state.pool, input, meta)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(created))
}

