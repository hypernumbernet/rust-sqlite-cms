//! 開発用環境リセットロジック（Rust ネイティブ実装）
//!
//! Python 版（utils/reset_test_db.py）より高速に動作することを目的とする。
//! 現在は "基本テストデータ" のみ提供。将来的に複数の Sample を追加可能。

use std::path::Path;

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// リセット結果のサマリ（UI 表示用）
#[derive(Debug, Clone)]
pub struct ResetResult {
    pub message: String,
    pub placeholders_count: i64,
    pub posts_count: i64,
    pub media_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedStrategy {
    Reset,
    Append,
}

struct PlaceholderSpec {
    name: &'static str,
    type_key: &'static str,
    config: &'static str,
}

const BASIC_PLACEHOLDERS: &[PlaceholderSpec] = &[
    PlaceholderSpec {
        name: "news",
        type_key: "news",
        config: r#"{"limit": 6}"#,
    },
    PlaceholderSpec {
        name: "announcements",
        type_key: "news",
        config: r#"{"limit": 10}"#,
    },
    PlaceholderSpec {
        name: "hero",
        type_key: "image",
        config: "{}",
    },
    PlaceholderSpec {
        name: "main_carousel",
        type_key: "carousel",
        config: r#"{"interval": 4, "width": "100%", "height": "420px"}"#,
    },
    PlaceholderSpec {
        name: "sidebar",
        type_key: "news",
        config: r#"{"limit": 4}"#,
    },
];

const BASIC_POST_SLUGS: &[&str] = &[
    "rust-cms-development-update",
    "new-placeholder-feature",
    "test-data-seed",
    "upcoming-widget-types",
    "maintenance-notice",
    "new-admin-ui",
    "welcome-to-test-env",
    "internal-roadmap",
    "quick-tip-1",
    "quick-tip-2",
    "hero-main",
    "slide-spring",
    "slide-dev",
    "slide-event",
];

const BASIC_MEDIA_FILES: &[&str] = &[
    "hero-sample.png",
    "carousel-1.png",
    "carousel-2.png",
    "carousel-3.png",
];

struct PlaceholderIds {
    news: i64,
    announcements: i64,
    hero: i64,
    carousel: i64,
    sidebar: i64,
}

/// 基本テストデータを適用する（現在唯一のサンプル）
pub async fn perform_basic_reset(state: &AppState) -> AppResult<ResetResult> {
    let config = &state.config;

    // 1. 既存の DB ファイルを削除（接続中のプールがあるため、再起動推奨を強く出す）
    let db_path = &config.database.path;
    if Path::new(db_path).exists() {
        std::fs::remove_file(db_path)?;
    }

    // 2. work/ 配下をクリアして再作成
    clear_and_recreate_work_dirs(&config.paths.work_dir, &config.paths.uploads_dir)?;

    // 3. 新規プールを作成してマイグレーションを再適用
    let fresh_pool = db::connect(db_path).await?;
    db::migrate(&fresh_pool).await?;

    // 4. テストデータ投入（Rustコードで直接INSERT - splitに依存しない方式）
    let mut tx = fresh_pool.begin().await?;
    let ids = resolve_placeholder_ids(&mut tx, SeedStrategy::Reset).await?;
    seed_basic_sample_content(&mut tx, &ids).await?;
    sqlx::query(
        "INSERT INTO options (option_name, option_value, autoload) 
         VALUES ('blogname', 'Rust SQLite CMS - テスト環境', 1)
         ON CONFLICT(option_name) DO UPDATE SET option_value = excluded.option_value",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // 5. テスト用画像ファイルを生成
    generate_test_images(&config.paths.uploads_dir)?;

    // 6. 件数を集計して返す
    let (placeholders_count, posts_count, media_count) = count_data(&fresh_pool).await?;

    Ok(ResetResult {
        message: "基本テストデータの適用が完了しました。サーバーを再起動することを強くおすすめします。".to_string(),
        placeholders_count,
        posts_count,
        media_count,
    })
}

/// 既存データを保持したまま基本テストデータを追加する（名前衝突時は中止）
pub async fn perform_basic_append(state: &AppState) -> AppResult<ResetResult> {
    let uploads_dir = &state.config.paths.uploads_dir;

    if let Err(conflicts) = check_basic_sample_conflicts(&state.pool, uploads_dir).await {
        return Err(AppError::Conflict(format!(
            "以下の名前が既に存在するため、サンプルの追加を中止しました: {}",
            conflicts.join(", ")
        )));
    }

    std::fs::create_dir_all(uploads_dir)?;

    let mut tx = state.pool.begin().await?;
    let ids = resolve_placeholder_ids(&mut tx, SeedStrategy::Append).await?;
    seed_basic_sample_content(&mut tx, &ids).await?;
    tx.commit().await?;

    generate_test_images(uploads_dir)?;

    let (placeholders_count, posts_count, media_count) = count_data(&state.pool).await?;

    Ok(ResetResult {
        message: "基本テストデータの追加が完了しました。サーバーの再起動は不要です。".to_string(),
        placeholders_count,
        posts_count,
        media_count,
    })
}

/// 基本サンプルと衝突する既存データを検出する
async fn check_basic_sample_conflicts(
    pool: &SqlitePool,
    uploads_dir: &str,
) -> Result<(), Vec<String>> {
    let mut conflicts = Vec::new();

    for spec in BASIC_PLACEHOLDERS {
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM placeholders WHERE name = ?")
            .bind(spec.name)
            .fetch_optional(pool)
            .await
            .map_err(|e| vec![e.to_string()])?;
        if exists.is_some() {
            conflicts.push(format!(r#"プレースホルダー "{}""#, spec.name));
        }
    }

    for slug in BASIC_POST_SLUGS {
        let exists: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM posts WHERE post_name = ? AND post_name IS NOT NULL",
        )
        .bind(slug)
        .fetch_optional(pool)
        .await
        .map_err(|e| vec![e.to_string()])?;
        if exists.is_some() {
            conflicts.push(format!(r#"投稿スラッグ "{}""#, slug));
        }
    }

    for file in BASIC_MEDIA_FILES {
        let exists: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT 1
            FROM postmeta pm
            JOIN posts p ON p.id = pm.post_id
            WHERE p.post_type = 'attachment'
              AND pm.meta_key = 'file_path'
              AND pm.meta_value = ?
            "#,
        )
        .bind(file)
        .fetch_optional(pool)
        .await
        .map_err(|e| vec![e.to_string()])?;
        if exists.is_some() {
            conflicts.push(format!(r#"メディアファイル "{}""#, file));
        }
    }

    let uploads = Path::new(uploads_dir);
    for file in BASIC_MEDIA_FILES {
        if uploads.join(file).exists() {
            conflicts.push(format!(r#"アップロードファイル "{}""#, file));
        }
    }

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(conflicts)
    }
}

/// work/ 配下をクリアして必要なディレクトリを再作成
fn clear_and_recreate_work_dirs(work_dir: &str, uploads_dir: &str) -> AppResult<()> {
    let work = Path::new(work_dir);

    // 存在すれば中身を削除（ディレクトリ自体は残して再利用）
    if work.exists() {
        for entry in std::fs::read_dir(work)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }

    // 必要なディレクトリを確実に作成
    std::fs::create_dir_all(work.join("templates/static"))?;
    std::fs::create_dir_all(work.join("pages"))?;
    std::fs::create_dir_all(uploads_dir)?;

    // 最低限の config.toml を生成（任意）
    let config_path = work.join("config.toml");
    if !config_path.exists() {
        let content = r#"[server]
bind_addr = "127.0.0.1:3000"

[site]
title = "Rust SQLite CMS - サンプルデータ"
tagline = "開発用テスト環境（サンプルからリセット）"
"#;
        std::fs::write(config_path, content)?;
    }

    // トップページテンプレートを presets からコピー
    let preset = crate::presets::HOME_INDEX;
    std::fs::write(work.join("templates/index.html"), preset)?;

    Ok(())
}

async fn resolve_placeholder_ids(
    tx: &mut Transaction<'_, Sqlite>,
    strategy: SeedStrategy,
) -> AppResult<PlaceholderIds> {
    match strategy {
        SeedStrategy::Reset => {
            let news = ensure_placeholder(tx, &BASIC_PLACEHOLDERS[0]).await?;
            let announcements = ensure_placeholder(tx, &BASIC_PLACEHOLDERS[1]).await?;
            let hero = ensure_placeholder(tx, &BASIC_PLACEHOLDERS[2]).await?;
            let carousel = ensure_placeholder(tx, &BASIC_PLACEHOLDERS[3]).await?;
            let sidebar = ensure_placeholder(tx, &BASIC_PLACEHOLDERS[4]).await?;
            Ok(PlaceholderIds {
                news,
                announcements,
                hero,
                carousel,
                sidebar,
            })
        }
        SeedStrategy::Append => {
            let news = insert_placeholder(tx, &BASIC_PLACEHOLDERS[0]).await?;
            let announcements = insert_placeholder(tx, &BASIC_PLACEHOLDERS[1]).await?;
            let hero = insert_placeholder(tx, &BASIC_PLACEHOLDERS[2]).await?;
            let carousel = insert_placeholder(tx, &BASIC_PLACEHOLDERS[3]).await?;
            let sidebar = insert_placeholder(tx, &BASIC_PLACEHOLDERS[4]).await?;
            Ok(PlaceholderIds {
                news,
                announcements,
                hero,
                carousel,
                sidebar,
            })
        }
    }
}

async fn seed_basic_sample_content(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &PlaceholderIds,
) -> AppResult<()> {
    let news_posts = [
        (
            "rust-cms-development-update",
            "Rust SQLite CMS 開発進捗（2026年春）",
            "管理画面の操作性が大幅に向上しました。",
            "2026-04-15T09:00:00Z",
            "publish",
        ),
        (
            "new-placeholder-feature",
            "プレースホルダー機能の大幅強化について",
            "画像・カルーセル・ニュースなど多様な表現に対応。",
            "2026-04-10T14:30:00Z",
            "publish",
        ),
        (
            "test-data-seed",
            "テストデータ投入スクリプトを追加",
            "Rustネイティブで高速にリセット可能になりました。",
            "2026-05-20T11:15:00Z",
            "publish",
        ),
        (
            "upcoming-widget-types",
            "今後追加予定のウィジェットタイプ",
            "地図、フォーム、FAQ などを計画中です。",
            "",
            "draft",
        ),
    ];

    for (slug, title, excerpt, published, status) in news_posts {
        insert_post(
            tx,
            ids.news,
            slug,
            title,
            excerpt,
            published,
            status,
        )
        .await?;
    }

    let announcements_posts = [
        (
            "maintenance-notice",
            "5月下旬 メンテナンスのお知らせ",
            "ご迷惑をおかけしますがよろしくお願いいたします。",
            "2026-05-25T23:00:00Z",
            "publish",
        ),
        (
            "new-admin-ui",
            "管理UIに「削除ボタン」が追加されました",
            "投稿一覧の右端操作列から直接削除可能になりました。",
            "2026-05-28T10:00:00Z",
            "publish",
        ),
        (
            "welcome-to-test-env",
            "テスト環境へようこそ",
            "このデータはRustコードで直接生成されています。",
            "2026-05-01T00:00:00Z",
            "publish",
        ),
        (
            "internal-roadmap",
            "内部ロードマップ（非公開）",
            "今後の開発予定をまとめた非公開メモ",
            "",
            "draft",
        ),
    ];

    for (slug, title, excerpt, published, status) in announcements_posts {
        insert_post(
            tx,
            ids.announcements,
            slug,
            title,
            excerpt,
            published,
            status,
        )
        .await?;
    }

    let sidebar_posts = [
        (
            "quick-tip-1",
            "MiniJinja テンプレート小技",
            "has_items / items 変数で安全にループできます。",
            "2026-05-18T08:00:00Z",
            "publish",
        ),
        (
            "quick-tip-2",
            "プレースホルダー名は一意に",
            "命名規則を守ると幸せになれます。",
            "2026-05-22T16:45:00Z",
            "publish",
        ),
    ];

    for (slug, title, excerpt, published, status) in sidebar_posts {
        insert_post(
            tx,
            ids.sidebar,
            slug,
            title,
            excerpt,
            published,
            status,
        )
        .await?;
    }

    let media_data = [
        ("hero-sample.png", "image/png", "hero-sample.png", 12345_i64),
        ("carousel-1.png", "image/png", "slide-1.png", 8901_i64),
        ("carousel-2.png", "image/png", "slide-2.png", 9012_i64),
        ("carousel-3.png", "image/png", "slide-3.png", 9123_i64),
    ];

    let mut media_ids = Vec::new();
    for (file, mime, orig, size) in media_data {
        let id = insert_media_attachment(tx, file, mime, orig, size).await?;
        media_ids.push(id);
    }

    let hero_media = media_ids[0];
    let carousel_media = &media_ids[1..];

    let hero_entry = insert_post(
        tx,
        ids.hero,
        "hero-main",
        "メインキービジュアル",
        "サイトを象徴する画像です",
        "2026-05-01T00:00:00Z",
        "publish",
    )
    .await?;
    insert_postmeta(tx, hero_entry, "media_id", &hero_media.to_string()).await?;
    insert_postmeta(tx, hero_entry, "float", "none").await?;
    insert_postmeta(tx, hero_entry, "margin", "0").await?;
    insert_postmeta(tx, hero_entry, "link_url", "https://example.com").await?;

    let carousel_slides = [
        ("slide-spring", "春の新生活キャンペーン"),
        ("slide-dev", "開発者向けアップデート"),
        ("slide-event", "6月 コミュニティミートアップ"),
    ];

    for (i, (slug, title)) in carousel_slides.iter().enumerate() {
        let media_id = carousel_media[i];
        let entry_id = insert_post(
            tx,
            ids.carousel,
            slug,
            title,
            "",
            "2026-05-10T00:00:00Z",
            "publish",
        )
        .await?;
        insert_postmeta(tx, entry_id, "media_id", &media_id.to_string()).await?;
        insert_postmeta(tx, entry_id, "alt", title).await?;
    }

    Ok(())
}

/// 指定された名前のプレースホルダーが存在しなければ作成する（リセット用）
async fn ensure_placeholder(
    tx: &mut Transaction<'_, Sqlite>,
    spec: &PlaceholderSpec,
) -> AppResult<i64> {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM placeholders WHERE name = ?")
        .bind(spec.name)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(id);
    }

    insert_placeholder(tx, spec).await
}

/// プレースホルダーを新規作成する（追加用）
async fn insert_placeholder(
    tx: &mut Transaction<'_, Sqlite>,
    spec: &PlaceholderSpec,
) -> AppResult<i64> {
    let id: i64 = sqlx::query_scalar(
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
    .await?;

    Ok(id)
}

/// work/uploads に最小限のテスト画像を配置
fn generate_test_images(uploads_dir: &str) -> AppResult<()> {
    let uploads = Path::new(uploads_dir);

    let tiny_png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
        0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
        0xD7, 0x63, 0xF8, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x37, 0x6E,
        0xF9, 0x24, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    for name in BASIC_MEDIA_FILES {
        std::fs::write(uploads.join(name), tiny_png)?;
    }

    Ok(())
}

async fn count_data(pool: &SqlitePool) -> AppResult<(i64, i64, i64)> {
    let ph: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM placeholders")
        .fetch_one(pool)
        .await?;
    let posts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash'",
    )
    .fetch_one(pool)
    .await?;
    let media: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE post_type = 'attachment'")
        .fetch_one(pool)
        .await?;
    Ok((ph, posts, media))
}

async fn insert_post(
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

    let id: i64 = sqlx::query_scalar(
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
    .await?;

    Ok(id)
}

async fn insert_media_attachment(
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

    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_path', ?)")
        .bind(id)
        .bind(file_path)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'mime_type', ?)")
        .bind(id)
        .bind(mime_type)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'original_name', ?)",
    )
    .bind(id)
    .bind(original_name)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_size', ?)")
        .bind(id)
        .bind(file_size.to_string())
        .execute(&mut **tx)
        .await?;

    Ok(id)
}

async fn insert_postmeta(
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
