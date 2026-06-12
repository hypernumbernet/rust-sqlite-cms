use axum::response::Html;
use minijinja::Value;

use crate::error::AppResult;
use crate::models::page::Page;
use crate::repos::options;
use crate::services;
use crate::session;
use crate::state::AppState;
use crate::widgets;

/// 公開ページ描画時の追加オプション（クエリパラメータ由来など）。
#[derive(Debug, Clone, Default)]
pub struct RenderPageOptions {
    pub contact_sent: Option<String>,
    pub contact_error: Option<String>,
}

/// ページ本文を公開サイトと同じパイプラインで描画する。
pub async fn render_page(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    render_page_with_query(state, page, RenderPageOptions::default()).await
}

/// クエリパラメータ等を反映して公開ページを描画する。
pub async fn render_page_with_query(
    state: &AppState,
    page: &Page,
    page_options: RenderPageOptions,
) -> AppResult<Html<String>> {
    let widget_options = build_widget_options(state, page_options);
    render_page_with_options(state, page, widget_options, RenderPageOptions::default()).await
}

/// プレビュー用: ウィジェット領域に編集モード向けマーカーを付与して描画する。
pub async fn render_page_preview(state: &AppState, page: &Page) -> AppResult<Html<String>> {
    let mut widget_options = build_widget_options(state, RenderPageOptions::default());
    widget_options.annotate_widgets = true;
    render_page_with_options(state, page, widget_options, RenderPageOptions::default()).await
}

async fn render_page_with_options(
    state: &AppState,
    page: &Page,
    widget_options: widgets::RenderOptions,
    _page_options: RenderPageOptions,
) -> AppResult<Html<String>> {
    let ctx = build_site_context(state, widget_options).await?;
    let html = state.templates().render(&page.template_name(), ctx)?;
    Ok(Html(html))
}

fn build_widget_options(state: &AppState, page_options: RenderPageOptions) -> widgets::RenderOptions {
    widgets::RenderOptions {
        annotate_widgets: false,
        contact_sent: page_options.contact_sent,
        contact_error: page_options.contact_error,
        session_secret: Some(session::resolve_session_secret(&state.config)),
    }
}

async fn build_site_context(
    state: &AppState,
    options: widgets::RenderOptions,
) -> AppResult<Value> {
    let blogname = options::get(&state.pool(), "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool(), "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());
    let favicon_url = services::media::site_favicon_url(&state.pool())
        .await
        .unwrap_or_default();

    widgets::build_render_context(
        &state.pool(),
        blogname,
        blogdescription,
        favicon_url,
        options,
    )
    .await
}
