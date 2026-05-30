use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::AppResult;

/// 指定キーの値を取得する。存在しなければ `None`。
pub async fn get(pool: &SqlitePool, name: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT option_value FROM options WHERE option_name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(value,)| value))
}

/// autoload 対象の設定をすべて取得する（公開サイト描画などで一括利用）。
pub async fn get_all_autoload(pool: &SqlitePool) -> AppResult<HashMap<String, String>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_name, option_value FROM options WHERE autoload = 1")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

/// 設定を upsert する（存在すれば値を更新）。
pub async fn set(pool: &SqlitePool, name: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO options (option_name, option_value, autoload) VALUES (?, ?, 1) \
         ON CONFLICT(option_name) DO UPDATE SET option_value = excluded.option_value",
    )
    .bind(name)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// 既定値を投入する。既に存在する場合は上書きしない。
async fn set_default(pool: &SqlitePool, name: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO options (option_name, option_value, autoload) VALUES (?, ?, 1) \
         ON CONFLICT(option_name) DO NOTHING",
    )
    .bind(name)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// 初回起動時などに、設定ファイル由来の既定 options を用意する。
/// 既存値は尊重して上書きしない。
pub async fn ensure_defaults(pool: &SqlitePool, config: &AppConfig) -> AppResult<()> {
    set_default(pool, "blogname", &config.site.title).await?;
    set_default(pool, "blogdescription", &config.site.tagline).await?;
    set_default(
        pool,
        "siteurl",
        &format!("http://{}", config.server.bind_addr),
    )
    .await?;
    set_default(
        pool,
        "permalink_structure",
        "/%year%/%monthnum%/%day%/%postname%/",
    )
    .await?;
    Ok(())
}
