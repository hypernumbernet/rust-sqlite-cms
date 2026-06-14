mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn assert_system_table_row_hidden(html: &str, table: &str) {
    let needle = format!(">{table}</a>");
    assert!(html.contains(&needle), "missing table row: {table}");
    let idx = html.find(&needle).expect("table row");
    let tr_start = html[..idx].rfind("<tr").expect("tr start");
    let tr_tag = &html[tr_start..idx];
    assert!(
        tr_tag.contains("db-table-row-system"),
        "table {table} is not marked as system row"
    );
    assert!(
        tr_tag.contains("hidden"),
        "table {table} system row is not hidden by default"
    );
}

fn parse_sse_events(body: &str) -> Vec<(String, serde_json::Value)> {
    let mut events = Vec::new();
    for block in body.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut event_name = "message".to_string();
        let mut data = String::new();
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_name = value.trim().to_string();
            } else if let Some(value) = line.strip_prefix("data:") {
                data = value.trim().to_string();
            }
        }
        if !data.is_empty() {
            let payload = serde_json::from_str(&data).unwrap_or(serde_json::Value::Null);
            events.push((event_name, payload));
        }
    }
    events
}

#[tokio::test]
async fn database_index_lists_cms_tables() {
    let app = common::TestApp::new().await;

    let response = app.admin_request("GET", "/admin/database", None, None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("DB管理"));
    assert!(html.contains("テーブル"));
    assert!(html.contains("ビュー"));
    assert!(html.contains("システムテーブル"));
    assert!(html.contains("db-show-system-tables"));
    assert!(!html.contains(r#"id="db-show-system-tables" checked"#));
    assert!(html.contains(r#"id="db-tables-empty""#));
    assert!(!html.contains(r#"id="db-tables-empty" hidden"#));
    assert!(!html.contains("_sqlx_migrations"));

    for table in [
        "widget_types",
        "placeholders",
        "posts",
        "postmeta",
        "options",
        "layouts",
        "pages",
        "users",
        "user_table_meta",
    ] {
        assert_system_table_row_hidden(&html, table);
    }
    let meta_row = html.split("user_table_meta</a>").nth(1).unwrap_or("");
    assert!(meta_row.contains("システム"));
    assert!(!meta_row.contains("列編集"));
    assert!(meta_row.contains("データ"));
    assert!(html.contains("システム"));
    assert!(!html.contains("リードオンリー"));
    assert!(html.contains(
        r#"<a class="text-mono" href="/admin/database/tables/user_table_meta/data">user_table_meta</a>"#
    ));
    let posts_row = html.split("posts</a>").nth(1).unwrap_or("");
    assert!(posts_row.contains("システム"));
}

#[tokio::test]
async fn database_views_tab_shows_empty_state() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request("GET", "/admin/database?tab=views", None, None)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("ビューがありません"));
}

#[tokio::test]
async fn database_table_form_renders_column_builder() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request("GET", "/admin/database/tables/new", None, None)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("id INTEGER PRIMARY KEY"));
    assert!(html.contains("カラムを追加"));
    assert!(html.contains("col_name"));
    assert!(html.contains("col_type"));
    assert!(html.contains("col_nullable"));
    assert!(html.contains("整数"));
    assert!(html.contains("文字列"));
    assert!(html.contains("日時"));
    assert!(html.contains("真偽値"));
}

#[tokio::test]
async fn database_create_user_table_and_view() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=custom_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app.admin_request("GET", "/admin/database", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("custom_notes"));
    assert!(html.contains(
        r#"<a class="text-mono" href="/admin/database/tables/custom_notes/data">custom_notes</a>"#
    ));
    let custom_row = html.split("custom_notes</a>").nth(1).unwrap_or("");
    assert!(custom_row.contains("ユーザー"));
    assert!(html.contains(r#"/admin/database/tables/custom_notes/edit"#));
    assert!(html.contains(r#"/admin/database/tables/custom_notes/data"#));
    assert!(custom_row.contains("列編集"));
    assert!(custom_row.contains("データ"));
    assert!(html.contains(r#"id="db-tables-empty" hidden"#));
    assert_system_table_row_hidden(&html, "posts");

    let posts_row = html.split("posts</a>").nth(1).unwrap_or("");
    assert!(!posts_row.contains(r#"/admin/database/tables/posts/edit"#));
    assert!(posts_row.contains(r#"/admin/database/tables/posts/data"#));
    assert!(!posts_row.contains("列編集"));
    assert!(posts_row.contains("データ"));
    let users_row = html.split("users</a>").nth(1).unwrap_or("");
    assert!(!users_row.contains(r#"/admin/database/tables/users/edit"#));
    assert!(users_row.contains(r#"/admin/database/tables/users/data"#));
    assert!(users_row.contains("システム"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some("name=custom_notes_view&definition=SELECT+id%2C+body+FROM+custom_notes"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request("GET", "/admin/database?tab=views", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("custom_notes_view"));
    assert!(html.contains(
        r#"<a class="text-mono" href="/admin/database/views/custom_notes_view/data">custom_notes_view</a>"#
    ));
    assert!(html.contains("SELECT id, body FROM custom_notes"));
    let view_row = html.split("custom_notes_view</a>").nth(1).unwrap_or("");
    assert!(view_row.contains("データ"));
    assert!(view_row.contains("定義編集"));
    assert!(html.contains(r#"/admin/database/views/custom_notes_view/data"#));
    assert!(html.contains(r#"/admin/database/views/custom_notes_view/edit"#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/custom_notes_view/data",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("db-table-data-panel"));
    assert!(html.contains("定義編集"));
    assert!(html.contains("ビュー一覧"));
    assert!(html.contains(r#"/admin/database?tab=views"#));
    assert!(html.contains(r#"/admin/database/views/custom_notes_view/data/rows"#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/custom_notes_view/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["id", "body"]));
    assert_eq!(json["rows"].as_array().unwrap().len(), 0);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/custom_notes_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ビューを編集"));
    assert!(html.contains("定義編集"));
    assert!(html.contains(
        r#"<a href="/admin/database/views/custom_notes_view/data">custom_notes_view データ</a>"#
    ));
    assert!(html.contains("SELECT id, body FROM custom_notes"));
    assert!(html.contains(r#"name="name" type="text" maxlength="120" value="custom_notes_view""#));
    assert!(!html.contains(r#"value="custom_notes_view" readonly"#));
    assert!(html.contains("view-form-tabs"));
    assert!(html.contains("view-tab-ui"));
    assert!(html.contains("ビジュアル編集"));
    assert!(!html.contains("UI編集"));
    assert!(html.contains(r#"class="active" data-view-tab="ui""#));
    assert!(html.contains(r#"id="view-tab-ui""#));
    assert!(html.contains(r#"id="view-tab-sql" hidden"#));
    assert!(html.contains(r#""base_table":"custom_notes""#));
    assert!(html.contains(r#""name":"body""#));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/custom_notes_view/edit",
            Some("name=custom_notes_view&definition=SELECT+body+FROM+custom_notes"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request("GET", "/admin/database?tab=views", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("SELECT body FROM custom_notes"));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/custom_notes_view/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["body"]));
    assert_eq!(json["rows"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn database_view_form_ui_builder() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=custom_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request("GET", "/admin/database/views/new", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("view-form-tabs"));
    assert!(html.contains(r#"data-view-tab="sql""#));
    assert!(html.contains(r#"data-view-tab="ui""#));
    assert!(html.contains("ビジュアル編集"));
    assert!(!html.contains("UI編集"));
    assert!(html.contains(r#"class="active" data-view-tab="ui""#));
    assert!(html.contains(r#"id="view-tab-ui""#));
    assert!(html.contains(r#"id="view-tab-sql" hidden"#));
    assert!(html.contains("view-ui-base-table"));
    assert!(html.contains(r#"<option value="custom_notes">custom_notes</option>"#));
    assert!(html.contains(r#"<script type="application/json" id="view-ui-initial">null</script>"#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/custom_notes/columns",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"][0]["name"], "id");
    assert_eq!(json["columns"][1]["name"], "body");

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some("name=ui_notes_view&definition=SELECT+%22body%22+FROM+%22custom_notes%22"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/ui_notes_view/edit",
            Some("name=ui_notes_view&definition=SELECT+%22id%22%2C+%22body%22+FROM+%22custom_notes%22"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/ui_notes_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("SELECT"));
    assert!(html.contains("custom_notes"));
    assert!(html.contains(r#""base_table":"custom_notes""#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/ui_notes_view/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["id", "body"]));
}

#[tokio::test]
async fn database_view_form_unsupported_visual_editing_notice() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=join_src&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=join_other&col_name=label&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some("name=join_view&definition=SELECT+%22id%22+FROM+%22join_src%22+JOIN+%22join_other%22+ON+1+%3D+1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/join_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ビジュアル編集"));
    assert!(html.contains(r#"class="active" data-view-tab="ui""#));
    assert!(html.contains("この定義はビジュアル編集に対応していません"));
    assert!(html.contains(r#"<script type="application/json" id="view-ui-initial">null</script>"#));
}

#[tokio::test]
async fn database_view_form_where_condition() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=where_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some("name=where_notes_view&definition=SELECT+%22id%22%2C+%22body%22+FROM+%22where_notes%22+WHERE+%22body%22+IS+NOT+NULL"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/where_notes_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#""base_table":"where_notes""#));
    assert!(html.contains(r#""where_condition":"IS NOT NULL""#));
    assert!(html.contains("view-ui-add-column-btn"));
    assert!(html.contains("view-ui-add-column-select"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/where_notes_view/edit",
            Some("name=where_notes_view&definition=SELECT+%22body%22+FROM+%22where_notes%22+WHERE+%22body%22+IS+NOT+NULL"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = 'where_notes_view'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(sql.contains(r#"SELECT "body" FROM "where_notes""#));
    assert!(sql.contains(r#"WHERE "body" IS NOT NULL"#));
}

#[tokio::test]
async fn database_view_form_column_alias() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=alias_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(
                "name=alias_notes_view&definition=SELECT+%22id%22+AS+%22user_id%22%2C+%22body%22+FROM+%22alias_notes%22",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/alias_notes_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#""base_table":"alias_notes""#));
    assert!(html.contains(r#""name":"id""#));
    assert!(html.contains(r#""alias":"user_id""#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/alias_notes_view/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["user_id", "body"]));
}

#[tokio::test]
async fn database_view_form_expression_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=expr_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(
                "name=expr_notes_view&definition=SELECT+LENGTH(%22body%22)+AS+%22body_len%22%2C+%22id%22+FROM+%22expr_notes%22",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/expr_notes_view/edit",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#""base_table":"expr_notes""#));
    assert!(html.contains(r#""expression":"LENGTH(\"body\")""#));
    assert!(html.contains(r#""alias":"body_len""#));
    assert!(html.contains("view-ui-add-column-expression"));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/expr_notes_view/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["body_len", "id"]));
}

async fn setup_expr_recover_view(
    app: &common::TestApp,
    table_name: &str,
    view_name: &str,
) -> String {
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(&format!(
                "name={table_name}&col_name=body&col_type=text&col_nullable=0"
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let valid_definition = format!(r#"SELECT LENGTH("body") AS "body_len" FROM "{table_name}""#);
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(&format!(
                "name={view_name}&definition={}",
                urlencoding::encode(&valid_definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    valid_definition
}

async fn post_view_edit_definition(
    app: &common::TestApp,
    view_name: &str,
    definition: &str,
) -> (StatusCode, String) {
    let response = app
        .admin_request(
            "POST",
            &format!("/admin/database/views/{view_name}/edit"),
            Some(&format!(
                "name={view_name}&definition={}",
                urlencoding::encode(definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn assert_view_edit_recovers_after_error(
    app: &common::TestApp,
    table_name: &str,
    view_name: &str,
    invalid_definition: &str,
    invalid_expression_json: &str,
    assert_invalid_error: impl Fn(&str),
    assert_view_unchanged: bool,
) {
    let valid_definition = setup_expr_recover_view(app, table_name, view_name).await;

    let (status, html) = post_view_edit_definition(app, view_name, invalid_definition).await;
    assert_eq!(status, StatusCode::OK);
    assert_invalid_error(&html);
    assert!(html.contains(invalid_expression_json));
    assert!(html.contains(&format!(r#""base_table":"{table_name}""#)));

    if assert_view_unchanged {
        let pool = app.state.pool();
        let view_sql: Option<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = ?",
        )
        .bind(view_name)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(
            view_sql.as_deref().is_some_and(|sql| {
                sql.contains("LENGTH(\"body\")") && !sql.contains("LEN(\"body\")")
            }),
            "invalid update must not persist broken view definition: {view_sql:?}"
        );
    }

    let (status, _) = post_view_edit_definition(app, view_name, &valid_definition).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            &format!("/admin/database/views/{view_name}/data/rows"),
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["body_len"]));
}

#[tokio::test]
async fn database_view_edit_recovers_after_invalid_expression_error() {
    let app = common::TestApp::new().await;
    assert_view_edit_recovers_after_error(
        &app,
        "expr_recover_notes",
        "expr_recover_view",
        r#"SELECT DROP("body") AS "body_len" FROM "expr_recover_notes""#,
        r#""expression":"DROP(\"body\")""#,
        |html| {
            assert!(html.contains("使用できない SQL キーワード"));
            assert!(html.contains("DROP"));
        },
        false,
    )
    .await;
}

#[tokio::test]
async fn database_view_edit_recovers_after_sqlite_invalid_function_error() {
    let app = common::TestApp::new().await;
    assert_view_edit_recovers_after_error(
        &app,
        "len_recover_notes",
        "len_recover_view",
        r#"SELECT LEN("body") AS "body_len" FROM "len_recover_notes""#,
        r#""expression":"LEN(\"body\")""#,
        |html| {
            assert!(
                html.contains("no such function") || html.contains("LEN"),
                "expected SQLite function error in response"
            );
        },
        true,
    )
    .await;
}

#[tokio::test]
async fn database_view_data_survives_definition_change_with_stale_sort() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=stale_sort_src&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "stale_sort_src" ("body") VALUES ('alpha'), ('beta')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(
                "name=stale_sort_view&definition=SELECT+%22id%22%2C+%22body%22+FROM+%22stale_sort_src%22",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let save_body = r#"{"sort":[{"column":"body","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/stale_sort_view/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/stale_sort_view/edit",
            Some("name=stale_sort_view&definition=SELECT+%22id%22+AS+%22user_id%22+FROM+%22stale_sort_src%22"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'stale_sort_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(stored_sort.as_deref(), Some("[]"));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/views/stale_sort_view/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["user_id"]));
    assert_eq!(json["rows"].as_array().unwrap().len(), 2);
    assert!(json["sort"].as_array().unwrap().is_empty());
}

async fn setup_rename_view_src(app: &common::TestApp, table_name: &str, view_name: &str) {
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(&format!(
                "name={table_name}&col_name=body&col_type=text&col_nullable=0"
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"INSERT INTO "{table_name}" ("body") VALUES ('alpha'), ('beta')"#
    )))
    .execute(&app.state.pool())
    .await
    .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(&format!(
                "name={view_name}&definition=SELECT+%22id%22%2C+%22body%22+FROM+%22{table_name}%22"
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn database_view_rename_preserves_ui_meta() {
    let app = common::TestApp::new().await;
    setup_rename_view_src(&app, "rename_meta_src", "rename_meta_view").await;

    let save_body = r#"{"sort":[{"column":"body","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_meta_view/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let definition = "SELECT \"id\", \"body\" FROM \"rename_meta_src\"";
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_meta_view/edit",
            Some(&format!(
                "name=renamed_meta_view&definition={}",
                urlencoding::encode(definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let old_meta: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'rename_meta_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(old_meta.is_none());

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'renamed_meta_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored_sort.unwrap().contains(r#""column":"body""#));

    let view_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'view' AND name = 'renamed_meta_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(view_exists.as_deref(), Some("renamed_meta_view"));

    let old_view: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'view' AND name = 'rename_meta_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(old_view.is_none());
}

#[tokio::test]
async fn database_view_rename_redirects_to_new_data_url() {
    let app = common::TestApp::new().await;
    setup_rename_view_src(&app, "rename_redirect_src", "rename_redirect_view").await;

    let definition = "SELECT \"id\", \"body\" FROM \"rename_redirect_src\"";
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_redirect_view/edit",
            Some(&format!(
                "name=renamed_redirect_view&definition={}",
                urlencoding::encode(definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/admin/database/views/renamed_redirect_view/data")
    );
}

#[tokio::test]
async fn database_view_rename_rejects_conflicting_name() {
    let app = common::TestApp::new().await;
    setup_rename_view_src(&app, "rename_conflict_src", "rename_conflict_view").await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some(
                "name=rename_conflict_existing&definition=SELECT+%22id%22+FROM+%22rename_conflict_src%22",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let definition = "SELECT \"id\", \"body\" FROM \"rename_conflict_src\"";
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_conflict_view/edit",
            Some(&format!(
                "name=rename_conflict_existing&definition={}",
                urlencoding::encode(definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("既に存在します"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_conflict_view/edit",
            Some(&format!(
                "name=rename_conflict_src&definition={}",
                urlencoding::encode(definition)
            )),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("既に存在します"));

    let view_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'view' AND name = 'rename_conflict_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(view_exists.as_deref(), Some("rename_conflict_view"));
}

#[tokio::test]
async fn database_view_rename_with_definition_change_clears_stale_sort() {
    let app = common::TestApp::new().await;
    setup_rename_view_src(&app, "rename_def_src", "rename_def_view").await;

    let save_body = r#"{"sort":[{"column":"body","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_def_view/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_def_view/edit",
            Some("name=renamed_def_view&definition=SELECT+%22id%22+AS+%22user_id%22+FROM+%22rename_def_src%22"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'renamed_def_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(stored_sort.as_deref(), Some("[]"));
}

#[tokio::test]
async fn database_view_rename_rollback_on_invalid_definition() {
    let app = common::TestApp::new().await;
    setup_rename_view_src(&app, "rename_rollback_src", "rename_rollback_view").await;

    let save_body = r#"{"sort":[{"column":"body","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_rollback_view/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/rename_rollback_view/edit",
            Some("name=renamed_rollback_view&definition=SELECT+DROP(%22body%22)+FROM+%22rename_rollback_src%22"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("使用できない SQL キーワード"));

    let view_exists: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'view' AND name = 'rename_rollback_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(view_exists.as_deref(), Some("rename_rollback_view"));

    let renamed_view: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'view' AND name = 'renamed_rollback_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(renamed_view.is_none());

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'rename_rollback_view'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored_sort.unwrap().contains(r#""column":"body""#));
}

#[tokio::test]
async fn database_table_data_survives_column_rename_with_stale_sort() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=rename_sort_src&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "rename_sort_src" ("body") VALUES ('alpha'), ('beta')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"sort":[{"column":"body","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/rename_sort_src/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/rename_sort_src/edit",
            Some(
                "name=rename_sort_src&col_orig_name=body&col_name=title&col_type=text&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'rename_sort_src'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored_sort.unwrap().contains(r#""column":"title""#));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/rename_sort_src/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["id", "title"]));
    assert_eq!(json["sort"][0]["column"], "title");
    assert_eq!(
        json["rows"].as_array().unwrap()[0][1],
        serde_json::json!("alpha")
    );
}

#[tokio::test]
async fn database_table_data_rows_ignore_invalid_sort_query_param() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=bad_sort_query&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "bad_sort_query" ("body") VALUES ('only')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/bad_sort_query/data/rows?offset=0&sort=missing:asc",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["rows"].as_array().unwrap().len(), 1);
    assert!(json["sort"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn database_create_table_with_sqlite_types() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=typed_events&col_name=created_at&col_type=timestamp&col_nullable=0&col_name=active&col_type=boolean&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'typed_events'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();

    assert!(definition.contains("id INTEGER PRIMARY KEY"));
    assert!(definition.contains(r#""created_at" TIMESTAMP NOT NULL"#));
    assert!(definition.contains(r#""active" BOOLEAN NOT NULL"#));
}

#[tokio::test]
async fn database_create_table_with_multilingual_names() {
    let app = common::TestApp::new().await;
    let table_name = "記事";
    let column_name = "タイトル";
    let encoded_table = urlencoding::encode(table_name);

    let body = format!(
        "name={}&col_name={}&col_type=text&col_nullable=0",
        urlencoding::encode(table_name),
        urlencoding::encode(column_name),
    );

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(&body),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table_name)
    .fetch_one(&app.state.pool())
    .await
    .unwrap();

    assert!(definition.contains(r#""記事""#));
    assert!(definition.contains(r#""タイトル" TEXT NOT NULL"#));

    let response = app.admin_request("GET", "/admin/database", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(table_name));
    assert!(html.contains(&format!(
        "/admin/database/tables/{encoded_table}/edit"
    )));

    let response = app
        .admin_request(
            "GET",
            &format!("/admin/database/tables/{encoded_table}/data"),
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(table_name));
    assert!(html.contains("データがありません"));

    let response = app
        .admin_request(
            "GET",
            &format!("/admin/database/tables/{encoded_table}/edit"),
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(&format!("value=\"{column_name}\"")));
}

#[tokio::test]
async fn database_edit_user_table_updates_columns() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=edit_me&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request("GET", "/admin/database/tables/edit_me/edit", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("列編集"));
    assert!(html.contains("value=\"body\""));
    assert!(html.contains("保存する"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/edit_me/edit",
            Some("name=edit_me&col_orig_name=body&col_name=title&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("列定義を保存しました"));
    assert!(html.contains("列編集"));

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'edit_me'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(definition.contains(r#""title" TEXT NOT NULL"#));
    assert!(!definition.contains("body"));
}

#[tokio::test]
async fn database_edit_user_table_preserves_rows_when_adding_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=keep_rows&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "keep_rows" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/keep_rows/edit",
            Some(
                "name=keep_rows&col_orig_name=body&col_name=body&col_type=text&col_nullable=0&col_name=memo&col_type=text&col_nullable=1",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let value: String = sqlx::query_scalar(r#"SELECT "body" FROM "keep_rows" WHERE id = 1"#)
        .fetch_one(&app.state.pool())
        .await
        .unwrap();
    assert_eq!(value, "hello");
}

#[tokio::test]
async fn database_edit_user_table_preserves_rows_when_renaming_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=rename_rows&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "rename_rows" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/rename_rows/edit",
            Some("name=rename_rows&col_orig_name=body&col_name=title&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let value: String = sqlx::query_scalar(r#"SELECT "title" FROM "rename_rows" WHERE id = 1"#)
        .fetch_one(&app.state.pool())
        .await
        .unwrap();
    assert_eq!(value, "hello");
}

#[tokio::test]
async fn database_edit_user_table_rejects_type_change() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=type_lock&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/type_lock/edit",
            Some("name=type_lock&col_orig_name=body&col_name=body&col_type=integer&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("型は変更できません"));

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'type_lock'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(definition.contains(r#""body" TEXT NOT NULL"#));
}

#[tokio::test]
async fn database_edit_user_table_relaxes_not_null_to_nullable() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=null_relax&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "null_relax" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/null_relax/edit",
            Some("name=null_relax&col_orig_name=body&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("列定義を保存しました"));

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'null_relax'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(definition.contains(r#""body" TEXT"#));
    assert!(!definition.contains(r#""body" TEXT NOT NULL"#));

    let value: String = sqlx::query_scalar(r#"SELECT "body" FROM "null_relax" WHERE id = 1"#)
        .fetch_one(&app.state.pool())
        .await
        .unwrap();
    assert_eq!(value, "hello");
}

#[tokio::test]
async fn database_edit_user_table_rejects_nullable_to_not_null() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=null_tighten&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/null_tighten/edit",
            Some("name=null_tighten&col_orig_name=body&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("NOT NULL に変更することはできません"));
}

#[tokio::test]
async fn database_edit_user_table_rejects_not_null_add_with_existing_rows() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=not_null_add&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "not_null_add" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/not_null_add/edit",
            Some(
                "name=not_null_add&col_orig_name=body&col_name=body&col_type=text&col_nullable=0&col_name=code&col_type=text&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("NOT NULL な列を追加できません"));
}

#[tokio::test]
async fn database_table_data_lists_rows() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=data_rows&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "data_rows" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request("GET", "/admin/database/tables/data_rows/data", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("data_rows"));
    assert!(html.contains(r#"data-api-url="/admin/database/tables/data_rows/data/rows""#));
    assert!(html.contains("db-data-status"));
    assert!(html.contains("db-table-data-panel"));
    assert!(html.contains("db-data-row-goto"));
    assert!(html.contains("db-row-goto-dialog"));
    assert!(html.contains("db-cell-edit-dialog"));
    assert!(html.contains("db-sorted-nav-confirm-dialog"));
    assert!(html.contains("db-sorted-nav-confirm-message"));
    assert!(html.contains(r#"data-read-only="false""#));
    assert!(html.contains("行 —"));
    assert!(html.contains("列編集"));
    assert!(html.contains("テストデータ生成"));
    assert!(!html.contains("ビュー一覧"));
    assert!(html.contains("db-data-fit-all-columns"));
    assert!(html.contains("全列幅調整"));
    assert!(html.contains(r#"/admin/database/tables/data_rows/data/seed"#));
    assert!(!html.contains("hello"));
}

#[tokio::test]
async fn database_table_data_rows_api_lists_rows() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=data_rows_api&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "data_rows_api" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/data_rows_api/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["columns"], serde_json::json!(["id", "body"]));
    assert_eq!(json["rows"], serde_json::json!([["1", "hello"]]));
    assert_eq!(json["column_meta"][0]["name"], "id");
    assert_eq!(json["column_meta"][0]["pk"], true);
    assert_eq!(json["column_meta"][1]["name"], "body");
    assert_eq!(json["column_meta"][1]["pk"], false);
    assert_eq!(json["total_count"], 1);
    assert_eq!(json["offset"], 0);
    assert_eq!(json["shown_count"], 1);
    assert_eq!(json["has_more"], false);
    assert_eq!(json["chunk_size"], 1000);
}

#[tokio::test]
async fn database_table_data_rows_api_serializes_null_values() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=null_rows_api&col_name=body&col_type=text&col_nullable=1&col_name=score&col_type=integer&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "null_rows_api" ("body", "score") VALUES ('', NULL)"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/null_rows_api/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["rows"], serde_json::json!([["1", "", null]]));
}

#[tokio::test]
async fn database_table_data_rows_includes_column_widths() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=col_widths&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let save_body = r#"{"widths":{"id":80,"body":200}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/col_widths/data/column-widths",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/col_widths/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["column_widths"]["id"], 80);
    assert_eq!(json["column_widths"]["body"], 200);
}

#[tokio::test]
async fn database_table_data_column_widths_save_and_load() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=col_widths_save&col_name=title&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let save_body = r#"{"widths":{"id":72,"title":240}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/col_widths_save/data/column-widths",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT column_widths_json FROM user_table_meta WHERE table_name = 'col_widths_save'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored.is_some());
    let parsed: serde_json::Value = serde_json::from_str(stored.unwrap().as_str()).unwrap();
    assert_eq!(parsed["id"], 72);
    assert_eq!(parsed["title"], 240);
}

#[tokio::test]
async fn database_table_data_column_widths_ignores_invalid_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=col_widths_bad&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let save_body = r#"{"widths":{"missing_col":100,"body":120}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/col_widths_bad/data/column-widths",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored: String = sqlx::query_scalar(
        "SELECT column_widths_json FROM user_table_meta WHERE table_name = 'col_widths_bad'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(stored.contains(r#""body":120"#));
    assert!(!stored.contains("missing_col"));
}

#[tokio::test]
async fn database_table_data_rows_sorts_by_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=sort_rows&col_name=label&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    for label in ["charlie", "alpha", "bravo"] {
        sqlx::query(r#"INSERT INTO "sort_rows" ("label") VALUES (?)"#)
            .bind(label)
            .execute(&app.state.pool())
            .await
            .unwrap();
    }

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/sort_rows/data/rows?offset=0&sort=label:asc",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["rows"].as_array().unwrap()[0][1],
        serde_json::json!("alpha")
    );
    assert_eq!(
        json["rows"].as_array().unwrap()[2][1],
        serde_json::json!("charlie")
    );
    assert_eq!(json["sort"][0]["column"], "label");
    assert_eq!(json["sort"][0]["direction"], "asc");
}

#[tokio::test]
async fn database_table_data_rows_multi_column_sort() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=multi_sort&col_name=group_name&col_type=text&col_nullable=0\
                 &col_name=score&col_type=integer&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let rows = [
        ("a", 2),
        ("a", 1),
        ("b", 1),
    ];
    for (group_name, score) in rows {
        sqlx::query(r#"INSERT INTO "multi_sort" ("group_name", "score") VALUES (?, ?)"#)
            .bind(group_name)
            .bind(score)
            .execute(&app.state.pool())
            .await
            .unwrap();
    }

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/multi_sort/data/rows?offset=0&sort=group_name:asc,score:desc",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let data_rows = json["rows"].as_array().unwrap();

    assert_eq!(data_rows[0][1], serde_json::json!("a"));
    assert_eq!(data_rows[0][2], serde_json::json!("2"));
    assert_eq!(data_rows[1][1], serde_json::json!("a"));
    assert_eq!(data_rows[1][2], serde_json::json!("1"));
    assert_eq!(data_rows[2][1], serde_json::json!("b"));
}

#[tokio::test]
async fn database_table_data_rows_ignore_invalid_filter_query_param() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=bad_filter_query&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "bad_filter_query" ("body") VALUES ('only')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/bad_filter_query/data/rows?offset=0&filter=missing:abc",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["rows"].as_array().unwrap().len(), 1);
    assert!(json["filter"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn database_table_data_sort_save_and_load() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=sort_save&col_name=title&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "sort_save" ("title") VALUES ('z'), ('a')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"sort":[{"column":"title","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/sort_save/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/sort_save/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sort"][0]["column"], "title");
    assert_eq!(json["sort"][0]["direction"], "asc");
    assert_eq!(
        json["rows"].as_array().unwrap()[0][1],
        serde_json::json!("a")
    );

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'sort_save'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored.unwrap().contains("title"));
}

#[tokio::test]
async fn database_table_data_rows_empty_sort_uses_default_order() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=sort_default&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "sort_default" ("body") VALUES ('b'), ('a')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"sort":[{"column":"body","direction":"desc"}]}"#;
    app.admin_request(
        "POST",
        "/admin/database/tables/sort_default/data/sort",
        Some(save_body),
        Some("application/json"),
    )
    .await;

    let clear_body = r#"{"sort":[]}"#;
    app.admin_request(
        "POST",
        "/admin/database/tables/sort_default/data/sort",
        Some(clear_body),
        Some("application/json"),
    )
    .await;

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/sort_default/data/rows?offset=0&sort=",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sort"], serde_json::json!([]));
    assert_eq!(
        json["rows"].as_array().unwrap()[0][0],
        serde_json::json!("1")
    );
    assert_eq!(
        json["rows"].as_array().unwrap()[1][0],
        serde_json::json!("2")
    );
}

#[tokio::test]
async fn database_table_data_rows_filters_by_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=filter_rows&col_name=label&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    for label in ["alpha", "beta", "alphabet"] {
        sqlx::query(r#"INSERT INTO "filter_rows" ("label") VALUES (?)"#)
            .bind(label)
            .execute(&app.state.pool())
            .await
            .unwrap();
    }

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/filter_rows/data/rows?offset=0&filter=label:ta",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["total_count"], 1);
    assert_eq!(
        json["rows"].as_array().unwrap()[0][1],
        serde_json::json!("beta")
    );
    assert_eq!(json["filter"][0]["column"], "label");
    assert_eq!(json["filter"][0]["text"], "ta");
}

#[tokio::test]
async fn database_table_data_rows_multi_column_filter() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=multi_filter&col_name=group_name&col_type=text&col_nullable=0\
                 &col_name=score&col_type=integer&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let rows = [("team-a", 10), ("team-a", 20), ("team-b", 20)];
    for (group_name, score) in rows {
        sqlx::query(r#"INSERT INTO "multi_filter" ("group_name", "score") VALUES (?, ?)"#)
            .bind(group_name)
            .bind(score)
            .execute(&app.state.pool())
            .await
            .unwrap();
    }

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/multi_filter/data/rows?offset=0&filter=group_name:team-a,score:20",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let data_rows = json["rows"].as_array().unwrap();

    assert_eq!(json["total_count"], 1);
    assert_eq!(data_rows[0][1], serde_json::json!("team-a"));
    assert_eq!(data_rows[0][2], serde_json::json!("20"));
}

#[tokio::test]
async fn database_table_data_sort_save_ignores_invalid_columns() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=sort_prune&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let save_body =
        r#"{"sort":[{"column":"missing","direction":"asc"},{"column":"body","direction":"desc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/sort_prune/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'sort_prune'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    let stored = stored.unwrap();
    assert!(!stored.contains("missing"));
    assert!(stored.contains(r#""column":"body""#));
}

#[tokio::test]
async fn database_table_data_survives_column_drop_with_stale_sort() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=drop_sort_src&col_name=keep&col_type=text&col_nullable=0\
                 &col_name=drop_me&col_type=text&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "drop_sort_src" ("keep", "drop_me") VALUES ('a', 'b')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"sort":[{"column":"drop_me","direction":"asc"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/drop_sort_src/data/sort",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/drop_sort_src/edit",
            Some("name=drop_sort_src&col_orig_name=keep&col_name=keep&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored_sort: Option<String> = sqlx::query_scalar(
        "SELECT sort_json FROM user_table_meta WHERE table_name = 'drop_sort_src'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert_eq!(stored_sort.as_deref(), Some("[]"));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/drop_sort_src/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["columns"], serde_json::json!(["id", "keep"]));
    assert_eq!(json["rows"].as_array().unwrap().len(), 1);
    assert!(json["sort"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn database_table_data_filter_save_and_load() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=filter_save&col_name=title&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "filter_save" ("title") VALUES ('alpha'), ('beta')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"filter":[{"column":"title","text":"lp"}]}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/filter_save/data/filter",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/filter_save/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["filter"][0]["column"], "title");
    assert_eq!(json["filter"][0]["text"], "lp");
    assert_eq!(json["total_count"], 1);
    assert_eq!(
        json["rows"].as_array().unwrap()[0][1],
        serde_json::json!("alpha")
    );

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT filter_json FROM user_table_meta WHERE table_name = 'filter_save'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .unwrap();
    assert!(stored.unwrap().contains("title"));
}

#[tokio::test]
async fn database_table_data_rows_empty_filter_clears_saved_filter() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=filter_default&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "filter_default" ("body") VALUES ('alpha'), ('beta')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"filter":[{"column":"body","text":"lp"}]}"#;
    app.admin_request(
        "POST",
        "/admin/database/tables/filter_default/data/filter",
        Some(save_body),
        Some("application/json"),
    )
    .await;

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/filter_default/data/rows?offset=0&filter=",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["filter"], serde_json::json!([]));
    assert_eq!(json["total_count"], 2);
}

#[tokio::test]
async fn database_table_data_column_widths_rejects_infra_table() {
    let app = common::TestApp::new().await;

    let save_body = r#"{"widths":{"table_name":100}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/_sqlx_migrations/data/column-widths",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("システムテーブル"));
}

#[tokio::test]
async fn database_table_data_rows_api_paginates() {
    let app = common::TestApp::new().await;

    sqlx::query(r#"CREATE TABLE "paginate_rows" (id INTEGER PRIMARY KEY, "n" INTEGER NOT NULL)"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    for index in 1..=1001 {
        sqlx::query(r#"INSERT INTO "paginate_rows" ("n") VALUES (?)"#)
            .bind(index)
            .execute(&app.state.pool())
            .await
            .unwrap();
    }

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/paginate_rows/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total_count"], 1001);
    assert_eq!(json["shown_count"], 1000);
    assert_eq!(json["has_more"], true);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/paginate_rows/data/rows?offset=1000",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["shown_count"], 1);
    assert_eq!(json["has_more"], false);
    assert_eq!(json["rows"][0][1], "1001");
}

#[tokio::test]
async fn database_table_data_rows_api_rejects_missing_table() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/missing_table/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn database_table_data_rows_api_rejects_system_table() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/_sqlx_migrations/data/rows",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("システムテーブル"));
}

#[tokio::test]
async fn database_table_seed_form_renders() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=seed_form&col_name=title&col_type=text&col_nullable=0&col_name=note&col_type=timestamp&col_nullable=1",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/seed_form/data/seed",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("テストデータ生成"));
    assert!(html.contains("title"));
    assert!(html.contains("note"));
    assert!(html.contains("ascii_alnum"));
    assert!(html.contains(r#"name="count""#));
    assert!(html.contains("seed-progress"));
    assert!(html.contains("seed-progress-bar"));
}

#[tokio::test]
async fn database_table_seed_generates_rows() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=seed_rows&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/seed_rows/data/seed",
            Some(
                "count=5&col_name=body&col_type=text&col_text_min=4&col_text_max=8&col_charset=ascii_alnum&col_include_null=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let sse = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&sse);
    assert!(events.iter().any(|(name, _)| name == "progress"));
    let done = events
        .iter()
        .find(|(name, _)| name == "done")
        .expect("done event");
    assert_eq!(done.1["count"], 5);
    assert!(done.1["elapsed_ms"].as_u64().is_some());
    assert!(done.1["redirect"]
        .as_str()
        .unwrap_or("")
        .contains("/admin/database/tables/seed_rows/data"));

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/seed_rows/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let rows = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
    assert_eq!(rows["total_count"], 5);

    let response = app
        .admin_request("GET", "/admin/database/tables/seed_rows/data", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("seed_rows"));
    assert!(html.contains(r#"data-api-url="/admin/database/tables/seed_rows/data/rows""#));
    assert!(html.contains("db-data-status"));
}

#[tokio::test]
async fn database_table_seed_mixed_column_types() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some(
                "name=seed_mixed&col_name=score&col_type=integer&col_nullable=0&col_name=%E3%81%82%E3%81%84%E3%81%86%E3%81%88&col_type=text&col_nullable=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/seed_mixed/data/seed",
            Some(
                "count=3\
                &col_name=score&col_name=%E3%81%82%E3%81%84%E3%81%86%E3%81%88\
                &col_type=integer&col_type=text\
                &col_int_min=0&col_int_min=0\
                &col_int_max=1000&col_int_max=1000\
                &col_real_min=0&col_real_min=0\
                &col_real_max=100&col_real_max=100\
                &col_text_min=8&col_text_min=4\
                &col_text_max=64&col_text_max=8\
                &col_charset=ascii_alnum&col_charset=ascii_alnum\
                &col_blob_min=1&col_blob_min=1\
                &col_blob_max=32&col_blob_max=32\
                &col_timestamp_from=2024-06-01T00:00&col_timestamp_from=2024-06-01T00:00\
                &col_timestamp_to=2025-06-01T00:00&col_timestamp_to=2025-06-01T00:00\
                &col_include_null=0&col_include_null=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let sse = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&sse);
    assert!(
        !events.iter().any(|(name, payload)| {
            name == "error"
                && payload["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("最小文字数")
        }),
        "unexpected validation error: {sse}"
    );
    let done = events
        .iter()
        .find(|(name, _)| name == "done")
        .expect("done event");
    assert_eq!(done.1["count"], 3);
}

#[tokio::test]
async fn database_table_seed_system_table_shows_notice() {
    let app = common::TestApp::new().await;

    for table in ["posts", "users"] {
        let response = app
            .admin_request(
                "GET",
                &format!("/admin/database/tables/{table}/data/seed"),
                None,
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("CMSのシステムテーブル"), "table: {table}");
        assert!(html.contains("列編集・テストデータ生成はできません"));
        assert!(html.contains("DB管理に戻る"));

        let response = app
            .admin_request(
                "POST",
                &format!("/admin/database/tables/{table}/data/seed"),
                Some("count=1"),
                Some("application/x-www-form-urlencoded"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let sse = String::from_utf8(body.to_vec()).unwrap();
        let events = parse_sse_events(&sse);
        let error = events
            .iter()
            .find(|(name, _)| name == "error")
            .expect("error event");
        assert!(error.1["message"]
            .as_str()
            .unwrap_or("")
            .contains("CMSのシステムテーブル"));
        assert!(error.1["message"]
            .as_str()
            .unwrap_or("")
            .contains("列編集・テストデータ生成はできません"));
    }
}

#[tokio::test]
async fn database_table_data_update_cell() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=cell_edit&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "cell_edit" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"column":"body","value":"updated","null":false,"keys":{"id":"1"}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/cell_edit/data/cells",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["value"], "updated");

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/cell_edit/data/rows?offset=0",
            None,
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["rows"], serde_json::json!([["1", "updated"]]));
}

#[tokio::test]
async fn database_table_data_update_cell_rejects_primary_key_column() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=cell_edit_pk&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "cell_edit_pk" ("body") VALUES ('hello')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();

    let save_body = r#"{"column":"id","value":"99","null":false,"keys":{"id":"1"}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/cell_edit_pk/data/cells",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("主キー"));
}

#[tokio::test]
async fn database_table_data_update_cell_rejects_cms_table() {
    let app = common::TestApp::new().await;

    let save_body = r#"{"column":"post_title","value":"x","null":false,"keys":{"id":"1"}}"#;
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/posts/data/cells",
            Some(save_body),
            Some("application/json"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("システムテーブル"));
}

#[tokio::test]
async fn database_cms_table_data_is_read_only() {
    let app = common::TestApp::new().await;

    for table in ["posts", "users"] {
        let response = app
            .admin_request(
                "GET",
                &format!("/admin/database/tables/{table}/data"),
                None,
                None,
            )
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains(table), "missing table name: {table}");
        assert!(html.contains("閲覧専用"));
        assert!(html.contains(r#"data-read-only="true""#));
        assert!(html.contains("CMSシステムテーブル"));
        assert!(html.contains("db-data-fit-all-columns"));
        assert!(html.contains("全列幅調整"));
        assert!(!html.contains(&format!("/admin/database/tables/{table}/data/seed")));
        assert!(!html.contains(&format!("/admin/database/tables/{table}/edit")));
    }
}

#[tokio::test]
async fn database_edit_system_table_shows_notice() {
    let app = common::TestApp::new().await;

    for table in ["posts", "users"] {
        let response = app
            .admin_request(
                "GET",
                &format!("/admin/database/tables/{table}/edit"),
                None,
                None,
            )
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("CMSのシステムテーブル"), "table: {table}");
        assert!(html.contains("列編集・テストデータ生成はできません"));
        assert!(html.contains("DB管理に戻る"));
    }
}

#[tokio::test]
async fn database_hidden_system_table_shows_infra_notice() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "GET",
            "/admin/database/tables/_sqlx_migrations/data",
            None,
            None,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("インフラ用のシステムテーブル"));
    assert!(!html.contains("CMSのシステムテーブル"));
}

#[tokio::test]
async fn database_table_seed_rejects_over_limit() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=seed_limit&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/seed_limit/data/seed",
            Some(
                "count=10000001&col_name=body&col_type=text&col_text_min=1&col_text_max=8&col_charset=ascii_alnum&col_include_null=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let sse = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&sse);
    let error = events
        .iter()
        .find(|(name, _)| name == "error")
        .expect("error event");
    assert!(error.1["message"]
        .as_str()
        .unwrap_or("")
        .contains("10000000"));
}

#[tokio::test]
async fn database_table_data_lists_rows_without_id_column() {
    let app = common::TestApp::new().await;

    sqlx::query(
        r#"CREATE TABLE "_sqlx_test" (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )"#,
    )
    .execute(&app.state.pool())
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO "_sqlx_test"
           (version, description, success, checksum, execution_time)
           VALUES (1, 'init', 1, X'0102', 42)"#,
    )
    .execute(&app.state.pool())
    .await
    .unwrap();

    let response = app
        .admin_request("GET", "/admin/database/tables/_sqlx_test/data", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("_sqlx_test"));
    assert!(html.contains(r#"data-api-url="/admin/database/tables/_sqlx_test/data/rows""#));
    assert!(html.contains("db-table-data-panel"));
    assert!(!html.contains("text-mono-cell"));
}

#[tokio::test]
async fn database_index_shows_new_button_for_each_tab() {
    let app = common::TestApp::new().await;

    let response = app.admin_request("GET", "/admin/database", None, None).await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#"href="/admin/database/tables/new""#));
    assert!(html.contains("新規追加"));

    let response = app
        .admin_request("GET", "/admin/database?tab=views", None, None)
        .await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#"href="/admin/database/views/new""#));
}

async fn create_dup_source_table(app: &common::TestApp) {
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=dup_source&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    sqlx::query(r#"INSERT INTO "dup_source" (body) VALUES ('hello'), ('world')"#)
        .execute(&app.state.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn database_index_embeds_duplicate_payloads() {
    let app = common::TestApp::new().await;
    create_dup_source_table(&app).await;
    create_dup_source_view(&app).await;

    let response = app.admin_request("GET", "/admin/database", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#"id="db-duplicate-payloads""#));

    let script = html
        .split(r#"id="db-duplicate-payloads""#)
        .nth(1)
        .and_then(|chunk| chunk.split("</script>").next())
        .unwrap_or("");
    let json_start = script.find('>').map(|idx| idx + 1).unwrap_or(0);
    let json: serde_json::Value = serde_json::from_str(script[json_start..].trim()).unwrap();
    assert_eq!(json["tables"]["dup_source"][0]["name"], "body");
    assert_eq!(json["tables"]["dup_source"][0]["type_key"], "text");
    assert_eq!(json["tables"]["dup_source"][0]["nullable"], true);
    assert_eq!(
        json["views"]["dup_view_src_view"],
        "SELECT id, body FROM dup_view_src"
    );
}

#[tokio::test]
async fn duplicate_user_table_copies_schema_only() {
    let app = common::TestApp::new().await;
    create_dup_source_table(&app).await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/dup_source/duplicate",
            Some("name=dup_schema&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("success_message="));

    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "dup_schema""#)
        .fetch_one(&app.state.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'dup_schema'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(definition.contains(r#""body" TEXT"#));
}

#[tokio::test]
async fn duplicate_user_table_copies_data_when_requested() {
    let app = common::TestApp::new().await;
    create_dup_source_table(&app).await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/dup_source/duplicate",
            Some(
                "name=dup_data&col_name=body&col_type=text&col_nullable=1&include_data=1",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let rows: Vec<(String,)> =
        sqlx::query_as(r#"SELECT body FROM "dup_data" ORDER BY id"#)
            .fetch_all(&app.state.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "hello");
    assert_eq!(rows[1].0, "world");
}

#[tokio::test]
async fn duplicate_user_table_rejects_same_or_existing_name() {
    let app = common::TestApp::new().await;
    create_dup_source_table(&app).await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/dup_source/duplicate",
            Some("name=dup_source&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("error_message="));
    assert!(urlencoding::decode(location).unwrap().contains("異なる必要があります"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/dup_source/duplicate",
            Some("name=dup_existing&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/dup_source/duplicate",
            Some("name=dup_existing&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("error_message="));
    assert!(urlencoding::decode(location).unwrap().contains("既に存在します"));
}

async fn create_dup_source_view(app: &common::TestApp) {
    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=dup_view_src&col_name=body&col_type=text&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/new",
            Some("name=dup_view_src_view&definition=SELECT+id%2C+body+FROM+dup_view_src"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn duplicate_user_view_copies_definition() {
    let app = common::TestApp::new().await;
    create_dup_source_view(&app).await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/dup_view_src_view/duplicate",
            Some(
                "name=dup_view_copy&definition=SELECT+id%2C+body+FROM+dup_view_src+WHERE+body+IS+NOT+NULL",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("tab=views"));
    assert!(location.contains("success_message="));

    let sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = 'dup_view_copy'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(sql.contains("dup_view_copy"));
    assert!(sql.contains("WHERE body IS NOT NULL"));
}

#[tokio::test]
async fn duplicate_user_view_rejects_same_or_existing_name() {
    let app = common::TestApp::new().await;
    create_dup_source_view(&app).await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/dup_view_src_view/duplicate",
            Some("name=dup_view_src_view&definition=SELECT+id+FROM+dup_view_src"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("tab=views"));
    assert!(location.contains("error_message="));
    assert!(urlencoding::decode(location).unwrap().contains("異なる必要があります"));

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/dup_view_src_view/duplicate",
            Some("name=dup_view_existing&definition=SELECT+id+FROM+dup_view_src"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/views/dup_view_src_view/duplicate",
            Some("name=dup_view_existing&definition=SELECT+id+FROM+dup_view_src"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("error_message="));
    assert!(urlencoding::decode(location).unwrap().contains("既に存在します"));
}

#[tokio::test]
async fn database_index_shows_duplicate_button_for_user_views() {
    let app = common::TestApp::new().await;
    create_dup_source_view(&app).await;

    let response = app
        .admin_request("GET", "/admin/database?tab=views", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    let view_row = html.split("dup_view_src_view</a>").nth(1).unwrap_or("");
    assert!(view_row.contains("data-view-duplicate"));
    assert!(view_row.contains("複製"));
}

#[tokio::test]
async fn database_index_shows_duplicate_button_for_user_tables() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/new",
            Some("name=custom_notes&col_name=body&col_type=text&col_nullable=0"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app.admin_request("GET", "/admin/database", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    let custom_row = html.split("custom_notes</a>").nth(1).unwrap_or("");
    assert!(custom_row.contains("data-table-duplicate"));
    assert!(custom_row.contains("複製"));

    let posts_row = html.split("posts</a>").nth(1).unwrap_or("");
    assert!(!posts_row.contains("data-table-duplicate"));
}

#[tokio::test]
async fn unauthenticated_database_redirects() {
    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin/database")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("/admin/login"));
}
