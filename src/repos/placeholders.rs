use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::placeholder::{Placeholder, PlaceholderInput};

/// 全プレースホルダーを名前順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Placeholder>> {
    Ok(sqlx::query_as::<_, Placeholder>(
        "SELECT id, name, widget_type_id, config, created_at, updated_at
         FROM placeholders
         ORDER BY name ASC, id ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID でプレースホルダーを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Placeholder> {
    sqlx::query_as::<_, Placeholder>(
        "SELECT id, name, widget_type_id, config, created_at, updated_at
         FROM placeholders
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// プレースホルダーを作成する。
pub async fn insert(pool: &SqlitePool, input: &PlaceholderInput) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO placeholders (name, widget_type_id, config)
         VALUES (?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(input.widget_type_id)
    .bind(&input.config)
    .fetch_one(pool)
    .await
    .map_err(map_unique_violation)?;

    Ok(row.0)
}

/// プレースホルダーを更新する。
pub async fn update(pool: &SqlitePool, id: i64, input: &PlaceholderInput) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE placeholders
         SET name = ?,
             widget_type_id = ?,
             config = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(input.widget_type_id)
    .bind(&input.config)
    .bind(id)
    .execute(pool)
    .await
    .map_err(map_unique_violation)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// プレースホルダーを削除する。配下に投稿がある場合は `Conflict`。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts
         WHERE placeholder_id = ? AND post_type = 'post' AND post_status != 'trash'",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    if count.0 > 0 {
        return Err(AppError::Conflict(
            "投稿が紐付いているプレースホルダーは削除できません".to_string(),
        ));
    }

    let result = sqlx::query("DELETE FROM placeholders WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

fn map_unique_violation(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.is_unique_violation()
    {
        return AppError::Conflict("このプレースホルダー名は既に使用されています".to_string());
    }
    AppError::Database(err)
}
