use axum::response::Html;
use minijinja::Value;

use crate::error::AppResult;
use crate::models::page::Page;
use crate::repos::options;
use crate::state::AppState;
use crate::widgets;

/// ページ本文を公開サイトと同じパイプラインで描画する。
pub async fn render_page(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    render_page_with_options(state, page, widgets::RenderOptions::default()).await
}

/// プレビュー用: ウィジェット領域に編集モード向けマーカーを付与して描画する。
pub async fn render_page_preview(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    render_page_with_options(
        state,
        page,
        widgets::RenderOptions {
            annotate_widgets: true,
        },
    )
    .await
}

async fn render_page_with_options(
    state: &AppState,
    page: &Page,
    options: widgets::RenderOptions,
) -> AppResult<Html<String>> {
    let ctx = build_site_context(state, options).await?;
    let html = state.templates.render(&page.template_name(), ctx)?;
    Ok(Html(html))
}

async fn build_site_context(state: &AppState, options: widgets::RenderOptions) -> AppResult<Value> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());

    widgets::build_render_context(&state.pool, blogname, blogdescription, options).await
}
