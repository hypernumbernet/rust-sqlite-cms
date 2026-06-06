use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
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
use crate::services;
use crate::state::AppState;
use crate::widgets::{self};

use super::{auth::AuthUser, layout};

#[derive(Debug, Deserialize)]
struct PlaceholderForm {
    name: String,
    widget_type_id: String,
    #[serde(default)]
    config: String,
}

#[derive(Debug, Deserialize, Default)]
struct ManageQuery {
    #[serde(default)]
    tab: String,
    #[serde(default)]
    embed: String,
}

#[derive(Debug, Deserialize, Default)]
struct EmbedQuery {
    #[serde(default)]
    embed: String,
}

#[derive(Debug, Clone)]
struct ManageFormOverride {
    name: String,
    widget_type_id: Option<i64>,
    config: String,
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

#[derive(Debug, Clone)]
struct TrashListItem {
    id: i64,
    title: String,
    post_name: String,
    placeholder_name: String,
    type_label: String,
    trashed_at: String,
}

#[derive(Template)]
#[template(path = "admin/posts/trash/index.html")]
struct TrashIndexTemplate {
    layout: layout::AdminLayoutCtx,
    items: Vec<TrashListItem>,
    has_items: bool,
}

#[derive(Template)]
#[template(path = "admin/posts/placeholders/index.html")]
struct PlaceholderIndexTemplate {
    layout: layout::AdminLayoutCtx,
    placeholders: Vec<PlaceholderListItem>,
    has_placeholders: bool,
}

#[derive(Template)]
#[template(path = "admin/posts/placeholders/form.html")]
struct PlaceholderFormTemplate {
    layout: layout::AdminLayoutCtx,
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
    // インスタンス設定（ウィジェットタイプごとのオプション: limit など）
    config: String,
    // このウィジェットタイプのインスタンス設定スキーマ（JSON）。これに基づいて入力欄を動的生成する。
    config_schema: String,
}

#[derive(Template)]
#[template(path = "admin/posts/placeholders/manage.html")]
struct PlaceholderManageTemplate {
    layout: layout::AdminLayoutCtx,
    placeholder_id: i64,
    placeholder_name: String,
    type_key: String,
    type_label: String,
    type_hint: String,
    is_entries_tab: bool,
    is_settings_tab: bool,
    entries_tab_url: String,
    settings_tab_url: String,
    entries_description: String,
    new_entry_url: String,
    new_entry_label: String,
    has_entries: bool,
    entries: Vec<EntryListItem>,
    image_entries: Vec<ImageEntryListItem>,
    carousel_entries: Vec<CarouselEntryListItem>,
    settings_action: String,
    delete_action: String,
    name: String,
    widget_types: Vec<WidgetTypeOption>,
    config: String,
    config_schema: String,
    template_example: String,
    template_help: String,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/form.html")]
struct EntryFormTemplate {
    layout: layout::AdminLayoutCtx,
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
#[template(path = "admin/posts/entries/form_image.html")]
struct ImageEntryFormTemplate {
    layout: layout::AdminLayoutCtx,
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

#[derive(Debug, Clone)]
struct CarouselEntryListItem {
    id: i64,
    alt: String,
    thumbnail_url: String,
    has_thumbnail: bool,
    status_label: String,
    updated_at: String,
}

#[derive(Template)]
#[template(path = "admin/posts/entries/form_carousel.html")]
struct CarouselEntryFormTemplate {
    layout: layout::AdminLayoutCtx,
    heading: String,
    action: String,
    submit_label: String,
    placeholder_name: String,
    back_url: String,
    title: String,
    content: String,
    is_draft: bool,
    is_publish: bool,
    media_items: Vec<MediaFormItem>,
    has_media: bool,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/posts", get(placeholder_index))
        .route("/admin/posts/trash", get(trash_index))
        .route(
            "/admin/posts/trash/{id}/restore",
            post(trash_restore),
        )
        .route("/admin/posts/trash/{id}/purge", post(trash_purge))
        .route(
            "/admin/posts/placeholders/new",
            get(placeholder_new).post(placeholder_create),
        )
        .route(
            "/admin/posts/placeholders/{id}",
            get(placeholder_manage),
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
        .route(
            "/admin/posts/placeholders/{id}/entries/{entry_id}/delete",
            post(entry_destroy),
        )
}

async fn placeholder_index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let type_map = widget_type_map(&state).await?;
    let placeholders = placeholders::list_all(&state.pool())
        .await?
        .into_iter()
        .map(|p| PlaceholderListItem::from_placeholder(p, &type_map))
        .collect::<Vec<_>>();
    let has_placeholders = !placeholders.is_empty();
    let html = PlaceholderIndexTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        placeholders,
        has_placeholders,
    }
    .render()?;

