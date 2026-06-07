mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

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
        "_sqlx_migrations",
    ] {
        assert!(html.contains(table), "missing table: {table}");
    }
    assert!(html.contains("システム"));
    assert!(!html.contains("リードオンリー"));
    assert!(html.contains("_sqlx_migrations"));
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
    assert!(!posts_row.contains(r#"/admin/database/tables/posts/data"#));

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
            Some("name=edit_me&col_name=title&col_type=integer&col_nullable=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let definition = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'edit_me'",
    )
    .fetch_one(&app.state.pool())
    .await
    .unwrap();
    assert!(definition.contains(r#""title" INTEGER"#));
    assert!(!definition.contains("body"));
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
    assert!(html.contains("body"));
    assert!(html.contains("hello"));
    assert!(html.contains("表示 1 / 全 1 件"));
    assert!(html.contains("列編集"));
    assert!(html.contains("テストデータ生成"));
    assert!(html.contains(r#"/admin/database/tables/data_rows/data/seed"#));
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
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("/admin/database/tables/seed_rows/data"));

    let response = app
        .admin_request("GET", "/admin/database/tables/seed_rows/data", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("表示 5 / 全 5 件"));
}

#[tokio::test]
async fn database_table_seed_system_table_not_found() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request("GET", "/admin/database/tables/posts/data/seed", None, None)
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .admin_request(
            "POST",
            "/admin/database/tables/posts/data/seed",
            Some("count=1"),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
                "count=100001&col_name=body&col_type=text&col_text_min=1&col_text_max=8&col_charset=ascii_alnum&col_include_null=0",
            ),
            Some("application/x-www-form-urlencoded"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("100000"));
}

#[tokio::test]
async fn database_table_data_system_table_not_found() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request("GET", "/admin/database/tables/posts/data", None, None)
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn database_edit_system_table_returns_not_found() {
    let app = common::TestApp::new().await;

    let response = app
        .admin_request("GET", "/admin/database/tables/posts/edit", None, None)
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
