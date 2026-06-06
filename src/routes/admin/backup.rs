use askama::Template;
use axum::{
    Router,
    extract::{Multipart, Query, State},
    http::{header, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum::extract::DefaultBodyLimit;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::services::backup::{self, BackupRestoreResult, BackupStats};
use crate::state::AppState;

use super::{auth::AuthUser, layout};

#[derive(Template)]
#[template(path = "admin/backup/index.html")]
struct BackupTemplate {
    layout: layout::AdminLayoutCtx,
    stats: BackupStats,
    database_path: String,
    work_dir: String,
    uploads_dir: String,
    config_path: String,
    success_message: String,
    error_message: String,
    last_result: Option<BackupRestoreResult>,
}

#[derive(Debug, Deserialize)]
struct BackupQuery {
    #[serde(default)]
    success_message: String,
    #[serde(default)]
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/backup", get(show))
        .route("/admin/backup/export", get(export_backup))
        .route(
            "/admin/backup/restore",
            post(restore_backup).layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
}

async fn show(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<BackupQuery>,
) -> AppResult<impl IntoResponse> {
    let stats = backup::collect_stats(&state.pool).await?;
    let html = BackupTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        stats,
        database_path: state.config.database.path.clone(),
        work_dir: state.config.paths.work_dir.clone(),
        uploads_dir: state.config.paths.uploads_dir.clone(),
        config_path: backup::config_display_path(),
        success_message: query.success_message,
        error_message: query.error_message,
        last_result: None,
    }
    .render()?;
    Ok(Html(html))
}

async fn export_backup(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> AppResult<Response> {
    let bytes = backup::export_site_backup(&state.pool, &state.config).await?;
    let filename = backup::export_filename();
    let disposition = format!("attachment; filename=\"{filename}\"");

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|e| AppError::Other(e.into()))?,
    );

    Ok((headers, bytes).into_response())
}

async fn restore_backup(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut package_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Other(err.into()))?
    {
        if field.name() == Some("package") {
            let data = field
                .bytes()
                .await
                .map_err(|err| AppError::Other(err.into()))?;
            package_bytes = Some(data.to_vec());
        }
    }

    let Some(bytes) = package_bytes else {
        return Ok(redirect_with_error("ZIP ファイル（package）を選択してください"));
    };

    match backup::import_site_backup(&state.pool, &state.config, &bytes).await {
        Ok(result) => Ok(render_result(auth, state, result, String::new()).await?),
        Err(err) => Ok(redirect_with_error(&err.to_string())),
    }
}

async fn render_result(
    auth: AuthUser,
    state: AppState,
    result: BackupRestoreResult,
    error_message: String,
) -> AppResult<Response> {
    let stats = backup::collect_stats(&state.pool).await?;
    let html = BackupTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        stats,
        database_path: state.config.database.path.clone(),
        work_dir: state.config.paths.work_dir.clone(),
        uploads_dir: state.config.paths.uploads_dir.clone(),
        config_path: backup::config_display_path(),
        success_message: result.message.clone(),
        error_message,
        last_result: Some(result),
    }
    .render()?;
    Ok(Html(html).into_response())
}

fn redirect_with_error(message: &str) -> Response {
    let encoded = urlencoding::encode(message);
    Redirect::to(&format!("/admin/backup?error_message={encoded}")).into_response()
}
