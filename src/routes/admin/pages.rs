use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::page::{Page as PageRow, PageInput};
use crate::presets;
use crate::repos::pages;
use crate::routes::url::{is_reserved_path, normalize_url_path};
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
struct PageForm {
    name: String,
    url_path: String,
    content: String,
    #[serde(default)]
    is_published: Option<String>,
}

#[derive(Debug, Clone)]
struct PageListItem {
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
#[template(path = "admin/pages/index.html")]
struct PageIndexTemplate {
    pages: Vec<PageListItem>,
    has_pages: bool,
}

#[derive(Template)]
#[template(path = "admin/pages/gallery.html")]
struct PageGalleryTemplate {
    presets: Vec<PresetCard>,
}

#[derive(Template)]
#[template(path = "admin/pages/form.html")]
struct PageFormTemplate {
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
        .route("/admin/pages", get(index).post(create))
        .route("/admin/pages/new", get(new_gallery))
        .route("/admin/pages/new/{design}", get(new_form))
        .route("/admin/pages/{id}/edit", get(edit).post(update))
        .route("/admin/pages/{id}/delete", post(destroy))
}

async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let pages = pages::list_all(&state.pool)
        .await?
        .into_iter()
        .map(PageListItem::from)
        .collect::<Vec<_>>();
    let html = PageIndexTemplate {
        has_pages: !pages.is_empty(),
        pages,
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
    let html = PageGalleryTemplate { presets }.render()?;

    Ok(Html(html))
}

async fn new_form(Path(design): Path<String>) -> AppResult<impl IntoResponse> {
    let (name, content) = if design == "blank" {
        (String::new(), String::new())
    } else {
        let preset = presets::get(&design).ok_or(AppError::NotFound)?;
        (preset.label.to_string(), preset.html.to_string())
    };

    let html = PageFormTemplate {
        heading: "固定ページを追加".to_string(),
        action: "/admin/pages".to_string(),
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
    Form(form): Form<PageForm>,
) -> AppResult<Redirect> {
    let input = form.into_input()?;
    let (id, file_name) = pages::insert(&state.pool, &input).await?;

    if let Err(err) =
        theme::write_page_source(&state.config.paths.work_dir, &file_name, &input.content)
    {
        let _ = pages::delete(&state.pool, id).await;
        return Err(err.into());
    }

    Ok(Redirect::to("/admin/pages"))
}

async fn edit(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
    let page = pages::find(&state.pool, id).await?;
    let content = match &page.file_name {
        Some(file_name) => theme::read_page_source(&state.config.paths.work_dir, file_name)
            .unwrap_or_default(),
        None => String::new(),
    };

    let html = PageFormTemplate {
        heading: "固定ページを編集".to_string(),
        action: format!("/admin/pages/{id}/edit"),
        submit_label: "更新する".to_string(),
        name: page.name,
        url_path: page.url_path.unwrap_or_default(),
        content,
        is_published: page.is_published,
        is_edit: true,
        delete_action: format!("/admin/pages/{id}/delete"),
    }
    .render()?;

    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PageForm>,
) -> AppResult<Redirect> {
    let page = pages::find(&state.pool, id).await?;
    let file_name = page
        .file_name
        .unwrap_or_else(|| format!("page-{id}.html"));
    let input = form.into_input()?;

    pages::update(&state.pool, id, &input).await?;
    theme::write_page_source(&state.config.paths.work_dir, &file_name, &input.content)?;

    Ok(Redirect::to("/admin/pages"))
}

async fn destroy(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    let page = pages::find(&state.pool, id).await?;
    pages::delete(&state.pool, id).await?;
    if let Some(file_name) = page.file_name {
        theme::remove_page_source(&state.config.paths.work_dir, &file_name)?;
    }

    Ok(Redirect::to("/admin/pages"))
}

impl PageForm {
    fn into_input(self) -> AppResult<PageInput> {
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

        Ok(PageInput {
            name: self.name.trim().to_string(),
            url_path,
            content: self.content,
            is_published,
        })
    }
}

impl From<PageRow> for PageListItem {
    fn from(page: PageRow) -> Self {
        let has_url = page.url_path.is_some();
        let status_label = if page.is_published {
            "公開"
        } else {
            "非公開"
        }
        .to_string();

        Self {
            id: page.id,
            name: if page.name.trim().is_empty() {
                "（無題）".to_string()
            } else {
                page.name
            },
            url_path: page.url_path.unwrap_or_else(|| "（未設定）".to_string()),
            has_url,
            is_published: page.is_published,
            status_label,
            updated_at: page.updated_at,
        }
    }
}
