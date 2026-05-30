use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::page::{Page, PageInput};
use crate::repos::url_paths;

const INDEX_FILE_NAME: &str = "index.html";

/// 管理画面向けに、全ページを取得する（トップを先頭、以降は更新日時の新しい順）。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT id, name, url_path, file_name, is_static, is_published, created_at, updated_at
         FROM pages
         ORDER BY CASE WHEN file_name = 'index.html' THEN 0 ELSE 1 END,
                  datetime(updated_at) DESC,
                  id DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID でページを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Page> {
    sqlx::query_as::<_, Page>(
        "SELECT id, name, url_path, file_name, is_static, is_published, created_at, updated_at
         FROM pages
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// ファイル名でページを取得する。
pub async fn find_by_file_name(pool: &SqlitePool, file_name: &str) -> AppResult<Option<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT id, name, url_path, file_name, is_static, is_published, created_at, updated_at
         FROM pages
         WHERE file_name = ?",
    )
    .bind(file_name)
    .fetch_optional(pool)
    .await?)
}

/// 公開サイト向けに、公開済みかつ URL が一致するページを取得する。
pub async fn find_published_by_path(pool: &SqlitePool, path: &str) -> AppResult<Option<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT id, name, url_path, file_name, is_static, is_published, created_at, updated_at
         FROM pages
         WHERE is_published = 1 AND url_path = ?",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?)
}

/// トップページ（`index.html`）の DB 行が無ければ作成する。
pub async fn ensure_index_page(pool: &SqlitePool) -> AppResult<()> {
    if find_by_file_name(pool, INDEX_FILE_NAME).await?.is_some() {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO pages (name, file_name, is_static, is_published)
         VALUES ('トップページ', ?, 0, 1)",
    )
    .bind(INDEX_FILE_NAME)
    .execute(pool)
    .await?;

    Ok(())
}

/// メタ情報を作成し、`id` から確定したファイル名（`page-{id}.html`）を
/// 行へ反映して `(id, file_name)` を返す。本文ファイルの書き込みは呼び出し側で行う。
pub async fn insert(pool: &SqlitePool, input: &PageInput) -> AppResult<(i64, String)> {
    url_paths::ensure_url_available(pool, input.url_path.as_deref(), None).await?;

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO pages (name, url_path, is_static, is_published)
         VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(input.is_static)
    .bind(input.is_published)
    .fetch_one(pool)
    .await?;

    let id = row.0;
    let file_name = format!("page-{id}.html");

    sqlx::query("UPDATE pages SET file_name = ? WHERE id = ?")
        .bind(&file_name)
        .bind(id)
        .execute(pool)
        .await?;

    Ok((id, file_name))
}

/// ページのメタ情報を更新する。
pub async fn update(pool: &SqlitePool, id: i64, input: &PageInput) -> AppResult<()> {
    let page = find(pool, id).await?;

    if page.is_home() {
        let result = sqlx::query(
            "UPDATE pages
             SET name = ?,
                 is_static = ?,
                 is_published = ?,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?",
        )
        .bind(&input.name)
        .bind(input.is_static)
        .bind(input.is_published)
        .bind(id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        return Ok(());
    }

    url_paths::ensure_url_available(pool, input.url_path.as_deref(), Some(id)).await?;

    let result = sqlx::query(
        "UPDATE pages
         SET name = ?,
             url_path = ?,
             is_static = ?,
             is_published = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(input.is_static)
    .bind(input.is_published)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// ページのメタ行を削除する。本文ファイルの削除は呼び出し側で行う。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let page = find(pool, id).await?;

    if page.is_home() {
        return Err(AppError::Conflict(
            "トップページ（index.html）は削除できません".to_string(),
        ));
    }

    let result = sqlx::query("DELETE FROM pages WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}
