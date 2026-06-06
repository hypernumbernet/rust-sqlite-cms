//! プレースホルダー名と html_template 変数名が異なる場合でも *_html が生成されることを検証。

mod common;

use rust_sqlite_cms::{
    models::placeholder::PlaceholderInput,
    models::post::PostInput,
    repos::{placeholders, posts, widget_types},
    widgets,
};

#[tokio::test]
async fn announcements_html_renders_with_type_fixed_template_vars() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");
    assert!(
        news_type.html_template.contains("has_items"),
        "expected migrated html_template to use has_items"
    );

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "announcements".to_string(),
            widget_type_id: news_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    posts::insert(
        &pool,
        &PostInput {
            placeholder_id,
            title: "テストお知らせ".to_string(),
            content: "本文".to_string(),
            excerpt: "抜粋".to_string(),
            post_status: "publish".to_string(),
            post_name: "test-announce".to_string(),
        },
    )
    .await
    .expect("insert post");

    let ctx = widgets::build_render_context(
        &pool,
        "Test Site".to_string(),
        "Description".to_string(),
        String::new(),
        widgets::RenderOptions::default(),
    )
    .await
    .expect("build context");

    let json = serde_json::to_value(&ctx).expect("serialize context");
    let html = json
        .get("announcements_html")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        html.contains("テストお知らせ"),
        "announcements_html should contain published post title, got: {html}"
    );
    assert!(
        html.contains("news-item"),
        "announcements_html should contain rendered widget markup"
    );
}

#[tokio::test]
async fn annotate_widgets_wraps_html_with_preview_markers() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "marked_news".to_string(),
            widget_type_id: news_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    posts::insert(
        &pool,
        &PostInput {
            placeholder_id,
            title: "マーカー付き".to_string(),
            content: "本文".to_string(),
            excerpt: "抜粋".to_string(),
            post_status: "publish".to_string(),
            post_name: "marked".to_string(),
        },
    )
    .await
    .expect("insert post");

    let ctx = widgets::build_render_context(
        &pool,
        "Test Site".to_string(),
        "Description".to_string(),
        String::new(),
        widgets::RenderOptions {
            annotate_widgets: true,
            ..Default::default()
        },
    )
    .await
    .expect("build context");

    let json = serde_json::to_value(&ctx).expect("serialize context");
    let html = json
        .get("marked_news_html")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        html.contains("cms-widget-target"),
        "annotated html should contain wrapper class, got: {html}"
    );
    assert!(
        html.contains(&format!("data-cms-placeholder-id=\"{placeholder_id}\"")),
        "annotated html should contain placeholder id, got: {html}"
    );
    assert!(
        html.contains("data-cms-placeholder-name=\"marked_news\""),
        "annotated html should contain placeholder name, got: {html}"
    );
}
