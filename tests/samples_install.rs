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
        placeholders_count,
        pages_count,
        ..
    } = result
    else {
        panic!("expected Layout result");
    };

    assert_eq!(layout_key, "corporate");
    assert_eq!(placeholders_count, 6);
    assert_eq!(pages_count, 4);

    let layout_key: String = sqlx::query_scalar("SELECT key FROM layouts WHERE key = 'corporate'")
        .fetch_one(&app.state.pool())
        .await
        .expect("corporate layout");
    assert_eq!(layout_key, "corporate");

    let (home_published, home_url): (i64, Option<String>) = sqlx::query_as(
        "SELECT is_published, url_path FROM pages WHERE file_name = 'pages/home.html' AND layout_id = (SELECT id FROM layouts WHERE key = 'corporate')",
    )
    .fetch_one(&app.state.pool())
    .await
    .expect("corporate home page");
    assert_eq!(home_published, 0);
    assert!(home_url.is_none());

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

struct CorporateManualCleanup {
    trashed_count: i64,
}

async fn prepare_corporate_manual_cleanup(
    pool: &sqlx::SqlitePool,
    uploads_dir: &str,
    delete_media: bool,
) -> CorporateManualCleanup {
    use rust_sqlite_cms::repos::{layouts, media, pages, placeholders, posts};

    let layout = layouts::find_by_key(pool, "corporate")
        .await
        .expect("find layout")
        .expect("corporate layout");

    let corporate_posts: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM posts WHERE post_type = 'post' AND post_name LIKE 'corporate-%'",
    )
    .fetch_all(pool)
    .await
    .expect("list corporate posts");

    for (id,) in corporate_posts {
        posts::delete(pool, id).await.expect("trash post");
    }

    for placeholder in placeholders::list_all(pool).await.expect("placeholders") {
        if placeholder.name.starts_with("corporate_") {
            placeholders::delete(pool, placeholder.id)
                .await
                .expect("delete placeholder");
        }
    }

    for page in pages::list_by_layout(pool, layout.id)
        .await
        .expect("list pages")
    {
        pages::delete(pool, page.id).await.expect("delete page");
    }

    layouts::delete(pool, layout.id)
        .await
        .expect("delete layout");

    if delete_media {
        for item in media::list_all(pool).await.expect("list media") {
            let Some(file_path) = item.file_path.as_deref() else {
                continue;
            };
            if !file_path.starts_with("corporate-") {
                continue;
            }
            let _ = rust_sqlite_cms::media::delete_file(uploads_dir, file_path);
            media::delete(pool, item.id)
                .await
                .expect("delete media attachment");
        }
    }

    let trashed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status = 'trash' AND post_name LIKE 'corporate-%'",
    )
    .fetch_one(pool)
    .await
    .expect("trashed count");

    CorporateManualCleanup { trashed_count }
}

#[tokio::test]
async fn reinstall_corporate_after_trashing_posts_succeeds() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let uploads_dir = &app.state.config.paths.uploads_dir;

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("first install");

    let cleanup = prepare_corporate_manual_cleanup(&pool, uploads_dir, true).await;
    assert!(
        cleanup.trashed_count > 0,
        "posts should remain in trash before reinstall"
    );

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("reinstall after manual cleanup should succeed");

    let trashed_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status = 'trash' AND post_name LIKE 'corporate-%'",
    )
    .fetch_one(&pool)
    .await
    .expect("trashed count after reinstall");
    assert_eq!(
        cleanup.trashed_count, trashed_after,
        "trashed posts should be preserved across reinstall"
    );

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash' AND post_name LIKE 'corporate-%'",
    )
    .fetch_one(&pool)
    .await
    .expect("active count");
    assert!(active_count >= 10);
}

#[tokio::test]
async fn reinstall_corporate_conflicts_when_media_remains() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let uploads_dir = &app.state.config.paths.uploads_dir;

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("first install");

    prepare_corporate_manual_cleanup(&pool, uploads_dir, false).await;

    let err = samples::install_sample_set(&app.state, "corporate")
        .await
        .expect_err("reinstall should conflict when media remains");

    match err {
        AppError::Conflict(msg) => {
            assert!(
                msg.contains("corporate-hero.png"),
                "expected media conflict, got: {msg}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
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
        placeholders_count,
        pages_count,
        ..
    } = result
    else {
        panic!("expected Layout result");
    };

    assert_eq!(layout_key, "bicycle");
    assert_eq!(placeholders_count, 6);
    assert_eq!(pages_count, 4);

    let about_name: String = sqlx::query_scalar(
        "SELECT name FROM pages WHERE file_name = 'pages/about.html' AND layout_id = (SELECT id FROM layouts WHERE key = 'bicycle')",
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