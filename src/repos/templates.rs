use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::template::{Template, TemplateInput};

/// 管理画面向けに、全テンプレートを更新日時の新しい順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Template>> {
    Ok(sqlx::query_as::<_, Template>(
        "SELECT id, name, url_path, file_name, is_published, created_at, updated_at
         FROM templates
         ORDER BY datetime(updated_at) DESC, id DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID でテンプレートを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Template> {
    sqlx::query_as::<_, Template>(
        "SELECT id, name, url_path, file_name, is_published, created_at, updated_at
         FROM templates
         WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// 公開サイト向けに、公開済みかつ URL が一致するテンプレートを取得する。
pub async fn find_published_by_path(pool: &SqlitePool, path: &str) -> AppResult<Option<Template>> {
    Ok(sqlx::query_as::<_, Template>(
        "SELECT id, name, url_path, file_name, is_published, created_at, updated_at
         FROM templates
         WHERE is_published = 1 AND url_path = ?",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?)
}

/// メタ情報を作成し、`id` から確定したファイル名（`page-{id}.html`）を
/// 行へ反映して `(id, file_name)` を返す。本文ファイルの書き込みは呼び出し側で行う。
/// `url_path` が他と衝突する場合は `Conflict`。
pub async fn insert(pool: &SqlitePool, input: &TemplateInput) -> AppResult<(i64, String)> {
    ensure_path_available(pool, input.url_path.as_deref(), None).await?;

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO templates (name, url_path, is_published)
         VALUES (?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(input.is_published)
    .fetch_one(pool)
    .await?;

    let id = row.0;
    let file_name = format!("page-{id}.html");

    sqlx::query("UPDATE templates SET file_name = ? WHERE id = ?")
        .bind(&file_name)
        .bind(id)
        .execute(pool)
        .await?;

    Ok((id, file_name))
}

/// テンプレートのメタ情報を更新する。`url_path` が他と衝突する場合は `Conflict`。
pub async fn update(pool: &SqlitePool, id: i64, input: &TemplateInput) -> AppResult<()> {
    ensure_path_available(pool, input.url_path.as_deref(), Some(id)).await?;

    let result = sqlx::query(
        "UPDATE templates
         SET name = ?,
             url_path = ?,
             is_published = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.url_path)
    .bind(input.is_published)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// テンプレートのメタ行を削除する。本文ファイルの削除は呼び出し側で行う。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// 指定 URL が他のテンプレートに使われていないか確認する。
/// `exclude_id` は更新時に自身を除外するために使う。
async fn ensure_path_available(
    pool: &SqlitePool,
    url_path: Option<&str>,
    exclude_id: Option<i64>,
) -> AppResult<()> {
    let Some(path) = url_path else {
        return Ok(());
    };

    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM templates WHERE url_path = ? AND id != ?")
            .bind(path)
            .bind(exclude_id.unwrap_or(-1))
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "URL「{path}」は既に他のテンプレートで使われています"
        )));
    }

    Ok(())
}
