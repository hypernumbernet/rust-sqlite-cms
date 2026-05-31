-- 統合初期スキーマ
-- 元々は 0001/0002/0003 に分割されていたが、単一ファイルに統合した。
-- 日時はすべて UTC の ISO8601 文字列(TEXT)で保持する。

-- ユーザーアカウント -------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY,
    username      TEXT    NOT NULL UNIQUE,
    email         TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    display_name  TEXT    NOT NULL DEFAULT '',
    role          TEXT    NOT NULL DEFAULT 'subscriber',
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS usermeta (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    meta_key   TEXT    NOT NULL,
    meta_value TEXT
);
CREATE INDEX IF NOT EXISTS idx_usermeta_user_key ON usermeta(user_id, meta_key);

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

-- 投稿・固定ページ・添付の共通テーブル -------------------------------------
CREATE TABLE IF NOT EXISTS posts (
    id             INTEGER PRIMARY KEY,
    post_author    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    post_type      TEXT    NOT NULL DEFAULT 'post',   -- post / page / attachment ...
    post_status    TEXT    NOT NULL DEFAULT 'draft',  -- draft / publish / future / trash
    post_parent    INTEGER REFERENCES posts(id) ON DELETE SET NULL,
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
CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(post_parent);
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

-- タクソノミー(カテゴリ・タグ) ----------------------------------------------
CREATE TABLE IF NOT EXISTS terms (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL,
    slug  TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS term_taxonomy (
    id          INTEGER PRIMARY KEY,
    term_id     INTEGER NOT NULL REFERENCES terms(id) ON DELETE CASCADE,
    taxonomy    TEXT    NOT NULL,            -- category / post_tag ...
    description TEXT    NOT NULL DEFAULT '',
    parent_id   INTEGER REFERENCES term_taxonomy(id) ON DELETE SET NULL,
    count       INTEGER NOT NULL DEFAULT 0,
    UNIQUE(term_id, taxonomy)
);
CREATE INDEX IF NOT EXISTS idx_term_taxonomy_taxonomy ON term_taxonomy(taxonomy);

CREATE TABLE IF NOT EXISTS term_relationships (
    post_id          INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    term_taxonomy_id INTEGER NOT NULL REFERENCES term_taxonomy(id) ON DELETE CASCADE,
    term_order       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (post_id, term_taxonomy_id)
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
