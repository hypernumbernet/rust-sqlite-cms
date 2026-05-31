use axum::response::Html;
use minijinja::Value;

use crate::error::{AppError, AppResult};
use crate::models::page::Page;
use crate::repos::options;
use crate::state::AppState;
use crate::theme;
use crate::widgets;

/// ページ本文を公開サイトと同じパイプラインで描画する。
pub async fn render_page(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    let file_name = page.file_name.as_deref().ok_or(AppError::NotFound)?;

    if page.is_static {
        let html = theme::read_page_content(&state.config.paths.work_dir, file_name, true)?;
        return Ok(Html(html));
    }

    let ctx = build_site_context(state).await?;
    let html = state.templates.render(file_name, ctx)?;

    Ok(Html(html))
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
