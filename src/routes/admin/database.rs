use askama::Template;
use axum::{
    Form, Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;

use crate::error::{ApiResult, AppError, AppResult, DomainError};
use crate::services::database::{
    self, DbObjectItem, SeedFormRow, TableColumnInput, TestDataSeedForm, DEFAULT_SEED_COUNT,
};
use crate::state::AppState;

use super::{auth::AuthUser, layout};

#[derive(Debug, Clone)]
struct TableListItem {
    name: String,
    row_count: Option<i64>,
    is_system: bool,
    can_edit: bool,
    can_view_data: bool,
    edit_url: String,
    data_url: String,
}

#[derive(Template)]
#[template(path = "admin/database/table_data.html")]
struct TableDataTemplate {
    layout: layout::AdminLayoutCtx,
    table_name: String,
    data_api_url: String,
    read_only: bool,
    edit_url: String,
    seed_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct TableDataRowsQuery {
    #[serde(default)]
    offset: i64,
}

#[derive(serde::Serialize)]
struct TableDataRowsResponse {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    total_count: i64,
    offset: i64,
    shown_count: i64,
    has_more: bool,
    chunk_size: i64,
}

#[derive(Template)]
#[template(path = "admin/database/table_notice.html")]
struct TableNoticeTemplate {
    layout: layout::AdminLayoutCtx,
    table_name: String,
    message: String,
}

#[derive(Template)]
#[template(path = "admin/database/table_seed_form.html")]
struct TableSeedFormTemplate {
    layout: layout::AdminLayoutCtx,
    table_name: String,
    action: String,
    data_url: String,
    count: String,
    max_count: u32,
    columns: Vec<SeedFormRow>,
    has_columns: bool,
    error_message: String,
}

#[derive(Template)]
#[template(path = "admin/database/index.html")]
struct DatabaseTemplate {
    layout: layout::AdminLayoutCtx,
    tables: Vec<TableListItem>,
    views: Vec<DbObjectItem>,
    has_tables: bool,
    has_views: bool,
    is_tables_tab: bool,
    is_views_tab: bool,
    tables_tab_url: &'static str,
    views_tab_url: &'static str,
    new_url: String,
    database_path: String,
}

#[derive(Debug, Clone)]
struct ColumnFormRow {
    name: String,
    orig_name: String,
    type_key: String,
    nullable: bool,
}

#[derive(Template)]
#[template(path = "admin/database/table_form.html")]
struct TableFormTemplate {
    layout: layout::AdminLayoutCtx,
    heading: String,
    action: String,
    cancel_url: &'static str,
    submit_label: String,
    name: String,
    name_readonly: bool,
    is_edit: bool,
    columns: Vec<ColumnFormRow>,
    error_message: String,
    success_message: String,
    data_url: String,
}

#[derive(Template)]
#[template(path = "admin/database/view_form.html")]
struct ViewFormTemplate {
    layout: layout::AdminLayoutCtx,
    action: &'static str,
    cancel_url: &'static str,
    name: String,
    definition: String,
    error_message: String,
}

#[derive(Debug, Deserialize, Default)]
struct DatabaseQuery {
    #[serde(default)]
    tab: String,
}

#[derive(Debug, Deserialize, Default)]
struct TableCreateForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    col_name: Vec<String>,
    #[serde(default)]
    col_type: Vec<String>,
    #[serde(default)]
    col_nullable: Vec<String>,
    #[serde(default)]
    col_orig_name: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ViewCreateForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    definition: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/database", get(index))
        .route(
            "/admin/database/tables/new",
            get(new_table_form).post(create_table),
        )
        .route(
            "/admin/database/tables/{name}/edit",
            get(edit_table_form).post(update_table),
        )
        .route("/admin/database/tables/{name}/data", get(table_data))
        .route(
            "/admin/database/tables/{name}/data/rows",
            get(table_data_rows),
        )
        .route(
            "/admin/database/tables/{name}/data/seed",
            get(table_seed_form).post(generate_table_seed),
        )
        .route(
            "/admin/database/views/new",
            get(new_view_form).post(create_view),
        )
}

async fn index(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<DatabaseQuery>,
) -> AppResult<impl IntoResponse> {
    let tables = database::list_tables(&state.pool())
        .await?
        .into_iter()
        .map(table_list_item)
        .collect::<Vec<_>>();
    let views = database::list_views(&state.pool()).await?;
    let is_views_tab = query.tab == "views";
    let new_url = if is_views_tab {
        "/admin/database/views/new".to_string()
    } else {
        "/admin/database/tables/new".to_string()
    };

    let html = DatabaseTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        has_tables: !tables.is_empty(),
        has_views: !views.is_empty(),
        tables,
        views,
        is_tables_tab: !is_views_tab,
        is_views_tab,
        tables_tab_url: "/admin/database",
        views_tab_url: "/admin/database?tab=views",
        new_url,
        database_path: state.config.database.path.clone(),
    }
    .render()?;

    Ok(Html(html))
}

