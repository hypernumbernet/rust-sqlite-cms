//! 管理画面の認証（ログイン・ログアウト・ミドルウェア）。

use askama::Template;
use axum::{
    extract::{FromRequestParts, Request, State},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::error::{AppError, AppResult, DomainError};
use crate::services::users as users_service;
use crate::state::AppState;

pub const SESSION_USER_ID: &str = "user_id";
pub const SESSION_LOGIN: &str = "login";
pub const SESSION_DISPLAY_NAME: &str = "display_name";

/// ログイン済み管理ユーザー（保護ルートの extractor）。
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

pub async fn require_admin_auth(session: Session, request: Request, next: Next) -> Response {
    if AuthUser::from_session(&session).await.is_none() {
        return Redirect::to("/admin/login").into_response();
    }

    next.run(request).await
}

#[derive(Deserialize)]
struct LoginForm {
    login: String,
    password: String,
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate {
    login: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/login", get(login_form).post(login_submit))
        .route("/admin/logout", post(logout))
}

async fn login_form(session: Session) -> AppResult<impl IntoResponse> {
    if AuthUser::from_session(&session).await.is_some() {
        return Ok(Redirect::to("/admin").into_response());
    }

    let html = LoginTemplate {
        login: String::new(),
        error_message: String::new(),
    }
    .render()?;
    Ok(Html(html).into_response())
}

async fn login_submit(
    session: Session,
    State(state): axum::extract::State<AppState>,
    Form(form): Form<LoginForm>,
) -> AppResult<impl IntoResponse> {
    if AuthUser::from_session(&session).await.is_some() {
        return Ok(Redirect::to("/admin").into_response());
    }

    match users_service::authenticate(&state.pool, &form.login, &form.password).await {
        Ok(user) => {
            let auth = AuthUser {
                id: user.id,
                login: user.login,
                display_name: user.display_name,
            };
            auth.save_to_session(&session).await?;
            Ok(Redirect::to("/admin").into_response())
        }
        Err(DomainError::Validation(msg)) => {
            let html = LoginTemplate {
                login: form.login,
                error_message: msg,
            }
            .render()?;
            Ok((StatusCode::UNAUTHORIZED, Html(html)).into_response())
        }
        Err(err) => Err(err.into()),
    }
}

async fn logout(session: Session) -> AppResult<impl IntoResponse> {
    session.flush().await.map_err(AppError::from)?;
    Ok(Redirect::to("/admin/login"))
}

/// テスト用: 固定パスワードで admin ユーザーを投入する。
pub async fn ensure_test_admin(pool: &SqlitePool, password: &str) -> AppResult<()> {
    use crate::models::user::{UserInput, PROTECTED_LOGIN, ROLE_ADMINISTRATOR};
    use crate::repos::users as users_repo;

    if users_repo::find_by_login(pool, PROTECTED_LOGIN)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let password_hash = users_service::hash_password(password).map_err(AppError::from)?;
    let input = UserInput {
        login: PROTECTED_LOGIN.to_string(),
        display_name: "管理者".to_string(),
        password_hash,
        role: ROLE_ADMINISTRATOR.to_string(),
    };
    users_repo::insert(pool, &input).await?;
    Ok(())
}
