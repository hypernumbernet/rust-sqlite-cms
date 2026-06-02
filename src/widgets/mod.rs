use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::models::post::Post;
use crate::models::widget::{build_image_style, CarouselWidgetConfig, NewsWidgetConfig, WidgetType};
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
    WidgetTypeDef {
        key: "carousel",
        label: "画像カルーセル",
        description: "複数の画像をスライドショー形式で表示します。各画像にリンクを設定可能",
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

/// カルーセルウィジェットの1スライド。
#[derive(Debug, Clone, Serialize)]
pub struct CarouselSlide {
    pub image_url: String,
    pub alt: String,
    pub link_url: String,
}

/// カルーセルウィジェットがテンプレートへ渡すオブジェクト。
#[derive(Debug, Clone, Serialize)]
pub struct CarouselData {
    pub slides: Vec<CarouselSlide>,
    pub interval: u32,
    pub width: String,
    pub height: String,
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

/// ウィジェットタイプの説明文を返す（静的レジストリのみ）。
pub fn type_description(type_key: &str) -> &str {
    WIDGET_TYPES
        .iter()
        .find(|def| def.key == type_key)
        .map(|def| def.description)
        .unwrap_or("")
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

    let fragment_example = match type_key {
        "news" => format!(
            r#"<section id="{name}">
  <h2>お知らせ</h2>
  <div class="news-list">
    {{{{ {name}_html | safe }}}}
  </div>
</section>"#
        ),
        "image" => format!(
            r#"<div class="widget-image-wrap">
  {{{{ {name}_html | safe }}}}
</div>"#
        ),
        "carousel" => format!(
            r#"<div class="carousel-wrap">
  {{{{ {name}_html | safe }}}}
</div>"#
        ),
        _ => format!("{{{{ {name}_html | safe }}}}"),
    };

    let inline_hint = match type_key {
        "news" => format!(
            "上級者向け（インライン）: <code>has_{name}</code> / <code>{name}</code> 配列で \
             <code>{{% for item in {name} %}}</code> ループも可能。"
        ),
        "image" => format!(
            "上級者向け（インライン）: <code>has_{name}</code> / <code>{name}</code> オブジェクト \
             （image_url / alt / style など）を直接参照も可能。"
        ),
        "carousel" => format!(
            "上級者向け（インライン）: <code>has_{name}</code> / <code>{name}.slides</code> など \
             構造化データをページ内で直接ループも可能。"
        ),
        other => format!(
            "ウィジェットタイプ <code>{other}</code> 向け。<code>{name}</code> / \
             <code>has_{name}</code> も利用できます。"
        ),
    };

    let help = format!(
        "推奨: ウィジェット HTML は <a href=\"/admin/widgets\">ウィジェット</a> 画面で編集し、\
         ページでは <code>{{{{ {name}_html | safe }}}}</code> で差し込みます（<code>| safe</code> 必須）。\
         MiniJinja テンプレートページ（静的 HTML 以外）でのみ有効です。{inline_hint}"
    );

    (fragment_example, help)
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
    // インスタンス設定（placeholder.config）を優先、未設定時はタイプのconfigをフォールバック
    let instance_config: serde_json::Value =
        serde_json::from_str(&placeholder.config).unwrap_or(serde_json::Value::Object(Default::default()));

    match widget_type.type_key.as_str() {
        "news" => {
            // インスタンス > タイプ > デフォルト の優先順
            let mut cfg: NewsWidgetConfig =
                serde_json::from_str(&widget_type.config).unwrap_or_default();
            if let Some(limit) = instance_config.get("limit").and_then(|v| v.as_i64()) {
                if (1..=50).contains(&limit) {
                    cfg.limit = limit;
                }
            }

            let items: Vec<NewsItem> = posts::list_published_for_placeholder(
                pool,
                placeholder.id,
                cfg.limit,
            )
            .await?
            .into_iter()
            .map(NewsItem::from)
            .collect();
            let has_items = !items.is_empty();

            // 構造化データ（後方互換）
            ctx.insert(
                placeholder.name.clone(),
                serde_json::to_value(&items).expect("NewsItem is serializable"),
            );
            ctx.insert(
                format!("has_{}", placeholder.name),
                serde_json::Value::Bool(has_items),
            );

            // ウィジェットHTMLフラグメントをサーバサイドレンダリング（タイプ固定キー）
            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert("items".to_string(), serde_json::to_value(&items).unwrap_or_default());
            frag_ctx.insert("has_items".to_string(), serde_json::Value::Bool(has_items));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
        }
        "image" => {
            let entries =
                posts::list_published_for_placeholder_ordered(pool, placeholder.id, 1).await?;
            let Some(entry) = entries.into_iter().next() else {
                ctx.insert(
                    format!("has_{}", placeholder.name),
                    serde_json::Value::Bool(false),
                );
                let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                frag_ctx.insert("has_item".to_string(), serde_json::Value::Bool(false));
                frag_ctx.insert("config".to_string(), instance_config.clone());
                insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
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
                let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                frag_ctx.insert("has_item".to_string(), serde_json::Value::Bool(false));
                frag_ctx.insert("config".to_string(), instance_config.clone());
                insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
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
                    let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                    frag_ctx.insert("has_item".to_string(), serde_json::Value::Bool(false));
                    frag_ctx.insert("config".to_string(), instance_config.clone());
                    insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
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
                let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                frag_ctx.insert("has_item".to_string(), serde_json::Value::Bool(false));
                frag_ctx.insert("config".to_string(), instance_config.clone());
                insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
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

            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert("item".to_string(), serde_json::to_value(&item).unwrap_or_default());
            frag_ctx.insert("has_item".to_string(), serde_json::Value::Bool(true));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
        }
        "carousel" => {
            let mut cfg: CarouselWidgetConfig =
                serde_json::from_str(&widget_type.config).unwrap_or_default();
            if let Some(interval) = instance_config.get("interval").and_then(|v| v.as_u64()) {
                if (1..=30).contains(&interval) {
                    cfg.interval = interval as u32;
                }
            }
            if let Some(width) = instance_config.get("width").and_then(|v| v.as_str()) {
                if !width.trim().is_empty() {
                    cfg.width = width.to_string();
                }
            }
            if let Some(height) = instance_config.get("height").and_then(|v| v.as_str()) {
                if !height.trim().is_empty() {
                    cfg.height = height.to_string();
                }
            }

            let entries =
                posts::list_published_for_placeholder_ordered(pool, placeholder.id, 100).await?;

            let mut slides: Vec<CarouselSlide> = Vec::new();
            for entry in entries {
                let media_id_str = postmeta::get(pool, entry.id, "media_id").await?;
                let media_id = match media_id_str {
                    Some(value) => value.parse::<i64>().ok(),
                    None => None,
                };

                let Some(media_id) = media_id else {
                    tracing::warn!(
                        placeholder = %placeholder.name,
                        entry_id = entry.id,
                        "carousel entry missing media_id"
                    );
                    continue;
                };

                let attachment = match media::find(pool, media_id).await {
                    Ok(item) => item,
                    Err(_) => {
                        tracing::warn!(
                            placeholder = %placeholder.name,
                            media_id,
                            "carousel entry references missing media"
                        );
                        continue;
                    }
                };

                if !attachment.is_image() {
                    tracing::warn!(
                        placeholder = %placeholder.name,
                        media_id,
                        "carousel entry references non-image media"
                    );
                    continue;
                }

                let link_url = entry.content.trim().to_string();
                let alt = if entry.title.trim().is_empty() {
                    attachment.title.clone()
                } else {
                    entry.title.clone()
                };

                slides.push(CarouselSlide {
                    image_url: attachment.public_url(),
                    alt,
                    link_url,
                });
            }

            let has_slides = !slides.is_empty();
            let data = CarouselData {
                slides,
                interval: cfg.interval,
                width: cfg.width,
                height: cfg.height,
            };

            ctx.insert(
                placeholder.name.clone(),
                serde_json::to_value(&data).expect("CarouselData is serializable"),
            );
            ctx.insert(
                format!("has_{}", placeholder.name),
                serde_json::Value::Bool(has_slides),
            );

            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert("carousel".to_string(), serde_json::to_value(&data).unwrap_or_default());
            frag_ctx.insert("has_carousel".to_string(), serde_json::Value::Bool(has_slides));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
        }
        other => {
            tracing::debug!(
                widget_type = other,
                placeholder = %placeholder.name,
                "generic widget type render"
            );
            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert("config".to_string(), instance_config.clone());
            let rendered =
                insert_placeholder_html(ctx, &placeholder.name, widget_type, frag_ctx).await;
            ctx.insert(
                format!("has_{}", placeholder.name),
                serde_json::Value::Bool(rendered),
            );
        }
    }

    Ok(())
}

async fn insert_placeholder_html(
    ctx: &mut serde_json::Map<String, serde_json::Value>,
    placeholder_name: &str,
    widget_type: &WidgetType,
    frag_ctx: serde_json::Map<String, serde_json::Value>,
) -> bool {
    if let Some(html) = render_widget_fragment_with_data(widget_type, &frag_ctx).await {
        ctx.insert(
            format!("{}_html", placeholder_name),
            serde_json::Value::String(html),
        );
        true
    } else {
        false
    }
}

/// ウィジェットタイプの html_template をタイプ固定キーのローカルコンテキストで
/// MiniJinja レンダリングする。成功したHTML文字列を返す（呼び出し側で *_html として登録）。
async fn render_widget_fragment_with_data(
    widget_type: &WidgetType,
    data: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let tpl = widget_type.html_template.trim();
    if tpl.is_empty() {
        return None;
    }
    let mut env = minijinja::Environment::new();
    let tname = format!("wfrag_{}", widget_type.type_key);
    if let Err(e) = env.add_template(&tname, tpl) {
        tracing::error!(error = %e, type_key = %widget_type.type_key, "widget html_template パース失敗");
        return None;
    }
    let tmpl = match env.get_template(&tname) {
        Ok(t) => t,
        Err(_) => return None,
    };
    match tmpl.render(minijinja::Value::from_serialize(data)) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!(error = %e, type_key = %widget_type.type_key, "widget fragment レンダリング失敗");
            None
        }
    }
}