async fn new_table_form(auth: AuthUser) -> AppResult<Response> {
    let html = table_form_template(
        &auth,
        TableFormParams {
            heading: "テーブルを追加",
            action: "/admin/database/tables/new".to_string(),
            submit_label: "追加する",
            name: "",
            name_readonly: false,
            is_edit: false,
            columns: &[],
            error_message: "",
            success_message: "",
        },
    )?;
    Ok(Html(html).into_response())
}

async fn edit_table_form(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    let columns = match database::load_user_table_columns(&state.pool(), &name).await {
        Ok(columns) => columns,
        Err(DomainError::SystemTable(message)) => {
            return Ok(system_table_notice_response(&auth, &name, &message).await?);
        }
        Err(DomainError::NotFound) => return Err(AppError::NotFound),
        Err(err) => {
            let message = domain_error_message(&err);
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "列を編集",
                    action: table_url(&name, "/edit"),
                    submit_label: "保存する",
                    name: &name,
                    name_readonly: true,
                    is_edit: true,
                    columns: &[],
                    error_message: &message,
                    success_message: "",
                },
            )?;
            return Ok(Html(html).into_response());
        }
    };

    let rows = columns_to_form_rows(&columns);
    let html = table_form_template(
        &auth,
        TableFormParams {
            heading: "列を編集",
            action: table_url(&name, "/edit"),
            submit_label: "保存する",
            name: &name,
            name_readonly: true,
            is_edit: true,
            columns: &rows,
            error_message: "",
            success_message: "",
        },
    )?;
    Ok(Html(html).into_response())
}

async fn table_data(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    match database::ensure_user_table_viewable(&state.pool(), &name).await {
        Ok(()) => {}
        Err(DomainError::SystemTable(message)) => {
            return Ok(system_table_notice_response(&auth, &name, &message).await?);
        }
        Err(DomainError::NotFound) => return Err(AppError::NotFound),
        Err(err) => return Err(err.into()),
    }

    let read_only = database::is_cms_readonly_table(&name);
    let html = TableDataTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        table_name: name.clone(),
        data_api_url: table_url(&name, "/data/rows"),
        read_only,
        edit_url: if read_only {
            String::new()
        } else {
            table_url(&name, "/edit")
        },
        seed_url: if read_only {
            String::new()
        } else {
            table_url(&name, "/data/seed")
        },
    }
    .render()?;

    Ok(Html(html).into_response())
}

async fn table_data_rows(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<TableDataRowsQuery>,
) -> ApiResult<Json<TableDataRowsResponse>> {
    let data = database::list_user_table_rows(&state.pool(), &name, query.offset).await?;
    let shown_count = data.rows.len() as i64;
    Ok(Json(TableDataRowsResponse {
        columns: data.columns,
        rows: data.rows,
        total_count: data.total_count,
        offset: data.offset,
        shown_count,
        has_more: data.has_more,
        chunk_size: database::TABLE_DATA_CHUNK_SIZE,
    }))
}

async fn table_seed_form(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    let columns = match database::load_user_table_columns(&state.pool(), &name).await {
        Ok(columns) => columns,
        Err(DomainError::SystemTable(message)) => {
            return Ok(system_table_notice_response(&auth, &name, &message).await?);
        }
        Err(DomainError::NotFound) => return Err(AppError::NotFound),
        Err(err) => {
            return Ok(Html(
                table_seed_form_template(
                    &auth,
                    &name,
                    &[],
                    DEFAULT_SEED_COUNT.to_string(),
                    &domain_error_message(&err),
                )?,
            )
            .into_response());
        }
    };

    let rows = database::build_seed_form_rows(&columns);
    Ok(Html(
        table_seed_form_template(
            &auth,
            &name,
            &rows,
            DEFAULT_SEED_COUNT.to_string(),
            "",
        )?,
    )
    .into_response())
}

