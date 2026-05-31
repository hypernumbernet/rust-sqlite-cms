-- html_template 内の変数名をプレースホルダー名に依存しないタイプ固定キーへ統一
-- news: items / has_items
-- image: item / has_item
-- carousel: carousel / has_carousel（タイプ名固定）

UPDATE widget_types
SET html_template = '{% if has_items %}
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
{% endif %}'
WHERE type_key = 'news';

UPDATE widget_types
SET html_template = '{% if has_item %}
<figure class="widget-image" style="{{ item.style }}">
  {% if item.link_url %}
  <a href="{{ item.link_url }}">
    <img src="{{ item.image_url }}" alt="{{ item.alt }}">
  </a>
  {% else %}
  <img src="{{ item.image_url }}" alt="{{ item.alt }}">
  {% endif %}
</figure>
{% endif %}'
WHERE type_key = 'image';

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

  track.style.transform = ''translateX(0)'';
  if (dots[0]) dots[0].classList.add(''active'');
  start();
})();
</script>
{% endif %}'
WHERE type_key = 'carousel';
