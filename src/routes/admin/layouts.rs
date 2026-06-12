use askama::Template;
use axum::{
    Form, Router,
    extract::{Multipart, Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::layout::{LayoutImportMode, LayoutInput};
use crate::repos::layouts as layouts_repo;
use crate::services::{self, layouts::LayoutAdminFile};
use crate::state::AppState;
use crate::theme;

use super::{auth::AuthUser, format_updated_at, layout as admin_layout};

#[derive(Debug, Deserialize)]
struct LayoutForm {
    key: String,
    name: String,
    #[serde(default)]
    is_default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LayoutFileForm {
    content: String,
}

#[derive(Debug, Deserialize)]
struct NewStaticFileForm {
    relative_path: String,
    content: String,
}

#[derive(Debug, Deserialize, Default)]
struct EditQuery {
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize, Default)]
struct LayoutIndexQuery {
    #[serde(default)]
    success_message: String,
    #[serde(default)]
    error_message: String,
}

#[derive(Debug, Deserialize)]
struct StaticDeleteForm {
    relative_path: String,
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

/// レイアウト編集画面のファイル一覧行。
#[derive(Debug, Clone)]
struct LayoutFileRow {
    display_path: String,
    kind_label: String,
    size_label: String,
    public_url: String,
    is_editable: bool,
    edit_url: String,
    is_deletable: bool,
    delete_path: String,
}

struct FileEditView {
    layout_id: i64,
    layout_name: String,
    file_label: String,
    action: String,
    content: String,
    relative_path: String,
    is_new_file: bool,
    help_text: String,
    error_message: String,
}

const SHELL_HELP: &str = "共通の head / nav / footer。ページ本文は block content に差し込まれます。\
    静的ファイルは /static/ レイアウトkey/ を参照できます。favicon は \
    <a href=\"/admin/media\">メディア</a> で公開 URL を /favicon.ico に設定してください。";

const NEW_STATIC_HELP: &str = "static/ からの相対パス（例: site.css, js/app.js）を下のパス欄に入力してください。\
    css / js / svg / json / txt / map のみ作成できます。";

#[derive(Template)]
#[template(path = "admin/layouts/index.html")]
struct LayoutIndexTemplate {
    layout: admin_layout::AdminLayoutCtx,
    layouts: Vec<LayoutListItem>,
    success_message: String,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/layouts/import.html")]
struct LayoutImportTemplate {
    layout: admin_layout::AdminLayoutCtx,
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
    layout_files: Vec<LayoutFileRow>,
    layout_id: i64,
    upload_action: String,
    new_file_url: String,
    is_edit: bool,
    key_readonly: bool,
    delete_action: String,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/layouts/file_edit.html")]
struct LayoutFileEditTemplate {
    layout: admin_layout::AdminLayoutCtx,
    heading: String,
    file_label: String,
    action: String,
    content: String,
    relative_path: String,
    is_new_file: bool,
    help_text: String,
    back_url: String,
    error_message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/layouts", get(index).post(create))
        .route("/admin/layouts/import", get(import_form).post(import_layout))
        .route("/admin/layouts/{id}/export", get(export_layout))
        .route("/admin/layouts/new", get(new_form))
        .route("/admin/layouts/{id}/edit", get(edit).post(update))
        .route("/admin/layouts/{id}/delete", post(destroy))
        .route("/admin/layouts/{id}/static/upload", post(upload_static))
        .route("/admin/layouts/{id}/static/delete", post(delete_static))
        .route(
            "/admin/layouts/{id}/files/shell.html",
            get(edit_shell).post(update_shell),
        )
        .route(
            "/admin/layouts/{id}/files/static/{*path}",
            get(edit_static).post(update_static),
        )
        .route(
            "/admin/layouts/{id}/files/new",
            get(new_static_file).post(create_static_file),
        )
}

async fn index(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<LayoutIndexQuery>,
) -> AppResult<impl IntoResponse> {
    let rows = layouts_repo::list_all(&state.pool()).await?;
    let mut layouts = Vec::with_capacity(rows.len());
    for row in rows {
        let page_count = layouts_repo::count_pages(&state.pool(), row.id).await?;
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
        success_message: query.success_message,
        error_message: query.error_message,
    }
    .render()?;

    Ok(Html(html))
}

async fn import_form(auth: AuthUser) -> AppResult<impl IntoResponse> {
    let html = LayoutImportTemplate {
        layout: admin_layout::AdminLayoutCtx::new(&auth),
    }
    .render()?;

    Ok(Html(html))
}

async fn export_layout(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> AppResult<Response> {
    let layout = layouts_repo::find(&state.pool(), id).await?;
    let bytes = services::layouts::export_layout_zip(&state.pool(), &state.config, id).await?;
    let filename = format!("layout-{}.zip", layout.key);
    let disposition = format!("attachment; filename=\"{filename}\"");

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|e| AppError::Other(e.into()))?,
    );

    Ok((headers, bytes).into_response())
}

async fn import_layout(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut mode = LayoutImportMode::Overwrite;
    let mut target_key = String::new();
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
                match text.trim() {
                    "skip" => mode = LayoutImportMode::Skip,
                    "rename" => mode = LayoutImportMode::Rename,
                    _ => mode = LayoutImportMode::Overwrite,
                }
            }
            Some("target_key") => {
                target_key = field
                    .text()
                    .await
                    .map_err(|err| AppError::Other(err.into()))?;
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
            "ZIP ファイル（package）を選択してください",
        ));
    };

    let target_key = (!target_key.trim().is_empty()).then(|| target_key.trim());
    match services::layouts::import_layout_zip(
        &state.pool(),
        &state.config,
        &bytes,
        mode,
        target_key,
    )
    .await {
        Ok((_, message)) => Ok(redirect_index_with_success(&message)),
        Err(err) => Ok(redirect_index_with_error(&err.to_string())),
    }
}

fn redirect_index_with_success(message: &str) -> Response {
    let encoded = urlencoding::encode(message);
    Redirect::to(&format!("/admin/layouts?success_message={encoded}")).into_response()
}

fn redirect_index_with_error(message: &str) -> Response {
    let encoded = urlencoding::encode(message);
    Redirect::to(&format!("/admin/layouts?error_message={encoded}")).into_response()
}

async fn new_form(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = build_layout_form(
        &state.pool(),
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを新規追加",
        "/admin/layouts",
        "作成する",
        String::new(),
        String::new(),
        false,
        Vec::new(),
        0,
        false,
        false,
        "",
        "",
    )
    .await?
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
            match services::layouts::create_layout_with_defaults(&state.pool(), &state.config, &input)
                .await
            {
                Ok(id) => Ok(redirect_layout_edit(id, None).into_response()),
                Err(err) => {
                    layout_error_response(
                        &state.pool(),
                        &state.config.paths.work_dir,
                        &auth,
                        &form,
                        false,
                        None,
                        err.to_string(),
                    )
                    .await
                }
            }
        }
        Err(message) => {
            layout_error_response(
                &state.pool(),
                &state.config.paths.work_dir,
                &auth,
                &form,
                false,
                None,
                message,
            )
            .await
        }
    }
}