async fn generate_table_seed(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Bytes,
) -> AppResult<Response> {
    let columns = match database::load_user_table_columns(&state.pool(), &name).await {
        Ok(columns) => columns,
        Err(DomainError::SystemTable(message)) => {
            return Ok(system_table_notice_response(&auth, &name, &message).await?);
        }
        Err(DomainError::NotFound) => return Err(AppError::NotFound),
        Err(err) => {
            let html = table_seed_form_template(
                &auth,
                &name,
                &database::build_seed_form_rows(&[]),
                DEFAULT_SEED_COUNT.to_string(),
                &domain_error_message(&err),
            )?;
            return Ok(Html(html).into_response());
        }
    };

    let form = match parse_seed_form_body(&body) {
        Ok(form) => form,
        Err(message) => {
            let html = table_seed_form_template(
                &auth,
                &name,
                &seed_form_rows_from_submission(&columns, &TestDataSeedForm::default()),
                DEFAULT_SEED_COUNT.to_string(),
                &message,
            )?;
            return Ok(Html(html).into_response());
        }
    };

    let count_display = form.count.clone();
    let (count, rules) = match database::parse_seed_form(&columns, &form) {
        Ok(parsed) => parsed,
        Err(err) => {
            let html = table_seed_form_template(
                &auth,
                &name,
                &seed_form_rows_from_submission(&columns, &form),
                count_display,
                &domain_error_message(&err),
            )?;
            return Ok(Html(html).into_response());
        }
    };

    match database::generate_test_data(&state.pool(), &name, count, &rules).await {
        Ok(_) => Ok(
            Redirect::to(&table_url(&name, "/data")).into_response(),
        ),
        Err(DomainError::SystemTable(message)) => {
            Ok(system_table_notice_response(&auth, &name, &message).await?)
        }
        Err(DomainError::NotFound) => Err(AppError::NotFound),
        Err(err) => {
            let html = table_seed_form_template(
                &auth,
                &name,
                &seed_form_rows_from_submission(&columns, &form),
                count_display,
                &domain_error_message(&err),
            )?;
            Ok(Html(html).into_response())
        }
    }
}

async fn create_table(
    auth: AuthUser,
    State(state): State<AppState>,
    body: Bytes,
) -> AppResult<Response> {
    let form = match parse_table_create_form(&body) {
        Ok(form) => form,
        Err(message) => {
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "テーブルを追加",
                    action: "/admin/database/tables/new".to_string(),
                    submit_label: "追加する",
                    name: "",
                    name_readonly: false,
                    is_edit: false,
                    columns: &[],
                    error_message: &message,
                    success_message: "",
                },
            )?;
            return Ok(Html(html).into_response());
        }
    };

    let columns = table_form_to_columns(&form);
    match database::create_user_table_from_columns(&state.pool(), &form.name, &columns).await {
        Ok(()) => Ok(Redirect::to("/admin/database").into_response()),
        Err(err) => {
            let rows = columns_to_form_rows(&columns);
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "テーブルを追加",
                    action: "/admin/database/tables/new".to_string(),
                    submit_label: "追加する",
                    name: &form.name,
                    name_readonly: false,
                    is_edit: false,
                    columns: &rows,
                    error_message: &domain_error_message(&err),
                    success_message: "",
                },
            )?;
            Ok(Html(html).into_response())
        }
    }
}

async fn update_table(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Bytes,
) -> AppResult<Response> {
    let form = match parse_table_create_form(&body) {
        Ok(form) => form,
        Err(message) => {
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "列を編集",
                    action: table_url(&name, "/edit"),
                    submit_label: "保存する",
                    name: &name,
                    name_readonly: true,
                    is_edit: true,
                    columns: &[],
                    error_message: &message,
                    success_message: "",
                },
            )?;
            return Ok(Html(html).into_response());
        }
    };

    if form.name != name {
        let html = table_form_template(
            &auth,
            TableFormParams {
                heading: "列を編集",
                action: table_url(&name, "/edit"),
                submit_label: "保存する",
                name: &name,
                name_readonly: true,
                is_edit: true,
                columns: &columns_to_form_rows(&table_form_to_columns(&form)),
                error_message: "テーブル名は変更できません",
                success_message: "",
            },
        )?;
        return Ok(Html(html).into_response());
    }

    let columns = table_form_to_columns(&form);
    match database::update_user_table_from_columns(&state.pool(), &name, &columns).await {
        Ok(()) => {
            let updated = database::load_user_table_columns(&state.pool(), &name).await?;
            let rows = columns_to_form_rows(&updated);
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "列を編集",
                    action: table_url(&name, "/edit"),
                    submit_label: "保存する",
                    name: &name,
                    name_readonly: true,
                    is_edit: true,
                    columns: &rows,
                    error_message: "",
                    success_message: "列定義を保存しました",
                },
            )?;
            Ok(Html(html).into_response())
        }
        Err(DomainError::SystemTable(message)) => {
            Ok(system_table_notice_response(&auth, &name, &message).await?)
        }
        Err(DomainError::NotFound) => Err(AppError::NotFound),
        Err(err) => {
            let rows = columns_to_form_rows(&columns);
            let html = table_form_template(
                &auth,
                TableFormParams {
                    heading: "列を編集",
                    action: table_url(&name, "/edit"),
                    submit_label: "保存する",
                    name: &name,
                    name_readonly: true,
                    is_edit: true,
                    columns: &rows,
                    error_message: &domain_error_message(&err),
                    success_message: "",
                },
            )?;
            Ok(Html(html).into_response())
        }
    }
}

