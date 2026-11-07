//! `Accept-Language` とクエリ・Cookie による言語解決。

use axum::http::{HeaderMap, HeaderValue, Uri};

use super::{Locale, LANG_COOKIE_NAME};

/// `?lang=ja|en` の明示指定（レスポンスで Cookie を書く）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangOverride {
    Ja,
    En,
}

/// ヘッダー・Cookie・クエリから表示言語を決定する。
pub fn resolve(headers: &HeaderMap, uri: &Uri) -> (Locale, Option<LangOverride>) {
    if let Some(ov) = lang_query_override(uri.query()) {
        return (ov.into(), Some(ov));
    }

    if let Some(locale) = locale_from_cookie(headers) {
        return (locale, None);
    }

    let accept = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok());
    (negotiate_accept_language(accept), None)
}

fn lang_query_override(query: Option<&str>) -> Option<LangOverride> {
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key != "lang" {
            continue;
        }
        let value = value.trim().to_ascii_lowercase();
        return match value.as_str() {
            "ja" => Some(LangOverride::Ja),
            "en" => Some(LangOverride::En),
            _ => None,
        };
    }
    None
}

fn locale_from_cookie(headers: &HeaderMap) -> Option<Locale> {
    let cookie_header = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        let (name, value) = part.split_once('=')?;
        if name.trim() != LANG_COOKIE_NAME {
            continue;
        }
        return match value.trim().to_ascii_lowercase().as_str() {
            "ja" => Some(Locale::Ja),
            "en" => Some(Locale::En),
            _ => None,
        };
    }
    None
}

/// RFC 7231 風に `Accept-Language` を解釈する（`q` 値対応）。
pub fn negotiate_accept_language(header: Option<&str>) -> Locale {
    let Some(header) = header else {
        return Locale::En;
    };

    let mut entries: Vec<(f32, usize, &str)> = Vec::new();
    for (idx, part) in header.split(',').enumerate() {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (tag, q) = parse_language_range(part);
        entries.push((q, idx, tag));
    }

    if entries.is_empty() {
        return Locale::En;
    }

    entries.sort_by(|a, b| {
        b.0
            .partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });

    if let Some((_, _, tag)) = entries.first() {
        if is_japanese_tag(tag) {
            return Locale::Ja;
        }
    }
    Locale::En
}

fn parse_language_range(part: &str) -> (&str, f32) {
    if let Some((tag, params)) = part.split_once(';') {
        let tag = tag.trim();
        let q = params
            .split(';')
            .find_map(|p| {
                let p = p.trim();
                p.strip_prefix("q=")
                    .or_else(|| p.strip_prefix("Q="))
                    .and_then(|v| v.trim().parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        (tag, q.clamp(0.0, 1.0))
    } else {
        (part.trim(), 1.0)
    }
}

fn is_japanese_tag(tag: &str) -> bool {
    let primary = tag.split('-').next().unwrap_or(tag).to_ascii_lowercase();
    primary == "ja"
}

/// 言語 Cookie の `Set-Cookie` 値を組み立てる。
pub fn lang_cookie_value(locale: Locale) -> HeaderValue {
    let value = format!(
        "{}={}; Path=/; Max-Age=31536000; SameSite=Lax",
        LANG_COOKIE_NAME,
        locale.cookie_value()
    );
    HeaderValue::from_str(&value).expect("static cookie value")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_language_prefers_higher_q() {
        assert_eq!(
            negotiate_accept_language(Some("en;q=0.9, ja;q=0.95")),
            Locale::Ja
        );
    }

    #[test]
    fn accept_language_ja_wildcard() {
        assert_eq!(
            negotiate_accept_language(Some("en-US, en;q=0.9, ja-JP;q=0.8")),
            Locale::En
        );
        assert_eq!(
            negotiate_accept_language(Some("ja-JP, ja;q=0.9, en;q=0.8")),
            Locale::Ja
        );
    }

    #[test]
    fn accept_language_defaults_english() {
        assert_eq!(negotiate_accept_language(None), Locale::En);
        assert_eq!(negotiate_accept_language(Some("")), Locale::En);
        assert_eq!(
            negotiate_accept_language(Some("de-DE, fr;q=0.5")),
            Locale::En
        );
    }

    #[test]
    fn query_lang_overrides_accept_language() {
        let uri: Uri = "/admin?lang=ja".parse().unwrap();
        let headers = HeaderMap::new();
        let (loc, ov) = resolve(&headers, &uri);
        assert_eq!(loc, Locale::Ja);
        assert_eq!(ov, Some(LangOverride::Ja));
    }

    #[test]
    fn cookie_overrides_accept_language_without_query() {
        let uri: Uri = "/".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("cms_lang=en"),
        );
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("ja"),
        );
        let (loc, ov) = resolve(&headers, &uri);
        assert_eq!(loc, Locale::En);
        assert_eq!(ov, None);
    }
}
