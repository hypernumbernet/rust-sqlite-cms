use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::AppResult;
use crate::models::layout::LayoutInput;
use crate::presets;
use crate::repos::layouts as layouts_repo;
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

async fn new_form(auth: AuthUser) -> AppResult<impl IntoResponse> {
    let html = layout_form_template(
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを追加",
        "/admin/layouts",
        "作成する",
        String::new(),
        String::new(),
        false,
        presets::DEFAULT_SHELL.to_string(),
        false,
        false,
        "",
        "",
    )
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
                return layout_error_response(&auth, &form, false, None, err.to_string());
            }
            Ok(Redirect::to("/admin/layouts").into_response())
        }
        Err(message) => layout_error_response(&auth, &form, false, None, message),
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

    let html = layout_form_template(
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを編集",
        &format!("/admin/layouts/{id}/edit"),
        "更新する",
        row.key,
        row.name,
        row.is_default,
        shell_content,
        true,
        true,
        "",
        &format!("/admin/layouts/{id}/delete"),
    )
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
                return layout_error_response(&auth, &form, true, Some(id), err.to_string());
            }
            Ok(Redirect::to("/admin/layouts").into_response())
        }
        Err(message) => layout_error_response(&auth, &form, true, Some(id), message),
    }
}

async fn destroy(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    services::layouts::delete_layout(&state.pool, &state.config, id).await?;
    Ok(Redirect::to("/admin/layouts"))
}

fn layout_form_template(
    layout: admin_layout::AdminLayoutCtx,
    heading: &str,
    action: &str,
    submit_label: &str,
    key: String,
    name: String,
    is_default: bool,
    shell_content: String,
    is_edit: bool,
    key_readonly: bool,
    error_message: &str,
    delete_action: &str,
) -> LayoutFormTemplate {
    LayoutFormTemplate {
        layout,
        heading: heading.to_string(),
        action: action.to_string(),
        submit_label: submit_label.to_string(),
        key,
        name,
        is_default,
        shell_content,
        is_edit,
        key_readonly,
        delete_action: delete_action.to_string(),
        error_message: error_message.to_string(),
    }
}

fn layout_error_response(
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

    let html = layout_form_template(
        admin_layout::AdminLayoutCtx::new(auth),
        heading,
        &action,
        submit_label,
        form.key.clone(),
        form.name.clone(),
        form.is_default.is_some(),
        form.shell_content.clone(),
        is_edit,
        is_edit,
        &message,
        &delete_action,
    )
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
        Ok(LayoutInput {
            key,
            name,
            is_default: self.is_default.is_some(),
        })
    }
}
