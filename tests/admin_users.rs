mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use rust_sqlite_cms::repos::users as users_repo;

#[tokio::test]
async fn users_index_lists_default_admin() {
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("admin"));
    assert!(html.contains("既定"));

    let users = users_repo::list_all(&app.state.pool).await.unwrap();
    assert_eq!(users.len(), 1);
    assert!(users[0].login.eq_ignore_ascii_case("admin"));
}

#[tokio::test]
async fn users_create_adds_second_user() {
    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("http://localhost/admin/users/new")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "login=editor1&display_name=編集者&password=password123",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let users = users_repo::list_all(&app.state.pool).await.unwrap();
    assert_eq!(users.len(), 2);
}

#[tokio::test]
async fn users_cannot_delete_admin() {
    let app = common::TestApp::new().await;
    let admin = users_repo::find_by_login(&app.state.pool, "admin")
        .await
        .unwrap()
        .expect("admin user");

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("http://localhost/admin/users/{}/delete", admin.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let users = users_repo::list_all(&app.state.pool).await.unwrap();
    assert_eq!(users.len(), 1);
}

#[tokio::test]
async fn users_duplicate_login_shows_error() {
    let app = common::TestApp::new().await;

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("http://localhost/admin/users/new")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "login=editor1&display_name=編集者&password=password123",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("http://localhost/admin/users/new")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "login=editor1&display_name=重複&password=password456",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("既に使用されています"));
}
