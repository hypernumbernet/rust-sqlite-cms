//! ブラウザ言語と Cookie に基づく UI 文言（ユーザー投稿コンテンツ以外）。

mod middleware;
mod negotiate;

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use tokio::task_local;

pub use middleware::set_request_locale;
pub use negotiate::{negotiate_accept_language, resolve};

pub const LANG_COOKIE_NAME: &str = "cms_lang";

/// 表示言語。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Ja,
    En,
}

impl Locale {
    pub fn html_lang(self) -> &'static str {
        match self {
            Locale::Ja => "ja",
            Locale::En => "en",
        }
    }

    pub fn cookie_value(self) -> &'static str {
        match self {
            Locale::Ja => "ja",
            Locale::En => "en",
        }
    }
}

impl From<negotiate::LangOverride> for Locale {
    fn from(value: negotiate::LangOverride) -> Self {
        match value {
            negotiate::LangOverride::Ja => Locale::Ja,
            negotiate::LangOverride::En => Locale::En,
        }
    }
}

task_local! {
    pub(crate) static CURRENT_LOCALE: Locale;
}

/// 現在リクエストの表示言語（未設定時は英語）。
pub fn current_locale() -> Locale {
    CURRENT_LOCALE.try_with(|l| *l).unwrap_or(Locale::En)
}

#[derive(Debug, Deserialize)]
struct I18nEntry {
    ja: String,
    en: String,
}

static STRINGS: LazyLock<HashMap<String, I18nEntry>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("strings.json")).expect("valid strings.json")
});

/// キーに対応する UI 文言を返す。未知キーはキー文字列をそのまま返す。
pub fn translate(locale: Locale, key: &str) -> &str {
    let Some(entry) = STRINGS.get(key) else {
        return key;
    };
    match locale {
        Locale::Ja => &entry.ja,
        Locale::En => &entry.en,
    }
}

/// `translate` のショートカット（現在ロケール）。
pub fn t(key: &str) -> &str {
    translate(current_locale(), key)
}

/// 公開サイトの問い合わせフォーム等に渡すビルトイン UI 文言。
pub fn public_contact_ui(locale: Locale) -> serde_json::Value {
    serde_json::json!({
        "name_label": translate(locale, "public.contact.name_label"),
        "email_label": translate(locale, "public.contact.email_label"),
        "phone_label": translate(locale, "public.contact.phone_label"),
        "message_label": translate(locale, "public.contact.message_label"),
        "submit_failed": translate(locale, "public.contact.submit_failed"),
        "init_failed": translate(locale, "public.contact.init_failed"),
        "submitting": translate(locale, "public.contact.submitting"),
        "required_marker": translate(locale, "public.contact.required_marker"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_returns_english_and_japanese() {
        assert_eq!(translate(Locale::En, "nav.dashboard"), "Dashboard");
        assert_eq!(translate(Locale::Ja, "nav.dashboard"), "ダッシュボード");
    }

    #[test]
    fn translate_unknown_key_returns_key() {
        assert_eq!(translate(Locale::En, "missing.key"), "missing.key");
    }
}
