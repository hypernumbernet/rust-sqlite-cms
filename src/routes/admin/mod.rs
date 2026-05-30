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

pub mod pages;
pub mod posts;

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