async fn edit(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Query(query): Query<EditQuery>,
) -> AppResult<impl IntoResponse> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    let layout_files = load_layout_file_rows(&state.config.paths.work_dir, id, &row.key);

    let html = build_layout_form(
        &state.pool(),
        admin_layout::AdminLayoutCtx::new(&auth),
        "レイアウトを編集",
        &format!("/admin/layouts/{id}/edit"),
        "更新する",
        row.key,
        row.name,
        row.is_default,
        layout_files,
        id,
        true,
        true,
        &query.error,
        &format!("/admin/layouts/{id}/delete"),
    )
    .await?
    .render()?;

    Ok(Html(html))
}

async fn update(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Form(form): Form<LayoutForm>,
) -> AppResult<Response> {
    match form.into_input() {
        Ok(input) => {
            if let Err(err) =
                services::layouts::update_layout_meta(&state.pool(), &state.config, id, &input).await
            {
                return layout_error_response(
                    &state.pool(),
                    &state.config.paths.work_dir,
                    &auth,
                    &form,
                    true,
                    Some(id),
                    err.to_string(),
                )
                .await;
            }
            Ok(redirect_layout_edit(id, None).into_response())
        }
        Err(message) => {
            layout_error_response(
                &state.pool(),
                &state.config.paths.work_dir,
                &auth,
                &form,
                true,
                Some(id),
                message,
            )
            .await
        }
    }
}

