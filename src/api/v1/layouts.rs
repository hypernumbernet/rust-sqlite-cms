use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::models::layout::{Layout, LayoutInput};
use crate::presets;
use crate::services;
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
struct CreateLayoutRequest {
    key: String,
    name: String,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    shell_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateLayoutRequest {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_default: Option<bool>,
    #[serde(default)]
    shell_content: Option<String>,
}

#[derive(serde::Serialize)]
struct LayoutListResponse {
    items: Vec<Layout>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/layouts", get(list))
        .route("/layouts", post(create))
        .route("/layouts/{id}", get(get_one))
        .route("/layouts/{id}", patch(update))
        .route("/layouts/{id}", delete(delete_one))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<LayoutListResponse>> {
    let items = services::layouts::list_all(&state.pool).await?;
    Ok(Json(LayoutListResponse { items }))
}

async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Layout>> {
    let layout = services::layouts::find(&state.pool, id).await?;
    Ok(Json(layout))
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreateLayoutRequest>,
) -> ApiResult<Json<Layout>> {
    let shell = payload
        .shell_content
        .unwrap_or_else(|| presets::DEFAULT_SHELL.to_string());
    let input = LayoutInput {
        key: payload.key.trim().to_string(),
        name: payload.name.trim().to_string(),
        is_default: payload.is_default,
    };
    if input.key.is_empty() || input.name.is_empty() {
        return Err(ApiError::Validation("key と name は必須です".into()));
    }

    let id = services::layouts::create_layout(&state.pool, &state.config, &input, &shell).await?;
    let created = services::layouts::find(&state.pool, id).await?;
    Ok(Json(created))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateLayoutRequest>,
) -> ApiResult<Json<Layout>> {
    let current = services::layouts::find(&state.pool, id).await?;
    let shell = match payload.shell_content {
        Some(content) => content,
        None => theme::read_shell(&state.config.paths.work_dir, &current.key).unwrap_or_default(),
    };
    let input = LayoutInput {
        key: payload.key.unwrap_or(current.key),
        name: payload.name.unwrap_or(current.name),
        is_default: payload.is_default.unwrap_or(current.is_default),
    };

    services::layouts::update_layout(&state.pool, &state.config, id, &input, &shell).await?;
    let updated = services::layouts::find(&state.pool, id).await?;
    Ok(Json(updated))
}

async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    services::layouts::delete_layout(&state.pool, &state.config, id).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
