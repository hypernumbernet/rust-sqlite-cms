//! メディア管理サービス（アップロード/一覧/削除/公開 URL）。

use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::media;
use crate::models::media::Media;
use crate::repos::{media as media_repo, url_paths};
use crate::routes::url;

const FAVICON_PATH: &str = "/favicon.ico";

/// 全メディア一覧。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<Media>> {
    media_repo::list_all(pool).await
}

/// ファイルアップロード処理（データを受け取り保存 + DB 登録）。
pub async fn upload(
    pool: &SqlitePool,
    uploads_root: &str,
    original_name: &str,
    data: &[u8],
) -> AppResult<()> {
    let (file_path, mime_type) = media::save_upload(uploads_root, original_name, data)?;

    let input = crate::models::media::MediaInput {
        title: original_name.to_string(),
        file_path,
        mime_type,
        original_name: original_name.to_string(),
        file_size: data.len() as i64,
    };

    media_repo::insert(pool, &input).await?;
    Ok(())
}

/// 削除（ファイル + DB）。
pub async fn delete(pool: &SqlitePool, uploads_root: &str, id: i64) -> AppResult<()> {
    let item = media_repo::find(pool, id).await?;

    if let Some(file_path) = item.file_path.as_deref() {
        media::delete_file(uploads_root, file_path)?;
    }

    media_repo::delete(pool, id).await
}

/// 公開 URL を更新する。
pub async fn update_public_url(pool: &SqlitePool, id: i64, raw_url: &str) -> AppResult<()> {
    let media = media_repo::find(pool, id).await?;
    let public_url = normalize_and_validate_public_url(raw_url)?;

    if public_url == "/favicon.ico" && !media.is_favicon_suitable() {
        return Err(AppError::Conflict(
            "favicon には画像または .ico 形式のメディアを選択してください".into(),
        ));
    }

    media_repo::ensure_public_url_available(pool, &public_url, Some(id)).await?;
    url_paths::ensure_url_available(pool, Some(&public_url), None).await?;
    media_repo::update_public_url(pool, id, &public_url).await
}

/// サイト favicon の公開 URL。`/favicon.ico` に割り当てられたメディアがあれば返す。
pub async fn site_favicon_url(pool: &SqlitePool) -> Option<String> {
    media_repo::find_by_public_url(pool, FAVICON_PATH)
        .await
        .ok()
        .flatten()
        .map(|_| FAVICON_PATH.to_string())
}

fn normalize_and_validate_public_url(raw: &str) -> AppResult<String> {
    let Some(path) = url::normalize_url_path(raw) else {
        return Err(AppError::Conflict("公開 URL を入力してください".into()));
    };

    if is_reserved_media_public_url(&path) {
        return Err(AppError::Conflict(format!(
            "公開 URL「{path}」はシステム予約のため使用できません"
        )));
    }

    Ok(path)
}

fn is_reserved_media_public_url(path: &str) -> bool {
    path == "/"
        || path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
        || path == "/api"
        || path.starts_with("/api/")
}