    Ok(Html(html))
}

async fn trash_index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let items = build_trash_list(&state).await?;
    let has_items = !items.is_empty();
    let html = TrashIndexTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        items,
        has_items,
    }
    .render()?;

    Ok(Html(html))
}

async fn trash_restore(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    match posts::restore(&state.pool(), id).await {
        Ok(()) => Ok(Redirect::to("/admin/posts/trash").into_response()),
        Err(AppError::NotFound) => Ok(Redirect::to("/admin/posts/trash").into_response()),
        Err(err) => Err(err),
    }
}

async fn trash_purge(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    match posts::purge(&state.pool(), id).await {
        Ok(()) => Ok(Redirect::to("/admin/posts/trash").into_response()),
        Err(AppError::NotFound) => Ok(Redirect::to("/admin/posts/trash").into_response()),
        Err(err) => Err(err),
    }
}

async fn build_trash_list(state: &AppState) -> AppResult<Vec<TrashListItem>> {
    let type_map = widget_type_map(state).await?;
    let placeholder_by_id: HashMap<i64, Placeholder> = placeholders::list_all(&state.pool())
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();

    let trashed = posts::list_trashed(&state.pool()).await?;
    Ok(trashed
        .into_iter()
        .filter_map(|post| {
            let placeholder_id = post.placeholder_id?;
            let placeholder = placeholder_by_id.get(&placeholder_id)?;
            let type_key = type_map
                .get(&placeholder.widget_type_id)
                .map(String::as_str)
                .unwrap_or("unknown");
            let post_name = post.post_name.unwrap_or_default();
            Some(TrashListItem {
                id: post.id,
                title: post.title,
                post_name,
                placeholder_name: placeholder.name.clone(),
                type_label: widgets::type_label(type_key).to_string(),
                trashed_at: super::format_updated_at(&post.updated_at),
            })
        })
        .collect())
}

async fn placeholder_new(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = build_placeholder_form(
        &auth,
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
        "{}",
    )
    .await?
    .render()?;

    Ok(Html(html))
}

async fn placeholder_create(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<PlaceholderForm>,
) -> AppResult<Response> {
    let input = match (&form).into_input() {
        Ok(input) => input,
        Err(message) => {
            let widget_type_id = form.widget_type_id.trim().parse::<i64>().ok();
            let html = build_placeholder_form(
        &auth,
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
                &form.config,
            )
            .await?
            .render()?;
            return Ok(Html(html).into_response());
        }
    };

    if let Err(err) = placeholders::insert(&state.pool(), &input).await {
        if let AppError::Conflict(message) = err {
            let html = build_placeholder_form(
        &auth,
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
                &input.config,
            )
            .await?
            .render()?;
            return Ok(Html(html).into_response());
        }
        return Err(err);
    }

    Ok(Redirect::to("/admin/posts").into_response())
}

async fn placeholder_edit(Path(id): Path<i64>) -> Redirect {
    Redirect::to(&format!("/admin/posts/placeholders/{id}?tab=settings"))
}

async fn placeholder_update(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<EmbedQuery>,
    Form(form): Form<PlaceholderForm>,
) -> AppResult<Response> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let input = match (&form).into_input() {
        Ok(input) => input,
        Err(message) => {
            let widget_type_id = form.widget_type_id.trim().parse::<i64>().ok();
            let html = build_manage_template(
        &auth,
        &state,
                &placeholder,
                "settings",
                &message,
                Some(ManageFormOverride {
                    name: form.name,
                    widget_type_id,
                    config: form.config,
                }),
                embed,
            )
            .await?
            .render()?;
            return Ok(Html(html).into_response());
        }
    };

    if let Err(err) = placeholders::update(&state.pool(), id, &input).await {
        if let AppError::Conflict(message) = err {
            let html = build_manage_template(
        &auth,
        &state,
                &placeholder,
                "settings",
                &message,
                None,
                embed,
            )
            .await?
            .render()?;
            return Ok(Html(html).into_response());
        }
        return Err(err);
    }

