//! 管理画面 Askama テンプレート向けの共通レイアウト値。

use super::auth::AuthUser;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// base.html が参照する共通レイアウト値。各 Template struct に 1 フィールドだけ持たせる。
pub struct AdminLayoutCtx {
    pub user_display_name: String,
    pub app_version: &'static str,
    pub embed: bool,
    /// embed 時 `?embed=1`、それ以外は空文字
    pub embed_query: String,
    /// embed 時 `&embed=1`、それ以外は空文字
    pub embed_amp: String,
}

impl AdminLayoutCtx {
    pub fn new(auth: &AuthUser) -> Self {
        Self::with_embed(auth, false)
    }

    pub fn with_embed(auth: &AuthUser, embed: bool) -> Self {
        Self {
            user_display_name: auth.display_name.clone(),
            app_version: APP_VERSION,
            embed,
            embed_query: if embed { "?embed=1".to_string() } else { String::new() },
            embed_amp: if embed { "&embed=1".to_string() } else { String::new() },
        }
    }
}
