use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json;

use crate::error::{AppError, AppResult};
use crate::models::widget::{validate_news_limit, ImageWidgetConfig, NewsWidgetConfig, WidgetType, WidgetTypeInput};
use crate::repos::widget_types as widget_types_repo;
use crate::state::AppState;
use crate::widgets::{self, WIDGET_TYPES};

#[derive(Debug, Deserialize)]
struct WidgetTypeForm {
    limit: String,
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

#[derive(Template)]
#[template(path = "admin/widgets/form.html")]
struct WidgetTypeFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    type_key: String,
    type_label: String,
    description: String,
    limit: String,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/widgets/form_image.html")]
struct ImageWidgetFormTemplate {
    heading: String,
    type_key: String,
    type_label: String,
    description: String,
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
    let html = render_widget_form(&widget_type, "", "")?;

    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(type_key): Path<String>,
    Form(form): Form<WidgetTypeForm>,
) -> AppResult<Response> {
    let widget_type = widget_types_repo::find_by_key(&state.pool, &type_key).await?;

    if widget_type.type_key == "image" {
        return Ok(Redirect::to("/admin/widgets").into_response());
    }

    let input = match form.into_input(&widget_type.type_key) {
        Ok(input) => input,
        Err(message) => {
            let html = render_widget_form(&widget_type, &message, &form.limit)?;
            return Ok(Html(html).into_response());
        }
    };

    widget_types_repo::update_config(&state.pool, &type_key, &input).await?;
    Ok(Redirect::to("/admin/widgets").into_response())
}

fn render_widget_form(
    widget_type: &WidgetType,
    error_message: &str,
    limit_override: &str,
) -> AppResult<String> {
    if widget_type.type_key == "image" {
        return image_widget_form_template(widget_type)?.render().map_err(Into::into);
    }

    widget_type_form_template(widget_type, error_message, limit_override)?.render().map_err(Into::into)
}

fn image_widget_form_template(widget_type: &WidgetType) -> AppResult<ImageWidgetFormTemplate> {
    let def = WIDGET_TYPES
        .iter()
        .find(|def| def.key == widget_type.type_key)
        .ok_or(AppError::NotFound)?;

    Ok(ImageWidgetFormTemplate {
        heading: format!("{} を編集", def.label),
        type_key: widget_type.type_key.clone(),
        type_label: def.label.to_string(),
        description: def.description.to_string(),
    })
}

fn widget_type_form_template(
    widget_type: &WidgetType,
    error_message: &str,
    limit_override: &str,
) -> AppResult<WidgetTypeFormTemplate> {
    let def = WIDGET_TYPES
        .iter()
        .find(|def| def.key == widget_type.type_key)
        .ok_or(AppError::NotFound)?;

    let limit = if limit_override.is_empty() {
        news_limit_from_config(&widget_type.config).to_string()
    } else {
        limit_override.to_string()
    };

    Ok(WidgetTypeFormTemplate {
        heading: format!("{} を編集", def.label),
        action: format!("/admin/widgets/{}/edit", widget_type.type_key),
        submit_label: "更新する".to_string(),
        type_key: widget_type.type_key.clone(),
        type_label: def.label.to_string(),
        description: def.description.to_string(),
        limit,
        error_message: error_message.to_string(),
    })
}

fn news_limit_from_config(config: &str) -> i64 {
    serde_json::from_str::<NewsWidgetConfig>(config)
        .map(|cfg| cfg.limit)
        .unwrap_or(5)
}

fn config_summary(widget_type: &WidgetType) -> String {
    match widget_type.type_key.as_str() {
        "news" => format!("表示件数: {}", news_limit_from_config(&widget_type.config)),
        "image" => "画像 1 枚表示".to_string(),
        other => other.to_string(),
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

impl WidgetTypeForm {
    fn into_input(&self, type_key: &str) -> Result<WidgetTypeInput, String> {
        if !widgets::is_known_type(type_key) {
            return Err("不明なウィジェットタイプです".to_string());
        }

        let config = match type_key {
            "news" => {
                let limit = self
                    .limit
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| "表示件数は整数で指定してください".to_string())?;
                validate_news_limit(limit)?;
                serde_json::to_string(&NewsWidgetConfig { limit })
                    .map_err(|_| "設定の保存に失敗しました".to_string())?
            }
            "image" => serde_json::to_string(&ImageWidgetConfig {})
                .map_err(|_| "設定の保存に失敗しました".to_string())?,
            _ => return Err("不明なウィジェットタイプです".to_string()),
        };

        Ok(WidgetTypeInput { config })
    }
}
