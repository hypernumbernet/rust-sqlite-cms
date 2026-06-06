//! HTTP セッション（tower-sessions）の組み立て。

use tower_sessions::{
    cookie::Key, service::SignedCookie, Expiry, MemoryStore, SessionManagerLayer,
};

use crate::config::AppConfig;

/// 開発用フォールバック（`session_secret` 未設定時）。本番では必ず固定秘密鍵を設定すること。
const DEV_SESSION_SECRET: &str = "rust-sqlite-cms-dev-insecure-session-secret";

fn session_key_from_secret(secret: &str) -> Key {
    let mut buf = [0u8; 64];
    for (i, b) in secret.as_bytes().iter().cycle().take(64).enumerate() {
        buf[i] = *b;
    }
    Key::from(&buf)
}

/// 設定からセッション署名用の秘密鍵文字列を解決する。
///
/// セッション Cookie とお問い合わせフォームトークンの双方で同じ値を使う。
pub fn resolve_session_secret(config: &AppConfig) -> String {
    if let Some(secret) = config.security.session_secret.as_deref() {
        let secret = secret.trim();
        if !secret.is_empty() {
            return secret.to_string();
        }
    }

    tracing::warn!(
        "CMS_SESSION_SECRET / security.session_secret が未設定です。\
         開発用の固定秘密鍵を使用します（本番環境では必ず固定の秘密鍵を設定してください）。"
    );
    DEV_SESSION_SECRET.to_string()
}

/// 設定からセッション署名キーを解決する。
pub fn resolve_session_key(config: &AppConfig) -> Key {
    session_key_from_secret(&resolve_session_secret(config))
}

/// アプリ全体に適用するセッション Layer を構築する。
pub fn session_layer(config: &AppConfig) -> SessionManagerLayer<MemoryStore, SignedCookie> {
    let key = resolve_session_key(config);
    SessionManagerLayer::new(MemoryStore::default())
        .with_name(config.session.cookie_name.clone())
        .with_expiry(Expiry::OnInactivity(time::Duration::seconds(
            config.session.max_age_secs,
        )))
        .with_secure(false)
        .with_signed(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_session_secret_uses_config_when_set() {
        let mut config = AppConfig::default();
        config.security.session_secret = Some("my-production-secret".to_string());
        assert_eq!(resolve_session_secret(&config), "my-production-secret");
    }

    #[test]
    fn resolve_session_secret_falls_back_for_dev() {
        let config = AppConfig::default();
        assert_eq!(resolve_session_secret(&config), DEV_SESSION_SECRET);
    }
}
