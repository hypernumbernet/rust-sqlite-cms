//! サンプルインストール時の衝突検出ユーティリティ。

use std::path::Path;

use sqlx::SqlitePool;

use crate::error::AppError;

/// 衝突一覧をユーザー向けエラーに変換する。
pub fn abort(conflicts: Vec<String>) -> AppError {
    AppError::Conflict(format!(
        "以下の名前が既に存在するため、インストールを中止しました: {}",
        conflicts.join(", ")
    ))
}

/// `sqlite_master` にオブジェクトが存在するか。
pub async fn sqlite_object_exists(
    pool: &SqlitePool,
    object_type: &str,
    name: &str,
) -> Result<bool, String> {
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = ? AND name = ? LIMIT 1",
    )
    .bind(object_type)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(exists.is_some())
}

/// 単一カラムの IN 検索で、既に存在する値を返す。
pub async fn existing_values(
    pool: &SqlitePool,
    table: &'static str,
    column: &'static str,
    values: &[&str],
) -> Result<Vec<String>, String> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut builder = sqlx::QueryBuilder::new(format!("SELECT {column} FROM {table} WHERE "));
    builder.push(column);
    builder.push(" IN (");
    let mut separated = builder.separated(", ");
    for value in values {
        separated.push_bind(value);
    }
    builder.push(")");

    builder
        .build_query_scalar::<String>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// アップロードディレクトリに既存ファイルがある名前を返す。
pub fn existing_upload_files(uploads_dir: &str, files: &[&str]) -> Vec<String> {
    let uploads = Path::new(uploads_dir);
    files
        .iter()
        .filter(|file| uploads.join(file).exists())
        .map(|file| format!(r#"アップロードファイル "{}""#, file))
        .collect()
}