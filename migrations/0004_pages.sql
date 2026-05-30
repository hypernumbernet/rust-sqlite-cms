-- 固定ページ（静的 HTML）。本文は work/pages/ に保持し、DB にはメタ情報のみを持つ。

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
