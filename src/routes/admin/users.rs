use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::AppResult;
use crate::models::user::User;
use crate::repos::users as users_repo;
use crate::services::users::{self, CreateUserParams, UpdateUserParams};
use crate::state::AppState;

use super::{auth::AuthUser, layout};

#[derive(Debug, Deserialize, Default)]
struct UserForm {
    #[serde(default)]
    login: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    password: String,
}

#[derive(Debug, Clone)]
struct UserListItem {
    id: i64,
    login: String,
    display_name: String,
    role_label: String,
    updated_at: String,
    can_delete: bool,
    is_protected: bool,
}

impl UserListItem {
    fn from_user(user: User, updated_at: String) -> Self {
        let is_protected = user.is_protected();
        let role_label = user.role_label().to_string();
        Self {
            id: user.id,
            login: user.login,
            display_name: user.display_name,
            role_label,
            updated_at,
            can_delete: !is_protected,
            is_protected,
        }
    }
}

#[derive(Template)]
#[template(path = "admin/users/index.html")]
struct UserIndexTemplate {
    layout: layout::AdminLayoutCtx,
    users: Vec<UserListItem>,
    has_users: bool,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/users/form.html")]
struct UserFormTemplate {
    layout: layout::AdminLayoutCtx,
    heading: String,
    action: String,
    submit_label: String,
    login: String,
    display_name: String,
    role_label: String,
    is_edit: bool,
    login_readonly: bool,
    password_help: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(index))
        .route("/admin/users/new", get(new_form).post(create))
        .route("/admin/users/{id}/edit", get(edit_form).post(update))
        .route("/admin/users/{id}/delete", post(destroy))
}

async fn index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = render_index(&auth, &state, "").await?;
    Ok(Html(html))
}

async fn new_form(auth: AuthUser) -> AppResult<impl IntoResponse> {
    let html = UserFormTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        heading: "ユーザーを追加".to_string(),
        action: "/admin/users/new".to_string(),
        submit_label: "追加する".to_string(),
        login: String::new(),
        display_name: String::new(),
        role_label: "管理者".to_string(),
        is_edit: false,
        login_readonly: false,
        password_help: "8文字以上で入力してください".to_string(),
        error_message: String::new(),
    }
    .render()?;
    Ok(Html(html))
}

async fn create(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<UserForm>,
) -> AppResult<Response> {
    match users::create(
        &state.pool(),
        CreateUserParams {
            login: &form.login,
            display_name: &form.display_name,
            password: &form.password,
        },
    )
    .await
    {
        Ok(_) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => {
            let message = domain_error_message(&err);
            let html = UserFormTemplate {
                layout: layout::AdminLayoutCtx::new(&auth),
                heading: "ユーザーを追加".to_string(),
                action: "/admin/users/new".to_string(),
                submit_label: "追加する".to_string(),
                login: form.login,
                display_name: form.display_name,
                role_label: "管理者".to_string(),
                is_edit: false,
                login_readonly: false,
                password_help: "8文字以上で入力してください".to_string(),
                error_message: message,
            }
            .render()?;
            Ok(Html(html).into_response())
        }
    }
}

async fn edit_form(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let user = users_repo::find(&state.pool(), id).await?;
    let html = user_form_template(&auth, &user, "")?;
    Ok(Html(html))
}

async fn update(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<UserForm>,
) -> AppResult<Response> {
    let user = users_repo::find(&state.pool(), id).await?;

    match users::update(
        &state.pool(),
        &user,
        UpdateUserParams {
            login: &form.login,
            display_name: &form.display_name,
            password: &form.password,
        },
    )
    .await
    {
        Ok(()) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => {
            let message = domain_error_message(&err);
            let mut display_user = user;
            display_user.display_name = form.display_name;
            let html = user_form_template(&auth, &display_user, &message)?;
            Ok(Html(html).into_response())
        }
    }
}

async fn destroy(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let user = users_repo::find(&state.pool(), id).await?;

    match users::delete(&state.pool(), &user).await {
        Ok(()) => Ok(Redirect::to("/admin/users").into_response()),
        Err(err) => {
            let html = render_index(&auth, &state, &domain_error_message(&err)).await?;
            Ok(Html(html).into_response())
        }
    }
}

async fn render_index(auth: &AuthUser, state: &AppState, error_message: &str) -> AppResult<String> {
    let users = users_repo::list_all(&state.pool())
        .await?
        .into_iter()
        .map(|u| {
            let updated_at = super::format_updated_at(&u.updated_at);
            UserListItem::from_user(u, updated_at)
        })
        .collect::<Vec<_>>();
    let has_users = !users.is_empty();

    Ok(UserIndexTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        users,
        has_users,
        error_message: error_message.to_string(),
    }
    .render()?)
}

fn user_form_template(auth: &AuthUser, user: &User, error_message: &str) -> AppResult<String> {
    Ok(UserFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        heading: "ユーザーを編集".to_string(),
        action: format!("/admin/users/{}/edit", user.id),
        submit_label: "変更を保存".to_string(),
        login: user.login.clone(),
        display_name: user.display_name.clone(),
        role_label: user.role_label().to_string(),
        is_edit: true,
        login_readonly: true,
        password_help: "空欄のままなら変更しません（8文字以上で変更）".to_string(),
        error_message: error_message.to_string(),
    }
    .render()?)
}

fn domain_error_message(err: &crate::error::DomainError) -> String {
    match err {
        crate::error::DomainError::Validation(msg)
        | crate::error::DomainError::Conflict(msg)
        | crate::error::DomainError::BadRequest(msg)
        | crate::error::DomainError::SystemTable(msg) => msg.clone(),
        crate::error::DomainError::NotFound => "ユーザーが見つかりません".to_string(),
        crate::error::DomainError::Internal(e) => e.to_string(),
    }
}
