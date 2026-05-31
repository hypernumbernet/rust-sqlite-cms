//! プレースホルダー管理サービス。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::placeholder::{Placeholder, PlaceholderInput};
use crate::repos::placeholders as placeholders_repo;

/// 全プレースホルダーを取得。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Placeholder>> {
    placeholders_repo::list_all(pool).await
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Placeholder> {
    placeholders_repo::find(pool, id).await
}

/// 新規作成（バリデーション込み）。
pub async fn create(pool: &SqlitePool, input: PlaceholderInput) -> AppResult<Placeholder> {
    crate::models::placeholder::validate_name(&input.name)
        .map_err(|msg| crate::error::AppError::Conflict(msg))?;

    let id = placeholders_repo::insert(pool, &input).await?;
    placeholders_repo::find(pool, id).await
}

/// 更新。
pub async fn update(pool: &SqlitePool, id: i64, input: PlaceholderInput) -> AppResult<()> {
    placeholders_repo::update(pool, id, &input).await
}

/// 削除。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    placeholders_repo::delete(pool, id).await
}
