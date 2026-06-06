//! レイアウト管理サービス。

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use sqlx::SqlitePool;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::config::AppConfig;
use crate::error::{AppError, DomainError, DomainResult};
use crate::models::layout::{
    Layout, LayoutExportManifest, LayoutExportMeta, LayoutExportPageMeta, LayoutImportAction,
    LayoutImportMode, LayoutInput,
};
use crate::models::page::PageInput;
use crate::presets;
use crate::repos::{
    layouts as layouts_repo, media as media_repo, pages as pages_repo, url_paths,
};
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

/// レイアウトと所属ページを ZIP パッケージとしてエクスポートする。
pub async fn export_layout_zip(
    pool: &SqlitePool,
    config: &AppConfig,
    layout_id: i64,
) -> DomainResult<Vec<u8>> {
    let layout = layouts_repo::find(pool, layout_id).await?;
    let pages = pages_repo::list_by_layout(pool, layout_id).await?;
    let work_dir = &config.paths.work_dir;
    let layout_key = &layout.key;

    let manifest = LayoutExportManifest {
        format_version: 1,
        layout: LayoutExportMeta {
            key: layout.key.clone(),
            name: layout.name.clone(),
            is_default: layout.is_default,
            favicon_media_id: layout.favicon_media_id,
        },
        pages: pages
            .iter()
            .map(|page| LayoutExportPageMeta {
                name: page.name.clone(),
                url_path: page.url_path.clone(),
                file_name: page.file_name.clone(),
                is_published: page.is_published,
            })
            .collect(),
    };

    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| AppError::Other(e.into()))?;

    let zip_options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", zip_options)
        .map_err(|e| AppError::Other(e.into()))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| AppError::Other(e.into()))?;

    let shell = theme::read_shell(work_dir, layout_key).map_err(AppError::from)?;
    write_zip_entry(&mut zip, zip_options, &format!("{layout_key}/shell.html"), shell.as_bytes())?;

    for entry in theme::list_static_files(work_dir, layout_key).map_err(AppError::from)? {
        let path = theme::layout_static_dir(work_dir, layout_key).join(&entry.relative_path);
        let bytes = std::fs::read(&path).map_err(AppError::from)?;
        write_zip_entry(
            &mut zip,
            zip_options,
            &format!("{layout_key}/static/{}", entry.relative_path),
            &bytes,
        )?;
    }

    for page in &pages {
        let body = theme::read_page_body(work_dir, layout_key, &page.file_name)
            .map_err(AppError::from)?;
        write_zip_entry(
            &mut zip,
            zip_options,
            &format!("{layout_key}/{}", page.file_name),
            body.as_bytes(),
        )?;
    }

    let cursor = zip.finish().map_err(|e| AppError::Other(e.into()))?;
    Ok(cursor.into_inner())
}

