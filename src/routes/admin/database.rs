use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::time::Instant;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::{Context, Poll};

use askama::Template;
use axum::{
    Form, Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    response::{
        Html, IntoResponse, Redirect, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::error::{ApiResult, AppError, AppResult, DomainError};
use crate::services::database::{
    self, DbObjectItem, SeedFormRow, TableCellUpdateRequest, TableColumnInput, TableDataColumnMeta,
    TableFilterEntry, TableSortEntry, TestDataSeedForm, DEFAULT_SEED_COUNT,
};
use crate::state::AppState;

use super::{auth::AuthUser, breadcrumb, layout};

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

#[derive(Debug, Clone)]
struct ViewListItem {
    name: String,
    sql_preview: String,
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
    is_view: bool,
    edit_url: String,
    edit_label: &'static str,
    seed_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct TableDataRowsQuery {
    #[serde(default)]
    offset: i64,
    sort: Option<String>,
    filter: Option<String>,
}

#[derive(serde::Serialize)]
struct TableDataRowsResponse {
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>,
    total_count: i64,
    offset: i64,
    shown_count: i64,
    has_more: bool,
    chunk_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    column_widths: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<TableSortEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<Vec<TableFilterEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column_meta: Option<Vec<TableDataColumnMeta>>,
}

#[derive(Debug, Deserialize)]
struct ColumnWidthsSaveRequest {
    widths: HashMap<String, i32>,
}

#[derive(Debug, Deserialize)]
struct SortSaveRequest {
    sort: Vec<TableSortEntry>,
}

#[derive(Debug, Deserialize)]
struct FilterSaveRequest {
    filter: Vec<TableFilterEntry>,
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
    views: Vec<ViewListItem>,
    has_views: bool,
    is_tables_tab: bool,
    is_views_tab: bool,
    has_user_tables: bool,
    new_url: String,
    success_message: String,
    error_message: String,
    duplicate_payloads_json: String,
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
}

#[derive(Debug, Clone)]
struct ViewSourceTableOption {
    name: String,
}

#[derive(Template)]
#[template(path = "admin/database/view_form.html")]
struct ViewFormTemplate {
    layout: layout::AdminLayoutCtx,
    heading: String,
    action: String,
    cancel_url: &'static str,
    submit_label: String,
    name: String,
    name_readonly: bool,
    definition: String,
    error_message: String,
    table_options: Vec<ViewSourceTableOption>,
    ui_builder_json: String,
}

struct ViewFormParams<'a> {
    heading: &'a str,
    action: String,
    submit_label: &'a str,
    name: &'a str,
    name_readonly: bool,
    is_edit: bool,
    definition: &'a str,
    error_message: &'a str,
}

#[derive(Debug, Clone, Copy)]
enum DbAdminObjectKind {
    Table,
    View,
}

struct ListActionUrls {
    can_edit: bool,
    can_view_data: bool,
    edit_url: String,
    data_url: String,
}

struct DataPageParams {
    name: String,
    data_api_url: String,
    read_only: bool,
    is_view: bool,
    edit_url: String,
    edit_label: &'static str,
    seed_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct DatabaseQuery {
    #[serde(default)]
    tab: String,
    #[serde(default)]
    success_message: String,
    #[serde(default)]
    error_message: String,
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
    #[serde(default)]
    include_data: Option<String>,
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
        .route(
            "/admin/database/tables/{name}/duplicate",
            post(duplicate_table),
        )
        .route("/admin/database/tables/{name}/data", get(table_data))
        .route(
            "/admin/database/tables/{name}/columns",
            get(table_columns_json),
        )
        .route(
            "/admin/database/tables/{name}/data/rows",
            get(table_data_rows),
        )
        .route(
            "/admin/database/tables/{name}/data/column-widths",
            post(save_table_column_widths),
        )
        .route(
            "/admin/database/tables/{name}/data/sort",
            post(save_table_sort),
        )
        .route(
            "/admin/database/tables/{name}/data/filter",
            post(save_table_filter),
        )
        .route(
            "/admin/database/tables/{name}/data/cells",
            post(update_table_cell),
        )
        .route(
            "/admin/database/tables/{name}/data/seed",
            get(table_seed_form).post(generate_table_seed),
        )
        .route(
            "/admin/database/views/new",
            get(new_view_form).post(create_view),
        )
        .route(
            "/admin/database/views/{name}/edit",
            get(edit_view_form).post(update_view),
        )
        .route(
            "/admin/database/views/{name}/duplicate",
            post(duplicate_view),
        )
        .route("/admin/database/views/{name}/data", get(view_data))
        .route(
            "/admin/database/views/{name}/data/rows",
            get(table_data_rows),
        )
        .route(
            "/admin/database/views/{name}/data/column-widths",
            post(save_table_column_widths),
        )
        .route(
            "/admin/database/views/{name}/data/sort",
            post(save_table_sort),
        )
        .route(
            "/admin/database/views/{name}/data/filter",
            post(save_table_filter),
        )
}

async fn index(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<DatabaseQuery>,
) -> AppResult<impl IntoResponse> {
    let pool = state.pool();
    let (table_items, view_items) =
        tokio::try_join!(database::list_tables(&pool), database::list_views(&pool))?;
    let duplicate_payloads_json =
        serde_json::to_string(&database::build_duplicate_payloads(&table_items, &view_items))
            .unwrap_or_else(|_| r#"{"tables":{},"views":{}}"#.to_string());
    let mut has_user_tables = false;
    let tables = table_items
        .into_iter()
        .map(|item| {
            if !item.is_system {
                has_user_tables = true;
            }
            table_list_item(item)
        })
        .collect::<Vec<_>>();
    let views = view_items
        .into_iter()
        .map(view_list_item)
        .collect::<Vec<_>>();
    let is_views_tab = query.tab == "views";
    let new_url = if is_views_tab {
        "/admin/database/views/new".to_string()
    } else {
        "/admin/database/tables/new".to_string()
    };

    let html = DatabaseTemplate {
        layout: breadcrumb::with(layout::AdminLayoutCtx::new(&auth), breadcrumb::database_index()),
        has_user_tables,
        has_views: !views.is_empty(),
        tables,
        views,
        is_tables_tab: !is_views_tab,
        is_views_tab,
        new_url,
        success_message: query.success_message,
        error_message: query.error_message,
        duplicate_payloads_json,
    }
    .render()?;

    Ok(Html(html))
}

async fn duplicate_table(
    State(state): State<AppState>,
    Path(source): Path<String>,
    body: Bytes,
) -> AppResult<Response> {
    let form = match parse_table_create_form(&body) {
        Ok(form) => form,
        Err(message) => return Ok(redirect_database_tab("", None, Some(&message))),
    };

    let columns = table_form_to_columns(&form);
    let include_data = form.include_data.is_some();
    match database::duplicate_user_table(
        &state.pool(),
        &source,
        &form.name,
        &columns,
        include_data,
    )
    .await
    {
        Ok(()) => Ok(redirect_database_tab(
            "",
            Some(&format!(
                "テーブル「{source}」を「{}」として複製しました",
                form.name
            )),
            None,
        )),
        Err(err) => Ok(redirect_database_tab("", None, Some(&domain_error_message(&err)))),
    }
}

fn redirect_database_tab(tab: &str, success_message: Option<&str>, error_message: Option<&str>) -> Response {
    let mut params = Vec::new();
    if !tab.is_empty() {
        params.push(format!("tab={tab}"));
    }
    if let Some(message) = success_message.filter(|message| !message.is_empty()) {
        params.push(format!(
            "success_message={}",
            urlencoding::encode(message)
        ));
    }
    if let Some(message) = error_message.filter(|message| !message.is_empty()) {
        params.push(format!(
            "error_message={}",
            urlencoding::encode(message)
        ));
    }
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    Redirect::to(&format!("/admin/database{query}")).into_response()
}

async fn duplicate_view(
    State(state): State<AppState>,
    Path(source): Path<String>,
    body: Bytes,
) -> AppResult<Response> {
    let form = match parse_html_form::<ViewCreateForm>(&body) {
        Ok(form) => form,
        Err(message) => return Ok(redirect_database_tab("views", None, Some(&message))),
    };

    match database::duplicate_user_view(
        &state.pool(),
        &source,
        &form.name,
        &form.definition,
    )
    .await
    {
        Ok(()) => Ok(redirect_database_tab(
            "views",
            Some(&format!(
                "ビュー「{source}」を「{}」として複製しました",
                form.name
            )),
            None,
        )),
        Err(err) => Ok(redirect_database_tab(
            "views",
            None,
            Some(&domain_error_message(&err)),
        )),
    }
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
                    heading: "列編集",
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
            heading: "列編集",
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
    if let Err(err) = database::ensure_user_table_viewable(&state.pool(), &name).await {
        return handle_object_viewable_error(&auth, &name, err).await;
    }

    let read_only = database::is_cms_readonly_table(&name);
    let html = render_data_page(
        &auth,
        DataPageParams {
            name: name.clone(),
            data_api_url: object_admin_url(DbAdminObjectKind::Table, &name, "/data/rows"),
            read_only,
            is_view: false,
            edit_url: if read_only {
                String::new()
            } else {
                object_admin_url(DbAdminObjectKind::Table, &name, "/edit")
            },
            edit_label: "列編集",
            seed_url: if read_only {
                String::new()
            } else {
                object_admin_url(DbAdminObjectKind::Table, &name, "/data/seed")
            },
        },
    )?;

    Ok(Html(html).into_response())
}

async fn view_data(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    if let Err(err) = database::ensure_user_view_viewable(&state.pool(), &name).await {
        return handle_object_viewable_error(&auth, &name, err).await;
    }

    let can_edit = database::is_db_admin_editable(&name);
    let html = render_data_page(
        &auth,
        DataPageParams {
            name: name.clone(),
            data_api_url: object_admin_url(DbAdminObjectKind::View, &name, "/data/rows"),
            read_only: true,
            is_view: true,
            edit_url: if can_edit {
                object_admin_url(DbAdminObjectKind::View, &name, "/edit")
            } else {
                String::new()
            },
            edit_label: "定義編集",
            seed_url: String::new(),
        },
    )?;

    Ok(Html(html).into_response())
}

async fn table_data_rows(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<TableDataRowsQuery>,
) -> ApiResult<Json<TableDataRowsResponse>> {
    let sort_override = if let Some(raw) = &query.sort {
        Some(database::parse_sort_query_param(raw)?)
    } else {
        None
    };
    let filter_override = if let Some(raw) = &query.filter {
        Some(database::parse_filter_query_param(raw)?)
    } else {
        None
    };
    let sort_ref = sort_override.as_deref();
    let filter_ref = filter_override.as_deref();
    let data = database::list_user_table_rows(
        &state.pool(),
        &name,
        query.offset,
        sort_ref,
        filter_ref,
    )
    .await?;
    let shown_count = data.rows.len() as i64;
    Ok(Json(TableDataRowsResponse {
        columns: data.columns,
        rows: data.rows,
        total_count: data.total_count,
        offset: data.offset,
        shown_count,
        has_more: data.has_more,
        chunk_size: database::TABLE_DATA_CHUNK_SIZE,
        column_widths: data.column_widths,
        sort: data.sort,
        filter: data.filter,
        column_meta: data.column_meta,
    }))
}

async fn update_table_cell(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<TableCellUpdateRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let result = database::update_table_cell(&state.pool(), &name, &body).await?;
    Ok(Json(json!({ "ok": true, "value": result.value })))
}

async fn save_table_sort(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SortSaveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    database::save_table_sort(&state.pool(), &name, &body.sort).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn save_table_filter(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<FilterSaveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    database::save_table_filter(&state.pool(), &name, &body.filter).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn save_table_column_widths(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ColumnWidthsSaveRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    database::save_table_column_widths(&state.pool(), &name, &body.widths).await?;
    Ok(Json(json!({ "ok": true })))
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Bytes,
) -> AppResult<Response> {
    let columns = match database::load_user_table_columns(&state.pool(), &name).await {
        Ok(columns) => columns,
        Err(DomainError::SystemTable(message)) => {
            return Ok(seed_sse_error(message).into_response());
        }
        Err(DomainError::NotFound) => return Err(AppError::NotFound),
        Err(err) => return Ok(seed_sse_error(domain_error_message(&err)).into_response()),
    };

    let form = match parse_seed_form_body(&body) {
        Ok(form) => form,
        Err(message) => return Ok(seed_sse_error(message).into_response()),
    };

    let (count, rules) = match database::parse_seed_form(&columns, &form) {
        Ok(parsed) => parsed,
        Err(err) => return Ok(seed_sse_error(domain_error_message(&err)).into_response()),
    };

    let redirect = table_url(&name, "/data");
    Ok(seed_sse_stream(state.pool().clone(), name, count, rules, redirect).into_response())
}

fn seed_sse_event(event_type: &str, data: serde_json::Value) -> Event {
    Event::default().event(event_type).data(data.to_string())
}

fn seed_sse_error(message: impl Into<String>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let event = seed_sse_event("error", json!({ "message": message.into() }));
    Sse::new(stream::once(async move { Ok(event) }))
}

struct CancelOnDrop<S> {
    inner: S,
    cancelled: Arc<AtomicBool>,
}

impl<S> Drop for CancelOnDrop<S> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl<S: Stream + Unpin> Stream for CancelOnDrop<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.as_mut().get_mut().inner).poll_next(cx)
    }
}

fn seed_sse_stream(
    pool: sqlx::SqlitePool,
    table_name: String,
    count: u32,
    rules: Vec<(String, database::ColumnSeedRule)>,
    redirect: String,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_worker = Arc::clone(&cancelled);
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let started = Instant::now();
        let result = database::generate_test_data(
            &pool,
            &table_name,
            count,
            &rules,
            Some(|done, total| {
                let _ = tx.send(Ok(seed_sse_event(
                    "progress",
                    json!({ "done": done, "total": total }),
                )));
            }),
            Some({
                let cancelled = Arc::clone(&cancelled_worker);
                move || cancelled.load(Ordering::Relaxed)
            }),
        )
        .await;

        match result {
            Ok(generated) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                let _ = tx.send(Ok(seed_sse_event(
                    "done",
                    json!({ "count": generated, "redirect": redirect, "elapsed_ms": elapsed_ms }),
                )));
            }
            Err(err) => {
                if err.to_string().contains("キャンセル") {
                    return;
                }
                let _ = tx.send(Ok(seed_sse_event(
                    "error",
                    json!({ "message": domain_error_message(&err) }),
                )));
            }
        }
    });

    let stream = CancelOnDrop {
        inner: UnboundedReceiverStream::new(rx),
        cancelled,
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
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
                    heading: "列編集",
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
                heading: "列編集",
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
                    heading: "列編集",
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
                    heading: "列編集",
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

async fn new_view_form(auth: AuthUser, State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = render_view_form(
        &auth,
        &state.pool(),
        ViewFormParams {
            heading: "ビューを追加",
            action: "/admin/database/views/new".to_string(),
            submit_label: "追加する",
            name: "",
            name_readonly: false,
            is_edit: false,
            definition: "",
            error_message: "",
        },
    )
    .await?;
    Ok(Html(html))
}

async fn edit_view_form(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AppResult<Response> {
    match database::load_user_view_definition(&state.pool(), &name).await {
        Ok(definition) => {
            let html = render_view_form(
                &auth,
                &state.pool(),
                view_edit_form_params(&name, &definition, ""),
            )
            .await?;
            Ok(Html(html).into_response())
        }
        Err(DomainError::SystemTable(message)) => {
            Ok(system_table_notice_response(&auth, &name, &message).await?)
        }
        Err(DomainError::NotFound) => Err(AppError::NotFound),
        Err(err) => {
            let html = render_view_form(
                &auth,
                &state.pool(),
                view_edit_form_params(&name, "", &domain_error_message(&err)),
            )
            .await?;
            Ok(Html(html).into_response())
        }
    }
}

async fn create_view(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<ViewCreateForm>,
) -> AppResult<Response> {
    match database::create_user_view(&state.pool(), &form.name, &form.definition).await {
        Ok(()) => Ok(Redirect::to(&view_url(&form.name, "/data")).into_response()),
        Err(err) => {
            let html = render_view_form(
                &auth,
                &state.pool(),
                ViewFormParams {
                    heading: "ビューを追加",
                    action: "/admin/database/views/new".to_string(),
                    submit_label: "追加する",
                    name: &form.name,
                    name_readonly: false,
                    is_edit: false,
                    definition: &form.definition,
                    error_message: &domain_error_message(&err),
                },
            )
            .await?;
            Ok(Html(html).into_response())
        }
    }
}

async fn update_view(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Form(form): Form<ViewCreateForm>,
) -> AppResult<Response> {
    let submitted_name = form.name.trim();
    if submitted_name.is_empty() {
        let html = render_view_form(
            &auth,
            &state.pool(),
            view_edit_form_params(&name, &form.definition, "ビュー名は必須です"),
        )
        .await?;
        return Ok(Html(html).into_response());
    }

    match database::update_user_view(
        &state.pool(),
        &name,
        submitted_name,
        &form.definition,
    )
    .await
    {
        Ok(()) => Ok(Redirect::to(&view_url(submitted_name, "/data")).into_response()),
        Err(DomainError::SystemTable(message)) => {
            Ok(system_table_notice_response(&auth, &name, &message).await?)
        }
        Err(DomainError::NotFound) => Err(AppError::NotFound),
        Err(err) => {
            let html = render_view_form(
                &auth,
                &state.pool(),
                view_edit_form_params(
                    submitted_name,
                    &form.definition,
                    &domain_error_message(&err),
                ),
            )
            .await?;
            Ok(Html(html).into_response())
        }
    }
}

async fn table_columns_json(
    _auth: AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let columns = database::table_columns_for_ui(&state.pool(), &name).await?;
    Ok(Json(json!({ "columns": columns })))
}

fn object_admin_url(kind: DbAdminObjectKind, name: &str, suffix: &str) -> String {
    let segment = match kind {
        DbAdminObjectKind::Table => "tables",
        DbAdminObjectKind::View => "views",
    };
    format!(
        "/admin/database/{segment}/{}{}",
        urlencoding::encode(name),
        suffix
    )
}

fn table_url(name: &str, suffix: &str) -> String {
    object_admin_url(DbAdminObjectKind::Table, name, suffix)
}

fn view_url(name: &str, suffix: &str) -> String {
    object_admin_url(DbAdminObjectKind::View, name, suffix)
}

fn list_action_urls(kind: DbAdminObjectKind, name: &str) -> ListActionUrls {
    let can_edit = database::is_db_admin_editable(name);
    let can_view_data = database::is_db_admin_data_viewable(name);
    ListActionUrls {
        can_edit,
        can_view_data,
        edit_url: if can_edit {
            object_admin_url(kind, name, "/edit")
        } else {
            String::new()
        },
        data_url: if can_view_data {
            object_admin_url(kind, name, "/data")
        } else {
            String::new()
        },
    }
}

fn table_list_item(item: DbObjectItem) -> TableListItem {
    let actions = list_action_urls(DbAdminObjectKind::Table, &item.name);
    TableListItem {
        name: item.name,
        row_count: item.row_count,
        is_system: item.is_system,
        can_edit: actions.can_edit,
        can_view_data: actions.can_view_data,
        edit_url: actions.edit_url,
        data_url: actions.data_url,
    }
}

fn view_list_item(item: DbObjectItem) -> ViewListItem {
    let actions = list_action_urls(DbAdminObjectKind::View, &item.name);
    ViewListItem {
        name: item.name,
        sql_preview: item.sql_preview,
        is_system: item.is_system,
        can_edit: actions.can_edit,
        can_view_data: actions.can_view_data,
        edit_url: actions.edit_url,
        data_url: actions.data_url,
    }
}

fn view_edit_form_params<'a>(
    name: &'a str,
    definition: &'a str,
    error_message: &'a str,
) -> ViewFormParams<'a> {
    ViewFormParams {
        heading: "ビューを編集",
        action: view_url(name, "/edit"),
        submit_label: "保存する",
        name,
        name_readonly: false,
        is_edit: true,
        definition,
        error_message,
    }
}

async fn handle_object_viewable_error(
    auth: &AuthUser,
    name: &str,
    err: DomainError,
) -> AppResult<Response> {
    match err {
        DomainError::SystemTable(message) => {
            Ok(system_table_notice_response(auth, name, &message).await?)
        }
        DomainError::NotFound => Err(AppError::NotFound),
        other => Err(other.into()),
    }
}

fn render_data_page(auth: &AuthUser, params: DataPageParams) -> AppResult<String> {
    Ok(TableDataTemplate {
        layout: breadcrumb::with(
            layout::AdminLayoutCtx::new(auth),
            breadcrumb::database_table_data(&params.name, params.read_only),
        ),
        table_name: params.name,
        data_api_url: params.data_api_url,
        read_only: params.read_only,
        is_view: params.is_view,
        edit_url: params.edit_url,
        edit_label: params.edit_label,
        seed_url: params.seed_url,
    }
    .render()?)
}

fn parse_seed_form_body(body: &Bytes) -> Result<TestDataSeedForm, String> {
    let body = std::str::from_utf8(body).map_err(|_| "フォームデータの形式が不正です".to_string())?;
    serde_html_form::from_str(body).map_err(|err| format!("フォームデータの解析に失敗しました: {err}"))
}

fn table_seed_form_template(
    auth: &AuthUser,
    table_name: &str,
    columns: &[SeedFormRow],
    count: String,
    error_message: &str,
) -> AppResult<String> {
    let data_url = table_url(table_name, "/data");
    Ok(TableSeedFormTemplate {
        layout: breadcrumb::with(
            layout::AdminLayoutCtx::new(auth),
            breadcrumb::database_table_seed(table_name, &data_url),
        ),
        table_name: table_name.to_string(),
        action: table_url(table_name, "/data/seed"),
        data_url,
        count,
        max_count: database::MAX_TEST_DATA_ROWS,
        has_columns: !columns.is_empty(),
        columns: columns.to_vec(),
        error_message: error_message.to_string(),
    }
    .render()?)
}

fn parse_table_create_form(body: &Bytes) -> Result<TableCreateForm, String> {
    parse_html_form(body)
}

fn parse_html_form<T>(body: &Bytes) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
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
    let breadcrumbs = if params.is_edit {
        breadcrumb::database_table_edit(params.name, &data_url)
    } else {
        breadcrumb::database_table_new()
    };
    Ok(TableFormTemplate {
        layout: breadcrumb::with(layout::AdminLayoutCtx::new(auth), breadcrumbs),
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
    }
    .render()?)
}

