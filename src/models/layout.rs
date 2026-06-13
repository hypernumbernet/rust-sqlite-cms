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
    /// 指定した key で新規作成する（manifest の key は使わない）。
    Rename,
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
    pub created_at: String,
    pub updated_at: String,
}

/// レイアウト作成・更新用。
#[derive(Debug, Clone)]
pub struct LayoutInput {
    pub key: String,
    pub name: String,
}

/// 管理画面レイアウト一覧用の集計行。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LayoutAdminSummary {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub updated_at: String,
    pub page_count: i64,
    pub published_count: i64,
    pub publishable_count: i64,
}

impl LayoutAdminSummary {
    pub fn is_live(&self) -> bool {
        self.published_count > 0
    }

    pub fn can_publish(&self) -> bool {
        !self.is_live() && self.publishable_count > 0
    }

    pub fn status_label(&self) -> &'static str {
        if self.is_live() {
            "公開中"
        } else {
            "下書き"
        }
    }
}