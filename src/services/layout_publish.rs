//! レイアウトセットの公開サイトへの差し替え。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::repos::{layouts as layouts_repo, pages as pages_repo};

/// 公開差し替えの結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishLayoutSetResult {
    pub published_count: usize,
    pub demoted_count: usize,
    pub demoted_layout_keys: Vec<String>,
}

/// 指定レイアウトを公開サイトに差し替える。
pub async fn publish_layout_set(pool: &SqlitePool, layout_id: i64) -> AppResult<PublishLayoutSetResult> {
    layouts_repo::find(pool, layout_id).await?;

    let stats = pages_repo::swap_live_layout(pool, layout_id).await?;
    Ok(PublishLayoutSetResult {
        published_count: stats.published_count,
        demoted_count: stats.demoted_count as usize,
        demoted_layout_keys: stats.demoted_layout_keys,
    })
}