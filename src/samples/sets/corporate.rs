//! 「コーポレートサイト」サンプルレイアウトセット。

use sqlx::{Sqlite, Transaction};

use crate::error::AppResult;
use crate::state::AppState;

use super::layout_common::{
    self, LayoutSetSpec, PageSpec, PlaceholderIds, PlaceholderSpec,
};

const SHELL_HTML: &str = include_str!("../../../presets/sample-sets/corporate/shell.html");
const SITE_CSS: &str = include_str!("../../../presets/sample-sets/corporate/static/site.css");
const HOME_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/corporate/pages/home.html");
const NEWS_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/corporate/pages/news.html");
const ABOUT_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/corporate/pages/about.html");
const CONTACT_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/corporate/pages/contact.html");

const PLACEHOLDERS: &[PlaceholderSpec] = &[
    PlaceholderSpec {
        name: "corporate_news",
        type_key: "news",
        config: r#"{"limit": 6}"#,
    },
    PlaceholderSpec {
        name: "corporate_announcements",
        type_key: "news",
        config: r#"{"limit": 10}"#,
    },
    PlaceholderSpec {
        name: "corporate_hero",
        type_key: "image",
        config: r#"{"width": "100%", "height": "280px", "object_fit": "cover", "border_radius": "12px"}"#,
    },
    PlaceholderSpec {
        name: "corporate_main_carousel",
        type_key: "carousel",
        config: r#"{"interval": 4, "width": "100%", "height": "420px"}"#,
    },
    PlaceholderSpec {
        name: "corporate_sidebar",
        type_key: "news",
        config: r#"{"limit": 4}"#,
    },
    PlaceholderSpec {
        name: "corporate_contact",
        type_key: "contact_form",
        config: r#"{"heading":"お問い合わせ"}"#,
    },
];

const POST_SLUGS: &[&str] = &[
    "corporate-new-service-launch",
    "corporate-spring-seminar",
    "corporate-case-study-manufacturing",
    "corporate-recruitment-2026",
    "corporate-office-renewal",
    "corporate-partner-draft",
    "corporate-summer-hours",
    "corporate-support-plan",
    "corporate-community-event",
    "corporate-partner-program",
    "corporate-tip-update-news",
    "corporate-tip-carousel-images",
    "corporate-hero-main",
    "corporate-slide-spring",
    "corporate-slide-case-study",
    "corporate-slide-seminar",
];

const MEDIA_FILES: &[&str] = &[
    "corporate-hero.png",
    "corporate-carousel-1.png",
    "corporate-carousel-2.png",
    "corporate-carousel-3.png",
];

const PAGE_URL_PATHS: &[&str] = &[
    "/corporate",
    "/corporate/news",
    "/corporate/about",
    "/corporate/contact",
];

const PAGES: &[PageSpec] = &[
    PageSpec {
        name: "トップページ",
        url_path: "/corporate",
        file_name: "pages/home.html",
        content: HOME_PAGE_HTML,
    },
    PageSpec {
        name: "お知らせ",
        url_path: "/corporate/news",
        file_name: "pages/news.html",
        content: NEWS_PAGE_HTML,
    },
    PageSpec {
        name: "会社概要",
        url_path: "/corporate/about",
        file_name: "pages/about.html",
        content: ABOUT_PAGE_HTML,
    },
    PageSpec {
        name: "お問い合わせ",
        url_path: "/corporate/contact",
        file_name: "pages/contact.html",
        content: CONTACT_PAGE_HTML,
    },
];

const SPEC: LayoutSetSpec = LayoutSetSpec {
    layout_key: "corporate",
    layout_name: "コーポレートサイト（サンプル）",
    shell_html: SHELL_HTML,
    site_css: SITE_CSS,
    preview_path: "/corporate",
    success_message: "コーポレートサイトのサンプルレイアウトセットをインストールしました。",
    placeholders: PLACEHOLDERS,
    post_slugs: POST_SLUGS,
    media_files: MEDIA_FILES,
    page_url_paths: PAGE_URL_PATHS,
    pages: PAGES,
};

/// コーポレートサイトサンプルをインストールする。
pub async fn install(state: &AppState) -> AppResult<super::super::InstallResult> {
    layout_common::install(state, &SPEC).await
}

