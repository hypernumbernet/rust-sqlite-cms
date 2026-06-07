use askama::Template;
use axum::{
    Router,
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    middleware,
    response::{Html, IntoResponse},
    routing::get,
};

const ADMIN_FAVICON: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/admin-favicon.ico"));

use crate::error::AppResult;
use crate::repos::options;
use crate::state::AppState;

use chrono::DateTime;

pub mod auth;
pub mod backup;
pub mod database;
pub mod layout;
pub mod layouts;
pub mod media;
pub mod pages;
pub mod posts;
pub mod samples;
pub mod settings;
pub mod users;
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
    layout: layout::AdminLayoutCtx,
    blogname: String,
    blogdescription: String,
}

pub fn router() -> Router<AppState> {
    let public = auth::router()
        .route("/admin/favicon.ico", get(admin_favicon));

    let protected = Router::new()
        .route("/admin", get(dashboard))
        .merge(posts::router())
        .merge(pages::router())
        .merge(layouts::router())
        .merge(widgets::router())
        .merge(media::router())
        .merge(settings::router())
        .merge(users::router())
        .merge(samples::router())
        .merge(backup::router())
        .merge(database::router())
        .route_layer(middleware::from_fn(auth::require_admin_auth));

    public.merge(protected)
}

async fn dashboard(
    auth: auth::AuthUser,
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
    let blogname = options::get(&state.pool(), "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool(), "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());

    let html = DashboardTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        blogname,
        blogdescription,
    }
    .render()?;
    Ok(Html(html))
}

async fn admin_favicon() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/x-icon"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (headers, ADMIN_FAVICON)
}
