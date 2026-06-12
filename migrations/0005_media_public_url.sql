-- favicon はレイアウトではなくメディアの public_url（postmeta）で管理する
ALTER TABLE layouts DROP COLUMN favicon_media_id;