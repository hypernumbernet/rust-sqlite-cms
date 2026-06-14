use askama::Template;
use axum::{
    Form, Router,
    extract::{Multipart, Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::media::{self, format_file_size};
use crate::models::media::MediaInput;
use crate::repos::media as media_repo;
use crate::services;
use crate::state::AppState;

use super::{auth::AuthUser, breadcrumb, layout};

#[derive(Debug, Clone)]
struct MediaListItem {
    id: i64,
    title: String,
    mime_type: String,
    file_size_label: String,
    updated_at: String,
    public_url: String,
    is_image: bool,
}

#[derive(Debug, Deserialize)]
struct PublicUrlForm {
    public_url: String,
}

#[derive(Template)]
#[template(path = "admin/media/index.html")]
struct MediaIndexTemplate {
    layout: layout::AdminLayoutCtx,
    media_items: Vec<MediaListItem>,
    has_media: bool,
    error_message: String,
    success_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/media", get(index))
        .route("/admin/media/upload", post(upload))
        .route("/admin/media/{id}/public-url", post(update_public_url))
        .route("/admin/media/{id}/delete", post(delete))
}

async fn index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = render_index(&auth, &state, "", "").await?;
    Ok(Html(html))
}

async fn upload(
    auth: AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let uploads_root = &state.config.paths.uploads_dir;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Other(err.into()))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original_name = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "upload".to_string());
        let data = field
            .bytes()
            .await
            .map_err(|err| AppError::Other(err.into()))?;

        return match process_upload(&state, uploads_root, &original_name, &data).await {
            Ok(()) => Ok(Redirect::to("/admin/media").into_response()),
            Err(AppError::Conflict(message)) => {
                let html = render_index(&auth, &state, &message, "").await?;
                Ok(Html(html).into_response())
            }
            Err(err) => Err(err),
        };
    }

    let html = render_index(&auth, &state, "ファイルが選択されていません", "").await?;
    Ok(Html(html).into_response())
}

async fn update_public_url(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PublicUrlForm>,
) -> AppResult<Response> {
    match services::media::update_public_url(&state.pool(), id, &form.public_url).await {
        Ok(()) => {
            let html = render_index(
                &auth,
                &state,
                "",
                "公開 URL を更新しました",
            )
            .await?;
            Ok(Html(html).into_response())
        }
        Err(AppError::Conflict(message)) => {
            let html = render_index(&auth, &state, &message, "").await?;
            Ok(Html(html).into_response())
        }
        Err(err) => Err(err),
    }
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let uploads_root = &state.config.paths.uploads_dir;
    let item = media_repo::find(&state.pool(), id).await?;

    if let Some(file_path) = item.file_path.as_deref() {
        media::delete_file(uploads_root, file_path)?;
    }

    media_repo::delete(&state.pool(), id).await?;
    Ok(Redirect::to("/admin/media").into_response())
}

async fn process_upload(
    state: &AppState,
    uploads_root: &str,
    original_name: &str,
    data: &[u8],
) -> AppResult<()> {
    let (file_path, mime_type) = media::save_upload(uploads_root, original_name, data)?;

    let input = MediaInput {
        title: original_name.to_string(),
        file_path,
        mime_type,
        original_name: original_name.to_string(),
        file_size: data.len() as i64,
    };

    media_repo::insert(&state.pool(), &input).await?;
    Ok(())
}

async fn render_index(
    auth: &AuthUser,
    state: &AppState,
    error_message: &str,
    success_message: &str,
) -> AppResult<String> {
    let media_items = media_repo::list_all(&state.pool())
        .await?
        .into_iter()
        .map(|item| MediaListItem {
            id: item.id,
            title: item.title.clone(),
            mime_type: item.mime_type.clone().unwrap_or_default(),
            file_size_label: format_file_size(item.file_size_bytes()),
            updated_at: super::format_updated_at(&item.updated_at),
            public_url: item.resolved_public_url(),
            is_image: item.is_image(),
        })
        .collect::<Vec<_>>();
    let has_media = !media_items.is_empty();

    Ok(MediaIndexTemplate {
        layout: breadcrumb::with(layout::AdminLayoutCtx::new(auth), breadcrumb::media_index()),
        media_items,
        has_media,
        error_message: error_message.to_string(),
        success_message: success_message.to_string(),
    }
    .render()?)
}