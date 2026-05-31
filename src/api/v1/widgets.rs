use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::models::widget::WidgetType;
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct UpdateWidgetConfigRequest {
    config: String,
    /// 任意。省略時は既存の html_template を維持（後方互換）。
    #[serde(default)]
    html_template: Option<String>,
}

#[derive(serde::Serialize)]
struct WidgetListResponse {
    items: Vec<WidgetType>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/widgets", get(list))
        .route("/widgets/{type_key}", patch(update_config))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<WidgetListResponse>> {
    let items = services::widgets::list_all(&state.pool).await?;
    Ok(Json(WidgetListResponse { items }))
}

async fn update_config(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
    Json(payload): Json<UpdateWidgetConfigRequest>,
) -> ApiResult<Json<WidgetType>> {
    // 存在確認
    let _ = services::widgets::list_all(&state.pool).await?; // 簡易
    // より良いのは find_by_key をサービスに追加することだが、現時点は repo 直接回避のため list で代用

    // html_template が省略された場合は既存値を維持（後方互換）
    let current_html = if let Some(h) = &payload.html_template {
        h.clone()
    } else {
        // 簡易: list から探す（本当は find_by_key を使うべき）
        let items = services::widgets::list_all(&state.pool).await?;
        items
            .into_iter()
            .find(|w| w.type_key == type_key)
            .map(|w| w.html_template)
            .unwrap_or_default()
    };

    services::widgets::update_config(&state.pool, &type_key, &payload.config, &current_html).await?;

    // 更新後再取得
    let items = services::widgets::list_all(&state.pool).await?;
    let updated = items
        .into_iter()
        .find(|w| w.type_key == type_key)
        .ok_or(ApiError::NotFound)?;

    Ok(Json(updated))
}

