//! メディア管理サービス（アップロード/一覧/削除）。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::media;
use crate::models::media::Media;
use crate::repos::media as media_repo;

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
