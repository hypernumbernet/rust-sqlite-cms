//! services::pages の重要な挙動を検証するテスト。

mod common;

use rust_sqlite_cms::{
    models::page::PageInput,
    services,
};

#[tokio::test]
async fn create_page_and_file_persistence() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;
    let config = app.state.config.as_ref();

    let input = PageInput {
        name: "テストページ from service".to_string(),
        url_path: Some("/test-service-page".to_string()),
        content: "<h1>Hello from test</h1>".to_string(),
        is_static: true,
        is_published: true,
    };

    // 作成
    let (id, file_name) = services::pages::create_page(pool, config, &input)
        .await
        .expect("create_page failed");

    assert!(id > 0);
    assert!(file_name.starts_with("page-"));

    // 取得できること
    let page = services::pages::find(pool, id).await.expect("find failed");
    assert_eq!(page.name, "テストページ from service");
    assert!(page.is_static);
    assert!(page.is_published);

    // ファイル実体が存在すること
    let content = rust_sqlite_cms::theme::read_page_content(&config.paths.work_dir, &file_name, true)
        .expect("file should exist");
    assert!(content.contains("Hello from test"));
}

#[tokio::test]
async fn delete_page_removes_file() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;
    let config = app.state.config.as_ref();

    let input = PageInput {
        name: "削除テスト".to_string(),
        url_path: Some("/to-be-deleted".to_string()),
        content: "will be gone".to_string(),
        is_static: false,
        is_published: false,
    };

    let (id, file_name) = services::pages::create_page(pool, config, &input)
        .await
        .unwrap();

    // 削除
    services::pages::delete_page(pool, config, id)
        .await
        .expect("delete_page failed");

    // ファイルが削除されていること
    let result = rust_sqlite_cms::theme::read_page_content(&config.paths.work_dir, &file_name, false);
    assert!(result.is_err());
}
