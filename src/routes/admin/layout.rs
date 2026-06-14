//! 管理画面 Askama テンプレート向けの共通レイアウト値。

use super::auth::AuthUser;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// パンくずリストの 1 項目。
pub struct BreadcrumbItem {
    pub label: String,
    /// 空文字のとき現在ページ（リンクなし）。
    pub href: String,
}

/// base.html が参照する共通レイアウト値。各 Template struct に 1 フィールドだけ持たせる。
pub struct AdminLayoutCtx {
    pub user_display_name: String,
    pub app_version: &'static str,
    pub embed: bool,
    /// embed 時 `?embed=1`、それ以外は空文字
    pub embed_query: String,
    /// embed 時 `&embed=1`、それ以外は空文字
    pub embed_amp: String,
    pub breadcrumbs: Vec<BreadcrumbItem>,
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
            breadcrumbs: Vec::new(),
        }
    }

    pub fn with_breadcrumbs(mut self, items: Vec<BreadcrumbItem>) -> Self {
        self.breadcrumbs = items
            .into_iter()
            .map(|mut item| {
                if !item.href.is_empty() {
                    item.href = self.url_with_embed(&item.href);
                }
                item
            })
            .collect();
        self
    }

    pub fn url_with_embed(&self, path: &str) -> String {
        if !self.embed || path.contains("embed=1") {
            return path.to_string();
        }
        if path.contains('?') {
            format!("{path}&embed=1")
        } else {
            format!("{path}?embed=1")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::admin::auth::AuthUser;

    fn embed_ctx() -> AdminLayoutCtx {
        AdminLayoutCtx::with_embed(
            &AuthUser {
                id: 1,
                login: "admin".to_string(),
                display_name: "Admin".to_string(),
            },
            true,
        )
    }

    #[test]
    fn url_with_embed_appends_query_once() {
        let ctx = embed_ctx();
        assert_eq!(
            ctx.url_with_embed("/admin/posts"),
            "/admin/posts?embed=1"
        );
        assert_eq!(
            ctx.url_with_embed("/admin/posts?tab=settings"),
            "/admin/posts?tab=settings&embed=1"
        );
        assert_eq!(
            ctx.url_with_embed("/admin/posts?embed=1"),
            "/admin/posts?embed=1"
        );
        assert_eq!(
            ctx.url_with_embed("/admin/posts?tab=settings&embed=1"),
            "/admin/posts?tab=settings&embed=1"
        );
    }
}