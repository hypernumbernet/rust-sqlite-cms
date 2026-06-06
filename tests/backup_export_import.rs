//! サイト全体バックアップ / リストアの統合テスト。

use std::io::{Cursor, Read, Write};

use http_body_util::BodyExt;
use rust_sqlite_cms::error::DomainError;
use rust_sqlite_cms::services::backup as backup_service;
use tower::ServiceExt;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

mod common;

fn read_zip_entry(bytes: &[u8], path: &str) -> Vec<u8> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).expect("zip archive");
    let mut file = archive.by_name(path).expect("zip entry");
    let mut raw = Vec::new();
    file.read_to_end(&mut raw).expect("read entry");
    raw
}

fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
    let cursor = Cursor::new(bytes);
    let archive = ZipArchive::new(cursor).expect("zip archive");
    archive.file_names().map(str::to_string).collect()
}

#[tokio::test]
async fn export_includes_manifest_database_and_layout_files() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let bytes = backup_service::export_site_backup(&&pool, config)
        .await
        .expect("export backup");

    let names = zip_entry_names(&bytes);
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.contains(&"database/cms.db".to_string()));
    assert!(
        names.iter().any(|name| name.contains("layouts/")),
        "expected layout files in backup, got: {names:?}"
    );

    let manifest_raw = read_zip_entry(&bytes, "manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_raw).expect("manifest json");
    assert_eq!(manifest["format_version"], 1);
}

#[tokio::test]
async fn roundtrip_restore_returns_original_data() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let original_posts: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash'",
    )
    .fetch_one(&pool)
    .await
    .expect("count posts");

    let bytes = backup_service::export_site_backup(&&pool, config)
        .await
        .expect("export backup");

    sqlx::query(
        "INSERT INTO posts (post_type, post_status, title, post_name, content)
         VALUES ('post', 'publish', 'Temporary', 'temporary-backup-test', 'body')",
    )
    .execute(&pool)
    .await
    .expect("insert temp post");

    let marker = app.state.config.paths.work_dir.clone() + "/backup-marker.txt";
    std::fs::write(&marker, "changed").expect("write marker");

    backup_service::import_site_backup(&app.state, &bytes)
        .await
        .expect("import backup");

    let restored_posts: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash'",
    )
    .fetch_one(&app.state.pool())
    .await
    .expect("count restored posts via reloaded pool");

    assert_eq!(restored_posts.0, original_posts.0);
    assert!(
        !std::path::Path::new(&marker).exists(),
        "marker file should be removed by restore"
    );
}

#[tokio::test]
async fn import_rejects_missing_manifest() {
    let app = common::TestApp::new().await;
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("database/cms.db", options).unwrap();
    zip.write_all(b"sqlite").unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let err = backup_service::import_site_backup(&app.state, &bytes)
        .await
        .expect_err("missing manifest should fail");

    assert!(matches!(err, DomainError::Validation(_)));
}

#[tokio::test]
async fn import_rejects_unsupported_format_version() {
    let app = common::TestApp::new().await;
    let db_bytes = read_zip_entry(
        &backup_service::export_site_backup(&app.state.pool(), app.state.config.as_ref())
            .await
            .expect("export"),
        "database/cms.db",
    );

    let manifest = serde_json::json!({
        "format_version": 99,
        "cms_version": "0.0.0",
        "created_at": "2026-01-01T00:00:00Z",
        "database_path": app.state.config.database.path,
        "paths": {
            "work_dir": app.state.config.paths.work_dir,
            "uploads_dir": app.state.config.paths.uploads_dir
        }
    });

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .unwrap();
    zip.start_file("database/cms.db", options).unwrap();
    zip.write_all(&db_bytes).unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let err = backup_service::import_site_backup(&app.state, &bytes)
        .await
        .expect_err("unsupported format should fail");

    assert!(matches!(err, DomainError::Validation(msg) if msg.contains("format_version")));
}

#[tokio::test]
async fn import_rejects_unsafe_zip_paths() {
    let app = common::TestApp::new().await;
    let db_bytes = b"sqlite";
    let work_prefix = app.state.config.paths.work_dir.clone();

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let manifest = serde_json::json!({
        "format_version": 1,
        "cms_version": "0.0.0",
        "created_at": "2026-01-01T00:00:00Z",
        "database_path": app.state.config.database.path,
        "paths": {
            "work_dir": work_prefix,
            "uploads_dir": format!("{work_prefix}/uploads")
        }
    });
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
        .unwrap();
    zip.start_file("../escape.txt", options).unwrap();
    zip.write_all(b"bad").unwrap();
    zip.start_file("database/cms.db", options).unwrap();
    zip.write_all(db_bytes).unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let err = backup_service::import_site_backup(&app.state, &bytes)
        .await
        .expect_err("path traversal should fail");

    assert!(matches!(err, DomainError::Validation(msg) if msg.contains("安全でない")));
}

#[tokio::test]
async fn admin_export_requires_auth_and_returns_zip() {
    let app = common::TestApp::new().await;

    let unauth = app
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("http://localhost/admin/backup/export")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .expect("unauth request");
    assert_eq!(unauth.status(), axum::http::StatusCode::SEE_OTHER);

    let response = app.admin_request("GET", "/admin/backup/export", None, None).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").and_then(|v| v.to_str().ok()),
        Some("application/zip")
    );

    let body = response.into_body().collect().await.expect("read body").to_bytes();
    assert!(zip_entry_names(&body).contains(&"manifest.json".to_string()));
}

#[tokio::test]
async fn admin_backup_page_renders_panels() {
    let app = common::TestApp::new().await;
    let response = app.admin_request("GET", "/admin/backup", None, None).await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let html = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(html.contains("バックアップをダウンロード"));
    assert!(html.contains("リストアを実行"));
    assert!(html.contains("backup-panels"));
    assert!(!html.contains("サーバーを再起動"));
}
