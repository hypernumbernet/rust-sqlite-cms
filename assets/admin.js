(function () {
  'use strict';

  function escapeHtml(text) {
    return String(text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function isCellNull(value) {
    return value === null || value === undefined;
  }

  function formatCellDisplay(text, editable) {
    let className = 'text-mono-cell';
    if (editable) {
      className += ' db-cell-editable';
    }
    let attrs = ' class="' + className + '"';
    if (editable) {
      attrs += ' tabindex="0" role="button"';
    }
    const content = isCellNull(text)
      ? '<span class="db-cell-null">null</span>'
      : escapeHtml(String(text));
    return '<td' + attrs + '>' + content + '</td>';
  }

  const DEFAULT_TIMESTAMP_OFFSET = '+09:00';
  const TIMESTAMP_WITH_OFFSET_RE =
    /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2})(?::\d{2})?(Z|[+-]\d{2}:?\d{2})$/i;
  const DATETIME_LOCAL_PREFIX_RE = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2})/;
  const OFFSET_HHMM_RE = /^[+-]\d{2}:\d{2}$/;
  const OFFSET_HHMM_COMPACT_RE = /^[+-]\d{4}$/;
  const OFFSET_HH_RE = /^[+-]\d{2}$/;

  function normalizeTimestampOffset(offset) {
    let normalized = String(offset || DEFAULT_TIMESTAMP_OFFSET).trim();
    if (!normalized) return DEFAULT_TIMESTAMP_OFFSET;
    if (normalized === 'Z' || normalized === 'z') return '+00:00';
    if (OFFSET_HHMM_RE.test(normalized)) return normalized;
    if (OFFSET_HHMM_COMPACT_RE.test(normalized)) {
      return normalized.slice(0, 3) + ':' + normalized.slice(3);
    }
    if (OFFSET_HH_RE.test(normalized)) {
      return normalized + ':00';
    }
    return DEFAULT_TIMESTAMP_OFFSET;
  }

  function parseTimestampWithOffset(value) {
    if (!value) {
      return { datetime: '', offset: DEFAULT_TIMESTAMP_OFFSET };
    }
    const normalized = String(value).trim().replace(' ', 'T');
    const withOffset = normalized.match(TIMESTAMP_WITH_OFFSET_RE);
    if (withOffset) {
      let offset = withOffset[2];
      if (offset === 'Z' || offset === 'z') offset = '+00:00';
      return {
        datetime: withOffset[1],
        offset: normalizeTimestampOffset(offset),
      };
    }
    const withoutOffset = normalized.match(DATETIME_LOCAL_PREFIX_RE);
    return {
      datetime: withoutOffset ? withoutOffset[1] : normalized.slice(0, 16),
      offset: DEFAULT_TIMESTAMP_OFFSET,
    };
  }

  function formatTimestampWithOffset(datetimeLocal, offset) {
    const datetime = String(datetimeLocal || '').trim();
    if (!datetime) return '';
    const match = datetime.match(DATETIME_LOCAL_PREFIX_RE);
    if (!match) return '';
    return match[1] + ':00' + normalizeTimestampOffset(offset);
  }

  function dbColumnTypeLabel(typeKey) {
    const labels = {
      integer: '整数',
      real: '実数',
      text: '文字列',
      blob: 'バイナリ',
      timestamp: '日時',
      boolean: '真偽値',
    };
    return labels[typeKey] || '不明';
  }

  // 仮想スクロールのスペーサー高（および scrollTop の到達上限）をこの値でキャップ
  // する。ブラウザは「最大要素高」（Chrome 約3,355万px / Firefox 約1,789万px）より
  // 手前で、大きな y オフセット位置の罫線・背景の描画を打ち切る（実測で約5〜6Mpx
  // 付近から行間罫線が消える）。この実描画限界を大きく下回る値にすることで、全行で
  // 罫線が確実に描画され、scrollTop の暴走も起きない。これを超える総高はスクロール
  // 位置を行インデックスへ比例マッピングして表示する。
  const SAFE_MAX_SCROLL_HEIGHT = 1500000;

  const DB_COL_WIDTH = {
    MIN: 40,
    MAX: 2000,
    AUTO_MAX: 500,
    // リサイズ幅(12) + ソート位置(right:12)・幅(14) に合わせたヘッダー右端の操作領域
    HEADER_CHROME: 26,
  };

  function isPrimaryPointerButton(e) {
    return e.button === 0;
  }

  function dbHeaderPkIconHtml(isPrimaryKey) {
    return isPrimaryKey
      ? '<span class="db-col-pk-icon" aria-hidden="true">🔑</span>'
      : '';
  }

  function dbHeaderLabelHtml(columnName, isPrimaryKey) {
    return (
      dbHeaderPkIconHtml(isPrimaryKey) +
      '<span class="text-mono">' +
      escapeHtml(columnName) +
      '</span>'
    );
  }

  function dbHeaderMeasureRowHtml(columnName, isPrimaryKey) {
    return (
      '<tr><th>' +
      dbHeaderLabelHtml(columnName, isPrimaryKey) +
      '<span class="db-col-sort-trigger" aria-hidden="true"><span class="db-col-sort-icon"></span></span>' +
      '<span class="db-col-resize-handle" aria-hidden="true"></span></th></tr>'
    );
  }

  function clampDbColumnWidth(width, maxWidth) {
    const max = maxWidth == null ? DB_COL_WIDTH.MAX : maxWidth;
    return Math.max(DB_COL_WIDTH.MIN, Math.min(max, Math.round(width)));
  }

  function dbHorizontalBoxExtra(cell) {
    const style = getComputedStyle(cell);
    return (
      parseFloat(style.paddingLeft) +
      parseFloat(style.paddingRight) +
      parseFloat(style.borderLeftWidth) +
      parseFloat(style.borderRightWidth)
    );
  }

  /** 非表示テーブル上でセル全体幅（padding・border・ヘッダーハンドル込み）を計測する。 */
  function dbTableCellWidth(panel, text, isHeader, isPrimaryKey) {
    const tableClass = isHeader ? 'db-table-head-table' : 'db-table-body-table';
    const rowHtml = isHeader
      ? dbHeaderMeasureRowHtml(String(text), isPrimaryKey)
      : '<tr><td class="text-mono-cell">' +
        escapeHtml(String(text)) +
        '</td></tr>';

    const wrapper = document.createElement('div');
    wrapper.className = 'db-table-measure-root';
    wrapper.innerHTML =
      '<table class="' +
      tableClass +
      ' db-table-measure-table">' +
      (isHeader ? '<thead>' : '<tbody>') +
      rowHtml +
      (isHeader ? '</thead>' : '</tbody>') +
      '</table>';
    panel.appendChild(wrapper);

    const cell = wrapper.querySelector(isHeader ? 'th' : 'td');
    if (!cell) {
      wrapper.remove();
      return 0;
    }

    const inner = cell.querySelector('.text-mono, .text-mono-cell');
    const contentWidth = inner ? inner.scrollWidth : cell.scrollWidth;
    const width = Math.ceil(
      contentWidth +
        dbHorizontalBoxExtra(cell) +
        (isHeader ? DB_COL_WIDTH.HEADER_CHROME : 0)
    );
    wrapper.remove();
    return width;
  }

  function dbHeaderCellHtml(columnName, sortEntry, sortPriority, showPriority, isPrimaryKey) {
    let thClass = '';
    let sortIndicatorHtml = '';
    let triggerClass = 'db-col-sort-trigger';
    let ariaSort = 'none';

    if (sortEntry) {
      thClass =
        sortEntry.direction === 'asc' ? ' is-sorted-asc' : ' is-sorted-desc';
      ariaSort = sortEntry.direction === 'asc' ? 'ascending' : 'descending';
      triggerClass += ' has-sort-state';
      sortIndicatorHtml =
        '<span class="db-col-sort-arrow" aria-hidden="true">' +
        (sortEntry.direction === 'asc' ? '▲' : '▼') +
        '</span>';
      if (showPriority && sortPriority > 0) {
        sortIndicatorHtml +=
          '<span class="db-col-sort-priority">' + sortPriority + '</span>';
      }
    }

    return (
      '<th class="' +
      thClass.trim() +
      '" aria-sort="' +
      ariaSort +
      '">' +
      dbHeaderLabelHtml(columnName, isPrimaryKey) +
      '<span class="' +
      triggerClass +
      '" role="button" tabindex="0" aria-haspopup="menu" aria-expanded="false" aria-label="' +
      escapeHtml(columnName) +
      ' のソート">' +
      sortIndicatorHtml +
      '<span class="db-col-sort-icon" aria-hidden="true"></span></span><span class="db-col-resize-handle" role="separator" aria-orientation="vertical" aria-label="' +
      escapeHtml(columnName) +
      ' 列幅変更"></span></th>'
    );
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

  function formatElapsedMs(ms) {
    if (typeof ms !== 'number' || ms < 0 || !Number.isFinite(ms)) return '';
    if (ms < 1000) {
      return ms + ' ミリ秒';
    }
    const seconds = ms / 1000;
    if (seconds < 60) {
      const text = seconds >= 10 ? seconds.toFixed(1) : seconds.toFixed(2);
      return text.replace(/\.?0+$/, '') + ' 秒';
    }
    const minutes = Math.floor(seconds / 60);
    const rem = Math.round(seconds % 60);
    return rem > 0 ? minutes + ' 分 ' + rem + ' 秒' : minutes + ' 分';
  }

  function parseSeedSseChunk(buffer, onEvent) {
    const lines = buffer.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
    let eventName = 'message';
    let dataLines = [];

    function flushEvent() {
      if (dataLines.length === 0) return;
      const payload = dataLines.join('\n');
      dataLines = [];
      try {
        onEvent(eventName, JSON.parse(payload));
      } catch (err) {
        onEvent('error', { message: '進捗データの解析に失敗しました' });
      }
      eventName = 'message';
    }

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (line === '') {
        flushEvent();
        continue;
      }
      if (line.indexOf('event:') === 0) {
        eventName = line.slice(6).trim();
      } else if (line.indexOf('data:') === 0) {
        dataLines.push(line.slice(5).trim());
      }
    }

    flushEvent();
  }

  function initSeedForm() {
    const form = document.getElementById('seed-form');
    if (!form) return;

    const progressEl = document.getElementById('seed-progress');
    const progressTextEl = progressEl ? progressEl.querySelector('.seed-progress-text') : null;
    const progressBarEl = document.getElementById('seed-progress-bar');
    const submitBtn = form.querySelector('button[type="submit"]');
    const streamUrl = form.getAttribute('action') || '';
    let seedAbortController = null;
    let seedRunId = 0;

    function syncRow(row) {
      const typeKey = row.dataset.typeKey;
      row.querySelectorAll('.seed-param-group').forEach(function (group) {
        const active = group.dataset.type === typeKey;
        group.classList.toggle('active', active);
        group.querySelectorAll('input, select').forEach(function (input) {
          input.required = active && input.type !== 'checkbox';
        });
      });
    }

    function setFormDisabled(disabled) {
      form.querySelectorAll('input, select, button, textarea').forEach(function (input) {
        input.disabled = disabled;
      });
      if (!disabled) {
        document.querySelectorAll('.seed-row').forEach(syncRow);
      }
    }

    function showProgress() {
      if (!progressEl) return;
      progressEl.hidden = false;
      progressEl.className = 'db-data-status is-loading';
      if (progressTextEl) progressTextEl.textContent = '準備中…';
      if (progressBarEl) {
        progressBarEl.value = 0;
        progressBarEl.max = 100;
      }
    }

    function updateProgress(done, total) {
      if (!progressEl) return;
      progressEl.className = 'db-data-status is-loading';
      const percent = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
      if (progressTextEl) {
        progressTextEl.textContent =
          done.toLocaleString() + ' / ' + total.toLocaleString() + ' 件を生成中…';
      }
      if (progressBarEl) {
        progressBarEl.max = 100;
        progressBarEl.value = percent;
      }
    }

    function showProgressError(message) {
      if (!progressEl) return;
      progressEl.className = 'db-data-status is-error';
      if (progressTextEl) progressTextEl.textContent = message;
      if (progressBarEl) progressBarEl.value = 0;
    }

    function hideProgress() {
      if (!progressEl) return;
      progressEl.hidden = true;
      progressEl.className = 'db-data-status';
    }

    function resetFormState() {
      hideProgress();
      setFormDisabled(false);
      if (submitBtn) submitBtn.disabled = false;
    }

    function buildUrlEncodedFormBody(targetForm) {
      const params = new URLSearchParams();
      targetForm.querySelectorAll('input, select, textarea').forEach(function (el) {
        if (!el.name || el.disabled) return;
        if (el.type === 'checkbox') {
          if (el.checked) params.append(el.name, el.value || 'on');
          return;
        }
        if (el.type === 'radio') {
          if (el.checked) params.append(el.name, el.value);
          return;
        }
        if (el.type === 'file') return;
        params.append(el.name, el.value);
      });
      return params.toString();
    }

    async function runSeedStream() {
      if (seedAbortController) {
        seedAbortController.abort();
      }

      const runId = ++seedRunId;
      const abortController = new AbortController();
      seedAbortController = abortController;
      const requestBody = buildUrlEncodedFormBody(form);

      showProgress();
      setFormDisabled(true);
      const seedStartedAt = performance.now();

      let pending = '';
      let finished = false;

      function handleSeedEvent(eventName, payload) {
        if (eventName === 'progress') {
          updateProgress(payload.done || 0, payload.total || 0);
        } else if (eventName === 'done') {
          finished = true;
          const elapsedMs =
            typeof payload.elapsed_ms === 'number'
              ? payload.elapsed_ms
              : Math.round(performance.now() - seedStartedAt);
          const elapsedText = formatElapsedMs(elapsedMs);
          if (progressEl) progressEl.className = 'db-data-status is-done';
          if (progressTextEl) {
            progressTextEl.textContent =
              (payload.count || 0).toLocaleString() +
              ' 件の生成が完了しました' +
              (elapsedText ? '（' + elapsedText + '）' : '') +
              '。';
          }
          if (progressBarEl) progressBarEl.value = 100;
          setFormDisabled(false);
        } else if (eventName === 'error') {
          finished = true;
          showProgressError(payload.message || '生成に失敗しました');
          setFormDisabled(false);
        }
      }

      try {
        const response = await fetch(streamUrl, {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8' },
          body: requestBody,
          credentials: 'same-origin',
          signal: abortController.signal,
        });

        if (!response.ok || !response.body) {
          throw new Error('生成リクエストに失敗しました');
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();

        while (true) {
          const chunk = await reader.read();
          if (chunk.done) break;
          pending += decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, '\n');
          const parts = pending.split('\n\n');
          pending = parts.pop() || '';
          for (let i = 0; i < parts.length; i++) {
            parseSeedSseChunk(parts[i], handleSeedEvent);
          }
        }

        if (pending.trim()) {
          parseSeedSseChunk(pending, handleSeedEvent);
        }

        if (!finished) {
          throw new Error('生成が完了する前に接続が終了しました');
        }
      } catch (err) {
        if (abortController.signal.aborted || (err && err.name === 'AbortError')) {
          return;
        }
        if (!finished && runId === seedRunId) {
          showProgressError(err && err.message ? err.message : '生成に失敗しました');
          setFormDisabled(false);
        }
      } finally {
        if (runId === seedRunId) {
          seedAbortController = null;
          if (!abortController.signal.aborted && submitBtn) {
            submitBtn.disabled = false;
          }
        }
      }
    }

    resetFormState();

    window.addEventListener('pagehide', function () {
      if (seedAbortController) {
        seedAbortController.abort();
      }
    });

    window.addEventListener('pageshow', function (event) {
      if (event.persisted) {
        resetFormState();
      }
    });

    document.querySelectorAll('.seed-row').forEach(syncRow);

    form.addEventListener('submit', function (event) {
      event.preventDefault();

      document.querySelectorAll('.seed-row').forEach(function (row) {
        const checkbox = row.querySelector('.null-checkbox');
        const hidden = row.querySelector('.null-value');
        if (checkbox && hidden) {
          hidden.value = checkbox.checked ? '1' : '0';
        }
      });

      const count = document.getElementById('count').value;
      if (!confirm(count + ' 件のテストデータを生成します。続行しますか？')) {
        return;
      }

      runSeedStream();
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

    const scrollEl = document.getElementById('db-table-scroll');
    const headerEl = panel.querySelector('.db-table-header');
    const headTable = panel.querySelector('.db-table-head-table');
    const bodyTable = panel.querySelector('.db-table-body-table');
    const statusEl = document.getElementById('db-data-status');
    const statusTextEl = statusEl ? statusEl.querySelector('.db-data-status-text') : null;
    const countEl = document.getElementById('db-data-row-goto');
    const sortIndicatorEl = document.getElementById('db-data-sort-indicator');
    const sortIndicatorLabelEl = document.getElementById('db-data-sort-indicator-label');
    const sortClearBtn = document.getElementById('db-data-sort-clear');
    const rowGotoDialog = document.getElementById('db-row-goto-dialog');
    const rowGotoForm = document.getElementById('db-row-goto-form');
    const rowGotoInput = document.getElementById('db-row-goto-input');
    const rowGotoRange = document.getElementById('db-row-goto-range');
    const rowGotoCancel = document.getElementById('db-row-goto-cancel');
    const sortedNavConfirmDialog = document.getElementById('db-sorted-nav-confirm-dialog');
    const sortedNavConfirmMessageEl = document.getElementById('db-sorted-nav-confirm-message');
    const sortedNavConfirmOk = document.getElementById('db-sorted-nav-confirm-ok');
    const sortedNavConfirmCancel = document.getElementById('db-sorted-nav-confirm-cancel');
    const cellEditDialog = document.getElementById('db-cell-edit-dialog');
    const cellEditForm = document.getElementById('db-cell-edit-form');
    const cellEditColumnEl = document.getElementById('db-cell-edit-column');
    const cellEditTypeEl = document.getElementById('db-cell-edit-type');
    const cellEditInputWrap = document.getElementById('db-cell-edit-input-wrap');
    const cellEditNullWrap = document.getElementById('db-cell-edit-null-wrap');
    const cellEditNullInput = document.getElementById('db-cell-edit-null');
    const cellEditErrorEl = document.getElementById('db-cell-edit-error');
    const cellEditCancel = document.getElementById('db-cell-edit-cancel');
    const thead = document.getElementById('db-table-head');
    const tbody = document.getElementById('db-table-body');
    const emptyEl = document.getElementById('db-table-empty');
    const apiUrl = panel.dataset.apiUrl || '';
    const readOnly = panel.dataset.readOnly === 'true';
    function dataApiPath(suffix) {
      return apiUrl.replace(/\/rows$/, suffix);
    }
    const columnWidthsUrl = dataApiPath('/column-widths');
    const sortUrl = dataApiPath('/sort');
    const cellsUrl = dataApiPath('/cells');
    const chunkSize = parseInt(panel.dataset.chunkSize || '1000', 10);
    const overscan = parseInt(panel.dataset.overscan || '3', 10);
    const maxCachedChunks = parseInt(panel.dataset.maxCachedChunks || '1000', 10);
    const FETCH_CONCURRENCY = 1;
    const COLUMN_WIDTH_MIN = 40;
    const SORT_SLOW_ROW_THRESHOLD = 1000000;

    let generation = 0;
    let abortController = null;
    let prefetchAbortController = null;
    let prefetchTargetOffset = null;
    let wantPrefetch = false;
    const cache = new Map();
    const inFlight = new Map();
    let highQueue = [];
    const queuedOffsets = new Set();
    const pinnedHighOffsets = new Set();
    let activeFetches = 0;
    let lastPrefetchStartIndex = 0;
    let columns = [];
    let columnMeta = [];
    let columnMetaMap = new Map();
    let editableColumnFlags = [];
    let totalCount = 0;
    let columnsRendered = false;
    let chunkSizeActual = chunkSize;
    let startIndex = 0;
    let visibleCount = 0;
    let rowHeight = 0;
    let isSyncingScroll = false;
    let scrollRaf = 0;
    let renderRaf = 0;
    let renderPending = false;
    let needsRefresh = false;
    let wheelAccumPx = 0;
    let lastSyncedScrollTop = -1;
    let savedColumnWidths = null;
    let columnWidths = [];
    let activeResize = null;
    let sortStack = [];
    let sortMenuEl = null;
    let sortMenuColumn = null;
    let sortMenuAnchor = null;
    let sortedNavConfirmResolve = null;
    let sortedNavConfirmPending = false;
    let sortedDeepNavAcknowledged = false;
    let navSerial = Promise.resolve();
    let columnWidthsApplied = false;
    let pendingCellEdit = null;

    function rebuildColumnCaches() {
      columnMetaMap = new Map();
      for (let i = 0; i < columnMeta.length; i++) {
        columnMetaMap.set(columnMeta[i].name, columnMeta[i]);
      }
      editableColumnFlags = columns.map(function (name) {
        if (readOnly) return false;
        const meta = columnMetaMap.get(name);
        return meta ? !meta.pk : false;
      });
    }

    function columnMetaByName(name) {
      return columnMetaMap.get(name) || null;
    }

    function isPrimaryKeyColumn(name) {
      const meta = columnMetaByName(name);
      return meta ? !!meta.pk : false;
    }

    function primaryKeyValuesFromRow(row) {
      const keys = {};
      for (let i = 0; i < columnMeta.length; i++) {
        if (columnMeta[i].pk) {
          keys[columnMeta[i].name] = row[i];
        }
      }
      return keys;
    }

    function rowIndexFromDataRow(tr) {
      if (!tr || !tr.dataset) return -1;
      const rowIndex = parseInt(tr.dataset.rowIndex, 10);
      return Number.isFinite(rowIndex) ? rowIndex : -1;
    }

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

    function hasMissingVisibleChunks() {
      const offsets = chunkOffsetsForRange(startIndex, visibleCount);
      for (let i = 0; i < offsets.length; i++) {
        if (!cache.has(offsets[i])) return true;
      }
      return false;
    }

    function updateViewStatus() {
      if (totalCount === 0) {
        setStatus('empty', 'データがありません', false);
        return;
      }
      if (hasMissingVisibleChunks()) {
        setStatus('loading', '読み込み中…', false);
        return;
      }
      setStatus('done', totalCount.toLocaleString() + ' 件', false);
    }

    function updateCount(startRow) {
      if (!countEl) return;
      if (startRow === '—') {
        countEl.textContent = '行 —';
        countEl.disabled = true;
        return;
      }
      countEl.textContent = '行 ' + startRow;
      countEl.disabled = totalCount === 0 || rowHeight <= 0;
    }

    function currentRowNumber() {
      if (totalCount === 0 || rowHeight <= 0) return 1;
      return startIndex + 1;
    }

    function openRowGotoDialog() {
      if (!rowGotoDialog || !rowGotoInput || totalCount === 0 || rowHeight <= 0) return;
      if (rowGotoRange) {
        rowGotoRange.textContent = '(1 〜 ' + totalCount.toLocaleString() + ')';
      }
      rowGotoInput.min = '1';
      rowGotoInput.max = String(totalCount);
      rowGotoInput.value = String(currentRowNumber());
      rowGotoInput.setCustomValidity('');
      rowGotoDialog.showModal();
      rowGotoInput.select();
    }

    function closeRowGotoDialog() {
      if (rowGotoDialog) rowGotoDialog.close();
    }

    async function submitRowGoto(e) {
      e.preventDefault();
      if (!rowGotoInput) return;
      const rowNum = parseInt(rowGotoInput.value, 10);
      if (!Number.isFinite(rowNum) || rowNum < 1 || rowNum > totalCount) {
        rowGotoInput.setCustomValidity('1 〜 ' + totalCount + ' の整数を入力してください');
        rowGotoInput.reportValidity();
        return;
      }
      rowGotoInput.setCustomValidity('');
      const moved = await requestNavigateToStartIndex(Math.min(rowNum - 1, maxStartIndex()), {
        alwaysAsk: true,
      });
      if (moved) closeRowGotoDialog();
    }

    function clearCellEditError() {
      if (cellEditErrorEl) {
        cellEditErrorEl.hidden = true;
        cellEditErrorEl.textContent = '';
      }
    }

    function showCellEditError(message) {
      if (!cellEditErrorEl) return;
      cellEditErrorEl.hidden = false;
      cellEditErrorEl.textContent = message;
    }

    function configureCellEditValueInput(input, meta) {
      input.id = 'db-cell-edit-value';
      input.name = 'value';
      input.required = !meta.nullable;
    }

    function buildCellEditInput(meta, currentValue) {
      if (!cellEditInputWrap) return null;
      cellEditInputWrap.innerHTML = '';

      let input;
      let offsetInput = null;
      if (meta.type_key === 'blob') {
        input = document.createElement('textarea');
        input.rows = 3;
        input.placeholder = '16進数（例: 0x0102）';
      } else if (meta.type_key === 'boolean') {
        input = document.createElement('select');
        input.innerHTML =
          '<option value="0">0 (false)</option><option value="1">1 (true)</option>';
      } else if (meta.type_key === 'integer') {
        input = document.createElement('input');
        input.type = 'number';
        input.step = '1';
      } else if (meta.type_key === 'real') {
        input = document.createElement('input');
        input.type = 'number';
        input.step = 'any';
      } else if (meta.type_key === 'timestamp') {
        const parsed = parseTimestampWithOffset(currentValue);
        const fields = document.createElement('div');
        fields.className = 'db-cell-edit-timestamp-fields';

        input = document.createElement('input');
        input.type = 'datetime-local';
        input.className = 'db-cell-edit-timestamp-input';
        configureCellEditValueInput(input, meta);
        input.value = parsed.datetime;

        offsetInput = document.createElement('input');
        offsetInput.type = 'text';
        offsetInput.className = 'db-cell-edit-offset-input';
        offsetInput.id = 'db-cell-edit-offset';
        offsetInput.name = 'offset';
        offsetInput.value = parsed.offset;
        offsetInput.placeholder = DEFAULT_TIMESTAMP_OFFSET;
        offsetInput.inputMode = 'text';
        offsetInput.autocomplete = 'off';
        offsetInput.setAttribute('aria-label', 'UTCオフセット');

        fields.appendChild(input);
        fields.appendChild(offsetInput);
        cellEditInputWrap.appendChild(fields);
        return { valueInput: input, offsetInput: offsetInput };
      } else {
        input = document.createElement('input');
        input.type = 'text';
      }

      configureCellEditValueInput(input, meta);

      if (meta.type_key === 'boolean') {
        input.value = currentValue === '1' || currentValue === 'true' ? '1' : '0';
      } else {
        input.value = isCellNull(currentValue) ? '' : currentValue;
      }

      cellEditInputWrap.appendChild(input);
      return { valueInput: input, offsetInput: null };
    }

    function syncCellEditNullState(meta, editInputs) {
      if (!cellEditNullWrap || !cellEditNullInput) return;
      const valueInput = editInputs && editInputs.valueInput;
      const offsetInput = editInputs && editInputs.offsetInput;
      const disabled = meta.nullable && cellEditNullInput.checked;
      if (meta.nullable) {
        cellEditNullWrap.hidden = false;
      } else {
        cellEditNullWrap.hidden = true;
        cellEditNullInput.checked = false;
      }
      if (valueInput) valueInput.disabled = disabled;
      if (offsetInput) offsetInput.disabled = disabled;
    }

    function openCellEditDialog(rowIndex, columnIndex) {
      if (readOnly || !cellEditDialog || columnIndex < 0 || columnIndex >= columns.length) return;
      const columnName = columns[columnIndex];
      const meta = columnMetaByName(columnName);
      if (!meta || meta.pk) return;

      const row = getRow(rowIndex);
      if (!row) return;

      pendingCellEdit = {
        rowIndex: rowIndex,
        columnIndex: columnIndex,
        columnName: columnName,
        meta: meta,
        keys: primaryKeyValuesFromRow(row),
        currentValue: row[columnIndex],
      };

      clearCellEditError();
      if (cellEditColumnEl) cellEditColumnEl.textContent = columnName;
      if (cellEditTypeEl) cellEditTypeEl.textContent = dbColumnTypeLabel(meta.type_key);

      const editInputs = buildCellEditInput(meta, pendingCellEdit.currentValue);
      pendingCellEdit.editInputs = editInputs;
      if (cellEditNullInput) {
        cellEditNullInput.checked = meta.nullable && isCellNull(pendingCellEdit.currentValue);
        cellEditNullInput.onchange = function () {
          syncCellEditNullState(meta, editInputs);
        };
      }
      syncCellEditNullState(meta, editInputs);

      cellEditDialog.showModal();
      const valueInput = editInputs && editInputs.valueInput;
      if (valueInput && !valueInput.disabled) {
        valueInput.focus();
        if (typeof valueInput.select === 'function') valueInput.select();
      }
    }

    function closeCellEditDialog() {
      if (cellEditDialog) cellEditDialog.close();
      pendingCellEdit = null;
      clearCellEditError();
    }

    function updateCachedCellValue(rowIndex, columnIndex, value) {
      const chunkOffset = chunkOffsetForIndex(rowIndex);
      const chunk = cache.get(chunkOffset);
      if (!chunk) return;
      const localIndex = rowIndex - chunkOffset;
      if (!chunk[localIndex]) return;
      chunk[localIndex][columnIndex] = value;
    }

    async function submitCellEdit(e) {
      e.preventDefault();
      if (!pendingCellEdit) return;

      clearCellEditError();
      const meta = pendingCellEdit.meta;
      const editInputs = pendingCellEdit.editInputs;
      const valueInput = editInputs && editInputs.valueInput;
      const useNull = !!(cellEditNullInput && cellEditNullInput.checked);
      let value = valueInput ? valueInput.value : '';

      if (meta.type_key === 'timestamp' && !useNull) {
        const offsetInput = editInputs && editInputs.offsetInput;
        const offset = offsetInput ? offsetInput.value : DEFAULT_TIMESTAMP_OFFSET;
        value = formatTimestampWithOffset(value, offset);
        if (!value) {
          showCellEditError('日時を入力してください');
          return;
        }
      }

      try {
        const response = await fetch(cellsUrl, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          credentials: 'same-origin',
          body: JSON.stringify({
            column: pendingCellEdit.columnName,
            value: value,
            null: useNull,
            keys: pendingCellEdit.keys,
          }),
        });

        if (!response.ok) {
          let message = '保存に失敗しました';
          try {
            const err = await response.json();
            if (err.error && err.error.message) message = err.error.message;
          } catch (parseErr) {}
          showCellEditError(message);
          return;
        }

        const data = await response.json();
        let newValue;
        if (data && Object.prototype.hasOwnProperty.call(data, 'value')) {
          newValue = isCellNull(data.value) ? null : String(data.value);
        } else {
          newValue = useNull ? null : value;
        }
        updateCachedCellValue(
          pendingCellEdit.rowIndex,
          pendingCellEdit.columnIndex,
          newValue
        );
        closeCellEditDialog();
        renderVisibleRows();
      } catch (err) {
        showCellEditError('保存に失敗しました');
      }
    }

    function handleCellEditActivation(cell) {
      if (!cell || !cell.classList.contains('db-cell-editable')) return;
      const tr = cell.closest('tr');
      if (!tr) return;
      const columnIndex = columnIndexFromCell(cell);
      const rowIndex = rowIndexFromDataRow(tr);
      if (columnIndex < 0 || rowIndex < 0) return;
      openCellEditDialog(rowIndex, columnIndex);
    }

    function chunkOffsetForIndex(rowIndex) {
      return Math.floor(rowIndex / chunkSizeActual) * chunkSizeActual;
    }

    function columnIndexFromCell(cell) {
      if (!cell || !cell.parentNode) return -1;
      return Array.prototype.indexOf.call(cell.parentNode.children, cell);
    }

    function cancelActiveColumnResize() {
      if (activeResize) onResizeMouseUp();
    }

    function sortIndexMap() {
      const map = new Map();
      for (let i = 0; i < sortStack.length; i++) {
        map.set(sortStack[i].column, { entry: sortStack[i], index: i });
      }
      return map;
    }

    function sortEntryForColumn(column) {
      for (let i = 0; i < sortStack.length; i++) {
        if (sortStack[i].column === column) {
          return { entry: sortStack[i], index: i };
        }
      }
      return null;
    }

    function sortQueryString() {
      if (!sortStack.length) return '';
      const encoded = sortStack
        .map(function (entry) {
          return encodeURIComponent(entry.column) + ':' + entry.direction;
        })
        .join(',');
      return '&sort=' + encoded;
    }

    function invalidateHeader() {
      columnsRendered = false;
    }

    function updateSortIndicator() {
      if (!sortIndicatorEl || !sortIndicatorLabelEl) return;
      if (sortStack.length === 0) {
        sortIndicatorEl.hidden = true;
        sortIndicatorLabelEl.textContent = '';
        return;
      }
      sortIndicatorLabelEl.textContent = sortStack
        .map(function (entry) {
          const arrow = entry.direction === 'asc' ? '▲' : '▼';
          return entry.column + ' ' + arrow;
        })
        .join(', ');
      sortIndicatorEl.hidden = false;
    }

    function ensureSortMenu() {
      if (sortMenuEl) return sortMenuEl;

      sortMenuEl = document.createElement('div');
      sortMenuEl.className = 'db-col-sort-menu';
      sortMenuEl.hidden = true;
      sortMenuEl.setAttribute('role', 'menu');
      sortMenuEl.innerHTML =
        '<button type="button" class="db-col-sort-menu-item" data-action="asc" role="menuitem">昇順</button>' +
        '<button type="button" class="db-col-sort-menu-item" data-action="desc" role="menuitem">降順</button>' +
        '<button type="button" class="db-col-sort-menu-item" data-action="clear" role="menuitem">この列のソートを解除</button>';
      document.body.appendChild(sortMenuEl);

      sortMenuEl.addEventListener('click', function (e) {
        const item = e.target.closest('.db-col-sort-menu-item');
        if (!item || item.disabled) return;
        applySortAction(sortMenuColumn, item.dataset.action);
        closeSortMenu();
      });

      document.addEventListener('click', function (e) {
        if (!sortMenuEl || sortMenuEl.hidden) return;
        if (sortMenuEl.contains(e.target)) return;
        if (sortMenuAnchor && sortMenuAnchor.contains(e.target)) return;
        closeSortMenu();
      });

      document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape' && sortMenuEl && !sortMenuEl.hidden) closeSortMenu();
      });

      window.addEventListener('resize', closeSortMenu);

      return sortMenuEl;
    }

    function positionSortMenu(anchor) {
      if (!sortMenuEl || !anchor) return;
      const th = anchor.closest('th');
      const alignRect = th
        ? th.getBoundingClientRect()
        : anchor.getBoundingClientRect();
      const menuWidth = sortMenuEl.offsetWidth;
      const viewportPadding = 8;
      let left = alignRect.right - menuWidth;
      if (left < viewportPadding) {
        left = viewportPadding;
      }
      sortMenuEl.style.left = left + 'px';
      sortMenuEl.style.top = alignRect.bottom + 4 + 'px';
    }

    function openSortMenu(anchor, column) {
      const menu = ensureSortMenu();
      sortMenuColumn = column;
      sortMenuAnchor = anchor;

      const found = sortEntryForColumn(column);
      menu.querySelectorAll('.db-col-sort-menu-item').forEach(function (btn) {
        const action = btn.dataset.action;
        btn.classList.remove('is-active');
        btn.disabled = false;
        if (action === 'asc' && found && found.entry.direction === 'asc') {
          btn.classList.add('is-active');
        } else if (action === 'desc' && found && found.entry.direction === 'desc') {
          btn.classList.add('is-active');
        } else if (action === 'clear') {
          btn.disabled = !found;
        }
      });

      menu.hidden = false;
      if (anchor) {
        anchor.setAttribute('aria-expanded', 'true');
        positionSortMenu(anchor);
      }
    }

    function closeSortMenu() {
      if (!sortMenuEl || sortMenuEl.hidden) return;
      sortMenuEl.hidden = true;
      sortMenuColumn = null;
      sortMenuAnchor = null;
      if (thead) {
        thead
          .querySelectorAll('.db-col-sort-trigger[aria-expanded="true"]')
          .forEach(function (el) {
            el.setAttribute('aria-expanded', 'false');
          });
      }
    }

    function saveSort() {
      if (!sortUrl) return Promise.resolve();
      return fetch(sortUrl, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sort: sortStack }),
      }).catch(function () {
        /* 保存失敗はセッション内ソートに影響しない */
      });
    }

    function resetSortedDeepNavAcknowledged() {
      sortedDeepNavAcknowledged = false;
    }

    function requiresSortedNavConfirm(targetStartIndex, options) {
      if (sortStack.length === 0) return false;
      if (targetStartIndex + 1 < SORT_SLOW_ROW_THRESHOLD) return false;
      if (options && options.alwaysAsk) return true;
      return !sortedDeepNavAcknowledged;
    }

    function sortedNavConfirmMessage(targetStartIndex) {
      return (
        'ソートが適用されているため、' +
        (targetStartIndex + 1).toLocaleString() +
        ' 行目以降へ移動するとデータ取得に時間がかかる場合があります。続行しますか？'
      );
    }

    function isScrollInputBlocked() {
      return (
        sortedNavConfirmPending ||
        (sortedNavConfirmDialog && sortedNavConfirmDialog.open)
      );
    }

    function closeSortedNavConfirmDialog(confirmed) {
      if (sortedNavConfirmDialog) sortedNavConfirmDialog.close();
      if (!sortedNavConfirmResolve) return;
      const resolve = sortedNavConfirmResolve;
      sortedNavConfirmResolve = null;
      resolve(confirmed);
    }

    function confirmSortedNav(targetStartIndex) {
      if (!sortedNavConfirmDialog) return Promise.resolve(true);
      if (sortedNavConfirmMessageEl) {
        sortedNavConfirmMessageEl.textContent = sortedNavConfirmMessage(targetStartIndex);
      }
      return new Promise(function (resolve) {
        sortedNavConfirmResolve = resolve;
        sortedNavConfirmDialog.showModal();
      });
    }

    function requestNavigateToStartIndex(newStart, options) {
      if (!scrollEl || rowHeight <= 0 || totalCount === 0) return Promise.resolve(false);
      newStart = Math.max(0, Math.min(newStart, maxStartIndex()));
      if (newStart === startIndex) return Promise.resolve(true);
      if (isScrollInputBlocked()) return Promise.resolve(false);

      const needsConfirm = requiresSortedNavConfirm(newStart, options);
      if (needsConfirm) {
        sortedNavConfirmPending = true;
        wheelAccumPx = 0;
        syncScrollTopFromStartIndex();
      }

      const task = navSerial.then(function () {
        return executeNavigateToStartIndex(newStart, options, needsConfirm);
      });
      navSerial = task.catch(function () {});
      return task;
    }

    async function executeNavigateToStartIndex(newStart, options, needsConfirm) {
      try {
        if (!scrollEl || rowHeight <= 0 || totalCount === 0) return false;
        newStart = Math.max(0, Math.min(newStart, maxStartIndex()));
        if (newStart === startIndex) return true;

        if (needsConfirm) {
          syncScrollTopFromStartIndex();
          const confirmed = await confirmSortedNav(newStart);
          if (!confirmed) {
            syncScrollTopFromStartIndex();
            return false;
          }
          sortedDeepNavAcknowledged = true;
        }

        startIndex = newStart;
        syncScrollTopFromStartIndex();
        refreshView(generation);
        return true;
      } finally {
        if (needsConfirm) sortedNavConfirmPending = false;
      }
    }

    async function commitSortChange() {
      resetSortedDeepNavAcknowledged();
      updateSortIndicator();
      invalidateHeader();
      await Promise.all([saveSort(), reloadForSort()]);
    }

    async function applySortAction(column, action) {
      if (!column) return;
      const found = sortEntryForColumn(column);
      let changed = false;
      if (action === 'clear') {
        if (found) {
          sortStack.splice(found.index, 1);
          changed = true;
        }
      } else if (action === 'asc' || action === 'desc') {
        if (found) {
          if (found.entry.direction !== action) {
            found.entry.direction = action;
            changed = true;
          }
        } else {
          sortStack.push({ column: column, direction: action });
          changed = true;
        }
      }
      if (!changed) return;
      await commitSortChange();
    }

    async function clearAllSort() {
      if (!sortStack.length) return;
      sortStack = [];
      await commitSortChange();
    }

    function autoFitColumnWidth(index) {
      if (index < 0 || index >= columns.length) return;

      const colName = columns[index];
      let width = dbTableCellWidth(panel, colName, true, isPrimaryKeyColumn(colName));
      const sampleCount = Math.min(totalCount, 40);
      let rowsHtml = '';
      for (let rowIndex = 0; rowIndex < sampleCount; rowIndex++) {
        const row = getRow(rowIndex);
        if (!row || index >= row.length) continue;
        rowsHtml += '<tr>' + formatCellDisplay(row[index]) + '</tr>';
      }

      if (rowsHtml) {
        const wrapper = document.createElement('div');
        wrapper.className = 'db-table-measure-root';
        wrapper.innerHTML =
          '<table class="db-table-body-table db-table-measure-table"><tbody>' +
          rowsHtml +
          '</tbody></table>';
        panel.appendChild(wrapper);
        wrapper.querySelectorAll('td').forEach(function (cell) {
          const inner = cell.querySelector('.text-mono-cell');
          const contentWidth = inner ? inner.scrollWidth : cell.scrollWidth;
          const measured = Math.ceil(contentWidth + dbHorizontalBoxExtra(cell));
          if (measured > width) width = measured;
        });
        wrapper.remove();
      }

      columnWidths[index] = clampDbColumnWidth(width, DB_COL_WIDTH.AUTO_MAX);
      columnWidthsApplied = false;
      applyColumnWidthsOnly();
      saveColumnWidths();
    }

    function padStyle(height) {
      return 'height:' + height + 'px;padding:0;border:0;line-height:0';
    }

    function padCellsHtml(colCount, heightPx) {
      let html = '';
      for (let i = 0; i < colCount; i++) {
        const style = i === 0 ? padStyle(heightPx) : padStyle(0);
        html += '<td style="' + style + '"></td>';
      }
      return html;
    }

    function emptyRowCellsHtml(colCount, heightPx) {
      let html = '';
      for (let i = 0; i < colCount; i++) {
        html +=
          '<td style="height:' +
          heightPx +
          'px;padding:0;border:0;line-height:0"></td>';
      }
      return html;
    }

    function hasCompleteColumnWidths() {
      return (
        columnWidths.length === columns.length &&
        columnWidths.length > 0 &&
        columnWidths.every(function (w) {
          return w >= COLUMN_WIDTH_MIN;
        })
      );
    }

    function ensureColgroup(table, colCount) {
      let colgroup = table.querySelector('colgroup');
      if (!colgroup) {
        colgroup = document.createElement('colgroup');
        table.insertBefore(colgroup, table.firstChild);
      }
      while (colgroup.children.length < colCount) {
        colgroup.appendChild(document.createElement('col'));
      }
      while (colgroup.children.length > colCount) {
        colgroup.removeChild(colgroup.lastChild);
      }
      return colgroup;
    }

    function updateColgroups(widths) {
      if (!headTable || !bodyTable || widths.length === 0) return;

      [headTable, bodyTable].forEach(function (table) {
        const colgroup = ensureColgroup(table, widths.length);
        for (let i = 0; i < widths.length; i++) {
          colgroup.children[i].style.width = Math.round(widths[i]) + 'px';
        }
      });
    }

    function clearInlineWidths(cells) {
      for (let i = 0; i < cells.length; i++) {
        cells[i].style.width = '';
        cells[i].style.minWidth = '';
        cells[i].style.maxWidth = '';
      }
    }

    function clearColgroups() {
      [headTable, bodyTable].forEach(function (table) {
        if (!table) return;
        const cols = table.querySelectorAll('colgroup col');
        for (let i = 0; i < cols.length; i++) {
          cols[i].style.width = '';
        }
      });
    }

    function totalColumnWidth(widths) {
      let total = 0;
      for (let i = 0; i < widths.length; i++) {
        total += widths[i] > 0 ? widths[i] : COLUMN_WIDTH_MIN;
      }
      return total;
    }

    function setTableTotalWidth(headTable, bodyTable, widths) {
      const total = totalColumnWidth(widths) + 'px';
      headTable.style.width = total;
      headTable.style.minWidth = total;
      bodyTable.style.width = total;
      bodyTable.style.minWidth = total;
    }

    function clearTableTotalWidth(headTable, bodyTable) {
      headTable.style.width = '';
      headTable.style.minWidth = '';
      bodyTable.style.width = '';
      bodyTable.style.minWidth = '';
    }

    function applyColumnWidthsOnly() {
      if (!headTable || !bodyTable || columnWidths.length === 0) return;

      headTable.style.tableLayout = 'fixed';
      bodyTable.style.tableLayout = 'fixed';
      updateColgroups(columnWidths);
      setTableTotalWidth(headTable, bodyTable, columnWidths);
      columnWidthsApplied = true;
    }

    function measureNeedColumns(headCells, needMeasure, colCount) {
      function measureCell(cell, index) {
        if (!cell || index >= colCount) return cell ? cell.getBoundingClientRect().width : 0;
        return cell.getBoundingClientRect().width;
      }

      const measured = new Array(colCount).fill(0);

      for (let j = 0; j < needMeasure.length; j++) {
        const idx = needMeasure[j];
        measured[idx] = Math.max(measured[idx], measureCell(headCells[idx], idx));
      }

      const firstRowData = getRow(0);
      if (firstRowData && tbody) {
        let measureHtml = '<tr class="db-virtual-measure">';
        for (let k = 0; k < firstRowData.length; k++) {
          measureHtml += formatCellDisplay(firstRowData[k]);
        }
        measureHtml += '</tr>';
        tbody.insertAdjacentHTML('beforeend', measureHtml);
        const measureRow = tbody.querySelector('tr.db-virtual-measure');
        if (measureRow) {
          const cells = measureRow.querySelectorAll('td');
          for (let j = 0; j < needMeasure.length; j++) {
            const idx = needMeasure[j];
            measured[idx] = Math.max(measured[idx], measureCell(cells[idx], idx));
          }
          measureRow.remove();
        }
      }

      return measured;
    }

    function syncColumnWidths() {
      if (hasCompleteColumnWidths()) {
        if (!columnWidthsApplied) applyColumnWidthsOnly();
        return;
      }

      const headRow = thead ? thead.querySelector('tr') : null;
      if (!tbody || !headRow || !headTable || !bodyTable) return;

      const headCells = headRow.querySelectorAll('th');
      const colCount = headCells.length;
      if (colCount === 0) return;

      headTable.style.tableLayout = '';
      bodyTable.style.tableLayout = '';
      clearTableTotalWidth(headTable, bodyTable);
      clearColgroups();
      clearInlineWidths(headCells);

      const widths = new Array(colCount).fill(0);
      const needMeasure = [];

      for (let i = 0; i < colCount; i++) {
        if (columnWidths[i] >= COLUMN_WIDTH_MIN) {
          widths[i] = columnWidths[i];
          continue;
        }
        const colName = columns[i];
        if (savedColumnWidths && savedColumnWidths[colName]) {
          widths[i] = savedColumnWidths[colName];
          continue;
        }
        needMeasure.push(i);
      }

      if (needMeasure.length > 0) {
        const measured = measureNeedColumns(headCells, needMeasure, colCount);
        for (let j = 0; j < needMeasure.length; j++) {
          const idx = needMeasure[j];
          widths[idx] = measured[idx];
        }
      }

      for (let i = 0; i < colCount; i++) {
        if (widths[i] < COLUMN_WIDTH_MIN) widths[i] = COLUMN_WIDTH_MIN;
      }

      columnWidths = widths;
      applyColumnWidthsOnly();
    }

    function saveColumnWidths() {
      if (!columnWidthsUrl || columns.length === 0) return;

      const widths = {};
      for (let i = 0; i < columns.length; i++) {
        if (columnWidths[i] > 0) {
          widths[columns[i]] = Math.round(columnWidths[i]);
        }
      }

      savedColumnWidths = widths;

      fetch(columnWidthsUrl, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ widths: widths }),
      }).catch(function () {
        /* 保存失敗は次回表示に影響するのみ */
      });
    }

    function onResizeMouseMove(e) {
      if (!activeResize) return;
      const delta = e.clientX - activeResize.startX;
      const newWidth = Math.max(
        COLUMN_WIDTH_MIN,
        Math.round(activeResize.startWidth + delta)
      );
      columnWidths[activeResize.index] = newWidth;
      columnWidthsApplied = false;
      applyColumnWidthsOnly();
    }

    function onResizeMouseUp() {
      if (!activeResize) return;

      document.removeEventListener('mousemove', onResizeMouseMove);
      document.removeEventListener('mouseup', onResizeMouseUp);
      document.body.style.userSelect = '';

      const handle = activeResize.handle;
      if (handle) handle.classList.remove('is-active');

      activeResize = null;
      saveColumnWidths();
    }

    function onResizeMouseDown(e) {
      if (!isPrimaryPointerButton(e)) return;

      const handle = e.target.closest('.db-col-resize-handle');
      if (!handle || !thead) return;

      closeSortMenu();
      e.preventDefault();
      const th = handle.closest('th');
      if (!th) return;

      const headRow = th.parentNode;
      const index = Array.prototype.indexOf.call(headRow.children, th);
      if (index < 0) return;

      activeResize = {
        index: index,
        startX: e.clientX,
        startWidth:
          columnWidths[index] >= COLUMN_WIDTH_MIN
            ? columnWidths[index]
            : th.getBoundingClientRect().width,
        handle: handle,
      };

      handle.classList.add('is-active');
      document.body.style.userSelect = 'none';
      document.addEventListener('mousemove', onResizeMouseMove);
      document.addEventListener('mouseup', onResizeMouseUp);
    }

    function onResizeDoubleClick(e) {
      const handle = e.target.closest('.db-col-resize-handle');
      if (!handle || !thead || !isPrimaryPointerButton(e)) return;

      e.preventDefault();
      e.stopPropagation();
      cancelActiveColumnResize();

      const index = columnIndexFromCell(handle.closest('th'));
      if (index < 0) return;

      autoFitColumnWidth(index);
    }

    function touchCacheOffset(offset) {
      if (!cache.has(offset)) return;
      const rows = cache.get(offset);
      cache.delete(offset);
      cache.set(offset, rows);
    }

    function setCacheOffset(offset, rows) {
      if (cache.has(offset)) cache.delete(offset);
      cache.set(offset, rows);
      evictCacheIfNeeded();
    }

    function pinnedChunkOffsets() {
      const pinned = new Set(pinnedHighOffsets);
      const visible = chunkOffsetsForRange(startIndex, visibleCount);
      for (let i = 0; i < visible.length; i++) {
        pinned.add(visible[i]);
      }
      return pinned;
    }

    function evictCacheIfNeeded() {
      const pinned = pinnedChunkOffsets();
      while (cache.size > maxCachedChunks) {
        let evicted = false;
        for (const offset of cache.keys()) {
          if (!pinned.has(offset)) {
            cache.delete(offset);
            evicted = true;
            break;
          }
        }
        if (!evicted) break;
      }
    }

    function clearFetchQueues() {
      highQueue = [];
      queuedOffsets.clear();
      pinnedHighOffsets.clear();
    }

    function cancelPrefetch() {
      wantPrefetch = false;
      if (prefetchAbortController) {
        prefetchAbortController.abort();
        prefetchAbortController = null;
      }
      prefetchTargetOffset = null;
    }

    function resetTableDataSession() {
      generation += 1;
      if (abortController) {
        abortController.abort();
        abortController = null;
      }
      cancelPrefetch();
      cache.clear();
      inFlight.clear();
      clearFetchQueues();
      lastPrefetchStartIndex = 0;
      renderPending = false;
      needsRefresh = false;
      if (renderRaf) {
        cancelAnimationFrame(renderRaf);
        renderRaf = 0;
      }
      if (scrollRaf) {
        cancelAnimationFrame(scrollRaf);
        scrollRaf = 0;
      }
      navSerial = Promise.resolve();
      sortedNavConfirmPending = false;
      if (sortedNavConfirmResolve) {
        closeSortedNavConfirmDialog(false);
      }
      closeSortMenu();
      closeRowGotoDialog();
      closeCellEditDialog();
      wheelAccumPx = 0;
    }

    function isOffsetInVisibleRange(offset) {
      const offsets = chunkOffsetsForRange(startIndex, visibleCount);
      for (let i = 0; i < offsets.length; i++) {
        if (offsets[i] === offset) return true;
      }
      return false;
    }

    function scheduledChunkCount() {
      return cache.size + inFlight.size + queuedOffsets.size;
    }

    function maxChunkOffset() {
      if (totalCount <= 0) return 0;
      return chunkOffsetForIndex(totalCount - 1);
    }

    function enqueueChunk(offset, gen) {
      if (gen !== generation) return;

      if (cache.has(offset)) {
        pinnedHighOffsets.add(offset);
        return;
      }

      pinnedHighOffsets.add(offset);
      if (inFlight.has(offset)) {
        pumpFetches();
        return;
      }

      const highIdx = highQueue.indexOf(offset);
      if (highIdx >= 0) {
        if (highIdx > 0) {
          highQueue.splice(highIdx, 1);
          highQueue.unshift(offset);
        }
        pumpFetches();
        return;
      }

      if (queuedOffsets.has(offset)) return;

      queuedOffsets.add(offset);
      highQueue.unshift(offset);
      pumpFetches();
    }

    function dequeueNextOffset() {
      while (highQueue.length > 0) {
        const offset = highQueue.shift();
        queuedOffsets.delete(offset);
        if (!cache.has(offset) && !inFlight.has(offset)) {
          return offset;
        }
        pinnedHighOffsets.delete(offset);
      }
      return null;
    }

    function pumpFetches() {
      const gen = generation;
      if (activeFetches >= FETCH_CONCURRENCY) return;

      const offset = dequeueNextOffset();
      if (offset !== null) {
        activeFetches += 1;
        fetchChunk(offset, gen)
          .then(function () {
            if (gen === generation && isOffsetInVisibleRange(offset)) {
              scheduleRender();
            }
          })
          .catch(function (err) {
            if (err && err.name === 'AbortError') return;
            if (gen === generation && isOffsetInVisibleRange(offset)) {
              setStatus('error', err.message || '取得に失敗しました', true);
            }
          })
          .finally(function () {
            pinnedHighOffsets.delete(offset);
            activeFetches = Math.max(0, activeFetches - 1);
            pumpFetches();
          });
        return;
      }

      if (!wantPrefetch) return;

      const prefetchOffset = peekNextPrefetchOffset();
      if (!canPrefetchOffset(prefetchOffset)) {
        wantPrefetch = false;
        return;
      }

      wantPrefetch = false;
      lastPrefetchStartIndex = startIndex;
      prefetchTargetOffset = prefetchOffset;
      ensurePrefetchAbortController();

      activeFetches += 1;
      fetchChunk(prefetchOffset, gen, { prefetch: true })
        .catch(function (err) {
          if (err && err.name === 'AbortError') return;
        })
        .finally(function () {
          prefetchTargetOffset = null;
          activeFetches = Math.max(0, activeFetches - 1);
          pumpFetches();
        });
    }

    function enqueueVisibleChunks(start, count, gen) {
      const offsets = chunkOffsetsForRange(start, count);
      for (let i = 0; i < offsets.length; i++) {
        enqueueChunk(offsets[i], gen);
      }
    }

    function canPrefetchOffset(offset) {
      if (offset < 0 || offset > maxChunkOffset()) return false;
      if (scheduledChunkCount() >= maxCachedChunks) return false;
      if (isOffsetInVisibleRange(offset)) return false;
      if (cache.has(offset)) return false;
      if (inFlight.has(offset)) return false;
      return true;
    }

    function peekNextPrefetchOffset() {
      const step = chunkSizeActual;
      const scrollingDown = startIndex >= lastPrefetchStartIndex;
      const firstVisible = chunkOffsetForIndex(startIndex);
      const lastVisible = chunkOffsetForIndex(
        Math.min(startIndex + Math.max(visibleCount, 1) - 1, totalCount - 1)
      );
      return scrollingDown ? lastVisible + step : firstVisible - step;
    }

    function ensurePrefetchAbortController() {
      if (!prefetchAbortController || prefetchAbortController.signal.aborted) {
        prefetchAbortController = new AbortController();
      }
    }

    function updatePrefetch(gen) {
      if (gen !== generation || totalCount <= 0) return;

      const nextOffset = peekNextPrefetchOffset();
      if (prefetchTargetOffset !== null && prefetchTargetOffset !== nextOffset) {
        if (prefetchAbortController) prefetchAbortController.abort();
        prefetchAbortController = null;
      }

      wantPrefetch = true;
      pumpFetches();
    }

    function scheduleRender() {
      if (renderRaf) return;
      renderRaf = requestAnimationFrame(function () {
        renderRaf = 0;
        if (renderPending) {
          needsRefresh = true;
          return;
        }
        renderVisibleRows();
        updateViewStatus();
      });
    }

    function getRow(rowIndex) {
      const chunkOffset = chunkOffsetForIndex(rowIndex);
      const chunk = cache.get(chunkOffset);
      if (!chunk) return null;
      touchCacheOffset(chunkOffset);
      return chunk[rowIndex - chunkOffset] || null;
    }

    function chunkOffsetsForRange(start, count) {
      if (totalCount === 0 || count <= 0) return [];
      const end = Math.min(start + count - 1, totalCount - 1);
      const firstChunk = chunkOffsetForIndex(start);
      const lastChunk = chunkOffsetForIndex(end);
      const offsets = [];
      for (let offset = firstChunk; offset <= lastChunk; offset += chunkSizeActual) {
        offsets.push(offset);
      }
      return offsets;
    }

    function renderHeader() {
      if (!thead || columnsRendered || columns.length === 0) return;
      const showPriority = sortStack.length > 1;
      const sortByColumn = sortIndexMap();
      thead.innerHTML =
        '<tr>' +
        columns
          .map(function (col) {
            const found = sortByColumn.get(col);
            const priority = found ? found.index + 1 : 0;
            return dbHeaderCellHtml(
              col,
              found ? found.entry : null,
              priority,
              showPriority,
              isPrimaryKeyColumn(col)
            );
          })
          .join('') +
        '</tr>';
      columnsRendered = true;
    }

    function syncHorizontalScroll() {
      if (!headerEl || !scrollEl) return;
      headerEl.scrollLeft = scrollEl.scrollLeft;
    }

    function scrollHorizontally(deltaPx) {
      if (!scrollEl || deltaPx === 0) return;
      scrollEl.scrollLeft += deltaPx;
      syncHorizontalScroll();
    }

    function computeScaledPads() {
      const eff = effectiveScrollHeight();
      const maxTopPad = Math.max(0, eff - visibleCount * rowHeight);
      const topPad = scrollEl ? Math.min(scrollEl.scrollTop, maxTopPad) : 0;
      const bottomPad = Math.max(0, eff - topPad - visibleCount * rowHeight);
      return { topPad: topPad, bottomPad: bottomPad };
    }

    function updateScaledPads() {
      if (!tbody || !isScaled() || totalCount <= 0) return;
      const pads = computeScaledPads();
      const topRow = tbody.querySelector('.db-virtual-pad-top');
      if (topRow) {
        const firstTd = topRow.querySelector('td');
        if (firstTd) firstTd.style.cssText = padStyle(pads.topPad);
      }
      const bottomRow = tbody.querySelector('.db-virtual-pad-bottom');
      if (bottomRow) {
        const firstTd = bottomRow.querySelector('td');
        if (firstTd) firstTd.style.cssText = padStyle(pads.bottomPad);
      }
    }

    function renderVisibleRows() {
      if (!tbody) return;

      if (columns.length === 0) {
        tbody.innerHTML = '';
        if (!columnsRendered && thead) thead.innerHTML = '';
        if (emptyEl) emptyEl.hidden = true;
        updateCount('—');
        return;
      }

      if (totalCount === 0) {
        tbody.innerHTML = '';
        renderHeader();
        syncColumnWidths();
        if (emptyEl) emptyEl.hidden = false;
        updateCount(0);
        return;
      }

      renderHeader();

      const colCount = columns.length;
      const dataRowCount = Math.min(visibleCount, Math.max(0, totalCount - startIndex));

      let topPad;
      let bottomPad;
      if (isScaled()) {
        // スケール時はスペーサ総高をキャップ済み高に保ちつつ、表示行を実スクロール
        // 位置へ重ねる。topPad を現在の scrollTop に合わせることで罫線・内容が
        // ビューポート内に正しく描画される。
        const pads = computeScaledPads();
        topPad = pads.topPad;
        bottomPad = pads.bottomPad;
      } else {
        topPad = startIndex * rowHeight;
        bottomPad = Math.max(0, (totalCount - startIndex - visibleCount) * rowHeight);
      }

      let html =
        '<tr class="db-virtual-pad-top" aria-hidden="true">' +
        padCellsHtml(colCount, topPad) +
        '</tr>';

      for (let i = 0; i < visibleCount; i++) {
        const rowIndex = startIndex + i;
        if (rowIndex >= totalCount) {
          html +=
            '<tr class="db-virtual-empty" aria-hidden="true">' +
            emptyRowCellsHtml(colCount, rowHeight) +
            '</tr>';
          continue;
        }
        const row = getRow(rowIndex);
        html += '<tr data-row-index="' + rowIndex + '">';
        if (row) {
          for (let k = 0; k < row.length; k++) {
            html += formatCellDisplay(row[k], editableColumnFlags[k]);
          }
        } else {
          for (let k = 0; k < colCount; k++) {
            html += '<td class="text-mono-cell"></td>';
          }
        }
        html += '</tr>';
      }

      html +=
        '<tr class="db-virtual-pad-bottom" aria-hidden="true">' +
        padCellsHtml(colCount, bottomPad) +
        '</tr>';

      const savedScrollTop = scrollEl ? scrollEl.scrollTop : 0;
      const savedScrollLeft = scrollEl ? scrollEl.scrollLeft : 0;
      tbody.innerHTML = html;
      if (scrollEl) {
        isSyncingScroll = true;
        scrollEl.scrollTop = clampScrollOffset(savedScrollTop);
        scrollEl.scrollLeft = savedScrollLeft;
        lastSyncedScrollTop = scrollEl.scrollTop;
        isSyncingScroll = false;
        syncHorizontalScroll();
      }
      if (emptyEl) emptyEl.hidden = true;

      if (dataRowCount > 0) {
        updateCount(startIndex + 1);
      } else {
        updateCount(0);
      }

      syncColumnWidths();
    }

    function measureRowHeight() {
      if (!tbody || columns.length === 0) return 35;

      const sampleRow = getRow(0);
      if (!sampleRow) return 35;

      let html = '<tr class="db-virtual-measure">';
      for (let k = 0; k < sampleRow.length; k++) {
        html += formatCellDisplay(sampleRow[k], editableColumnFlags[k]);
      }
      html += '</tr>';
      tbody.innerHTML = html;

      const measureTr = tbody.querySelector('.db-virtual-measure');
      if (measureTr) {
        const height = measureTr.getBoundingClientRect().height;
        if (height > 0) {
          rowHeight = height;
          panel.style.setProperty('--db-row-height', rowHeight + 'px');
          return rowHeight;
        }
      }
      rowHeight = 35;
      panel.style.setProperty('--db-row-height', '35px');
      return rowHeight;
    }

    function scrollViewportHeight() {
      if (!scrollEl) return 0;
      let height = scrollEl.clientHeight;
      if (height > 0) return height;
      if (!panel) return 0;
      const panelHeight = panel.getBoundingClientRect().height;
      const headerHeight = headerEl ? headerEl.getBoundingClientRect().height : 0;
      return Math.max(0, panelHeight - headerHeight);
    }

    function calcVisibleCount(viewportOverride) {
      if (!scrollEl || rowHeight <= 0) return 10;
      let viewportHeight =
        typeof viewportOverride === 'number' && viewportOverride > 0
          ? viewportOverride
          : scrollViewportHeight();
      if (viewportHeight <= 0) return 10;
      return Math.ceil(viewportHeight / rowHeight) + overscan;
    }

    function maxStartIndex() {
      return Math.max(0, totalCount - visibleCount);
    }

    function chunkResponse(offset) {
      return {
        rows: cache.get(offset) || [],
        columns: columns,
        total_count: totalCount,
        chunk_size: chunkSizeActual,
        offset: offset,
      };
    }

    async function fetchChunk(offset, gen, options) {
      const isPrefetch = !!(options && options.prefetch);
      if (cache.has(offset)) {
        touchCacheOffset(offset);
        return chunkResponse(offset);
      }
      if (inFlight.has(offset)) {
        await inFlight.get(offset);
        return cache.has(offset) ? chunkResponse(offset) : null;
      }

      const promise = (async function () {
        const url =
          apiUrl +
          (apiUrl.indexOf('?') >= 0 ? '&' : '?') +
          'offset=' +
          offset +
          sortQueryString();
        const signal = isPrefetch
          ? prefetchAbortController
            ? prefetchAbortController.signal
            : undefined
          : abortController
            ? abortController.signal
            : undefined;
        const response = await fetch(url, {
          signal: signal,
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
          columnMeta = Array.isArray(data.column_meta) ? data.column_meta : [];
          rebuildColumnCaches();
          totalCount = data.total_count || 0;
          chunkSizeActual = data.chunk_size || chunkSize;
          savedColumnWidths = data.column_widths || null;
          columnWidths = [];
          columnWidthsApplied = false;
          invalidateHeader();
          sortStack = Array.isArray(data.sort) ? data.sort.slice() : [];
          updateSortIndicator();
        }

        setCacheOffset(offset, data.rows || []);
        return data;
      })();

      inFlight.set(offset, promise);
      try {
        return await promise;
      } catch (err) {
        if (err && err.name === 'AbortError') return null;
        throw err;
      } finally {
        inFlight.delete(offset);
      }
    }

    function refreshView(gen) {
      if (renderPending) {
        needsRefresh = true;
        return;
      }
      renderPending = true;

      try {
        do {
          needsRefresh = false;

          renderVisibleRows();
          updateViewStatus();

          enqueueVisibleChunks(startIndex, visibleCount, gen);
          updatePrefetch(gen);
        } while (needsRefresh && gen === generation);
      } finally {
        renderPending = false;
      }
    }

    function totalScrollHeight() {
      return totalCount * rowHeight;
    }

    // 実際にDOMへ与えるスクロール領域高。ブラウザ上限を超えないようキャップする。
    function effectiveScrollHeight() {
      return Math.min(totalScrollHeight(), SAFE_MAX_SCROLL_HEIGHT);
    }

    // 総高がキャップを超え、スクロール位置を比例マッピングする必要があるか。
    function isScaled() {
      return totalScrollHeight() > SAFE_MAX_SCROLL_HEIGHT;
    }

    function maxScrollOffset() {
      if (rowHeight > 0 && totalCount > 0) {
        return Math.max(0, effectiveScrollHeight() - scrollEl.clientHeight);
      }
      return Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
    }

    function clampScrollOffset(offset) {
      return Math.max(0, Math.min(offset, maxScrollOffset()));
    }

    // 行インデックスから対応するスクロール位置を求める（リサイズ時の補正に使用）。
    function offsetFromStartIndex(index) {
      if (isScaled()) {
        const maxStart = maxStartIndex();
        const fraction = maxStart > 0 ? index / maxStart : 0;
        return Math.round(fraction * maxScrollOffset());
      }
      return index * rowHeight;
    }

    function startIndexFromOffset(offset) {
      const maxScroll = maxScrollOffset();
      if (offset >= maxScroll - 1) {
        return maxStartIndex();
      }
      if (isScaled()) {
        const fraction = maxScroll > 0 ? offset / maxScroll : 0;
        return Math.min(Math.round(fraction * maxStartIndex()), maxStartIndex());
      }
      return Math.min(Math.floor(offset / rowHeight), maxStartIndex());
    }

    function syncFromOffset(offset) {
      if (rowHeight <= 0 || totalCount === 0) return;

      const newStart = startIndexFromOffset(offset);
      if (newStart === startIndex) {
        if (isScaled()) updateScaledPads();
        return;
      }

      if (isScrollInputBlocked()) {
        syncScrollTopFromStartIndex();
        return;
      }

      if (requiresSortedNavConfirm(newStart)) {
        syncScrollTopFromStartIndex();
        requestNavigateToStartIndex(newStart);
        return;
      }

      startIndex = newStart;
      refreshView(generation);
    }

    function syncFromScroll() {
      if (isSyncingScroll || rowHeight <= 0 || totalCount === 0) return;
      syncFromOffset(scrollEl.scrollTop);
    }

    // isScaled() 時のホイール・キー入力は圧縮スクロール空間ではなく行単位で処理する。
    function canUseRowScrollInput() {
      return isScaled() && rowHeight > 0 && totalCount > 0;
    }

    function syncScrollTopFromStartIndex() {
      if (!scrollEl) return;
      isSyncingScroll = true;
      scrollEl.scrollTop = offsetFromStartIndex(startIndex);
      lastSyncedScrollTop = scrollEl.scrollTop;
      isSyncingScroll = false;
    }

    function wheelDeltaToPixels(e, rowH, clientHeight) {
      switch (e.deltaMode) {
        case WheelEvent.DOM_DELTA_LINE:
          return e.deltaY * rowH;
        case WheelEvent.DOM_DELTA_PAGE:
          return e.deltaY * clientHeight;
        default:
          return e.deltaY;
      }
    }

    function pageRowCount() {
      return Math.max(1, Math.floor(scrollEl.clientHeight / rowHeight));
    }

    function scrollToStartIndex(newStart, options) {
      requestNavigateToStartIndex(newStart, options);
    }

    function scrollByRows(deltaRows, options) {
      if (deltaRows === 0) return;
      scrollToStartIndex(startIndex + deltaRows, options);
    }

    function isHorizontalWheel(e) {
      return e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
    }

    function horizontalWheelDelta(e) {
      // Shift+ホイールは deltaY のみを使う（一部ブラウザは deltaX も送り方向が逆になる）
      if (e.shiftKey) return e.deltaY;
      return e.deltaX;
    }

    function onWheel(e) {
      if (isHorizontalWheel(e)) {
        e.preventDefault();
        scrollHorizontally(horizontalWheelDelta(e));
        return;
      }

      if (!canUseRowScrollInput()) return;

      e.preventDefault();
      if (isScrollInputBlocked()) {
        wheelAccumPx = 0;
        syncScrollTopFromStartIndex();
        return;
      }

      wheelAccumPx += wheelDeltaToPixels(e, rowHeight, scrollEl.clientHeight);
      const rows = Math.trunc(wheelAccumPx / rowHeight);
      if (rows === 0) return;
      wheelAccumPx -= rows * rowHeight;
      scrollByRows(rows);
    }

    function onKeyDown(e) {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const step = Math.max(40, Math.floor(scrollEl.clientWidth * 0.1));
        scrollHorizontally(e.key === 'ArrowRight' ? step : -step);
        return;
      }

      if (!canUseRowScrollInput()) return;
      if (isScrollInputBlocked()) {
        e.preventDefault();
        syncScrollTopFromStartIndex();
        return;
      }

      if (e.key === 'Home') {
        e.preventDefault();
        scrollToStartIndex(0);
        return;
      }
      if (e.key === 'End') {
        e.preventDefault();
        scrollToStartIndex(maxStartIndex());
        return;
      }

      let rows;
      if (e.key === 'ArrowUp') rows = -1;
      else if (e.key === 'ArrowDown') rows = 1;
      else if (e.key === 'PageUp') rows = -pageRowCount();
      else if (e.key === 'PageDown') rows = pageRowCount();
      else return;

      e.preventDefault();
      scrollByRows(rows);
    }

    function onResize() {
      if (rowHeight <= 0 || totalCount === 0) return;

      const dataTr = tbody.querySelector(
        'tr:not(.db-virtual-pad-top):not(.db-virtual-pad-bottom):not(.db-virtual-empty)'
      );
      if (dataTr) {
        const measured = dataTr.getBoundingClientRect().height;
        if (measured > 0) {
          rowHeight = measured;
          panel.style.setProperty('--db-row-height', rowHeight + 'px');
        }
      }

      const prevStart = startIndex;
      visibleCount = calcVisibleCount();
      startIndex = Math.min(prevStart, maxStartIndex());
      syncScrollTopFromStartIndex();
      refreshView(generation);
    }

    function rafThrottle(fn) {
      return function () {
        if (scrollRaf) return;
        scrollRaf = requestAnimationFrame(function () {
          scrollRaf = 0;
          fn();
        });
      };
    }

    async function fetchAndRenderFromStart(gen) {
      const first = await fetchChunk(0, gen);
      if (!first || gen !== generation) return false;

      if (first.total_count === 0) {
        setStatus('empty', 'データがありません', false);
        renderVisibleRows();
        return true;
      }

      // measureRowHeight が tbody を計測用1行に差し替えると、スクロール領域の
      // clientHeight が一時的に 0 になり visibleCount が overscan だけになる。
      // 差し替え前のビューポート高を保持して表示行数を求める。
      const viewportBeforeMeasure = scrollViewportHeight();
      rowHeight = measureRowHeight();
      visibleCount = calcVisibleCount(viewportBeforeMeasure);
      startIndex = 0;

      renderVisibleRows();
      updateViewStatus();

      enqueueVisibleChunks(0, visibleCount, gen);
      updatePrefetch(gen);

      requestAnimationFrame(function () {
        if (gen !== generation) return;
        const nextVisible = calcVisibleCount();
        if (nextVisible > visibleCount) {
          visibleCount = nextVisible;
          refreshView(gen);
        }
      });

      return true;
    }

    function handleReloadError(err, gen) {
      if (err && err.name === 'AbortError') return;
      if (gen !== generation) return;
      setStatus('error', err.message || '取得に失敗しました', true);
    }

    async function reloadData(options) {
      const fullReset = !!(options && options.fullReset);
      resetTableDataSession();
      const gen = generation;
      abortController = new AbortController();
      invalidateHeader();
      startIndex = 0;
      lastSyncedScrollTop = -1;

      if (fullReset) {
        resetSortedDeepNavAcknowledged();
        columns = [];
        columnMeta = [];
        rebuildColumnCaches();
        totalCount = 0;
        chunkSizeActual = chunkSize;
        rowHeight = 0;
        visibleCount = 0;
        savedColumnWidths = null;
        columnWidths = [];
        columnWidthsApplied = false;
        sortStack = [];
        updateSortIndicator();
        activeResize = null;
        if (tbody) tbody.innerHTML = '';
        if (thead) thead.innerHTML = '';
        if (emptyEl) emptyEl.hidden = true;
        if (scrollEl) {
          scrollEl.scrollTop = 0;
          scrollEl.scrollLeft = 0;
        }
        syncHorizontalScroll();
        updateCount('—');
      } else if (scrollEl) {
        scrollEl.scrollTop = 0;
      }

      setStatus('loading', '読み込み中…', false);

      try {
        const ok = await fetchAndRenderFromStart(gen);
        if (!ok && gen === generation) {
          setStatus('error', '取得に失敗しました', true);
        }
      } catch (err) {
        handleReloadError(err, gen);
      }
    }

    async function load() {
      await reloadData({ fullReset: true });
    }

    async function reloadForSort() {
      await reloadData({ fullReset: false });
    }

    if (scrollEl) {
      scrollEl.addEventListener(
        'scroll',
        rafThrottle(function () {
          if (sortMenuEl && !sortMenuEl.hidden) closeSortMenu();
          syncHorizontalScroll();
          const top = scrollEl.scrollTop;
          if (top !== lastSyncedScrollTop) {
            lastSyncedScrollTop = top;
            syncFromScroll();
          }
        })
      );
      scrollEl.addEventListener('wheel', onWheel, { passive: false });
      scrollEl.addEventListener('keydown', onKeyDown);
      new ResizeObserver(rafThrottle(onResize)).observe(scrollEl);
    }

    if (thead) {
      thead.addEventListener('mousedown', onResizeMouseDown);
      thead.addEventListener('dblclick', onResizeDoubleClick);
      thead.addEventListener('click', function (e) {
        const trigger = e.target.closest('.db-col-sort-trigger');
        if (!trigger) return;
        e.preventDefault();
        e.stopPropagation();
        cancelActiveColumnResize();

        const th = trigger.closest('th');
        const index = columnIndexFromCell(th);
        if (index < 0 || index >= columns.length) return;

        const column = columns[index];
        if (sortMenuEl && !sortMenuEl.hidden && sortMenuColumn === column) {
          closeSortMenu();
          return;
        }
        closeSortMenu();
        openSortMenu(trigger, column);
      });
      thead.addEventListener('keydown', function (e) {
        const trigger = e.target.closest('.db-col-sort-trigger');
        if (!trigger) return;
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          trigger.click();
        }
      });
    }

    if (countEl) {
      countEl.addEventListener('click', openRowGotoDialog);
    }
    if (sortClearBtn) {
      sortClearBtn.addEventListener('click', function (e) {
        e.preventDefault();
        e.stopPropagation();
        clearAllSort();
      });
    }
    if (rowGotoForm) {
      rowGotoForm.addEventListener('submit', submitRowGoto);
    }
    if (rowGotoCancel) {
      rowGotoCancel.addEventListener('click', closeRowGotoDialog);
    }
    if (rowGotoDialog) {
      rowGotoDialog.addEventListener('click', function (e) {
        if (e.target === rowGotoDialog) closeRowGotoDialog();
      });
    }
    if (sortedNavConfirmOk) {
      sortedNavConfirmOk.addEventListener('click', function () {
        closeSortedNavConfirmDialog(true);
      });
    }
    if (sortedNavConfirmCancel) {
      sortedNavConfirmCancel.addEventListener('click', function () {
        closeSortedNavConfirmDialog(false);
      });
    }
    if (sortedNavConfirmDialog) {
      sortedNavConfirmDialog.addEventListener('click', function (e) {
        if (e.target === sortedNavConfirmDialog) closeSortedNavConfirmDialog(false);
      });
      sortedNavConfirmDialog.addEventListener('cancel', function (e) {
        e.preventDefault();
        closeSortedNavConfirmDialog(false);
      });
    }
    if (cellEditForm) {
      cellEditForm.addEventListener('submit', submitCellEdit);
    }
    if (cellEditCancel) {
      cellEditCancel.addEventListener('click', closeCellEditDialog);
    }
    if (cellEditDialog) {
      cellEditDialog.addEventListener('click', function (e) {
        if (e.target === cellEditDialog) closeCellEditDialog();
      });
    }
    if (tbody && !readOnly) {
      tbody.addEventListener('click', function (e) {
        const cell = e.target.closest('td.db-cell-editable');
        if (!cell) return;
        handleCellEditActivation(cell);
      });
      tbody.addEventListener('keydown', function (e) {
        if (e.key !== 'Enter' && e.key !== ' ') return;
        const cell = e.target.closest('td.db-cell-editable');
        if (!cell) return;
        e.preventDefault();
        handleCellEditActivation(cell);
      });
    }

    window.addEventListener('pagehide', resetTableDataSession);
    window.addEventListener('pageshow', function (event) {
      if (event.persisted) {
        reloadData({ fullReset: true });
      }
    });

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

  function initDatabaseIndex() {
    const checkbox = document.getElementById('db-show-system-tables');
    const emptyEl = document.getElementById('db-tables-empty');
    const tbody = document.getElementById('db-tables-body');
    if (!checkbox) return;

    const STORAGE_KEY = 'admin.database.showSystemTables';
    const systemRows = tbody ? tbody.querySelectorAll('.db-table-row-system') : [];
    const userRowCount = tbody ? tbody.rows.length - systemRows.length : 0;

    function applyShowSystem(show) {
      for (let i = 0; i < systemRows.length; i++) {
        systemRows[i].hidden = !show;
      }
      checkbox.checked = show;
      if (emptyEl && tbody) {
        emptyEl.hidden = show || userRowCount > 0;
      }
      try {
        localStorage.setItem(STORAGE_KEY, show ? '1' : '0');
      } catch (_err) {
        /* ignore quota / private mode */
      }
    }

    let show = false;
    try {
      show = localStorage.getItem(STORAGE_KEY) === '1';
    } catch (_err) {
      /* ignore */
    }

    applyShowSystem(show);
    checkbox.addEventListener('change', function () {
      applyShowSystem(checkbox.checked);
    });
  }

  function initPageModules() {
    initTemplateRepeater();
    initSeedForm();
    initMediaPicker();
    initTableData();
    initWidgetConfig();
    initDatabaseIndex();
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
