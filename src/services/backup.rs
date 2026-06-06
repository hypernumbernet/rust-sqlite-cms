//! サイト全体のバックアップ / リストア。

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::config::{self, AppConfig};
use crate::error::{AppError, DomainError, DomainResult};

const FORMAT_VERSION: u32 = 1;
const DB_ZIP_PATH: &str = "database/cms.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    format_version: u32,
    cms_version: String,
    created_at: String,
    database_path: String,
    paths: BackupManifestPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifestPaths {
    work_dir: String,
    uploads_dir: String,
}

/// 管理画面表示用の現在データ概要。
#[derive(Debug, Clone)]
pub struct BackupStats {
    pub posts_count: i64,
    pub pages_count: i64,
    pub media_count: i64,
    pub layouts_count: i64,
    pub users_count: i64,
}

/// リストア結果（UI 表示用）。
#[derive(Debug, Clone)]
pub struct BackupRestoreResult {
    pub message: String,
    pub restored_files_count: usize,
    pub backup_created_at: Option<String>,
}

pub async fn collect_stats(pool: &SqlitePool) -> DomainResult<BackupStats> {
    let posts_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash'",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;

    let pages_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM pages").fetch_one(pool).await.map_err(AppError::from)?;

    let media_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'attachment' AND post_status = 'inherit'",
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;

    let layouts_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM layouts").fetch_one(pool).await.map_err(AppError::from)?;

    let users_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(pool).await.map_err(AppError::from)?;

    Ok(BackupStats {
        posts_count: posts_count.0,
        pages_count: pages_count.0,
        media_count: media_count.0,
        layouts_count: layouts_count.0,
        users_count: users_count.0,
    })
}

/// SQLite DB・work/・uploads/ を ZIP 1 ファイルにまとめる。
pub async fn export_site_backup(pool: &SqlitePool, config: &AppConfig) -> DomainResult<Vec<u8>> {
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(pool)
        .await
        .map_err(AppError::from)?;

    let db_path = Path::new(&config.database.path);
    if !db_path.is_file() {
        return Err(DomainError::Validation(format!(
            "データベースファイルが見つかりません: {}",
            config.database.path
        )));
    }

    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        cms_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        database_path: config.database.path.clone(),
        paths: BackupManifestPaths {
            work_dir: config.paths.work_dir.clone(),
            uploads_dir: config.paths.uploads_dir.clone(),
        },
    };

    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| AppError::Other(e.into()))?;

    let zip_options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));

    write_zip_entry(&mut zip, zip_options, "manifest.json", manifest_json.as_bytes())?;

    let db_bytes = std::fs::read(db_path).map_err(AppError::from)?;
    write_zip_entry(&mut zip, zip_options, DB_ZIP_PATH, &db_bytes)?;

    let work_dir = Path::new(&config.paths.work_dir);
    let uploads_dir = Path::new(&config.paths.uploads_dir);

    for (zip_path, file_path) in collect_directory_files(
        work_dir,
        &normalized_path_prefix(&config.paths.work_dir),
    )? {
        if file_path == db_path {
            continue;
        }
        let bytes = std::fs::read(&file_path).map_err(AppError::from)?;
        write_zip_entry(&mut zip, zip_options, &zip_path, &bytes)?;
    }

    if !uploads_dir.starts_with(work_dir) {
        for (zip_path, file_path) in collect_directory_files(
            uploads_dir,
            &normalized_path_prefix(&config.paths.uploads_dir),
        )? {
            let bytes = std::fs::read(&file_path).map_err(AppError::from)?;
            write_zip_entry(&mut zip, zip_options, &zip_path, &bytes)?;
        }
    }

    let cursor = zip.finish().map_err(|e| AppError::Other(e.into()))?;
    Ok(cursor.into_inner())
}

/// ZIP からサイト全体をリストアする（既存データは上書き）。
pub async fn import_site_backup(
    _pool: &SqlitePool,
    config: &AppConfig,
    bytes: &[u8],
) -> DomainResult<BackupRestoreResult> {
    let (manifest, zip_files) = parse_backup_zip(bytes)?;
    validate_backup_zip(&manifest, &zip_files)?;

    let work_dir = Path::new(&config.paths.work_dir);
    let uploads_dir = Path::new(&config.paths.uploads_dir);

    clear_directory_contents(work_dir)?;
    if !uploads_dir.starts_with(work_dir) {
        clear_directory_contents(uploads_dir)?;
    }

    let db_path = Path::new(&config.database.path);
    if db_path.exists() {
        std::fs::remove_file(db_path).map_err(AppError::from)?;
    }
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }

    let mut restored_files_count = 0usize;

    for (zip_path, content) in &zip_files {
        let Some(dest) = map_zip_path_to_dest(zip_path, &manifest, config) else {
            continue;
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::from)?;
        }
        std::fs::write(&dest, content).map_err(AppError::from)?;
        restored_files_count += 1;
    }

    Ok(BackupRestoreResult {
        message: "バックアップからのリストアが完了しました。サーバーを再起動することを強くおすすめします。"
            .to_string(),
        restored_files_count,
        backup_created_at: Some(manifest.created_at),
    })
}

