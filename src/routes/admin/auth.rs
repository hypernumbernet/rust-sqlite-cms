//! 管理画面の認証（ログイン・ログアウト）。

use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

pub use crate::auth::{require_admin_auth, AuthUser};

use crate::error::{AppError, AppResult, DomainError};
use crate::services::users as users_service;
use crate::state::AppState;

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

    match users_service::authenticate(&state.pool(), &form.login, &form.password).await {
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

/// `cargo run -- --test` 時の admin パスワード。
pub const TEST_MODE_ADMIN_PASSWORD: &str = "testpass";

/// テスト用: 固定パスワードで admin ユーザーを作成またはパスワードを上書きする。
pub async fn ensure_test_admin(pool: &SqlitePool, password: &str) -> AppResult<()> {
    use crate::models::user::{UserInput, PROTECTED_LOGIN, ROLE_ADMINISTRATOR};
    use crate::repos::users as users_repo;

    let password_hash = users_service::hash_password(password).map_err(AppError::from)?;

    if let Some(user) = users_repo::find_by_login(pool, PROTECTED_LOGIN).await? {
        users_repo::update(pool, user.id, &user.display_name, Some(&password_hash)).await?;
    } else {
        let input = UserInput {
            login: PROTECTED_LOGIN.to_string(),
            display_name: "管理者".to_string(),
            password_hash,
            role: ROLE_ADMINISTRATOR.to_string(),
        };
        users_repo::insert(pool, &input).await?;
    }
    Ok(())
}
