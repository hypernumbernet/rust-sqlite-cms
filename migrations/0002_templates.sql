-- ユーザー編集可能な HTML テンプレート。
-- URL にマッピングし、公開フラグが立つと公開サイトのフォールバックルートで配信する。

CREATE TABLE IF NOT EXISTS templates (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,                 -- 例 "/about"。NULL 可（下書きは未設定でも複数可）
    content      TEXT    NOT NULL DEFAULT '',    -- MiniJinja/HTML ソース
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_templates_published ON templates(is_published);
