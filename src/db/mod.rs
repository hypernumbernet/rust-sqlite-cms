use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::error::AppResult;

/// SQLite プールを生成する。DB ファイルが無ければ自動生成し、
/// 親ディレクトリも作成する。外部キー制約は有効化する。
pub async fn connect(database_path: &str) -> AppResult<SqlitePool> {
    if let Some(parent) = Path::new(database_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    Ok(pool)
}

/// `migrations/` 配下の SQL を順に適用する。
pub async fn migrate(pool: &SqlitePool) -> AppResult<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
