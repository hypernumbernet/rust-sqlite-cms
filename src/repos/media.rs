use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::media::{Media, MediaInput};

const MEDIA_LIST_SQL: &str = "
    SELECT
        p.id,
        p.title,
        pm_path.meta_value AS file_path,
        pm_mime.meta_value AS mime_type,
        pm_orig.meta_value AS original_name,
        pm_size.meta_value AS file_size,
        p.created_at,
        p.updated_at,
        pm_pub.meta_value AS public_url
    FROM posts p
    LEFT JOIN postmeta pm_path ON pm_path.post_id = p.id AND pm_path.meta_key = 'file_path'
    LEFT JOIN postmeta pm_mime ON pm_mime.post_id = p.id AND pm_mime.meta_key = 'mime_type'
    LEFT JOIN postmeta pm_orig ON pm_orig.post_id = p.id AND pm_orig.meta_key = 'original_name'
    LEFT JOIN postmeta pm_size ON pm_size.post_id = p.id AND pm_size.meta_key = 'file_size'
    LEFT JOIN postmeta pm_pub ON pm_pub.post_id = p.id AND pm_pub.meta_key = 'public_url'
    WHERE p.post_type = 'attachment' AND p.post_status != 'trash'
    ORDER BY datetime(p.created_at) DESC, p.id DESC
";

const MEDIA_FIND_SQL: &str = "
    SELECT
        p.id,
        p.title,
        pm_path.meta_value AS file_path,
        pm_mime.meta_value AS mime_type,
        pm_orig.meta_value AS original_name,
        pm_size.meta_value AS file_size,
        p.created_at,
        p.updated_at,
        pm_pub.meta_value AS public_url
    FROM posts p
    LEFT JOIN postmeta pm_path ON pm_path.post_id = p.id AND pm_path.meta_key = 'file_path'
    LEFT JOIN postmeta pm_mime ON pm_mime.post_id = p.id AND pm_mime.meta_key = 'mime_type'
    LEFT JOIN postmeta pm_orig ON pm_orig.post_id = p.id AND pm_orig.meta_key = 'original_name'
    LEFT JOIN postmeta pm_size ON pm_size.post_id = p.id AND pm_size.meta_key = 'file_size'
    LEFT JOIN postmeta pm_pub ON pm_pub.post_id = p.id AND pm_pub.meta_key = 'public_url'
    WHERE p.post_type = 'attachment' AND p.post_status != 'trash' AND p.id = ?
";

/// 全メディアを新しい順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Media>> {
    Ok(sqlx::query_as::<_, Media>(MEDIA_LIST_SQL)
        .fetch_all(pool)
        .await?)
}

/// ID でメディアを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Media> {
    Ok(sqlx::query_as::<_, Media>(MEDIA_FIND_SQL)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?)
}

/// 公開 URL でメディアを取得する。
pub async fn find_by_public_url(pool: &SqlitePool, public_url: &str) -> AppResult<Option<Media>> {
    Ok(sqlx::query_as::<_, Media>(
        "
        SELECT
            p.id,
            p.title,
            pm_path.meta_value AS file_path,
            pm_mime.meta_value AS mime_type,
            pm_orig.meta_value AS original_name,
            pm_size.meta_value AS file_size,
            p.created_at,
            p.updated_at,
            pm_pub.meta_value AS public_url
        FROM posts p
        INNER JOIN postmeta pm_pub ON pm_pub.post_id = p.id AND pm_pub.meta_key = 'public_url'
        LEFT JOIN postmeta pm_path ON pm_path.post_id = p.id AND pm_path.meta_key = 'file_path'
        LEFT JOIN postmeta pm_mime ON pm_mime.post_id = p.id AND pm_mime.meta_key = 'mime_type'
        LEFT JOIN postmeta pm_orig ON pm_orig.post_id = p.id AND pm_orig.meta_key = 'original_name'
        LEFT JOIN postmeta pm_size ON pm_size.post_id = p.id AND pm_size.meta_key = 'file_size'
        WHERE p.post_type = 'attachment' AND p.post_status != 'trash' AND pm_pub.meta_value = ?
        ",
    )
    .bind(public_url)
    .fetch_optional(pool)
    .await?)
}

/// 他メディアが同じ public_url を使っていないか確認する。
pub async fn ensure_public_url_available(
    pool: &SqlitePool,
    public_url: &str,
    exclude_media_id: Option<i64>,
) -> AppResult<()> {
    let existing: Option<(i64,)> = sqlx::query_as(
        "
        SELECT p.id
        FROM posts p
        INNER JOIN postmeta pm_pub ON pm_pub.post_id = p.id AND pm_pub.meta_key = 'public_url'
        WHERE p.post_type = 'attachment' AND p.post_status != 'trash'
          AND pm_pub.meta_value = ? AND p.id != ?
        ",
    )
    .bind(public_url)
    .bind(exclude_media_id.unwrap_or(-1))
    .fetch_optional(pool)
    .await?;

    if let Some((id,)) = existing {
        return Err(AppError::Conflict(format!(
            "公開 URL「{public_url}」は既に他のメディア（ID: {id}）で使われています"
        )));
    }

    Ok(())
}

/// メディア行と postmeta を作成する。
pub async fn insert(pool: &SqlitePool, input: &MediaInput) -> AppResult<i64> {
    let mut tx = pool.begin().await?;

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO posts (post_type, post_status, title)
         VALUES ('attachment', 'inherit', ?)
         RETURNING id",
    )
    .bind(&input.title)
    .fetch_one(&mut *tx)
    .await?;
    let id = row.0;

    insert_meta(&mut tx, id, "file_path", &input.file_path).await?;
    insert_meta(&mut tx, id, "mime_type", &input.mime_type).await?;
    insert_meta(&mut tx, id, "original_name", &input.original_name).await?;
    insert_meta(
        &mut tx,
        id,
        "file_size",
        &input.file_size.to_string(),
    )
    .await?;
    let default_public_url = format!("/uploads/{}", input.file_path);
    insert_meta(&mut tx, id, "public_url", &default_public_url).await?;

    tx.commit().await?;
    Ok(id)
}

/// メディアの公開 URL を更新する。
pub async fn update_public_url(pool: &SqlitePool, id: i64, public_url: &str) -> AppResult<()> {
    let _ = find(pool, id).await?;

    let updated = sqlx::query(
        "
        UPDATE postmeta
        SET meta_value = ?
        WHERE post_id = ? AND meta_key = 'public_url'
        ",
    )
    .bind(public_url)
    .bind(id)
    .execute(pool)
    .await?;

    if updated.rows_affected() == 0 {
        insert_meta_standalone(pool, id, "public_url", public_url).await?;
    }

    sqlx::query(
        "UPDATE posts SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

/// メディア行を削除する（postmeta は CASCADE）。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query(
        "DELETE FROM posts WHERE id = ? AND post_type = 'attachment'",
    )
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

async fn insert_meta(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    post_id: i64,
    meta_key: &str,
    meta_value: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)",
    )
    .bind(post_id)
    .bind(meta_key)
    .bind(meta_value)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_meta_standalone(
    pool: &SqlitePool,
    post_id: i64,
    meta_key: &str,
    meta_value: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)",
    )
    .bind(post_id)
    .bind(meta_key)
    .bind(meta_value)
    .execute(pool)
    .await?;
    Ok(())
}