-- ウィジェット設計変更（2026）
-- 目的:
--   * widget_types に html_template（ウィジェットを構成するHTML/MiniJinja）を追加
--   * placeholders に config（インスタンスごとの設定JSON）を追加
-- これにより、ウィジェット画面は「HTML構成の編集」、インスタンス設定はプレースホルダー側で管理する形へ移行。

-- 1. widget_types 拡張
ALTER TABLE widget_types ADD COLUMN html_template TEXT NOT NULL DEFAULT '';

-- 2. placeholders 拡張（インスタンス設定の受け皿）
ALTER TABLE placeholders ADD COLUMN config TEXT NOT NULL DEFAULT '{}';

-- 3. 既存レコードへのバックフィル（template_usage() の出力と完全一致させる）
--    ここで投入する値は src/widgets/mod.rs template_usage() の各ケースから正確に抽出。
--    プレースホルダー名として代表的な値（news / hero / carousel）を使用。

-- news ウィジェット（.news-list の内側に入れる前提の軽量版）
UPDATE widget_types
SET html_template = '{% if has_news %}
  {% for item in news %}
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
{% endif %}'
WHERE type_key = 'news';

-- image ウィジェット（デフォルト名 hero として）
UPDATE widget_types
SET html_template = '{% if has_hero %}
<figure class="widget-image" style="{{ hero.style }}">
  {% if hero.link_url %}
  <a href="{{ hero.link_url }}">
    <img src="{{ hero.image_url }}" alt="{{ hero.alt }}">
  </a>
  {% else %}
  <img src="{{ hero.image_url }}" alt="{{ hero.alt }}">
  {% endif %}
</figure>
{% endif %}'
WHERE type_key = 'image';

-- carousel ウィジェット（完全なセルフコンテインド実装）
-- 注意: この文字列リテラル内のすべての単一引用符(')はSQL用に2つ('')にエスケープされている。
UPDATE widget_types
SET html_template = '{% if has_carousel %}
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

  // init
  track.style.transform = ''translateX(0)'';
  if (dots[0]) dots[0].classList.add(''active'');
  start();
})();
</script>
{% endif %}'
WHERE type_key = 'carousel';

-- 注意:
-- * バックフィル値は 2026年時点の template_usage() 出力と一致させること。
-- * 将来新しいウィジェットタイプを追加する場合は、INSERT 時に html_template も明示的に設定する。
-- * 既存の config カラムは「タイプデフォルト」として残している（インスタンス側でオーバーライド可能にする）。