//! 管理画面パンくずリストの組み立てヘルパー。

use super::layout::{AdminLayoutCtx, BreadcrumbItem};

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
    trail(vec![current("ダッシュボード")])
}

pub fn posts_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("投稿")])
}

pub fn posts_trash() -> Vec<BreadcrumbItem> {
    trail(vec![link("投稿", "/admin/posts"), current("ゴミ箱")])
}

pub fn posts_placeholder_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("投稿", "/admin/posts"),
        current("新規プレースホルダー"),
    ])
}

pub fn posts_placeholder_edit(name: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("投稿", "/admin/posts"),
        current(format!("{name} — 設定")),
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
        items.push(link("投稿", "/admin/posts"));
    }
    if is_settings_tab {
        items.push(link(
            name,
            format!("/admin/posts/placeholders/{placeholder_id}"),
        ));
        items.push(current("設定"));
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
        items.push(link("投稿", "/admin/posts"));
    }
    items.push(link(
        placeholder_name,
        format!("/admin/posts/placeholders/{placeholder_id}"),
    ));
    items.push(current(page_label));
    items
}

pub fn pages_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("ページ")])
}

pub fn pages_form(current_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("ページ", "/admin/pages"),
        current(current_label),
    ])
}

pub fn pages_gallery() -> Vec<BreadcrumbItem> {
    trail(vec![link("ページ", "/admin/pages"), current("デザインを選ぶ")])
}

pub fn pages_preview_error() -> Vec<BreadcrumbItem> {
    trail(vec![link("ページ", "/admin/pages"), current("プレビュー")])
}

pub fn layouts_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("レイアウト")])
}

pub fn layouts_import() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("レイアウト", "/admin/layouts"),
        current("インポート"),
    ])
}

pub fn layouts_form(name: &str, is_new: bool) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("レイアウト", "/admin/layouts"),
        current(if is_new {
            "新規レイアウト".to_string()
        } else {
            name.to_string()
        }),
    ])
}

pub fn layouts_file_edit(layout_id: i64, layout_name: &str, file_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("レイアウト", "/admin/layouts"),
        link(layout_name, format!("/admin/layouts/{layout_id}/edit")),
        current(file_label),
    ])
}

pub fn widgets_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("ウィジェット")])
}

pub fn widgets_import() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("ウィジェット", "/admin/widgets"),
        current("インポート"),
    ])
}

pub fn widgets_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("ウィジェット", "/admin/widgets"),
        current("新規追加"),
    ])
}

pub fn widgets_edit(type_label: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("ウィジェット", "/admin/widgets"),
        current(type_label),
    ])
}

pub fn media_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("メディア")])
}

pub fn samples_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("サンプル")])
}

pub fn users_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("ユーザー")])
}

pub fn users_form(display_name: &str, is_new: bool) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("ユーザー", "/admin/users"),
        current(if is_new {
            "新規ユーザー".to_string()
        } else {
            display_name.to_string()
        }),
    ])
}

pub fn settings_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("設定")])
}

pub fn backup_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("バックアップ / リストア")])
}

pub fn database_index() -> Vec<BreadcrumbItem> {
    trail(vec![current("DB管理")])
}

pub fn database_table_data(table_name: &str, read_only: bool) -> Vec<BreadcrumbItem> {
    let suffix = if read_only { "（閲覧専用）" } else { "" };
    trail(vec![
        link("DB管理", "/admin/database"),
        current(format!("{table_name} データ{suffix}")),
    ])
}

pub fn database_table_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database"),
        current("テーブル作成"),
    ])
}

pub fn database_table_edit(name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database"),
        link(format!("{name} データ"), data_url),
        current("列編集"),
    ])
}

pub fn database_table_seed(table_name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database"),
        link(format!("{table_name} データ"), data_url),
        current("テストデータ生成"),
    ])
}

pub fn database_table_notice(table_name: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database"),
        current(format!("{table_name} — DB管理")),
    ])
}

pub fn database_view_new() -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database?tab=views"),
        current("ビュー作成"),
    ])
}

pub fn database_view_edit(name: &str, data_url: &str) -> Vec<BreadcrumbItem> {
    trail(vec![
        link("DB管理", "/admin/database?tab=views"),
        link(format!("{name} データ"), data_url),
        current("定義編集"),
    ])
}

/// `AdminLayoutCtx` にパンくずを設定するショートカット。
pub fn with(ctx: AdminLayoutCtx, items: Vec<BreadcrumbItem>) -> AdminLayoutCtx {
    ctx.with_breadcrumbs(items)
}