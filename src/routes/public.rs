use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use tokio::fs;

use crate::error::{AppError, AppResult};
use crate::page_render;
use crate::repos::pages;
use crate::state::AppState;
use crate::theme;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/static/{*path}", get(serve_layout_static))
}

async fn home(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let page = pages::find_home(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    if !page.is_published {
        return Err(AppError::NotFound);
    }

    page_render::render_page(&state, &page).await
}

/// `work/layouts/{layout_key}/static/*` を `/static/{layout_key}/*` で配信する。
async fn serve_layout_static(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Response, AppError> {
    let Some(file_path) = theme::resolve_static_path(&state.config.paths.work_dir, &path) else {
        return Err(AppError::NotFound);
    };

    let bytes = fs::read(&file_path).await.map_err(|_| AppError::NotFound)?;
    let content_type = theme::content_type_for_path(&file_path);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap())
}

/// 既存ルートに一致しなかったパスを、公開済みページとして配信する。
pub async fn serve_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
) -> AppResult<impl IntoResponse> {
    let path = normalize_path(uri.path());

    if is_reserved_public_path(&path) {
        return Err(AppError::NotFound);
    }

    let page = pages::find_published_by_path(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;

    page_render::render_page(&state, &page).await
}

fn is_reserved_public_path(path: &str) -> bool {
    path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
        || path == "/api"
        || path.starts_with("/api/")
}

/// URL を正規化する。ルート以外の末尾スラッシュを取り除く。
fn normalize_path(path: &str) -> String {
    if path.len() > 1 && path.ends_with('/') {
        path.trim_end_matches('/').to_string()
    } else {
        path.to_string()
    }
}
