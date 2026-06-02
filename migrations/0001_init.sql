-- 統合初期スキーマ（旧 migrations 0001〜0005 を単一ファイル化）
-- 日時はすべて UTC の ISO8601 文字列(TEXT)で保持する。

-- ウィジェットタイプ（コードレジストリと 1:1、設定のみ DB 保持） -------------
CREATE TABLE IF NOT EXISTS widget_types (
    id            INTEGER PRIMARY KEY,
    type_key      TEXT NOT NULL UNIQUE,
    config        TEXT NOT NULL DEFAULT '{}',
    html_template TEXT NOT NULL DEFAULT '',
    config_schema TEXT NOT NULL DEFAULT '{}',
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- プレースホルダー（テンプレート参照名） ------------------------------------
CREATE TABLE IF NOT EXISTS placeholders (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL UNIQUE,
    widget_type_id INTEGER NOT NULL REFERENCES widget_types(id),
    config         TEXT NOT NULL DEFAULT '{}',
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_placeholders_widget_type ON placeholders(widget_type_id);

-- 投稿エントリ・メディア添付の共通テーブル ---------------------------------
CREATE TABLE IF NOT EXISTS posts (
    id             INTEGER PRIMARY KEY,
    post_type      TEXT    NOT NULL DEFAULT 'post',   -- post / attachment
    post_status    TEXT    NOT NULL DEFAULT 'draft',  -- draft / publish / future / trash
    post_name      TEXT,                              -- スラッグ
    title          TEXT    NOT NULL DEFAULT '',
    content        TEXT    NOT NULL DEFAULT '',
    excerpt        TEXT    NOT NULL DEFAULT '',
    menu_order     INTEGER NOT NULL DEFAULT 0,
    published_at   TEXT,                              -- 公開日時(予約投稿で利用)
    placeholder_id INTEGER REFERENCES placeholders(id),
    created_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_posts_type_status ON posts(post_type, post_status);
CREATE INDEX IF NOT EXISTS idx_posts_name ON posts(post_name);
CREATE INDEX IF NOT EXISTS idx_posts_placeholder ON posts(placeholder_id);

CREATE TABLE IF NOT EXISTS postmeta (
    id         INTEGER PRIMARY KEY,
    post_id    INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    meta_key   TEXT    NOT NULL,
    meta_value TEXT
);
CREATE INDEX IF NOT EXISTS idx_postmeta_post_key ON postmeta(post_id, meta_key);

-- サイト設定(key-value) ----------------------------------------------------
CREATE TABLE IF NOT EXISTS options (
    option_id    INTEGER PRIMARY KEY,
    option_name  TEXT    NOT NULL UNIQUE,
    option_value TEXT    NOT NULL DEFAULT '',
    autoload     INTEGER NOT NULL DEFAULT 1
);

-- ページ（テンプレート / 固定ページ。本文は work/ 配下、DB にはメタ情報のみ） ----
-- is_static=0: work/templates/ + MiniJinja、is_static=1: work/pages/ + 生 HTML
CREATE TABLE IF NOT EXISTS pages (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,
    file_name    TEXT    UNIQUE,
    is_static    INTEGER NOT NULL DEFAULT 0,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_pages_published ON pages(is_published);

-- 初期シードデータ ---------------------------------------------------------
-- テンプレート（presets/index.html など）が参照する "news" プレースホルダー
INSERT INTO widget_types (type_key, config, html_template, config_schema)
VALUES (
    'news',
    '{"limit":5}',
    '{% if has_items %}
  {% for item in items %}
  <article class="news-item">
    <time class="news-date">{{ item.display_date }}</time>
    <div>
      <h3 class="news-title">{{ item.title }}</h3>
      <p class="news-excerpt">{{ item.excerpt }}</p>
    </div>
  </article>
  {% endfor %}
{% else %}
  <p class="empty-news">現在公開中のお知らせはありません。</p>
{% endif %}',
    '{
  "fields": [
    {
      "key": "limit",
      "label": "表示件数",
      "type": "number",
      "default": 5,
      "min": 1,
      "max": 50,
      "help": "1〜50の範囲で指定してください"
    }
  ]
}'
);

INSERT INTO placeholders (name, widget_type_id)
SELECT 'news', id FROM widget_types WHERE type_key = 'news';

-- 画像ウィジェットタイプ（単一表示）
INSERT INTO widget_types (type_key, config, html_template, config_schema)
VALUES (
    'image',
    '{}',
    '{% if has_item %}
<figure class="widget-image" style="{{ item.style }}">
  {% if item.link_url %}
  <a href="{{ item.link_url }}">
    <img src="{{ item.image_url }}" alt="{{ item.alt }}">
  </a>
  {% else %}
  <img src="{{ item.image_url }}" alt="{{ item.alt }}">
  {% endif %}
</figure>
{% endif %}',
    '{
  "fields": []
}'
);

-- 画像カルーセルウィジェットタイプ
INSERT INTO widget_types (type_key, config, html_template, config_schema)
VALUES (
    'carousel',
    '{"interval":5,"width":"100%","height":"400px"}',
    '{% if has_carousel %}
<div class="carousel" style="width:{{ carousel.width }}; height:{{ carousel.height }}; --interval: {{ carousel.interval }}s;">
  <div class="carousel-track">
    {% for slide in carousel.slides %}
    <div class="carousel-slide">
      {% if slide.link_url %}
      <a href="{{ slide.link_url }}">
        <img src="{{ slide.image_url }}" alt="{{ slide.alt }}">
      </a>
      {% else %}
      <img src="{{ slide.image_url }}" alt="{{ slide.alt }}">
      {% endif %}
    </div>
    {% endfor %}
  </div>
  {% if carousel.slides | length > 1 %}
  <button class="carousel-prev" type="button" aria-label="前へ">‹</button>
  <button class="carousel-next" type="button" aria-label="次へ">›</button>
  <div class="carousel-dots">
    {% for slide in carousel.slides %}<button class="carousel-dot" data-index="{{ loop.index0 }}" type="button"></button>{% endfor %}
  </div>
  {% endif %}
</div>
<style>
.carousel { position:relative; overflow:hidden; border-radius:8px; background:#f3f4f6; }
.carousel-track { display:flex; height:100%; transition:transform 0.5s ease; }
.carousel-slide { flex:0 0 100%; height:100%; }
.carousel-slide img { width:100%; height:100%; object-fit:cover; display:block; }
.carousel-slide a { display:block; height:100%; }
.carousel-prev, .carousel-next { position:absolute; top:50%; transform:translateY(-50%); background:rgba(0,0,0,0.45); color:#fff; border:none; font-size:28px; width:40px; height:40px; border-radius:50%; cursor:pointer; display:flex; align-items:center; justify-content:center; }
.carousel-prev { left:12px; } .carousel-next { right:12px; }
.carousel-dots { position:absolute; bottom:12px; left:50%; transform:translateX(-50%); display:flex; gap:8px; }
.carousel-dot { width:10px; height:10px; border-radius:50%; background:rgba(255,255,255,0.6); border:none; padding:0; cursor:pointer; }
.carousel-dot.active { background:#fff; }
</style>
<script>
(function() {
  var root = document.currentScript.previousElementSibling;
  if (!root || !root.classList.contains(''carousel'')) root = document.currentScript.parentElement.querySelector(''.carousel'');
  if (!root) return;
  var track = root.querySelector(''.carousel-track'');
  var slides = track ? Array.prototype.slice.call(track.children) : [];
  if (slides.length < 2) return;
  var prev = root.querySelector(''.carousel-prev'');
  var next = root.querySelector(''.carousel-next'');
  var dots = Array.prototype.slice.call(root.querySelectorAll(''.carousel-dot''));
  var index = 0;
  var intervalMs = (parseFloat(getComputedStyle(root).getPropertyValue(''--interval'')) || 5) * 1000;
  var timer = null;

  function go(i) {
    index = (i + slides.length) % slides.length;
    track.style.transform = ''translateX(-'' + (index * 100) + ''%)'';
    dots.forEach(function(d, di) { d.classList.toggle(''active'', di === index); });
  }

  function start() {
    stop();
    timer = setInterval(function() { go(index + 1); }, intervalMs);
  }
  function stop() { if (timer) clearInterval(timer); }

  if (prev) prev.addEventListener(''click'', function() { go(index - 1); start(); });
  if (next) next.addEventListener(''click'', function() { go(index + 1); start(); });
  dots.forEach(function(dot, di) {
    dot.addEventListener(''click'', function() { go(di); start(); });
  });

  root.addEventListener(''mouseenter'', stop);
  root.addEventListener(''mouseleave'', start);

  track.style.transform = ''translateX(0)'';
  if (dots[0]) dots[0].classList.add(''active'');
  start();
})();
</script>
{% endif %}',
    '{
  "fields": [
    {
      "key": "interval",
      "label": "スライド間隔（秒）",
      "type": "number",
      "default": 5,
      "min": 1,
      "max": 30,
      "help": "1〜30秒"
    },
    {
      "key": "width",
      "label": "領域の幅",
      "type": "text",
      "default": "100%",
      "help": "例: 100%, 600px, 50vw"
    },
    {
      "key": "height",
      "label": "領域の高さ",
      "type": "text",
      "default": "400px",
      "help": "例: 400px, 50vh"
    }
  ]
}'
);