async fn edit_shell(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Query(query): Query<EditQuery>,
) -> AppResult<impl IntoResponse> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    let content =
        theme::read_shell(&state.config.paths.work_dir, &row.key).unwrap_or_default();

    render_file_edit_page(
        &auth,
        FileEditView {
            layout_id: id,
            layout_name: row.name,
            file_label: "shell.html（MiniJinja）".to_string(),
            action: format!("/admin/layouts/{id}/files/shell.html"),
            content,
            relative_path: String::new(),
            is_new_file: false,
            help_text: SHELL_HELP.to_string(),
            error_message: query.error,
        },
    )
}

async fn update_shell(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Form(form): Form<LayoutFileForm>,
) -> AppResult<Response> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    match services::layouts::write_shell_content(&state.config, &row.key, &form.content) {
        Ok(()) => Ok(redirect_layout_edit(id, None).into_response()),
        Err(err) => {
            render_file_edit_response(
                &auth,
                FileEditView {
                    layout_id: id,
                    layout_name: row.name,
                    file_label: "shell.html（MiniJinja）".to_string(),
                    action: format!("/admin/layouts/{id}/files/shell.html"),
                    content: form.content,
                    relative_path: String::new(),
                    is_new_file: false,
                    help_text: SHELL_HELP.to_string(),
                    error_message: err.to_string(),
                },
            )
            .await
        }
    }
}

async fn edit_static(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((id, path)): AxumPath<(i64, String)>,
    Query(query): Query<EditQuery>,
) -> AppResult<impl IntoResponse> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    if !theme::is_editable_text_static(&path) {
        return Err(AppError::NotFound);
    }
    let content = theme::read_static_text(&state.config.paths.work_dir, &row.key, &path)
        .map_err(|_| AppError::NotFound)?;

    let encoded_path = urlencoding::encode(&path);
    render_file_edit_page(
        &auth,
        FileEditView {
            layout_id: id,
            layout_name: row.name,
            file_label: format!("static/{path}"),
            action: format!("/admin/layouts/{id}/files/static/{encoded_path}"),
            content,
            relative_path: String::new(),
            is_new_file: false,
            help_text: format!("公開 URL: /static/{}/{}", row.key, path),
            error_message: query.error,
        },
    )
}

async fn update_static(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath((id, path)): AxumPath<(i64, String)>,
    Form(form): Form<LayoutFileForm>,
) -> AppResult<Response> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    let label = format!("static/{path}");
    match services::layouts::write_static_text_file(&state.config, &row.key, &path, &form.content) {
        Ok(()) => Ok(redirect_layout_edit(id, None).into_response()),
        Err(err) => {
            let encoded_path = urlencoding::encode(&path);
            render_file_edit_response(
                &auth,
                FileEditView {
                    layout_id: id,
                    layout_name: row.name,
                    file_label: label,
                    action: format!("/admin/layouts/{id}/files/static/{encoded_path}"),
                    content: form.content,
                    relative_path: String::new(),
                    is_new_file: false,
                    help_text: format!("公開 URL: /static/{}/{}", row.key, path),
                    error_message: err.to_string(),
                },
            )
            .await
        }
    }
}

