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
    let default = layouts::find_bootstrap_layout(&pool)
        .await
        .expect("bootstrap layout");

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
    let default = layouts::find_bootstrap_layout(&pool).await.unwrap();

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

#[tokio::test]
async fn home_page_url_can_be_changed_and_reassigned() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();
    let bootstrap = layouts::find_bootstrap_layout(&pool)
        .await
        .expect("bootstrap layout");

    let home = rust_sqlite_cms::repos::pages::find_home(&pool)
        .await
        .expect("find home")
        .expect("home page exists");

    let other_input = PageInput {
        name: "別ページ".to_string(),
        url_path: Some("/other-page".to_string()),
        content: r#"{% extends "default/shell.html" %}
{% block content %}<h1>Other</h1>{% endblock %}"#
            .to_string(),
        layout_id: bootstrap.id,
        is_published: true,
    };
    let (other_id, _) = services::pages::create_page(&pool, config, &other_input)
        .await
        .expect("create other page");

    let conflict_input = PageInput {
        name: "別ページ".to_string(),
        url_path: Some("/".to_string()),
        content: other_input.content.clone(),
        layout_id: bootstrap.id,
        is_published: true,
    };
    let err = services::pages::update_page(&pool, config, other_id, &conflict_input)
        .await
        .expect_err("should fail while / is still taken");
    assert!(err.to_string().contains("/"));

    let move_home_input = PageInput {
        name: home.name.clone(),
        url_path: Some("/old-home".to_string()),
        layout_id: home.layout_id,
        content: theme::read_page_body(&config.paths.work_dir, &home.layout_key, &home.file_name)
            .unwrap_or_default(),
        is_published: true,
    };
    services::pages::update_page(&pool, config, home.id, &move_home_input)
        .await
        .expect("move home away from /");

    let promote_input = PageInput {
        name: "別ページ".to_string(),
        url_path: Some("/".to_string()),
        content: other_input.content.clone(),
        layout_id: bootstrap.id,
        is_published: true,
    };
    services::pages::update_page(&pool, config, other_id, &promote_input)
        .await
        .expect("promote other page to /");

    let new_home = services::pages::find(&pool, other_id)
        .await
        .expect("find promoted page");
    assert_eq!(new_home.url_path.as_deref(), Some("/"));

    let former_home = services::pages::find(&pool, home.id)
        .await
        .expect("find former home");
    assert_eq!(former_home.url_path.as_deref(), Some("/old-home"));
}

#[tokio::test]
async fn home_page_can_be_deleted() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let config = app.state.config.as_ref();

    let home = rust_sqlite_cms::repos::pages::find_home(&pool)
        .await
        .expect("find home")
        .expect("home page exists");

    services::pages::delete_page(&pool, config, home.id)
        .await
        .expect("delete home page");

    let missing = rust_sqlite_cms::repos::pages::find_home(&pool)
        .await
        .expect("find home after delete");
    assert!(missing.is_none());
}
