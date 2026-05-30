/// `pages` テーブルの行に対応する。本文は `file_name` が指す
/// `work/pages/` 配下のファイルに保持し、DB にはメタ情報のみを持つ。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Page {
    pub id: i64,
    pub name: String,
    pub url_path: Option<String>,
    /// 本文 HTML を保持するファイル名（例: `page-3.html`）。
    pub file_name: Option<String>,
    pub is_published: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 固定ページ作成・更新時にリポジトリ／ストアへ渡す入力値。
/// `content` は DB ではなくファイルへ書き込む。
#[derive(Debug, Clone)]
pub struct PageInput {
    pub name: String,
    /// URL（例: `/about`）。未設定の下書きは `None`。
    pub url_path: Option<String>,
    pub content: String,
    pub is_published: bool,
}
