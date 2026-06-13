//! サンプルインストール時の衝突検出ユーティリティ。

use std::collections::HashSet;
use std::path::Path;

use sqlx::SqlitePool;

use crate::error::AppError;

const ACTIVE_POST_FILTER: &str = "post_type = 'post' AND post_status != 'trash'";

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
    existing_values_where(pool, table, column, values, None).await
}

/// ゴミ箱以外の投稿スラッグのうち、既に存在するものを返す。
pub async fn existing_active_post_slugs(
    pool: &SqlitePool,
    slugs: &[&str],
) -> Result<Vec<String>, String> {
    existing_values_where(pool, "posts", "post_name", slugs, Some(ACTIVE_POST_FILTER)).await
}

/// ゴミ箱以外の attachment が参照する file_path のうち、既に存在するものを返す。
pub async fn existing_attachment_file_paths(
    pool: &SqlitePool,
    file_paths: &[&str],
) -> Result<Vec<String>, String> {
    if file_paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut builder = sqlx::QueryBuilder::new(
        r#"
        SELECT pm.meta_value
        FROM postmeta pm
        INNER JOIN posts p ON p.id = pm.post_id
        WHERE p.post_type = 'attachment'
          AND p.post_status != 'trash'
          AND pm.meta_key = 'file_path'
          AND pm.meta_value IN (
        "#,
    );
    let mut separated = builder.separated(", ");
    for file in file_paths {
        separated.push_bind(file);
    }
    builder.push(")");

    builder
        .build_query_scalar::<String>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// 衝突値を UI 向けラベル付きメッセージに変換する。
pub fn format_labeled(label: &str, values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| format!(r#"{label} "{value}""#))
        .collect()
}

/// 単一カラムの IN 検索で、追加条件を満たす既存値を返す。
pub async fn existing_values_where(
    pool: &SqlitePool,
    table: &'static str,
    column: &'static str,
    values: &[&str],
    extra_where: Option<&str>,
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
    if let Some(clause) = extra_where {
        builder.push(" AND ");
        builder.push(clause);
    }

    builder
        .build_query_scalar::<String>()
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

/// アップロードディレクトリに既存ファイルがある名前を返す。
pub fn existing_upload_files(uploads_dir: &str, files: &[&str]) -> Vec<String> {
    existing_upload_files_excluding(uploads_dir, files, &[])
}

/// DB で既に検出済みのパスを除き、ディスク上の既存ファイルを返す。
pub fn existing_upload_files_excluding(
    uploads_dir: &str,
    files: &[&str],
    exclude: &[String],
) -> Vec<String> {
    if files.is_empty() {
        return Vec::new();
    }

    let exclude: HashSet<&str> = exclude.iter().map(String::as_str).collect();
    let uploads = Path::new(uploads_dir);
    files
        .iter()
        .filter(|file| !exclude.contains(*file))
        .filter(|file| uploads.join(file).exists())
        .map(|file| format!(r#"アップロードファイル "{}""#, file))
        .collect()
}