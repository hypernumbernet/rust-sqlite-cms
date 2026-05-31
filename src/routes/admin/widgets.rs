use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::widget::{WidgetType};
use crate::repos::widget_types as widget_types_repo;
use crate::services;
use crate::state::AppState;
use crate::widgets::{self, WIDGET_TYPES};

#[derive(Debug, Deserialize)]
struct WidgetTypeForm {
    #[serde(default)]
    type_key: String,
    #[serde(default)]
    html_template: String,
    #[serde(default)]
    config_schema: String,
}

#[derive(Debug, Clone)]
struct WidgetTypeListItem {
    type_key: String,
    type_label: String,
    config_summary: String,
    updated_at: String,
}

#[derive(Template)]
#[template(path = "admin/widgets/index.html")]
struct WidgetIndexTemplate {
    widget_types: Vec<WidgetTypeListItem>,
}



/// ウィジェット編集画面用（html_template + type_key + インスタンス設定スキーマ編集）
#[derive(Template)]
#[template(path = "admin/widgets/form_edit.html")]
struct WidgetEditFormTemplate {
    heading: String,
    action: String,
    type_key: String,
    description: String,
    html_template: String,
    config_schema: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/widgets", get(index))
        .route("/admin/widgets/{type_key}/edit", get(edit).post(update))
}

async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let widget_types = widget_types_repo::list_all(&state.pool)
        .await?
        .into_iter()
        .map(WidgetTypeListItem::from)
        .collect::<Vec<_>>();
    let html = WidgetIndexTemplate { widget_types }.render()?;

    Ok(Html(html))
}

async fn edit(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
) -> AppResult<impl IntoResponse> {
    let widget_type = widget_types_repo::find_by_key(&state.pool, &type_key).await?;
    let html = render_widget_edit_form(&widget_type, "")?;

    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(old_type_key): Path<String>,
    Form(form): Form<WidgetTypeForm>,
) -> AppResult<Response> {
    let widget_type = widget_types_repo::find_by_key(&state.pool, &old_type_key).await?;

    let submitted_key = if form.type_key.trim().is_empty() {
        old_type_key.clone()
    } else {
        form.type_key.trim().to_string()
    };

    let html = form.html_template.clone();
    let schema = form.config_schema.clone();  // 新規追加: インスタンス設定スキーマ

    // type_key / html_template / config_schema を更新
    if let Err(message) = services::widgets::update_with_schema(
        &state.pool,
        &old_type_key,
        &submitted_key,
        &html,
        &widget_type.config,
        &schema,
    )
    .await
    {
        let html_page = render_widget_edit_form(&widget_type, &message.to_string())?;
        return Ok(Html(html_page).into_response());
    }

    // 更新後のキーでリダイレクト（キーが変わった場合に備える）
    Ok(Redirect::to(&format!("/admin/widgets/{}/edit", submitted_key)).into_response())
}

fn render_widget_edit_form(
    widget_type: &WidgetType,
    error_message: &str,
) -> AppResult<String> {
    let def = WIDGET_TYPES
        .iter()
        .find(|def| def.key == widget_type.type_key)
        .ok_or(AppError::NotFound)?;

    let template = WidgetEditFormTemplate {
        heading: format!("{} を編集", def.label),
        action: format!("/admin/widgets/{}/edit", widget_type.type_key),
        type_key: widget_type.type_key.clone(),
        description: def.description.to_string(),
        html_template: widget_type.html_template.clone(),
        config_schema: widget_type.config_schema.clone(),
        error_message: error_message.to_string(),
    };
    template.render().map_err(Into::into)
}








fn config_summary(widget_type: &WidgetType) -> String {
    // 新設計では html_template の有無を主に表示
    if widget_type.html_template.trim().is_empty() {
        "HTMLテンプレート未設定".to_string()
    } else {
        let len = widget_type.html_template.len();
        format!("HTMLテンプレート ({}文字)", len)
    }
}



impl From<WidgetType> for WidgetTypeListItem {
    fn from(widget_type: WidgetType) -> Self {
        Self {
            type_key: widget_type.type_key.clone(),
            type_label: widgets::type_label(&widget_type.type_key).to_string(),
            config_summary: config_summary(&widget_type),
            updated_at: super::format_updated_at(&widget_type.updated_at),
        }
    }
}


