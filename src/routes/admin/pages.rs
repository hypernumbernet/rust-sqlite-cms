use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::models::page::{Page as PageRow, PageInput};
use crate::page_render;
use crate::presets;
use crate::repos::pages; // まだ一部で直接使っている（list/findなど）
use crate::routes::url::{is_reserved_path, normalize_url_path};
use crate::services;
use crate::state::AppState;
use crate::theme;

#[derive(Debug, Deserialize)]
struct PageForm {
    name: String,
    #[serde(default)]
    url_path: String,
    content: String,
    #[serde(default)]
    is_static: Option<String>,
    #[serde(default)]
    is_published: Option<String>,
}

#[derive(Debug, Clone)]
struct PageListItem {
    id: i64,
    name: String,
    url_path: String,
    kind_label: String,
    has_url: bool,
    is_published: bool,
    status_label: String,
    updated_at: String,
    can_delete: bool,
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
}

#[derive(Template)]
#[template(path = "admin/pages/gallery.html")]
struct PageGalleryTemplate {
    presets: Vec<PresetCard>,
}

#[derive(Template)]
#[template(path = "admin/pages/preview_error.html")]
struct PagePreviewErrorTemplate {
    status_code: u16,
    status_label: String,
    summary: String,
    detail: String,
    has_page: bool,
    page_id: String,
    page_name: String,
    file_name: String,
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
    is_static: bool,
    is_published: bool,
    is_edit: bool,
    is_home: bool,
    show_is_static: bool,
    static_help: String,
    delete_action: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/pages", get(index).post(create))
        .route("/admin/pages/new", get(new_gallery))
        .route("/admin/pages/new/{design}", get(new_form))
        .route("/admin/pages/{id}/edit", get(edit).post(update))
        .route("/admin/pages/{id}/preview", get(preview))
        .route("/admin/pages/{id}/delete", post(destroy))
}

async fn index(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let pages = pages::list_all(&state.pool)
        .await?
        .into_iter()
        .map(PageListItem::from)
        .collect::<Vec<_>>();
    let html = PageIndexTemplate { pages }.render()?;

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
    let (name, content, is_static) = if design == "blank" {
        (String::new(), String::new(), false)
    } else {
        let preset = presets::get(&design).ok_or(AppError::NotFound)?;
        let is_static = design == "simple-page";
        (preset.label.to_string(), preset.html.to_string(), is_static)
    };

    let html = page_form_template(
        "ページを追加",
        "/admin/pages",
        "作成する",
        name,
        String::new(),
        content,
        is_static,
        false,
        false,
        false,
        true,
        "",
        "",
    )
    .render()?;

    Ok(Html(html))
}

