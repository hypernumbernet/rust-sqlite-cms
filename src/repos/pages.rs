use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::page::{Page, PageInput};
use crate::repos::{layouts, url_paths};

const HOME_FILE_NAME: &str = "pages/index.html";

/// 指定レイアウトに所属するページを取得する。
pub async fn list_by_layout(pool: &SqlitePool, layout_id: i64) -> AppResult<Vec<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         WHERE p.layout_id = ?
         ORDER BY CASE WHEN p.url_path = '/' THEN 0 ELSE 1 END,
                  datetime(p.updated_at) DESC,
                  p.id DESC",
    )
    .bind(layout_id)
    .fetch_all(pool)
    .await?)
}

/// 管理画面向けに、全ページを取得する（トップを先頭、以降は更新日時の新しい順）。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         ORDER BY CASE WHEN p.url_path = '/' THEN 0 ELSE 1 END,
                  datetime(p.updated_at) DESC,
                  p.id DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID でページを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Page> {
    sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         WHERE p.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 公開トップページを取得する。
pub async fn find_home(pool: &SqlitePool) -> AppResult<Option<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         WHERE p.url_path = '/'",
    )
    .fetch_optional(pool)
    .await?)
}

/// レイアウト内のファイル名でページを取得する。
pub async fn find_by_layout_file(
    pool: &SqlitePool,
    layout_id: i64,
    file_name: &str,
) -> AppResult<Option<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         WHERE p.layout_id = ? AND p.file_name = ?",
    )
    .bind(layout_id)
    .bind(file_name)
    .fetch_optional(pool)
    .await?)
}

/// 公開サイト向けに、公開済みかつ URL が一致するページを取得する。
pub async fn find_published_by_path(pool: &SqlitePool, path: &str) -> AppResult<Option<Page>> {
    Ok(sqlx::query_as::<_, Page>(
        "SELECT p.id, p.layout_id, p.name, p.url_path, p.file_name, p.is_published,
         p.created_at, p.updated_at, l.key AS layout_key
         FROM pages p
         INNER JOIN layouts l ON l.id = p.layout_id
         WHERE p.is_published = 1 AND p.url_path = ?",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?)
}

/// トップページの DB 行が無ければ作成する。
pub async fn ensure_index_page(pool: &SqlitePool) -> AppResult<()> {
    if find_home(pool).await?.is_some() {
        return Ok(());
    }

    let default = layouts::find_default(pool).await?;

    sqlx::query(
        "INSERT INTO pages (name, url_path, file_name, layout_id, is_published)
         VALUES ('トップページ', '/', ?, ?, 1)",
    )
    .bind(HOME_FILE_NAME)
    .bind(default.id)
    .execute(pool)
    .await?;

    Ok(())
}

/// 指定した `file_name` でページメタを作成する（インポート用）。
/// 本文ファイルの書き込みは呼び出し側で行う。
pub async fn insert_with_file_name(
    pool: &SqlitePool,
    input: &PageInput,
    file_name: &str,
) -> AppResult<i64> {
    url_paths::ensure_url_available(pool, input.url_path.as_deref(), None).await?;

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO pages (name, url_path, file_name, layout_id, is_published)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(file_name)
    .bind(input.layout_id)
    .bind(input.is_published)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// メタ情報を作成し、`id` から確定したファイル名（`pages/page-{id}.html`）を
/// 行へ反映して `(id, file_name)` を返す。本文ファイルの書き込みは呼び出し側で行う。
pub async fn insert(pool: &SqlitePool, input: &PageInput) -> AppResult<(i64, String)> {
    url_paths::ensure_url_available(pool, input.url_path.as_deref(), None).await?;

    let mut tx = pool.begin().await?;
    let temp_file = format!(
        "pages/.new-{}",
        std::time::SystemTime::now()
            .elapsed()
            .unwrap_or_default()
            .as_nanos()
    );

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO pages (name, url_path, file_name, layout_id, is_published)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(&temp_file)
    .bind(input.layout_id)
    .bind(input.is_published)
    .fetch_one(&mut *tx)
    .await?;

    let id = row.0;
    let file_name = format!("pages/page-{id}.html");

    sqlx::query("UPDATE pages SET file_name = ? WHERE id = ?")
        .bind(&file_name)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok((id, file_name))
}

/// ページのメタ情報を更新する。
pub async fn update(pool: &SqlitePool, id: i64, input: &PageInput) -> AppResult<()> {
    let page = find(pool, id).await?;

    if page.is_home() {
        let result = sqlx::query(
            "UPDATE pages
             SET name = ?,
                 layout_id = ?,
                 is_published = ?,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?",
        )
        .bind(&input.name)
        .bind(input.layout_id)
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
             layout_id = ?,
             is_published = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(input.layout_id)
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
            "トップページは削除できません".to_string(),
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
