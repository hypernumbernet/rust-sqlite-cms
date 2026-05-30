use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

/// 指定 URL が他のページに使われていないか確認する。
/// `exclude_page_id` は更新時に自身を除外するために使う。
pub async fn ensure_url_available(
    pool: &SqlitePool,
    url_path: Option<&str>,
    exclude_page_id: Option<i64>,
) -> AppResult<()> {
    let Some(path) = url_path else {
        return Ok(());
    };

    let page: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM pages WHERE url_path = ? AND id != ?")
            .bind(path)
            .bind(exclude_page_id.unwrap_or(-1))
            .fetch_optional(pool)
            .await?;

    if let Some((id,)) = page {
        return Err(AppError::Conflict(format!(
            "URL「{path}」は既に他のページ（ID: {id}）で使われています"
        )));
    }

    Ok(())
}