    Ok(redirect_or_embed_saved(
        embed,
        &manage_settings_tab_url(id, embed),
    ))
}

async fn placeholder_destroy(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    match placeholders::delete(&state.pool(), id).await {
        Ok(()) => Ok(Redirect::to("/admin/posts").into_response()),
        Err(AppError::Conflict(message)) => {
            let placeholder = placeholders::find(&state.pool(), id).await?;
            let html = build_manage_template(
        &auth,
        &state,
                &placeholder,
                "settings",
                &message,
                None,
                false,
            )
            .await?
            .render()?;
            Ok(Html(html).into_response())
        }
        Err(err) => Err(err),
    }
}

async fn placeholder_manage(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<ManageQuery>,
) -> AppResult<impl IntoResponse> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let active_tab = if query.tab == "settings" {
        "settings"
    } else {
        "entries"
    };
    let html = build_manage_template(&auth, &state, &placeholder, active_tab, "", None, embed)
        .await?
        .render()?;
    Ok(Html(html))
}

async fn entry_new(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<EmbedQuery>,
) -> AppResult<impl IntoResponse> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        let html =
            image_entry_form_template(&auth, &state, &placeholder, None, "", "", embed).await?;
        return Ok(Html(html));
    }

    if type_key == "carousel" {
        let html =
            carousel_entry_form_template(&auth, &state, &placeholder, None, "", "", embed).await?;
        return Ok(Html(html));
    }

    let html = EntryFormTemplate::new(layout::AdminLayoutCtx::with_embed(&auth, embed), &placeholder, embed)
        .render()?;
    Ok(Html(html))
}

async fn entry_create(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<EmbedQuery>,
    Form(form): Form<EntryForm>,
) -> AppResult<Response> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        return image_entry_save(&auth, &state, &placeholder, None, form, embed).await;
    }

    if type_key == "carousel" {
        return carousel_entry_save(&auth, &state, &placeholder, None, form, embed).await;
    }

    let input = form.into_input(id);
    posts::insert(&state.pool(), &input).await?;
    Ok(redirect_or_embed_saved(
        embed,
        &url_embed(&format!("/admin/posts/placeholders/{id}"), embed),
    ))
}

async fn entry_edit(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
    Query(query): Query<EmbedQuery>,
) -> AppResult<impl IntoResponse> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;
    let entry = posts::find_in_placeholder(&state.pool(), id, entry_id).await?;

    if type_key == "image" {
        let html = image_entry_form_template(
            &auth,
            &state,
            &placeholder,
            Some(&entry),
            "",
            "",
            embed,
        )
        .await?;
        return Ok(Html(html));
    }

    if type_key == "carousel" {
        let html = carousel_entry_form_template(
            &auth,
            &state,
            &placeholder,
            Some(&entry),
            "",
            "",
            embed,
        )
        .await?;
        return Ok(Html(html));
    }

    let html = EntryFormTemplate::edit(
        layout::AdminLayoutCtx::with_embed(&auth, embed),
        &placeholder,
        entry,
        embed,
    )
    .render()?;
    Ok(Html(html))
}

async fn entry_update(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
    Query(query): Query<EmbedQuery>,
    Form(form): Form<EntryForm>,
) -> AppResult<Response> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let type_key = placeholder_type_key(&state, &placeholder).await?;

    if type_key == "image" {
        return image_entry_save(&auth, &state, &placeholder, Some(entry_id), form, embed).await;
    }

    if type_key == "carousel" {
        return carousel_entry_save(&auth, &state, &placeholder, Some(entry_id), form, embed).await;
    }

    let input = form.into_input(id);
    posts::update(&state.pool(), entry_id, &input).await?;
    Ok(redirect_or_embed_saved(
        embed,
        &url_embed(&format!("/admin/posts/placeholders/{id}"), embed),
    ))
}

async fn entry_destroy(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, entry_id)): Path<(i64, i64)>,
    Query(query): Query<EmbedQuery>,
) -> AppResult<Response> {
    let embed = is_embed_flag(&query.embed);
    let placeholder = placeholders::find(&state.pool(), id).await?;
    let manage_url = url_embed(&format!("/admin/posts/placeholders/{id}"), embed);
    match posts::delete_in_placeholder(&state.pool(), id, entry_id).await {
        Ok(()) => Ok(Redirect::to(&manage_url).into_response()),
        Err(AppError::NotFound) => Ok(Redirect::to(&manage_url).into_response()),
        Err(AppError::Conflict(message)) => {
            let html = build_manage_template(
        &auth,
        &state,
                &placeholder,
                "entries",
                &message,
                None,
                embed,
            )
            .await?
            .render()?;
            Ok(Html(html).into_response())
        }
        Err(err) => Err(err),
    }
}

