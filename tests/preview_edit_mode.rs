//! ページプレビューの編集モード UI を検証する。

mod common;

use axum::body::to_bytes;
use axum::http::StatusCode;
use rust_sqlite_cms::repos::pages;

async fn response_body_string(response: axum::http::Response<axum::body::Body>) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

#[tokio::test]
async fn preview_includes_edit_mode_ui_and_widget_markers() {
    let app = common::TestApp::new().await;
    let page = pages::find_by_file_name(&app.state.pool, "index.html")
        .await
        .expect("lookup index page")
        .expect("index page should exist");

    let response = app
        .admin_request("GET", &format!("/admin/pages/{}/preview", page.id), None, None)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let html = response_body_string(response).await;

    assert!(
        html.contains("cms-preview-edit-toggle"),
        "preview should include edit mode toggle"
    );
    assert!(
        html.contains("cms-preview-modal"),
        "preview should include edit modal"
    );
    assert!(
        html.contains("cms-preview-edit-script"),
        "preview should include edit mode script"
    );
    assert!(
        html.contains("cms-widget-target"),
        "preview should annotate widget regions"
    );
    assert!(
        html.contains("data-cms-placeholder-id"),
        "preview widget markers should include placeholder id"
    );
}
