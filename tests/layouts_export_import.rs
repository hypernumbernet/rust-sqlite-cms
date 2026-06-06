//! レイアウト ZIP エクスポート / インポートの統合テスト。

use std::io::{Cursor, Read, Write};

use rust_sqlite_cms::models::layout::{
    LayoutExportManifest, LayoutImportAction, LayoutImportMode, LayoutInput,
};
use rust_sqlite_cms::models::page::PageInput;
use rust_sqlite_cms::repos::{layouts, pages};
use rust_sqlite_cms::services::layouts as layouts_service;
use rust_sqlite_cms::theme;
use zip::ZipArchive;

mod common;

async fn create_test_layout_with_page(app: &common::TestApp) -> (i64, String) {
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let input = LayoutInput {
        key: "export-src".to_string(),
        name: "エクスポート元".to_string(),
        is_default: false,
        favicon_media_id: None,
    };
    let layout_id = layouts_service::create_layout_with_defaults(&pool, config, &input)
        .await
        .expect("create layout");

    let page_input = PageInput {
        name: "About".to_string(),
        url_path: Some("/export-src-about".to_string()),
        content: "{% extends \"export-src/shell.html\" %}\n{% block content %}<p>about</p>{% endblock %}"
            .to_string(),
        layout_id,
        is_published: true,
    };
    layouts_service::write_shell_content(config, "export-src", "<!-- custom shell -->")
        .expect("write shell");
    let (_, file_name) = pages::insert(&pool, &page_input)
        .await
        .expect("insert page");
    theme::write_page_body(
        &config.paths.work_dir,
        "export-src",
        &file_name,
        &page_input.content,
    )
    .expect("write page body");

    (layout_id, "export-src".to_string())
}

fn read_zip_manifest(bytes: &[u8]) -> LayoutExportManifest {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).expect("zip archive");
    let mut file = archive.by_name("manifest.json").expect("manifest.json");
    let mut raw = String::new();
    file.read_to_string(&mut raw).expect("read manifest");
    serde_json::from_str(&raw).expect("parse manifest")
}

fn read_zip_entry(bytes: &[u8], path: &str) -> String {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).expect("zip archive");
    let mut file = archive.by_name(path).expect("zip entry");
    let mut raw = String::new();
    file.read_to_string(&mut raw).expect("read entry");
    raw
}

fn rewrite_manifest_key(bytes: &[u8], new_key: &str) -> Vec<u8> {
    let mut manifest = read_zip_manifest(bytes);
    manifest.layout.key = new_key.to_string();
    for page in &mut manifest.pages {
        if page.url_path.as_deref() == Some("/export-src-about") {
            page.url_path = Some("/imported-about".to_string());
        }
    }
    let manifest_json = serde_json::to_string_pretty(&manifest).expect("serialize manifest");

    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).expect("zip archive");
    let mut out = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).expect("zip entry");
            let name = file.name().replace('\\', "/");
            if name == "manifest.json" {
                writer
                    .start_file("manifest.json", options)
                    .expect("start manifest");
                writer
                    .write_all(manifest_json.as_bytes())
                    .expect("write manifest");
                continue;
            }

            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("read entry");
            let new_name = name.replacen("export-src", new_key, 1);
            writer.start_file(&new_name, options).expect("start file");
            writer.write_all(&data).expect("write file");
        }
        writer.finish().expect("finish zip");
    }
    out
}

#[tokio::test]
async fn export_includes_manifest_shell_and_pages() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, layout_key) = create_test_layout_with_page(&app).await;

    let bytes = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    let manifest = read_zip_manifest(&bytes);
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.layout.key, layout_key);
    assert!(!manifest.pages.is_empty());

    let shell = read_zip_entry(&bytes, &format!("{layout_key}/shell.html"));
    assert!(shell.contains("custom shell"));

    let page = manifest
        .pages
        .iter()
        .find(|p| p.url_path.as_deref() == Some("/export-src-about"));
    assert!(page.is_some());
    let page = page.expect("about page");
    let body = read_zip_entry(&bytes, &format!("{layout_key}/{}", page.file_name));
    assert!(body.contains("about"));
}

#[tokio::test]
async fn import_creates_new_layout() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _) = create_test_layout_with_page(&app).await;
    let exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");
    let import_bytes = rewrite_manifest_key(&exported, "imported-new");

    let (action, _) =
        layouts_service::import_layout_zip(
            &pool,
            config,
            &import_bytes,
            LayoutImportMode::Overwrite,
            None,
        )
        .await
        .expect("import");
    assert_eq!(action, LayoutImportAction::Created);

    let imported = layouts::find_by_key(&pool, "imported-new")
        .await
        .expect("lookup")
        .expect("imported layout");
    assert_eq!(imported.name, "エクスポート元");

    let shell = theme::read_shell(&config.paths.work_dir, "imported-new").expect("shell");
    assert!(shell.contains("custom shell"));
}

