//! 管理画面・REST API で共有するセッション認証。

use axum::{
    extract::{FromRequestParts, Request},
    http::request::Parts,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::error::{AppError, AppResult, ApiError};
use crate::state::AppState;

pub const SESSION_USER_ID: &str = "user_id";
pub const SESSION_LOGIN: &str = "login";
pub const SESSION_DISPLAY_NAME: &str = "display_name";

/// ログイン済みユーザー（管理画面・API 共通）。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub login: String,
    pub display_name: String,
}

impl AuthUser {
    pub async fn from_session(session: &Session) -> Option<Self> {
        let user_id: i64 = session.get(SESSION_USER_ID).await.ok()??;
        let login: String = session.get(SESSION_LOGIN).await.ok()??;
        let display_name: String = session.get(SESSION_DISPLAY_NAME).await.ok()??;
        Some(Self {
            id: user_id,
            login,
            display_name,
        })
    }

    pub async fn save_to_session(&self, session: &Session) -> AppResult<()> {
        session
            .insert(SESSION_USER_ID, self.id)
            .await
            .map_err(AppError::from)?;
        session
            .insert(SESSION_LOGIN, self.login.clone())
            .await
            .map_err(AppError::from)?;
        session
            .insert(SESSION_DISPLAY_NAME, self.display_name.clone())
            .await
            .map_err(AppError::from)?;
        Ok(())
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| Redirect::to("/admin/login"))?;
        Self::from_session(&session)
            .await
            .ok_or_else(|| Redirect::to("/admin/login"))
    }
}

/// 管理画面保護ルート用ミドルウェア（未認証 → ログインページへリダイレクト）。
pub async fn require_admin_auth(session: Session, request: Request, next: Next) -> Response {
    if AuthUser::from_session(&session).await.is_none() {
        return Redirect::to("/admin/login").into_response();
    }

    next.run(request).await
}

/// REST API 保護ルート用ミドルウェア（未認証 → 401 JSON）。
pub async fn require_api_auth(session: Session, request: Request, next: Next) -> Response {
    if AuthUser::from_session(&session).await.is_none() {
        return ApiError::Unauthorized("認証が必要です".into()).into_response();
    }

    next.run(request).await
}
