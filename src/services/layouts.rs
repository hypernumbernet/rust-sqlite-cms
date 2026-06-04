//! レイアウト管理サービス。

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::{AppError, DomainResult};
use crate::models::layout::{Layout, LayoutInput};
use crate::repos::{layouts as layouts_repo, media as media_repo};
use crate::theme;

/// 全レイアウト一覧。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<Layout>> {
    layouts_repo::list_all(pool).await.map_err(Into::into)
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> DomainResult<Layout> {
    layouts_repo::find(pool, id).await.map_err(Into::into)
}

/// 既定レイアウトを取得。
pub async fn find_default(pool: &SqlitePool) -> DomainResult<Layout> {
    layouts_repo::find_default(pool).await.map_err(Into::into)
}

/// 新規レイアウト作成（DB + ディレクトリ + 空 shell）。
pub async fn create_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    input: &LayoutInput,
    shell_content: &str,
) -> DomainResult<i64> {
    validate_layout_key(&input.key)?;
    validate_favicon_media(pool, input.favicon_media_id).await?;

    if layouts_repo::find_by_key(pool, &input.key).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "レイアウト key「{}」は既に使われています",
            input.key
        ))
        .into());
    }

    let id = layouts_repo::insert(pool, input).await?;
    theme::ensure_layout_dirs(&config.paths.work_dir, &input.key)?;
    theme::write_shell(&config.paths.work_dir, &input.key, shell_content)?;

    Ok(id)
}

/// レイアウト更新（メタ + shell 本文）。
pub async fn update_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
    input: &LayoutInput,
    shell_content: &str,
) -> DomainResult<()> {
    validate_layout_key(&input.key)?;
    validate_favicon_media(pool, input.favicon_media_id).await?;

    let current = layouts_repo::find(pool, id).await?;
    if current.key != input.key {
        if layouts_repo::find_by_key(pool, &input.key).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "レイアウト key「{}」は既に使われています",
                input.key
            ))
            .into());
        }
        theme::rename_layout_dir(&config.paths.work_dir, &current.key, &input.key)?;
    }

    layouts_repo::update(pool, id, input).await?;
    theme::write_shell(&config.paths.work_dir, &input.key, shell_content)?;

    Ok(())
}

/// レイアウト削除。所属ページがある場合は拒否。
pub async fn delete_layout(pool: &SqlitePool, config: &AppConfig, id: i64) -> DomainResult<()> {
    let layout = layouts_repo::find(pool, id).await?;

    if layout.is_default {
        return Err(AppError::Conflict("既定レイアウトは削除できません".to_string()).into());
    }

    let count = layouts_repo::count_pages(pool, id).await?;
    if count > 0 {
        return Err(AppError::Conflict(format!(
            "このレイアウトに属するページが {count} 件あるため削除できません"
        ))
        .into());
    }

    layouts_repo::delete(pool, id).await?;
    theme::remove_layout_dir(&config.paths.work_dir, &layout.key)?;

    Ok(())
}

/// レイアウトに設定された favicon の公開 URL。未設定・無効参照は `None`。
pub async fn favicon_url_for_layout(pool: &SqlitePool, layout_id: i64) -> Option<String> {
    let layout = layouts_repo::find(pool, layout_id).await.ok()?;
    let media_id = layout.favicon_media_id?;
    let media = media_repo::find(pool, media_id).await.ok()?;
    if media.is_favicon_suitable() {
        Some(media.public_url())
    } else {
        None
    }
}

async fn validate_favicon_media(pool: &SqlitePool, media_id: Option<i64>) -> DomainResult<()> {
    let Some(id) = media_id else {
        return Ok(());
    };
    let media = media_repo::find(pool, id).await?;
    if media.is_favicon_suitable() {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "favicon には画像または .ico 形式のメディアを選択してください".into(),
        )
        .into())
    }
}

fn validate_layout_key(key: &str) -> DomainResult<()> {
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !key.starts_with('_');

    if valid {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "レイアウト key は英数字・ハイフン・アンダースコアのみ（先頭に _ は不可）".into(),
        )
        .into())
    }
}