#[tokio::test]
async fn import_overwrite_updates_layout() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, layout_key) = create_test_layout_with_page(&app).await;
    let exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    layouts_service::write_shell_content(config, &layout_key, "<!-- old shell on disk -->")
        .expect("mutate shell");

    let (action, _) =
        layouts_service::import_layout_zip(
            &pool,
            config,
            &exported,
            LayoutImportMode::Overwrite,
            None,
        )
        .await
        .expect("import overwrite");
    assert_eq!(action, LayoutImportAction::Updated);

    let shell = theme::read_shell(&config.paths.work_dir, &layout_key).expect("shell");
    assert!(shell.contains("custom shell"));
    assert!(!shell.contains("old shell on disk"));
}

#[tokio::test]
async fn import_skip_leaves_layout_unchanged() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, layout_key) = create_test_layout_with_page(&app).await;
    let exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    layouts_service::write_shell_content(config, &layout_key, "<!-- kept shell -->")
        .expect("mutate shell");

    let (action, _) =
        layouts_service::import_layout_zip(&pool, config, &exported, LayoutImportMode::Skip, None)
            .await
            .expect("import skip");
    assert_eq!(action, LayoutImportAction::Skipped);

    let shell = theme::read_shell(&config.paths.work_dir, &layout_key).expect("shell");
    assert!(shell.contains("kept shell"));
}

#[tokio::test]
async fn import_rejects_url_path_conflict() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let default_layout = layouts::find_default(&pool).await.expect("default");
    let page_input = PageInput {
        name: "衝突ページ".to_string(),
        url_path: Some("/conflict-url".to_string()),
        content: "body".to_string(),
        layout_id: default_layout.id,
        is_published: false,
    };
    pages::insert(&pool, &page_input).await.expect("seed page");

    let (layout_id, _) = create_test_layout_with_page(&app).await;
    let mut exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    let mut manifest = read_zip_manifest(&exported);
    for page in &mut manifest.pages {
        if page.url_path.as_deref() == Some("/export-src-about") {
            page.url_path = Some("/conflict-url".to_string());
        }
    }
    let manifest_json = serde_json::to_string_pretty(&manifest).expect("serialize");

    let cursor = Cursor::new(&exported);
    let mut archive = ZipArchive::new(cursor).expect("zip");
    let mut out = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).expect("entry");
            let name = file.name().replace('\\', "/");
            if name == "manifest.json" {
                writer.start_file("manifest.json", options).expect("start");
                writer.write_all(manifest_json.as_bytes()).expect("write");
                continue;
            }
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("read");
            writer.start_file(&name, options).expect("start file");
            writer.write_all(&data).expect("write file");
        }
        writer.finish().expect("finish");
    }
    exported = out;

    let err = layouts_service::import_layout_zip(
        &pool,
        config,
        &exported,
        LayoutImportMode::Overwrite,
        None,
    )
    .await
    .expect_err("url conflict");
    assert!(err.to_string().contains("conflict-url"));
}

#[tokio::test]
async fn import_rename_creates_layout_with_rewritten_references() {
    let source_app = common::TestApp::new().await;
    let (layout_id, _) = create_test_layout_with_page(&source_app).await;
    let exported = layouts_service::export_layout_zip(
        &source_app.state.pool(),
        &source_app.state.config,
        layout_id,
    )
    .await
    .expect("export");

    let target_app = common::TestApp::new().await;
    let pool = &target_app.state.pool();
    let config = target_app.state.config.as_ref();

    let (action, _) = layouts_service::import_layout_zip(
        &pool,
        config,
        &exported,
        LayoutImportMode::Rename,
        Some("renamed-layout"),
    )
    .await
    .expect("rename import");
    assert_eq!(action, LayoutImportAction::Created);

    let imported = layouts::find_by_key(&pool, "renamed-layout")
        .await
        .expect("lookup")
        .expect("renamed layout");
    assert_eq!(imported.name, "エクスポート元");

    let pages = pages::list_by_layout(&pool, imported.id).await.expect("pages");
    let about = pages
        .iter()
        .find(|p| p.url_path.as_deref() == Some("/renamed-layout/export-src-about"))
        .expect("about page");
    let body = theme::read_page_body(&config.paths.work_dir, "renamed-layout", &about.file_name)
        .expect("page body");
    assert!(body.contains("renamed-layout/shell.html"));
    assert!(!body.contains("export-src/shell.html"));
}

#[tokio::test]
async fn import_rename_on_same_site_prefixes_urls_to_avoid_conflict() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _) = create_test_layout_with_page(&app).await;
    let exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    let (action, _) = layouts_service::import_layout_zip(
        &pool,
        config,
        &exported,
        LayoutImportMode::Rename,
        Some("layout-copy"),
    )
    .await
    .expect("rename import on same site");
    assert_eq!(action, LayoutImportAction::Created);

    let copied = layouts::find_by_key(&pool, "layout-copy")
        .await
        .expect("lookup")
        .expect("copied layout");
    let pages = pages::list_by_layout(&pool, copied.id).await.expect("pages");
    let about = pages
        .iter()
        .find(|p| p.url_path.as_deref() == Some("/layout-copy/export-src-about"))
        .expect("prefixed url");
    assert!(about.is_published);

    let original = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("original pages")
        .into_iter()
        .find(|p| p.url_path.as_deref() == Some("/export-src-about"))
        .expect("original page still exists");
    assert_ne!(about.id, original.id);
}
