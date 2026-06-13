//! レイアウト管理サービス。

use std::collections::{HashMap, HashSet};
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
use crate::models::placeholder::{self as placeholder_model, PlaceholderInput};
use crate::models::page::PageInput;
use crate::presets;
use crate::repos::{
    layouts as layouts_repo, pages as pages_repo, placeholders as placeholders_repo,
    postmeta as postmeta_repo, posts as posts_repo,
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

/// 初回トップページ作成や新規ページの初期レイアウトに使うレイアウトを取得。
pub async fn find_bootstrap_layout(pool: &SqlitePool) -> DomainResult<Layout> {
    layouts_repo::find_bootstrap_layout(pool)
        .await
        .map_err(Into::into)
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

/// レイアウトを別の key に複製する（shell / static / 所属ページをコピー）。
pub async fn duplicate_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    source_id: i64,
    target_key: &str,
    include_posts: bool,
) -> DomainResult<String> {
    let source = layouts_repo::find(pool, source_id).await?;
    let target_key = target_key.trim();
    validate_layout_key(target_key)?;

    if target_key == source.key {
        return Err(DomainError::Validation(
            "複製先の layout key は元の key と異なる必要があります".to_string(),
        )
        .into());
    }

    if layouts_repo::find_by_key(pool, target_key).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "レイアウト key「{target_key}」は既に使われています"
        ))
        .into());
    }

    let bytes = export_layout_zip(pool, config, source_id).await?;
    let (action, _) = import_layout_zip(
        pool,
        config,
        &bytes,
        LayoutImportMode::Rename,
        Some(target_key),
    )
    .await?;

    if action != LayoutImportAction::Created {
        return Err(AppError::Other(anyhow::anyhow!(
            "レイアウトの複製に失敗しました（予期しないインポート結果）"
        ))
        .into());
    }

    let mut message = format!(
        "レイアウト「{}」を「{}」として複製しました",
        source.name, target_key
    );

    if include_posts {
        let target_layout = layouts_repo::find_by_key(pool, target_key)
            .await?
            .ok_or(AppError::NotFound)?;
        match copy_posts_for_layout(
            pool,
            config,
            &target_layout,
            &source.key,
            target_key,
        )
        .await
        {
            Ok(summary) => {
                if summary.placeholder_count == 0 {
                    message.push_str("（投稿対象のプレースホルダーは見つかりませんでした）");
                } else {
                    message.push_str(&format!(
                        "（プレースホルダー {} 件・投稿 {} 件をコピー）",
                        summary.placeholder_count, summary.post_count
                    ));
                }
            }
            Err(err) => {
                let _ = delete_layout(pool, config, target_layout.id).await;
                return Err(err);
            }
        }
    }

    Ok(message)
}

struct CopyPostsSummary {
    placeholder_count: usize,
    post_count: usize,
}

struct PlaceholderRemap {
    source: crate::models::placeholder::Placeholder,
    new_name: String,
}

enum LayoutTextTarget {
    Shell,
    Page(String),
    Static(String),
}

struct LayoutTextFile {
    target: LayoutTextTarget,
    content: String,
}