pub(super) async fn seed_content(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &PlaceholderIds,
) -> AppResult<()> {
    let news_posts = [
        (
            "corporate-new-service-launch",
            "新サービス「サポートプラン」を開始しました",
            "中小企業向けの運用支援プランを提供開始。初月無料キャンペーン実施中。",
            "2026-04-15T09:00:00Z",
            "publish",
        ),
        (
            "corporate-spring-seminar",
            "5月開催：Web サイト改善セミナーのご案内",
            "更新しやすいサイト運用のポイントを、事例を交えてご紹介します。",
            "2026-04-10T14:30:00Z",
            "publish",
        ),
        (
            "corporate-case-study-manufacturing",
            "導入事例：製造業 A 社の業務効率化プロジェクト",
            "受注管理のデジタル化により、処理時間を約 40% 削減しました。",
            "2026-03-20T11:15:00Z",
            "publish",
        ),
        (
            "corporate-recruitment-2026",
            "2026年度 新卒・キャリア採用を開始",
            "エンジニア、デザイナー、カスタマーサクセスのポジションを募集しています。",
            "2026-03-01T10:00:00Z",
            "publish",
        ),
        (
            "corporate-office-renewal",
            "オフィス移転のお知らせ",
            "2026年7月より新オフィスへ移転予定です。詳細は順次お知らせします。",
            "2026-02-15T09:00:00Z",
            "publish",
        ),
        (
            "corporate-partner-draft",
            "パートナー制度の準備中（下書き）",
            "近日中に詳細を公開予定です。",
            "",
            "draft",
        ),
    ];
    for (slug, title, excerpt, published, status) in news_posts {
        layout_common::insert_post(tx, ids.news, slug, title, excerpt, published, status).await?;
    }

    let announcements = [
        (
            "corporate-summer-hours",
            "夏季休業のお知らせ（8月13日〜8月16日）",
            "休業期間中のお問い合わせは、8月17日以降に順次対応いたします。",
            "2026-05-25T23:00:00Z",
            "publish",
        ),
        (
            "corporate-support-plan",
            "サポートプランの料金表を更新しました",
            "スタンダードプランに月次レポート機能を追加しました。",
            "2026-05-28T10:00:00Z",
            "publish",
        ),
        (
            "corporate-community-event",
            "地域ビジネス交流会に出展します",
            "6月20日（土）に地元のビジネス交流会へ参加予定です。",
            "2026-05-01T00:00:00Z",
            "publish",
        ),
        (
            "corporate-partner-program",
            "パートナープログラムの募集を開始",
            "協業パートナー企業を広く募集しています。",
            "2026-04-28T14:00:00Z",
            "publish",
        ),
    ];
    for (slug, title, excerpt, published, status) in announcements {
        layout_common::insert_post(
            tx, ids.announcements, slug, title, excerpt, published, status,
        )
        .await?;
    }

    let sidebar = [
        (
            "corporate-tip-update-news",
            "お知らせは定期的に更新しましょう",
            "月1回の更新でも、サイトの信頼感は大きく変わります。",
            "2026-05-18T08:00:00Z",
            "publish",
        ),
        (
            "corporate-tip-carousel-images",
            "カルーセル画像は統一感を意識",
            "同じ比率・トーンの画像を使うと、見た目が整います。",
            "2026-05-22T16:45:00Z",
            "publish",
        ),
    ];
    for (slug, title, excerpt, published, status) in sidebar {
        layout_common::insert_post(tx, ids.sidebar, slug, title, excerpt, published, status)
            .await?;
    }

    let media_ids = [
        layout_common::insert_media_attachment(
            tx, "corporate-hero.png", "image/png", "corporate-hero.png", 12345,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "corporate-carousel-1.png",
            "image/png",
            "corporate-slide-1.png",
            8901,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "corporate-carousel-2.png",
            "image/png",
            "corporate-slide-2.png",
            9012,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "corporate-carousel-3.png",
            "image/png",
            "corporate-slide-3.png",
            9123,
        )
        .await?,
    ];

    let hero_entry = layout_common::insert_post(
        tx,
        ids.hero,
        "corporate-hero-main",
        "メインキービジュアル",
        "サイトを象徴するビジュアルです",
        "2026-05-01T00:00:00Z",
        "publish",
    )
    .await?;
    layout_common::insert_postmeta(tx, hero_entry, "media_id", &media_ids[0].to_string()).await?;
    layout_common::insert_postmeta(tx, hero_entry, "float", "none").await?;
    layout_common::insert_postmeta(tx, hero_entry, "margin", "0").await?;

    let slides = [
        ("corporate-slide-spring", "春の新生活キャンペーン"),
        ("corporate-slide-case-study", "導入事例を公開しました"),
        ("corporate-slide-seminar", "6月 セミナー開催のご案内"),
    ];
    for (i, (slug, title)) in slides.iter().enumerate() {
        let entry_id = layout_common::insert_post(
            tx,
            ids.carousel,
            slug,
            title,
            "",
            "2026-05-10T00:00:00Z",
            "publish",
        )
        .await?;
        layout_common::insert_postmeta(tx, entry_id, "media_id", &media_ids[i + 1].to_string())
            .await?;
        layout_common::insert_postmeta(tx, entry_id, "alt", title).await?;
    }

    Ok(())
}