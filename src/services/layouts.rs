//! レイアウト管理サービス。

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::error::{AppError, DomainError, DomainResult};
use crate::models::layout::{Layout, LayoutInput};
use crate::presets;
use crate::repos::{layouts as layouts_repo, media as media_repo};
use crate::theme::{self, StaticFileEntry, StaticFileKind};

/// 管理画面のレイアウトファイル一覧行。
#[derive(Debug, Clone)]
pub struct LayoutAdminFile {
    pub display_path: String,
    pub kind_label: String,
    pub size_label: String,
    pub public_url: String,
    pub is_text_editable: bool,
    pub is_deletable: bool,
    pub delete_path: Option<String>,
}

impl LayoutAdminFile {
    /// テキスト編集画面の URL。編集不可なら `None`。
    pub fn edit_url(&self, layout_id: i64) -> Option<String> {
        if !self.is_text_editable {
            return None;
        }
        match self.display_path.as_str() {
            "shell.html" => Some(format!("/admin/layouts/{layout_id}/files/shell.html")),
            path if path.starts_with("static/") => {
                let relative = path.strip_prefix("static/")?;
                Some(format!(
                    "/admin/layouts/{layout_id}/files/static/{}",
                    urlencoding::encode(relative)
                ))
            }
            _ => None,
        }
    }
}

/// 全レイアウト一覧。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<Layout>> {
    layouts_repo::list_all(pool).await.map_err(Into::into)
}

/// ID で取得。
pub async fn find(pool: &SqlitePool, id: i64) -> DomainResult<Layout> {
    layouts_repo::find(pool, id).await.map_err(Into::into)
}

/// 既定レイアウトを取得。
pub async fn find_default(pool: &SqlitePool) -> DomainResult<Layout> {
    layouts_repo::find_default(pool).await.map_err(Into::into)
}

/// 管理画面からの新規作成（既定 shell + site.css）。
pub async fn create_layout_with_defaults(
    pool: &SqlitePool,
    config: &AppConfig,
    input: &LayoutInput,
) -> DomainResult<i64> {
    create_layout(
        pool,
        config,
        input,
        presets::DEFAULT_SHELL,
        &default_static_text_files_for_create(),
    )
    .await
}

/// 新規レイアウト作成（DB + ディレクトリ + shell + テキスト static）。
pub async fn create_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    input: &LayoutInput,
    shell_content: &str,
    static_text_files: &HashMap<String, String>,
) -> DomainResult<i64> {
    validate_layout_key(&input.key)?;
    validate_favicon_media(pool, input.favicon_media_id).await?;

    if layouts_repo::find_by_key(pool, &input.key).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "レイアウト key「{}」は既に使われています",
            input.key
        ))
        .into());
    }

    let id = layouts_repo::insert(pool, input).await?;
    theme::ensure_layout_dirs(&config.paths.work_dir, &input.key)?;
    theme::write_shell(&config.paths.work_dir, &input.key, shell_content)?;
    sync_static_text_files(config, &input.key, static_text_files, &[])?;

    Ok(id)
}

/// レイアウトのメタデータのみ更新する。
pub async fn update_layout_meta(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
    input: &LayoutInput,
) -> DomainResult<()> {
    validate_layout_key(&input.key)?;
    validate_favicon_media(pool, input.favicon_media_id).await?;

    let current = layouts_repo::find(pool, id).await?;
    if current.key != input.key {
        if layouts_repo::find_by_key(pool, &input.key).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "レイアウト key「{}」は既に使われています",
                input.key
            ))
            .into());
        }
        theme::rename_layout_dir(&config.paths.work_dir, &current.key, &input.key)?;
    }

    layouts_repo::update(pool, id, input).await?;
    Ok(())
}

/// レイアウト更新（メタ + shell 本文 + テキスト static）。API 互換用。
pub async fn update_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    id: i64,
    input: &LayoutInput,
    shell_content: &str,
    static_text_files: &HashMap<String, String>,
    deleted_static_paths: &[String],
) -> DomainResult<()> {
    update_layout_meta(pool, config, id, input).await?;
    let layout = layouts_repo::find(pool, id).await?;
    theme::write_shell(&config.paths.work_dir, &layout.key, shell_content)?;
    sync_static_text_files(config, &layout.key, static_text_files, deleted_static_paths)?;
    Ok(())
}

