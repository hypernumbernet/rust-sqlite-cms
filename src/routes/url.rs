/// 入力された URL を正規化する。空なら `None`、先頭スラッシュ付与・末尾スラッシュ除去。
pub fn normalize_url_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut path = trimmed.to_string();
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    if path.len() > 1 {
        path = path.trim_end_matches('/').to_string();
    }

    Some(path)
}

/// システム（公開トップ・管理画面）が使用する予約済みパスかどうか。
pub fn is_reserved_path(path: &str) -> bool {
    path == "/"
        || path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
}
