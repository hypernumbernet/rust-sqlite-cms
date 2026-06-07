mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

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
    for table in [
        "widget_types",
        "placeholders",
        "posts",
        "postmeta",
        "options",
        "layouts",
        "pages",
        "users",
    ] {
        assert!(html.contains(table), "missing table: {table}");
    }
    assert!(html.contains("システム"));
    assert!(!html.contains("リードオンリー"));
    assert!(!html.contains("_sqlx_migrations"));
    // CMS コアテーブルはすべて種別「システム」
    assert!(html.contains("posts</span>"));
    let posts_row = html.split("posts</span>").nth(1).unwrap_or("");
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
    let custom_row = html.split("custom_notes</span>").nth(1).unwrap_or("");
    assert!(custom_row.contains("ユーザー"));
    assert!(html.contains(r#"/admin/database/tables/custom_notes/edit"#));
    assert!(html.contains(r#"/admin/database/tables/custom_notes/data"#));
    assert!(custom_row.contains("列編集"));
    assert!(custom_row.contains("データ"));
    let posts_row = html.split("posts</span>").nth(1).unwrap_or("");
    assert!(!posts_row.contains(r#"/admin/database/tables/posts/edit"#));
    assert!(posts_row.contains(r#"/admin/database/tables/posts/data"#));
    assert!(!posts_row.contains("列編集"));
    assert!(posts_row.contains("データ"));
    let users_row = html.split("users</span>").nth(1).unwrap_or("");
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
    assert!(html.contains("SELECT id, body FROM custom_notes"));
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
    assert!(html.contains("列を編集"));
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
    assert!(html.contains("列を編集"));

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
    assert!(html.contains("表示 — / 全 — 件"));
    assert!(html.contains("列編集"));
    assert!(html.contains("テストデータ生成"));
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
    assert_eq!(json["total_count"], 1);
    assert_eq!(json["offset"], 0);
    assert_eq!(json["shown_count"], 1);
    assert_eq!(json["has_more"], false);
    assert_eq!(json["chunk_size"], 1000);
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
        assert!(html.contains("CMSシステムテーブル"));
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
                "count=1000001&col_name=body&col_type=text&col_text_min=1&col_text_max=8&col_charset=ascii_alnum&col_include_null=0",
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
        .contains("1000000"));
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
