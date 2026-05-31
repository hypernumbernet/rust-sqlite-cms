/// `posts`（`post_type = 'attachment'`）+ `postmeta` の結合行。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Media {
    pub id: i64,
    pub title: String,
    pub file_path: Option<String>,
    pub mime_type: Option<String>,
    pub original_name: Option<String>,
    pub file_size: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// メディア作成時にリポジトリへ渡す入力値。
#[derive(Debug, Clone)]
pub struct MediaInput {
    pub title: String,
    pub file_path: String,
    pub mime_type: String,
    pub original_name: String,
    pub file_size: i64,
}

impl Media {
    pub fn public_url(&self) -> String {
        let path = self.file_path.as_deref().unwrap_or("");
        format!("/uploads/{path}")
    }

    pub fn is_image(&self) -> bool {
        self.mime_type
            .as_deref()
            .map(|m| m.starts_with("image/"))
            .unwrap_or(false)
    }

    pub fn file_size_bytes(&self) -> i64 {
        self.file_size
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}
