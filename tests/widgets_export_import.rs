//! ウィジェット エクスポート / インポートの統合テスト。

use rust_sqlite_cms::models::widget::{WidgetImportMode, WidgetPackage};
use rust_sqlite_cms::repos::widget_types;
use rust_sqlite_cms::services::widgets;
use serde_json::json;

mod common;

#[tokio::test]
async fn import_creates_custom_widget_type() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let package = WidgetPackage {
        format_version: 1,
        type_key: "my_banner".to_string(),
        label: "カスタムバナー".to_string(),
        description: "テスト用ウィジェット".to_string(),
        config: "{}".to_string(),
        html_template: "<div class=\"banner\">{{ config }}</div>".to_string(),
        config_schema: r#"{"fields":[]}"#.to_string(),
    };

    let (action, _) = widgets::import_package(pool, &package, WidgetImportMode::Overwrite, None)
        .await
        .expect("import should succeed");
    assert_eq!(action, rust_sqlite_cms::models::widget::WidgetImportAction::Created);

    let row = widget_types::find_by_key(pool, "my_banner")
        .await
        .expect("custom widget should exist");
    assert_eq!(row.label, "カスタムバナー");
    assert_eq!(row.html_template, package.html_template);
}

#[tokio::test]
async fn export_import_roundtrip_preserves_html_template() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let exported = widgets::export_package(pool, "news")
        .await
        .expect("export news");
    assert_eq!(exported.format_version, 1);
    assert_eq!(exported.type_key, "news");
    assert!(!exported.html_template.is_empty());

    let mut modified = exported.clone();
    modified.html_template = "<!-- roundtrip test -->\n".to_string() + &modified.html_template;

    widgets::import_package(pool, &modified, WidgetImportMode::Overwrite, None)
        .await
        .expect("re-import");

    let re_exported = widgets::export_package(pool, "news")
        .await
        .expect("re-export");
    assert_eq!(re_exported.html_template, modified.html_template);
}

#[tokio::test]
async fn import_skip_does_not_overwrite() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let before = widgets::export_package(pool, "image")
        .await
        .expect("export image");

    let mut attacker = before.clone();
    attacker.html_template = "<!-- should not apply -->".to_string();

    let (action, _) = widgets::import_package(pool, &attacker, WidgetImportMode::Skip, None)
        .await
        .expect("skip import");
    assert_eq!(
        action,
        rust_sqlite_cms::models::widget::WidgetImportAction::Skipped
    );

    let after = widgets::export_package(pool, "image")
        .await
        .expect("export image again");
    assert_eq!(after.html_template, before.html_template);
}

#[tokio::test]
async fn api_export_and_import() {
    let app = common::TestApp::new().await;

    let res = app
        .api_request_authed("GET", "/api/v1/widgets/news/export", None)
        .await;
    assert_eq!(res.status(), 200);

    let package = json!({
        "format_version": 1,
        "type_key": "api_widget",
        "label": "API Widget",
        "description": "from api test",
        "config": "{}",
        "html_template": "<p>api</p>",
        "config_schema": "{\"fields\":[]}"
    });

    let res = app
        .api_request_authed(
            "POST",
            "/api/v1/widgets/import",
            Some(json!({
                "package": package,
                "mode": "overwrite"
            })),
        )
        .await;
    assert_eq!(res.status(), 200);

    let res = app
        .api_request_authed("GET", "/api/v1/widgets/api_widget/export", None)
        .await;
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn delete_custom_widget_without_placeholders() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let package = WidgetPackage {
        format_version: 1,
        type_key: "to_delete".to_string(),
        label: "削除テスト".to_string(),
        description: "".to_string(),
        config: "{}".to_string(),
        html_template: "<p>x</p>".to_string(),
        config_schema: "{}".to_string(),
    };
    widgets::import_package(pool, &package, WidgetImportMode::Overwrite, None)
        .await
        .unwrap();

    widgets::delete(pool, "to_delete").await.expect("delete should succeed");
    assert!(widget_types::find_by_key(pool, "to_delete").await.is_err());
}

#[tokio::test]
async fn delete_blocked_when_placeholder_exists() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let err = widgets::delete(pool, "news")
        .await
        .expect_err("news has default placeholder");
    assert!(err.to_string().contains("プレースホルダー"));
}

#[tokio::test]
async fn validate_package_rejects_invalid_type_key() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let package = WidgetPackage {
        format_version: 1,
        type_key: "Invalid-Key".to_string(),
        label: "".to_string(),
        description: "".to_string(),
        config: "{}".to_string(),
        html_template: "".to_string(),
        config_schema: "{}".to_string(),
    };

    let err = widgets::import_package(pool, &package, WidgetImportMode::Overwrite, None)
        .await
        .expect_err("invalid type_key should fail");
    assert!(err.to_string().contains("type_key"));
}

#[tokio::test]
async fn import_rename_creates_widget_under_new_type_key() {
    let app = common::TestApp::new().await;
    let pool = &app.state.pool;

    let exported = widgets::export_package(pool, "news")
        .await
        .expect("export news");

    let (action, _) =
        widgets::import_package(pool, &exported, WidgetImportMode::Rename, Some("news_copy"))
            .await
            .expect("rename import");
    assert_eq!(
        action,
        rust_sqlite_cms::models::widget::WidgetImportAction::Created
    );

    let row = widget_types::find_by_key(pool, "news_copy")
        .await
        .expect("renamed widget should exist");
    assert_eq!(row.html_template, exported.html_template);
    assert!(widget_types::find_by_key(pool, "news").await.is_ok());
}
