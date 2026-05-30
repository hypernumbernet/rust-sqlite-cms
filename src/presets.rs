//! 同梱のスターターデザイン。新規テンプレート作成時に複製元として選択する。

/// 1 つのプリセットデザイン。
pub struct Preset {
    /// URL や選択に使う一意キー。
    pub key: &'static str,
    /// 管理画面に表示するラベル。
    pub label: &'static str,
    /// 説明文。
    pub description: &'static str,
    /// 複製元となる HTML（MiniJinja ソース）。
    pub html: &'static str,
}

/// 利用可能なプリセット一覧。
pub const PRESETS: &[Preset] = &[
    Preset {
        key: "landing",
        label: "ランディング",
        description: "ヒーローと特徴を並べた、トップページ向けの華やかなデザイン。",
        html: include_str!("../presets/landing.html"),
    },
    Preset {
        key: "simple-page",
        label: "シンプルページ",
        description: "見出しと本文だけの、固定ページ向けの落ち着いたデザイン。",
        html: include_str!("../presets/simple-page.html"),
    },
    Preset {
        key: "news",
        label: "お知らせ一覧",
        description: "公開済みのお知らせを一覧表示する動的デザイン。",
        html: include_str!("../presets/news.html"),
    },
];

/// キーからプリセットを取得する。
pub fn get(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.key == key)
}
