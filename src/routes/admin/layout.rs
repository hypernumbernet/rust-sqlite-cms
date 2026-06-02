//! 管理画面 Askama テンプレート向けの共通レイアウト値。

use super::auth::AuthUser;

pub fn user_display_name(auth: &AuthUser) -> String {
    auth.display_name.clone()
}
