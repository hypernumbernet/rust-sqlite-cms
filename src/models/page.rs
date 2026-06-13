/// `pages` テーブルの行に対応する。本文は `layout_id` が指すレイアウト配下の
/// `work/layouts/{key}/pages/` に保持し、DB にはメタ情報のみを持つ。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Page {
    pub id: i64,
    pub layout_id: i64,
    pub name: String,
    pub url_path: Option<String>,
    /// レイアウトディレクトリからの相対パス（例: `pages/index.html`）。
    pub file_name: String,
    pub is_published: bool,
    pub created_at: String,
    pub updated_at: String,
    /// 所属レイアウトの key（`work/layouts/{key}/`）。取得クエリは layouts と JOIN する。
    pub layout_key: String,
}

/// ページ作成・更新時にリポジトリへ渡す入力値。
/// `content` は DB ではなくファイルへ書き込む。
#[derive(Debug, Clone)]
pub struct PageInput {
    pub name: String,
    /// URL（例: `/about`）。未設定の下書きは `None`。
    pub url_path: Option<String>,
    pub content: String,
    pub layout_id: i64,
    pub is_published: bool,
}

/// 公開差し替えで URL を付与する標準ページの `file_name` 一覧。
pub const STANDARD_PUBLISH_FILE_NAMES: &[&str] = &[
    "pages/home.html",
    "pages/index.html",
    "pages/news.html",
    "pages/about.html",
    "pages/contact.html",
];

/// 標準ページの `file_name` から固定公開 URL を解決する。
pub fn standard_publish_url(file_name: &str) -> Option<&'static str> {
    let stem = file_name.rsplit('/').next()?;
    let stem = stem.strip_suffix(".html")?;
    match stem {
        "home" | "index" => Some("/"),
        "news" => Some("/news"),
        "about" => Some("/about"),
        "contact" => Some("/contact"),
        _ => None,
    }
}

/// 公開差し替えの対象ページかどうか（`STANDARD_PUBLISH_FILE_NAMES` と SQL の IN 句と同期）。
pub fn is_standard_publish_page(file_name: &str) -> bool {
    STANDARD_PUBLISH_FILE_NAMES.contains(&file_name)
}

impl Page {
    /// 公開トップ（`url_path = /`）かどうか。
    pub fn is_home(&self) -> bool {
        self.url_path.as_deref() == Some("/")
    }

    /// MiniJinja テンプレート名（`{layout_key}/{file_name}`）。
    pub fn template_name(&self) -> String {
        format!("{}/{}", self.layout_key, self.file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_standard_publish_page, standard_publish_url, STANDARD_PUBLISH_FILE_NAMES,
    };

    #[test]
    fn standard_publish_url_maps_standard_pages() {
        assert_eq!(standard_publish_url("pages/home.html"), Some("/"));
        assert_eq!(standard_publish_url("pages/index.html"), Some("/"));
        assert_eq!(standard_publish_url("pages/news.html"), Some("/news"));
        assert_eq!(standard_publish_url("pages/about.html"), Some("/about"));
        assert_eq!(standard_publish_url("pages/contact.html"), Some("/contact"));
        assert_eq!(standard_publish_url("pages/page-42.html"), None);
    }

    #[test]
    fn is_standard_publish_page_matches_url_mapping() {
        for file_name in STANDARD_PUBLISH_FILE_NAMES {
            assert!(
                is_standard_publish_page(file_name),
                "{file_name} should be publishable"
            );
            assert!(
                standard_publish_url(file_name).is_some(),
                "{file_name} should have a URL"
            );
        }
        assert!(!is_standard_publish_page("pages/page-42.html"));
    }
}
