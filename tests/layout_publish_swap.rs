//! レイアウトセットの公開差し替えテスト。

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rust_sqlite_cms::{
    repos::{layouts, pages},
    samples,
    services::layout_publish,
};
use tower::ServiceExt;

#[tokio::test]
async fn publish_layout_set_swaps_live_site_and_demotes_previous() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let example_layout = layouts::find_by_key(&pool, "example")
        .await
        .expect("find example")
        .expect("example layout");
    let example_home = pages::find_by_layout_file(&pool, example_layout.id, "pages/index.html")
        .await
        .expect("find example home")
        .expect("example home page");
    assert!(example_home.is_published);
    assert_eq!(example_home.url_path.as_deref(), Some("/"));

    samples::install_sample_set(&app.state, "corporate")
        .await
        .expect("install corporate");

    let corporate_layout = layouts::find_by_key(&pool, "corporate")
        .await
        .expect("find corporate")
        .expect("corporate layout");

    let corporate_home = pages::find_by_layout_file(&pool, corporate_layout.id, "pages/home.html")
        .await
        .expect("find corporate home")
        .expect("corporate home page");
    assert!(!corporate_home.is_published);
    assert!(corporate_home.url_path.is_none());

    let result = layout_publish::publish_layout_set(&pool, corporate_layout.id)
        .await
        .expect("publish corporate layout");
    assert_eq!(result.published_count, 4);
    assert_eq!(result.demoted_count, 1);
    assert_eq!(result.demoted_layout_keys, vec!["example".to_string()]);

    let example_home = pages::find(&pool, example_home.id)
        .await
        .expect("reload example home");
    assert!(!example_home.is_published);
    assert!(example_home.url_path.is_none());

    let corporate_home = pages::find(&pool, corporate_home.id)
        .await
        .expect("reload corporate home");
    assert!(corporate_home.is_published);
    assert_eq!(corporate_home.url_path.as_deref(), Some("/"));

    let corporate_news = pages::find_by_layout_file(&pool, corporate_layout.id, "pages/news.html")
        .await
        .expect("find corporate news")
        .expect("corporate news page");
    assert!(corporate_news.is_published);
    assert_eq!(corporate_news.url_path.as_deref(), Some("/news"));

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("home request");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = String::from_utf8_lossy(&body);
    assert!(
        body.contains("地域と企業の成長を支える"),
        "home should render corporate layout, got: {body}"
    );
}