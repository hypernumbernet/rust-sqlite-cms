-- レイアウト単位の公開サイト構造（doc/LAYOUT_SPEC.md）

CREATE TABLE IF NOT EXISTS layouts (
    id           INTEGER PRIMARY KEY,
    key          TEXT    NOT NULL UNIQUE,
    name         TEXT    NOT NULL DEFAULT '',
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO layouts (key, name, is_default)
VALUES ('default', '既定', 1);

-- pages: layout_id 追加・is_static 廃止・file_name をレイアウト内相対パスに統一
CREATE TABLE pages_new (
    id           INTEGER PRIMARY KEY,
    layout_id    INTEGER NOT NULL REFERENCES layouts(id) ON DELETE RESTRICT,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,
    file_name    TEXT    NOT NULL,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (layout_id, file_name)
);

INSERT INTO pages_new (id, layout_id, name, url_path, file_name, is_published, created_at, updated_at)
SELECT
    p.id,
    (SELECT id FROM layouts WHERE key = 'default'),
    p.name,
    CASE
        WHEN p.file_name = 'index.html' THEN '/'
        ELSE p.url_path
    END,
    CASE
        WHEN p.file_name = 'index.html' THEN 'pages/index.html'
        WHEN p.file_name LIKE 'pages/%' THEN p.file_name
        ELSE 'pages/' || p.file_name
    END,
    p.is_published,
    p.created_at,
    p.updated_at
FROM pages p;

DROP TABLE pages;
ALTER TABLE pages_new RENAME TO pages;

CREATE INDEX IF NOT EXISTS idx_pages_published ON pages(is_published);
CREATE INDEX IF NOT EXISTS idx_pages_layout ON pages(layout_id);
