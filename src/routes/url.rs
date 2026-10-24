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

fn is_reserved_system_path(path: &str) -> bool {
    path == "/admin"
        || path.starts_with("/admin/")
        || path == "/static"
        || path.starts_with("/static/")
        || path == "/uploads"
        || path.starts_with("/uploads/")
        || path == "/api"
        || path.starts_with("/api/")
}

/// 固定ページの URL に使えない予約パス。`/favicon.ico` はメディア専用ルートで配信する。
/// `/` はページ URL として使用可能。
pub fn is_reserved_path(path: &str) -> bool {
    path == "/favicon.ico" || is_reserved_system_path(path)
}

/// メディアの custom `public_url` に使えないパス（トップ `/` も含む）。
/// `/favicon.ico` はサイト favicon 用にメディアから割り当て可能。
pub fn is_reserved_media_public_url(path: &str) -> bool {
    path == "/" || is_reserved_system_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_paths_include_api_and_favicon() {
        assert!(is_reserved_path("/api"));
        assert!(is_reserved_path("/api/v1/session"));
        assert!(is_reserved_path("/favicon.ico"));
        assert!(!is_reserved_path("/about"));
        assert!(!is_reserved_path("/"));
    }

    #[test]
    fn media_reserved_includes_root_but_allows_favicon() {
        assert!(is_reserved_media_public_url("/"));
        assert!(is_reserved_media_public_url("/api/v1/session"));
        assert!(!is_reserved_media_public_url("/favicon.ico"));
        assert!(!is_reserved_media_public_url("/logo.png"));
    }
}
