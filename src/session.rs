//! HTTP セッション（tower-sessions）の組み立て。

use tower_sessions::{
    cookie::Key, service::SignedCookie, Expiry, MemoryStore, SessionManagerLayer,
};

use crate::config::AppConfig;

fn session_key_from_secret(secret: &str) -> Key {
    let mut buf = [0u8; 64];
    for (i, b) in secret.as_bytes().iter().cycle().take(64).enumerate() {
        buf[i] = *b;
    }
    Key::from(&buf)
}

/// 設定からセッション署名キーを解決する。
pub fn resolve_session_key(config: &AppConfig) -> Key {
    if let Some(secret) = config.security.session_secret.as_deref() {
        session_key_from_secret(secret)
    } else {
        tracing::warn!(
            "CMS_SESSION_SECRET / security.session_secret が未設定です。\
             起動ごとにランダムなセッション鍵を生成します（再起動で全セッションが無効になります）。\
             本番環境では必ず固定の秘密鍵を設定してください。"
        );
        Key::generate()
    }
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
