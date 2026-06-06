//! ウィジェット設定サービス。

use sqlx::SqlitePool;

use crate::error::DomainResult;
use crate::models::widget::{
    WidgetImportAction, WidgetImportMode, WidgetPackage, WidgetType, WidgetTypeInput,
};
use crate::repos::widget_types as widget_types_repo;

/// 全ウィジェットタイプを取得。
pub async fn list_all(pool: &SqlitePool) -> DomainResult<Vec<WidgetType>> {
    widget_types_repo::list_all(pool).await.map_err(Into::into)
}

/// 指定タイプの config + html_template を更新。
/// ウィジェット画面のHTML編集に対応。
pub async fn update_config(
    pool: &SqlitePool,
    type_key: &str,
    config_json: &str,
    html_template: &str,
) -> DomainResult<()> {
    let input = WidgetTypeInput {
        config: config_json.to_string(),
        html_template: html_template.to_string(),
    };
    widget_types_repo::update_config(pool, type_key, &input)
        .await
        .map_err(Into::into)
}

/// ウィジェットタイプ全体を更新（type_key の変更 + html_template + config）。
pub async fn update(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
) -> DomainResult<()> {
    widget_types_repo::update(pool, old_type_key, new_type_key, html_template, config)
        .await
        .map_err(Into::into)
}

/// ウィジェットタイプ全体を更新（config_schema も含む）。
pub async fn update_with_schema(
    pool: &SqlitePool,
    old_type_key: &str,
    new_type_key: &str,
    html_template: &str,
    config: &str,
    config_schema: &str,
) -> DomainResult<()> {
    widget_types_repo::update_with_schema(
        pool,
        old_type_key,
        new_type_key,
        html_template,
        config,
        config_schema,
    )
    .await
    .map_err(Into::into)
}

/// ウィジェットタイプをエクスポート用パッケージに変換する。
pub async fn export_package(pool: &SqlitePool, type_key: &str) -> DomainResult<WidgetPackage> {
    let row = widget_types_repo::find_by_key(pool, type_key).await?;
    Ok(widget_type_to_package(&row))
}

fn widget_type_to_package(row: &WidgetType) -> WidgetPackage {
    let label = effective_label(row);
    WidgetPackage {
        format_version: 1,
        type_key: row.type_key.clone(),
        label,
        description: row.description.clone(),
        config: row.config.clone(),
        html_template: row.html_template.clone(),
        config_schema: row.config_schema.clone(),
    }
}

fn effective_label(row: &WidgetType) -> String {
    let trimmed = row.label.trim();
    if trimmed.is_empty() {
        row.type_key.clone()
    } else {
        trimmed.to_string()
    }
}

/// パッケージを検証する。
pub fn validate_package(package: &WidgetPackage) -> DomainResult<()> {
    if package.format_version != 1 {
        return Err(crate::error::DomainError::Validation(
            "format_version は 1 のみ対応しています".to_string(),
        ));
    }

    let key = package.type_key.trim();
    if key.is_empty() {
        return Err(crate::error::DomainError::Validation(
            "type_key を指定してください".to_string(),
        ));
    }
    if !is_valid_type_key(key) {
        return Err(crate::error::DomainError::Validation(
            "type_key は英小文字で始まり、英小文字・数字・アンダースコアのみ使用できます".to_string(),
        ));
    }

    serde_json::from_str::<serde_json::Value>(&package.config).map_err(|_| {
        crate::error::DomainError::Validation("config は有効な JSON である必要があります".to_string())
    })?;
    serde_json::from_str::<serde_json::Value>(&package.config_schema).map_err(|_| {
        crate::error::DomainError::Validation(
            "config_schema は有効な JSON である必要があります".to_string(),
        )
    })?;

    if package.html_template.trim().is_empty() {
        tracing::warn!(type_key = %key, "importing widget with empty html_template");
    }

    Ok(())
}

fn is_valid_type_key(key: &str) -> bool {
    let Some(first) = key.chars().next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// パッケージをインポートする。
pub async fn import_package(
    pool: &SqlitePool,
    package: &WidgetPackage,
    mode: WidgetImportMode,
    target_key: Option<&str>,
) -> DomainResult<(WidgetImportAction, String)> {
    validate_package(package)?;

    let (type_key, package) = if mode == WidgetImportMode::Rename {
        let key = target_key
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                crate::error::DomainError::Validation(
                    "インポート先 type_key を指定してください".to_string(),
                )
            })?;
        if !is_valid_type_key(key) {
            return Err(crate::error::DomainError::Validation(
                "type_key は英小文字で始まり、英小文字・数字・アンダースコアのみ使用できます"
                    .to_string(),
            )
            .into());
        }
        if widget_types_repo::exists_by_key(pool, key).await? {
            return Err(crate::error::DomainError::Conflict(format!(
                "指定した type_key「{key}」は既に使われています"
            ))
            .into());
        }
        let mut renamed = package.clone();
        renamed.type_key = key.to_string();
        validate_package(&renamed)?;
        (key.to_string(), renamed)
    } else {
        (package.type_key.trim().to_string(), package.clone())
    };

    let exists = widget_types_repo::exists_by_key(pool, &type_key).await?;

    if exists && mode == WidgetImportMode::Skip {
        return Ok((
            WidgetImportAction::Skipped,
            format!("ウィジェット「{type_key}」は既に存在するためスキップしました"),
        ));
    }

    let label = {
        let trimmed = package.label.trim();
        if trimmed.is_empty() {
            type_key.clone()
        } else {
            trimmed.to_string()
        }
    };
    let description = package.description.trim().to_string();

    let action = if exists {
        WidgetImportAction::Updated
    } else {
        WidgetImportAction::Created
    };

    widget_types_repo::upsert_package(
        pool,
        &type_key,
        &label,
        &description,
        &package.config,
        &package.html_template,
        &package.config_schema,
    )
    .await?;

    let message = match action {
        WidgetImportAction::Created => {
            if mode == WidgetImportMode::Rename {
                format!("ウィジェット「{type_key}」を別名で新規登録しました")
            } else {
                format!("ウィジェット「{type_key}」を新規登録しました")
            }
        }
        WidgetImportAction::Updated => format!("ウィジェット「{type_key}」を更新しました"),
        WidgetImportAction::Skipped => unreachable!(),
    };

    Ok((action, message))
}

/// type_key の表示ラベル（DB label 優先、空なら静的レジストリ、それもなければ type_key）。
pub fn display_label(row: &WidgetType) -> String {
    let trimmed = row.label.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    crate::widgets::type_label(&row.type_key).to_string()
}

/// ウィジェットタイプを削除する。プレースホルダーが紐付いている場合は `Conflict`。
pub async fn delete(pool: &SqlitePool, type_key: &str) -> DomainResult<()> {
    let refs = widget_types_repo::count_placeholder_references(pool, type_key).await?;
    if refs > 0 {
        return Err(crate::error::DomainError::Conflict(format!(
            "プレースホルダーが {refs} 件紐付いているため削除できません。先に /admin/posts からプレースホルダーを削除してください。"
        )));
    }

    widget_types_repo::delete_by_type_key(pool, type_key)
        .await
        .map_err(Into::into)
}

/// 説明文（DB 優先、空なら静的レジストリ）。
pub fn display_description(row: &WidgetType) -> String {
    let trimmed = row.description.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    crate::widgets::type_description(&row.type_key).to_string()
}