/// shell.html を保存する。
pub fn write_shell_content(
    config: &AppConfig,
    layout_key: &str,
    content: &str,
) -> DomainResult<()> {
    theme::write_shell(&config.paths.work_dir, layout_key, content).map_err(AppError::from)?;
    Ok(())
}

/// テキスト static ファイル 1 件を保存する。
pub fn write_static_text_file(
    config: &AppConfig,
    layout_key: &str,
    relative_path: &str,
    content: &str,
) -> DomainResult<()> {
    let trimmed = normalize_static_relative_path(relative_path)?;
    validate_editable_text_path(trimmed)?;
    theme::write_static_text(&config.paths.work_dir, layout_key, trimmed, content)
        .map_err(AppError::from)?;
    Ok(())
}

/// shell.html と static/ を含む管理画面用ファイル一覧。
pub fn list_admin_files(work_dir: &str, layout_key: &str) -> DomainResult<Vec<LayoutAdminFile>> {
    let static_entries = theme::list_static_files(work_dir, layout_key)?;
    let shell_size = theme::shell_file_size_bytes(work_dir, layout_key);
    let mut files = Vec::with_capacity(1 + static_entries.len());
    files.push(LayoutAdminFile {
        display_path: "shell.html".to_string(),
        kind_label: "テンプレート".to_string(),
        size_label: theme::format_static_file_size(shell_size),
        public_url: "—".to_string(),
        is_text_editable: true,
        is_deletable: false,
        delete_path: None,
    });

    for entry in static_entries {
        files.push(static_entry_to_admin_file(&entry));
    }
    Ok(files)
}

/// multipart アップロードの保存先パスを決める。
pub fn resolve_static_upload_target_path(relative_path: &str, original_name: &str) -> String {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        original_name.to_string()
    } else if trimmed.ends_with('/') {
        format!("{trimmed}{original_name}")
    } else {
        trimmed.to_string()
    }
}

/// テキスト static ファイルを同期する（削除 → 書き込み）。
pub fn sync_static_text_files(
    config: &AppConfig,
    layout_key: &str,
    files: &HashMap<String, String>,
    deleted_paths: &[String],
) -> DomainResult<()> {
    let work_dir = &config.paths.work_dir;

    for path in deleted_paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }
        theme::validate_static_relative_path(trimmed).map_err(DomainError::Validation)?;
        theme::remove_static_file(work_dir, layout_key, trimmed).map_err(AppError::from)?;
    }

    for (path, content) in files {
        let trimmed = normalize_static_relative_path(path)?;
        validate_editable_text_path(trimmed)?;
        theme::write_static_text(work_dir, layout_key, trimmed, content).map_err(AppError::from)?;
    }

    Ok(())
}

/// バイナリ static をアップロードする。
pub fn upload_static_file(
    config: &AppConfig,
    layout_key: &str,
    relative_path: &str,
    bytes: &[u8],
) -> DomainResult<StaticFileEntry> {
    if bytes.len() > theme::MAX_STATIC_UPLOAD_SIZE {
        return Err(DomainError::Validation(format!(
            "ファイルサイズは {} 以下にしてください",
            theme::format_static_file_size(theme::MAX_STATIC_UPLOAD_SIZE as u64)
        ))
        .into());
    }

    let trimmed = relative_path.trim();
    theme::validate_static_relative_path(trimmed).map_err(DomainError::Validation)?;
    if !theme::is_allowed_static_upload(trimmed) {
        return Err(DomainError::Validation(
            "この拡張子のファイルは許可されていません".into(),
        )
        .into());
    }

    theme::ensure_layout_dirs(&config.paths.work_dir, layout_key).map_err(AppError::from)?;
    theme::write_static_bytes(&config.paths.work_dir, layout_key, trimmed, bytes)
        .map_err(AppError::from)?;

    Ok(StaticFileEntry {
        relative_path: trimmed.to_string(),
        kind: if theme::is_editable_text_static(trimmed) {
            theme::StaticFileKind::Text
        } else {
            theme::StaticFileKind::Binary
        },
        size_bytes: bytes.len() as u64,
        public_url: format!("/static/{layout_key}/{trimmed}"),
    })
}

