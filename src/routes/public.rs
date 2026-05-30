use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use minijinja::context;
use serde::Serialize;

use crate::error::AppResult;
use crate::models::post::Post;
use crate::repos::{options, posts};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
struct NewsItem {
    title: String,
    excerpt: String,
    display_date: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());
    let theme = options::get(&state.pool, "active_theme")
        .await?
        .unwrap_or_else(|| state.config.theme.active.clone());
    let news = posts::list_published(&state.pool, 5)
        .await?
        .into_iter()
        .map(NewsItem::from)
        .collect::<Vec<_>>();

    let html = state.templates.render(
        &theme,
        "index.html",
        context! {
            blogname,
            blogdescription,
            has_news => !news.is_empty(),
            news,
        },
    )?;

    Ok(Html(html))
}

impl From<Post> for NewsItem {
    fn from(post: Post) -> Self {
        let display_date = post.published_at.unwrap_or(post.created_at);
        let excerpt = if post.excerpt.trim().is_empty() {
            post.content
        } else {
            post.excerpt
        };

        Self {
            title: post.title,
            excerpt,
            display_date,
        }
    }
}
