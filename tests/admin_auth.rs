mod common;

use axum::http::StatusCode;
use http_body_util::BodyExt;

#[tokio::test]
async fn unauthenticated_get_admin_redirects() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(location.contains("/admin/login"));
}

#[tokio::test]
async fn unauthenticated_admin_users_redirects() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn login_with_wrong_password_shows_error() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("http://localhost/admin/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("login=admin&password=wrong-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ログイン名またはパスワードが正しくありません"));
}

#[tokio::test]
async fn login_with_valid_credentials_allows_admin_access() {
    let app = common::TestApp::new().await;

    let response = app.admin_request("GET", "/admin", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("ダッシュボード"));
    assert!(html.contains("ログアウト"));
}

#[tokio::test]
async fn logout_requires_login_again() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = common::TestApp::new().await;
    let cookie = app.login_admin_cookie().await;

    let logout = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("http://localhost/admin/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("http://localhost/admin/posts")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
