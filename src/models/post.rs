/// `posts` テーブルのお知らせ行に対応する。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Post {
    pub id: i64,
    pub placeholder_id: Option<i64>,
    pub post_status: String,
    pub post_name: Option<String>,
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// お知らせ作成・更新時にリポジトリへ渡す入力値。
#[derive(Debug, Clone)]
pub struct PostInput {
    pub placeholder_id: i64,
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub post_status: String,
    pub post_name: String,
}
