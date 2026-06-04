/// `layouts` テーブルの行。shell と static は `work/layouts/{key}/` に保持する。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Layout {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// レイアウト作成・更新用。
#[derive(Debug, Clone)]
pub struct LayoutInput {
    pub key: String,
    pub name: String,
    pub is_default: bool,
}
