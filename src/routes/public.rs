use axum::{
    Router,
    extract::{OriginalUri, State},
    response::{Html, IntoResponse},
    routing::get,
};
use minijinja::Value;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::models::post::Post;
use crate::repos::{options, posts, templates};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
struct NewsItem {
    title: String,
    excerpt: String,
    display_date: String,
}

/// 公開サイトの描画で共通利用するコンテキスト。
#[derive(Debug, Clone, Serialize)]
struct SiteContext {
    blogname: String,
    blogdescription: String,
    has_news: bool,
    news: Vec<NewsItem>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(home))
}

async fn home(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let ctx = build_site_context(&state).await?;
    let theme = options::get(&state.pool, "active_theme")
        .await?
        .unwrap_or_else(|| state.config.theme.active.clone());

    let html = state
        .templates
        .render(&theme, "index.html", Value::from_serialize(&ctx))?;

    Ok(Html(html))
}

/// 既存ルートに一致しなかったパスを、公開済みテンプレートとして配信する。
pub async fn serve_template(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
) -> AppResult<impl IntoResponse> {
    let path = normalize_path(uri.path());

    // 管理画面の名前空間はテンプレート配信の対象外にする。
    if path == "/admin" || path.starts_with("/admin/") {
        return Err(AppError::NotFound);
    }

    let template = templates::find_published_by_path(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;

    let ctx = build_site_context(&state).await?;
    let html = state
        .templates
        .render_str("page.html", &template.content, Value::from_serialize(&ctx))?;

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

async fn build_site_context(state: &AppState) -> AppResult<SiteContext> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());
    let news = posts::list_published(&state.pool, 5)
        .await?
        .into_iter()
        .map(NewsItem::from)
        .collect::<Vec<_>>();

    Ok(SiteContext {
        blogname,
        blogdescription,
        has_news: !news.is_empty(),
        news,
    })
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
