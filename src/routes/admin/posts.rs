use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::placeholder::{validate_name, Placeholder, PlaceholderInput};
use crate::models::post::{Post, PostInput};
use crate::repos::{placeholders, posts, widget_types};
use crate::state::AppState;
use crate::widgets::{self};

#[derive(Debug, Deserialize)]
struct PlaceholderForm {
    name: String,
    widget_type_id: String,
}

#[derive(Debug, Deserialize)]
struct EntryForm {
    title: String,
    content: String,
    excerpt: String,
    post_status: String,
    post_name: String,
}

#[derive(Debug, Clone)]
struct PlaceholderListItem {
    id: i64,
    name: String,
    type_label: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct WidgetTypeOption {
    id: i64,
    label: String,
    selected: bool,
}

#[derive(Debug, Clone)]
struct EntryListItem {
    id: i64,
    title: String,
    status_label: String,
    post_name: String,
    display_date: String,
    updated_at: String,
}

#[derive(Template)]
#[template(path = "admin/posts/placeholders/index.html")]
struct PlaceholderIndexTemplate {
    placeholders: Vec<PlaceholderListItem>,
    has_placeholders: bool,
}

#[derive(Template)]
#[template(path = "admin/posts/placeholders/form.html")]
struct PlaceholderFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    name: String,
    widget_types: Vec<WidgetTypeOption>,
    is_edit: bool,
    entries_url: String,
    delete_action: String,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/index.html")]
struct EntryIndexTemplate {
    placeholder_id: i64,
    placeholder_name: String,
    type_label: String,
    entries: Vec<EntryListItem>,
    has_entries: bool,
    settings_url: String,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/form.html")]
struct EntryFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    placeholder_name: String,
    back_url: String,
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
        .route("/admin/posts", get(placeholder_index))
        .route(
            "/admin/posts/placeholders/new",
            get(placeholder_new).post(placeholder_create),
        )
        .route(
            "/admin/posts/placeholders/{id}",
            get(entry_index),
        )
        .route(
            "/admin/posts/placeholders/{id}/edit",
            get(placeholder_edit).post(placeholder_update),
        )
        .route(
            "/admin/posts/placeholders/{id}/delete",
            post(placeholder_destroy),
        )
        .route(
            "/admin/posts/placeholders/{id}/entries/new",
            get(entry_new).post(entry_create),
        )
        .route(
            "/admin/posts/placeholders/{id}/entries/{entry_id}/edit",
            get(entry_edit).post(entry_update),
        )
}

async fn placeholder_index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let type_map = widget_type_map(&state).await?;
    let placeholders = placeholders::list_all(&state.pool)
        .await?
        .into_iter()
        .map(|p| PlaceholderListItem::from_placeholder(p, &type_map))
        .collect::<Vec<_>>();
    let html = PlaceholderIndexTemplate {
        has_placeholders: !placeholders.is_empty(),
        placeholders,
    }
    .render()?;

    Ok(Html(html))
}

async fn placeholder_new(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let widget_types = widget_type_options(&state, None).await?;
    let html = PlaceholderFormTemplate {
        heading: "プレースホルダーを追加".to_string(),
        action: "/admin/posts/placeholders/new".to_string(),
        submit_label: "追加する".to_string(),
        name: String::new(),
        widget_types,
        is_edit: false,
        entries_url: String::new(),
        delete_action: String::new(),
        error_message: String::new(),
    }
    .render()?;

    Ok(Html(html))
}

