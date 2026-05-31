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
        "carousel" => (
            format!(
                r#"{{% if has_{name} %}}
<div class="carousel" style="width:{{{{ {name}.width }}}}; height:{{{{ {name}.height }}}}; --interval: {{{{ {name}.interval }}}}s;">
  <div class="carousel-track">
    {{% for slide in {name}.slides %}}
    <div class="carousel-slide">
      {{% if slide.link_url %}}
      <a href="{{{{ slide.link_url }}}}">
        <img src="{{{{ slide.image_url }}}}" alt="{{{{ slide.alt }}}}">
      </a>
      {{% else %}}
      <img src="{{{{ slide.image_url }}}}" alt="{{{{ slide.alt }}}}">
      {{% endif %}}
    </div>
    {{% endfor %}}
  </div>
  {{% if {name}.slides | length > 1 %}}
  <button class="carousel-prev" type="button" aria-label="前へ">‹</button>
  <button class="carousel-next" type="button" aria-label="次へ">›</button>
  <div class="carousel-dots">
    {{% for slide in {name}.slides %}}<button class="carousel-dot" data-index="{{{{ loop.index0 }}}}" type="button"></button>{{% endfor %}}
  </div>
  {{% endif %}}
</div>
<style>
.carousel {{ position:relative; overflow:hidden; border-radius:8px; background:#f3f4f6; }}
.carousel-track {{ display:flex; height:100%; transition:transform 0.5s ease; }}
.carousel-slide {{ flex:0 0 100%; height:100%; }}
.carousel-slide img {{ width:100%; height:100%; object-fit:cover; display:block; }}
.carousel-slide a {{ display:block; height:100%; }}
.carousel-prev, .carousel-next {{ position:absolute; top:50%; transform:translateY(-50%); background:rgba(0,0,0,0.45); color:#fff; border:none; font-size:28px; width:40px; height:40px; border-radius:50%; cursor:pointer; display:flex; align-items:center; justify-content:center; }}
.carousel-prev {{ left:12px; }} .carousel-next {{ right:12px; }}
.carousel-dots {{ position:absolute; bottom:12px; left:50%; transform:translateX(-50%); display:flex; gap:8px; }}
.carousel-dot {{ width:10px; height:10px; border-radius:50%; background:rgba(255,255,255,0.6); border:none; padding:0; cursor:pointer; }}
.carousel-dot.active {{ background:#fff; }}
</style>
<script>
(function() {{
  var root = document.currentScript.previousElementSibling;
  if (!root || !root.classList.contains('carousel')) root = document.currentScript.parentElement.querySelector('.carousel');
  if (!root) return;
  var track = root.querySelector('.carousel-track');
  var slides = track ? Array.prototype.slice.call(track.children) : [];
  if (slides.length < 2) return;
  var prev = root.querySelector('.carousel-prev');
  var next = root.querySelector('.carousel-next');
  var dots = Array.prototype.slice.call(root.querySelectorAll('.carousel-dot'));
  var index = 0;
  var intervalMs = (parseFloat(getComputedStyle(root).getPropertyValue('--interval')) || 5) * 1000;
  var timer = null;

  function go(i) {{
    index = (i + slides.length) % slides.length;
    track.style.transform = 'translateX(-' + (index * 100) + '%)';
    dots.forEach(function(d, di) {{ d.classList.toggle('active', di === index); }});
  }}

  function start() {{
    stop();
    timer = setInterval(function() {{ go(index + 1); }}, intervalMs);
  }}
  function stop() {{ if (timer) clearInterval(timer); }}

  if (prev) prev.addEventListener('click', function() {{ go(index - 1); start(); }});
  if (next) next.addEventListener('click', function() {{ go(index + 1); start(); }});
  dots.forEach(function(dot, di) {{
    dot.addEventListener('click', function() {{ go(di); start(); }});
  }});

  root.addEventListener('mouseenter', stop);
  root.addEventListener('mouseleave', start);

  // init
  track.style.transform = 'translateX(0)';
  if (dots[0]) dots[0].classList.add('active');
  start();
}})();
</script>
{{% endif %}}"#
            ),
            format!(
                "変数 <code>{name}</code> は CarouselData（slides の配列 + interval / width / height）。\
                 各スライドは image_url / alt / link_url を持ちます。\
                 例のスニペットは自動再生・前後ボタン・ドット付きのセルフコンテインド実装です。\
                 <code>has_{name}</code> は公開済みスライドが1件以上あるとき true です。"
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

            // ウィジェットHTMLフラグメントをサーバサイドレンダリング（活性テンプレート）
            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert(placeholder.name.clone(), serde_json::to_value(&items).unwrap_or_default());
            frag_ctx.insert(format!("has_{}", placeholder.name), serde_json::Value::Bool(has_items));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            if let Some(html) = render_widget_fragment_with_data(widget_type, &frag_ctx).await {
                ctx.insert(format!("{}_html", placeholder.name), serde_json::Value::String(html));
            }
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

            let mut frag_ctx: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            frag_ctx.insert(placeholder.name.clone(), serde_json::to_value(&item).unwrap_or_default());
            frag_ctx.insert(format!("has_{}", placeholder.name), serde_json::Value::Bool(true));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            if let Some(html) = render_widget_fragment_with_data(widget_type, &frag_ctx).await {
                ctx.insert(format!("{}_html", placeholder.name), serde_json::Value::String(html));
            }
        }
        "carousel" => {
            let mut cfg: CarouselWidgetConfig =
                serde_json::from_str(&widget_type.config).unwrap_or_default();
            // インスタンス設定で一部オーバーライド可能（将来拡張）
            if let Some(interval) = instance_config.get("interval").and_then(|v| v.as_u64()) {
                if (1..=30).contains(&interval) {
                    cfg.interval = interval as u32;
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
            frag_ctx.insert(placeholder.name.clone(), serde_json::to_value(&data).unwrap_or_default());
            frag_ctx.insert(format!("has_{}", placeholder.name), serde_json::Value::Bool(has_slides));
            frag_ctx.insert("config".to_string(), instance_config.clone());
            if let Some(html) = render_widget_fragment_with_data(widget_type, &frag_ctx).await {
                ctx.insert(format!("{}_html", placeholder.name), serde_json::Value::String(html));
            }
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

/// ウィジェットタイプの html_template を、プレースホルダー名をキーとしたローカルコンテキストで
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
