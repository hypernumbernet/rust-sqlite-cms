//! サンプルセットのインストールテスト。

mod common;

use rust_sqlite_cms::error::AppError;
use rust_sqlite_cms::samples::{self, InstallResult};

#[tokio::test]
async fn install_corporate_sample_set_succeeds() {
    let app = common::TestApp::new().await;

    let result = samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("install should succeed");

    let InstallResult::Layout {
        layout_key,
        preview_path,
        placeholders_count,
        pages_count,
        ..
    } = result
    else {
        panic!("expected Layout result");
    };

    assert_eq!(layout_key, "corporate");
    assert_eq!(preview_path, "/corporate");
    assert_eq!(placeholders_count, 6);
    assert_eq!(pages_count, 4);

    let layout_key: String = sqlx::query_scalar("SELECT key FROM layouts WHERE key = 'corporate'")
        .fetch_one(&app.state.pool())
        .await
        .expect("corporate layout");
    assert_eq!(layout_key, "corporate");

    let home_published: i64 = sqlx::query_scalar(
        "SELECT is_published FROM pages WHERE url_path = '/corporate'",
    )
    .fetch_one(&app.state.pool())
    .await
    .expect("corporate home page");
    assert_eq!(home_published, 1);

    let placeholder_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM placeholders WHERE name = 'corporate_news'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .expect("placeholder query");
    assert!(placeholder_exists.is_some());

    let post_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE post_name LIKE 'corporate-%' AND post_type = 'post'",
    )
    .fetch_one(&app.state.pool())
    .await
    .expect("post count");
    assert!(post_count >= 10);
}

#[tokio::test]
async fn install_corporate_sample_set_conflicts_on_second_install() {
    let app = common::TestApp::new().await;

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("first install");

    let err = samples::install_sample_set(&app.state, "corporate")
        .await
        .expect_err("second install should conflict");

    match err {
        AppError::Conflict(msg) => {
            assert!(msg.contains("corporate"));
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn install_corporate_tables_sample_set_succeeds() {
    let app = common::TestApp::new().await;

    let result = samples::install_sample_set(&app.state, "corporate-tables")
        .await
        .expect("install should succeed");

    let InstallResult::Tables {
        tables_count,
        views_count,
        rows_count,
        ..
    } = result
    else {
        panic!("expected Tables result");
    };

    assert_eq!(tables_count, 2);
    assert_eq!(views_count, 1);
    assert_eq!(rows_count, 11);

    let services_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM corporate_services")
            .fetch_one(&app.state.pool())
            .await
            .expect("services count");
    assert_eq!(services_count, 5);

    let team_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM corporate_team")
        .fetch_one(&app.state.pool())
        .await
        .expect("team count");
    assert_eq!(team_count, 6);

    let featured_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM corporate_featured_services")
            .fetch_one(&app.state.pool())
            .await
            .expect("featured view count");
    assert_eq!(featured_count, 3);
}

#[tokio::test]
async fn install_corporate_tables_conflicts_on_second_install() {
    let app = common::TestApp::new().await;

    samples::install_sample_set(&app.state, "corporate-tables")
        .await
        .expect("first install");

    let err = samples::install_sample_set(&app.state, "corporate-tables")
        .await
        .expect_err("second install should conflict");

    match err {
        AppError::Conflict(msg) => {
            assert!(msg.contains("corporate_services") || msg.contains("corporate_team"));
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn layout_and_table_sets_install_independently() {
    let app = common::TestApp::new().await;

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("layout install");
    samples::install_sample_set(&app.state, "corporate-tables")
        .await
        .expect("table install");

    let layout_exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM layouts WHERE key = 'corporate'")
        .fetch_optional(&app.state.pool())
        .await
        .expect("layout");
    assert!(layout_exists.is_some());

    let table_exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'corporate_services'",
    )
    .fetch_optional(&app.state.pool())
    .await
    .expect("table");
    assert!(table_exists.is_some());
}

#[tokio::test]
async fn install_bicycle_sample_set_succeeds() {
    let app = common::TestApp::new().await;

    let result = samples::install_sample_set(&app.state, "bicycle")
        .await
        .expect("install should succeed");

    let InstallResult::Layout {
        layout_key,
        preview_path,
        placeholders_count,
        pages_count,
        ..
    } = result
    else {
        panic!("expected Layout result");
    };

    assert_eq!(layout_key, "bicycle");
    assert_eq!(preview_path, "/bicycle");
    assert_eq!(placeholders_count, 6);
    assert_eq!(pages_count, 4);

    let about_name: String = sqlx::query_scalar(
        "SELECT name FROM pages WHERE url_path = '/bicycle/about'",
    )
    .fetch_one(&app.state.pool())
    .await
    .expect("about page");
    assert_eq!(about_name, "店舗紹介");
}

#[tokio::test]
async fn install_unknown_sample_set_returns_error() {
    let app = common::TestApp::new().await;

    let err = samples::install_sample_set(&app.state, "unknown")
        .await
        .expect_err("unknown sample");

    match err {
        AppError::Conflict(msg) => assert!(msg.contains("unknown")),
        other => panic!("expected Conflict, got {other:?}"),
    }
}