use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::{AppError, AppResult};
use crate::models::placeholder::{validate_name, Placeholder, PlaceholderInput};
use crate::models::post::{Post, PostInput};
use crate::models::widget::{validate_image_float, validate_image_link_url, validate_image_margin};
use crate::repos::{media, placeholders, postmeta, posts, widget_types};
use crate::state::AppState;
use crate::widgets::{self};

#[derive(Debug, Deserialize)]
struct PlaceholderForm {
    name: String,
    widget_type_id: String,
}

#[derive(Debug, Deserialize, Clone)]
struct EntryForm {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    excerpt: String,
    #[serde(default)]
    post_status: String,
    #[serde(default)]
    post_name: String,
    #[serde(default)]
    media_id: String,
    #[serde(default)]
    float: String,
    #[serde(default)]
    margin: String,
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
    widget_type_label: String,
    template_example: String,
    template_help: String,
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

#[derive(Debug, Clone)]
struct ImageEntryListItem {
    id: i64,
    alt: String,
    thumbnail_url: String,
    has_thumbnail: bool,
    layout_summary: String,
    status_label: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct MediaFormItem {
    id: i64,
    title: String,
    public_url: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/index_image.html")]
struct ImageEntryIndexTemplate {
    placeholder_id: i64,
    placeholder_name: String,
    type_label: String,
    entries: Vec<ImageEntryListItem>,
    has_entries: bool,
    settings_url: String,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/form_image.html")]
struct ImageEntryFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    placeholder_name: String,
    back_url: String,
    title: String,
    content: String,
    margin: String,
    is_draft: bool,
    is_publish: bool,
    is_float_none: bool,
    is_float_left: bool,
    is_float_right: bool,
    media_items: Vec<MediaFormItem>,
    has_media: bool,
    error_message: String,
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
    let html = build_placeholder_form(
        &state,
        "プレースホルダーを追加",
        "/admin/posts/placeholders/new",
        "追加する",
        "",
        None,
        false,
        "",
        "",
        "",
    )
    .await?
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
            let widget_type_id = form.widget_type_id.trim().parse::<i64>().ok();
            let html = build_placeholder_form(
                &state,
                "プレースホルダーを追加",
                "/admin/posts/placeholders/new",
                "追加する",
                &form.name,
                widget_type_id,
                false,
                "",
                "",
                &message,
            )
            .await?
            .render()?;
            return Ok(Html(html).into_response());
        }
    };

    if let Err(err) = placeholders::insert(&state.pool, &input).await {
        if let AppError::Conflict(message) = err {
            let html = build_placeholder_form(
                &state,
                "プレースホルダーを追加",
                "/admin/posts/placeholders/new",
                "追加する",
                &input.name,
                Some(input.widget_type_id),
                false,
                "",
                "",
                &message,
            )
            .await?
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
    let type_key = placeholder_type_key(&state, &placeholder).await?;
    let type_label = widgets::type_label(&type_key).to_string();

    if type_key == "image" {
        let entries = build_image_entry_list(&state, id).await?;
        let html = ImageEntryIndexTemplate {
            placeholder_id: id,
            placeholder_name: placeholder.name,
            type_label,
            has_entries: !entries.is_empty(),
            entries,
            settings_url: format!("/admin/posts/placeholders/{id}/edit"),
        }
        .render()?;
        return Ok(Html(html));
    }

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
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        let html = image_entry_form_template(&state, &placeholder, None, "", "").await?;
        return Ok(Html(html));
    }

    let html = EntryFormTemplate::new(&placeholder).render()?;
    Ok(Html(html))
}

async fn entry_create(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<EntryForm>,
) -> AppResult<Response> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        return image_entry_save(&state, &placeholder, None, form).await;
    }

    let input = form.into_input(id);
    posts::insert(&state.pool, &input).await?;
    Ok(Redirect::to(&format!("/admin/posts/placeholders/{id}")).into_response())
}

async fn entry_edit(
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;
    let entry = posts::find_in_placeholder(&state.pool, id, entry_id).await?;

    if type_key == "image" {
        let html = image_entry_form_template(&state, &placeholder, Some(&entry), "", "").await?;
        return Ok(Html(html));
    }

    let html = EntryFormTemplate::edit(&placeholder, entry).render()?;
    Ok(Html(html))
}

async fn entry_update(
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
    Form(form): Form<EntryForm>,
) -> AppResult<Response> {
    let placeholder = placeholders::find(&state.pool, id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        return image_entry_save(&state, &placeholder, Some(entry_id), form).await;
    }

    let input = form.into_input(id);
    posts::update(&state.pool, entry_id, &input).await?;
    Ok(Redirect::to(&format!("/admin/posts/placeholders/{id}")).into_response())
}

