//! services::pages の重要な挙動を検証するテスト。

mod common;

use rust_sqlite_cms::{
    models::page::PageInput,
    repos::layouts,
    services,
    theme,
};

#[tokio::test]
async fn create_page_and_file_persistence() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();
    let default = layouts::find_default(&pool).await.expect("default layout");

    let input = PageInput {
        name: "テストページ from service".to_string(),
        url_path: Some("/test-service-page".to_string()),
        content: r#"{% extends "default/shell.html" %}
{% block content %}<h1>Hello from test</h1>{% endblock %}"#
            .to_string(),
        layout_id: default.id,
        is_published: true,
    };

    let (id, file_name) = services::pages::create_page(&pool, config, &input)
        .await
        .expect("create_page failed");

    assert!(id > 0);
    assert!(file_name.starts_with("pages/page-"));

    let page = services::pages::find(&pool, id).await.expect("find failed");
    assert_eq!(page.name, "テストページ from service");
    assert_eq!(page.layout_id, default.id);
    assert_eq!(page.layout_key, "default");
    assert!(page.is_published);

    let content = theme::read_page_body(&config.paths.work_dir, &page.layout_key, &file_name)
        .expect("file exists");
    assert!(content.contains("Hello from test"));
}

#[tokio::test]
async fn delete_page_removes_file() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();
    let default = layouts::find_default(&pool).await.unwrap();

    let input = PageInput {
        name: "削除テスト".to_string(),
        url_path: Some("/to-be-deleted".to_string()),
        content: "{% extends \"default/shell.html\" %}{% block content %}gone{% endblock %}"
            .to_string(),
        layout_id: default.id,
        is_published: false,
    };

    let (id, file_name) = services::pages::create_page(&pool, config, &input)
        .await
        .unwrap();

    services::pages::delete_page(&pool, config, id)
        .await
        .expect("delete_page failed");

    let result = theme::read_page_body(&config.paths.work_dir, "default", &file_name);
    assert!(result.is_err());
}
