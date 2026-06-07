(function () {
  'use strict';

  function escapeHtml(text) {
    return String(text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function normalizeUrlPath(raw) {
    const trimmed = raw.trim();
    if (!trimmed) return '';
    let path = trimmed;
    if (!path.startsWith('/')) path = '/' + path;
    if (path.length > 1) path = path.replace(/\/+$/, '');
    return path;
  }

  function isReservedPath(path) {
    return path === '/'
      || path === '/admin'
      || path.startsWith('/admin/')
      || path === '/static'
      || path.startsWith('/static/');
  }

  function validatePageUrlForm(form) {
    const published = form.querySelector('#is_published');
    const urlInput = form.querySelector('#url_path');
    const raw = urlInput?.value ?? '';
    const path = normalizeUrlPath(raw);

    if (published?.checked && !raw.trim()) {
      alert('公開するには URL を指定してください。');
      urlInput?.focus();
      return false;
    }

    if (path && isReservedPath(path)) {
      alert('URL「' + path + '」はシステムで予約されているため使用できません。');
      urlInput?.focus();
      return false;
    }

    return true;
  }

  function initConfirmForms() {
    document.addEventListener('submit', function (event) {
      const form = event.target;
      if (!(form instanceof HTMLFormElement)) return;

      if (form.dataset.validate === 'page-url' && !validatePageUrlForm(form)) {
        event.preventDefault();
        return;
      }

      const message = form.dataset.confirm;
      if (message && !confirm(message)) {
        event.preventDefault();
      }
    });
  }

  function initCopyButtons() {
    document.querySelectorAll('[data-copy-target]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const selector = btn.dataset.copyTarget;
        const input = selector ? document.querySelector(selector) : null;
        if (input) {
          navigator.clipboard.writeText(input.value);
        }
      });
    });
  }

  function initTemplateRepeater() {
    const rowsEl = document.getElementById('column-rows');
    const template = document.getElementById('column-row-template');
    const addBtn = document.getElementById('add-column-btn');
    if (!rowsEl || !template || !addBtn) return;

    function bindRow(row) {
      const removeBtn = row.querySelector('.column-remove-btn');
      removeBtn.addEventListener('click', function () {
        row.remove();
      });
    }

    function addRow() {
      const fragment = template.content.cloneNode(true);
      rowsEl.appendChild(fragment);
      bindRow(rowsEl.lastElementChild);
    }

    rowsEl.querySelectorAll('.column-row').forEach(bindRow);
    addBtn.addEventListener('click', addRow);
  }

  function initSeedForm() {
    const form = document.getElementById('seed-form');
    if (!form) return;

    function syncRow(row) {
      const typeKey = row.dataset.typeKey;
      row.querySelectorAll('.seed-param-group').forEach(function (group) {
        const active = group.dataset.type === typeKey;
        group.classList.toggle('active', active);
        group.querySelectorAll('input, select').forEach(function (input) {
          input.disabled = !active;
          input.required = active && input.type !== 'checkbox';
        });
      });
    }

    document.querySelectorAll('.seed-row').forEach(syncRow);

    form.addEventListener('submit', function (event) {
      document.querySelectorAll('.seed-row').forEach(function (row) {
        const checkbox = row.querySelector('.null-checkbox');
        const hidden = row.querySelector('.null-value');
        if (checkbox && hidden) {
          hidden.value = checkbox.checked ? '1' : '0';
        }
      });

      const count = document.getElementById('count').value;
      if (!confirm(count + ' 件のテストデータを生成します。続行しますか？')) {
        event.preventDefault();
      }
    });
  }

  function initMediaPicker() {
    const hidden = document.getElementById('favicon_media_id');
    const preview = document.getElementById('favicon-preview');
    const openBtn = document.getElementById('favicon-open-btn');
    const clearBtn = document.getElementById('favicon-clear-btn');
    const dialog = document.getElementById('favicon-dialog');
    const closeBtn = document.getElementById('favicon-dialog-close');
    const grid = document.getElementById('favicon-picker-grid');
    if (!hidden || !preview || !openBtn || !clearBtn || !dialog || !closeBtn || !grid) return;

    function updateClearButton() {
      clearBtn.disabled = !hidden.value;
    }

    function highlightSelected() {
      const current = hidden.value;
      grid.querySelectorAll('.media-picker-item').forEach(function (btn) {
        btn.classList.toggle('is-selected', btn.dataset.mediaId === current);
      });
    }

    function setPreview(id, title, url, showPreview) {
      hidden.value = id;
      if (!id) {
        preview.innerHTML = '<span class="favicon-picker-empty">未設定</span>';
      } else {
        const img = showPreview
          ? '<img src="' + escapeHtml(url) + '" alt="">'
          : '';
        preview.innerHTML =
          img +
          '<div class="meta"><strong>' + escapeHtml(title) + '</strong>' +
          '<span>ID ' + escapeHtml(id) + '</span></div>';
      }
      updateClearButton();
      highlightSelected();
    }

    openBtn.addEventListener('click', function () {
      highlightSelected();
      dialog.showModal();
    });

    closeBtn.addEventListener('click', function () {
      dialog.close();
    });

    dialog.addEventListener('click', function (e) {
      if (e.target === dialog) dialog.close();
    });

    grid.addEventListener('click', function (e) {
      const btn = e.target.closest('.media-picker-item');
      if (!btn) return;
      setPreview(
        btn.dataset.mediaId,
        btn.dataset.title,
        btn.dataset.publicUrl,
        btn.dataset.showPreview === '1'
      );
      dialog.close();
    });

    clearBtn.addEventListener('click', function () {
      setPreview('', '', '', false);
    });
  }

  function initTableData() {
    const panel = document.getElementById('db-table-data-panel');
    if (!panel) return;

    const statusEl = document.getElementById('db-data-status');
    const statusTextEl = statusEl ? statusEl.querySelector('.db-data-status-text') : null;
    const countEl = document.querySelector('.data-count');
    const thead = document.getElementById('db-table-head');
    const tbody = document.getElementById('db-table-body');
    const emptyEl = document.getElementById('db-table-empty');
    const apiUrl = panel.dataset.apiUrl || '';
    const chunkSize = parseInt(panel.dataset.chunkSize || '1000', 10);
    const maxPrefetchChunks = parseInt(panel.dataset.maxPrefetchChunks || '10', 10);

    let generation = 0;
    let abortController = null;
    const cache = new Map();
    const inFlight = new Set();
    let columns = [];
    let totalCount = 0;
    let columnsRendered = false;

    function setStatus(state, text, showRetry) {
      if (!statusEl || !statusTextEl) return;
      statusEl.className = 'db-data-status';
      if (state) statusEl.classList.add('is-' + state);
      statusTextEl.textContent = text;
      const existingRetry = statusEl.querySelector('.db-data-status-retry');
      if (existingRetry) existingRetry.remove();
      if (showRetry) {
        const retryBtn = document.createElement('button');
        retryBtn.type = 'button';
        retryBtn.className = 'db-data-status-retry button secondary';
        retryBtn.textContent = '再試行';
        retryBtn.addEventListener('click', function () {
          load();
        });
        statusEl.appendChild(retryBtn);
      }
    }

    function updateCount(shown, total) {
      if (!countEl) return;
      countEl.textContent = '表示 ' + shown + ' / 全 ' + total + ' 件';
    }

    function renderTable() {
      if (!thead || !tbody) return;

      let rowCount = 0;
      const offsets = Array.from(cache.keys()).sort(function (a, b) {
        return a - b;
      });
      for (let i = 0; i < offsets.length; i++) {
        rowCount += cache.get(offsets[i]).length;
      }

      if (columns.length === 0 || rowCount === 0) {
        tbody.innerHTML = '';
        if (!columnsRendered) thead.innerHTML = '';
        if (emptyEl) emptyEl.hidden = columns.length > 0;
        if (columns.length > 0) updateCount(0, totalCount);
        return;
      }

      if (!columnsRendered) {
        thead.innerHTML =
          '<tr>' +
          columns
            .map(function (col) {
              return '<th><span class="text-mono">' + escapeHtml(col) + '</span></th>';
            })
            .join('') +
          '</tr>';
        columnsRendered = true;
      }

      let html = '';
      for (let i = 0; i < offsets.length; i++) {
        const rows = cache.get(offsets[i]);
        for (let j = 0; j < rows.length; j++) {
          html += '<tr>';
          for (let k = 0; k < rows[j].length; k++) {
            html += '<td class="text-mono-cell">' + escapeHtml(rows[j][k]) + '</td>';
          }
          html += '</tr>';
        }
      }
      tbody.innerHTML = html;
      if (emptyEl) emptyEl.hidden = true;
      updateCount(rowCount, totalCount);
    }

    async function fetchChunk(offset, gen) {
      if (cache.has(offset)) return cache.get(offset);
      if (inFlight.has(offset)) return null;

      inFlight.add(offset);
      try {
        const url = apiUrl + (apiUrl.indexOf('?') >= 0 ? '&' : '?') + 'offset=' + offset;
        const response = await fetch(url, {
          signal: abortController ? abortController.signal : undefined,
          credentials: 'same-origin',
        });

        if (gen !== generation) return null;

        if (!response.ok) {
          let message = '取得に失敗しました';
          try {
            const err = await response.json();
            if (err.error && err.error.message) message = err.error.message;
          } catch (e) {}
          throw new Error(message);
        }

        const data = await response.json();
        if (gen !== generation) return null;

        if (offset === 0) {
          columns = data.columns || [];
          totalCount = data.total_count || 0;
          columnsRendered = false;
        }

        cache.set(offset, data.rows || []);
        renderTable();
        return data;
      } finally {
        inFlight.delete(offset);
      }
    }

    async function load() {
      generation += 1;
      const gen = generation;
      if (abortController) abortController.abort();
      abortController = new AbortController();
      cache.clear();
      inFlight.clear();
      columns = [];
      totalCount = 0;
      columnsRendered = false;
      if (tbody) tbody.innerHTML = '';
      if (thead) thead.innerHTML = '';
      if (emptyEl) emptyEl.hidden = true;
      updateCount('—', '—');
      setStatus('loading', '読み込み中…', false);

      try {
        const first = await fetchChunk(0, gen);
        if (!first || gen !== generation) return;

        if (first.total_count === 0) {
          setStatus('empty', 'データがありません', false);
          renderTable();
          return;
        }

        const chunkSizeActual = first.chunk_size || chunkSize;
        const totalChunks = Math.ceil(first.total_count / chunkSizeActual);
        const prefetchChunks = Math.min(totalChunks, maxPrefetchChunks);

        for (let chunkIndex = 1; chunkIndex < prefetchChunks; chunkIndex += 1) {
          if (gen !== generation) return;
          const offset = chunkIndex * chunkSizeActual;
          const loadedRows = Array.from(cache.values()).reduce(function (sum, rows) {
            return sum + rows.length;
          }, 0);
          setStatus(
            'loading',
            '読み込み中…（' +
              loadedRows.toLocaleString() +
              ' / ' +
              first.total_count.toLocaleString() +
              ' 件・' +
              chunkIndex +
              ' / ' +
              prefetchChunks +
              ' チャンク）',
            false
          );
          await fetchChunk(offset, gen);
        }

        if (gen !== generation) return;

        const shownRows = Array.from(cache.values()).reduce(function (sum, rows) {
          return sum + rows.length;
        }, 0);
        let doneText;
        if (totalChunks > maxPrefetchChunks) {
          doneText =
            shownRows.toLocaleString() +
            ' 件を表示（全 ' +
            first.total_count.toLocaleString() +
            ' 件中・先頭のみ）';
        } else {
          doneText = shownRows.toLocaleString() + ' 件を表示（完了）';
        }
        setStatus('done', doneText, false);
      } catch (err) {
        if (err && err.name === 'AbortError') return;
        if (gen !== generation) return;
        setStatus('error', err.message || '取得に失敗しました', true);
      }
    }

    load();
  }

  function initWidgetConfig() {
    const schemaEl = document.getElementById('dynamic-config-fields');
    const hiddenConfig = document.getElementById('config');
    const rawTextarea = document.getElementById('config-raw');
    const widgetTypeSelect = document.getElementById('widget_type_id');
    if (!schemaEl || !hiddenConfig) return;

    let currentConfig = {};
    try {
      currentConfig = JSON.parse(hiddenConfig.value || '{}');
    } catch (e) {
      currentConfig = {};
    }

    let currentSchema = window.__WIDGET_CONFIG_SCHEMA__ || {};

    function updateHiddenConfig() {
      const fields = (currentSchema && currentSchema.fields) || [];
      const newValues = {};
      fields.forEach(function (field) {
        const input = document.querySelector('input[name="dynamic_' + field.key + '"]');
        if (input) {
          let val = input.value;
          if (field.type === 'number' && val !== '') val = parseFloat(val);
          newValues[field.key] = val;
        }
      });
      hiddenConfig.value = JSON.stringify(newValues, null, 2);
      if (rawTextarea) rawTextarea.value = hiddenConfig.value;
    }

    function renderFields(schema, values) {
      schemaEl.innerHTML = '';
      const fields = (schema && schema.fields) || [];
      if (fields.length === 0) {
        const p = document.createElement('p');
        p.className = 'help';
        p.textContent = 'このウィジェットタイプにインスタンス固有の設定項目はありません。';
        schemaEl.appendChild(p);
        return;
      }

      fields.forEach(function (field) {
        const wrapper = document.createElement('div');
        wrapper.className = 'field';
        wrapper.style.marginBottom = '10px';

        const label = document.createElement('label');
        label.textContent = field.label || field.key;
        label.style.display = 'block';
        label.style.marginBottom = '4px';
        label.style.fontSize = '13px';

        let input;
        if (field.type === 'number') {
          input = document.createElement('input');
          input.type = 'number';
          if (field.min !== undefined) input.min = field.min;
          if (field.max !== undefined) input.max = field.max;
          if (field.step !== undefined) input.step = field.step;
        } else {
          input = document.createElement('input');
          input.type = 'text';
        }

        input.name = 'dynamic_' + field.key;
        input.value = (values && values[field.key] !== undefined) ? values[field.key] : (field.default ?? '');
        input.style.width = '100%';
        input.style.fontFamily = 'ui-monospace, monospace';
        input.addEventListener('input', updateHiddenConfig);

        wrapper.appendChild(label);
        wrapper.appendChild(input);
        if (field.help) {
          const help = document.createElement('p');
          help.className = 'help';
          help.style.marginTop = '2px';
          help.style.fontSize = '11px';
          help.textContent = field.help;
          wrapper.appendChild(help);
        }
        schemaEl.appendChild(wrapper);
      });
    }

    if (rawTextarea) {
      rawTextarea.addEventListener('input', function () {
        hiddenConfig.value = rawTextarea.value;
        try {
          currentConfig = JSON.parse(rawTextarea.value || '{}');
          renderFields(currentSchema, currentConfig);
        } catch (e) {}
      });
    }

    renderFields(currentSchema, currentConfig);

    if (widgetTypeSelect) {
      widgetTypeSelect.addEventListener('change', function () {
        if (confirm('ウィジェットタイプを変更しました。設定フォームを最新のスキーマで更新するためページを再読み込みしますか？')) {
          location.reload();
        }
      });
    }

    const form = schemaEl.closest('form');
    if (form) {
      form.addEventListener('submit', function () {
        updateHiddenConfig();
      });
    }
  }

  function initPageModules() {
    initTemplateRepeater();
    initSeedForm();
    initMediaPicker();
    initTableData();
    initWidgetConfig();
  }

  window.Admin = {
    escapeHtml: escapeHtml,
    initConfirmForms: initConfirmForms,
    initCopyButtons: initCopyButtons,
    initPageModules: initPageModules,
  };

  document.addEventListener('DOMContentLoaded', function () {
    Admin.initConfirmForms();
    Admin.initCopyButtons();
    Admin.initPageModules();
  });
})();
