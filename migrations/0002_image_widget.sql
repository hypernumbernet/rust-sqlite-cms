-- 画像ウィジェットタイプ（単一表示）
INSERT INTO widget_types (type_key, config)
SELECT 'image', '{}'
WHERE NOT EXISTS (SELECT 1 FROM widget_types WHERE type_key = 'image');
