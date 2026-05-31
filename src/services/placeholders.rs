//! プレースホルダー管理サービス。

use sqlx::SqlitePool;

use crate::error::{DomainError, DomainResult};
use crate::models::placeholder::{Placeholder, PlaceholderInput};
use crate::repos::placeholders as placeholders_repo;

/// 全プレースホルダーを取得。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<Placeholder>> {
    placeholders_repo::list_all(pool).await.map_err(Into::into)
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> DomainResult<Placeholder> {
    placeholders_repo::find(pool, id).await.map_err(Into::into)
}

/// 新規作成（バリデーション込み）。
pub async fn create(pool: &SqlitePool, input: PlaceholderInput) -> DomainResult<Placeholder> {
    crate::models::placeholder::validate_name(&input.name)
        .map_err(DomainError::Conflict)?;

    let id = placeholders_repo::insert(pool, &input).await?;
    placeholders_repo::find(pool, id).await.map_err(Into::into)
}

/// 更新。
pub async fn update(pool: &SqlitePool, id: i64, input: PlaceholderInput) -> DomainResult<()> {
    placeholders_repo::update(pool, id, &input)
        .await
        .map_err(Into::into)
}

/// 削除。
pub async fn delete(pool: &SqlitePool, id: i64) -> DomainResult<()> {
    placeholders_repo::delete(pool, id).await.map_err(Into::into)
}
