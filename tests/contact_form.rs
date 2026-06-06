//! お問い合わせフォームウィジェットの統合テスト。

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rust_sqlite_cms::{
    models::placeholder::PlaceholderInput,
    repos::{placeholders, widget_types},
    services::contact_form,
    widgets,
};
use tower::ServiceExt;

#[tokio::test]
async fn contact_html_renders_form_with_double_submit_guard() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();
    let contact_type = widget_types::find_by_key(&pool, "contact_form")
        .await
        .expect("contact_form widget type");
    assert!(
        contact_type.html_template.contains("dataset.submitting"),
        "expected anti double-submit script in html_template"
    );

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "contact".to_string(),
            widget_type_id: contact_type.id,
            config: r#"{"heading":"お問い合わせ"}"#.to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let ctx = widgets::build_render_context(
        &pool,
        "Test Site".to_string(),
        "Description".to_string(),
        String::new(),
        widgets::RenderOptions {
            session_secret: app.state.config.security.session_secret.clone(),
            ..Default::default()
        },
    )
    .await
    .expect("build context");

    let json = serde_json::to_value(&ctx).expect("serialize context");
    let html = json
        .get("contact_html")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(html.contains("<form"), "contact_html should contain form, got: {html}");
    assert!(
        html.contains(&format!("/contact/{placeholder_id}")),
        "contact_html should contain action URL"
    );
    assert!(html.contains("name=\"token\""), "contact_html should contain token field");
    assert!(
        html.contains("dataset.submitting"),
        "contact_html should contain double-submit guard"
    );
}

#[tokio::test]
async fn contact_form_post_creates_single_post_and_redirects() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let contact_type = widget_types::find_by_key(&pool, "contact_form")
        .await
        .expect("contact_form widget type");
    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "contact".to_string(),
            widget_type_id: contact_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let secret = app
        .state
        .config
        .security
        .session_secret
        .as_deref()
        .expect("session secret");
    let token = contact_form::issue_token(placeholder_id, secret).expect("issue token");

    let body = format!(
        "name=Test+User&email=test%40example.com&message=Hello+world&token={}",
        urlencoding::encode(&token)
    );

    let response = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("http://localhost/contact/{placeholder_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .header("referer", "http://localhost/contact")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .expect("post request failed");

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.contains("contact_sent=contact"),
        "expected PRG redirect with contact_sent, got: {location}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE placeholder_id = ? AND post_type = 'post'",
    )
    .bind(placeholder_id)
    .fetch_one(&pool)
    .await
    .expect("count posts");
    assert_eq!(count, 1, "expected exactly one submission");

    let email: Option<String> = sqlx::query_scalar(
        r#"
        SELECT pm.meta_value
        FROM postmeta pm
        JOIN posts p ON p.id = pm.post_id
        WHERE p.placeholder_id = ?
          AND pm.meta_key = 'contact_email'
        LIMIT 1
        "#,
    )
    .bind(placeholder_id)
    .fetch_one(&pool)
    .await
    .expect("contact_email meta");
    assert_eq!(email.as_deref(), Some("test@example.com"));

    let response2 = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("http://localhost/contact/{placeholder_id}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .header("referer", "http://localhost/contact")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("second post request failed");

    assert_eq!(response2.status(), StatusCode::SEE_OTHER);

    let count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE placeholder_id = ? AND post_type = 'post'",
    )
    .bind(placeholder_id)
    .fetch_one(&pool)
    .await
    .expect("count posts after duplicate");
    assert_eq!(
        count_after, 2,
        "token reuse within TTL allows another submission; PRG prevents refresh duplicates"
    );
}

#[tokio::test]
async fn contact_sent_shows_success_message_without_form() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let contact_type = widget_types::find_by_key(&pool, "contact_form")
        .await
        .expect("contact_form widget type");
    placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "contact".to_string(),
            widget_type_id: contact_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let ctx = widgets::build_render_context(
        &pool,
        "Test Site".to_string(),
        "Description".to_string(),
        String::new(),
        widgets::RenderOptions {
            contact_sent: Some("contact".to_string()),
            session_secret: app.state.config.security.session_secret.clone(),
            ..Default::default()
        },
    )
    .await
    .expect("build context");

    let json = serde_json::to_value(&ctx).expect("serialize context");
    let html = json
        .get("contact_html")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        html.contains("contact-form-success"),
        "expected success message, got: {html}"
    );
    assert!(
        !html.contains("<form"),
        "success state should not render the form again"
    );
}

#[tokio::test]
async fn contact_error_still_renders_form_for_retry() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let contact_type = widget_types::find_by_key(&pool, "contact_form")
        .await
        .expect("contact_form widget type");
    placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "contact".to_string(),
            widget_type_id: contact_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let ctx = widgets::build_render_context(
        &pool,
        "Test Site".to_string(),
        "Description".to_string(),
        String::new(),
        widgets::RenderOptions {
            contact_error: Some("contact".to_string()),
            session_secret: app.state.config.security.session_secret.clone(),
            ..Default::default()
        },
    )
    .await
    .expect("build context");

    let json = serde_json::to_value(&ctx).expect("serialize context");
    let html = json
        .get("contact_html")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        html.contains("contact-form-error"),
        "expected error banner, got: {html}"
    );
    assert!(
        html.contains("<form"),
        "error state should still render form for retry, got: {html}"
    );
    assert!(
        html.contains("name=\"token\""),
        "error state should include fresh token"
    );
}