async fn placeholder_type_key(state: &AppState, placeholder: &Placeholder) -> AppResult<String> {
    let widget_type = widget_types::find(&state.pool, placeholder.widget_type_id).await?;
    Ok(widget_type.type_key)
}

async fn build_image_entry_list(state: &AppState, placeholder_id: i64) -> AppResult<Vec<ImageEntryListItem>> {
    let posts = posts::list_all_for_placeholder(&state.pool, placeholder_id).await?;
    let mut items = Vec::with_capacity(posts.len());

    for post in posts {
        let media_id = postmeta::get(&state.pool, post.id, "media_id").await?;
        let float = postmeta::get(&state.pool, post.id, "float")
            .await?
            .unwrap_or_else(|| "none".to_string());
        let margin = postmeta::get(&state.pool, post.id, "margin")
            .await?
            .unwrap_or_default();

        let (thumbnail_url, has_thumbnail) = if let Some(id_str) = media_id.as_deref() {
            if let Ok(media_id) = id_str.parse::<i64>() {
                if let Ok(item) = media::find(&state.pool, media_id).await {
                    (item.public_url(), item.is_image())
                } else {
                    (String::new(), false)
                }
            } else {
                (String::new(), false)
            }
        } else {
            (String::new(), false)
        };

        let float_label = match float.as_str() {
            "left" => "左回り込み",
            "right" => "右回り込み",
            _ => "回り込みなし",
        };
        let layout_summary = if margin.trim().is_empty() {
            float_label.to_string()
        } else {
            format!("{float_label} / margin: {margin}")
        };

        let status_label = match post.post_status.as_str() {
            "publish" => "公開",
            "draft" => "下書き",
            _ => "その他",
        }
        .to_string();

        items.push(ImageEntryListItem {
            id: post.id,
            alt: post.title,
            thumbnail_url,
            has_thumbnail,
            layout_summary,
            status_label,
            updated_at: super::format_updated_at(&post.updated_at),
        });
    }

    Ok(items)
}

