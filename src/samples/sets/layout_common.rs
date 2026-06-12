//! レイアウトセットインストールの共通ロジック。

use std::collections::HashMap;

use sqlx::{Sqlite, Transaction};

use crate::error::AppResult;
use crate::models::layout::LayoutInput;
use crate::samples::conflict;
use crate::services::layouts;
use crate::state::AppState;

/// プレースホルダー定義。
pub struct PlaceholderSpec {
    pub name: &'static str,
    pub type_key: &'static str,
    pub config: &'static str,
}

/// ページ定義。
pub struct PageSpec {
    pub name: &'static str,
    pub url_path: &'static str,
    pub file_name: &'static str,
    pub content: &'static str,
}

/// レイアウトセットの静的メタデータと資産。
pub struct LayoutSetSpec {
    pub layout_key: &'static str,
    pub layout_name: &'static str,
    pub shell_html: &'static str,
    pub site_css: &'static str,
    pub preview_path: &'static str,
    pub success_message: &'static str,
    pub placeholders: &'static [PlaceholderSpec],
    pub post_slugs: &'static [&'static str],
    pub media_files: &'static [&'static str],
    pub page_url_paths: &'static [&'static str],
    pub pages: &'static [PageSpec],
}

/// シード用プレースホルダー ID 束。
pub struct PlaceholderIds {
    pub news: i64,
    pub announcements: i64,
    pub hero: i64,
    pub carousel: i64,
    pub sidebar: i64,
    pub contact: i64,
}

/// レイアウトセットをインストールする。
pub async fn install(state: &AppState, spec: &LayoutSetSpec) -> AppResult<super::super::InstallResult> {
    let pool = state.pool();
    let config = &state.config;
    let work_dir = &config.paths.work_dir;
    let uploads_dir = &config.paths.uploads_dir;

    if let Err(conflicts) = check_conflicts(&pool, spec, uploads_dir).await {
        return Err(conflict::abort(conflicts));
    }

    std::fs::create_dir_all(uploads_dir)?;

    let mut static_files = HashMap::new();
    static_files.insert("site.css".to_string(), spec.site_css.to_string());

    let layout_input = LayoutInput {
        key: spec.layout_key.to_string(),
        name: spec.layout_name.to_string(),
    };

    layouts::create_layout(
        &pool,
        config,
        &layout_input,
        spec.shell_html,
        &static_files,
    )
    .await
    .map_err(crate::error::AppError::from)?;

    let mut tx = pool.begin().await?;
    let ids = insert_placeholders(&mut tx, spec.placeholders).await?;
    seed_layout_content(spec.layout_key, &mut tx, &ids).await?;
    insert_pages(&mut tx, work_dir, spec.layout_key, spec.pages).await?;
    tx.commit().await?;

    write_sample_images(uploads_dir, spec.media_files)?;

    Ok(super::super::InstallResult::Layout {
        message: spec.success_message.to_string(),
        layout_key: spec.layout_key.to_string(),
        preview_path: spec.preview_path.to_string(),
        placeholders_count: spec.placeholders.len() as i64,
        posts_count: spec.post_slugs.len() as i64,
        media_count: spec.media_files.len() as i64,
        pages_count: spec.pages.len() as i64,
    })
}

async fn seed_layout_content(
    layout_key: &str,
    tx: &mut Transaction<'_, Sqlite>,
    ids: &PlaceholderIds,
) -> AppResult<()> {
    match layout_key {
        "corporate" => super::corporate::seed_content(tx, ids).await,
        "bicycle" => super::bicycle::seed_content(tx, ids).await,
        other => Err(crate::error::AppError::Conflict(format!(
            "不明なレイアウトセットです: {other}"
        ))),
    }
}

