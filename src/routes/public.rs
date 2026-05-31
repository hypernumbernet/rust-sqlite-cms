use axum::{
    Router,
    extract::{OriginalUri, State},
    response::{Html, IntoResponse},
    routing::get,
};
use minijinja::Value;

use crate::error::{AppError, AppResult};
use crate::models::page::Page;
use crate::repos::{options, pages};
use crate::state::AppState;
use crate::theme;
use crate::widgets;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let page = pages::find_by_file_name(&state.pool, "index.html")
        .await?
        .ok_or(AppError::NotFound)?;

    if !page.is_published {
        return Err(AppError::NotFound);
    }

    render_page(&state, &page).await
}

/// 既存ルートに一致しなかったパスを、公開済みページとして配信する。
pub async fn serve_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
) -> AppResult<impl IntoResponse> {
    let path = normalize_path(uri.path());

    // システム名前空間（管理画面・静的配信）は配信の対象外にする。
    if path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
    {
        return Err(AppError::NotFound);
    }

    let page = pages::find_published_by_path(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;

    render_page(&state, &page).await
}

async fn render_page(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    let file_name = page.file_name.as_deref().ok_or(AppError::NotFound)?;

    if page.is_static {
        let html = theme::read_page_content(&state.config.paths.work_dir, file_name, true)?;
        return Ok(Html(html));
    }

    let ctx = build_site_context(state).await?;
    let html = state.templates.render(file_name, ctx)?;

    Ok(Html(html))
}

/// URL を正規化する。ルート以外の末尾スラッシュを取り除く。
fn normalize_path(path: &str) -> String {
    if path.len() > 1 {
        path.trim_end_matches('/').to_string()
    } else {
        path.to_string()
    }
}

async fn build_site_context(state: &AppState) -> AppResult<Value> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());

    widgets::build_render_context(&state.pool, blogname, blogdescription).await
}
