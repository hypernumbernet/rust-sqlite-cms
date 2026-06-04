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

use super::{auth::AuthUser, layout};

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
    layout: layout::AdminLayoutCtx,
    pages: Vec<PageListItem>,
}

#[derive(Template)]
#[template(path = "admin/pages/gallery.html")]
struct PageGalleryTemplate {
    layout: layout::AdminLayoutCtx,
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
    layout: layout::AdminLayoutCtx,
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

async fn index(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let pages = pages::list_all(&state.pool)
        .await?
        .into_iter()
        .map(PageListItem::from)
        .collect::<Vec<_>>();
    let html = PageIndexTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        pages,
    }
    .render()?;

    Ok(Html(html))
}

async fn new_gallery(auth: AuthUser) -> AppResult<impl IntoResponse> {
    let presets = presets::PRESETS
        .iter()
        .map(|preset| PresetCard {
            key: preset.key.to_string(),
            label: preset.label.to_string(),
            description: preset.description.to_string(),
        })
        .collect::<Vec<_>>();
    let html = PageGalleryTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        presets,
    }
    .render()?;

    Ok(Html(html))
}

async fn new_form(auth: AuthUser, Path(design): Path<String>) -> AppResult<impl IntoResponse> {
    let (name, content, is_static) = if design == "blank" {
        (String::new(), String::new(), false)
    } else {
        let preset = presets::get(&design).ok_or(AppError::NotFound)?;
        let is_static = design == "simple-page";
        (preset.label.to_string(), preset.html.to_string(), is_static)
    };

    let html = page_form_template(
        layout::AdminLayoutCtx::new(&auth),
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
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<PageForm>,
) -> AppResult<Response> {
    let input = match form.into_input(false) {
        Ok(input) => input,
        Err(AppError::Conflict(message)) => {
            let html = conflict_form_response(&auth, &form, false, None, false, message)?.render()?;
            return Ok(Html(html).into_response());
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = services::pages::create_page(&state.pool, &state.config, &input).await {
        let app_err: AppError = err.into();
        if matches!(app_err, AppError::Conflict(_)) {
            let html =
                conflict_form_response(&auth, &form, false, None, false, app_err.to_string())?.render()?;
            return Ok(Html(html).into_response());
        }
        return Err(app_err);
    }

    Ok(Redirect::to("/admin/pages").into_response())
}

async fn edit(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
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
        layout::AdminLayoutCtx::new(&auth),
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
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<PageForm>,
) -> AppResult<Response> {
    let page = pages::find(&state.pool, id).await?;
    let is_home = page.is_home();

    let input = match form.into_input(is_home) {
        Ok(input) => input,
        Err(AppError::Conflict(message)) => {
            let html =
                conflict_form_response(&auth, &form, true, Some(id), is_home, message)?.render()?;
            return Ok(Html(html).into_response());
        }
        Err(err) => return Err(err),
    };

    if let Err(err) = services::pages::update_page(&state.pool, &state.config, id, &input).await {
        let app_err: AppError = err.into();
        if matches!(app_err, AppError::Conflict(_)) {
            let html = conflict_form_response(
                &auth,
                &form,
                true,
                Some(id),
                is_home,
                app_err.to_string(),
            )?
            .render()?;
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

    match page_render::render_page_preview(&state, &page).await {
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
    let static_note = if page.is_static {
        r#"<span class="cms-preview-static-note"> — 静的HTMLのためウィジェット編集は利用できません</span>"#
    } else {
        ""
    };

    let head_banner = format!(
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
.cms-preview-banner .cms-preview-static-note {{ color: #c3c4c7; font-size: 12px; }}
.cms-preview-banner-actions {{
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
}}
.cms-preview-edit-toggle {{
  padding: 5px 12px;
  font-size: 12px;
  font-weight: 600;
  border: 1px solid #2271b1;
  border-radius: 4px;
  background: #2271b1;
  color: #fff;
  cursor: pointer;
}}
.cms-preview-edit-toggle:hover {{ background: #135e96; }}
.cms-preview-edit-toggle.is-active {{
  background: #d63638;
  border-color: #d63638;
}}
.cms-preview-edit-toggle.is-active:hover {{ background: #b32d2e; }}
.cms-preview-edit-toggle:disabled {{
  opacity: 0.55;
  cursor: not-allowed;
}}
body.cms-preview-edit-mode .cms-widget-target {{
  cursor: pointer;
  position: relative;
}}
body.cms-preview-edit-mode .cms-widget-target:hover {{
  outline: 3px solid #d63638;
  outline-offset: 2px;
}}
body.cms-preview-edit-mode .cms-widget-target:hover::after {{
  content: attr(data-cms-placeholder-name);
  position: absolute;
  top: 0;
  left: 0;
  z-index: 10000;
  padding: 2px 8px;
  background: #d63638;
  color: #fff;
  font: 12px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Hiragino Sans", "Noto Sans JP", sans-serif;
  pointer-events: none;
}}
.cms-preview-modal {{
  position: fixed;
  inset: 0;
  z-index: 100000;
  display: none;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: rgba(0, 0, 0, 0.55);
}}
.cms-preview-modal.is-open {{ display: flex; }}
.cms-preview-modal-panel {{
  position: relative;
  width: min(960px, 96vw);
  height: min(88vh, 900px);
  background: #fff;
  border-radius: 6px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}}
.cms-preview-modal-header {{
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid #dcdcde;
  background: #f6f7f7;
  font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Hiragino Sans", "Noto Sans JP", sans-serif;
}}
.cms-preview-modal-title {{ font-weight: 600; color: #1d2327; }}
.cms-preview-modal-close {{
  padding: 4px 10px;
  font-size: 12px;
  border: 1px solid #dcdcde;
  border-radius: 4px;
  background: #fff;
  cursor: pointer;
}}
.cms-preview-modal-close:hover {{ background: #f0f0f1; }}
.cms-preview-modal iframe {{
  flex: 1;
  width: 100%;
  border: 0;
}}
.cms-preview-toast {{
  position: fixed;
  bottom: 20px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 100001;
  padding: 8px 14px;
  background: #1d2327;
  color: #f0f0f1;
  font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Hiragino Sans", "Noto Sans JP", sans-serif;
  border-radius: 4px;
  display: none;
}}
.cms-preview-toast.is-visible {{ display: block; }}
</style>
<div class="cms-preview-banner" role="status">
  <span><strong>プレビュー</strong> — {name}{unpublished_note}{static_note}</span>
  <span class="cms-preview-banner-actions">
    <button type="button" class="cms-preview-edit-toggle" id="cms-preview-edit-toggle"{edit_toggle_disabled}>編集モード</button>
    <span class="note"><a href="/admin/pages">ページ一覧に戻る</a></span>
  </span>
</div>"#,
        edit_toggle_disabled = if page.is_static { " disabled" } else { "" },
    );

    let footer = format!(
        r#"<div class="cms-preview-modal" id="cms-preview-modal" aria-hidden="true">
  <div class="cms-preview-modal-panel" role="dialog" aria-modal="true" aria-labelledby="cms-preview-modal-title">
    <div class="cms-preview-modal-header">
      <span class="cms-preview-modal-title" id="cms-preview-modal-title">投稿を編集</span>
      <button type="button" class="cms-preview-modal-close" id="cms-preview-modal-close">閉じる</button>
    </div>
    <iframe id="cms-preview-modal-iframe" title="投稿編集"></iframe>
  </div>
</div>
<div class="cms-preview-toast" id="cms-preview-toast" role="status"></div>
<script id="cms-preview-edit-script">
(function() {{
  var STORAGE_KEY = "cms-preview-edit-mode";
  var toggle = document.getElementById("cms-preview-edit-toggle");
  var modal = document.getElementById("cms-preview-modal");
  var iframe = document.getElementById("cms-preview-modal-iframe");
  var modalTitle = document.getElementById("cms-preview-modal-title");
  var modalClose = document.getElementById("cms-preview-modal-close");
  var toast = document.getElementById("cms-preview-toast");

  function countWidgetTargets() {{
    return document.querySelectorAll(".cms-widget-target").length;
  }}

  function setEditMode(on) {{
    document.body.classList.toggle("cms-preview-edit-mode", on);
    if (toggle) {{
      toggle.classList.toggle("is-active", on);
      toggle.textContent = on ? "編集モードを終了" : "編集モード";
    }}
    try {{
      if (on) sessionStorage.setItem(STORAGE_KEY, "1");
      else sessionStorage.removeItem(STORAGE_KEY);
    }} catch (e) {{}}
  }}

  function showToast(message) {{
    if (!toast) return;
    toast.textContent = message;
    toast.classList.add("is-visible");
    setTimeout(function() {{ toast.classList.remove("is-visible"); }}, 3000);
  }}

  function openModal(placeholderId, placeholderName) {{
    if (!modal || !iframe) return;
    modalTitle.textContent = "投稿を編集 — " + placeholderName;
    iframe.src = "/admin/posts/placeholders/" + placeholderId + "?embed=1";
    modal.classList.add("is-open");
    modal.setAttribute("aria-hidden", "false");
  }}

  function closeModal() {{
    if (!modal || !iframe) return;
    modal.classList.remove("is-open");
    modal.setAttribute("aria-hidden", "true");
    iframe.src = "about:blank";
  }}

  if (toggle && !toggle.disabled) {{
    var saved = false;
    try {{ saved = sessionStorage.getItem(STORAGE_KEY) === "1"; }} catch (e) {{}}
    if (saved) setEditMode(true);

    toggle.addEventListener("click", function() {{
      var on = !document.body.classList.contains("cms-preview-edit-mode");
      if (on && countWidgetTargets() === 0) {{
        showToast("編集可能なウィジェットがありません");
        return;
      }}
      setEditMode(on);
    }});
  }}

  document.body.addEventListener("click", function(e) {{
    if (!document.body.classList.contains("cms-preview-edit-mode")) return;
    var el = e.target.closest(".cms-widget-target");
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    var id = el.getAttribute("data-cms-placeholder-id");
    var name = el.getAttribute("data-cms-placeholder-name") || id;
    if (id) openModal(id, name);
  }}, true);

  if (modalClose) modalClose.addEventListener("click", closeModal);
  if (modal) {{
    modal.addEventListener("click", function(e) {{
      if (e.target === modal) closeModal();
    }});
  }}
  document.addEventListener("keydown", function(e) {{
    if (e.key === "Escape") closeModal();
  }});

  window.addEventListener("message", function(e) {{
    if (e.origin !== window.location.origin) return;
    var data = e.data;
    if (!data || data.type !== "cms-embed-saved") return;
    closeModal();
    window.location.reload();
  }});
}})();
</script>"#
    );

    let html_lower = html.to_lowercase();
    if let Some(body_pos) = html_lower.find("<body") {
        if let Some(body_gt) = html[body_pos..].find('>') {
            let body_insert_at = body_pos + body_gt + 1;
            html.insert_str(body_insert_at, head_banner.as_str());
        }
    } else {
        html.insert_str(0, head_banner.as_str());
    }

    if let Some(close_pos) = html.to_lowercase().rfind("</body>") {
        html.insert_str(close_pos, footer.as_str());
    } else {
        html.push_str(footer.as_str());
    }

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
    layout: layout::AdminLayoutCtx,
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
        layout,
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
    auth: &AuthUser,
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
        layout::AdminLayoutCtx::new(auth),
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
        "MiniJinja の構文（{{ blogname }} など）が使えます。サイト変数: blogname / blogdescription。\
         プレースホルダーは /admin/posts で定義し、推奨は {{ プレースホルダー名_html | safe }} でウィジェット HTML を差し込む方式です。\
         上級者向けに {{ news }} / has_news など構造化変数の直接参照も可能です（MiniJinja テンプレートページのみ）。".to_string()
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
            updated_at: super::format_updated_at(&page.updated_at),
            can_delete: !is_home,
        }
    }
}
