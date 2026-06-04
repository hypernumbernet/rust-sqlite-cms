use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::layout::LayoutInput;
use crate::presets;
use crate::repos::{layouts as layouts_repo, media as media_repo};
use crate::services;
use crate::state::AppState;
use crate::theme;

use super::{auth::AuthUser, format_updated_at, layout as admin_layout};

#[derive(Debug, Deserialize)]
struct LayoutForm {
    key: String,
    name: String,
    #[serde(default)]
    is_default: Option<String>,
    #[serde(default)]
    favicon_media_id: String,
    shell_content: String,
}

#[derive(Debug, Clone)]
struct LayoutListItem {
    id: i64,
    key: String,
    name: String,
    is_default: bool,
    page_count: i64,
    updated_at: String,
    can_delete: bool,
}

#[derive(Debug, Clone)]
struct FaviconPreview {
    id: i64,
    title: String,
    public_url: String,
    show_preview: bool,
}

/// ダイアログ内のメディア一覧用。
#[derive(Debug, Clone)]
struct MediaPickerItem {
    id: i64,
    title: String,
    public_url: String,
    show_preview: bool,
}

#[derive(Template)]
#[template(path = "admin/layouts/index.html")]
struct LayoutIndexTemplate {
    layout: admin_layout::AdminLayoutCtx,
    layouts: Vec<LayoutListItem>,
}

#[derive(Template)]
#[template(path = "admin/layouts/form.html")]
struct LayoutFormTemplate {
    layout: admin_layout::AdminLayoutCtx,
    heading: String,
    action: String,
    submit_label: String,
    key: String,
    name: String,
    is_default: bool,
    shell_content: String,
    favicon_media_id_value: String,
    favicon_selected: Option<FaviconPreview>,
    media_picker_items: Vec<MediaPickerItem>,
    has_media: bool,
    is_edit: bool,
    key_readonly: bool,
    delete_action: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/layouts", get(index).post(create))
        .route("/admin/layouts/new", get(new_form))
        .route("/admin/layouts/{id}/edit", get(edit).post(update))
        .route("/admin/layouts/{id}/delete", post(destroy))
}

async fn index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let rows = layouts_repo::list_all(&state.pool).await?;
    let mut layouts = Vec::with_capacity(rows.len());
    for row in rows {
        let page_count = layouts_repo::count_pages(&state.pool, row.id).await?;
        layouts.push(LayoutListItem {
            id: row.id,
            key: row.key,
            name: row.name,
            is_default: row.is_default,
            page_count,
            updated_at: format_updated_at(&row.updated_at),
            can_delete: !row.is_default && page_count == 0,
        });
    }

    let html = LayoutIndexTemplate {
        layout: admin_layout::AdminLayoutCtx::new(&auth),
        layouts,
    }
    .render()?;

    Ok(Html(html))
}

async fn new_form(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = build_layout_form(
        &state.pool,
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを追加",
        "/admin/layouts",
        "作成する",
        String::new(),
        String::new(),
        false,
        presets::DEFAULT_SHELL.to_string(),
        None,
        false,
        false,
        "",
        "",
    )
    .await?
    .render()?;

    Ok(Html(html))
}

async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<LayoutForm>,
) -> AppResult<Response> {
    match form.into_input() {
        Ok(input) => {
            if let Err(err) =
                services::layouts::create_layout(&state.pool, &state.config, &input, &form.shell_content)
                    .await
            {
                return layout_error_response(&state.pool, &auth, &form, false, None, err.to_string())
                    .await;
            }
            Ok(Redirect::to("/admin/layouts").into_response())
        }
        Err(message) => {
            layout_error_response(&state.pool, &auth, &form, false, None, message).await
        }
    }
}

async fn edit(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let row = layouts_repo::find(&state.pool, id).await?;
    let shell_content =
        theme::read_shell(&state.config.paths.work_dir, &row.key).unwrap_or_default();

    let html = build_layout_form(
        &state.pool,
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを編集",
        &format!("/admin/layouts/{id}/edit"),
        "更新する",
        row.key,
        row.name,
        row.is_default,
        shell_content,
        row.favicon_media_id,
        true,
        true,
        "",
        &format!("/admin/layouts/{id}/delete"),
    )
    .await?
    .render()?;

    Ok(Html(html))
}

async fn update(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<LayoutForm>,
) -> AppResult<Response> {
    match form.into_input() {
        Ok(input) => {
            if let Err(err) =
                services::layouts::update_layout(&state.pool, &state.config, id, &input, &form.shell_content)
                    .await
            {
                return layout_error_response(&state.pool, &auth, &form, true, Some(id), err.to_string())
                    .await;
            }
            Ok(Redirect::to("/admin/layouts").into_response())
        }
        Err(message) => layout_error_response(&state.pool, &auth, &form, true, Some(id), message).await,
    }
}

