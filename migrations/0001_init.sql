-- 統合初期スキーマ（migrations を単一ファイル化）
-- 日時はすべて UTC の ISO8601 文字列(TEXT)で保持する。

-- ウィジェットタイプ（コードレジストリと 1:1、設定のみ DB 保持） -------------
CREATE TABLE IF NOT EXISTS widget_types (
    id            INTEGER PRIMARY KEY,
    type_key      TEXT NOT NULL UNIQUE,
    label         TEXT NOT NULL DEFAULT '',
    description   TEXT NOT NULL DEFAULT '',
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

-- レイアウト（公開サイトの shell / pages / static の単位） --------------------
CREATE TABLE IF NOT EXISTS layouts (
    id         INTEGER PRIMARY KEY,
    key        TEXT    NOT NULL UNIQUE,
    name       TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- ページ（テンプレート / 固定ページ。本文は work/layouts/{key}/ 配下、DB にはメタ情報のみ） --
CREATE TABLE IF NOT EXISTS pages (
    id           INTEGER PRIMARY KEY,
    layout_id    INTEGER NOT NULL REFERENCES layouts(id) ON DELETE RESTRICT,
    name         TEXT    NOT NULL DEFAULT '',
    url_path     TEXT    UNIQUE,
    file_name    TEXT    NOT NULL,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (layout_id, file_name)
);
CREATE INDEX IF NOT EXISTS idx_pages_published ON pages(is_published);
CREATE INDEX IF NOT EXISTS idx_pages_layout ON pages(layout_id);

-- 管理ユーザー -------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY,
    login         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name  TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'administrator',
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_users_login ON users(login);

-- DB 管理画面の列幅・ソート設定（CMS コアテーブル） -------------------------
CREATE TABLE IF NOT EXISTS user_table_meta (
    table_name          TEXT PRIMARY KEY NOT NULL,
    column_widths_json  TEXT NOT NULL DEFAULT '{}',
    sort_json           TEXT NOT NULL DEFAULT '[]',
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- 初期シードデータ ---------------------------------------------------------
INSERT INTO layouts (key, name)
VALUES ('default', '既定');

-- テンプレート（presets/index.html など）が参照する "news" プレースホルダー
INSERT INTO widget_types (type_key, label, description, config, html_template, config_schema)
VALUES (
    'news',
    'お知らせ',
    '公開済みの投稿を新しい順に表示します',
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
INSERT INTO widget_types (type_key, label, description, config, html_template, config_schema)
VALUES (
    'image',
    '画像',
    'メディアライブラリの画像を1枚表示します。幅・高さ・収め方・角丸と、エントリごとの回り込み・マージンを設定できます',
    '{"width":"100%","height":"auto","object_fit":"cover","border_radius":"0"}',
    '{% if has_item %}
<figure class="widget-image" style="{{ item.style }}">
  {% if item.link_url %}
  <a href="{{ item.link_url }}">
    <img src="{{ item.image_url }}" alt="{{ item.alt }}" style="{{ item.img_style }}">
  </a>
  {% else %}
  <img src="{{ item.image_url }}" alt="{{ item.alt }}" style="{{ item.img_style }}">
  {% endif %}
</figure>
{% endif %}',
    '{
  "fields": [
    {
      "key": "width",
      "label": "表示幅",
      "type": "text",
      "default": "100%",
      "help": "例: 100%, 600px"
    },
    {
      "key": "height",
      "label": "表示高さ",
      "type": "text",
      "default": "auto",
      "help": "例: auto, 320px, 40vh"
    },
    {
      "key": "object_fit",
      "label": "画像の収め方",
      "type": "text",
      "default": "cover",
      "help": "cover, contain, fill, none, scale-down"
    },
    {
      "key": "border_radius",
      "label": "角丸",
      "type": "text",
      "default": "0",
      "help": "例: 0, 8px, 50%"
    }
  ]
}'
);

-- 画像カルーセルウィジェットタイプ
INSERT INTO widget_types (type_key, label, description, config, html_template, config_schema)
VALUES (
    'carousel',
    '画像カルーセル',
    '複数の画像をスライドショー形式で表示します。各画像にリンクを設定可能',
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

-- お問い合わせフォームウィジェットタイプ
INSERT INTO widget_types (type_key, label, description, config, html_template, config_schema)
VALUES (
    'contact_form',
    'お問い合わせフォーム',
    '名前・メール・本文を受け付けるフォーム。二重送信防止（ボタン無効化 + PRG）付き',
    '{"heading":"お問い合わせ","submit_label":"送信する","success_message":"お問い合わせを受け付けました。担当者より折り返しご連絡いたします。","show_phone":false}',
    '{% if form.sent %}
<div class="contact-form-message contact-form-success" role="status">{{ config.success_message }}</div>
{% else %}
<div class="contact-form-widget">
  {% if form.error %}
  <div class="contact-form-message contact-form-error" role="alert">送信に失敗しました。入力内容を確認して再度お試しください。</div>
  {% endif %}
  {% if not form.token %}
  <div class="contact-form-message contact-form-error" role="alert">フォームを初期化できませんでした。ページを再読み込みしてください。</div>
  {% else %}
  <h2 class="contact-form-heading">{{ config.heading }}</h2>
  <form class="contact-form" method="post" action="{{ form.action }}" novalidate>
    <input type="hidden" name="token" value="{{ form.token }}">
    <div class="form-group">
      <label for="contact-name-{{ form.id }}">お名前 <span class="required" aria-hidden="true">*</span></label>
      <input type="text" id="contact-name-{{ form.id }}" name="name" required maxlength="100" autocomplete="name">
    </div>
    <div class="form-group">
      <label for="contact-email-{{ form.id }}">メールアドレス <span class="required" aria-hidden="true">*</span></label>
      <input type="email" id="contact-email-{{ form.id }}" name="email" required maxlength="254" autocomplete="email">
    </div>
    {% if config.show_phone %}
    <div class="form-group">
      <label for="contact-phone-{{ form.id }}">電話番号</label>
      <input type="tel" id="contact-phone-{{ form.id }}" name="phone" maxlength="30" autocomplete="tel">
    </div>
    {% endif %}
    <div class="form-group">
      <label for="contact-message-{{ form.id }}">お問い合わせ内容 <span class="required" aria-hidden="true">*</span></label>
      <textarea id="contact-message-{{ form.id }}" name="message" required maxlength="5000" rows="6"></textarea>
    </div>
    <button type="submit" class="button contact-form-submit">{{ config.submit_label }}</button>
  </form>
  {% endif %}
</div>
<style>
.contact-form-widget { max-width: 560px; }
.contact-form-heading { margin: 0 0 1.25rem; font-size: 1.5rem; }
.contact-form .form-group { margin-bottom: 1rem; }
.contact-form label { display: block; margin-bottom: 0.35rem; font-weight: 600; }
.contact-form .required { color: #b32d2e; }
.contact-form input[type="text"],
.contact-form input[type="email"],
.contact-form input[type="tel"],
.contact-form textarea {
  width: 100%; padding: 0.6rem 0.75rem; border: 1px solid #cbd5e1; border-radius: 6px; font: inherit;
}
.contact-form-submit { margin-top: 0.5rem; }
.contact-form-submit:disabled { opacity: 0.65; cursor: not-allowed; }
.contact-form-message { padding: 1rem 1.25rem; border-radius: 8px; margin: 1rem 0; }
.contact-form-success { background: #e8f5e9; border: 1px solid #81c784; color: #2e7d32; }
.contact-form-error { background: #fff5f5; border: 1px solid #f5c6cb; color: #b32d2e; }
</style>
<script>
(function() {
  var root = document.currentScript.previousElementSibling;
  if (!root || !root.classList.contains(''contact-form-widget'')) {
    root = document.currentScript.parentElement.querySelector(''.contact-form-widget'');
  }
  if (!root) return;
  var form = root.querySelector(''form.contact-form'');
  if (!form) return;
  var btn = form.querySelector(''.contact-form-submit'');
  form.addEventListener(''submit'', function(e) {
    if (form.dataset.submitting === ''1'') {
      e.preventDefault();
      return;
    }
    form.dataset.submitting = ''1'';
    if (btn) {
      btn.disabled = true;
      btn.textContent = ''送信中…'';
    }
  });
})();
</script>
{% endif %}',
    '{
  "fields": [
    {
      "key": "heading",
      "label": "見出し",
      "type": "text",
      "default": "お問い合わせ",
      "help": "フォーム上部に表示する見出し"
    },
    {
      "key": "submit_label",
      "label": "送信ボタン文言",
      "type": "text",
      "default": "送信する"
    },
    {
      "key": "success_message",
      "label": "送信完了メッセージ",
      "type": "text",
      "default": "お問い合わせを受け付けました。担当者より折り返しご連絡いたします。"
    },
    {
      "key": "show_phone",
      "label": "電話番号フィールドを表示",
      "type": "boolean",
      "default": false
    }
  ]
}'
);
