//! レイアウト favicon（メディア選択）の検証。

mod common;

use std::collections::HashMap;

use rust_sqlite_cms::{
    media,
    models::{
        layout::LayoutInput,
        media::MediaInput,
    },
    page_render,
    repos::{layouts, media as media_repo, pages},
    services,
    theme,
};

/// 1x1 PNG（最小）
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
    0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
    0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

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
async fn layout_rejects_non_favicon_media() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;
    let config = app.state.config.as_ref();
    let uploads = &config.paths.uploads_dir;

    let (file_path, mime_type) =
        media::save_upload(uploads, "doc.pdf", b"%PDF-1.4\n").expect("pdf upload");
    let pdf_id = media_repo::insert(
        pool,
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

    let layout = layouts::find_default(pool).await.expect("default layout");
    let shell = theme::read_shell(&config.paths.work_dir, &layout.key).unwrap_or_default();
    let input = LayoutInput {
        key: layout.key.clone(),
        name: layout.name.clone(),
        is_default: layout.is_default,
        favicon_media_id: Some(pdf_id),
    };

    let err = services::layouts::update_layout(
        pool,
        config,
        layout.id,
        &input,
        &shell,
        &HashMap::new(),
        &[],
    )
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
    let pool = &app.state.pool;
    let config = app.state.config.as_ref();
    let uploads = &config.paths.uploads_dir;

    let (file_path, mime_type) =
        media::save_upload(uploads, "favicon.png", TINY_PNG).expect("png upload");
    let media_id = media_repo::insert(
        pool,
        &MediaInput {
            title: "favicon.png".to_string(),
            file_path: file_path.clone(),
            mime_type,
            original_name: "favicon.png".to_string(),
            file_size: TINY_PNG.len() as i64,
        },
    )
    .await
    .expect("insert media");

    let layout = layouts::find_default(pool).await.expect("default layout");
    let shell = theme::read_shell(&config.paths.work_dir, &layout.key).unwrap_or_default();
    let input = LayoutInput {
        key: layout.key.clone(),
        name: layout.name.clone(),
        is_default: layout.is_default,
        favicon_media_id: Some(media_id),
    };
    services::layouts::update_layout(
        pool,
        config,
        layout.id,
        &input,
        &shell,
        &HashMap::new(),
        &[],
    )
    .await
    .expect("update layout favicon");

    let page = pages::find_home(pool).await.expect("home").expect("home page");
    let html = page_render::render_page(&app.state, &page)
        .await
        .expect("render home")
        .0;

    assert!(
        html.contains(r#"rel="icon" href="#) && html.contains("uploads"),
        "HTML should contain favicon link, got excerpt: {}",
        &html[..html.len().min(800)]
    );
    let stored_name = file_path.rsplit('/').next().expect("stored file name");
    assert!(
        html.contains(stored_name),
        "favicon href should reference uploaded file {stored_name}"
    );
}
