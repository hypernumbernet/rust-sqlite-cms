//! 投稿（エントリ）管理サービス。プレースホルダー配下の posts + postmeta を扱う。

use sqlx::SqlitePool;

use crate::error::DomainResult;
use crate::models::post::{Post, PostInput};
use crate::repos::posts as posts_repo;

/// 指定プレースホルダー配下の全投稿を取得（管理用）。
pub async fn list_for_placeholder(pool: &SqlitePool, placeholder_id: i64) -> DomainResult<Vec<Post>> {
    posts_repo::list_all_for_placeholder(pool, placeholder_id)
        .await
        .map_err(Into::into)
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> DomainResult<Post> {
    posts_repo::find(pool, id).await.map_err(Into::into)
}

/// 新規作成（image widget の postmeta もここで扱う想定）。
pub async fn create(
    pool: &SqlitePool,
    input: PostInput,
    meta: Option<std::collections::HashMap<String, String>>,
) -> DomainResult<Post> {
    let id = posts_repo::insert(pool, &input).await?;
    if let Some(m) = meta {
        for (k, v) in m {
            let _ = crate::repos::postmeta::set(pool, id, &k, &v).await;
        }
    }
    posts_repo::find(pool, id).await.map_err(Into::into)
}

/// 更新。
pub async fn update(
    pool: &SqlitePool,
    id: i64,
    input: PostInput,
    meta: Option<std::collections::HashMap<String, String>>,
) -> DomainResult<()> {
    posts_repo::update(pool, id, &input).await?;
    if let Some(m) = meta {
        for (k, v) in m {
            let _ = crate::repos::postmeta::set(pool, id, &k, &v).await;
        }
    }
    Ok(())
}

/// 削除（status を 'trash' に更新するソフト削除）。
pub async fn delete(pool: &SqlitePool, id: i64) -> DomainResult<()> {
    posts_repo::delete(pool, id).await.map_err(Into::into)
}
