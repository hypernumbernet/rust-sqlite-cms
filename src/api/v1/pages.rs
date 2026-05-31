use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::models::page::{Page, PageInput};
use crate::routes::url::normalize_url_path;
use crate::services;
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
struct CreatePageRequest {
    name: String,
    #[serde(default)]
    url_path: Option<String>,
    content: String,
    #[serde(default)]
    is_static: bool,
    #[serde(default)]
    is_published: bool,
}

#[derive(Debug, Deserialize)]
struct UpdatePageRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    is_static: Option<bool>,
    #[serde(default)]
    is_published: Option<bool>,
}

#[derive(serde::Serialize)]
struct PageListResponse {
    items: Vec<Page>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pages", get(list))
        .route("/pages", post(create))
        .route("/pages/{id}", get(get_one))
        .route("/pages/{id}", patch(update))
        .route("/pages/{id}", delete(delete_page))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<PageListResponse>> {
    let items = services::pages::list_all(&state.pool).await?;
    Ok(Json(PageListResponse { items }))
}

async fn get_one(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Page>> {
    let page = services::pages::find(&state.pool, id).await?;
    Ok(Json(page))
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<CreatePageRequest>,
) -> ApiResult<Json<Page>> {
    // 簡単な入力正規化（API 側でも最低限のチェック）
    let url_path = payload.url_path.as_deref().and_then(normalize_url_path);

    if payload.is_published && url_path.is_none() {
        return Err(ApiError::Validation("公開するには url_path を指定してください".into()));
    }

    let input = PageInput {
        name: payload.name.trim().to_string(),
        url_path,
        content: payload.content,
        is_static: payload.is_static,
        is_published: payload.is_published,
    };

    let (id, _) = services::pages::create_page(&state.pool, &state.config, &input)
        .await
        ?;

    let created = services::pages::find(&state.pool, id).await?;
    Ok(Json(created))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePageRequest>,
) -> ApiResult<Json<Page>> {
    let current = services::pages::find(&state.pool, id).await?;

    let input = PageInput {
        name: payload.name.unwrap_or(current.name),
        url_path: payload
            .url_path
            .as_deref()
            .map(|s| normalize_url_path(s))
            .unwrap_or(current.url_path),
        content: payload.content.unwrap_or_else(|| {
            // content が指定されなかった場合は現在のファイルを読み直す
            current
                .file_name
                .as_ref()
                .and_then(|f| {
                    theme::read_page_content(&state.config.paths.work_dir, f, current.is_static).ok()
                })
                .unwrap_or_default()
        }),
        is_static: payload.is_static.unwrap_or(current.is_static),
        is_published: payload.is_published.unwrap_or(current.is_published),
    };

    services::pages::update_page(&state.pool, &state.config, id, &input)
        .await
        ?;

    let updated = services::pages::find(&state.pool, id).await?;
    Ok(Json(updated))
}

async fn delete_page(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    services::pages::delete_page(&state.pool, &state.config, id)
        .await
        ?;

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

// プレビューなどは既存の /admin/pages/{id}/preview を再利用（APIからは不要な場合が多い）

