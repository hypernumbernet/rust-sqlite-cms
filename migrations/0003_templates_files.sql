-- テンプレート本文をファイル（work/templates/）へ移行する。
-- DB には URL・公開フラグなどのメタ情報と、対応する物理ファイル名のみを残す。

ALTER TABLE templates ADD COLUMN file_name TEXT;
ALTER TABLE templates DROP COLUMN content;
