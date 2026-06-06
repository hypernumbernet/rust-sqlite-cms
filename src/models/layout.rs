use serde::{Deserialize, Serialize};

/// レイアウト ZIP エクスポート用 manifest（format_version 1）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutExportManifest {
    pub format_version: u32,
    pub layout: LayoutExportMeta,
    pub pages: Vec<LayoutExportPageMeta>,
}

/// manifest 内のレイアウトメタ情報。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutExportMeta {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    pub favicon_media_id: Option<i64>,
}

/// manifest 内のページメタ情報（本文は ZIP 内ファイル）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayoutExportPageMeta {
    pub name: String,
    pub url_path: Option<String>,
    pub file_name: String,
    pub is_published: bool,
}

/// インポート時の衝突解決モード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutImportMode {
    /// 既存レイアウトがあれば上書き、なければ新規作成。
    Overwrite,
    /// 既存レイアウトがあれば変更しない。
    Skip,
}

/// インポート結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutImportAction {
    Created,
    Updated,
    Skipped,
}

/// `layouts` テーブルの行。shell と static は `work/layouts/{key}/` に保持する。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Layout {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub is_default: bool,
    /// メディア（attachment）の ID。未設定は `None`。
    pub favicon_media_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// レイアウト作成・更新用。
#[derive(Debug, Clone)]
pub struct LayoutInput {
    pub key: String,
    pub name: String,
    pub is_default: bool,
    pub favicon_media_id: Option<i64>,
}
