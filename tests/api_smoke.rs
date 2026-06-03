//! APIレイヤーのスモークテスト。

use axum::http::StatusCode;
use serde_json::json;

mod common;

#[tokio::test]
async fn api_health_and_basic_crud() {
    let app = common::TestApp::new().await;

    // 未認証は 401
    let res = app.api_request("GET", "/api/v1/placeholders", None, None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 1. プレースホルダー一覧
    let res = app
        .api_request_authed("GET", "/api/v1/placeholders", None)
        .await;
    assert_eq!(res.status(), 200);

    // 2. プレースホルダー作成
    let create_body = json!({
        "name": "test_from_api",
        "widget_type_id": 1
    });
    let res = app
        .api_request_authed("POST", "/api/v1/placeholders", Some(create_body))
        .await;
    assert_eq!(res.status(), 200);

    // 3. 設定取得
    let res = app
        .api_request_authed("GET", "/api/v1/settings", None)
        .await;
    assert_eq!(res.status(), 200);

    // 4. ページ一覧
    let res = app.api_request_authed("GET", "/api/v1/pages", None).await;
    assert_eq!(res.status(), 200);
}
