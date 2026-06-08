-- ユーザーテーブル（および CMS コアテーブル）の DB 管理 UI メタデータ
CREATE TABLE IF NOT EXISTS _user_table_meta (
    table_name          TEXT PRIMARY KEY NOT NULL,
    column_widths_json  TEXT NOT NULL DEFAULT '{}',
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
