use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::post::Post;
use crate::models::widget::{build_image_style, NewsWidgetConfig, WidgetType};
use crate::repos::{media, placeholders, postmeta, posts, widget_types};

/// 登録済みウィジェットタイプの定義。
#[derive(Debug, Clone, Copy)]
pub struct WidgetTypeDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const WIDGET_TYPES: &[WidgetTypeDef] = &[
    WidgetTypeDef {
        key: "news",
        label: "お知らせ",
        description: "公開済みの投稿を新しい順に表示します",
    },
    WidgetTypeDef {
        key: "image",
        label: "画像",
        description: "メディアライブラリの画像を1枚表示します。回り込み・マージンを設定できます",
    },
];

/// news ウィジェットがテンプレートへ渡す各項目。
#[derive(Debug, Clone, Serialize)]
pub struct NewsItem {
    pub title: String,
    pub excerpt: String,
    pub display_date: String,
}

/// image ウィジェットがテンプレートへ渡すオブジェクト。
#[derive(Debug, Clone, Serialize)]
pub struct ImageItem {
    pub image_url: String,
    pub alt: String,
    pub link_url: String,
    pub float: String,
    pub margin: String,
    pub style: String,
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

/// 管理画面向けに、プレースホルダー名を埋め込んだ MiniJinja 利用例と説明文を返す。
pub fn template_usage(type_key: &str, placeholder_name: &str) -> (String, String) {
    let name = {
        let trimmed = placeholder_name.trim();
        if trimmed.is_empty() {
            match type_key {
                "image" => "hero",
                _ => "news",
            }
        } else {
            trimmed
        }
    };

    match type_key {
        "news" => (
            format!(
                r#"{{% if has_{name} %}}
<section id="{name}">
  <h2>お知らせ</h2>
  <div class="news-list">
    {{% for item in {name} %}}
    <article class="news-item">
      <time class="news-date">{{{{ item.display_date }}}}</time>
      <div>
        <h3 class="news-title">{{{{ item.title }}}}</h3>
        <p class="news-excerpt">{{{{ item.excerpt }}}}</p>
      </div>
    </article>
    {{% endfor %}}
  </div>
</section>
{{% else %}}
<p class="empty-news">現在公開中のお知らせはありません。</p>
{{% endif %}}"#
            ),
            format!(
                "変数 <code>{name}</code> は NewsItem の配列（title / excerpt / display_date）。\
                 <code>has_{name}</code> は公開済みエントリが 1 件以上あるとき true になります。"
            ),
        ),
        "image" => (
            format!(
                r#"{{% if has_{name} %}}
<figure class="widget-image" style="{{{{ {name}.style }}}}">
  {{% if {name}.link_url %}}
  <a href="{{{{ {name}.link_url }}}}">
    <img src="{{{{ {name}.image_url }}}}" alt="{{{{ {name}.alt }}}}">
  </a>
  {{% else %}}
  <img src="{{{{ {name}.image_url }}}}" alt="{{{{ {name}.alt }}}}">
  {{% endif %}}
</figure>
{{% endif %}}"#
            ),
            format!(
                "変数 <code>{name}</code> は ImageItem（image_url / alt / link_url / style など）。\
                 float と margin は <code>{name}.style</code> に反映済みです。\
                 <code>has_{name}</code> は公開済み画像エントリがあるとき true です。"
            ),
        ),
        other => (
            format!(
                "{{% if has_{name} %}}\n  {{{{ {name} }}}}\n{{% endif %}}"
            ),
            format!(
                "ウィジェットタイプ <code>{other}</code> 向けの例です。\
                 <code>{name}</code> と <code>has_{name}</code> をテンプレートで参照します。"
            ),
        ),
    }
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
        "image" => {
            let entries =
                posts::list_published_for_placeholder_ordered(pool, placeholder.id, 1).await?;
            let Some(entry) = entries.into_iter().next() else {
                ctx.insert(
                    format!("has_{}", placeholder.name),
                    serde_json::Value::Bool(false),
                );
                return Ok(());
            };

            // 並列で postmeta を取得（N+1 緩和）
            let (media_id_str, float, margin) = tokio::join!(
                postmeta::get(pool, entry.id, "media_id"),
                postmeta::get(pool, entry.id, "float"),
                postmeta::get(pool, entry.id, "margin"),
            );

            let media_id = match media_id_str? {
                Some(value) => value.parse::<i64>().ok(),
                None => None,
            };

            let Some(media_id) = media_id else {
                tracing::warn!(
                    placeholder = %placeholder.name,
                    entry_id = entry.id,
                    "image entry missing media_id"
                );
                ctx.insert(
                    format!("has_{}", placeholder.name),
                    serde_json::Value::Bool(false),
                );
                return Ok(());
            };

            let attachment = match media::find(pool, media_id).await {
                Ok(item) => item,
                Err(_) => {
                    tracing::warn!(
                        placeholder = %placeholder.name,
                        media_id,
                        "image entry references missing media"
                    );
                    ctx.insert(
                        format!("has_{}", placeholder.name),
                        serde_json::Value::Bool(false),
                    );
                    return Ok(());
                }
            };

            if !attachment.is_image() {
                tracing::warn!(
                    placeholder = %placeholder.name,
                    media_id,
                    "image entry references non-image media"
                );
                ctx.insert(
                    format!("has_{}", placeholder.name),
                    serde_json::Value::Bool(false),
                );
                return Ok(());
            }

            let float = float?.unwrap_or_else(|| "none".to_string());
            let margin = margin?.unwrap_or_default();
            let style = build_image_style(&float, &margin);

            let item = ImageItem {
                image_url: attachment.public_url(),
                alt: entry.title,
                link_url: entry.content,
                float,
                margin,
                style,
            };

            ctx.insert(
                placeholder.name.clone(),
                serde_json::to_value(&item).expect("ImageItem is serializable"),
            );
            ctx.insert(
                format!("has_{}", placeholder.name),
                serde_json::Value::Bool(true),
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
