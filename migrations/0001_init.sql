-- 統合初期スキーマ（旧 0001_init + 0002_image_widget を単一ファイル化）
-- 日時はすべて UTC の ISO8601 文字列(TEXT)で保持する。

-- ウィジェットタイプ（コードレジストリと 1:1、設定のみ DB 保持） -------------
CREATE TABLE IF NOT EXISTS widget_types (
    id         INTEGER PRIMARY KEY,
    type_key   TEXT NOT NULL UNIQUE,
    config     TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- プレースホルダー（テンプレート参照名） ------------------------------------
CREATE TABLE IF NOT EXISTS placeholders (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL UNIQUE,
    widget_type_id INTEGER NOT NULL REFERENCES widget_types(id),
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_placeholders_widget_type ON placeholders(widget_type_id);

-- 投稿エントリ・メディア添付の共通テーブル ---------------------------------
CREATE TABLE IF NOT EXISTS posts (
    id             INTEGER PRIMARY KEY,
    post_type      TEXT    NOT NULL DEFAULT 'post',   -- post / attachment
    post_status    TEXT    NOT NULL DEFAULT 'draft',  -- draft / publish / future / trash
    post_name      TEXT,                              -- スラッグ
    title          TEXT    NOT NULL DEFAULT '',
    content        TEXT    NOT NULL DEFAULT '',
    excerpt        TEXT    NOT NULL DEFAULT '',
    menu_order     INTEGER NOT NULL DEFAULT 0,
    published_at   TEXT,                              -- 公開日時(予約投稿で利用)
    placeholder_id INTEGER REFERENCES placeholders(id),
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_posts_type_status ON posts(post_type, post_status);
CREATE INDEX IF NOT EXISTS idx_posts_name ON posts(post_name);
CREATE INDEX IF NOT EXISTS idx_posts_placeholder ON posts(placeholder_id);

CREATE TABLE IF NOT EXISTS postmeta (
    id         INTEGER PRIMARY KEY,
    post_id    INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    meta_key   TEXT    NOT NULL,
    meta_value TEXT
);
CREATE INDEX IF NOT EXISTS idx_postmeta_post_key ON postmeta(post_id, meta_key);

-- サイト設定(key-value) ----------------------------------------------------
CREATE TABLE IF NOT EXISTS options (
    option_id    INTEGER PRIMARY KEY,
    option_name  TEXT    NOT NULL UNIQUE,
    option_value TEXT    NOT NULL DEFAULT '',
    autoload     INTEGER NOT NULL DEFAULT 1
);

-- ページ（テンプレート / 固定ページ。本文は work/ 配下、DB にはメタ情報のみ） ----
-- is_static=0: work/templates/ + MiniJinja、is_static=1: work/pages/ + 生 HTML
CREATE TABLE IF NOT EXISTS pages (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,
    file_name    TEXT    UNIQUE,
    is_static    INTEGER NOT NULL DEFAULT 0,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_pages_published ON pages(is_published);

-- 初期シードデータ ---------------------------------------------------------
-- テンプレート（presets/index.html など）が参照する "news" プレースホルダー
INSERT INTO widget_types (type_key, config)
VALUES ('news', '{"limit":5}');

INSERT INTO placeholders (name, widget_type_id)
SELECT 'news', id FROM widget_types WHERE type_key = 'news';

-- 画像ウィジェットタイプ（単一表示）
INSERT INTO widget_types (type_key, config)
SELECT 'image', '{}'
WHERE NOT EXISTS (SELECT 1 FROM widget_types WHERE type_key = 'image');