/// ZIP パッケージからレイアウトをインポートする。
pub async fn import_layout_zip(
    pool: &SqlitePool,
    config: &AppConfig,
    bytes: &[u8],
    mode: LayoutImportMode,
) -> DomainResult<(LayoutImportAction, String)> {
    let (manifest, zip_files) = parse_layout_zip(bytes)?;
    validate_manifest(&manifest, &zip_files)?;

    let layout_key = manifest.layout.key.trim();
    let existing = layouts_repo::find_by_key(pool, layout_key).await?;

    if existing.is_some() && mode == LayoutImportMode::Skip {
        return Ok((
            LayoutImportAction::Skipped,
            format!("レイアウト「{layout_key}」は既に存在するためスキップしました"),
        ));
    }

    for page in &manifest.pages {
        let exclude = if let Some(ref layout) = existing {
            pages_repo::find_by_layout_file(pool, layout.id, &page.file_name)
                .await?
                .map(|p| p.id)
        } else {
            None
        };
        url_paths::ensure_url_available(pool, page.url_path.as_deref(), exclude).await?;
    }

    let favicon_media_id =
        resolve_import_favicon_media_id(pool, manifest.layout.favicon_media_id).await;

    let action = if let Some(layout) = existing {
        let input = LayoutInput {
            key: layout.key.clone(),
            name: manifest.layout.name.trim().to_string(),
            is_default: layout.is_default,
            favicon_media_id,
        };
        update_layout_meta(pool, config, layout.id, &input).await?;
        apply_layout_package(pool, config, layout.id, layout_key, &manifest, &zip_files).await?;
        LayoutImportAction::Updated
    } else {
        let input = LayoutInput {
            key: layout_key.to_string(),
            name: manifest.layout.name.trim().to_string(),
            is_default: false,
            favicon_media_id,
        };
        validate_layout_key(&input.key)?;
        let id = layouts_repo::insert(pool, &input).await?;
        theme::ensure_layout_dirs(&config.paths.work_dir, layout_key).map_err(AppError::from)?;
        apply_layout_package(pool, config, id, layout_key, &manifest, &zip_files).await?;
        LayoutImportAction::Created
    };

    let message = match action {
        LayoutImportAction::Created => {
            format!("レイアウト「{layout_key}」をインポートしました（新規作成）")
        }
        LayoutImportAction::Updated => {
            format!("レイアウト「{layout_key}」をインポートしました（上書き）")
        }
        LayoutImportAction::Skipped => unreachable!("handled above"),
    };

    Ok((action, message))
}

/// manifest の内容を検証する。
pub fn validate_manifest(
    manifest: &LayoutExportManifest,
    zip_files: &HashMap<String, Vec<u8>>,
) -> DomainResult<()> {
    if manifest.format_version != 1 {
        return Err(DomainError::Validation(
            "format_version は 1 のみ対応しています".to_string(),
        )
        .into());
    }

    validate_layout_key(manifest.layout.key.trim())?;

    if manifest.layout.name.trim().is_empty() {
        return Err(DomainError::Validation("レイアウト名は必須です".to_string()).into());
    }

    let prefix = format!("{}/", manifest.layout.key.trim());

    for page in &manifest.pages {
        if page.name.trim().is_empty() {
            return Err(DomainError::Validation("ページ名は必須です".to_string()).into());
        }
        if !page.file_name.starts_with("pages/") {
            return Err(DomainError::Validation(format!(
                "file_name は pages/ で始まる必要があります（got: {}）",
                page.file_name
            ))
            .into());
        }
        if !is_safe_zip_relative_path(&page.file_name) {
            return Err(DomainError::Validation(format!(
                "不正な file_name です: {}",
                page.file_name
            ))
            .into());
        }
        let zip_path = format!("{}/{}", manifest.layout.key.trim(), page.file_name);
        if !zip_files.contains_key(&zip_path) {
            return Err(DomainError::Validation(format!(
                "ZIP 内にページファイルがありません: {zip_path}"
            ))
            .into());
        }
    }

    let shell_path = format!("{}shell.html", prefix);
    if !zip_files.contains_key(&shell_path) {
        return Err(DomainError::Validation(
            "ZIP 内に shell.html がありません".to_string(),
        )
        .into());
    }

    Ok(())
}

