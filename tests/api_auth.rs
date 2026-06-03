//! API セッション認証テスト。

use axum::http::StatusCode;
use serde_json::json;

mod common;

#[tokio::test]
async fn api_session_login_and_logout() {
    let app = common::TestApp::new().await;

    // 未認証で GET /session → 401
    let res = app.api_request("GET", "/api/v1/session", None, None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // ログイン成功
    let cookie = app.login_api_session().await;
    assert!(!cookie.is_empty());

    // GET /session → 200
    let res = app
        .api_request("GET", "/api/v1/session", None, Some(&cookie))
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    // 保護ルート → 200
    let res = app
        .api_request("GET", "/api/v1/settings", None, Some(&cookie))
        .await;
    assert_eq!(res.status(), StatusCode::OK);

    // ログアウト
    let res = app
        .api_request("DELETE", "/api/v1/session", None, Some(&cookie))
        .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ログアウト後は保護ルート → 401
    let res = app
        .api_request("GET", "/api/v1/settings", None, Some(&cookie))
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_session_login_invalid_credentials() {
    let app = common::TestApp::new().await;

    let body = json!({
        "login": "admin",
        "password": "wrong-password",
    });
    let res = app
        .api_request("POST", "/api/v1/session", Some(body), None)
        .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_login_cookie_works_for_api() {
    let app = common::TestApp::new().await;

    // 管理画面ログインの Cookie で API も利用可能
    let cookie = app.login_admin_cookie().await;
    let res = app
        .api_request("GET", "/api/v1/settings", None, Some(&cookie))
        .await;
    assert_eq!(res.status(), StatusCode::OK);
}
