//! サンプルレイアウトセット・DBテーブルセットの提供・インストール。

pub mod conflict;
pub mod sets;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// 管理画面に表示するレイアウトセットのメタ情報。
#[derive(Debug, Clone, Copy)]
pub struct SampleLayoutSetMeta {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub layout_key: &'static str,
    pub tags: &'static [&'static str],
}

/// 管理画面に表示する DBテーブルセットのメタ情報。
#[derive(Debug, Clone, Copy)]
pub struct SampleTableSetMeta {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub table_names: &'static [&'static str],
}

/// 利用可能なサンプルレイアウトセット一覧。
pub const SAMPLE_LAYOUT_SETS: &[SampleLayoutSetMeta] = &[
    SampleLayoutSetMeta {
        key: "corporate",
        label: "コーポレートサイト",
        description: "企業向けのトップページ・お知らせ・会社概要・お問い合わせを含むサイト構成です。",
        layout_key: "corporate",
        tags: &["レイアウト", "4ページ", "6ウィジェット", "投稿・画像"],
    },
    SampleLayoutSetMeta {
        key: "bicycle",
        label: "街の自転車屋さん",
        description: "地域の自転車屋向け。販売・修理・レンタルの紹介とお知らせ、店舗情報ページ付きです。",
        layout_key: "bicycle",
        tags: &["レイアウト", "4ページ", "6ウィジェット", "投稿・画像"],
    },
];

/// 利用可能な DBテーブルセット一覧。
pub const SAMPLE_TABLE_SETS: &[SampleTableSetMeta] = &[
    SampleTableSetMeta {
        key: "corporate-tables",
        label: "業務データ（コーポレート向け）",
        description: "サービス一覧・チーム紹介のサンプルテーブルとビューを追加します。",
        tags: &["2テーブル", "1ビュー", "サンプル行"],
        table_names: &["corporate_services", "corporate_team"],
    },
    SampleTableSetMeta {
        key: "distinct-tables",
        label: "DISTINCT デモ（部署・オフィス）",
        description:
            "部署とオフィスに重複を含む社員テーブルと、DISTINCT で重複を除いた一覧ビューを追加します。",
        tags: &["1テーブル", "3ビュー", "DISTINCT", "サンプル14行"],
        table_names: &["distinct_employees"],
    },
];

/// インストール結果（UI 表示用）。
#[derive(Debug, Clone)]
pub enum InstallResult {
    Layout {
        message: String,
        layout_key: String,
        placeholders_count: i64,
        posts_count: i64,
        media_count: i64,
        pages_count: i64,
    },
    Tables {
        message: String,
        tables_count: i64,
        views_count: i64,
        rows_count: i64,
    },
}

/// 指定したサンプルセットをインストールする。
pub async fn install_sample_set(state: &AppState, key: &str) -> AppResult<InstallResult> {
    match key {
        "corporate" => sets::corporate::install(state).await,
        "bicycle" => sets::bicycle::install(state).await,
        "corporate-tables" => sets::corporate_tables::install(state).await,
        "distinct-tables" => sets::distinct_tables::install(state).await,
        _ => Err(AppError::Conflict(format!(
            "不明なサンプルセットです: {key}"
        ))),
    }
}