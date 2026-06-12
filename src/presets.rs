//! 同梱のスターターデザイン。新規テンプレート作成時に複製元として選択する。

/// 例示レイアウトの shell（`work/layouts/example/shell.html`）。
pub const DEFAULT_SHELL: &str = include_str!("../presets/shell.html");

/// 公開トップ `/` の初期ページ本文（`pages/index.html`）。
pub const DEFAULT_HOME_PAGE: &str = include_str!("../presets/home_page.html");

/// 例示レイアウトの site.css。
pub const DEFAULT_SITE_CSS: &str = include_str!("../presets/example/site.css");

/// 1 つのプリセットデザイン（ページ本文雛形）。
pub struct Preset {
    /// URL や選択に使う一意キー。
    pub key: &'static str,
    /// 管理画面に表示するラベル。
    pub label: &'static str,
    /// 説明文。
    pub description: &'static str,
    /// 複製元となる MiniJinja 本文（`extends example/shell.html` 付き）。
    pub html: &'static str,
}

/// 利用可能なプリセット一覧。
pub const PRESETS: &[Preset] = &[
    Preset {
        key: "landing",
        label: "ランディング",
        description: "ヒーローと特徴を並べた、トップページ向けの華やかなデザイン。",
        html: include_str!("../presets/pages/landing.html"),
    },
    Preset {
        key: "simple-page",
        label: "シンプルページ",
        description: "見出しと本文だけの、固定ページ向けの落ち着いたデザイン。",
        html: include_str!("../presets/pages/simple-page.html"),
    },
    Preset {
        key: "news",
        label: "お知らせ一覧",
        description: "公開済みのお知らせを一覧表示する動的デザイン。",
        html: include_str!("../presets/pages/news.html"),
    },
];

/// キーからプリセットを取得する。
pub fn get(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.key == key)
}