fn parse_backup_zip(bytes: &[u8]) -> DomainResult<(BackupManifest, HashMap<String, Vec<u8>>)> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| DomainError::Validation(format!("ZIP ファイルを読み取れません: {e}")))?;

    let mut manifest: Option<BackupManifest> = None;
    let mut zip_files = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            DomainError::Validation(format!("ZIP エントリの読み取りに失敗: {e}"))
        })?;
        let name = normalize_zip_entry_name(file.name());

        if !is_safe_zip_relative_path(&name) {
            return Err(DomainError::Validation(format!(
                "安全でない ZIP エントリパスです: {name}"
            )));
        }

        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| DomainError::Validation(format!("ZIP エントリの読み取りに失敗: {e}")))?;

        if name == "manifest.json" {
            manifest = Some(
                serde_json::from_slice(&content)
                    .map_err(|e| DomainError::Validation(format!("manifest.json が不正です: {e}")))?,
            );
        } else {
            zip_files.insert(name, content);
        }
    }

    let manifest = manifest.ok_or_else(|| {
        DomainError::Validation("manifest.json が ZIP に含まれていません".to_string())
    })?;

    Ok((manifest, zip_files))
}

fn validate_backup_zip(manifest: &BackupManifest, zip_files: &HashMap<String, Vec<u8>>) -> DomainResult<()> {
    if manifest.format_version != FORMAT_VERSION {
        return Err(DomainError::Validation(format!(
            "未対応のバックアップ形式です (format_version={})",
            manifest.format_version
        )));
    }

    if !zip_files.contains_key(DB_ZIP_PATH) {
        return Err(DomainError::Validation(format!(
            "必須ファイル {DB_ZIP_PATH} が ZIP に含まれていません"
        )));
    }

    Ok(())
}

fn map_zip_path_to_dest(
    zip_path: &str,
    manifest: &BackupManifest,
    config: &AppConfig,
) -> Option<PathBuf> {
    if zip_path == DB_ZIP_PATH {
        return Some(PathBuf::from(&config.database.path));
    }

    let work_prefix = normalized_path_prefix(&manifest.paths.work_dir);
    if let Some(rest) = zip_path.strip_prefix(&format!("{work_prefix}/")) {
        return Some(Path::new(&config.paths.work_dir).join(rest));
    }

    let uploads_prefix = normalized_path_prefix(&manifest.paths.uploads_dir);
    let work_path = Path::new(&manifest.paths.work_dir);
    let uploads_path = Path::new(&manifest.paths.uploads_dir);
    if !uploads_path.starts_with(work_path)
        && let Some(rest) = zip_path.strip_prefix(&format!("{uploads_prefix}/"))
    {
        return Some(Path::new(&config.paths.uploads_dir).join(rest));
    }

    None
}

fn collect_directory_files(base: &Path, zip_prefix: &str) -> DomainResult<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    collect_directory_files_inner(base, base, zip_prefix, &mut files)?;
    Ok(files)
}

fn collect_directory_files_inner(
    dir: &Path,
    base: &Path,
    zip_prefix: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> DomainResult<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        if path.is_dir() {
            collect_directory_files_inner(&path, base, zip_prefix, out)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(base).map_err(|_| {
                DomainError::Validation(format!("ファイルパスの解決に失敗: {}", path.display()))
            })?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let zip_path = format!("{zip_prefix}/{rel_str}");
            out.push((zip_path, path));
        }
    }

    Ok(())
}

fn write_zip_entry(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    options: SimpleFileOptions,
    path: &str,
    bytes: &[u8],
) -> DomainResult<()> {
    zip.start_file(path, options)
        .map_err(|e| AppError::Other(e.into()))?;
    zip.write_all(bytes).map_err(|e| AppError::Other(e.into()))?;
    Ok(())
}

fn normalize_zip_entry_name(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches("./").to_string()
}

fn is_safe_zip_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains("..")
        && !path.starts_with('/')
        && path.split('/').all(|part| !part.is_empty() && part != "..")
}

fn normalized_path_prefix(prefix: &str) -> String {
    prefix.trim().trim_matches('/').to_string()
}

fn clear_directory_contents(dir: &Path) -> DomainResult<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(AppError::from)?;
        } else {
            std::fs::remove_file(&path).map_err(AppError::from)?;
        }
    }

    Ok(())
}

/// エクスポート用の推奨ファイル名。
pub fn export_filename() -> String {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    format!("cms-backup-{timestamp}.zip")
}

/// 管理画面向け: config.toml の表示パス。
pub fn config_display_path() -> String {
    config::config_path().display().to_string()
}