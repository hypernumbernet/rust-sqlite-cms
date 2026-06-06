use axum::{
    extract::{Path, State},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::models::widget::{WidgetImportAction, WidgetImportMode, WidgetPackage, WidgetType};
use crate::repos::widget_types as widget_types_repo;
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct UpdateWidgetConfigRequest {
    config: String,
    /// 任意。省略時は既存の html_template を維持（後方互換）。
    #[serde(default)]
    html_template: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportWidgetRequest {
    package: WidgetPackage,
    #[serde(default = "default_import_mode")]
    mode: String,
    #[serde(default)]
    target_key: Option<String>,
}

fn default_import_mode() -> String {
    "overwrite".to_string()
}

#[derive(serde::Serialize)]
struct WidgetListResponse {
    items: Vec<WidgetType>,
}

#[derive(serde::Serialize)]
struct ImportWidgetResponse {
    type_key: String,
    action: String,
    message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/widgets", get(list))
        .route("/widgets/import", post(import_widget))
        .route("/widgets/{type_key}/export", get(export_widget))
        .route("/widgets/{type_key}", patch(update_config).delete(destroy))
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<WidgetListResponse>> {
    let items = services::widgets::list_all(&state.pool).await?;
    Ok(Json(WidgetListResponse { items }))
}

async fn export_widget(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
) -> ApiResult<Json<WidgetPackage>> {
    let package = services::widgets::export_package(&state.pool, &type_key).await?;
    Ok(Json(package))
}

async fn import_widget(
    State(state): State<AppState>,
    Json(payload): Json<ImportWidgetRequest>,
) -> ApiResult<Json<ImportWidgetResponse>> {
    let mode = parse_import_mode(&payload.mode)?;
    let target_key = payload
        .target_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    let (action, message) = services::widgets::import_package(
        &state.pool,
        &payload.package,
        mode,
        target_key,
    )
    .await?;

    let type_key = if mode == WidgetImportMode::Rename {
        target_key.expect("rename requires target_key").to_string()
    } else {
        payload.package.type_key.trim().to_string()
    };
    Ok(Json(ImportWidgetResponse {
        type_key,
        action: import_action_str(action).to_string(),
        message,
    }))
}

fn parse_import_mode(mode: &str) -> ApiResult<WidgetImportMode> {
    match mode.trim() {
        "overwrite" => Ok(WidgetImportMode::Overwrite),
        "skip" => Ok(WidgetImportMode::Skip),
        "rename" => Ok(WidgetImportMode::Rename),
        other => Err(ApiError::Validation(format!(
            "mode は overwrite / skip / rename を指定してください（got: {other}）"
        ))),
    }
}

fn import_action_str(action: WidgetImportAction) -> &'static str {
    match action {
        WidgetImportAction::Created => "created",
        WidgetImportAction::Updated => "updated",
        WidgetImportAction::Skipped => "skipped",
    }
}

async fn destroy(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    services::widgets::delete(&state.pool, &type_key).await?;
    Ok(Json(serde_json::json!({
        "type_key": type_key,
        "action": "deleted"
    })))
}

async fn update_config(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
    Json(payload): Json<UpdateWidgetConfigRequest>,
) -> ApiResult<Json<WidgetType>> {
    let current = widget_types_repo::find_by_key(&state.pool, &type_key).await?;

    let current_html = if let Some(h) = &payload.html_template {
        h.clone()
    } else {
        current.html_template
    };

    services::widgets::update_config(&state.pool, &type_key, &payload.config, &current_html).await?;

    let updated = widget_types_repo::find_by_key(&state.pool, &type_key).await?;
    Ok(Json(updated))
}
