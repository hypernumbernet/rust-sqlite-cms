use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::error::AppResult;

/// 指定 post の meta_key に対応する値を取得する。
pub async fn get(pool: &SqlitePool, post_id: i64, meta_key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT meta_value FROM postmeta WHERE post_id = ? AND meta_key = ?",
    )
    .bind(post_id)
    .bind(meta_key)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(value,)| value))
}

/// 指定 post の meta_key を upsert する。
pub async fn set(pool: &SqlitePool, post_id: i64, meta_key: &str, meta_value: &str) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE postmeta SET meta_value = ? WHERE post_id = ? AND meta_key = ?",
    )
    .bind(meta_value)
    .bind(post_id)
    .bind(meta_key)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)",
        )
        .bind(post_id)
        .bind(meta_key)
        .bind(meta_value)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// 指定 post に複数の meta を upsert する。
pub async fn set_many(pool: &SqlitePool, post_id: i64, values: &HashMap<String, String>) -> AppResult<()> {
    for (key, value) in values {
        set(pool, post_id, key, value).await?;
    }
    Ok(())
}

/// 指定 post の postmeta 行をすべて削除する。
pub async fn delete_for_post(pool: &SqlitePool, post_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM postmeta WHERE post_id = ?")
        .bind(post_id)
        .execute(pool)
        .await?;
    Ok(())
}
