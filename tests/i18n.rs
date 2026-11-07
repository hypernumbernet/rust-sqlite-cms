//! 言語ネゴシエーションのテスト。

mod common;

use rust_sqlite_cms::i18n::{self, Locale};

#[test]
fn accept_language_japanese_variants() {
    assert_eq!(
        i18n::negotiate_accept_language(Some("ja")),
        Locale::Ja
    );
    assert_eq!(
        i18n::negotiate_accept_language(Some("ja-JP,en;q=0.8")),
        Locale::Ja
    );
}

#[test]
fn accept_language_non_japanese_defaults_english() {
    assert_eq!(i18n::negotiate_accept_language(None), Locale::En);
    assert_eq!(
        i18n::negotiate_accept_language(Some("en-US,en;q=0.9")),
        Locale::En
    );
}

#[tokio::test]
async fn admin_dashboard_renders_english_without_accept_language() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;
    let cookie = app.login_admin_cookie().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#"lang="en""#));
    assert!(html.contains("Dashboard"));
    assert!(!html.contains("ダッシュボード"));
}

#[tokio::test]
async fn lang_query_sets_cookie_and_persists() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin/login?lang=ja")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("cms_lang=ja"))
        .expect("expected cms_lang cookie");
    assert!(set_cookie.contains("Path=/"));

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(r#"lang="ja""#));
}
