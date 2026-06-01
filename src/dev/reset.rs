//! 開発用環境リセットロジック（Rust ネイティブ実装）
//!
//! Python 版（utils/reset_test_db.py）より高速に動作することを目的とする。
//! 現在は "基本テストデータ" のみ提供。将来的に複数の Sample を追加可能。

use std::path::Path;

use sqlx::SqlitePool;

use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

/// リセット結果のサマリ（UI 表示用）
#[derive(Debug, Clone)]
pub struct ResetResult {
    pub message: String,
    pub placeholders_count: i64,
    pub posts_count: i64,
    pub media_count: i64,
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
    seed_basic_test_data(&fresh_pool).await?;

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

/// 基本テストデータをRustコードで直接投入する（seed SQLのsplitに一切依存しない方式）
/// 将来的に複数のサンプルを追加する場合も、この関数をベースに拡張しやすい。
async fn seed_basic_test_data(pool: &SqlitePool) -> AppResult<()> {
    // 1. 必要なプレースホルダーを確実に作成（存在しなければINSERT）
    let news_id = ensure_placeholder(pool, "news", "news", r#"{"limit": 6}"#).await?;
    let announcements_id = ensure_placeholder(pool, "announcements", "news", r#"{"limit": 10}"#).await?;
    let hero_id = ensure_placeholder(pool, "hero", "image", "{}").await?;
    let carousel_id = ensure_placeholder(pool, "main-carousel", "carousel", r#"{"interval": 4, "width": "100%", "height": "420px"}"#).await?;
    let sidebar_id = ensure_placeholder(pool, "sidebar", "news", r#"{"limit": 4}"#).await?;

    // 2. ニュース系・お知らせ系の投稿を直接INSERT（現実的な日本語テストデータ）
    let news_posts = vec![
        ("rust-cms-development-update", "Rust SQLite CMS 開発進捗（2026年春）", "管理画面の操作性が大幅に向上しました。", "2026-04-15T09:00:00Z", "publish"),
        ("new-placeholder-feature", "プレースホルダー機能の大幅強化について", "画像・カルーセル・ニュースなど多様な表現に対応。", "2026-04-10T14:30:00Z", "publish"),
        ("test-data-seed", "テストデータ投入スクリプトを追加", "Rustネイティブで高速にリセット可能になりました。", "2026-05-20T11:15:00Z", "publish"),
        ("upcoming-widget-types", "今後追加予定のウィジェットタイプ", "地図、フォーム、FAQ などを計画中です。", "", "draft"),
    ];

    for (slug, title, excerpt, published, status) in news_posts {
        insert_post(pool, news_id, slug, title, excerpt, published, status).await?;
    }

    let announcements_posts = vec![
        ("maintenance-notice", "5月下旬 メンテナンスのお知らせ", "ご迷惑をおかけしますがよろしくお願いいたします。", "2026-05-25T23:00:00Z", "publish"),
        ("new-admin-ui", "管理UIに「削除ボタン」が追加されました", "投稿一覧の右端操作列から直接削除可能になりました。", "2026-05-28T10:00:00Z", "publish"),
        ("welcome-to-test-env", "テスト環境へようこそ", "このデータはRustコードで直接生成されています。", "2026-05-01T00:00:00Z", "publish"),
        ("internal-roadmap", "内部ロードマップ（非公開）", "今後の開発予定をまとめた非公開メモ", "", "draft"),
    ];

    for (slug, title, excerpt, published, status) in announcements_posts {
        insert_post(pool, announcements_id, slug, title, excerpt, published, status).await?;
    }

    let sidebar_posts = vec![
        ("quick-tip-1", "MiniJinja テンプレート小技", "has_items / items 変数で安全にループできます。", "2026-05-18T08:00:00Z", "publish"),
        ("quick-tip-2", "プレースホルダー名は一意に", "命名規則を守ると幸せになれます。", "2026-05-22T16:45:00Z", "publish"),
    ];

    for (slug, title, excerpt, published, status) in sidebar_posts {
        insert_post(pool, sidebar_id, slug, title, excerpt, published, status).await?;
    }

    // 3. メディア（attachment）を4件作成
    let media_data = vec![
        ("hero-sample.png", "image/png", "hero-sample.png", 12345),
        ("carousel-1.png", "image/png", "slide-1.png", 8901),
        ("carousel-2.png", "image/png", "slide-2.png", 9012),
        ("carousel-3.png", "image/png", "slide-3.png", 9123),
    ];

    let mut media_ids = Vec::new();
    for (file, mime, orig, size) in media_data {
        let id = insert_media_attachment(pool, file, mime, orig, size).await?;
        media_ids.push(id);
    }

    let hero_media = media_ids[0];
    let carousel_media = &media_ids[1..];

    // 4. Hero画像エントリ + postmeta
    let hero_entry = insert_post(pool, hero_id, "hero-main", "メインキービジュアル", "サイトを象徴する画像です", "2026-05-01T00:00:00Z", "publish").await?;
    insert_postmeta(pool, hero_entry, "media_id", &hero_media.to_string()).await?;
    insert_postmeta(pool, hero_entry, "float", "none").await?;
    insert_postmeta(pool, hero_entry, "margin", "0").await?;
    insert_postmeta(pool, hero_entry, "link_url", "https://example.com").await?;

    // 5. カルーセル用スライド3枚
    let carousel_slides = vec![
        ("slide-spring", "春の新生活キャンペーン"),
        ("slide-dev", "開発者向けアップデート"),
        ("slide-event", "6月 コミュニティミートアップ"),
    ];

    for (i, (slug, title)) in carousel_slides.iter().enumerate() {
        let media_id = carousel_media[i];
        let entry_id = insert_post(pool, carousel_id, slug, title, "", "2026-05-10T00:00:00Z", "publish").await?;
        insert_postmeta(pool, entry_id, "media_id", &media_id.to_string()).await?;
        insert_postmeta(pool, entry_id, "alt", title).await?;
    }

    // 6. テスト用にサイト名を少し変更（任意）
    sqlx::query(
        "INSERT INTO options (option_name, option_value, autoload) 
         VALUES ('blogname', 'Rust SQLite CMS - テスト環境', 1)
         ON CONFLICT(option_name) DO UPDATE SET option_value = excluded.option_value"
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// 指定された名前のプレースホルダーが存在しなければ作成する。
/// widget_type の存在も前提とする（マイグレーションで保証されている）。
async fn ensure_placeholder(
    pool: &SqlitePool,
    name: &str,
    type_key: &str,
    config_json: &str,
) -> AppResult<i64> {
    // 既に存在すればそのまま返す
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM placeholders WHERE name = ?"
    )
    .bind(name)
    .fetch_optional(pool)
    .await? {
        return Ok(id);
    }

    // 存在しなければ INSERT（widget_type_id をサブクエリで取得）
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO placeholders (name, widget_type_id, config, created_at, updated_at)
        SELECT ?, id, ?, datetime('now'), datetime('now')
        FROM widget_types 
        WHERE type_key = ?
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(config_json)
    .bind(type_key)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// 画像・カルーセル用の実データ構築（Python版のロジックをRust移植）
async fn build_rich_visual_data(pool: &SqlitePool) -> AppResult<()> {
    // まず、hero / main-carousel プレースホルダーが存在することを保証する
    // （seed SQLの実行が不完全でもこのリセット機能が自己完結するようにする）
    let hero_id = ensure_placeholder(pool, "hero", "image", "{}").await?;
    let carousel_id = ensure_placeholder(pool, "main-carousel", "carousel", r#"{"interval": 4, "width": "100%", "height": "420px"}"#).await?;

    // 既存の hero / main-carousel の仮データをクリア
    sqlx::query(
        r#"DELETE FROM posts 
           WHERE placeholder_id IN (?, ?) AND post_type = 'post'"#,
    )
    .bind(hero_id)
    .bind(carousel_id)
    .execute(pool)
    .await?;

    // テスト画像を media (attachment) として登録
    let test_images = vec![
        ("hero-sample.png", "image/png", "hero-sample.png", 12345),
        ("carousel-1.png", "image/png", "slide-1.png", 8901),
        ("carousel-2.png", "image/png", "slide-2.png", 9012),
        ("carousel-3.png", "image/png", "slide-3.png", 9123),
    ];

    let mut media_ids = Vec::new();

    for (file_name, mime, orig, size) in test_images {
        let row: (i64,) = sqlx::query_as(
            r#"INSERT INTO posts (post_type, post_status, title, created_at, updated_at)
               VALUES ('attachment', 'inherit', ?, datetime('now'), datetime('now'))
               RETURNING id"#,
        )
        .bind(file_name)
        .fetch_one(pool)
        .await?;

        let post_id = row.0;
        media_ids.push(post_id);

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_path', ?)",
        )
        .bind(post_id)
        .bind(file_name)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'mime_type', ?)",
        )
        .bind(post_id)
        .bind(mime)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'original_name', ?)",
        )
        .bind(post_id)
        .bind(orig)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_size', ?)",
        )
        .bind(post_id)
        .bind(size.to_string())
        .execute(pool)
        .await?;
    }

    let hero_media_id = media_ids[0];
    let carousel_ids = &media_ids[1..];

    // hero エントリ作成（すでに ensure 済みの ID を使用）
    let hero_entry_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO posts (post_type, post_status, post_name, title, content, excerpt, published_at, placeholder_id, created_at, updated_at)
           VALUES ('post', 'publish', 'hero-main', 'メインキービジュアル', '', 'サイトを象徴する画像です', '2026-05-01T00:00:00Z', ?, datetime('now'), datetime('now'))
           RETURNING id"#,
    )
    .bind(hero_id)
    .fetch_one(pool)
    .await?;

    for (key, value) in [
        ("media_id", hero_media_id.to_string()),
        ("float", "none".to_string()),
        ("margin", "0".to_string()),
        ("link_url", "https://example.com".to_string()),
    ] {
        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)",
        )
        .bind(hero_entry_id)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    }

    // main-carousel のスライド3枚（すでに ensure 済みの ID を使用）
    let slides = [
        ("slide-spring", "春の新生活キャンペーン"),
        ("slide-dev", "開発者向けアップデート"),
        ("slide-event", "6月 コミュニティミートアップ"),
    ];

    for (i, (slug, title)) in slides.iter().enumerate() {
        let media_id = carousel_ids[i];

        let entry_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO posts (post_type, post_status, post_name, title, content, excerpt, published_at, placeholder_id, created_at, updated_at)
               VALUES ('post', 'publish', ?, ?, '', '', '2026-05-10T00:00:00Z', ?, datetime('now'), datetime('now'))
               RETURNING id"#,
        )
        .bind(slug)
        .bind(title)
        .bind(carousel_id)
        .fetch_one(pool)
        .await?;

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'media_id', ?)",
        )
        .bind(entry_id)
        .bind(media_id.to_string())
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'alt', ?)",
        )
        .bind(entry_id)
        .bind(title)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// work/uploads に最小限のテスト画像を配置