async fn image_entry_form_template(
    state: &AppState,
    placeholder: &Placeholder,
    entry: Option<&Post>,
    error_message: &str,
    media_id_override: &str,
) -> AppResult<String> {
    let back_url = format!("/admin/posts/placeholders/{}", placeholder.id);
    let selected_media_id = if !media_id_override.is_empty() {
        media_id_override.to_string()
    } else if let Some(entry) = entry {
        postmeta::get(&state.pool, entry.id, "media_id")
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let float = if let Some(entry) = entry {
        postmeta::get(&state.pool, entry.id, "float")
            .await?
            .unwrap_or_else(|| "none".to_string())
    } else {
        "none".to_string()
    };
    let margin = if let Some(entry) = entry {
        postmeta::get(&state.pool, entry.id, "margin")
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let media_items = media::list_all(&state.pool)
        .await?
        .into_iter()
        .filter(|item| item.is_image())
        .map(|item| MediaFormItem {
            id: item.id,
            title: item.title.clone(),
            public_url: item.public_url(),
            selected: selected_media_id == item.id.to_string(),
        })
        .collect::<Vec<_>>();
    let has_media = !media_items.is_empty();

    let (heading, action, submit_label, title, content, _post_status, is_draft, is_publish) =
        if let Some(entry) = entry {
            let post_status = normalize_status(&entry.post_status);
            let is_publish = post_status == "publish";
            (
                "画像を編集".to_string(),
                format!(
                    "/admin/posts/placeholders/{}/entries/{}/edit",
                    placeholder.id, entry.id
                ),
                "更新する".to_string(),
                entry.title.clone(),
                entry.content.clone(),
                post_status,
                !is_publish,
                is_publish,
            )
        } else {
            (
                "画像を追加".to_string(),
                format!("/admin/posts/placeholders/{}/entries/new", placeholder.id),
                "追加する".to_string(),
                String::new(),
                String::new(),
                "draft".to_string(),
                true,
                false,
            )
        };

    Ok(ImageEntryFormTemplate {
        heading,
        action,
        submit_label,
        placeholder_name: placeholder.name.clone(),
        back_url,
        title,
        content,
        margin,
        is_draft,
        is_publish,
        is_float_none: float == "none",
        is_float_left: float == "left",
        is_float_right: float == "right",
        media_items,
        has_media,
        error_message: error_message.to_string(),
    }
    .render()?)
}

async fn image_entry_save(
    state: &AppState,
    placeholder: &Placeholder,
    entry_id: Option<i64>,
    form: EntryForm,
) -> AppResult<Response> {
    let id = placeholder.id;
    let media_id_on_error = form.media_id.clone();

    let parsed = match form.into_image_input(&state.pool, id).await {
        Ok(input) => input,
        Err(message) => {
            let entry = if let Some(entry_id) = entry_id {
                Some(posts::find_in_placeholder(&state.pool, id, entry_id).await?)
            } else {
                None
            };
            let html = image_entry_form_template(
                state,
                placeholder,
                entry.as_ref(),
                &message,
                &media_id_on_error,
            )
            .await?;
            return Ok(Html(html).into_response());
        }
    };

    let post_id = if let Some(entry_id) = entry_id {
        posts::update(&state.pool, entry_id, &parsed.post).await?;
        entry_id
    } else {
        posts::insert(&state.pool, &parsed.post).await?
    };

    postmeta::set_many(&state.pool, post_id, &parsed.meta).await?;
    Ok(Redirect::to(&format!("/admin/posts/placeholders/{id}")).into_response())
}

struct ImageEntryParsed {
    post: PostInput,
    meta: HashMap<String, String>,
}

async fn placeholder_form_for(
    state: &AppState,
    placeholder: &Placeholder,
    error_message: &str,
) -> AppResult<PlaceholderFormTemplate> {
    build_placeholder_form(
        state,
        "プレースホルダーを編集",
        &format!("/admin/posts/placeholders/{}/edit", placeholder.id),
        "更新する",
        &placeholder.name,
        Some(placeholder.widget_type_id),
        true,
        &format!("/admin/posts/placeholders/{}", placeholder.id),
        &format!("/admin/posts/placeholders/{}/delete", placeholder.id),
        error_message,
    )
    .await
}

async fn build_placeholder_form(
    state: &AppState,
    heading: &str,
    action: &str,
    submit_label: &str,
    name: &str,
    widget_type_id: Option<i64>,
    is_edit: bool,
    entries_url: &str,
    delete_action: &str,
    error_message: &str,
) -> AppResult<PlaceholderFormTemplate> {
    let widget_types = widget_type_options(state, widget_type_id).await?;
    let effective_type_id = widget_type_id.or_else(|| {
        widget_types
            .iter()
            .find(|option| option.selected)
            .map(|option| option.id)
            .or_else(|| widget_types.first().map(|option| option.id))
    });
    let type_key = if let Some(id) = effective_type_id {
        widget_types::find(&state.pool, id).await?.type_key
    } else {
        "news".to_string()
    };
    let (template_example, template_help) = widgets::template_usage(&type_key, name);

    Ok(PlaceholderFormTemplate {
        heading: heading.to_string(),
        action: action.to_string(),
        submit_label: submit_label.to_string(),
        name: name.to_string(),
        widget_types,
        is_edit,
        entries_url: entries_url.to_string(),
        delete_action: delete_action.to_string(),
        error_message: error_message.to_string(),
        widget_type_label: widgets::type_label(&type_key).to_string(),
        template_example,
        template_help,
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
            updated_at: super::format_updated_at(&placeholder.updated_at),
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
            updated_at: super::format_updated_at(&post.updated_at),
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
        let post_name = normalize_slug(&self.post_name, &title, "news");

        PostInput {
            placeholder_id,
            title,
            content: self.content.trim().to_string(),
            excerpt: self.excerpt.trim().to_string(),
            post_status,
            post_name,
        }
    }

    async fn into_image_input(
        self,
        pool: &sqlx::SqlitePool,
        placeholder_id: i64,
    ) -> Result<ImageEntryParsed, String> {
        let media_id = self
            .media_id
            .trim()
            .parse::<i64>()
            .map_err(|_| "画像を選択してください".to_string())?;

        let attachment = media::find(pool, media_id)
            .await
            .map_err(|_| "選択したメディアが見つかりません".to_string())?;
        if !attachment.is_image() {
            return Err("画像ファイルを選択してください".to_string());
        }

        let float = if self.float.trim().is_empty() {
            "none".to_string()
        } else {
            self.float.trim().to_string()
        };
        validate_image_float(&float)?;

        let margin = self.margin.trim().to_string();
        validate_image_margin(&margin)?;

        let link_url = self.content.trim().to_string();
        validate_image_link_url(&link_url)?;

        let alt = self.title.trim().to_string();
        let alt = if alt.is_empty() {
            attachment.title.clone()
        } else {
            alt
        };

        let post_status = normalize_status(&self.post_status);
        let post_name = normalize_slug(&self.post_name, &alt, "image");

        let mut meta = HashMap::new();
        meta.insert("media_id".to_string(), media_id.to_string());
        meta.insert("float".to_string(), float);
        meta.insert("margin".to_string(), margin);

        Ok(ImageEntryParsed {
            post: PostInput {
                placeholder_id,
                title: alt,
                content: link_url,
                excerpt: String::new(),
                post_status,
                post_name,
            },
            meta,
        })
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

fn normalize_slug(raw_slug: &str, title: &str, prefix: &str) -> String {
    let source = if raw_slug.trim().is_empty() {
        title
    } else {
        raw_slug
    };
    let slug = slugify(source);

    if slug.is_empty() {
        format!("{prefix}-{}", Utc::now().format("%Y%m%d%H%M%S"))
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
