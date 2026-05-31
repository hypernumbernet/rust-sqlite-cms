//! ウィジェット設定サービス。

use sqlx::SqlitePool;

use crate::error::DomainResult;
use crate::models::widget::{WidgetType, WidgetTypeInput};
use crate::repos::widget_types as widget_types_repo;

/// 全ウィジェットタイプを取得。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<WidgetType>> {
    widget_types_repo::list_all(pool).await.map_err(Into::into)
}

/// 指定タイプの config + html_template を更新。
/// ウィジェット画面のHTML編集に対応。
pub async fn update_config(pool: &SqlitePool, type_key: &str, config_json: &str, html_template: &str) -> DomainResult<()> {
    let input = WidgetTypeInput {
        config: config_json.to_string(),
        html_template: html_template.to_string(),
    };
    widget_types_repo::update_config(pool, type_key, &input)
        .await
        .map_err(Into::into)
}

/// ウィジェットタイプ全体を更新（type_key の変更 + html_template + config）。
pub async fn update(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
) -> DomainResult<()> {
    widget_types_repo::update(pool, old_type_key, new_type_key, html_template, config)
        .await
        .map_err(Into::into)
}

/// ウィジェットタイプ全体を更新（config_schema も含む）。
pub async fn update_with_schema(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
    config_schema: &str,
) -> DomainResult<()> {
    widget_types_repo::update_with_schema(pool, old_type_key, new_type_key, html_template, config, config_schema)
        .await
        .map_err(Into::into)
}