/// static ファイルを削除する。
pub fn delete_static_file(
    config: &AppConfig,
    layout_key: &str,
    relative_path: &str,
) -> DomainResult<()> {
    let trimmed = relative_path.trim();
    theme::validate_static_relative_path(trimmed).map_err(DomainError::Validation)?;
    theme::remove_static_file(&config.paths.work_dir, layout_key, trimmed).map_err(AppError::from)?;
    Ok(())
}

/// 新規レイアウト用の既定テキスト static（site.css）。
pub fn default_static_text_files_for_create() -> HashMap<String, String> {
    let mut files = HashMap::new();
    files.insert("site.css".to_string(), presets::DEFAULT_SITE_CSS.to_string());
    files
}

/// レイアウト削除。所属ページがある場合は拒否。
pub async fn delete_layout(pool: &SqlitePool, config: &AppConfig, id: i64) -> DomainResult<()> {
    let layout = layouts_repo::find(pool, id).await?;

    if layout.is_default {
        return Err(AppError::Conflict("既定レイアウトは削除できません".to_string()).into());
    }

    let count = layouts_repo::count_pages(pool, id).await?;
    if count > 0 {
        return Err(AppError::Conflict(format!(
            "このレイアウトに属するページが {count} 件あるため削除できません"
        ))
        .into());
    }

    layouts_repo::delete(pool, id).await?;
    theme::remove_layout_dir(&config.paths.work_dir, &layout.key)?;

    Ok(())
}

/// レイアウトに設定された favicon の公開 URL。未設定・無効参照は `None`。
pub async fn favicon_url_for_layout(pool: &SqlitePool, layout_id: i64) -> Option<String> {
    let layout = layouts_repo::find(pool, layout_id).await.ok()?;
    let media_id = layout.favicon_media_id?;
    let media = media_repo::find(pool, media_id).await.ok()?;
    if media.is_favicon_suitable() {
        Some(media.public_url())
    } else {
        None
    }
}

async fn validate_favicon_media(pool: &SqlitePool, media_id: Option<i64>) -> DomainResult<()> {
    let Some(id) = media_id else {
        return Ok(());
    };
    let media = media_repo::find(pool, id).await?;
    if media.is_favicon_suitable() {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "favicon には画像または .ico 形式のメディアを選択してください".into(),
        )
        .into())
    }
}

fn static_entry_to_admin_file(entry: &StaticFileEntry) -> LayoutAdminFile {
    LayoutAdminFile {
        display_path: format!("static/{}", entry.relative_path),
        kind_label: entry.kind_label().to_string(),
        size_label: theme::format_static_file_size(entry.size_bytes),
        public_url: entry.public_url.clone(),
        is_text_editable: entry.kind == StaticFileKind::Text,
        is_deletable: true,
        delete_path: Some(entry.relative_path.clone()),
    }
}

fn normalize_static_relative_path(path: &str) -> DomainResult<&str> {
    let trimmed = path.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(DomainError::Validation("ファイルパスは必須です".into()).into());
    }
    Ok(trimmed)
}

fn validate_editable_text_path(path: &str) -> DomainResult<()> {
    theme::validate_static_relative_path(path).map_err(DomainError::Validation)?;
    if theme::is_editable_text_static(path) {
        Ok(())
    } else {
        Err(DomainError::Validation(format!(
            "「{path}」はテキストエディタで編集できないファイルです"
        ))
        .into())
    }
}

fn validate_layout_key(key: &str) -> DomainResult<()> {
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !key.starts_with('_');

    if valid {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "レイアウト key は英数字・ハイフン・アンダースコアのみ（先頭に _ は不可）".into(),
        )
        .into())
    }
}
