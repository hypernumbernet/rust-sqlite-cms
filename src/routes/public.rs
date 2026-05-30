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
use crate::repos::{options, pages, posts, templates};
use crate::state::AppState;
use crate::theme;

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
    let html = state
        .templates
        .render("index.html", Value::from_serialize(&ctx))?;

    Ok(Html(html))
}

/// 既存ルートに一致しなかったパスを、公開済み固定ページまたはテンプレートとして配信する。
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
    {
        return Err(AppError::NotFound);
    }

    if let Some(page) = pages::find_published_by_path(&state.pool, &path).await? {
        let file_name = page.file_name.ok_or(AppError::NotFound)?;
        let html = theme::read_page_source(&state.config.paths.work_dir, &file_name)?;
        return Ok(Html(html));
    }

    let template = templates::find_published_by_path(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;
    let file_name = template.file_name.ok_or(AppError::NotFound)?;

    let ctx = build_site_context(&state).await?;
    let html = state
        .templates
        .render(&file_name, Value::from_serialize(&ctx))?;

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
