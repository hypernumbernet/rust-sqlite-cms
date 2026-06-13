//! レイアウト ZIP エクスポート / インポートの統合テスト。

use std::io::{Cursor, Read, Write};

use rust_sqlite_cms::models::layout::{
    LayoutExportManifest, LayoutImportAction, LayoutImportMode, LayoutInput,
};
use rust_sqlite_cms::models::page::PageInput;
use rust_sqlite_cms::models::placeholder::PlaceholderInput;
use rust_sqlite_cms::models::post::PostInput;
use rust_sqlite_cms::repos::{layouts, pages, placeholders, posts, widget_types};
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

    let imported_pages = pages::list_by_layout(&pool, imported.id)
        .await
        .expect("imported pages");
    let about = imported_pages
        .iter()
        .find(|p| p.name == "About")
        .expect("about page");
    assert!(about.url_path.is_none());
    assert!(!about.is_published);

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

    let layout = layouts::find_by_key(&pool, &layout_key)
        .await
        .expect("lookup")
        .expect("layout");
    let layout_pages = pages::list_by_layout(&pool, layout.id)
        .await
        .expect("layout pages");
    let about = layout_pages
        .iter()
        .find(|p| p.name == "About")
        .expect("about page");
    assert!(about.url_path.is_none());
    assert!(!about.is_published);

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
        .find(|p| p.name == "About")
        .expect("about page");
    assert!(about.url_path.is_none());
    assert!(!about.is_published);
    let body = theme::read_page_body(&config.paths.work_dir, "renamed-layout", &about.file_name)
        .expect("page body");
    assert!(body.contains("renamed-layout/shell.html"));
    assert!(!body.contains("export-src/shell.html"));
}

#[tokio::test]
async fn import_rename_on_same_site_imports_unpublished_pages_without_urls() {
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
    let copied_pages = pages::list_by_layout(&pool, copied.id).await.expect("pages");
    let copied_about = copied_pages
        .iter()
        .find(|p| p.name == "About")
        .expect("copied about page");
    assert!(copied_about.url_path.is_none());
    assert!(!copied_about.is_published);

    let original = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("original pages")
        .into_iter()
        .find(|p| p.url_path.as_deref() == Some("/export-src-about"))
        .expect("original page still exists");
    assert_ne!(copied_about.id, original.id);
}

#[tokio::test]
async fn import_overwrite_clears_existing_page_urls() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, layout_key) = create_test_layout_with_page(&app).await;
    let original_pages = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("original pages");
    let original_about = original_pages
        .iter()
        .find(|p| p.name == "About")
        .expect("original about");
    assert_eq!(
        original_about.url_path.as_deref(),
        Some("/export-src-about")
    );
    assert!(original_about.is_published);

    let exported = layouts_service::export_layout_zip(&pool, config, layout_id)
        .await
        .expect("export");

    let (action, _) = layouts_service::import_layout_zip(
        &pool,
        config,
        &exported,
        LayoutImportMode::Overwrite,
        None,
    )
    .await
    .expect("import overwrite");
    assert_eq!(action, LayoutImportAction::Updated);

    let updated_pages = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("updated pages");
    let updated_about = updated_pages
        .iter()
        .find(|p| p.id == original_about.id)
        .expect("updated about");
    assert!(updated_about.url_path.is_none());
    assert!(!updated_about.is_published);

    let shell = theme::read_shell(&config.paths.work_dir, &layout_key).expect("shell");
    assert!(shell.contains("custom shell"));
}

#[tokio::test]
async fn duplicate_layout_copies_to_new_key() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _) = create_test_layout_with_page(&app).await;

    let message = layouts_service::duplicate_layout(&pool, config, layout_id, "layout-dup", false)
        .await
        .expect("duplicate");
    assert!(message.contains("layout-dup"));

    let copied = layouts::find_by_key(&pool, "layout-dup")
        .await
        .expect("lookup")
        .expect("copied layout");
    assert_eq!(copied.name, "エクスポート元");

    let copied_pages = pages::list_by_layout(&pool, copied.id).await.expect("pages");
    let copied_about = copied_pages
        .iter()
        .find(|p| p.name == "About")
        .expect("copied about page");
    assert!(copied_about.url_path.is_none());
    assert!(!copied_about.is_published);

    let body = theme::read_page_body(&config.paths.work_dir, "layout-dup", &copied_about.file_name)
        .expect("page body");
    assert!(body.contains("layout-dup/shell.html"));
    assert!(!body.contains("export-src/shell.html"));

    let original = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("original pages")
        .into_iter()
        .find(|p| p.url_path.as_deref() == Some("/export-src-about"))
        .expect("original page still exists");
    assert_eq!(original.url_path.as_deref(), Some("/export-src-about"));
    assert!(original.is_published);
}