async fn placeholder_create(
    State(state): State<AppState>,
    Form(form): Form<PlaceholderForm>,
) -> AppResult<Response> {
    let input = match (&form).into_input() {
        Ok(input) => input,
        Err(message) => {
            let widget_types = widget_type_options(&state, None).await?;
            let html = PlaceholderFormTemplate {
                heading: "プレースホルダーを追加".to_string(),
                action: "/admin/posts/placeholders/new".to_string(),
                submit_label: "追加する".to_string(),
                name: form.name.clone(),
                widget_types,
                is_edit: false,
                entries_url: String::new(),
                delete_action: String::new(),
                error_message: message,
            }
            .render()?;
            return Ok(Html(html).into_response());
        }
    };

    if let Err(err) = placeholders::insert(&state.pool, &input).await {
        if let AppError::Conflict(message) = err {
            let widget_types = widget_type_options(&state, None).await?;
            let html = PlaceholderFormTemplate {
                heading: "プレースホルダーを追加".to_string(),
                action: "/admin/posts/placeholders/new".to_string(),
                submit_label: "追加する".to_string(),
                name: input.name.clone(),
                widget_types,
                is_edit: false,
                entries_url: String::new(),
                delete_action: String::new(),
                error_message: message,
            }
            .render()?;
            return Ok(Html(html).into_response());
        }
        return Err(err);
    }

    Ok(Redirect::to("/admin/posts").into_response())
}

async fn placeholder_edit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let html = placeholder_form_for(&state, &placeholder, "").await?.render()?;
    Ok(Html(html))
}

async fn placeholder_update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PlaceholderForm>,
) -> AppResult<Response> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let input = match (&form).into_input() {
        Ok(input) => input,
        Err(message) => {
            let html = placeholder_form_for(&state, &placeholder, &message).await?.render()?;
            return Ok(Html(html).into_response());
        }
    };

    if let Err(err) = placeholders::update(&state.pool, id, &input).await {
        if let AppError::Conflict(message) = err {
            let html = placeholder_form_for(&state, &placeholder, &message).await?.render()?;
            return Ok(Html(html).into_response());
        }
        return Err(err);
    }

    Ok(Redirect::to("/admin/posts").into_response())
}

async fn placeholder_destroy(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    match placeholders::delete(&state.pool, id).await {
        Ok(()) => Ok(Redirect::to("/admin/posts").into_response()),
        Err(AppError::Conflict(message)) => {
            let placeholder = placeholders::find(&state.pool, id).await?;
            let html = placeholder_form_for(&state, &placeholder, &message).await?.render()?;
            Ok(Html(html).into_response())
        }
        Err(err) => Err(err),
    }
}

async fn entry_index(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let type_map = widget_type_map(&state).await?;
    let type_label = type_map
        .get(&placeholder.widget_type_id)
        .map(|key| widgets::type_label(key).to_string())
        .unwrap_or_else(|| "不明".to_string());
    let entries = posts::list_all_for_placeholder(&state.pool, id)
        .await?
        .into_iter()
        .map(EntryListItem::from)
        .collect::<Vec<_>>();
    let html = EntryIndexTemplate {
        placeholder_id: id,
        placeholder_name: placeholder.name,
        type_label,
        has_entries: !entries.is_empty(),
        entries,
        settings_url: format!("/admin/posts/placeholders/{id}/edit"),
    }
    .render()?;

    Ok(Html(html))
}

async fn entry_new(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let html = EntryFormTemplate::new(&placeholder).render()?;
    Ok(Html(html))
}

async fn entry_create(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<EntryForm>,
) -> AppResult<Redirect> {
    placeholders::find(&state.pool, id).await?;
    let input = form.into_input(id);
    posts::insert(&state.pool, &input).await?;
    Ok(Redirect::to(&format!("/admin/posts/placeholders/{id}")))
}

async fn entry_edit(
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let entry = posts::find_in_placeholder(&state.pool, id, entry_id).await?;
    let html = EntryFormTemplate::edit(&placeholder, entry).render()?;
    Ok(Html(html))
}

async fn entry_update(
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
    Form(form): Form<EntryForm>,
) -> AppResult<Redirect> {
    placeholders::find(&state.pool, id).await?;
    let input = form.into_input(id);
    posts::update(&state.pool, entry_id, &input).await?;
    Ok(Redirect::to(&format!("/admin/posts/placeholders/{id}")))
}

