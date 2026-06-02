use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::post::{Post, PostInput};

/// 指定プレースホルダー配下の投稿を、公開状態にかかわらず新しい順で取得する。
pub async fn list_all_for_placeholder(pool: &SqlitePool, placeholder_id: i64) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status != 'trash' AND placeholder_id = ?
         ORDER BY datetime(COALESCE(published_at, created_at)) DESC, id DESC",
    )
    .bind(placeholder_id)
    .fetch_all(pool)
    .await?)
}

/// 指定プレースホルダー配下の公開済み投稿を新しい順で取得する。
pub async fn list_published_for_placeholder(
    pool: &SqlitePool,
    placeholder_id: i64,
    limit: i64,
) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status = 'publish' AND placeholder_id = ?
         ORDER BY datetime(COALESCE(published_at, created_at)) DESC, id DESC
         LIMIT ?",
    )
    .bind(placeholder_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// 指定プレースホルダー配下の公開済み投稿を menu_order 順で取得する。
pub async fn list_published_for_placeholder_ordered(
    pool: &SqlitePool,
    placeholder_id: i64,
    limit: i64,
) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status = 'publish' AND placeholder_id = ?
         ORDER BY menu_order ASC, id ASC
         LIMIT ?",
    )
    .bind(placeholder_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// ID でお知らせを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Post> {
    sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE id = ? AND post_type = 'post'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 指定プレースホルダー配下の投稿を取得する。
pub async fn find_in_placeholder(
    pool: &SqlitePool,
    placeholder_id: i64,
    id: i64,
) -> AppResult<Post> {
    sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE id = ? AND post_type = 'post' AND placeholder_id = ?",
    )
    .bind(id)
    .bind(placeholder_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// お知らせを作成する。
pub async fn insert(pool: &SqlitePool, input: &PostInput) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO posts (
            post_type, placeholder_id, post_status, post_name, title, content, excerpt, published_at
         ) VALUES (
            'post', ?, ?, ?, ?, ?, ?,
            CASE WHEN ? = 'publish' THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ELSE NULL END
         )
         RETURNING id",
    )
    .bind(input.placeholder_id)
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
         WHERE id = ? AND post_type = 'post' AND placeholder_id = ?",
    )
    .bind(&input.post_status)
    .bind(&input.post_name)
    .bind(&input.title)
    .bind(&input.content)
    .bind(&input.excerpt)
    .bind(&input.post_status)
    .bind(&input.post_status)
    .bind(id)
    .bind(input.placeholder_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ゴミ箱内の投稿を新しい順で取得する（全プレースホルダー横断）。
pub async fn list_trashed(pool: &SqlitePool) -> AppResult<Vec<Post>> {
    Ok(sqlx::query_as::<_, Post>(
        "SELECT id, placeholder_id, post_status, post_name, title, content, excerpt, published_at, created_at, updated_at
         FROM posts
         WHERE post_type = 'post' AND post_status = 'trash'
         ORDER BY datetime(updated_at) DESC, id DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// ゴミ箱から復元する。`published_at` があれば公開、なければ下書きに戻す。
pub async fn restore(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE posts
         SET post_status = CASE WHEN published_at IS NOT NULL THEN 'publish' ELSE 'draft' END,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ? AND post_type = 'post' AND post_status = 'trash'",
    )
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ゴミ箱内の投稿を物理削除する（postmeta は CASCADE）。
pub async fn purge(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query(
        "DELETE FROM posts WHERE id = ? AND post_type = 'post' AND post_status = 'trash'",
    )
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ソフト削除（post_status を 'trash' に更新）。ID指定（API / グローバル用途）。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE posts SET post_status = 'trash', updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ? AND post_type = 'post'"
    )
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// ソフト削除（post_status を 'trash' に更新）。プレースホルダー配下であることを検証。
pub async fn delete_in_placeholder(
    pool: &SqlitePool,
    placeholder_id: i64,
    id: i64,
) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE posts SET post_status = 'trash', updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ? AND post_type = 'post' AND placeholder_id = ?"
    )
    .bind(id)
    .bind(placeholder_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
