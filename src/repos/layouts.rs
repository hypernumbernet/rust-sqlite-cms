use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::layout::{Layout, LayoutInput};

/// 全レイアウトを key 順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Layout>> {
    Ok(sqlx::query_as::<_, Layout>(
        "SELECT id, key, name, created_at, updated_at
         FROM layouts
         ORDER BY key ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID で取得する。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Layout> {
    sqlx::query_as::<_, Layout>(
        "SELECT id, key, name, created_at, updated_at
         FROM layouts
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// ID から layout key のみ取得する（ファイル I/O 向けの軽量クエリ）。
pub async fn find_key_by_id(pool: &SqlitePool, id: i64) -> AppResult<String> {
    let row: (String,) = sqlx::query_as("SELECT key FROM layouts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.0)
}

/// key で取得する。
pub async fn find_by_key(pool: &SqlitePool, key: &str) -> AppResult<Option<Layout>> {
    Ok(sqlx::query_as::<_, Layout>(
        "SELECT id, key, name, created_at, updated_at
         FROM layouts
         WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?)
}

/// 初回トップページ作成や新規ページの初期レイアウトに使うレイアウトを取得する。
/// `default` key を優先し、なければ key 順の先頭を返す。
pub async fn find_bootstrap_layout(pool: &SqlitePool) -> AppResult<Layout> {
    if let Some(layout) = find_by_key(pool, "default").await? {
        return Ok(layout);
    }

    sqlx::query_as::<_, Layout>(
        "SELECT id, key, name, created_at, updated_at
         FROM layouts
         ORDER BY key ASC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// レイアウトを作成する。
pub async fn insert(pool: &SqlitePool, input: &LayoutInput) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO layouts (key, name)
         VALUES (?, ?)
         RETURNING id",
    )
    .bind(&input.key)
    .bind(&input.name)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// レイアウトを更新する。
pub async fn update(pool: &SqlitePool, id: i64, input: &LayoutInput) -> AppResult<()> {
    let _ = find(pool, id).await?;

    let result = sqlx::query(
        "UPDATE layouts
         SET key = ?,
             name = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.key)
    .bind(&input.name)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// レイアウトを削除する。所属ページがある場合は呼び出し側で拒否する。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM layouts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// レイアウトに紐づくページ件数。
pub async fn count_pages(pool: &SqlitePool, layout_id: i64) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pages WHERE layout_id = ?")
        .bind(layout_id)
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}