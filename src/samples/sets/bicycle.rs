//! 「街の自転車屋さん」サンプルレイアウトセット。

use sqlx::{Sqlite, Transaction};

use crate::error::AppResult;
use crate::state::AppState;

use super::layout_common::{
    self, LayoutSetSpec, PageSpec, PlaceholderIds, PlaceholderSpec,
};

const SHELL_HTML: &str = include_str!("../../../presets/sample-sets/bicycle/shell.html");
const SITE_CSS: &str = include_str!("../../../presets/sample-sets/bicycle/static/site.css");
const HOME_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/bicycle/pages/home.html");
const NEWS_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/bicycle/pages/news.html");
const ABOUT_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/bicycle/pages/about.html");
const CONTACT_PAGE_HTML: &str = include_str!("../../../presets/sample-sets/bicycle/pages/contact.html");

const PLACEHOLDERS: &[PlaceholderSpec] = &[
    PlaceholderSpec {
        name: "bicycle_news",
        type_key: "news",
        config: r#"{"limit": 6}"#,
    },
    PlaceholderSpec {
        name: "bicycle_announcements",
        type_key: "news",
        config: r#"{"limit": 10}"#,
    },
    PlaceholderSpec {
        name: "bicycle_hero",
        type_key: "image",
        config: r#"{"width": "100%", "height": "280px", "object_fit": "cover", "border_radius": "12px"}"#,
    },
    PlaceholderSpec {
        name: "bicycle_main_carousel",
        type_key: "carousel",
        config: r#"{"interval": 4, "width": "100%", "height": "420px"}"#,
    },
    PlaceholderSpec {
        name: "bicycle_sidebar",
        type_key: "news",
        config: r#"{"limit": 4}"#,
    },
    PlaceholderSpec {
        name: "bicycle_contact",
        type_key: "contact_form",
        config: r#"{"heading":"お問い合わせ・ご予約"}"#,
    },
];

const POST_SLUGS: &[&str] = &[
    "bicycle-spring-new-arrival",
    "bicycle-safety-check-campaign",
    "bicycle-sunday-group-ride",
    "bicycle-ebike-test-ride",
    "bicycle-rental-plan-update",
    "bicycle-winter-hours-draft",
    "bicycle-rainy-day-tips",
    "bicycle-pump-station-free",
    "bicycle-kids-bike-fair",
    "bicycle-staff-wanted",
    "bicycle-tip-tire-pressure",
    "bicycle-tip-chain-care",
    "bicycle-hero-main",
    "bicycle-slide-spring-sale",
    "bicycle-slide-repair",
    "bicycle-slide-ride-event",
];

const MEDIA_FILES: &[&str] = &[
    "bicycle-hero.png",
    "bicycle-carousel-1.png",
    "bicycle-carousel-2.png",
    "bicycle-carousel-3.png",
];

const PAGE_URL_PATHS: &[&str] = &[
    "/bicycle",
    "/bicycle/news",
    "/bicycle/about",
    "/bicycle/contact",
];

const PAGES: &[PageSpec] = &[
    PageSpec {
        name: "トップページ",
        url_path: "/bicycle",
        file_name: "pages/home.html",
        content: HOME_PAGE_HTML,
    },
    PageSpec {
        name: "お知らせ",
        url_path: "/bicycle/news",
        file_name: "pages/news.html",
        content: NEWS_PAGE_HTML,
    },
    PageSpec {
        name: "店舗紹介",
        url_path: "/bicycle/about",
        file_name: "pages/about.html",
        content: ABOUT_PAGE_HTML,
    },
    PageSpec {
        name: "お問い合わせ",
        url_path: "/bicycle/contact",
        file_name: "pages/contact.html",
        content: CONTACT_PAGE_HTML,
    },
];

const SPEC: LayoutSetSpec = LayoutSetSpec {
    layout_key: "bicycle",
    layout_name: "街の自転車屋さん（サンプル）",
    shell_html: SHELL_HTML,
    site_css: SITE_CSS,
    preview_path: "/bicycle",
    success_message: "「街の自転車屋さん」のサンプルレイアウトセットをインストールしました。",
    placeholders: PLACEHOLDERS,
    post_slugs: POST_SLUGS,
    media_files: MEDIA_FILES,
    page_url_paths: PAGE_URL_PATHS,
    pages: PAGES,
};

