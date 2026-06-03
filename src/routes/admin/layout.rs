//! 管理画面 Askama テンプレート向けの共通レイアウト値。

use super::auth::AuthUser;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// base.html が参照する共通レイアウト値。各 Template struct に 1 フィールドだけ持たせる。
pub struct AdminLayoutCtx {
    pub user_display_name: String,
    pub app_version: &'static str,
}

impl AdminLayoutCtx {
    pub fn new(auth: &AuthUser) -> Self {
        Self {
            user_display_name: auth.display_name.clone(),
            app_version: APP_VERSION,
        }
    }
}
