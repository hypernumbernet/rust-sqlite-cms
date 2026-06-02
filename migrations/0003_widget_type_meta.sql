-- widget_types に表示用メタデータを追加（エクスポート/インポート・カスタムタイプ対応）
ALTER TABLE widget_types ADD COLUMN label TEXT NOT NULL DEFAULT '';
ALTER TABLE widget_types ADD COLUMN description TEXT NOT NULL DEFAULT '';

UPDATE widget_types
SET label = 'お知らせ',
    description = '公開済みの投稿を新しい順に表示します'
WHERE type_key = 'news';

UPDATE widget_types
SET label = '画像',
    description = 'メディアライブラリの画像を1枚表示します。回り込み・マージンを設定できます'
WHERE type_key = 'image';

UPDATE widget_types
SET label = '画像カルーセル',
    description = '複数の画像をスライドショー形式で表示します。各画像にリンクを設定可能'
WHERE type_key = 'carousel';