async fn check_conflicts(
    pool: &sqlx::SqlitePool,
    spec: &LayoutSetSpec,
    uploads_dir: &str,
) -> Result<(), Vec<String>> {
    let mut conflicts = Vec::new();

    if crate::repos::layouts::find_by_key(pool, spec.layout_key)
        .await
        .map_err(|e| vec![e.to_string()])?
        .is_some()
    {
        conflicts.push(format!(r#"レイアウト "{}""#, spec.layout_key));
    }

    let placeholder_names: Vec<&str> = spec.placeholders.iter().map(|p| p.name).collect();
    for name in conflict::existing_values(pool, "placeholders", "name", &placeholder_names)
        .await
        .map_err(|e| vec![e])?
    {
        conflicts.push(format!(r#"プレースホルダー "{}""#, name));
    }

    for slug in conflict::existing_values(pool, "posts", "post_name", spec.post_slugs)
        .await
        .map_err(|e| vec![e])?
    {
        conflicts.push(format!(r#"投稿スラッグ "{}""#, slug));
    }

    if !spec.media_files.is_empty() {
        let mut builder = sqlx::QueryBuilder::new(
            r#"
            SELECT pm.meta_value
            FROM postmeta pm
            JOIN posts p ON p.id = pm.post_id
            WHERE p.post_type = 'attachment'
              AND pm.meta_key = 'file_path'
              AND pm.meta_value IN (
            "#,
        );
        let mut separated = builder.separated(", ");
        for file in spec.media_files {
            separated.push_bind(file);
        }
        builder.push(")");

        for file in builder
            .build_query_scalar::<String>()
            .fetch_all(pool)
            .await
            .map_err(|e| vec![e.to_string()])?
        {
            conflicts.push(format!(r#"メディアファイル "{}""#, file));
        }
    }

    conflicts.extend(conflict::existing_upload_files(uploads_dir, spec.media_files));

    let page_paths: Vec<&str> = spec.page_url_paths.to_vec();
    for path in conflict::existing_values(pool, "pages", "url_path", &page_paths)
        .await
        .map_err(|e| vec![e])?
    {
        conflicts.push(format!(r#"ページ URL "{}""#, path));
    }

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(conflicts)
    }
}

async fn insert_placeholders(
    tx: &mut Transaction<'_, Sqlite>,
    specs: &[PlaceholderSpec],
) -> AppResult<PlaceholderIds> {
    let news = insert_placeholder(tx, &specs[0]).await?;
    let announcements = insert_placeholder(tx, &specs[1]).await?;
    let hero = insert_placeholder(tx, &specs[2]).await?;
    let carousel = insert_placeholder(tx, &specs[3]).await?;
    let sidebar = insert_placeholder(tx, &specs[4]).await?;
    let contact = insert_placeholder(tx, &specs[5]).await?;
    Ok(PlaceholderIds {
        news,
        announcements,
        hero,
        carousel,
        sidebar,
        contact,
    })
}

async fn insert_pages(
    tx: &mut Transaction<'_, Sqlite>,
    work_dir: &str,
    layout_key: &str,
    pages: &[PageSpec],
) -> AppResult<()> {
    for spec in pages {
        sqlx::query(
            r#"
            INSERT INTO pages (name, url_path, file_name, layout_id, is_published)
            SELECT ?, ?, ?, id, 1 FROM layouts WHERE key = ?
            "#,
        )
        .bind(spec.name)
        .bind(spec.url_path)
        .bind(spec.file_name)
        .bind(layout_key)
        .execute(&mut **tx)
        .await?;

        crate::theme::write_page_body(work_dir, layout_key, spec.file_name, spec.content)?;
    }
    Ok(())
}

async fn insert_placeholder(
    tx: &mut Transaction<'_, Sqlite>,
    spec: &PlaceholderSpec,
) -> AppResult<i64> {
    sqlx::query_scalar(
        r#"
        INSERT INTO placeholders (name, widget_type_id, config, created_at, updated_at)
        SELECT ?, id, ?, datetime('now'), datetime('now')
        FROM widget_types
        WHERE type_key = ?
        RETURNING id
        "#,
    )
    .bind(spec.name)
    .bind(spec.config)
    .bind(spec.type_key)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x37, 0x6E, 0xF9, 0x24, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn write_sample_images(uploads_dir: &str, files: &[&str]) -> AppResult<()> {
    let uploads = std::path::Path::new(uploads_dir);
    for name in files {
        std::fs::write(uploads.join(name), TINY_PNG)?;
    }
    Ok(())
}

pub async fn insert_post(
    tx: &mut Transaction<'_, Sqlite>,
    placeholder_id: i64,
    post_name: &str,
    title: &str,
    excerpt: &str,
    published_at: &str,
    status: &str,
) -> AppResult<i64> {
    let pub_at: Option<&str> = if published_at.is_empty() {
        None
    } else {
        Some(published_at)
    };

    sqlx::query_scalar(
        r#"
        INSERT INTO posts (post_type, post_status, post_name, title, content, excerpt, published_at, placeholder_id, created_at, updated_at)
        VALUES ('post', ?, ?, ?, '', ?, ?, ?, datetime('now'), datetime('now'))
        RETURNING id
        "#,
    )
    .bind(status)
    .bind(post_name)
    .bind(title)
    .bind(excerpt)
    .bind(pub_at)
    .bind(placeholder_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

pub async fn insert_media_attachment(
    tx: &mut Transaction<'_, Sqlite>,
    file_path: &str,
    mime_type: &str,
    original_name: &str,
    file_size: i64,
) -> AppResult<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO posts (post_type, post_status, title, created_at, updated_at)
        VALUES ('attachment', 'inherit', ?, datetime('now'), datetime('now'))
        RETURNING id
        "#,
    )
    .bind(file_path)
    .fetch_one(&mut **tx)
    .await?;

    insert_postmeta(tx, id, "file_path", file_path).await?;
    insert_postmeta(tx, id, "mime_type", mime_type).await?;
    insert_postmeta(tx, id, "original_name", original_name).await?;
    insert_postmeta(tx, id, "file_size", &file_size.to_string()).await?;

    Ok(id)
}

pub async fn insert_postmeta(
    tx: &mut Transaction<'_, Sqlite>,
    post_id: i64,
    key: &str,
    value: &str,
) -> AppResult<()> {
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)")
        .bind(post_id)
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    Ok(())
}