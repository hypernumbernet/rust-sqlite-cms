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