async fn render_view_form(
    auth: &AuthUser,
    pool: &sqlx::SqlitePool,
    params: ViewFormParams<'_>,
) -> AppResult<String> {
    let table_names = database::list_view_source_tables(pool).await?;
    let table_options = table_names
        .into_iter()
        .map(|name| ViewSourceTableOption { name })
        .collect::<Vec<_>>();
    let ui_builder_json = build_view_ui_builder_json(pool, params.definition).await;

    let data_url = if params.is_edit {
        view_url(params.name, "/data")
    } else {
        String::new()
    };
    let breadcrumbs = if params.is_edit {
        breadcrumb::database_view_edit(params.name, &data_url)
    } else {
        breadcrumb::database_view_new()
    };
    Ok(ViewFormTemplate {
        layout: breadcrumb::with(layout::AdminLayoutCtx::new(auth), breadcrumbs),
        heading: params.heading.to_string(),
        action: params.action,
        cancel_url: "/admin/database?tab=views",
        submit_label: params.submit_label.to_string(),
        name: params.name.to_string(),
        name_readonly: params.name_readonly,
        definition: params.definition.to_string(),
        error_message: params.error_message.to_string(),
        table_options,
        ui_builder_json,
    }
    .render()?)
}

async fn build_view_ui_builder_json(pool: &sqlx::SqlitePool, definition: &str) -> String {
    let Some(parsed) = database::parse_simple_view_select(definition) else {
        return "null".to_string();
    };
    let Ok(table_columns) = database::table_columns_for_ui(pool, &parsed.base_table).await else {
        return "null".to_string();
    };
    let spec = database::build_simple_view_ui_spec(&parsed, &table_columns);
    serde_json::to_string(&spec).unwrap_or_else(|_| "null".to_string())
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
        layout: breadcrumb::with(
            layout::AdminLayoutCtx::new(auth),
            breadcrumb::database_table_notice(table_name),
        ),
        table_name: table_name.to_string(),
        message: message.to_string(),
    }
    .render()?;
    Ok(Html(html).into_response())
}
