use askama::Template;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};

use crate::error::AppResult;
use crate::repos::options;
use crate::state::AppState;

use chrono::DateTime;

pub mod media;
pub mod pages;
pub mod posts;
pub mod settings;
pub mod widgets;

/// データベースに保存されている ISO8601 (UTC, Z suffix) 形式の日時文字列を
/// 管理画面の「更新日」表示用に `YYYY/MM/DD HH:mm` 形式へ変換する。
pub(crate) fn format_updated_at(iso: &str) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.format("%Y/%m/%d %H:%M").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    blogname: String,
    blogdescription: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin", get(dashboard))
        .merge(posts::router())
        .merge(pages::router())
        .merge(widgets::router())
        .merge(media::router())
        .merge(settings::router())
}

async fn dashboard(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());

    let html = DashboardTemplate {
        blogname,
        blogdescription,
    }
    .render()?;
    Ok(Html(html))
}
