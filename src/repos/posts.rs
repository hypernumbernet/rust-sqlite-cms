use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::post::{Post, PostInput};

/// 管理画面向けに、お知らせを公開状態にかかわらず新しい順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status != 'trash'
         ORDER BY datetime(COALESCE(published_at, created_at)) DESC, id DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// 公開サイト向けに、公開済みのお知らせだけを新しい順で取得する。
pub async fn list_published(pool: &SqlitePool, limit: i64) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status = 'publish'
         ORDER BY datetime(COALESCE(published_at, created_at)) DESC, id DESC
         LIMIT ?",
    )
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// ID でお知らせを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Post> {
    sqlx::query_as::<_, Post>(
        "SELECT id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE id = ? AND post_type = 'post'",
    )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

/// お知らせを作成する。
pub async fn insert(pool: &SqlitePool, input: &PostInput) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO posts (
            post_type, post_status, post_name, title, content, excerpt, published_at
         ) VALUES (
            'post', ?, ?, ?, ?, ?,
            CASE WHEN ? = 'publish' THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ELSE NULL END
         )
         RETURNING id",
    )
    .bind(&input.post_status)
    .bind(&input.post_name)
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.excerpt)
    .bind(&input.post_status)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// お知らせを更新する。
pub async fn update(pool: &SqlitePool, id: i64, input: &PostInput) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE posts
         SET post_status = ?,
             post_name = ?,
             title = ?,
             content = ?,
             excerpt = ?,
             published_at = CASE
                 WHEN ? = 'publish' AND published_at IS NULL
                     THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                 WHEN ? = 'draft'
                     THEN NULL
                 ELSE published_at
             END,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ? AND post_type = 'post'",
    )
    .bind(&input.post_status)
    .bind(&input.post_name)
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.excerpt)
    .bind(&input.post_status)
    .bind(&input.post_status)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}