fn generate_test_images(uploads_dir: &str) -> AppResult<()> {
    let uploads = Path::new(uploads_dir);

    // 超軽量の1色PNG（1x1 相当の最小データ）を埋め込み
    // 本物のPNGヘッダを持つ最小限データ（実用上問題ないサイズ）
    let tiny_png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00,
        0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x37, 0x6E, 0xF9, 0x24, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    for name in ["hero-sample.png", "carousel-1.png", "carousel-2.png", "carousel-3.png"] {
        std::fs::write(uploads.join(name), tiny_png)?;
    }

    Ok(())
}

async fn count_data(pool: &SqlitePool) -> AppResult<(i64, i64, i64)> {
    let ph: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM placeholders").fetch_one(pool).await?;
    let posts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE post_type = 'post' AND post_status != 'trash'").fetch_one(pool).await?;
    let media: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE post_type = 'attachment'").fetch_one(pool).await?;
    Ok((ph, posts, media))
}

// ============================================================
// seed_basic_test_data 用の小さなヘルパー（直接INSERT方式）
// ============================================================

async fn insert_post(
    pool: &SqlitePool,
    placeholder_id: i64,
    post_name: &str,
    title: &str,
    excerpt: &str,
    published_at: &str,
    status: &str,
) -> AppResult<i64> {
    let pub_at: Option<&str> = if published_at.is_empty() { None } else { Some(published_at) };

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
    .fetch_one(pool)
    .await?;

    Ok(id)
}

async fn insert_media_attachment(
    pool: &SqlitePool,
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
    .fetch_one(pool)
    .await?;

    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_path', ?)")
        .bind(id).bind(file_path).execute(pool).await?;
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'mime_type', ?)")
        .bind(id).bind(mime_type).execute(pool).await?;
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'original_name', ?)")
        .bind(id).bind(original_name).execute(pool).await?;
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, 'file_size', ?)")
        .bind(id).bind(file_size.to_string()).execute(pool).await?;

    Ok(id)
}

async fn insert_postmeta(
    pool: &SqlitePool,
    post_id: i64,
    key: &str,
    value: &str,
) -> AppResult<()> {
    sqlx::query("INSERT INTO postmeta (post_id, meta_key, meta_value) VALUES (?, ?, ?)")
        .bind(post_id)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}
