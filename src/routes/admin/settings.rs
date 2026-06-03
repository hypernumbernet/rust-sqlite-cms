use askama::Template;
use axum::{
    Form, Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::error::AppResult;
use crate::services;
use crate::state::AppState;

use super::{auth::AuthUser, layout};

// 読み取り用に repos を残す（または services 経由に完全移行）
use crate::repos::options;

const MAX_TEXT_LEN: usize = 200;

#[derive(Debug, Default, Deserialize)]
struct SettingsForm {
    blogname: String,
    blogdescription: String,
    siteurl: String,
}

#[derive(Template)]
#[template(path = "admin/settings/form.html")]
struct SettingsFormTemplate {
    layout: layout::AdminLayoutCtx,
    blogname: String,
    blogdescription: String,
    siteurl: String,
    error_message: String,
    success_message: String,
}

#[derive(Debug, Default, Deserialize)]
struct ShowQuery {
    saved: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/settings", get(show).post(save))
}

async fn show(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ShowQuery>,
) -> AppResult<impl IntoResponse> {
    let success_message = if query.saved.as_deref() == Some("1") {
        "設定を保存しました"
    } else {
        ""
    };
    let html = render_form(&auth, &state, "", success_message, None).await?;
    Ok(Html(html))
}

async fn save(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<SettingsForm>,
) -> AppResult<Response> {
    match validate(&form) {
        Ok(()) => {}
        Err(message) => {
            let html = render_form(&auth, &state, &message, "", Some(&form)).await?;
            return Ok(Html(html).into_response());
        }
    }

    let blogname = form.blogname.trim().to_string();
    let blogdescription = form.blogdescription.trim().to_string();
    let siteurl = form.siteurl.trim().to_string();

    services::options::update_site_settings(&state.pool, &blogname, &blogdescription, &siteurl).await?;

    Ok(Redirect::to("/admin/settings?saved=1").into_response())
}

async fn render_form(
    auth: &AuthUser,
    state: &AppState,
    error_message: &str,
    success_message: &str,
    overrides: Option<&SettingsForm>,
) -> AppResult<String> {
    let (blogname, blogdescription, siteurl) = match overrides {
        Some(form) => (
            form.blogname.clone(),
            form.blogdescription.clone(),
            form.siteurl.clone(),
        ),
        None => load_current_values(state).await?,
    };

    Ok(SettingsFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        blogname,
        blogdescription,
        siteurl,
        error_message: error_message.to_string(),
        success_message: success_message.to_string(),
    }
    .render()?)
}

async fn load_current_values(state: &AppState) -> AppResult<(String, String, String)> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());
    let siteurl = options::get(&state.pool, "siteurl").await?.unwrap_or_else(|| {
        format!("http://{}", state.config.server.bind_addr)
    });

    Ok((blogname, blogdescription, siteurl))
}

fn validate(form: &SettingsForm) -> Result<(), String> {
    let blogname = form.blogname.trim();
    let blogdescription = form.blogdescription.trim();
    let siteurl = form.siteurl.trim();

    if blogname.is_empty() {
        return Err("サイト名を入力してください".to_string());
    }
    if blogname.len() > MAX_TEXT_LEN {
        return Err(format!("サイト名は {MAX_TEXT_LEN} 文字以内にしてください"));
    }
    if blogdescription.is_empty() {
        return Err("サイトの説明を入力してください".to_string());
    }
    if blogdescription.len() > MAX_TEXT_LEN {
        return Err(format!("サイトの説明は {MAX_TEXT_LEN} 文字以内にしてください"));
    }
    if siteurl.is_empty() {
        return Err("サイト URL を入力してください".to_string());
    }
    if !(siteurl.starts_with("http://") || siteurl.starts_with("https://")) {
        return Err("サイト URL は http:// または https:// で始めてください".to_string());
    }

    Ok(())
}