async fn new_view_form(auth: AuthUser) -> AppResult<impl IntoResponse> {
    let html = view_form_template(&auth, "", "", "")?;
    Ok(Html(html))
}

async fn create_view(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<ViewCreateForm>,
) -> AppResult<Response> {
    match database::create_user_view(&state.pool(), &form.name, &form.definition).await {
        Ok(()) => Ok(Redirect::to("/admin/database?tab=views").into_response()),
        Err(err) => {
            let html = view_form_template(
                &auth,
                &form.name,
                &form.definition,
                &domain_error_message(&err),
            )?;
            Ok(Html(html).into_response())
        }
    }
}

fn table_url(name: &str, suffix: &str) -> String {
    format!(
        "/admin/database/tables/{}{}",
        urlencoding::encode(name),
        suffix
    )
}

fn table_list_item(item: DbObjectItem) -> TableListItem {
    let can_edit = database::is_db_admin_editable(&item.name);
    let can_view_data = database::is_db_admin_data_viewable(&item.name);
    let edit_url = if can_edit {
        table_url(&item.name, "/edit")
    } else {
        String::new()
    };
    let data_url = if can_view_data {
        table_url(&item.name, "/data")
    } else {
        String::new()
    };

    TableListItem {
        name: item.name,
        row_count: item.row_count,
        is_system: item.is_system,
        can_edit,
        can_view_data,
        edit_url,
        data_url,
    }
}

fn parse_seed_form_body(body: &Bytes) -> Result<TestDataSeedForm, String> {
    let body = std::str::from_utf8(body).map_err(|_| "フォームデータの形式が不正です".to_string())?;
    serde_html_form::from_str(body).map_err(|err| format!("フォームデータの解析に失敗しました: {err}"))
}

fn seed_form_rows_from_submission(
    columns: &[TableColumnInput],
    form: &TestDataSeedForm,
) -> Vec<SeedFormRow> {
    let defaults = database::build_seed_form_rows(columns);
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let default = defaults.get(index).cloned().unwrap_or_else(|| SeedFormRow {
                name: column.name.clone(),
                type_key: column.type_key.clone(),
                type_label: column.type_key.clone(),
                nullable: column.nullable,
                int_min: "0".to_string(),
                int_max: "1000".to_string(),
                real_min: "0".to_string(),
                real_max: "100".to_string(),
                text_min: "8".to_string(),
                text_max: "64".to_string(),
                charset: "ascii_alnum".to_string(),
                blob_min: "1".to_string(),
                blob_max: "32".to_string(),
                timestamp_from: String::new(),
                timestamp_to: String::new(),
                include_null: false,
            });
            SeedFormRow {
                name: column.name.clone(),
                type_key: column.type_key.clone(),
                type_label: default.type_label,
                nullable: column.nullable,
                int_min: form
                    .col_int_min
                    .get(index)
                    .cloned()
                    .unwrap_or(default.int_min),
                int_max: form
                    .col_int_max
                    .get(index)
                    .cloned()
                    .unwrap_or(default.int_max),
                real_min: form
                    .col_real_min
                    .get(index)
                    .cloned()
                    .unwrap_or(default.real_min),
                real_max: form
                    .col_real_max
                    .get(index)
                    .cloned()
                    .unwrap_or(default.real_max),
                text_min: form
                    .col_text_min
                    .get(index)
                    .cloned()
                    .unwrap_or(default.text_min),
                text_max: form
                    .col_text_max
                    .get(index)
                    .cloned()
                    .unwrap_or(default.text_max),
                charset: form
                    .col_charset
                    .get(index)
                    .cloned()
                    .unwrap_or(default.charset),
                blob_min: form
                    .col_blob_min
                    .get(index)
                    .cloned()
                    .unwrap_or(default.blob_min),
                blob_max: form
                    .col_blob_max
                    .get(index)
                    .cloned()
                    .unwrap_or(default.blob_max),
                timestamp_from: form
                    .col_timestamp_from
                    .get(index)
                    .cloned()
                    .unwrap_or(default.timestamp_from),
                timestamp_to: form
                    .col_timestamp_to
                    .get(index)
                    .cloned()
                    .unwrap_or(default.timestamp_to),
                include_null: form
                    .col_include_null
                    .get(index)
                    .map(|value| value == "1")
                    .unwrap_or(false),
            }
        })
        .collect()
}

