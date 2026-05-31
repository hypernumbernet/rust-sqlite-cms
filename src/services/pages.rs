//! ページ管理サービス。DB メタ + ファイル本文（theme 経由）の整合性を保証。
//!
//! 注: フォームバリデーション（予約URL、公開時のURL必須など）の多くは
//! 呼び出し側（HTMLフォーム or API DTO）で行い、ここでは DB+ファイルの
//! トランザクション的整合性に集中する。

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::AppResult;
use crate::models::page::{Page, PageInput};
use crate::repos::pages as pages_repo;
use crate::theme;

/// 全ページ一覧（管理用）。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Page>> {
    pages_repo::list_all(pool).await
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<Page> {
    pages_repo::find(pool, id).await
}

/// ファイル名で取得。
pub async fn find_by_file_name(pool: &SqlitePool, file_name: &str) -> AppResult<Option<Page>> {
    pages_repo::find_by_file_name(pool, file_name).await
}

/// 新規作成。
///
/// - DB にメタを登録（`pages::insert` が file_name を確定）
/// - 対応する本文ファイルを書き込み
/// - 書き込み失敗時は DB 行を補償削除
pub async fn create_page(
    pool: &SqlitePool,
    config: &AppConfig,
    input: &PageInput,
) -> AppResult<(i64, String)> {
    let (id, file_name) = pages_repo::insert(pool, input).await?;

    if let Err(err) = theme::write_page_content(
        &config.paths.work_dir,
        &file_name,
        input.is_static,
        &input.content,
    ) {
        // ベストエフォートで補償削除
        let _ = pages_repo::delete(pool, id).await;
        return Err(err.into());
    }

    Ok((id, file_name))
}

/// ページ更新。
///
/// - メタ更新（ホームページは url_path などを無視する特別扱いは repo 側）
/// - is_static 変更時は古いファイルを削除してから新しい種別で書き込み
pub async fn update_page(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
    input: &PageInput,
) -> AppResult<()> {
    let page = pages_repo::find(pool, id).await?;
    let old_is_static = page.is_static;
    let old_file_name = page.file_name.clone().unwrap_or_else(|| format!("page-{id}.html"));

    pages_repo::update(pool, id, input).await?;

    if old_is_static != input.is_static {
        // 種別変更時は古いファイルを削除（失敗は無視して続行）
        let _ = theme::remove_page_content(&config.paths.work_dir, &old_file_name, old_is_static);
    }

    theme::write_page_content(
        &config.paths.work_dir,
        &old_file_name,
        input.is_static,
        &input.content,
    )?;

    Ok(())
}

/// ページ削除。
///
/// - ホームページ削除は repo 側で拒否される
/// - 本文ファイルを削除してから DB 行を削除
pub async fn delete_page(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
) -> AppResult<()> {
    let page = pages_repo::find(pool, id).await?;

    // まずファイルを削除（無くても気にしない）
    if let Some(ref file_name) = page.file_name {
        let _ = theme::remove_page_content(&config.paths.work_dir, file_name, page.is_static);
    }

    pages_repo::delete(pool, id).await
}

/// 公開サイト向けにページを取得してレンダリングできる状態で返す（既存 page_render と共用しやすい形）。
pub async fn find_published_for_render(pool: &SqlitePool, path: &str) -> AppResult<Option<Page>> {
    pages_repo::find_published_by_path(pool, path).await
}