#[tokio::test]
async fn duplicate_layout_rejects_same_or_existing_key() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _) = create_test_layout_with_page(&app).await;

    let same_key_err =
        layouts_service::duplicate_layout(&pool, config, layout_id, "export-src", false)
            .await
            .expect_err("same key");
    assert!(same_key_err.to_string().contains("異なる必要があります"));

    layouts_service::duplicate_layout(&pool, config, layout_id, "layout-dup", false)
        .await
        .expect("first duplicate");

    let existing_err =
        layouts_service::duplicate_layout(&pool, config, layout_id, "layout-dup", false)
            .await
            .expect_err("existing key");
    assert!(existing_err.to_string().contains("既に使われています"));
}

async fn create_test_layout_with_placeholder_post(app: &common::TestApp) -> (i64, i64, i64) {
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _) = create_test_layout_with_page(app).await;

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");
    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "export_src_news".to_string(),
            widget_type_id: news_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let post_id = posts::insert(
        &pool,
        &PostInput {
            placeholder_id,
            title: "複製元お知らせ".to_string(),
            content: "本文".to_string(),
            excerpt: "抜粋".to_string(),
            post_status: "publish".to_string(),
            post_name: "export-src-post".to_string(),
        },
    )
    .await
    .expect("insert post");

    let about = pages::list_by_layout(&pool, layout_id)
        .await
        .expect("pages")
        .into_iter()
        .find(|p| p.name == "About")
        .expect("about page");
    let content = format!(
        "{{% extends \"export-src/shell.html\" %}}\n{{% block content %}}<div>{{{{ export_src_news_html | safe }}}}</div>{{% endblock %}}"
    );
    theme::write_page_body(&config.paths.work_dir, "export-src", &about.file_name, &content)
        .expect("write page body");

    (layout_id, placeholder_id, post_id)
}

#[tokio::test]
async fn duplicate_layout_copies_posts_when_requested() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let (layout_id, _placeholder_id, original_post_id) =
        create_test_layout_with_placeholder_post(&app).await;

    let message =
        layouts_service::duplicate_layout(&pool, config, layout_id, "export-src-dup", true)
            .await
            .expect("duplicate with posts");
    assert!(message.contains("プレースホルダー 1 件"));
    assert!(message.contains("投稿 1 件"));

    let copied_layout = layouts::find_by_key(&pool, "export-src-dup")
        .await
        .expect("lookup")
        .expect("copied layout");
    let copied_about = pages::list_by_layout(&pool, copied_layout.id)
        .await
        .expect("pages")
        .into_iter()
        .find(|p| p.name == "About")
        .expect("about page");
    let body = theme::read_page_body(
        &config.paths.work_dir,
        "export-src-dup",
        &copied_about.file_name,
    )
    .expect("page body");
    assert!(body.contains("export_src_dup_news_html"));
    assert!(!body.contains("export_src_news_html"));

    let new_placeholder = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM placeholders WHERE name = 'export_src_dup_news'",
    )
    .fetch_one(&pool)
    .await
    .expect("new placeholder");

    let copied_posts = posts::list_all_for_placeholder(&pool, new_placeholder.0)
        .await
        .expect("copied posts");
    assert_eq!(copied_posts.len(), 1);
    assert_eq!(copied_posts[0].post_status, "publish");
    assert!(copied_posts[0].published_at.is_some());
    assert_ne!(copied_posts[0].id, original_post_id);

    let original_post = posts::find(&pool, original_post_id)
        .await
        .expect("original post");
    assert_eq!(original_post.post_status, "publish");
}
