//! ページ管理サービス。DB メタ + ファイル本文（theme 経由）の整合性を保証。

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::DomainResult;
use crate::models::page::{Page, PageInput};
use crate::repos::{layouts as layouts_repo, pages as pages_repo};
use crate::theme;

/// 全ページ一覧（管理用）。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<Page>> {
    pages_repo::list_all(pool).await.map_err(Into::into)
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> DomainResult<Page> {
    pages_repo::find(pool, id).await.map_err(Into::into)
}

/// 新規作成。
pub async fn create_page(
    pool: &SqlitePool,
    config: &AppConfig,
    input: &PageInput,
) -> DomainResult<(i64, String)> {
    let layout_key = layouts_repo::find_key_by_id(pool, input.layout_id).await?;
    let (id, file_name) = pages_repo::insert(pool, input).await?;

    if let Err(err) = theme::write_page_body(
        &config.paths.work_dir,
        &layout_key,
        &file_name,
        &input.content,
    ) {
        let _ = pages_repo::delete(pool, id).await;
        return Err(err.into());
    }

    Ok((id, file_name))
}

/// ページ更新。
pub async fn update_page(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
    input: &PageInput,
) -> DomainResult<()> {
    let page = pages_repo::find(pool, id).await?;
    let new_layout_key = layouts_repo::find_key_by_id(pool, input.layout_id).await?;

    pages_repo::update(pool, id, input).await?;

    theme::write_page_body(
        &config.paths.work_dir,
        &new_layout_key,
        &page.file_name,
        &input.content,
    )?;

    if page.layout_id != input.layout_id {
        let _ = theme::remove_page_body(
            &config.paths.work_dir,
            &page.layout_key,
            &page.file_name,
        );
    }

    Ok(())
}

/// ページ削除。
pub async fn delete_page(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
) -> DomainResult<()> {
    let page = pages_repo::find(pool, id).await?;

    let _ = theme::remove_page_body(
        &config.paths.work_dir,
        &page.layout_key,
        &page.file_name,
    );
    pages_repo::delete(pool, id).await.map_err(Into::into)
}

/// 公開サイト向けに公開済みページをパスで取得する。
pub async fn find_published_for_render(pool: &SqlitePool, path: &str) -> DomainResult<Option<Page>> {
    pages_repo::find_published_by_path(pool, path)
        .await
        .map_err(Into::into)
}