async fn destroy(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    services::layouts::delete_layout(&state.pool, &state.config, id).await?;
    Ok(Redirect::to("/admin/layouts"))
}

async fn build_layout_form(
    pool: &SqlitePool,
    layout: admin_layout::AdminLayoutCtx,
    heading: &str,
    action: &str,
    submit_label: &str,
    key: String,
    name: String,
    is_default: bool,
    shell_content: String,
    favicon_media_id: Option<i64>,
    is_edit: bool,
    key_readonly: bool,
    error_message: &str,
    delete_action: &str,
) -> AppResult<LayoutFormTemplate> {
    let favicon_media_id_value = favicon_media_id.map(|id| id.to_string()).unwrap_or_default();
    let (media_picker_items, has_media) = load_favicon_media_picker_items(pool).await?;
    let favicon_selected =
        resolve_favicon_preview(pool, favicon_media_id, &media_picker_items).await?;

    Ok(LayoutFormTemplate {
        layout,
        heading: heading.to_string(),
        action: action.to_string(),
        submit_label: submit_label.to_string(),
        key,
        name,
        is_default,
        shell_content,
        favicon_media_id_value,
        favicon_selected,
        media_picker_items,
        has_media,
        is_edit,
        key_readonly,
        delete_action: delete_action.to_string(),
        error_message: error_message.to_string(),
    })
}

async fn load_favicon_media_picker_items(
    pool: &SqlitePool,
) -> AppResult<(Vec<MediaPickerItem>, bool)> {
    let items = media_repo::list_all(pool)
        .await?
        .into_iter()
        .filter(|item| item.is_favicon_suitable())
        .map(|item| {
            let title = item.title.clone();
            MediaPickerItem {
                id: item.id,
                title,
                public_url: item.public_url(),
                show_preview: item.is_image(),
            }
        })
        .collect::<Vec<_>>();
    let has_media = !items.is_empty();
    Ok((items, has_media))
}

async fn resolve_favicon_preview(
    pool: &SqlitePool,
    favicon_media_id: Option<i64>,
    picker_items: &[MediaPickerItem],
) -> AppResult<Option<FaviconPreview>> {
    let Some(id) = favicon_media_id else {
        return Ok(None);
    };
    if let Some(item) = picker_items.iter().find(|i| i.id == id) {
        return Ok(Some(FaviconPreview {
            id: item.id,
            title: item.title.clone(),
            public_url: item.public_url.clone(),
            show_preview: item.show_preview,
        }));
    }
    if let Ok(media) = media_repo::find(pool, id).await {
        if media.is_favicon_suitable() {
            let title = media.title.clone();
            return Ok(Some(FaviconPreview {
                id: media.id,
                title,
                public_url: media.public_url(),
                show_preview: media.is_image(),
            }));
        }
    }
    Ok(None)
}

async fn layout_error_response(
    pool: &SqlitePool,
    auth: &AuthUser,
    form: &LayoutForm,
    is_edit: bool,
    id: Option<i64>,
    message: String,
) -> AppResult<Response> {
    let (heading, action, submit_label, delete_action) = if is_edit {
        let id = id.expect("edit requires id");
        (
            "レイアウトを編集",
            format!("/admin/layouts/{id}/edit"),
            "更新する",
            format!("/admin/layouts/{id}/delete"),
        )
    } else {
        (
            "レイアウトを追加",
            "/admin/layouts".to_string(),
            "作成する",
            String::new(),
        )
    };

    let favicon_media_id = parse_favicon_media_id(&form.favicon_media_id).ok().flatten();

    let html = build_layout_form(
        pool,
        admin_layout::AdminLayoutCtx::new(auth),
        heading,
        &action,
        submit_label,
        form.key.clone(),
        form.name.clone(),
        form.is_default.is_some(),
        form.shell_content.clone(),
        favicon_media_id,
        is_edit,
        is_edit,
        &message,
        &delete_action,
    )
    .await?
    .render()?;

    Ok(Html(html).into_response())
}

impl LayoutForm {
    fn into_input(&self) -> Result<LayoutInput, String> {
        let key = self.key.trim().to_string();
        let name = self.name.trim().to_string();
        if key.is_empty() || name.is_empty() {
            return Err("key と名前は必須です".to_string());
        }
        let favicon_media_id = parse_favicon_media_id(&self.favicon_media_id)?;
        Ok(LayoutInput {
            key,
            name,
            is_default: self.is_default.is_some(),
            favicon_media_id,
        })
    }
}

fn parse_favicon_media_id(raw: &str) -> Result<Option<i64>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<i64>()
        .map(Some)
        .map_err(|_| "favicon のメディア ID が不正です".to_string())
}