async fn copy_posts_for_layout(
    pool: &SqlitePool,
    config: &AppConfig,
    target_layout: &Layout,
    source_key: &str,
    target_key: &str,
) -> DomainResult<CopyPostsSummary> {
    let work_dir = &config.paths.work_dir;
    let mut text_files =
        load_layout_text_files(pool, work_dir, target_layout.id, target_key).await?;
    let layout_text = text_files
        .iter()
        .map(|file| file.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let all_placeholders = placeholders_repo::list_all(pool).await?;
    let remaps = build_placeholder_remaps(&layout_text, &all_placeholders, source_key, target_key)?;
    if remaps.is_empty() {
        return Ok(CopyPostsSummary {
            placeholder_count: 0,
            post_count: 0,
        });
    }

    let rewrite_pairs = build_placeholder_rewrite_pairs(&remaps);
    let target_token = layout_key_to_placeholder_token(target_key);
    let mut created_placeholder_ids = Vec::with_capacity(remaps.len());
    let mut reserved_slugs = HashSet::new();
    let mut post_count = 0usize;

    for remap in &remaps {
        let input = PlaceholderInput {
            name: remap.new_name.clone(),
            widget_type_id: remap.source.widget_type_id,
            config: remap.source.config.clone(),
        };
        let new_placeholder_id = placeholders_repo::insert(pool, &input).await?;
        created_placeholder_ids.push(new_placeholder_id);

        for source_post in posts_repo::list_all_for_placeholder(pool, remap.source.id).await? {
            let original_slug = source_post.post_name.as_deref().unwrap_or("post");
            let slug = unique_copy_post_slug(
                pool,
                &target_token,
                original_slug,
                &mut reserved_slugs,
            )
            .await?;
            let new_post_id =
                posts_repo::insert_copy(pool, new_placeholder_id, source_post.id, &slug).await?;
            postmeta_repo::copy_for_post(pool, source_post.id, new_post_id).await?;
            post_count += 1;
        }
    }

    for file in &mut text_files {
        file.content = rewrite_placeholder_refs(&file.content, &rewrite_pairs);
    }

    if let Err(err) = save_layout_text_files(work_dir, target_key, &text_files) {
        rollback_created_placeholders(pool, &created_placeholder_ids).await?;
        return Err(err);
    }

    Ok(CopyPostsSummary {
        placeholder_count: remaps.len(),
        post_count,
    })
}

async fn load_layout_text_files(
    pool: &SqlitePool,
    work_dir: &str,
    layout_id: i64,
    layout_key: &str,
) -> DomainResult<Vec<LayoutTextFile>> {
    let mut files = Vec::new();

    if let Ok(shell) = theme::read_shell(work_dir, layout_key) {
        files.push(LayoutTextFile {
            target: LayoutTextTarget::Shell,
            content: shell,
        });
    }

    for page in pages_repo::list_by_layout(pool, layout_id).await? {
        if let Ok(body) = theme::read_page_body(work_dir, layout_key, &page.file_name) {
            files.push(LayoutTextFile {
                target: LayoutTextTarget::Page(page.file_name),
                content: body,
            });
        }
    }

    for entry in theme::list_static_files(work_dir, layout_key).map_err(AppError::from)? {
        if entry.kind == StaticFileKind::Text {
            if let Ok(text) = theme::read_static_text(work_dir, layout_key, &entry.relative_path) {
                files.push(LayoutTextFile {
                    target: LayoutTextTarget::Static(entry.relative_path),
                    content: text,
                });
            }
        }
    }

    Ok(files)
}

fn save_layout_text_files(
    work_dir: &str,
    layout_key: &str,
    files: &[LayoutTextFile],
) -> DomainResult<()> {
    for file in files {
        match &file.target {
            LayoutTextTarget::Shell => {
                theme::write_shell(work_dir, layout_key, &file.content).map_err(AppError::from)?;
            }
            LayoutTextTarget::Page(file_name) => {
                theme::write_page_body(work_dir, layout_key, file_name, &file.content)
                    .map_err(AppError::from)?;
            }
            LayoutTextTarget::Static(relative_path) => {
                theme::write_static_text(work_dir, layout_key, relative_path, &file.content)
                    .map_err(AppError::from)?;
            }
        }
    }
    Ok(())
}

fn build_placeholder_remaps(
    layout_text: &str,
    all_placeholders: &[crate::models::placeholder::Placeholder],
    source_key: &str,
    target_key: &str,
) -> DomainResult<Vec<PlaceholderRemap>> {
    let source_token = layout_key_to_placeholder_token(source_key);
    let target_token = layout_key_to_placeholder_token(target_key);

    let mut remaps: Vec<PlaceholderRemap> = all_placeholders
        .iter()
        .filter(|placeholder| placeholder_used_in_text(layout_text, &placeholder.name))
        .map(|placeholder| {
            let new_name = remap_placeholder_name(&source_token, &target_token, &placeholder.name);
            PlaceholderRemap {
                source: placeholder.clone(),
                new_name,
            }
        })
        .collect();

    remaps.sort_by(|a, b| b.source.name.len().cmp(&a.source.name.len()));
    for remap in &remaps {
        placeholder_model::validate_name(&remap.new_name).map_err(DomainError::Validation)?;
    }
    Ok(remaps)
}

fn build_placeholder_rewrite_pairs(remaps: &[PlaceholderRemap]) -> Vec<(String, String)> {
    let mut pairs = Vec::with_capacity(remaps.len() * 2);
    for remap in remaps {
        pairs.push((
            format!("{}_html", remap.source.name),
            format!("{}_html", remap.new_name),
        ));
        pairs.push((
            format!("has_{}", remap.source.name),
            format!("has_{}", remap.new_name),
        ));
    }
    pairs
}

fn placeholder_used_in_text(text: &str, name: &str) -> bool {
    let marker = format!("{name}_html");
    let mut start = 0;
    while let Some(pos) = text[start..].find(&marker) {
        let abs_pos = start + pos;
        if abs_pos == 0 || !is_placeholder_name_char(text.as_bytes()[abs_pos - 1]) {
            return true;
        }
        start = abs_pos + 1;
    }
    false
}

fn is_placeholder_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn layout_key_to_placeholder_token(key: &str) -> String {
    key.replace('-', "_")
}

fn remap_placeholder_name(source_token: &str, target_token: &str, name: &str) -> String {
    let prefix = format!("{source_token}_");
    if let Some(suffix) = name.strip_prefix(&prefix) {
        format!("{target_token}_{suffix}")
    } else {
        format!("{target_token}_{name}")
    }
}

async fn unique_copy_post_slug(
    pool: &SqlitePool,
    target_token: &str,
    original_slug: &str,
    reserved: &mut HashSet<String>,
) -> DomainResult<String> {
    let base = format!("{target_token}-{original_slug}");
    let mut candidate = base.clone();
    let mut suffix = 2i64;
    while reserved.contains(&candidate) || active_post_slug_exists(pool, &candidate).await? {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    reserved.insert(candidate.clone());
    Ok(candidate)
}

async fn active_post_slug_exists(pool: &SqlitePool, slug: &str) -> DomainResult<bool> {
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM posts WHERE post_type = 'post' AND post_status != 'trash' AND post_name = ? LIMIT 1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;
    Ok(exists.is_some())
}

async fn rollback_created_placeholders(pool: &SqlitePool, placeholder_ids: &[i64]) -> DomainResult<()> {
    for id in placeholder_ids {
        sqlx::query("DELETE FROM posts WHERE placeholder_id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(AppError::from)?;
        sqlx::query("DELETE FROM placeholders WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(AppError::from)?;
    }
    Ok(())
}

fn rewrite_placeholder_refs(text: &str, pairs: &[(String, String)]) -> String {
    let mut result = text.to_string();
    for (old, new) in pairs {
        if result.contains(old) {
            result = result.replace(old, new);
        }
    }
    result
}

/// ZIP パッケージからレイアウトをインポートする。
pub async fn import_layout_zip(
    pool: &SqlitePool,
    config: &AppConfig,
    bytes: &[u8],
    mode: LayoutImportMode,
    target_key: Option<&str>,
) -> DomainResult<(LayoutImportAction, String)> {
    let (mut manifest, mut zip_files) = parse_layout_zip(bytes)?;

    let layout_key = if mode == LayoutImportMode::Rename {
        let key = target_key
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                DomainError::Validation("インポート先 layout key を指定してください".to_string())
            })?;
        validate_layout_key(key)?;
        if layouts_repo::find_by_key(pool, key).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "指定した layout key「{key}」は既に使われています"
            ))
            .into());
        }
        let source_key = manifest.layout.key.trim();
        (manifest, zip_files) = remap_layout_package(source_key, key, &manifest, &zip_files);
        key.to_string()
    } else {
        manifest.layout.key.trim().to_string()
    };

    validate_manifest(&manifest, &zip_files)?;

    let existing = layouts_repo::find_by_key(pool, &layout_key).await?;

    if existing.is_some() && mode == LayoutImportMode::Skip {
        return Ok((
            LayoutImportAction::Skipped,
            format!("レイアウト「{layout_key}」は既に存在するためスキップしました"),
        ));
    }

    let action = if let Some(layout) = existing {
        let input = LayoutInput {
            key: layout.key.clone(),
            name: manifest.layout.name.trim().to_string(),
        };
        update_layout_meta(pool, config, layout.id, &input).await?;
        apply_layout_package(pool, config, layout.id, &layout_key, &manifest, &zip_files).await?;
        LayoutImportAction::Updated
    } else {
        let input = LayoutInput {
            key: layout_key.clone(),
            name: manifest.layout.name.trim().to_string(),
        };
        validate_layout_key(&input.key)?;
        let id = layouts_repo::insert(pool, &input).await?;
        theme::ensure_layout_dirs(&config.paths.work_dir, &layout_key).map_err(AppError::from)?;
        apply_layout_package(pool, config, id, &layout_key, &manifest, &zip_files).await?;
        LayoutImportAction::Created
    };

    let message = match action {
        LayoutImportAction::Created => {
            if mode == LayoutImportMode::Rename {
                format!("レイアウト「{layout_key}」を別名でインポートしました（新規作成）")
            } else {
                format!("レイアウト「{layout_key}」をインポートしました（新規作成）")
            }
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
            url_path: None,
            content: body.clone(),
            layout_id,
            is_published: false,
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

fn remap_layout_package(
    source_key: &str,
    target_key: &str,
    manifest: &LayoutExportManifest,
    zip_files: &HashMap<String, Vec<u8>>,
) -> (LayoutExportManifest, HashMap<String, Vec<u8>>) {
    let mut remapped_manifest = manifest.clone();
    remapped_manifest.layout.key = target_key.to_string();

    let old_prefix = format!("{source_key}/");
    let new_prefix = format!("{target_key}/");
    let mut remapped_files = HashMap::new();

    for (path, bytes) in zip_files {
        let Some(relative) = path.strip_prefix(&old_prefix) else {
            continue;
        };
        let new_path = format!("{new_prefix}{relative}");
        let new_bytes = if relative == "shell.html" || relative.starts_with("pages/") {
            rewrite_layout_key_in_text(bytes, source_key, target_key)
        } else if let Some(static_rel) = relative.strip_prefix("static/") {
            if theme::is_editable_text_static(static_rel) {
                rewrite_layout_key_in_text(bytes, source_key, target_key)
            } else {
                bytes.clone()
            }
        } else {
            bytes.clone()
        };
        remapped_files.insert(new_path, new_bytes);
    }

    (remapped_manifest, remapped_files)
}

fn rewrite_layout_key_in_text(bytes: &[u8], source_key: &str, target_key: &str) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    text.replace(&format!("{source_key}/"), &format!("{target_key}/"))
        .replace(
            &format!("/static/{source_key}/"),
            &format!("/static/{target_key}/"),
        )
        .into_bytes()
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

#[cfg(test)]
mod duplicate_posts_tests {
    use super::*;

    #[test]
    fn placeholder_used_in_text_rejects_partial_name_match() {
        let text = "{{ export_src_news_html | safe }}";
        assert!(!placeholder_used_in_text(text, "news"));
        assert!(placeholder_used_in_text(text, "export_src_news"));
    }

    #[test]
    fn remap_placeholder_name_replaces_source_prefix() {
        assert_eq!(
            remap_placeholder_name("corporate", "corporate_v2", "corporate_news"),
            "corporate_v2_news"
        );
        assert_eq!(
            remap_placeholder_name("corporate", "corporate_v2", "news"),
            "corporate_v2_news"
        );
    }

    #[test]
    fn rewrite_placeholder_refs_updates_html_and_has_prefix() {
        let pairs = vec![
            ("corporate_news_html".to_string(), "corporate_v2_news_html".to_string()),
            ("has_corporate_news".to_string(), "has_corporate_v2_news".to_string()),
        ];
        let text = "{{ corporate_news_html | safe }}{% if has_corporate_news %}";
        let rewritten = rewrite_placeholder_refs(text, &pairs);
        assert!(rewritten.contains("corporate_v2_news_html"));
        assert!(rewritten.contains("has_corporate_v2_news"));
        assert!(!rewritten.contains("corporate_news_html"));
    }
}


