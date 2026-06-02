use askama::Template;
use axum::{
    Form, Router,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::widget::{WidgetImportMode, WidgetPackage, WidgetType};
use crate::repos::widget_types as widget_types_repo;
use crate::services;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct WidgetTypeForm {
    #[serde(default)]
    type_key: String,
    #[serde(default)]
    html_template: String,
    #[serde(default)]
    config_schema: String,
}

#[derive(Debug, Deserialize, Default)]
struct WidgetIndexQuery {
    #[serde(default)]
    success_message: String,
    #[serde(default)]
    error_message: String,
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
    success_message: String,
    error_message: String,
}

/// ウィジェット編集画面用（html_template + type_key + インスタンス設定スキーマ編集）
#[derive(Template)]
#[template(path = "admin/widgets/form_edit.html")]
struct WidgetEditFormTemplate {
    heading: String,
    action: String,
    delete_action: String,
    export_url: String,
    type_label: String,
    type_key: String,
    description: String,
    html_template: String,
    config_schema: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/widgets", get(index))
        .route("/admin/widgets/import", post(import_widget))
        .route("/admin/widgets/{type_key}/export", get(export_widget))
        .route("/admin/widgets/{type_key}/edit", get(edit).post(update))
        .route("/admin/widgets/{type_key}/delete", post(destroy))
}

async fn index(
    State(state): State<AppState>,
    Query(query): Query<WidgetIndexQuery>,
) -> AppResult<impl IntoResponse> {
    let widget_types = widget_types_repo::list_all(&state.pool)
        .await?
        .into_iter()
        .map(WidgetTypeListItem::from)
        .collect::<Vec<_>>();
    let html = WidgetIndexTemplate {
        widget_types,
        success_message: query.success_message,
        error_message: query.error_message,
    }
    .render()?;

    Ok(Html(html))
}

async fn export_widget(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
) -> AppResult<Response> {
    let package = services::widgets::export_package(&state.pool, &type_key).await?;
    let body = serde_json::to_string_pretty(&package).map_err(|e| AppError::Other(e.into()))?;
    let filename = format!("widget-{}.json", type_key);
    let disposition = format!("attachment; filename=\"{filename}\"");

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|e| AppError::Other(e.into()))?,
    );

    Ok((headers, body).into_response())
}

async fn import_widget(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut mode = WidgetImportMode::Overwrite;
    let mut package_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Other(err.into()))?
    {
        match field.name() {
            Some("mode") => {
                let text = field
                    .text()
                    .await
                    .map_err(|err| AppError::Other(err.into()))?;
                if text.trim() == "skip" {
                    mode = WidgetImportMode::Skip;
                }
            }
            Some("package") => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|err| AppError::Other(err.into()))?;
                package_bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let Some(bytes) = package_bytes else {
        return Ok(redirect_index_with_error(
            "JSON ファイル（package）を選択してください",
        ));
    };

    let package: WidgetPackage = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return Ok(redirect_index_with_error(&format!(
                "JSON の形式が正しくありません: {e}"
            )));
        }
    };

    match services::widgets::import_package(&state.pool, &package, mode).await {
        Ok((_, message)) => Ok(redirect_index_with_success(&message)),
        Err(err) => {
            let msg = err.to_string();
            Ok(redirect_index_with_error(&msg))
        }
    }
}

fn redirect_index_with_success(message: &str) -> Response {
    let encoded = urlencoding::encode(message);
    Redirect::to(&format!("/admin/widgets?success_message={encoded}")).into_response()
}

fn redirect_index_with_error(message: &str) -> Response {
    let encoded = urlencoding::encode(message);
    Redirect::to(&format!("/admin/widgets?error_message={encoded}")).into_response()
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
    let schema = form.config_schema.clone();

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

    Ok(Redirect::to(&format!("/admin/widgets/{}/edit", submitted_key)).into_response())
}

async fn destroy(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
) -> AppResult<Response> {
    let widget_type = widget_types_repo::find_by_key(&state.pool, &type_key).await?;

    match services::widgets::delete(&state.pool, &type_key).await {
        Ok(()) => Ok(redirect_index_with_success(&format!(
            "ウィジェット「{}」を削除しました",
            services::widgets::display_label(&widget_type)
        ))),
        Err(err) => Ok(redirect_index_with_error(&err.to_string())),
    }
}

fn render_widget_edit_form(widget_type: &WidgetType, error_message: &str) -> AppResult<String> {
    let label = services::widgets::display_label(widget_type);
    let description = services::widgets::display_description(widget_type);

    let template = WidgetEditFormTemplate {
        heading: format!("{} を編集", label),
        action: format!("/admin/widgets/{}/edit", widget_type.type_key),
        delete_action: format!("/admin/widgets/{}/delete", widget_type.type_key),
        export_url: format!("/admin/widgets/{}/export", widget_type.type_key),
        type_label: label,
        type_key: widget_type.type_key.clone(),
        description,
        html_template: widget_type.html_template.clone(),
        config_schema: widget_type.config_schema.clone(),
        error_message: error_message.to_string(),
    };
    template.render().map_err(Into::into)
}

fn config_summary(widget_type: &WidgetType) -> String {
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
            type_label: services::widgets::display_label(&widget_type),
            config_summary: config_summary(&widget_type),
            updated_at: super::format_updated_at(&widget_type.updated_at),
        }
    }
}
