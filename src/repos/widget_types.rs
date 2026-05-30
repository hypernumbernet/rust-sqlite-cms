use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::widget::{WidgetType, WidgetTypeInput};

/// 全ウィジェットタイプを type_key 順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<WidgetType>> {
    Ok(sqlx::query_as::<_, WidgetType>(
        "SELECT id, type_key, config, updated_at
         FROM widget_types
         ORDER BY type_key ASC, id ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// type_key でウィジェットタイプを取得する。存在しなければ `NotFound`。
pub async fn find_by_key(pool: &SqlitePool, type_key: &str) -> AppResult<WidgetType> {
    sqlx::query_as::<_, WidgetType>(
        "SELECT id, type_key, config, updated_at
         FROM widget_types
         WHERE type_key = ?",
    )
    .bind(type_key)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// ID でウィジェットタイプを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<WidgetType> {
    sqlx::query_as::<_, WidgetType>(
        "SELECT id, type_key, config, updated_at
         FROM widget_types
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// ウィジェットタイプの設定を更新する。
pub async fn update_config(pool: &SqlitePool, type_key: &str, input: &WidgetTypeInput) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE widget_types
         SET config = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE type_key = ?",
    )
    .bind(&input.config)
    .bind(type_key)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}
