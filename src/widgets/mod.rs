use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::post::Post;
use crate::models::widget::{NewsWidgetConfig, WidgetType};
use crate::repos::{placeholders, posts, widget_types};

/// 登録済みウィジェットタイプの定義。
#[derive(Debug, Clone, Copy)]
pub struct WidgetTypeDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const WIDGET_TYPES: &[WidgetTypeDef] = &[WidgetTypeDef {
    key: "news",
    label: "お知らせ",
    description: "公開済みの投稿を新しい順に表示します",
}];

/// news ウィジェットがテンプレートへ渡す各項目。
#[derive(Debug, Clone, Serialize)]
pub struct NewsItem {
    pub title: String,
    pub excerpt: String,
    pub display_date: String,
}

impl From<Post> for NewsItem {
    fn from(post: Post) -> Self {
        let display_date = post.published_at.unwrap_or(post.created_at);
        let excerpt = if post.excerpt.trim().is_empty() {
            post.content
        } else {
            post.excerpt
        };

        Self {
            title: post.title,
            excerpt,
            display_date,
        }
    }
}

/// 登録キーが有効なウィジェットタイプかどうか。
pub fn is_known_type(type_key: &str) -> bool {
    WIDGET_TYPES.iter().any(|def| def.key == type_key)
}

/// ウィジェットタイプの表示ラベルを返す。
pub fn type_label(type_key: &str) -> &str {
    WIDGET_TYPES
        .iter()
        .find(|def| def.key == type_key)
        .map(|def| def.label)
        .unwrap_or(type_key)
}

/// サイト変数と全プレースホルダーを解決し、MiniJinja 用コンテキストを返す。
pub async fn build_render_context(
    pool: &SqlitePool,
    blogname: String,
    blogdescription: String,
) -> AppResult<minijinja::Value> {
    let placeholder_rows = placeholders::list_all(pool).await?;
    let widget_type_rows = widget_types::list_all(pool).await?;
    let type_by_id: std::collections::HashMap<i64, WidgetType> =
        widget_type_rows.into_iter().map(|t| (t.id, t)).collect();

    let mut ctx = serde_json::Map::new();
    ctx.insert(
        "blogname".into(),
        serde_json::Value::String(blogname),
    );
    ctx.insert(
        "blogdescription".into(),
        serde_json::Value::String(blogdescription),
    );

    for placeholder in &placeholder_rows {
        let Some(widget_type) = type_by_id.get(&placeholder.widget_type_id) else {
            tracing::warn!(
                placeholder = %placeholder.name,
                widget_type_id = placeholder.widget_type_id,
                "placeholder references missing widget type"
            );
            continue;
        };
        resolve_placeholder(pool, placeholder, widget_type, &mut ctx).await?;
    }

    Ok(minijinja::Value::from_serialize(ctx))
}

async fn resolve_placeholder(
    pool: &SqlitePool,
    placeholder: &crate::models::placeholder::Placeholder,
    widget_type: &WidgetType,
    ctx: &mut serde_json::Map<String, serde_json::Value>,
) -> AppResult<()> {
    match widget_type.type_key.as_str() {
        "news" => {
            let config: NewsWidgetConfig =
                serde_json::from_str(&widget_type.config).unwrap_or_default();
            let items: Vec<NewsItem> = posts::list_published_for_placeholder(
                pool,
                placeholder.id,
                config.limit,
            )
            .await?
            .into_iter()
            .map(NewsItem::from)
            .collect();
            let has_items = !items.is_empty();
            ctx.insert(
                placeholder.name.clone(),
                serde_json::to_value(&items).expect("NewsItem is serializable"),
            );
            ctx.insert(
                format!("has_{}", placeholder.name),
                serde_json::Value::Bool(has_items),
            );
        }
        other => {
            tracing::warn!(
                widget_type = other,
                placeholder = %placeholder.name,
                "unknown widget type"
            );
        }
    }

    Ok(())
}