async fn placeholder_type_key(state: &AppState, placeholder: &Placeholder) -> AppResult<String> {
    let widget_type = widget_types::find(&state.pool(), placeholder.widget_type_id).await?;
    Ok(widget_type.type_key)
}

async fn build_image_entry_list(state: &AppState, placeholder_id: i64) -> AppResult<Vec<ImageEntryListItem>> {
    let posts = posts::list_all_for_placeholder(&state.pool(), placeholder_id).await?;
    let mut items = Vec::with_capacity(posts.len());

    for post in posts {
        let media_id = postmeta::get(&state.pool(), post.id, "media_id").await?;
        let float = postmeta::get(&state.pool(), post.id, "float")
            .await?
            .unwrap_or_else(|| "none".to_string());
        let margin = postmeta::get(&state.pool(), post.id, "margin")
            .await?
            .unwrap_or_default();

        let (thumbnail_url, has_thumbnail) = if let Some(id_str) = media_id.as_deref() {
            if let Ok(media_id) = id_str.parse::<i64>() {
                if let Ok(item) = media::find(&state.pool(), media_id).await {
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

async fn build_carousel_entry_list(state: &AppState, placeholder_id: i64) -> AppResult<Vec<CarouselEntryListItem>> {
    let posts = posts::list_all_for_placeholder(&state.pool(), placeholder_id).await?;
    let mut items = Vec::with_capacity(posts.len());

    for post in posts {
        let media_id = postmeta::get(&state.pool(), post.id, "media_id").await?;

        let (thumbnail_url, has_thumbnail) = if let Some(id_str) = media_id.as_deref() {
            if let Ok(media_id) = id_str.parse::<i64>() {
                if let Ok(item) = media::find(&state.pool(), media_id).await {
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

        let status_label = match post.post_status.as_str() {
            "publish" => "公開",
            "draft" => "下書き",
            _ => "その他",
        }
        .to_string();

        items.push(CarouselEntryListItem {
            id: post.id,
            alt: post.title,
            thumbnail_url,
            has_thumbnail,
            status_label,
            updated_at: super::format_updated_at(&post.updated_at),
        });
    }

    Ok(items)
}

async fn image_entry_form_template(
    auth: &AuthUser,
    state: &AppState,
    placeholder: &Placeholder,
    entry: Option<&Post>,
    error_message: &str,
    media_id_override: &str,
    embed: bool,
) -> AppResult<String> {
    let back_url = url_embed(
        &format!("/admin/posts/placeholders/{}", placeholder.id),
        embed,
    );
    let selected_media_id = if !media_id_override.is_empty() {
        media_id_override.to_string()
    } else if let Some(entry) = entry {
        postmeta::get(&state.pool(), entry.id, "media_id")
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let float = if let Some(entry) = entry {
        postmeta::get(&state.pool(), entry.id, "float")
            .await?
            .unwrap_or_else(|| "none".to_string())
    } else {
        "none".to_string()
    };
    let margin = if let Some(entry) = entry {
        postmeta::get(&state.pool(), entry.id, "margin")
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let media_items = media::list_all(&state.pool())
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
                url_embed(
                    &format!(
                        "/admin/posts/placeholders/{}/entries/{}/edit",
                        placeholder.id, entry.id
                    ),
                    embed,
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
                url_embed(
                    &format!("/admin/posts/placeholders/{}/entries/new", placeholder.id),
                    embed,
                ),
                "追加する".to_string(),
                String::new(),
                String::new(),
                "draft".to_string(),
                true,
                false,
            )
        };

    Ok(ImageEntryFormTemplate {
        layout: layout::AdminLayoutCtx::with_embed(auth, embed),
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
    auth: &AuthUser,
    state: &AppState,
    placeholder: &Placeholder,
    entry_id: Option<i64>,
    form: EntryForm,
    embed: bool,
) -> AppResult<Response> {
    let id = placeholder.id;
    let media_id_on_error = form.media_id.clone();

    let parsed = match form.into_image_input(&state.pool(), id).await {
        Ok(input) => input,
        Err(message) => {
            let entry = if let Some(entry_id) = entry_id {
                Some(posts::find_in_placeholder(&state.pool(), id, entry_id).await?)
            } else {
                None
            };
            let html = image_entry_form_template(
                auth,
                state,
                placeholder,
                entry.as_ref(),
                &message,
                &media_id_on_error,
                embed,
            )
            .await?;
            return Ok(Html(html).into_response());
        }
    };

    let post_id = if let Some(entry_id) = entry_id {
        posts::update(&state.pool(), entry_id, &parsed.post).await?;
        entry_id
    } else {
        posts::insert(&state.pool(), &parsed.post).await?
    };

    postmeta::set_many(&state.pool(), post_id, &parsed.meta).await?;
    Ok(redirect_or_embed_saved(
        embed,
        &url_embed(&format!("/admin/posts/placeholders/{id}"), embed),
    ))
}

async fn carousel_entry_form_template(
    auth: &AuthUser,
    state: &AppState,
    placeholder: &Placeholder,
    entry: Option<&Post>,
    error_message: &str,
    media_id_override: &str,
    embed: bool,
) -> AppResult<String> {
    let back_url = url_embed(
        &format!("/admin/posts/placeholders/{}", placeholder.id),
        embed,
    );
    let selected_media_id = if !media_id_override.is_empty() {
        media_id_override.to_string()
    } else if let Some(entry) = entry {
        postmeta::get(&state.pool(), entry.id, "media_id")
            .await?
            .unwrap_or_default()
    } else {
        String::new()
    };

    let media_items = media::list_all(&state.pool())
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
                "スライドを編集".to_string(),
                url_embed(
                    &format!(
                        "/admin/posts/placeholders/{}/entries/{}/edit",
                        placeholder.id, entry.id
                    ),
                    embed,
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
                "スライドを追加".to_string(),
                url_embed(
                    &format!("/admin/posts/placeholders/{}/entries/new", placeholder.id),
                    embed,
                ),
                "追加する".to_string(),
                String::new(),
                String::new(),
                "draft".to_string(),
                true,
                false,
            )
        };

    Ok(CarouselEntryFormTemplate {
        layout: layout::AdminLayoutCtx::with_embed(auth, embed),
        heading,
        action,
        submit_label,
        placeholder_name: placeholder.name.clone(),
        back_url,
        title,
        content,
        is_draft,
        is_publish,
        media_items,
        has_media,
        error_message: error_message.to_string(),
    }
    .render()?)
}

async fn carousel_entry_save(
    auth: &AuthUser,
    state: &AppState,
    placeholder: &Placeholder,
    entry_id: Option<i64>,
    form: EntryForm,
    embed: bool,
) -> AppResult<Response> {
    let id = placeholder.id;
    let media_id_on_error = form.media_id.clone();

    let parsed = match form.into_carousel_input(&state.pool(), id).await {
        Ok(input) => input,
        Err(message) => {
            let entry = if let Some(entry_id) = entry_id {
                Some(posts::find_in_placeholder(&state.pool(), id, entry_id).await?)
            } else {
                None
            };
            let html = carousel_entry_form_template(
                auth,
                state,
                placeholder,
                entry.as_ref(),
                &message,
                &media_id_on_error,
                embed,
            )
            .await?;
            return Ok(Html(html).into_response());
        }
    };

    let post_id = if let Some(entry_id) = entry_id {
        posts::update(&state.pool(), entry_id, &parsed.post).await?;
        entry_id
    } else {
        posts::insert(&state.pool(), &parsed.post).await?
    };

    postmeta::set_many(&state.pool(), post_id, &parsed.meta).await?;
    Ok(redirect_or_embed_saved(
        embed,
        &url_embed(&format!("/admin/posts/placeholders/{id}"), embed),
    ))
}

struct ImageEntryParsed {
    post: PostInput,
    meta: HashMap<String, String>,
}

struct CarouselEntryParsed {
    post: PostInput,
    meta: HashMap<String, String>,
}

async fn build_manage_template(
    auth: &AuthUser,
    state: &AppState,
    placeholder: &Placeholder,
    active_tab: &str,
    error_message: &str,
    form_override: Option<ManageFormOverride>,
    embed: bool,
) -> AppResult<PlaceholderManageTemplate> {
    let id = placeholder.id;
    let type_key = placeholder_type_key(state, placeholder).await?;
    let type_label = widgets::type_label(&type_key).to_string();

    let (type_hint, entries_description, new_entry_label) = match type_key.as_str() {
        "image" => (
            "公開済み 1 件が表示されます".to_string(),
            "このプレースホルダーに表示する画像を管理します。".to_string(),
            "新規追加".to_string(),
        ),
        "carousel" => (
            "公開済みの全スライドが順番に表示されます".to_string(),
            "カルーセル用画像スライドを管理します。複数追加でき、公開状態のものがスライドショーになります。"
                .to_string(),
            "スライドを追加".to_string(),
        ),
        "contact_form" => (
            "公開フォームからの送信がここに記録されます".to_string(),
            "サイトから受信したお問い合わせ一覧です。".to_string(),
            "新規追加".to_string(),
        ),
        _ => (
            String::new(),
            "このプレースホルダーに表示する投稿を管理します。".to_string(),
            "新規追加".to_string(),
        ),
    };

    let (entries, image_entries, carousel_entries, has_entries) = match type_key.as_str() {
        "image" => {
            let list = build_image_entry_list(state, id).await?;
            let has = !list.is_empty();
            (Vec::new(), list, Vec::new(), has)
        }
        "carousel" => {
            let list = build_carousel_entry_list(state, id).await?;
            let has = !list.is_empty();
            (Vec::new(), Vec::new(), list, has)
        }
        _ => {
            let list = posts::list_all_for_placeholder(&state.pool(), id)
                .await?
                .into_iter()
                .map(EntryListItem::from)
                .collect::<Vec<_>>();
            let has = !list.is_empty();
            (list, Vec::new(), Vec::new(), has)
        }
    };

    let (name, widget_type_id, config) = if let Some(override_form) = form_override {
        (
            override_form.name,
            override_form.widget_type_id,
            override_form.config,
        )
    } else {
        (
            placeholder.name.clone(),
            Some(placeholder.widget_type_id),
            placeholder.config.clone(),
        )
    };

    let widget_types = widget_type_options(state, widget_type_id).await?;
    let effective_type_id = effective_widget_type_id(&widget_types, widget_type_id);
    let config_schema = widget_config_schema(&state.pool(), effective_type_id).await?;

    let is_settings_tab = active_tab == "settings";
    let (template_example, template_help) = widgets::template_usage(&type_key, &name);

    Ok(PlaceholderManageTemplate {
        layout: layout::AdminLayoutCtx::with_embed(auth, embed),
        placeholder_id: id,
        placeholder_name: placeholder.name.clone(),
        type_key,
        type_label,
        type_hint,
        is_entries_tab: !is_settings_tab,
        is_settings_tab,
        entries_tab_url: url_embed(&format!("/admin/posts/placeholders/{id}"), embed),
        settings_tab_url: manage_settings_tab_url(id, embed),
        entries_description,
        new_entry_url: url_embed(
            &format!("/admin/posts/placeholders/{id}/entries/new"),
            embed,
        ),
        new_entry_label,
        has_entries,
        entries,
        image_entries,
        carousel_entries,
        settings_action: url_embed(
            &format!("/admin/posts/placeholders/{id}/edit"),
            embed,
        ),
        delete_action: format!("/admin/posts/placeholders/{id}/delete"),
        name,
        widget_types,
        config,
        config_schema,
        template_example,
        template_help,
        error_message: error_message.to_string(),
    })
}

async fn build_placeholder_form(
    auth: &AuthUser,
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
    config: &str,
) -> AppResult<PlaceholderFormTemplate> {
    let widget_types = widget_type_options(state, widget_type_id).await?;
    let effective_type_id = effective_widget_type_id(&widget_types, widget_type_id);
    let type_key = widget_type_key(&state.pool(), effective_type_id).await?;
    let (template_example, template_help) = widgets::template_usage(&type_key, name);
    let config_schema = widget_config_schema(&state.pool(), effective_type_id).await?;

    Ok(PlaceholderFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
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
        config: config.to_string(),
        config_schema,
    })
}

async fn widget_type_map(state: &AppState) -> AppResult<std::collections::HashMap<i64, String>> {
    Ok(widget_types::list_all(&state.pool())
        .await?
        .into_iter()
        .map(|t| (t.id, t.type_key))
        .collect())
}

fn effective_widget_type_id(
    widget_types: &[WidgetTypeOption],
    widget_type_id: Option<i64>,
) -> Option<i64> {
    widget_type_id.or_else(|| {
        widget_types
            .iter()
            .find(|option| option.selected)
            .map(|option| option.id)
            .or_else(|| widget_types.first().map(|option| option.id))
    })
}

async fn widget_config_schema(pool: &sqlx::SqlitePool, widget_type_id: Option<i64>) -> AppResult<String> {
    if let Some(id) = widget_type_id {
        Ok(widget_types::find(pool, id)
            .await
            .map(|wt| wt.config_schema)
            .unwrap_or_else(|_| "{}".to_string()))
    } else {
        Ok("{}".to_string())
    }
}

async fn widget_type_key(pool: &sqlx::SqlitePool, widget_type_id: Option<i64>) -> AppResult<String> {
    if let Some(id) = widget_type_id {
        Ok(widget_types::find(pool, id)
            .await
            .map(|wt| wt.type_key)
            .unwrap_or_else(|_| "news".to_string()))
    } else {
        Ok("news".to_string())
    }
}

async fn widget_type_options(
    state: &AppState,
    selected_id: Option<i64>,
) -> AppResult<Vec<WidgetTypeOption>> {
    let rows = widget_types::list_all(&state.pool()).await?;
    Ok(rows
        .into_iter()
        .map(|row| WidgetTypeOption {
            id: row.id,
            label: services::widgets::display_label(&row),
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

        Ok(PlaceholderInput {
            name,
            widget_type_id,
            config: self.config.clone(),
        })
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

    async fn into_carousel_input(
        self,
        pool: &sqlx::SqlitePool,
        placeholder_id: i64,
    ) -> Result<CarouselEntryParsed, String> {
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

        let link_url = self.content.trim().to_string();
        validate_image_link_url(&link_url)?;

        let alt = self.title.trim().to_string();
        let alt = if alt.is_empty() {
            attachment.title.clone()
        } else {
            alt
        };

        let post_status = normalize_status(&self.post_status);
        let post_name = normalize_slug(&self.post_name, &alt, "carousel");

        let mut meta = HashMap::new();
        meta.insert("media_id".to_string(), media_id.to_string());
        // carousel では float / margin は不要（コンテナ全体でサイズ指定）

        Ok(CarouselEntryParsed {
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
    fn new(layout: layout::AdminLayoutCtx, placeholder: &Placeholder, embed: bool) -> Self {
        let back_url = url_embed(
            &format!("/admin/posts/placeholders/{}", placeholder.id),
            embed,
        );
        Self {
            layout,
            heading: "投稿を追加".to_string(),
            action: url_embed(
                &format!("/admin/posts/placeholders/{}/entries/new", placeholder.id),
                embed,
            ),
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

    fn edit(
        layout: layout::AdminLayoutCtx,
        placeholder: &Placeholder,
        entry: Post,
        embed: bool,
    ) -> Self {
        let back_url = url_embed(
            &format!("/admin/posts/placeholders/{}", placeholder.id),
            embed,
        );
        let post_status = normalize_status(&entry.post_status);
        let is_publish = post_status == "publish";

        Self {
            layout,
            heading: "投稿を編集".to_string(),
            action: url_embed(
                &format!(
                    "/admin/posts/placeholders/{}/entries/{}/edit",
                    placeholder.id, entry.id
                ),
                embed,
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

fn is_embed_flag(value: &str) -> bool {
    value == "1"
}

fn url_embed(path: &str, embed: bool) -> String {
    if embed {
        format!("{path}?embed=1")
    } else {
        path.to_string()
    }
}

fn manage_settings_tab_url(id: i64, embed: bool) -> String {
    if embed {
        format!("/admin/posts/placeholders/{id}?tab=settings&embed=1")
    } else {
        format!("/admin/posts/placeholders/{id}?tab=settings")
    }
}

fn embed_saved_response() -> Response {
    Html(
        r#"<!DOCTYPE html><html><body><script>window.parent.postMessage({ type: 'cms-embed-saved' }, '*');</script></body></html>"#
            .to_string(),
    )
    .into_response()
}

fn redirect_or_embed_saved(embed: bool, url: &str) -> Response {
    if embed {
        embed_saved_response()
    } else {
        Redirect::to(url).into_response()
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
