use axum::{
    extract::State,
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    #[serde(default)]
    blogname: Option<String>,
    #[serde(default)]
    blogdescription: Option<String>,
    #[serde(default)]
    siteurl: Option<String>,
}

#[derive(serde::Serialize)]
struct SettingsResponse {
    blogname: String,
    blogdescription: String,
    siteurl: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(show))
        .route("/settings", patch(update))
}

async fn show(State(state): State<AppState>) -> ApiResult<Json<SettingsResponse>> {
    let (blogname, blogdescription, siteurl) =
        services::options::get_site_settings(&state.pool(), &state.config).await?;

    Ok(Json(SettingsResponse {
        blogname,
        blogdescription,
        siteurl,
    }))
}

async fn update(
    State(state): State<AppState>,
    Json(payload): Json<UpdateSettingsRequest>,
) -> ApiResult<Json<SettingsResponse>> {
    // 現在の値を取得して部分更新
    let (mut blogname, mut blogdescription, mut siteurl) =
        services::options::get_site_settings(&state.pool(), &state.config).await?;

    if let Some(v) = payload.blogname {
        blogname = v;
    }
    if let Some(v) = payload.blogdescription {
        blogdescription = v;
    }
    if let Some(v) = payload.siteurl {
        siteurl = v;
    }

    // 簡易バリデーション（本格的にはサービス側へ）
    if blogname.trim().is_empty() {
        return Err(ApiError::Validation("サイト名は必須です".into()));
    }
    if blogdescription.trim().is_empty() {
        return Err(ApiError::Validation("サイトの説明は必須です".into()));
    }

    services::options::update_site_settings(&state.pool(), &blogname, &blogdescription, &siteurl).await?;

    Ok(Json(SettingsResponse {
        blogname,
        blogdescription,
        siteurl,
    }))
}

