//! サイト設定（options + config.toml 同期）サービス。

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::AppResult;
use crate::repos::options;

/// 現在のサイト設定を取得（options 優先、未設定時は config デフォルト）。
pub async fn get_site_settings(pool: &SqlitePool, config: &AppConfig) -> AppResult<(String, String, String)> {
    let blogname = options::get(pool, "blogname")
        .await?
        .unwrap_or_else(|| config.site.title.clone());
    let blogdescription = options::get(pool, "blogdescription")
        .await?
        .unwrap_or_else(|| config.site.tagline.clone());
    let siteurl = options::get(pool, "siteurl")
        .await?
        .unwrap_or_else(|| format!("http://{}", config.server.bind_addr));

    Ok((blogname, blogdescription, siteurl))
}

/// サイト設定を更新（options テーブル + work/config.toml の [site] 同期）。
pub async fn update_site_settings(
    pool: &SqlitePool,
    blogname: &str,
    blogdescription: &str,
    siteurl: &str,
) -> AppResult<()> {
    options::set(pool, "blogname", blogname).await?;
    options::set(pool, "blogdescription", blogdescription).await?;
    options::set(pool, "siteurl", siteurl).await?;

    // config.toml 同期（失敗はログ + エラーに）
    if let Err(err) = AppConfig::save_site_section(blogname, blogdescription) {
        tracing::error!(error = %err, "work/config.toml の保存に失敗しました");
        return Err(crate::error::AppError::Other(anyhow::anyhow!("{err}")));
    }

    Ok(())
}
