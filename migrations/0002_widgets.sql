-- サイト全体で共有するウィジェット（プレースホルダー）定義
CREATE TABLE IF NOT EXISTS widgets (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    widget_type  TEXT NOT NULL,
    config       TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- 既存 presets/index.html との後方互換
INSERT INTO widgets (name, widget_type, config)
VALUES ('news', 'news', '{"limit":5}');
