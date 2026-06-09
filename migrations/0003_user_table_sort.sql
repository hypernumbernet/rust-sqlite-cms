-- ユーザーテーブルデータ画面の列ソート設定
ALTER TABLE _user_table_meta ADD COLUMN sort_json TEXT NOT NULL DEFAULT '[]';