fn table_seed_form_template(
    auth: &AuthUser,
    table_name: &str,
    columns: &[SeedFormRow],
    count: String,
    error_message: &str,
) -> AppResult<String> {
    Ok(TableSeedFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        table_name: table_name.to_string(),
        action: table_url(table_name, "/data/seed"),
        data_url: table_url(table_name, "/data"),
        count,
        max_count: database::MAX_TEST_DATA_ROWS,
        has_columns: !columns.is_empty(),
        columns: columns.to_vec(),
        error_message: error_message.to_string(),
    }
    .render()?)
}

fn parse_table_create_form(body: &Bytes) -> Result<TableCreateForm, String> {
    let body = std::str::from_utf8(body).map_err(|_| "フォームデータの形式が不正です".to_string())?;
    serde_html_form::from_str(body).map_err(|err| format!("フォームデータの解析に失敗しました: {err}"))
}

fn table_form_to_columns(form: &TableCreateForm) -> Vec<TableColumnInput> {
    let row_count = form
        .col_name
        .len()
        .max(form.col_type.len())
        .max(form.col_nullable.len())
        .max(form.col_orig_name.len());

    let mut columns = Vec::new();
    for index in 0..row_count {
        let name = form.col_name.get(index).cloned().unwrap_or_default();
        let type_key = form
            .col_type
            .get(index)
            .cloned()
            .unwrap_or_else(|| "text".to_string());
        let nullable = form
            .col_nullable
            .get(index)
            .map(|value| value == "1")
            .unwrap_or(true);
        let orig_name = form
            .col_orig_name
            .get(index)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        columns.push(TableColumnInput {
            name,
            type_key,
            nullable,
            orig_name,
        });
    }
    columns
}

fn columns_to_form_rows(columns: &[TableColumnInput]) -> Vec<ColumnFormRow> {
    columns
        .iter()
        .map(|column| ColumnFormRow {
            name: column.name.clone(),
            orig_name: column
                .orig_name
                .clone()
                .unwrap_or_else(|| column.name.clone()),
            type_key: column.type_key.clone(),
            nullable: column.nullable,
        })
        .collect()
}

struct TableFormParams<'a> {
    heading: &'a str,
    action: String,
    submit_label: &'a str,
    name: &'a str,
    name_readonly: bool,
    is_edit: bool,
    columns: &'a [ColumnFormRow],
    error_message: &'a str,
    success_message: &'a str,
}

fn table_form_template(auth: &AuthUser, params: TableFormParams<'_>) -> AppResult<String> {
    let data_url = if params.is_edit {
        table_url(params.name, "/data")
    } else {
        String::new()
    };
    Ok(TableFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        heading: params.heading.to_string(),
        action: params.action,
        cancel_url: "/admin/database",
        submit_label: params.submit_label.to_string(),
        name: params.name.to_string(),
        name_readonly: params.name_readonly,
        is_edit: params.is_edit,
        columns: params.columns.to_vec(),
        error_message: params.error_message.to_string(),
        success_message: params.success_message.to_string(),
        data_url,
    }
    .render()?)
}

fn view_form_template(
    auth: &AuthUser,
    name: &str,
    definition: &str,
    error_message: &str,
) -> AppResult<String> {
    Ok(ViewFormTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        action: "/admin/database/views/new",
        cancel_url: "/admin/database?tab=views",
        name: name.to_string(),
        definition: definition.to_string(),
        error_message: error_message.to_string(),
    }
    .render()?)
}

fn domain_error_message(err: &DomainError) -> String {
    match err {
        DomainError::Validation(msg) | DomainError::Conflict(msg) | DomainError::BadRequest(msg) => {
            msg.clone()
        }
        DomainError::SystemTable(msg) => msg.clone(),
        DomainError::NotFound => "オブジェクトが見つかりません".to_string(),
        DomainError::Internal(e) => e.to_string(),
    }
}

async fn system_table_notice_response(
    auth: &AuthUser,
    table_name: &str,
    message: &str,
) -> AppResult<Response> {
    let html = TableNoticeTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        table_name: table_name.to_string(),
        message: message.to_string(),
    }
    .render()?;
    Ok(Html(html).into_response())
}
