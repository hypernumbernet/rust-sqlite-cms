//! API セッション（ログイン・ログアウト・現在ユーザー取得）。

use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult, DomainError};
use crate::services::users as users_service;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct LoginRequest {
    login: String,
    password: String,
}

#[derive(serde::Serialize)]
struct UserResponse {
    id: i64,
    login: String,
    display_name: String,
}

#[derive(serde::Serialize)]
struct SessionResponse {
    user: UserResponse,
}

fn user_response(auth: &AuthUser) -> UserResponse {
    UserResponse {
        id: auth.id,
        login: auth.login.clone(),
        display_name: auth.display_name.clone(),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", post(create).get(show).delete(destroy))
}

async fn create(
    session: Session,
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Json<SessionResponse>> {
    if AuthUser::from_session(&session).await.is_some() {
        return Err(ApiError::BadRequest("既にログインしています".into()));
    }

    match users_service::authenticate(&state.pool, &body.login, &body.password).await {
        Ok(user) => {
            let auth = AuthUser {
                id: user.id,
                login: user.login,
                display_name: user.display_name,
            };
            auth.save_to_session(&session)
                .await
                .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
            Ok(Json(SessionResponse {
                user: user_response(&auth),
            }))
        }
        Err(DomainError::Validation(msg)) => Err(ApiError::Unauthorized(msg)),
        Err(err) => Err(err.into()),
    }
}

async fn show(session: Session) -> ApiResult<Json<SessionResponse>> {
    let auth = AuthUser::from_session(&session)
        .await
        .ok_or_else(|| ApiError::Unauthorized("認証が必要です".into()))?;
    Ok(Json(SessionResponse {
        user: user_response(&auth),
    }))
}

async fn destroy(session: Session) -> ApiResult<StatusCode> {
    session.flush().await.map_err(|e| {
        ApiError::Internal(anyhow::anyhow!("{e}"))
    })?;
    Ok(StatusCode::NO_CONTENT)
}
