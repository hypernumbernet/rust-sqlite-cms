use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form,
};
use serde::Deserialize;
use tokio::fs;

use std::path::Path as FsPath;

use crate::error::{AppError, AppResult, DomainError};
use crate::models::page::Page;
use crate::page_render::{self, RenderPageOptions};
use crate::repos::{layouts, media as media_repo, pages, placeholders, widget_types};
use crate::services::contact_form::{self, ContactFormSubmission};
use crate::session;
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
pub struct ContactQueryParams {
    contact_sent: Option<String>,
    contact_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContactFormBody {
    name: String,
    email: String,
    message: String,
    phone: Option<String>,
    token: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/favicon.ico", get(serve_favicon))
        .route("/static/{*path}", get(serve_layout_static))
        .route("/contact/{placeholder_id}", post(submit_contact_form))
}

/// 既定レイアウトの favicon を `/favicon.ico` で配信する。
async fn serve_favicon(State(state): State<AppState>) -> Result<Response, AppError> {
    let layout = layouts::find_default(&state.pool)
        .await
        .map_err(|_| AppError::NotFound)?;
    let media_id = layout
        .favicon_media_id
        .ok_or(AppError::NotFound)?;
    let attachment = media_repo::find(&state.pool, media_id)
        .await
        .map_err(|_| AppError::NotFound)?;
    if !attachment.is_favicon_suitable() {
        return Err(AppError::NotFound);
    }
    let relative = attachment
        .file_path
        .as_deref()
        .ok_or(AppError::NotFound)?;
    let full = FsPath::new(&state.config.paths.uploads_dir).join(relative);
    let bytes = fs::read(&full).await.map_err(|_| AppError::NotFound)?;
    let content_type = attachment
        .mime_type
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or("application/octet-stream");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap())
}

async fn home(
    State(state): State<AppState>,
    Query(query): Query<ContactQueryParams>,
) -> AppResult<impl IntoResponse> {
    let page = pages::find_home(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    if !page.is_published {
        return Err(AppError::NotFound);
    }

    render_public_page(&state, &page, query).await
}

/// `work/layouts/{layout_key}/static/*` を `/static/{layout_key}/*` で配信する。
async fn serve_layout_static(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Response, AppError> {
    let Some(file_path) = theme::resolve_static_path(&state.config.paths.work_dir, &path) else {
        return Err(AppError::NotFound);
    };

    let bytes = fs::read(&file_path).await.map_err(|_| AppError::NotFound)?;
    let content_type = theme::content_type_for_path(&file_path);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .unwrap())
}

/// 既存ルートに一致しなかったパスを、公開済みページとして配信する。
pub async fn serve_fallback(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<ContactQueryParams>,
) -> AppResult<impl IntoResponse> {
    let path = normalize_path(uri.path());

    if is_reserved_public_path(&path) {
        return Err(AppError::NotFound);
    }

    let page = pages::find_published_by_path(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;

    render_public_page(&state, &page, query).await
}

async fn render_public_page(
    state: &AppState,
    page: &Page,
    query: ContactQueryParams,
) -> AppResult<Html<String>> {
    page_render::render_page_with_query(
        state,
        page,
        RenderPageOptions {
            contact_sent: query.contact_sent,
            contact_error: query.contact_error,
        },
    )
    .await
}

async fn submit_contact_form(
    State(state): State<AppState>,
    Path(placeholder_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<ContactFormBody>,
) -> AppResult<impl IntoResponse> {
    let placeholder = placeholders::find(&state.pool, placeholder_id).await?;
    let widget_type = widget_types::find(&state.pool, placeholder.widget_type_id).await?;
    if widget_type.type_key != "contact_form" {
        return Err(AppError::NotFound);
    }

    let secret = session::resolve_session_secret(&state.config);

    let submission = ContactFormSubmission {
        name: form.name,
        email: form.email,
        phone: form.phone,
        message: form.message,
        token: form.token,
    };

    match contact_form::submit(&state.pool, placeholder_id, &secret, &submission).await {
        Ok(()) => {
            let redirect_to = redirect_target(headers.get(header::REFERER), &placeholder.name);
            Ok(Redirect::to(&format!(
                "{redirect_to}?contact_sent={}",
                urlencoding::encode(&placeholder.name)
            ))
            .into_response())
        }
        Err(DomainError::Validation(ref msg)) => {
            tracing::warn!(
                placeholder_id,
                placeholder = %placeholder.name,
                error = %msg,
                "contact form validation failed"
            );
            let redirect_to = redirect_target(headers.get(header::REFERER), &placeholder.name);
            Ok(Redirect::to(&format!(
                "{redirect_to}?contact_error={}",
                urlencoding::encode(&placeholder.name)
            ))
            .into_response())
        }
        Err(DomainError::BadRequest(ref msg)) => {
            tracing::warn!(
                placeholder_id,
                placeholder = %placeholder.name,
                error = %msg,
                "contact form rejected"
            );
            let redirect_to = redirect_target(headers.get(header::REFERER), &placeholder.name);
            Ok(Redirect::to(&format!(
                "{redirect_to}?contact_error={}",
                urlencoding::encode(&placeholder.name)
            ))
            .into_response())
        }
        Err(err) => Err(err.into()),
    }
}

fn redirect_target(referer: Option<&axum::http::HeaderValue>, placeholder_name: &str) -> String {
    if let Some(value) = referer.and_then(|v| v.to_str().ok()) {
        if let Some(path) = extract_local_path(value) {
            return path;
        }
    }
    if placeholder_name == "contact" {
        "/contact".to_string()
    } else {
        "/".to_string()
    }
}

fn extract_local_path(referer: &str) -> Option<String> {
    let path = referer
        .strip_prefix("http://127.0.0.1:3000")
        .or_else(|| referer.strip_prefix("http://localhost:3000"))
        .or_else(|| referer.strip_prefix("http://localhost"))
        .or_else(|| {
            if referer.starts_with('/') {
                Some(referer)
            } else {
                None
            }
        })?;
    let path = path.split('?').next()?.split('#').next()?;
    if path.is_empty() || path.starts_with("/admin") || path.starts_with("/api") {
        return None;
    }
    Some(path.to_string())
}

fn is_reserved_public_path(path: &str) -> bool {
    path == "/favicon.ico"
        || path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
        || path == "/api"
        || path.starts_with("/api/")
}

/// URL を正規化する。ルート以外の末尾スラッシュを取り除く。
fn normalize_path(path: &str) -> String {
    if path.len() > 1 && path.ends_with('/') {
        path.trim_end_matches('/').to_string()
    } else {
        path.to_string()
    }
}
