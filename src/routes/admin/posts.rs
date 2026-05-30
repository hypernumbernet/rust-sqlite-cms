use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use chrono::Utc;
use serde::Deserialize;

use crate::error::AppResult;
use crate::models::post::{Post, PostInput};
use crate::repos::posts;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct PostForm {
    title: String,
    content: String,
    excerpt: String,
    post_status: String,
    post_name: String,
}

#[derive(Debug, Clone)]
struct PostListItem {
    id: i64,
    title: String,
    status_label: String,
    post_name: String,
    display_date: String,
    updated_at: String,
}

#[derive(Template)]
#[template(path = "admin/posts/index.html")]
struct PostIndexTemplate {
    posts: Vec<PostListItem>,
    has_posts: bool,
}

#[derive(Template)]
#[template(path = "admin/posts/form.html")]
struct PostFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    title: String,
    content: String,
    excerpt: String,
    post_status: String,
    post_name: String,
    is_draft: bool,
    is_publish: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/posts", get(index))
        .route("/admin/posts/new", get(new).post(create))
        .route("/admin/posts/{id}/edit", get(edit).post(update))
}

async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let posts = posts::list_all(&state.pool)
        .await?
        .into_iter()
        .map(PostListItem::from)
        .collect::<Vec<_>>();
    let html = PostIndexTemplate {
        has_posts: !posts.is_empty(),
        posts,
    }
    .render()?;

    Ok(Html(html))
}

async fn new() -> AppResult<impl IntoResponse> {
    let html = PostFormTemplate::new().render()?;
    Ok(Html(html))
}

async fn create(State(state): State<AppState>, Form(form): Form<PostForm>) -> AppResult<Redirect> {
    let input = form.into_input();
    posts::insert(&state.pool, &input).await?;
    Ok(Redirect::to("/admin/posts"))
}

async fn edit(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
    let post = posts::find(&state.pool, id).await?;
    let html = PostFormTemplate::edit(post).render()?;
    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PostForm>,
) -> AppResult<Redirect> {
    let input = form.into_input();
    posts::update(&state.pool, id, &input).await?;
    Ok(Redirect::to("/admin/posts"))
}

impl From<Post> for PostListItem {
    fn from(post: Post) -> Self {
        let display_date = post
            .published_at
            .clone()
            .unwrap_or_else(|| post.created_at.clone());
        let post_name = post.post_name.unwrap_or_default();
        let status_label = match post.post_status.as_str() {
            "publish" => "公開",
            "draft" => "下書き",
            _ => "その他",
        }
        .to_string();

        Self {
            id: post.id,
            title: post.title,
            status_label,
            post_name,
            display_date,
            updated_at: post.updated_at,
        }
    }
}

impl PostForm {
    fn into_input(self) -> PostInput {
        let title = self.title.trim().to_string();
        let post_status = normalize_status(&self.post_status);
        let post_name = normalize_slug(&self.post_name, &title);

        PostInput {
            title,
            content: self.content.trim().to_string(),
            excerpt: self.excerpt.trim().to_string(),
            post_status,
            post_name,
        }
    }
}

impl PostFormTemplate {
    fn new() -> Self {
        Self {
            heading: "お知らせを追加".to_string(),
            action: "/admin/posts/new".to_string(),
            submit_label: "追加する".to_string(),
            title: String::new(),
            content: String::new(),
            excerpt: String::new(),
            post_status: "draft".to_string(),
            post_name: String::new(),
            is_draft: true,
            is_publish: false,
        }
    }

    fn edit(post: Post) -> Self {
        let post_status = normalize_status(&post.post_status);
        let is_publish = post_status == "publish";

        Self {
            heading: "お知らせを編集".to_string(),
            action: format!("/admin/posts/{}/edit", post.id),
            submit_label: "更新する".to_string(),
            title: post.title,
            content: post.content,
            excerpt: post.excerpt,
            post_status,
            post_name: post.post_name.unwrap_or_default(),
            is_draft: !is_publish,
            is_publish,
        }
    }
}

fn normalize_status(status: &str) -> String {
    match status {
        "publish" => "publish".to_string(),
        _ => "draft".to_string(),
    }
}

fn normalize_slug(raw_slug: &str, title: &str) -> String {
    let source = if raw_slug.trim().is_empty() {
        title
    } else {
        raw_slug
    };
    let slug = slugify(source);

    if slug.is_empty() {
        Utc::now().format("news-%Y%m%d%H%M%S").to_string()
    } else {
        slug
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    slug.trim_end_matches('-').to_string()
}
