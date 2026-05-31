-- 画像カルーセルウィジェットタイプを追加（存在しなければ）
INSERT INTO widget_types (type_key, config)
SELECT 'carousel', '{"interval":5,"width":"100%","height":"400px"}'
WHERE NOT EXISTS (SELECT 1 FROM widget_types WHERE type_key = 'carousel');