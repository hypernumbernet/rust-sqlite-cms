use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::widget::{WidgetType, WidgetTypeInput};

/// 全ウィジェットタイプを type_key 順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<WidgetType>> {
    Ok(sqlx::query_as::<_, WidgetType>(
        "SELECT id, type_key, config, html_template, config_schema, updated_at
         FROM widget_types
         ORDER BY type_key ASC, id ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// type_key でウィジェットタイプを取得する。存在しなければ `NotFound`。
pub async fn find_by_key(pool: &SqlitePool, type_key: &str) -> AppResult<WidgetType> {
    sqlx::query_as::<_, WidgetType>(
        "SELECT id, type_key, config, html_template, config_schema, updated_at
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
        "SELECT id, type_key, config, html_template, config_schema, updated_at
         FROM widget_types
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// ウィジェットタイプの設定（config + html_template）を更新する。
/// ウィジェット画面のHTML編集とインスタンス設定移行に対応。
pub async fn update_config(pool: &SqlitePool, type_key: &str, input: &WidgetTypeInput) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE widget_types
         SET config = ?,
             html_template = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE type_key = ?",
    )
    .bind(&input.config)
    .bind(&input.html_template)
    .bind(type_key)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ウィジェットタイプを更新する（type_key の変更 + html_template + config）。
/// type_key を変更する場合、old_type_key で検索し new_type_key に更新する。
pub async fn update(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE widget_types
         SET type_key = ?,
             html_template = ?,
             config = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE type_key = ?",
    )
    .bind(new_type_key)
    .bind(html_template)
    .bind(config)
    .bind(old_type_key)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ウィジェットタイプを更新（config_schema も含む拡張版）。
pub async fn update_with_schema(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
    config_schema: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE widget_types
         SET type_key = ?,
             html_template = ?,
             config = ?,
             config_schema = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE type_key = ?",
    )
    .bind(new_type_key)
    .bind(html_template)
    .bind(config)
    .bind(config_schema)
    .bind(old_type_key)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}
