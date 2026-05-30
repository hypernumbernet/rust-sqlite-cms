use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::template::{Template as TemplateRow, TemplateInput};
use crate::presets;
use crate::repos::templates;
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
struct TemplateForm {
    name: String,
    url_path: String,
    content: String,
    #[serde(default)]
    is_published: Option<String>,
}

#[derive(Debug, Clone)]
struct TemplateListItem {
    id: i64,
    name: String,
    url_path: String,
    has_url: bool,
    is_published: bool,
    status_label: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct PresetCard {
    key: String,
    label: String,
    description: String,
}

#[derive(Template)]
#[template(path = "admin/templates/index.html")]
struct TemplateIndexTemplate {
    templates: Vec<TemplateListItem>,
    has_templates: bool,
}

#[derive(Template)]
#[template(path = "admin/templates/gallery.html")]
struct TemplateGalleryTemplate {
    presets: Vec<PresetCard>,
}

#[derive(Template)]
#[template(path = "admin/templates/form.html")]
struct TemplateFormTemplate {
    heading: String,
    action: String,
    submit_label: String,
    name: String,
    url_path: String,
    content: String,
    is_published: bool,
    is_edit: bool,
    delete_action: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/templates", get(index).post(create))
        .route("/admin/templates/new", get(new_gallery))
        .route("/admin/templates/new/{design}", get(new_form))
        .route("/admin/templates/{id}/edit", get(edit).post(update))
        .route("/admin/templates/{id}/delete", post(destroy))
}

async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let templates = templates::list_all(&state.pool)
        .await?
        .into_iter()
        .map(TemplateListItem::from)
        .collect::<Vec<_>>();
    let html = TemplateIndexTemplate {
        has_templates: !templates.is_empty(),
        templates,
    }
    .render()?;

    Ok(Html(html))
}

async fn new_gallery() -> AppResult<impl IntoResponse> {
    let presets = presets::PRESETS
        .iter()
        .map(|preset| PresetCard {
            key: preset.key.to_string(),
            label: preset.label.to_string(),
            description: preset.description.to_string(),
        })
        .collect::<Vec<_>>();
    let html = TemplateGalleryTemplate { presets }.render()?;

    Ok(Html(html))
}

async fn new_form(Path(design): Path<String>) -> AppResult<impl IntoResponse> {
    let (name, content) = if design == "blank" {
        (String::new(), String::new())
    } else {
        let preset = presets::get(&design).ok_or(AppError::NotFound)?;
        (preset.label.to_string(), preset.html.to_string())
    };

    let html = TemplateFormTemplate {
        heading: "テンプレートを追加".to_string(),
        action: "/admin/templates".to_string(),
        submit_label: "作成する".to_string(),
        name,
        url_path: String::new(),
        content,
        is_published: false,
        is_edit: false,
        delete_action: String::new(),
    }
    .render()?;

    Ok(Html(html))
}

async fn create(
    State(state): State<AppState>,
    Form(form): Form<TemplateForm>,
) -> AppResult<Redirect> {
    let input = form.into_input()?;
    let (id, file_name) = templates::insert(&state.pool, &input).await?;

    // 本文ファイルの書き込みに失敗した場合はメタ行を削除して整合を保つ。
    if let Err(err) = theme::write_source(&state.config.paths.work_dir, &file_name, &input.content) {
        let _ = templates::delete(&state.pool, id).await;
        return Err(err.into());
    }

    Ok(Redirect::to("/admin/templates"))
}

async fn edit(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
    let template = templates::find(&state.pool, id).await?;
    let content = match &template.file_name {
        Some(file_name) => {
            theme::read_source(&state.config.paths.work_dir, file_name).unwrap_or_default()
        }
        None => String::new(),
    };

    let html = TemplateFormTemplate {
        heading: "テンプレートを編集".to_string(),
        action: format!("/admin/templates/{id}/edit"),
        submit_label: "更新する".to_string(),
        name: template.name,
        url_path: template.url_path.unwrap_or_default(),
        content,
        is_published: template.is_published,
        is_edit: true,
        delete_action: format!("/admin/templates/{id}/delete"),
    }
    .render()?;

    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<TemplateForm>,
) -> AppResult<Redirect> {
    let template = templates::find(&state.pool, id).await?;
    let file_name = template
        .file_name
        .unwrap_or_else(|| format!("page-{id}.html"));
    let input = form.into_input()?;

    templates::update(&state.pool, id, &input).await?;
    theme::write_source(&state.config.paths.work_dir, &file_name, &input.content)?;

    Ok(Redirect::to("/admin/templates"))
}

async fn destroy(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    let template = templates::find(&state.pool, id).await?;
    templates::delete(&state.pool, id).await?;
    if let Some(file_name) = template.file_name {
        theme::remove_source(&state.config.paths.work_dir, &file_name)?;
    }

    Ok(Redirect::to("/admin/templates"))
}

impl TemplateForm {
    fn into_input(self) -> AppResult<TemplateInput> {
        let url_path = normalize_url_path(&self.url_path);

        if let Some(path) = url_path.as_deref()
            && is_reserved_path(path)
        {
            return Err(AppError::Conflict(format!(
                "URL「{path}」はシステムで予約されているため使用できません"
            )));
        }

        let is_published = self.is_published.is_some();

        if is_published && url_path.is_none() {
            return Err(AppError::Conflict(
                "公開するには URL を指定してください".to_string(),
            ));
        }

        Ok(TemplateInput {
            name: self.name.trim().to_string(),
            url_path,
            content: self.content,
            is_published,
        })
    }
}

impl From<TemplateRow> for TemplateListItem {
    fn from(template: TemplateRow) -> Self {
        let has_url = template.url_path.is_some();
        let status_label = if template.is_published {
            "公開"
        } else {
            "非公開"
        }
        .to_string();

        Self {
            id: template.id,
            name: if template.name.trim().is_empty() {
                "（無題）".to_string()
            } else {
                template.name
            },
            url_path: template.url_path.unwrap_or_else(|| "（未設定）".to_string()),
            has_url,
            is_published: template.is_published,
            status_label,
            updated_at: template.updated_at,
        }
    }
}

/// 入力された URL を正規化する。空なら `None`、先頭スラッシュ付与・末尾スラッシュ除去。
fn normalize_url_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut path = trimmed.to_string();
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    if path.len() > 1 {
        path = path.trim_end_matches('/').to_string();
    }

    Some(path)
}

/// システム（公開トップ・管理画面）が使用する予約済みパスかどうか。
/// 公開トップ `/` は index.html で描画し、`/admin` 配下は管理画面、`/static` 配下は
/// 静的配信に割り当てているため、テンプレートの URL としては使用できない。
fn is_reserved_path(path: &str) -> bool {
    path == "/"
        || path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
}
