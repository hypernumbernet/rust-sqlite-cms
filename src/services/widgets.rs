//! ウィジェット設定サービス。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::widget::{WidgetType, WidgetTypeInput};
use crate::repos::widget_types as widget_types_repo;

/// 全ウィジェットタイプを取得。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<WidgetType>> {
    widget_types_repo::list_all(pool).await
}

/// 指定タイプの config を更新（JSON 文字列で受け取り、そのまま保存）。
pub async fn update_config(pool: &SqlitePool, type_key: &str, config_json: &str) -> AppResult<()> {
    let input = WidgetTypeInput {
        config: config_json.to_string(),
    };
    widget_types_repo::update_config(pool, type_key, &input).await
}