async fn new_static_file(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Query(query): Query<EditQuery>,
) -> AppResult<impl IntoResponse> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    render_file_edit_page(
        &auth,
        FileEditView {
            layout_id: id,
            layout_name: row.name,
            file_label: "新規テキストファイル".to_string(),
            action: format!("/admin/layouts/{id}/files/new"),
            content: String::new(),
            relative_path: String::new(),
            is_new_file: true,
            help_text: NEW_STATIC_HELP.to_string(),
            error_message: query.error,
        },
    )
}

async fn create_static_file(
    auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Form(form): Form<NewStaticFileForm>,
) -> AppResult<Response> {
    let row = layouts_repo::find(&state.pool(), id).await?;
    match services::layouts::write_static_text_file(
        &state.config,
        &row.key,
        &form.relative_path,
        &form.content,
    ) {
        Ok(()) => Ok(redirect_layout_edit(id, None).into_response()),
        Err(err) => {
            render_file_edit_response(
                &auth,
                FileEditView {
                    layout_id: id,
                    layout_name: row.name,
                    file_label: "新規テキストファイル".to_string(),
                    action: format!("/admin/layouts/{id}/files/new"),
                    content: form.content,
                    relative_path: form.relative_path.trim().to_string(),
                    is_new_file: true,
                    help_text: NEW_STATIC_HELP.to_string(),
                    error_message: err.to_string(),
                },
            )
            .await
        }
    }
}

async fn upload_static(
    _auth: AuthUser,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let layout = layouts_repo::find(&state.pool(), id).await?;
    let mut relative_path = String::new();
    let mut file_bytes: Option<axum::body::Bytes> = None;
    let mut original_name = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Other(err.into()))?
    {
        match field.name() {
            Some("relative_path") => {
                relative_path = field
                    .text()
                    .await
                    .map_err(|err| AppError::Other(err.into()))?
                    .trim()
                    .to_string();
            }
            Some("file") => {
                original_name = field
                    .file_name()
                    .map(str::to_string)
                    .unwrap_or_else(|| "upload".to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|err| AppError::Other(err.into()))?;
                file_bytes = Some(data);
            }
            _ => {}
        }
    }

    let Some(bytes) = file_bytes else {
        return Ok(redirect_layout_edit(id, Some("ファイルが選択されていません")).into_response());
    };

    let target_path =
        services::layouts::resolve_static_upload_target_path(&relative_path, &original_name);

    match services::layouts::upload_static_file(&state.config, &layout.key, &target_path, &bytes) {
        Ok(_) => Ok(redirect_layout_edit(id, None).into_response()),
        Err(err) => Ok(redirect_layout_edit(id, Some(&err.to_string())).into_response()),
    }
}

async fn delete_static(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
    Form(form): Form<StaticDeleteForm>,
) -> AppResult<Redirect> {
    let layout = layouts_repo::find(&state.pool(), id).await?;
    services::layouts::delete_static_file(&state.config, &layout.key, &form.relative_path)?;
    Ok(redirect_layout_edit(id, None))
}

async fn destroy(State(state): State<AppState>, AxumPath(id): AxumPath<i64>) -> AppResult<Redirect> {
    services::layouts::delete_layout(&state.pool(), &state.config, id).await?;
    Ok(Redirect::to("/admin/layouts"))
}

async fn build_layout_form(
    _pool: &SqlitePool,
    layout: admin_layout::AdminLayoutCtx,
    heading: &str,
    action: &str,
    submit_label: &str,
    key: String,
    name: String,
    is_default: bool,
    layout_files: Vec<LayoutFileRow>,
    layout_id: i64,
    is_edit: bool,
    key_readonly: bool,
    error_message: &str,
    delete_action: &str,
) -> AppResult<LayoutFormTemplate> {
    let upload_action = if is_edit {
        format!("/admin/layouts/{layout_id}/static/upload")
    } else {
        String::new()
    };
    let new_file_url = if is_edit {
        format!("/admin/layouts/{layout_id}/files/new")
    } else {
        String::new()
    };

    Ok(LayoutFormTemplate {
        layout,
        heading: heading.to_string(),
        action: action.to_string(),
        submit_label: submit_label.to_string(),
        key,
        name,
        is_default,
        layout_files,
        layout_id,
        upload_action,
        new_file_url,
        is_edit,
        key_readonly,
        delete_action: delete_action.to_string(),
        error_message: error_message.to_string(),
    })
}