async fn create(
    State(state): State<AppState>,
    Form(form): Form<PageForm>,
) -> AppResult<Response> {
    let input = match form.into_input(false) {
        Ok(input) => input,
        Err(AppError::Conflict(message)) => {
            let html = conflict_form_response(&form, false, None, false, message)?.render()?;
            return Ok(Html(html).into_response());
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = services::pages::create_page(&state.pool, &state.config, &input).await {
        let app_err: AppError = err.into();
        if matches!(app_err, AppError::Conflict(_)) {
            let html = conflict_form_response(&form, false, None, false, app_err.to_string())?.render()?;
            return Ok(Html(html).into_response());
        }
        return Err(app_err);
    }

    Ok(Redirect::to("/admin/pages").into_response())
}

async fn edit(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
    let page = pages::find(&state.pool, id).await?;
    let content = match &page.file_name {
        Some(file_name) => theme::read_page_content(
            &state.config.paths.work_dir,
            file_name,
            page.is_static,
        )
        .unwrap_or_default(),
        None => String::new(),
    };

    let is_home = page.is_home();
    let html = page_form_template(
        if is_home {
            "トップページを編集"
        } else {
            "ページを編集"
        },
        &format!("/admin/pages/{id}/edit"),
        "更新する",
        page.name,
        page.url_path.unwrap_or_default(),
        content,
        page.is_static,
        page.is_published,
        true,
        is_home,
        true,
        "",
        &format!("/admin/pages/{id}/delete"),
    )
    .render()?;

    Ok(Html(html))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PageForm>,
) -> AppResult<Response> {
    let page = pages::find(&state.pool, id).await?;
    let is_home = page.is_home();

    let input = match form.into_input(is_home) {
        Ok(input) => input,
        Err(AppError::Conflict(message)) => {
            let html = conflict_form_response(&form, true, Some(id), is_home, message)?.render()?;
            return Ok(Html(html).into_response());
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = services::pages::update_page(&state.pool, &state.config, id, &input).await {
        let app_err: AppError = err.into();
        if matches!(app_err, AppError::Conflict(_)) {
            let html = conflict_form_response(&form, true, Some(id), is_home, app_err.to_string())?.render()?;
            return Ok(Html(html).into_response());
        }
        return Err(app_err);
    }

    Ok(Redirect::to("/admin/pages").into_response())
}

async fn preview(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let page = match pages::find(&state.pool, id).await {
        Ok(page) => page,
        Err(err) => return preview_error_response(err, None),
    };

    match page_render::render_page(&state, &page).await {
        Ok(html) => wrap_preview_html(html.0, &page).into_response(),
        Err(err) => preview_error_response(err, Some(&page)),
    }
}

fn wrap_preview_html(mut html: String, page: &PageRow) -> Html<String> {
    let name = if page.name.trim().is_empty() {
        "（無題）"
    } else {
        page.name.as_str()
    };
    let unpublished_note = if page.is_published {
        ""
    } else {
        "（未公開 — 公開サイトには表示されません）"
    };

    let banner = format!(
        r#"<style id="cms-preview-banner-style">
.cms-preview-banner {{
  position: sticky;
  top: 0;
  z-index: 9999;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
  padding: 10px 16px;
  background: #1d2327;
  color: #f0f0f1;
  font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Hiragino Sans", "Noto Sans JP", sans-serif;
  border-bottom: 2px solid #2271b1;
}}
.cms-preview-banner strong {{ color: #fff; }}
.cms-preview-banner a {{ color: #9ec2e6; }}
.cms-preview-banner .note {{ color: #c3c4c7; }}
</style>
<div class="cms-preview-banner" role="status">
  <span><strong>プレビュー</strong> — {name}{unpublished_note}</span>
  <span class="note"><a href="/admin/pages">ページ一覧に戻る</a></span>
</div>"#
    );

    if let Some(pos) = html.to_lowercase().find("<body") {
        if let Some(gt) = html[pos..].find('>') {
            let insert_at = pos + gt + 1;
            html.insert_str(insert_at, banner.as_str());
            return Html(html);
        }
    }

    html.insert_str(0, banner.as_str());
    Html(html)
}

fn preview_error_response(err: AppError, page: Option<&PageRow>) -> Response {
    let (status, status_label) = match &err {
        AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found"),
        AppError::Conflict(_) => (StatusCode::CONFLICT, "Conflict"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
    };

    let summary = err.to_string();
    let detail = format!("{err:?}");

    let (has_page, page_id, page_name, file_name) = if let Some(page) = page {
        (
            true,
            page.id.to_string(),
            if page.name.trim().is_empty() {
                "（無題）".to_string()
            } else {
                page.name.clone()
            },
            page.file_name.clone().unwrap_or_else(|| "—".to_string()),
        )
    } else {
        (false, String::new(), String::new(), String::new())
    };

    let template = PagePreviewErrorTemplate {
        status_code: status.as_u16(),
        status_label: status_label.to_string(),
        summary,
        detail,
        has_page,
        page_id,
        page_name,
        file_name,
    };

    match template.render() {
        Ok(body) => (status, Html(body)).into_response(),
        Err(render_err) => {
            tracing::error!(error = %render_err, "preview error template failed");
            (status, summary_from_app_error(&err)).into_response()
        }
    }
}

fn summary_from_app_error(err: &AppError) -> String {
    format!("{}\n\n{:?}", err, err)
}

async fn destroy(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Redirect> {
    services::pages::delete_page(&state.pool, &state.config, id).await?;
    Ok(Redirect::to("/admin/pages"))
}

fn page_form_template(
    heading: &str,
    action: &str,
    submit_label: &str,
    name: String,
    url_path: String,
    content: String,
    is_static: bool,
    is_published: bool,
    is_edit: bool,
    is_home: bool,
    show_is_static: bool,
    error_message: &str,
    delete_action: &str,
) -> PageFormTemplate {
    PageFormTemplate {
        heading: heading.to_string(),
        action: action.to_string(),
        submit_label: submit_label.to_string(),
        name,
        url_path,
        content,
        is_static,
        is_published,
        is_edit,
        is_home,
        show_is_static,
        static_help: static_help_text(is_static),
        delete_action: delete_action.to_string(),
        error_message: error_message.to_string(),
    }
}

/// バリデーション衝突時にフォームを再描画し、画面上で alert する。
fn conflict_form_response(
    form: &PageForm,
    is_edit: bool,
    id: Option<i64>,
    is_home: bool,
    message: String,
) -> AppResult<PageFormTemplate> {
    let is_static = form.is_static.is_some();
    let (heading, action, submit_label, delete_action) = if is_edit {
        let id = id.expect("edit conflict requires page id");
        let heading = if is_home {
            "トップページを編集"
        } else {
            "ページを編集"
        };
        (
            heading,
            format!("/admin/pages/{id}/edit"),
            "更新する",
            format!("/admin/pages/{id}/delete"),
        )
    } else {
        (
            "ページを追加",
            "/admin/pages".to_string(),
            "作成する",
            String::new(),
        )
    };

    Ok(page_form_template(
        heading,
        &action,
        submit_label,
        form.name.clone(),
        form.url_path.clone(),
        form.content.clone(),
        is_static,
        form.is_published.is_some(),
        is_edit,
        is_home,
        true,
        &message,
        &delete_action,
    ))
}

fn static_help_text(is_static: bool) -> String {
    if is_static {
        "完成した HTML をそのまま保存します。MiniJinja の構文は展開されません。".to_string()
    } else {
        "MiniJinja の構文（{{ blogname }} など）が使えます。サイト変数: blogname / blogdescription。プレースホルダー名は /admin/posts で定義し、テンプレートではその名前（例: news, has_news）を参照できます。".to_string()
    }
}

impl PageForm {
    fn into_input(&self, is_home: bool) -> AppResult<PageInput> {
        let url_path = if is_home {
            None
        } else {
            normalize_url_path(self.url_path.as_str())
        };

        if let Some(path) = url_path.as_deref()
            && is_reserved_path(path)
        {
            return Err(AppError::Conflict(format!(
                "URL「{path}」はシステムで予約されているため使用できません"
            )));
        }

        let is_static = self.is_static.is_some();
        let is_published = self.is_published.is_some();

        if is_published && url_path.is_none() && !is_home {
            return Err(AppError::Conflict(
                "公開するには URL を指定してください".to_string(),
            ));
        }

        Ok(PageInput {
            name: self.name.trim().to_string(),
            url_path,
            content: self.content.clone(),
            is_static,
            is_published,
        })
    }
}

impl From<PageRow> for PageListItem {
    fn from(page: PageRow) -> Self {
        let is_home = page.is_home();
        let has_url = (is_home && page.is_published) || page.url_path.is_some();
        let url_path = if is_home {
            "/".to_string()
        } else {
            page.url_path.unwrap_or_else(|| "（未設定）".to_string())
        };
        let kind_label = if is_home && page.is_static {
            "トップ・固定"
        } else if is_home {
            "トップ"
        } else if page.is_static {
            "固定"
        } else {
            "テンプレート"
        }
        .to_string();
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
            url_path,
            kind_label,
            has_url,
            is_published: page.is_published,
            status_label,
            updated_at: page.updated_at,
            can_delete: !is_home,
        }
    }
}
