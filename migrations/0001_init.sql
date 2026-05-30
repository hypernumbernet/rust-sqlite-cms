-- 初期スキーマ: Phase 1/2 のコアテーブル。
-- 日時はすべて UTC の ISO8601 文字列(TEXT)で保持する想定。

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

-- 投稿・固定ページ・添付の共通テーブル -------------------------------------
CREATE TABLE IF NOT EXISTS posts (
    id           INTEGER PRIMARY KEY,
    post_author  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    post_type    TEXT    NOT NULL DEFAULT 'post',   -- post / page / attachment ...
    post_status  TEXT    NOT NULL DEFAULT 'draft',  -- draft / publish / future / trash
    post_parent  INTEGER REFERENCES posts(id) ON DELETE SET NULL,
    post_name    TEXT,                              -- スラッグ
    title        TEXT    NOT NULL DEFAULT '',
    content      TEXT    NOT NULL DEFAULT '',
    excerpt      TEXT    NOT NULL DEFAULT '',
    menu_order   INTEGER NOT NULL DEFAULT 0,
    published_at TEXT,                              -- 公開日時(予約投稿で利用)
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_posts_type_status ON posts(post_type, post_status);
CREATE INDEX IF NOT EXISTS idx_posts_name ON posts(post_name);
CREATE INDEX IF NOT EXISTS idx_posts_parent ON posts(post_parent);

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

-- コメント -----------------------------------------------------------------
CREATE TABLE IF NOT EXISTS comments (
    id           INTEGER PRIMARY KEY,
    post_id      INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    parent_id    INTEGER REFERENCES comments(id) ON DELETE CASCADE,
    user_id      INTEGER REFERENCES users(id) ON DELETE SET NULL,
    author_name  TEXT    NOT NULL DEFAULT '',
    author_email TEXT    NOT NULL DEFAULT '',
    author_url   TEXT    NOT NULL DEFAULT '',
    content      TEXT    NOT NULL DEFAULT '',
    status       TEXT    NOT NULL DEFAULT 'hold',  -- hold / approve / spam / trash
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_comments_post_status ON comments(post_id, status);

-- HTML テンプレート（本文は work/templates/、DB にはメタ情報のみ） ----------------
CREATE TABLE IF NOT EXISTS templates (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,                 -- 例 "/about"。NULL 可（下書きは未設定でも複数可）
    file_name    TEXT,                           -- work/templates/ 内のファイル名
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_templates_published ON templates(is_published);

-- 固定ページ（静的 HTML。本文は work/pages/、DB にはメタ情報のみ） ---------------
CREATE TABLE IF NOT EXISTS pages (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,
    file_name    TEXT,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_pages_published ON pages(is_published);
