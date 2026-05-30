/// `templates` テーブルのユーザー編集可能な HTML テンプレート行に対応する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub url_path: Option<String>,
    pub content: String,
    pub is_published: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// テンプレート作成・更新時にリポジトリへ渡す入力値。
#[derive(Debug, Clone)]
pub struct TemplateInput {
    pub name: String,
    /// URL（例: `/about`）。未設定の下書きは `None`。
    pub url_path: Option<String>,
    pub content: String,
    pub is_published: bool,
}