/// 街の自転車屋さんサンプルをインストールする。
pub async fn install(state: &AppState) -> AppResult<super::super::InstallResult> {
    layout_common::install(state, &SPEC).await
}

pub(super) async fn seed_content(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &PlaceholderIds,
) -> AppResult<()> {
    let news_posts = [
        (
            "bicycle-spring-new-arrival",
            "春の新車入荷のお知らせ",
            "街乗り向けクロスバイクと電動アシストモデルを追加しました。",
            "2026-04-15T09:00:00Z",
            "publish",
        ),
        (
            "bicycle-safety-check-campaign",
            "安全点検無料キャンペーン（5月）",
            "ブレーキ・タイヤ・ライトの点検を無料で承ります。要予約。",
            "2026-04-10T14:30:00Z",
            "publish",
        ),
        (
            "bicycle-sunday-group-ride",
            "日曜グループライドを再開します",
            "初心者歓迎の河川敷コース。集合は店舗前 8:30 です。",
            "2026-03-20T11:15:00Z",
            "publish",
        ),
        (
            "bicycle-ebike-test-ride",
            "電動アシスト試乗会を開催",
            "最新モデルを実際に走って体感できます。予約優先。",
            "2026-03-01T10:00:00Z",
            "publish",
        ),
        (
            "bicycle-rental-plan-update",
            "レンタルプランをリニューアル",
            "1時間プランと半日プランを追加しました。",
            "2026-02-15T09:00:00Z",
            "publish",
        ),
        (
            "bicycle-winter-hours-draft",
            "冬季営業時間の案内（下書き）",
            "12月からの営業時間変更を準備中です。",
            "",
            "draft",
        ),
    ];
    for (slug, title, excerpt, published, status) in news_posts {
        layout_common::insert_post(tx, ids.news, slug, title, excerpt, published, status).await?;
    }

    let announcements = [
        (
            "bicycle-rainy-day-tips",
            "雨の日のライドに関するお願い",
            "視界確保のためライトの点灯をお願いしています。",
            "2026-05-25T23:00:00Z",
            "publish",
        ),
        (
            "bicycle-pump-station-free",
            "空気入れステーションを無料開放中",
            "店舗前のスタンドをいつでもご利用いただけます。",
            "2026-05-28T10:00:00Z",
            "publish",
        ),
        (
            "bicycle-kids-bike-fair",
            "キッズバイクフェア開催",
            "6月第1土曜日。子ども用ヘルメットの相談も承ります。",
            "2026-05-01T00:00:00Z",
            "publish",
        ),
        (
            "bicycle-staff-wanted",
            "スタッフ募集のお知らせ",
            "自転車が好きな方、整備経験者を歓迎します。",
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
            "bicycle-tip-tire-pressure",
            "タイヤの空気圧を月1回チェック",
            "適正な空気圧で走行が楽になり、パンクも防げます。",
            "2026-05-18T08:00:00Z",
            "publish",
        ),
        (
            "bicycle-tip-chain-care",
            "チェーンの清掃と注油",
            "走行後の砂ぼこりを落とすと、変速がスムーズになります。",
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
            tx, "bicycle-hero.png", "image/png", "bicycle-hero.png", 12345,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "bicycle-carousel-1.png",
            "image/png",
            "bicycle-slide-1.png",
            8901,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "bicycle-carousel-2.png",
            "image/png",
            "bicycle-slide-2.png",
            9012,
        )
        .await?,
        layout_common::insert_media_attachment(
            tx,
            "bicycle-carousel-3.png",
            "image/png",
            "bicycle-slide-3.png",
            9123,
        )
        .await?,
    ];

    let hero_entry = layout_common::insert_post(
        tx,
        ids.hero,
        "bicycle-hero-main",
        "店頭の自転車ディスプレイ",
        "春の新車ラインナップをご覧ください",
        "2026-05-01T00:00:00Z",
        "publish",
    )
    .await?;
    layout_common::insert_postmeta(tx, hero_entry, "media_id", &media_ids[0].to_string()).await?;
    layout_common::insert_postmeta(tx, hero_entry, "float", "none").await?;
    layout_common::insert_postmeta(tx, hero_entry, "margin", "0").await?;

    let slides = [
        ("bicycle-slide-spring-sale", "春の新車セール開催中"),
        ("bicycle-slide-repair", "修理・点検は事前予約がおすすめ"),
        ("bicycle-slide-ride-event", "6月 グループライド参加者募集"),
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