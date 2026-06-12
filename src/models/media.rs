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
    /// 公開 URL（postmeta `public_url`）。未設定時は `/uploads/{file_path}` を導出する。
    pub public_url: Option<String>,
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
    /// 公開 URL。postmeta が無ければ `/uploads/{file_path}` を返す。
    pub fn resolved_public_url(&self) -> String {
        if let Some(url) = self.public_url.as_deref().filter(|u| !u.is_empty()) {
            return url.to_string();
        }
        let path = self.file_path.as_deref().unwrap_or("");
        format!("/uploads/{path}")
    }

    pub fn is_image(&self) -> bool {
        self.mime_type
            .as_deref()
            .map(|m| m.starts_with("image/"))
            .unwrap_or(false)
    }

    /// favicon（`/favicon.ico`）として設定可能か（画像または `.ico`）。
    pub fn is_favicon_suitable(&self) -> bool {
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

    /// カスタム alias（`/uploads/` 以外の public_url）かどうか。
    pub fn has_custom_public_url(&self) -> bool {
        let url = self.resolved_public_url();
        !url.starts_with("/uploads/")
    }
}