async fn placeholder_form_for(
    state: &AppState,
    placeholder: &Placeholder,
    error_message: &str,
) -> AppResult<PlaceholderFormTemplate> {
    Ok(PlaceholderFormTemplate {
        heading: "プレースホルダーを編集".to_string(),
        action: format!("/admin/posts/placeholders/{}/edit", placeholder.id),
        submit_label: "更新する".to_string(),
        name: placeholder.name.clone(),
        widget_types: widget_type_options(state, Some(placeholder.widget_type_id)).await?,
        is_edit: true,
        entries_url: format!("/admin/posts/placeholders/{}", placeholder.id),
        delete_action: format!("/admin/posts/placeholders/{}/delete", placeholder.id),
        error_message: error_message.to_string(),
    })
}

async fn widget_type_map(state: &AppState) -> AppResult<std::collections::HashMap<i64, String>> {
    Ok(widget_types::list_all(&state.pool)
        .await?
        .into_iter()
        .map(|t| (t.id, t.type_key))
        .collect())
}

async fn widget_type_options(
    state: &AppState,
    selected_id: Option<i64>,
) -> AppResult<Vec<WidgetTypeOption>> {
    let rows = widget_types::list_all(&state.pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| WidgetTypeOption {
            id: row.id,
            label: widgets::type_label(&row.type_key).to_string(),
            selected: Some(row.id) == selected_id,
        })
        .collect())
}

impl PlaceholderListItem {
    fn from_placeholder(
        placeholder: Placeholder,
        type_map: &std::collections::HashMap<i64, String>,
    ) -> Self {
        let type_key = type_map
            .get(&placeholder.widget_type_id)
            .map(String::as_str)
            .unwrap_or("unknown");
        Self {
            id: placeholder.id,
            name: placeholder.name,
            type_label: widgets::type_label(type_key).to_string(),
            updated_at: placeholder.updated_at,
        }
    }
}

impl From<Post> for EntryListItem {
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

impl PlaceholderForm {
    fn into_input(&self) -> Result<PlaceholderInput, String> {
        let name = self.name.trim().to_string();
        validate_name(&name)?;

        let widget_type_id = self
            .widget_type_id
            .trim()
            .parse::<i64>()
            .map_err(|_| "ウィジェットタイプを選択してください".to_string())?;

        if widget_type_id <= 0 {
            return Err("ウィジェットタイプを選択してください".to_string());
        }

        Ok(PlaceholderInput { name, widget_type_id })
    }
}

impl EntryForm {
    fn into_input(self, placeholder_id: i64) -> PostInput {
        let title = self.title.trim().to_string();
        let post_status = normalize_status(&self.post_status);
        let post_name = normalize_slug(&self.post_name, &title);

        PostInput {
            placeholder_id,
            title,
            content: self.content.trim().to_string(),
            excerpt: self.excerpt.trim().to_string(),
            post_status,
            post_name,
        }
    }
}

impl EntryFormTemplate {
    fn new(placeholder: &Placeholder) -> Self {
        let back_url = format!("/admin/posts/placeholders/{}", placeholder.id);
        Self {
            heading: "投稿を追加".to_string(),
            action: format!("/admin/posts/placeholders/{}/entries/new", placeholder.id),
            submit_label: "追加する".to_string(),
            placeholder_name: placeholder.name.clone(),
            back_url,
            title: String::new(),
            content: String::new(),
            excerpt: String::new(),
            post_status: "draft".to_string(),
            post_name: String::new(),
            is_draft: true,
            is_publish: false,
        }
    }

    fn edit(placeholder: &Placeholder, entry: Post) -> Self {
        let back_url = format!("/admin/posts/placeholders/{}", placeholder.id);
        let post_status = normalize_status(&entry.post_status);
        let is_publish = post_status == "publish";

        Self {
            heading: "投稿を編集".to_string(),
            action: format!(
                "/admin/posts/placeholders/{}/entries/{}/edit",
                placeholder.id, entry.id
            ),
            submit_label: "更新する".to_string(),
            placeholder_name: placeholder.name.clone(),
            back_url,
            title: entry.title,
            content: entry.content,
            excerpt: entry.excerpt,
            post_status,
            post_name: entry.post_name.unwrap_or_default(),
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
