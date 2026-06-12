//! メディア公開 URL（favicon 含む）の検証。

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rust_sqlite_cms::{
    error::AppError,
    media,
    models::media::MediaInput,
    page_render,
    repos::{media as media_repo, pages},
    services,
};
use tower::ServiceExt;

/// 1x1 PNG（最小）
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
    0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
    0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

async fn insert_png_media(app: &common::TestApp, name: &str) -> i64 {
    let pool = app.state.pool();
    let uploads = &app.state.config.paths.uploads_dir;
    let (file_path, mime_type) = media::save_upload(uploads, name, TINY_PNG).expect("png upload");
    media_repo::insert(
        &pool,
        &MediaInput {
            title: name.to_string(),
            file_path,
            mime_type,
            original_name: name.to_string(),
            file_size: TINY_PNG.len() as i64,
        },
    )
    .await
    .expect("insert media")
}

#[test]
fn ico_extension_is_allowed_for_upload() {
    let mime = media::save_upload(
        &std::env::temp_dir().to_string_lossy(),
        "site.ico",
        &[0x00, 0x00, 0x01, 0x00],
    );
    assert!(mime.is_ok(), "ico upload should be accepted: {:?}", mime.err());
    let (_, mime_type) = mime.unwrap();
    assert_eq!(mime_type, "image/x-icon");
}

#[tokio::test]
async fn rejects_pdf_as_favicon_public_url() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let uploads = &app.state.config.paths.uploads_dir;

    let (file_path, mime_type) =
        media::save_upload(uploads, "doc.pdf", b"%PDF-1.4\n").expect("pdf upload");
    let pdf_id = media_repo::insert(
        &pool,
        &MediaInput {
            title: "doc.pdf".to_string(),
            file_path,
            mime_type,
            original_name: "doc.pdf".to_string(),
            file_size: 8,
        },
    )
    .await
    .expect("insert pdf");

    let err = services::media::update_public_url(&pool, pdf_id, "/favicon.ico")
        .await
        .expect_err("pdf favicon should fail");
    assert!(
        err.to_string().contains("favicon"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn rendered_page_includes_favicon_link() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let media_id = insert_png_media(&app, "favicon.png").await;
    services::media::update_public_url(&pool, media_id, "/favicon.ico")
        .await
        .expect("set favicon public url");

    let page = pages::find_home(&pool).await.expect("home").expect("home page");
    let html = page_render::render_page(&app.state, &page)
        .await
        .expect("render home")
        .0;

    assert!(
        html.contains(r#"rel="icon""#) && html.contains("favicon.ico"),
        "HTML should contain favicon link, got excerpt: {}",
        &html[..html.len().min(800)]
    );
}

#[tokio::test]
async fn favicon_route_serves_media_with_public_url() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let media_id = insert_png_media(&app, "site-icon.png").await;
    services::media::update_public_url(&pool, media_id, "/favicon.ico")
        .await
        .expect("set favicon public url");

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/favicon.ico")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("favicon request");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.starts_with("image/"));
}

#[tokio::test]
async fn custom_public_url_is_served_via_fallback() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let media_id = insert_png_media(&app, "logo.png").await;
    services::media::update_public_url(&pool, media_id, "/brand-logo.png")
        .await
        .expect("set custom public url");

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/brand-logo.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("custom alias request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_duplicate_public_url() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let first = insert_png_media(&app, "a.png").await;
    let second = insert_png_media(&app, "b.png").await;
    services::media::update_public_url(&pool, first, "/shared.png")
        .await
        .expect("first url");

    let err = services::media::update_public_url(&pool, second, "/shared.png")
        .await
        .expect_err("duplicate public url");
    match err {
        AppError::Conflict(msg) => assert!(msg.contains("shared.png")),
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn rejects_public_url_conflicting_with_page() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let media_id = insert_png_media(&app, "conflict.png").await;
    let err = services::media::update_public_url(&pool, media_id, "/")
        .await
        .expect_err("root path should fail");

    assert!(
        err.to_string().contains("予約") || err.to_string().contains("URL"),
        "unexpected error: {err}"
    );
}