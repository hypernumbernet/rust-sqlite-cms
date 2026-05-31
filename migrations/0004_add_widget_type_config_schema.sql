-- 各ウィジェットタイプがインスタンス（プレースホルダー）で持つべき設定項目のスキーマを定義
-- これにより、プレースホルダー編集画面で動的に入力フォームを生成できる

ALTER TABLE widget_types ADD COLUMN config_schema TEXT NOT NULL DEFAULT '{}';

-- 既存レコードに初期スキーマを投入
-- news: 表示件数
UPDATE widget_types
SET config_schema = '{
  "fields": [
    {
      "key": "limit",
      "label": "表示件数",
      "type": "number",
      "default": 5,
      "min": 1,
      "max": 50,
      "help": "1〜50の範囲で指定してください"
    }
  ]
}'
WHERE type_key = 'news';

-- carousel: スライド間隔・サイズ
UPDATE widget_types
SET config_schema = '{
  "fields": [
    {
      "key": "interval",
      "label": "スライド間隔（秒）",
      "type": "number",
      "default": 5,
      "min": 1,
      "max": 30,
      "help": "1〜30秒"
    },
    {
      "key": "width",
      "label": "領域の幅",
      "type": "text",
      "default": "100%",
      "help": "例: 100%, 600px, 50vw"
    },
    {
      "key": "height",
      "label": "領域の高さ",
      "type": "text",
      "default": "400px",
      "help": "例: 400px, 50vh"
    }
  ]
}'
WHERE type_key = 'carousel';

-- image: インスタンスレベルの共通設定は基本的にない（個別エントリのpostmetaで制御）
UPDATE widget_types
SET config_schema = '{
  "fields": []
}'
WHERE type_key = 'image';

-- 注意: 将来的に新しいウィジェットタイプを追加する場合は、config_schema も適切に定義して INSERT してください。
-- スキーマの "type" として現在サポート: "number", "text", "select" (options配列が必要)