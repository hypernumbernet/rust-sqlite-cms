//! 管理画面パンくずリストの組み立てヘルパー。

use super::layout::{AdminLayoutCtx, BreadcrumbItem};
use crate::i18n;

fn t(key: &str) -> String {
    i18n::t(key).to_string()
}

fn link(label: impl Into<String>, href: impl Into<String>) -> BreadcrumbItem {
    BreadcrumbItem {
        label: label.into(),
        href: href.into(),
    }
}

fn current(label: impl Into<String>) -> BreadcrumbItem {
    BreadcrumbItem {
        label: label.into(),
        href: String::new(),
    }
}

fn trail(items: Vec<BreadcrumbItem>) -> Vec<BreadcrumbItem> {
    items
}

pub fn dashboard() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.dashboard"))])
}

pub fn posts_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.posts"))])
}

pub fn posts_trash() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.posts"), "/admin/posts"),
        current(t("breadcrumb.trash")),
    ])
}

pub fn posts_placeholder_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.posts"), "/admin/posts"),
        current(t("breadcrumb.placeholder_new")),
    ])
}

pub fn posts_placeholder_edit(name: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.posts"), "/admin/posts"),
        current(format!("{name}{}", t("breadcrumb.placeholder_settings_em"))),
    ])
}

pub fn posts_placeholder_manage(
    placeholder_id: i64,
    name: &str,
    embed: bool,
    is_settings_tab: bool,
) -> Vec<BreadcrumbItem> {
    let mut items = Vec::new();
    if !embed {
        items.push(link(t("breadcrumb.posts"), "/admin/posts"));
    }
    if is_settings_tab {
        items.push(link(
            name,
            format!("/admin/posts/placeholders/{placeholder_id}"),
        ));
        items.push(current(t("breadcrumb.settings")));
    } else {
        items.push(current(name.to_string()));
    }
    items
}

pub fn posts_entry(
    placeholder_id: i64,
    placeholder_name: &str,
    page_label: &str,
    embed: bool,
) -> Vec<BreadcrumbItem> {
    let mut items = Vec::new();
    if !embed {
        items.push(link(t("breadcrumb.posts"), "/admin/posts"));
    }
    items.push(link(
        placeholder_name,
        format!("/admin/posts/placeholders/{placeholder_id}"),
    ));
    items.push(current(page_label));
    items
}

pub fn pages_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.pages"))])
}

pub fn pages_form(current_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.pages"), "/admin/pages"),
        current(current_label),
    ])
}

pub fn pages_gallery() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.pages"), "/admin/pages"),
        current(t("breadcrumb.choose_design")),
    ])
}

pub fn pages_preview_error() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.pages"), "/admin/pages"),
        current(t("breadcrumb.preview")),
    ])
}

pub fn layouts_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.layouts"))])
}

pub fn layouts_import() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.layouts"), "/admin/layouts"),
        current(t("breadcrumb.import")),
    ])
}

pub fn layouts_form(name: &str, is_new: bool) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.layouts"), "/admin/layouts"),
        current(if is_new {
            t("breadcrumb.layout_new")
        } else {
            name.to_string()
        }),
    ])
}

pub fn layouts_file_edit(layout_id: i64, layout_name: &str, file_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.layouts"), "/admin/layouts"),
        link(layout_name, format!("/admin/layouts/{layout_id}/edit")),
        current(file_label),
    ])
}

pub fn widgets_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.widgets"))])
}

pub fn widgets_import() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.widgets"), "/admin/widgets"),
        current(t("breadcrumb.import")),
    ])
}

pub fn widgets_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.widgets"), "/admin/widgets"),
        current(t("breadcrumb.widget_new")),
    ])
}

pub fn widgets_edit(type_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.widgets"), "/admin/widgets"),
        current(type_label),
    ])
}

pub fn media_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.media"))])
}

pub fn samples_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.samples"))])
}

pub fn users_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.users"))])
}

pub fn users_form(display_name: &str, is_new: bool) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.users"), "/admin/users"),
        current(if is_new {
            t("breadcrumb.user_new")
        } else {
            display_name.to_string()
        }),
    ])
}

pub fn settings_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.settings"))])
}

pub fn backup_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.backup"))])
}

pub fn database_index() -> Vec<BreadcrumbItem> {
    trail(vec![current(t("breadcrumb.database"))])
}

pub fn database_table_data(table_name: &str, read_only: bool) -> Vec<BreadcrumbItem> {
    let label = if read_only {
        format!("{table_name}{}", t("breadcrumb.data_read_only"))
    } else {
        format!("{table_name}{}", t("breadcrumb.data_suffix"))
    };
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database"),
        current(label),
    ])
}

pub fn database_table_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database"),
        current(t("breadcrumb.table_create")),
    ])
}

pub fn database_table_edit(name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database"),
        link(format!("{name}{}", t("breadcrumb.data_suffix")), data_url),
        current(t("breadcrumb.column_edit")),
    ])
}

pub fn database_table_seed(table_name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database"),
        link(
            format!("{table_name}{}", t("breadcrumb.data_suffix")),
            data_url,
        ),
        current(t("breadcrumb.seed_data")),
    ])
}

pub fn database_table_notice(table_name: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database"),
        current(format!("{table_name}{}", t("breadcrumb.table_db_admin_suffix"))),
    ])
}

pub fn database_view_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database?tab=views"),
        current(t("breadcrumb.view_create")),
    ])
}

pub fn database_view_edit(name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link(t("breadcrumb.database"), "/admin/database?tab=views"),
        link(format!("{name}{}", t("breadcrumb.data_suffix")), data_url),
        current(t("breadcrumb.view_definition_edit")),
    ])
}

/// `AdminLayoutCtx` にパンくずを設定するショートカット。
pub fn with(ctx: AdminLayoutCtx, items: Vec<BreadcrumbItem>) -> AdminLayoutCtx {
    ctx.with_breadcrumbs(items)
}
