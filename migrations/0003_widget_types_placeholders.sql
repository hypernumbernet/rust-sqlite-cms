-- ウィジェットタイプ（コードレジストリと 1:1、設定のみ DB 保持）
CREATE TABLE IF NOT EXISTS widget_types (
    id         INTEGER PRIMARY KEY,
    type_key   TEXT NOT NULL UNIQUE,
    config     TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- プレースホルダー（テンプレート参照名）
CREATE TABLE IF NOT EXISTS placeholders (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL UNIQUE,
    widget_type_id INTEGER NOT NULL REFERENCES widget_types(id),
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_placeholders_widget_type ON placeholders(widget_type_id);

-- 投稿をプレースホルダーに紐付け
ALTER TABLE posts ADD COLUMN placeholder_id INTEGER REFERENCES placeholders(id);
CREATE INDEX IF NOT EXISTS idx_posts_placeholder ON posts(placeholder_id);

-- widgets テーブルから移行（0002 適用済み環境向け）
INSERT INTO widget_types (type_key, config)
SELECT widget_type, config FROM widgets
WHERE NOT EXISTS (SELECT 1 FROM widget_types WHERE type_key = widgets.widget_type)
GROUP BY widget_type;

INSERT INTO placeholders (name, widget_type_id)
SELECT w.name, wt.id
FROM widgets w
JOIN widget_types wt ON wt.type_key = w.widget_type
WHERE NOT EXISTS (SELECT 1 FROM placeholders p WHERE p.name = w.name);

UPDATE posts
SET placeholder_id = (SELECT id FROM placeholders WHERE name = 'news')
WHERE post_type = 'post' AND placeholder_id IS NULL;

DROP TABLE IF EXISTS widgets;
