//! レイアウト static ファイル（CSS 等）の同期・アップロード検証。

mod common;

use std::collections::HashMap;

use axum::body::to_bytes;
use axum::http::StatusCode;
use tower::ServiceExt;
use rust_sqlite_cms::{
    models::layout::LayoutInput,
    repos::layouts,
    services,
    theme::{self, resolve_static_path},
};

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
    0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
    0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn update_layout_syncs_site_css() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let layout = layouts::find_default(&pool).await.expect("default layout");
    let shell = theme::read_shell(&config.paths.work_dir, &layout.key).unwrap_or_default();
    let input = LayoutInput {
        key: layout.key.clone(),
        name: layout.name.clone(),
        is_default: layout.is_default,
    };

    let mut static_files = HashMap::new();
    static_files.insert("site.css".to_string(), "body { color: red; }".to_string());

    services::layouts::update_layout(
        &pool,
        config,
        layout.id,
        &input,
        &shell,
        &static_files,
        &[],
    )
    .await
    .expect("update layout with css");

    let css = theme::read_static_text(&config.paths.work_dir, &layout.key, "site.css")
        .expect("read site.css");
    assert_eq!(css, "body { color: red; }");
}

#[tokio::test]
async fn sync_static_text_files_deletes_marked_paths() {
    let app = common::TestApp::new().await;
    let config = app.state.config.as_ref();
    let work_dir = &config.paths.work_dir;

    theme::write_static_text(work_dir, "default", "old.css", "old").expect("seed old");
    theme::write_static_text(work_dir, "default", "site.css", "keep").expect("seed site");

    services::layouts::sync_static_text_files(
        config,
        "default",
        &HashMap::from([("site.css".to_string(), "updated".to_string())]),
        &["old.css".to_string()],
    )
    .expect("sync");

    assert!(theme::read_static_text(work_dir, "default", "old.css").is_err());
    let site = theme::read_static_text(work_dir, "default", "site.css").expect("site.css");
    assert_eq!(site, "updated");
}

#[tokio::test]
async fn upload_static_file_rejects_unsafe_path() {
    let app = common::TestApp::new().await;
    let config = app.state.config.as_ref();

    let err = services::layouts::upload_static_file(config, "default", "../evil.png", TINY_PNG)
        .expect_err("traversal should fail");
    assert!(
        err.to_string().contains("不正") || err.to_string().contains("パス"),
        "unexpected: {err}"
    );
}

#[tokio::test]
async fn upload_static_binary_served_at_public_url() {
    let app = common::TestApp::new().await;
    let config = app.state.config.as_ref();

    services::layouts::upload_static_file(config, "default", "logo.png", TINY_PNG)
        .expect("upload png");

    let resolved = resolve_static_path(&config.paths.work_dir, "default/logo.png")
        .expect("resolve static");
    assert!(resolved.is_file());

    let response = app
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("http://localhost/static/default/logo.png")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .expect("static request");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(body.as_ref(), TINY_PNG);
}

#[tokio::test]
async fn list_admin_files_includes_shell_and_static() {
    let app = common::TestApp::new().await;
    let config = app.state.config.as_ref();
    let layout = layouts::find_default(&app.state.pool())
        .await
        .expect("default layout");

    let files =
        services::layouts::list_admin_files(&config.paths.work_dir, &layout.key).expect("list");
    assert!(files.iter().any(|f| f.display_path == "shell.html" && f.is_text_editable));
    assert!(files
        .iter()
        .any(|f| f.display_path == "static/site.css" && f.is_text_editable));
    assert_eq!(
        files
            .iter()
            .find(|f| f.display_path == "shell.html")
            .and_then(|f| f.edit_url(layout.id))
            .as_deref(),
        Some(format!("/admin/layouts/{}/files/shell.html", layout.id).as_str())
    );
}

#[tokio::test]
async fn create_layout_seeds_site_css() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let input = LayoutInput {
        key: "corp".to_string(),
        name: "Corporate".to_string(),
        is_default: false,

    };
    let static_files = services::layouts::default_static_text_files_for_create();

    services::layouts::create_layout(
        &pool,
        config,
        &input,
        "<html></html>",
        &static_files,
    )
    .await
    .expect("create layout");

    let css = theme::read_static_text(&config.paths.work_dir, "corp", "site.css")
        .expect("seeded site.css");
    assert!(!css.is_empty());
}