async fn apply_layout_package(
    pool: &SqlitePool,
    config: &AppConfig,
    layout_id: i64,
    layout_key: &str,
    manifest: &LayoutExportManifest,
    zip_files: &HashMap<String, Vec<u8>>,
) -> DomainResult<()> {
    let work_dir = &config.paths.work_dir;
    let prefix = format!("{layout_key}/");

    let shell_path = format!("{prefix}shell.html");
    let shell_bytes = zip_files
        .get(&shell_path)
        .ok_or_else(|| DomainError::Validation("shell.html が見つかりません".to_string()))?;
    let shell = String::from_utf8(shell_bytes.clone()).map_err(|_| {
        DomainError::Validation("shell.html は UTF-8 テキストである必要があります".to_string())
    })?;
    theme::write_shell(work_dir, layout_key, &shell).map_err(AppError::from)?;

    for (path, bytes) in zip_files {
        if let Some(relative) = path.strip_prefix(&prefix).and_then(|p| p.strip_prefix("static/"))
        {
            if !is_safe_zip_relative_path(relative) {
                continue;
            }
            if theme::is_allowed_static_upload(relative) {
                theme::write_static_bytes(work_dir, layout_key, relative, bytes)
                    .map_err(AppError::from)?;
            }
        }
    }

    for page_meta in &manifest.pages {
        let zip_path = format!("{prefix}{}", page_meta.file_name);
        let body_bytes = zip_files
            .get(&zip_path)
            .ok_or_else(|| DomainError::Validation(format!("{zip_path} が見つかりません")))?;
        let body = String::from_utf8(body_bytes.clone()).map_err(|_| {
            DomainError::Validation(format!(
                "{} は UTF-8 テキストである必要があります",
                page_meta.file_name
            ))
        })?;

        let page_input = PageInput {
            name: page_meta.name.trim().to_string(),
            url_path: page_meta.url_path.clone(),
            content: body.clone(),
            layout_id,
            is_published: page_meta.is_published,
        };

        if let Some(existing) =
            pages_repo::find_by_layout_file(pool, layout_id, &page_meta.file_name).await?
        {
            pages_repo::update(pool, existing.id, &page_input).await?;
            theme::write_page_body(work_dir, layout_key, &page_meta.file_name, &body)
                .map_err(AppError::from)?;
        } else {
            pages_repo::insert_with_file_name(pool, &page_input, &page_meta.file_name).await?;
            theme::write_page_body(work_dir, layout_key, &page_meta.file_name, &body)
                .map_err(AppError::from)?;
        }
    }

    Ok(())
}

fn parse_layout_zip(bytes: &[u8]) -> DomainResult<(LayoutExportManifest, HashMap<String, Vec<u8>>)> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| {
        DomainError::Validation(format!("ZIP ファイルを読み取れません: {e}"))
    })?;

    let mut manifest: Option<LayoutExportManifest> = None;
    let mut zip_files = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| DomainError::Validation(format!("ZIP エントリの読み取りに失敗: {e}")))?;
        let name = normalize_zip_entry_name(file.name());

        if name == "manifest.json" {
            let mut raw = String::new();
            file.read_to_string(&mut raw).map_err(|e| {
                DomainError::Validation(format!("manifest.json の読み取りに失敗: {e}"))
            })?;
            manifest = Some(serde_json::from_str(&raw).map_err(|e| {
                DomainError::Validation(format!("manifest.json の形式が正しくありません: {e}"))
            })?);
            continue;
        }

        if name.contains("..") {
            return Err(DomainError::Validation(format!(
                "不正な ZIP パスです: {name}"
            ))
            .into());
        }

        if file.is_dir() {
            continue;
        }

        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(|e| {
            DomainError::Validation(format!("ZIP エントリの読み取りに失敗: {e}"))
        })?;
        zip_files.insert(name, data);
    }

    let Some(manifest) = manifest else {
        return Err(DomainError::Validation(
            "manifest.json が見つかりません".to_string(),
        )
        .into());
    };

    let prefix = format!("{}/", manifest.layout.key.trim());
    for path in zip_files.keys() {
        if !path.starts_with(&prefix) {
            return Err(DomainError::Validation(format!(
                "ZIP 内のパスは {prefix} で始まる必要があります（got: {path}）"
            ))
            .into());
        }
        let relative = path.strip_prefix(&prefix).unwrap_or(path);
        if !relative.is_empty() && !is_safe_zip_relative_path(relative) {
            return Err(DomainError::Validation(format!(
                "不正な ZIP パスです: {path}"
            ))
            .into());
        }
    }

    Ok((manifest, zip_files))
}

async fn resolve_import_favicon_media_id(
    pool: &SqlitePool,
    media_id: Option<i64>,
) -> Option<i64> {
    let Some(id) = media_id else {
        return None;
    };
    if validate_favicon_media(pool, Some(id)).await.is_ok() {
        Some(id)
    } else {
        None
    }
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