async fn layout_error_response(
    pool: &SqlitePool,
    work_dir: &str,
    auth: &AuthUser,
    form: &LayoutForm,
    is_edit: bool,
    id: Option<i64>,
    message: String,
) -> AppResult<Response> {
    let (heading, action, submit_label, delete_action, layout_id) = if is_edit {
        let id = id.expect("edit requires id");
        (
            "レイアウトを編集",
            format!("/admin/layouts/{id}/edit"),
            "更新する",
            format!("/admin/layouts/{id}/delete"),
            id,
        )
    } else {
        (
            "レイアウトを新規追加",
            "/admin/layouts".to_string(),
            "作成する",
            String::new(),
            0,
        )
    };

    let layout_files = if is_edit {
        let row = layouts_repo::find(pool, layout_id).await?;
        load_layout_file_rows(work_dir, layout_id, &row.key)
    } else {
        Vec::new()
    };

    let html = build_layout_form(
        pool,
        admin_layout::AdminLayoutCtx::new(auth),
        heading,
        &action,
        submit_label,
        form.key.clone(),
        form.name.clone(),
        form.is_default.is_some(),
        layout_files,
        layout_id,
        is_edit,
        is_edit,
        &message,
        &delete_action,
    )
    .await?
    .render()?;

    Ok(Html(html).into_response())
}

fn load_layout_file_rows(
    work_dir: &str,
    layout_id: i64,
    layout_key: &str,
) -> Vec<LayoutFileRow> {
    services::layouts::list_admin_files(work_dir, layout_key)
        .unwrap_or_default()
        .into_iter()
        .map(|file| admin_file_to_row(layout_id, &file))
        .collect()
}

fn admin_file_to_row(layout_id: i64, file: &LayoutAdminFile) -> LayoutFileRow {
    LayoutFileRow {
        display_path: file.display_path.clone(),
        kind_label: file.kind_label.clone(),
        size_label: file.size_label.clone(),
        public_url: file.public_url.clone(),
        is_editable: file.is_text_editable,
        edit_url: file.edit_url(layout_id).unwrap_or_default(),
        is_deletable: file.is_deletable,
        delete_path: file.delete_path.clone().unwrap_or_default(),
    }
}

fn redirect_layout_edit(id: i64, error: Option<&str>) -> Redirect {
    match error {
        Some(message) => {
            let encoded = urlencoding::encode(message);
            Redirect::to(&format!("/admin/layouts/{id}/edit?error={encoded}"))
        }
        None => Redirect::to(&format!("/admin/layouts/{id}/edit")),
    }
}

fn render_file_edit_page(auth: &AuthUser, view: FileEditView) -> AppResult<Html<String>> {
    let html = build_file_edit_template(auth, view).render()?;
    Ok(Html(html))
}

async fn render_file_edit_response(auth: &AuthUser, view: FileEditView) -> AppResult<Response> {
    Ok(render_file_edit_page(auth, view)?.into_response())
}

fn build_file_edit_template(auth: &AuthUser, view: FileEditView) -> LayoutFileEditTemplate {
    LayoutFileEditTemplate {
        layout: admin_layout::AdminLayoutCtx::new(auth),
        heading: format!("{} — {}", view.layout_name, view.file_label),
        file_label: view.file_label,
        action: view.action,
        content: view.content,
        relative_path: view.relative_path,
        is_new_file: view.is_new_file,
        help_text: view.help_text,
        back_url: format!("/admin/layouts/{}/edit", view.layout_id),
        error_message: view.error_message,
    }
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
