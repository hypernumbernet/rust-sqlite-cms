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
    AUTO_MAX: 1000,
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

  function dbHeaderMeasureThHtml(columnName, isPrimaryKey) {
    return (
      '<th>' +
      dbHeaderLabelHtml(columnName, isPrimaryKey) +
      '<span class="db-col-sort-trigger" aria-hidden="true"><span class="db-col-sort-icon"></span></span>' +
      '<span class="db-col-resize-handle" aria-hidden="true"></span></th>'
    );
  }

  function dbHeaderMeasureRowHtml(columnName, isPrimaryKey) {
    return '<tr>' + dbHeaderMeasureThHtml(columnName, isPrimaryKey) + '</tr>';
  }

  function measureDbCellWidth(cell, isHeader) {
    const inner = cell.querySelector(isHeader ? '.text-mono' : '.text-mono-cell');
    const contentWidth = inner ? inner.scrollWidth : cell.scrollWidth;
    return Math.ceil(
      contentWidth +
        dbHorizontalBoxExtra(cell) +
        (isHeader ? DB_COL_WIDTH.HEADER_CHROME : 0)
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

    const width = measureDbCellWidth(cell, isHeader);
    wrapper.remove();
    return width;
  }

  function dbHeaderCellHtml(
    columnName,
    sortEntry,
    sortPriority,
    showPriority,
    isPrimaryKey,
    filterText
  ) {
    let thClass = '';
    let badgeHtml = '';
    let triggerClass = 'db-col-sort-trigger';
    let ariaSort = 'none';
    const hasFilter = !!(filterText && String(filterText).length);

    if (hasFilter) {
      triggerClass += ' has-filter-state';
      badgeHtml +=
        '<span class="db-col-filter-mark" title="フィルター: ' +
        escapeHtml(filterText) +
        '" aria-hidden="true">■</span>';
    }

    if (sortEntry) {
      thClass +=
        sortEntry.direction === 'asc' ? ' is-sorted-asc' : ' is-sorted-desc';
      ariaSort = sortEntry.direction === 'asc' ? 'ascending' : 'descending';
      triggerClass += ' has-sort-state';
      badgeHtml +=
        '<span class="db-col-sort-arrow" aria-hidden="true">' +
        (sortEntry.direction === 'asc' ? '▲' : '▼') +
        '</span>';
      if (showPriority && sortPriority > 0) {
        badgeHtml += '<span class="db-col-sort-priority">' + sortPriority + '</span>';
      }
    }

    let actionLabel = ' のソート';
    if (hasFilter && sortEntry) {
      actionLabel = ' のソートとフィルター';
    } else if (hasFilter) {
      actionLabel = ' のフィルター';
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
      actionLabel +
      '">' +
      badgeHtml +
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
    return path === '/admin'
      || path.startsWith('/admin/')
      || path === '/static'
      || path.startsWith('/static/')
      || path === '/uploads'
      || path.startsWith('/uploads/');
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
    const fitAllColumnsBtn = document.getElementById('db-data-fit-all-columns');
    const sortIndicatorEl = document.getElementById('db-data-sort-indicator');
    const sortIndicatorLabelEl = document.getElementById('db-data-sort-indicator-label');
    const sortClearBtn = document.getElementById('db-data-sort-clear');
    const filterIndicatorEl = document.getElementById('db-data-filter-indicator');
    const filterIndicatorLabelEl = document.getElementById('db-data-filter-indicator-label');
    const filterClearBtn = document.getElementById('db-data-filter-clear');
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
    const filterUrl = dataApiPath('/filter');
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
    let filterStack = [];
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
        setStatus('empty', emptyDataStatusText(), false);
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

    function stackEntryForColumn(stack, column) {
      for (let i = 0; i < stack.length; i++) {
        if (stack[i].column === column) {
          return { entry: stack[i], index: i };
        }
      }
      return null;
    }

    function sortIndexMap() {
      const map = new Map();
      for (let i = 0; i < sortStack.length; i++) {
        map.set(sortStack[i].column, { entry: sortStack[i], index: i });
      }
      return map;
    }

    function activeFilterEntries() {
      const active = [];
      for (let i = 0; i < filterStack.length; i++) {
        if (filterStack[i].text) active.push(filterStack[i]);
      }
      return active;
    }

    function filterIndexMap() {
      const map = new Map();
      const active = activeFilterEntries();
      for (let i = 0; i < active.length; i++) {
        map.set(active[i].column, active[i]);
      }
      return map;
    }

    function dataViewQueryString() {
      let query = '';
      if (sortStack.length) {
        query +=
          '&sort=' +
          sortStack
            .map(function (entry) {
              return encodeURIComponent(entry.column) + ':' + entry.direction;
            })
            .join(',');
      }
      const activeFilters = activeFilterEntries();
      if (activeFilters.length) {
        query +=
          '&filter=' +
          activeFilters
            .map(function (entry) {
              return (
                encodeURIComponent(entry.column) + ':' + encodeURIComponent(entry.text)
              );
            })
            .join(',');
      }
      return query;
    }

    function hasActiveFilters() {
      return activeFilterEntries().length > 0;
    }

    function emptyDataStatusText() {
      return hasActiveFilters() ? '一致するデータがありません' : 'データがありません';
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

    function updateFilterIndicator() {
      if (!filterIndicatorEl || !filterIndicatorLabelEl) return;
      const active = activeFilterEntries();
      if (!active.length) {
        filterIndicatorEl.hidden = true;
        filterIndicatorLabelEl.textContent = '';
        return;
      }
      filterIndicatorLabelEl.textContent = active
        .map(function (entry) {
          return entry.column + ': ' + entry.text;
        })
        .join(', ');
      filterIndicatorEl.hidden = false;
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
        '<button type="button" class="db-col-sort-menu-item" data-action="clear" role="menuitem">この列のソートを解除</button>' +
        '<div class="db-col-filter-section">' +
        '<label class="db-col-filter-label" for="db-col-filter-input">フィルター</label>' +
        '<div class="db-col-filter-row">' +
        '<input type="text" class="db-col-filter-input" id="db-col-filter-input" placeholder="部分一致" autocomplete="off">' +
        '<button type="button" class="db-col-filter-clear" data-action="clear-filter">クリア</button>' +
        '</div></div>';
      document.body.appendChild(sortMenuEl);

      const filterInput = sortMenuEl.querySelector('.db-col-filter-input');
      const filterClearBtn = sortMenuEl.querySelector('.db-col-filter-clear');
      const filterSection = sortMenuEl.querySelector('.db-col-filter-section');

      if (filterSection) {
        filterSection.addEventListener('click', function (e) {
          e.stopPropagation();
        });
      }

      if (filterInput) {
        filterInput.addEventListener('keydown', function (e) {
          e.stopPropagation();
          if (e.key !== 'Enter') return;
          e.preventDefault();
          const column = sortMenuColumn;
          if (!column) return;
          applyFilterText(column, filterInput.value);
          closeSortMenu({ skipFilterFlush: true });
        });
      }

      if (filterClearBtn) {
        filterClearBtn.addEventListener('click', function (e) {
          e.preventDefault();
          e.stopPropagation();
          if (!sortMenuColumn || !filterInput) return;
          filterInput.value = '';
        });
      }

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

      const found = stackEntryForColumn(sortStack, column);
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

      const filterInput = menu.querySelector('.db-col-filter-input');
      const filterFound = stackEntryForColumn(filterStack, column);
      if (filterInput) {
        filterInput.value = filterFound ? filterFound.entry.text : '';
      }

      menu.hidden = false;
      if (anchor) {
        anchor.setAttribute('aria-expanded', 'true');
        positionSortMenu(anchor);
      }
    }

    function flushFilterFromMenu() {
      if (!sortMenuEl || sortMenuEl.hidden || !sortMenuColumn) return;
      const filterInput = sortMenuEl.querySelector('.db-col-filter-input');
      if (!filterInput) return;
      applyFilterText(sortMenuColumn, filterInput.value);
    }

    function closeSortMenu(options) {
      if (!sortMenuEl || sortMenuEl.hidden) return;
      const skipFilterFlush = !!(options && options.skipFilterFlush);
      if (!skipFilterFlush) {
        flushFilterFromMenu();
      }
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

    function saveFilter() {
      if (!filterUrl) return Promise.resolve();
      return fetch(filterUrl, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filter: filterStack }),
      }).catch(function () {
        /* 保存失敗はセッション内フィルターに影響しない */
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
      await Promise.all([saveSort(), reloadForViewChange()]);
    }

    async function applySortAction(column, action) {
      if (!column) return;
      const found = stackEntryForColumn(sortStack, column);
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

    async function commitFilterChange() {
      updateFilterIndicator();
      invalidateHeader();
      await Promise.all([saveFilter(), reloadForViewChange()]);
    }

    async function applyFilterText(column, text) {
      if (!column) return;
      const trimmed = (text || '').trim();
      const found = stackEntryForColumn(filterStack, column);
      let changed = false;

      if (trimmed) {
        if (found) {
          if (found.entry.text !== trimmed) {
            found.entry.text = trimmed;
            changed = true;
          }
        } else {
          filterStack.push({ column: column, text: trimmed });
          changed = true;
        }
      } else if (found) {
        filterStack.splice(found.index, 1);
        changed = true;
      }

      if (!changed) return;
      await commitFilterChange();
    }

    async function clearAllFilters() {
      if (!hasActiveFilters()) return;
      filterStack = [];
      await commitFilterChange();
    }

    async function reloadForViewChange() {
      await reloadData({ fullReset: false });
    }

    const AUTO_FIT_SAMPLE_ROWS = 40;

    function currentColumnWidth(index) {
      return columnWidths[index] >= COLUMN_WIDTH_MIN ? columnWidths[index] : 0;
    }

    function resolveAutoFitWidth(index, measuredWidth) {
      const fitted = clampDbColumnWidth(measuredWidth, DB_COL_WIDTH.AUTO_MAX);
      const current = currentColumnWidth(index);
      return current > DB_COL_WIDTH.AUTO_MAX
        ? clampDbColumnWidth(current, DB_COL_WIDTH.MAX)
        : fitted;
    }

    function measureSampleRowCellsHtml(row, columnIndex) {
      if (columnIndex == null) {
        let html = '';
        for (let i = 0; i < columns.length; i++) {
          html += formatCellDisplay(row && i < row.length ? row[i] : null);
        }
        return html;
      }
      if (!row || columnIndex >= row.length) return formatCellDisplay(null);
      return formatCellDisplay(row[columnIndex]);
    }

    function withMeasureTable(tableHtml, callback) {
      const wrapper = document.createElement('div');
      wrapper.className = 'db-table-measure-root';
      wrapper.innerHTML = tableHtml;
      panel.appendChild(wrapper);
      try {
        return callback(wrapper);
      } finally {
        wrapper.remove();
      }
    }

    function buildAutoFitSampleRowsHtml(columnIndex) {
      const sampleCount = Math.min(totalCount, AUTO_FIT_SAMPLE_ROWS);
      let rowsHtml = '';
      for (let rowIndex = 0; rowIndex < sampleCount; rowIndex++) {
        const row = getRow(rowIndex);
        if (!row) continue;
        rowsHtml += '<tr>' + measureSampleRowCellsHtml(row, columnIndex) + '</tr>';
      }
      return rowsHtml;
    }

    function measureColumnContentWidth(index) {
      if (index < 0 || index >= columns.length) return DB_COL_WIDTH.MIN;

      let width = dbTableCellWidth(
        panel,
        columns[index],
        true,
        isPrimaryKeyColumn(columns[index])
      );
      const rowsHtml = buildAutoFitSampleRowsHtml(index);
      if (!rowsHtml) return width;

      return withMeasureTable(
        '<table class="db-table-body-table db-table-measure-table"><tbody>' +
          rowsHtml +
          '</tbody></table>',
        function (wrapper) {
          wrapper.querySelectorAll('td').forEach(function (cell) {
            width = Math.max(width, measureDbCellWidth(cell, false));
          });
          return width;
        }
      );
    }

    function measureAllColumnContentWidths() {
      const colCount = columns.length;
      const widths = new Array(colCount).fill(0);

      let headerRowHtml = '<tr>';
      for (let i = 0; i < colCount; i++) {
        headerRowHtml += dbHeaderMeasureThHtml(
          columns[i],
          isPrimaryKeyColumn(columns[i])
        );
      }
      headerRowHtml += '</tr>';

      withMeasureTable(
        '<table class="db-table-head-table db-table-measure-table"><thead>' +
          headerRowHtml +
          '</thead></table>',
        function (wrapper) {
          wrapper.querySelectorAll('th').forEach(function (cell, i) {
            widths[i] = measureDbCellWidth(cell, true);
          });
        }
      );

      const rowsHtml = buildAutoFitSampleRowsHtml();
      if (rowsHtml) {
        withMeasureTable(
          '<table class="db-table-body-table db-table-measure-table"><tbody>' +
            rowsHtml +
            '</tbody></table>',
          function (wrapper) {
            wrapper.querySelectorAll('tr').forEach(function (rowEl) {
              rowEl.querySelectorAll('td').forEach(function (cell, i) {
                if (i < colCount) {
                  widths[i] = Math.max(widths[i], measureDbCellWidth(cell, false));
                }
              });
            });
          }
        );
      }

      for (let i = 0; i < colCount; i++) {
        if (widths[i] < DB_COL_WIDTH.MIN) widths[i] = DB_COL_WIDTH.MIN;
      }
      return widths;
    }

    function finishAutoFit() {
      columnWidthsApplied = false;
      applyColumnWidthsOnly();
      saveColumnWidths();
    }

    function commitAutoFitWidths(measuredWidths) {
      for (let i = 0; i < columns.length; i++) {
        columnWidths[i] = resolveAutoFitWidth(i, measuredWidths[i]);
      }
      finishAutoFit();
    }

    function autoFitColumnWidth(index) {
      if (index < 0 || index >= columns.length) return;
      columnWidths[index] = resolveAutoFitWidth(
        index,
        measureColumnContentWidth(index)
      );
      finishAutoFit();
    }

    function autoFitAllColumnWidths() {
      if (columns.length === 0) return;
      if (columnWidths.length < columns.length) {
        columnWidths.length = columns.length;
      }
      commitAutoFitWidths(measureAllColumnContentWidths());
    }

    function updateFitAllColumnsButton() {
      if (!fitAllColumnsBtn) return;
      fitAllColumnsBtn.disabled = columns.length === 0;
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
      const filterByColumn = filterIndexMap();
      thead.innerHTML =
        '<tr>' +
        columns
          .map(function (col) {
            const found = sortByColumn.get(col);
            const priority = found ? found.index + 1 : 0;
            const filterEntry = filterByColumn.get(col);
            return dbHeaderCellHtml(
              col,
              found ? found.entry : null,
              priority,
              showPriority,
              isPrimaryKeyColumn(col),
              filterEntry ? filterEntry.text : ''
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
          dataViewQueryString();
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
          filterStack = Array.isArray(data.filter)
            ? data.filter.filter(function (entry) {
                return entry.text;
              })
            : [];
          updateSortIndicator();
          updateFilterIndicator();
          updateFitAllColumnsButton();
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
        setStatus('empty', emptyDataStatusText(), false);
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
        updateFitAllColumnsButton();
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
    if (fitAllColumnsBtn) {
      fitAllColumnsBtn.addEventListener('click', function () {
        autoFitAllColumnWidths();
      });
    }
    if (sortClearBtn) {
      sortClearBtn.addEventListener('click', function (e) {
        e.preventDefault();
        e.stopPropagation();
        clearAllSort();
      });
    }
    if (filterClearBtn) {
      filterClearBtn.addEventListener('click', function (e) {
        e.preventDefault();
        e.stopPropagation();
        clearAllFilters();
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

  function readDbDuplicatePayloads() {
    const el = document.getElementById('db-duplicate-payloads');
    if (!el) {
      return { tables: {}, views: {} };
    }
    try {
      const payload = JSON.parse(el.textContent || '{}');
      return {
        tables: payload.tables || {},
        views: payload.views || {},
      };
    } catch (_err) {
      return { tables: {}, views: {} };
    }
  }

  function duplicateDefaultName(name) {
    return name ? name + '-copy' : '';
  }

  function wireAdminDuplicateDialog(dialog, cancelBtn, onClose) {
    function closeDialog() {
      if (dialog.open) {
        dialog.close();
      }
    }

    if (cancelBtn) {
      cancelBtn.addEventListener('click', closeDialog);
    }

    dialog.addEventListener('cancel', function (event) {
      event.preventDefault();
      closeDialog();
    });

    if (onClose) {
      dialog.addEventListener('close', onClose);
    }

    return closeDialog;
  }

  function initTableDuplicate() {
    const dialog = document.getElementById('table-duplicate-dialog');
    const form = document.getElementById('table-duplicate-form');
    const sourceEl = document.getElementById('table-duplicate-source');
    const targetNameInput = document.getElementById('table-duplicate-target-name');
    const rowsEl = document.getElementById('table-duplicate-column-rows');
    const template = document.getElementById('table-duplicate-column-row-template');
    const addBtn = document.getElementById('table-duplicate-add-column-btn');
    const cancelBtn = document.getElementById('table-duplicate-cancel');
    const includeDataCheckbox = document.getElementById('table-duplicate-include-data');
    if (!dialog || !form || !sourceEl || !targetNameInput || !rowsEl || !template) return;

    const duplicatePayloads = readDbDuplicatePayloads();

    wireAdminDuplicateDialog(dialog, cancelBtn, function () {
      form.reset();
      rowsEl.replaceChildren();
    });

    function bindRow(row) {
      const removeBtn = row.querySelector('.column-remove-btn');
      if (removeBtn) {
        removeBtn.addEventListener('click', function () {
          row.remove();
        });
      }
    }

    function setSelectValue(select, value) {
      if (!select) return;
      const option = Array.from(select.options).find(function (opt) {
        return opt.value === value;
      });
      if (option) {
        select.value = value;
      }
    }

    function addColumnRow(column) {
      const fragment = template.content.cloneNode(true);
      const row = fragment.querySelector('.column-row');
      if (!row) return;

      const nameInput = row.querySelector('input[name="col_name"]');
      const typeSelect = row.querySelector('select[name="col_type"]');
      const nullableSelect = row.querySelector('select[name="col_nullable"]');

      if (column) {
        if (nameInput) nameInput.value = column.name || '';
        setSelectValue(typeSelect, column.type_key || 'text');
        setSelectValue(nullableSelect, column.nullable ? '1' : '0');
      }

      rowsEl.appendChild(fragment);
      bindRow(rowsEl.lastElementChild);
    }

    if (addBtn) {
      addBtn.addEventListener('click', function () {
        addColumnRow(null);
      });
    }

    document.querySelectorAll('[data-table-duplicate]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const tableName = btn.dataset.tableName || '';
        if (!tableName) return;

        form.action =
          '/admin/database/tables/' + encodeURIComponent(tableName) + '/duplicate';
        sourceEl.textContent = '複製元: ' + tableName;
        targetNameInput.value = duplicateDefaultName(tableName);
        rowsEl.replaceChildren();
        if (includeDataCheckbox) {
          includeDataCheckbox.checked = false;
        }

        const columns = duplicatePayloads.tables[tableName] || [];
        if (columns.length === 0) {
          addColumnRow(null);
        } else {
          columns.forEach(function (column) {
            addColumnRow(column);
          });
        }
        dialog.showModal();
        targetNameInput.focus();
        targetNameInput.select();
      });
    });
  }

  function initViewDuplicate() {
    const dialog = document.getElementById('view-duplicate-dialog');
    const form = document.getElementById('view-duplicate-form');
    const sourceEl = document.getElementById('view-duplicate-source');
    const targetNameInput = document.getElementById('view-duplicate-target-name');
    const definitionInput = document.getElementById('view-duplicate-definition');
    const cancelBtn = document.getElementById('view-duplicate-cancel');
    if (!dialog || !form || !sourceEl || !targetNameInput || !definitionInput) return;

    const duplicatePayloads = readDbDuplicatePayloads();

    wireAdminDuplicateDialog(dialog, cancelBtn, function () {
      form.reset();
      definitionInput.value = '';
    });

    document.querySelectorAll('[data-view-duplicate]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const viewName = btn.dataset.viewName || '';
        if (!viewName) return;

        form.action =
          '/admin/database/views/' + encodeURIComponent(viewName) + '/duplicate';
        sourceEl.textContent = '複製元: ' + viewName;
        targetNameInput.value = duplicateDefaultName(viewName);
        definitionInput.value = duplicatePayloads.views[viewName] || '';
        dialog.showModal();
        targetNameInput.focus();
        targetNameInput.select();
      });
    });
  }

  function initLayoutDuplicate() {
    const dialog = document.getElementById('layout-duplicate-dialog');
    const form = document.getElementById('layout-duplicate-form');
    const sourceEl = document.getElementById('layout-duplicate-source');
    const targetKeyInput = document.getElementById('layout-duplicate-target-key');
    const cancelBtn = document.getElementById('layout-duplicate-cancel');
    if (!dialog || !form || !sourceEl || !targetKeyInput) return;

    function closeDialog() {
      if (dialog.open) {
        dialog.close();
      }
    }

    document.querySelectorAll('[data-layout-duplicate]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const layoutId = btn.dataset.layoutId;
        const layoutKey = btn.dataset.layoutKey || '';
        const layoutName = btn.dataset.layoutName || layoutKey;
        if (!layoutId) return;

        form.action = '/admin/layouts/' + layoutId + '/duplicate';
        sourceEl.textContent =
          '複製元: ' + layoutName + '（key: ' + layoutKey + '）';
        targetKeyInput.value = layoutKey ? layoutKey + '-copy' : '';
        dialog.showModal();
        targetKeyInput.focus();
        targetKeyInput.select();
      });
    });

    if (cancelBtn) {
      cancelBtn.addEventListener('click', closeDialog);
    }

    dialog.addEventListener('cancel', function (event) {
      event.preventDefault();
      closeDialog();
    });

    dialog.addEventListener('close', function () {
      form.reset();
    });
  }

  function initViewForm() {
    const form = document.getElementById('view-form');
    const definitionInput = document.getElementById('definition');
    const tabButtons = document.querySelectorAll('.view-form-tabs [data-view-tab]');
    const sqlPanel = document.getElementById('view-tab-sql');
    const uiPanel = document.getElementById('view-tab-ui');
    const baseTableSelect = document.getElementById('view-ui-base-table');
    const columnsWrap = document.getElementById('view-ui-columns-wrap');
    const columnsList = document.getElementById('view-ui-columns');
    const addColumnSelect = document.getElementById('view-ui-add-column-select');
    const addColumnBtn = document.getElementById('view-ui-add-column-btn');
    const unsupportedNotice = document.getElementById('view-ui-unsupported');
    const builderEl = document.getElementById('view-ui-builder');
    const uiError = document.getElementById('view-ui-error');
    const loadingEl = document.getElementById('view-ui-loading');
    const emptyEl = document.getElementById('view-ui-empty');
    const initialEl = document.getElementById('view-ui-initial');

    if (!form || !definitionInput || !tabButtons.length || !sqlPanel || !uiPanel) return;

    let activeTab = 'ui';
    let tableApiColumns = [];
    let dragSourceItem = null;
    let dragGhostEl = null;
    let dragPointerOffset = { x: 0, y: 0 };
    let lastReorderTarget = null;
    let lastReorderBefore = null;
    let emptyDragImage = null;
    let columnNameTooltipEl = null;
    let activeColumnNameTooltipWrap = null;
    const prefersReducedMotion =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    function quoteSqlIdentifier(name) {
      return '"' + String(name).replace(/"/g, '""') + '"';
    }

    function isIdentifierChar(ch) {
      return /[A-Za-z0-9_]/.test(ch);
    }

    function findSqlKeyword(sql, keyword, start) {
      const upper = sql.toUpperCase();
      const kw = keyword.toUpperCase();
      let searchStart = start;
      while (true) {
        const rel = upper.indexOf(kw, searchStart);
        if (rel === -1) return -1;
        const pos = rel;
        const beforeOk = pos === 0 || !isIdentifierChar(upper.charAt(pos - 1));
        const afterPos = pos + kw.length;
        const afterOk = afterPos >= upper.length || !isIdentifierChar(upper.charAt(afterPos));
        if (beforeOk && afterOk) return pos;
        searchStart = pos + 1;
      }
    }

    function containsUnsupportedTrailingClause(remainder) {
      const clauses = [
        'JOIN',
        'GROUP',
        'HAVING',
        'ORDER',
        'LIMIT',
        'UNION',
        'EXCEPT',
        'INTERSECT',
      ];
      return clauses.some(function (clause) {
        return findSqlKeyword(remainder, clause, 0) !== -1;
      });
    }

    function hasUnclosedStringLiteral(input) {
      let inSingleQuote = false;
      for (let i = 0; i < input.length; i += 1) {
        if (input.charAt(i) === "'") {
          inSingleQuote = !inSingleQuote;
        }
      }
      return inSingleQuote;
    }

    function findTopLevelKeyword(input, keyword) {
      let inSingleQuote = false;
      let i = 0;
      while (i < input.length) {
        const ch = input.charAt(i);
        if (ch === "'") {
          inSingleQuote = !inSingleQuote;
          i += 1;
          continue;
        }
        if (!inSingleQuote && findSqlKeyword(input.slice(i), keyword, 0) === 0) {
          return i;
        }
        i += 1;
      }
      return -1;
    }

    function splitTopLevelAnd(input) {
      if (hasUnclosedStringLiteral(input)) return null;
      const parts = [];
      let segmentStart = 0;
      let i = 0;
      while (true) {
        const andPos = findTopLevelKeyword(input.slice(i), 'AND');
        if (andPos === -1) break;
        const splitAt = i + andPos;
        const segment = input.slice(segmentStart, splitAt).trim();
        if (!segment) return null;
        parts.push(segment);
        i = splitAt + 'AND'.length;
        while (i < input.length && /\s/.test(input.charAt(i))) {
          i += 1;
        }
        segmentStart = i;
      }
      const last = input.slice(segmentStart).trim();
      if (!last) {
        return parts.length > 0 ? parts : null;
      }
      parts.push(last);
      return parts;
    }

    function parseColumnWhereCondition(part) {
      const parsed = parseSqlIdentifierPrefix(part.trim());
      if (!parsed) return null;
      const suffix = parsed.rest.trim();
      if (!suffix) return null;
      return { column: parsed.name, suffix: suffix };
    }

    function parseSimpleWhereConditions(wherePart) {
      if (findTopLevelKeyword(wherePart, 'OR') !== -1) return null;
      const parts = splitTopLevelAnd(wherePart);
      if (!parts) return null;
      const conditions = [];
      for (let idx = 0; idx < parts.length; idx += 1) {
        const parsed = parseColumnWhereCondition(parts[idx]);
        if (!parsed) return null;
        conditions.push(parsed);
      }
      return conditions;
    }

    function parseOptionalWhereConditions(afterTable) {
      if (!afterTable) return [];
      const wherePos = findSqlKeyword(afterTable, 'WHERE', 0);
      if (wherePos === -1) return null;
      if (wherePos !== 0) return null;
      const wherePart = afterTable.slice('WHERE'.length).trim();
      if (!wherePart) return null;
      return parseSimpleWhereConditions(wherePart);
    }

    function assignWhereConditionsToSelectColumns(columns, whereConditions) {
      const used = whereConditions.map(function () {
        return false;
      });
      return columns.map(function (item) {
        for (let i = 0; i < whereConditions.length; i += 1) {
          if (!used[i] && whereConditions[i].column === item.name) {
            used[i] = true;
            return whereConditions[i].suffix;
          }
        }
        return null;
      });
    }

    function parseSqlIdentifierPrefix(input) {
      input = input.trim();
      if (!input) return null;
      if (input.charAt(0) === '"') {
        let i = 1;
        let name = '';
        while (i < input.length) {
          if (input.charAt(i) === '"') {
            if (i + 1 < input.length && input.charAt(i + 1) === '"') {
              name += '"';
              i += 2;
            } else {
              return { name: name, rest: input.slice(i + 1) };
            }
          } else {
            name += input.charAt(i);
            i += 1;
          }
        }
        return null;
      }
      const match = input.match(/^([A-Za-z_][A-Za-z0-9_]*)/);
      if (!match) return null;
      return { name: match[1], rest: input.slice(match[1].length) };
    }

    function parseCommaSeparatedSelectItems(input) {
      const items = [];
      let rest = input.trim();
      while (rest) {
        const parsed = parseSqlIdentifierPrefix(rest);
        if (!parsed) return null;
        let alias = null;
        rest = parsed.rest.trim();
        if (findSqlKeyword(rest, 'AS', 0) === 0) {
          const afterAs = rest.slice('AS'.length).trim();
          const aliasParsed = parseSqlIdentifierPrefix(afterAs);
          if (!aliasParsed) return null;
          alias = aliasParsed.name;
          rest = aliasParsed.rest.trim();
        }
        items.push({ name: parsed.name, alias: alias });
        if (!rest) break;
        if (!rest.startsWith(',')) return null;
        rest = rest.slice(1).trim();
      }
      return items;
    }

    function effectiveViewColumnName(column) {
      const alias =
        column.alias != null ? String(column.alias).trim() : '';
      return alias || column.name;
    }

    function selectOutputNamesUnique(columns) {
      const seen = new Set();
      for (let i = 0; i < columns.length; i += 1) {
        const outputName = effectiveViewColumnName(columns[i]);
        if (seen.has(outputName)) return false;
        seen.add(outputName);
      }
      return true;
    }

    function validateViewColumnAlias(alias) {
      const trimmed = String(alias).trim();
      if (!trimmed) return null;
      if (trimmed.length > 120) {
        return '別名は 120 文字以内で指定してください';
      }
      for (let i = 0; i < trimmed.length; i += 1) {
        const ch = trimmed.charAt(i);
        const code = trimmed.charCodeAt(i);
        if (code < 32 || ch === '"' || ch === ';') {
          return '別名に使用できない文字が含まれています';
        }
      }
      return null;
    }

    function validateViewOutputColumnNames(columns) {
      const seen = new Set();
      for (let i = 0; i < columns.length; i += 1) {
        const column = columns[i];
        const alias =
          column.alias != null ? String(column.alias).trim() : '';
        if (alias) {
          const aliasError = validateViewColumnAlias(alias);
          if (aliasError) return aliasError;
        }
        const outputName = effectiveViewColumnName(column);
        if (seen.has(outputName)) {
          return '出力列名が重複しています。別名で区別してください。';
        }
        seen.add(outputName);
      }
      return null;
    }

    function formatSimpleViewSelectColumn(column) {
      const quoted = quoteSqlIdentifier(column.name);
      const alias =
        column.alias != null ? String(column.alias).trim() : '';
      if (!alias) return quoted;
      return quoted + ' AS ' + quoteSqlIdentifier(alias);
    }

    function parseSimpleSelect(definition) {
      const trimmed = definition.trim();
      if (!trimmed.toUpperCase().startsWith('SELECT')) return null;
      const fromPos = findSqlKeyword(trimmed, 'FROM', 'SELECT'.length);
      if (fromPos === -1) return null;
      const selectPart = trimmed.slice('SELECT'.length, fromPos).trim();
      const afterFrom = trimmed.slice(fromPos + 'FROM'.length).trim();
      if (!afterFrom) return null;
      const tableParsed = parseSqlIdentifierPrefix(afterFrom);
      if (!tableParsed) return null;
      const afterTable = tableParsed.rest.trim();
      if (containsUnsupportedTrailingClause(afterTable)) return null;
      const whereConditions = parseOptionalWhereConditions(afterTable);
      if (whereConditions === null) return null;
      if (selectPart === '*') {
        return {
          baseTable: tableParsed.name,
          allColumns: true,
          columns: [],
          whereConditions: whereConditions,
        };
      }
      const columns = parseCommaSeparatedSelectItems(selectPart);
      if (!columns || columns.length === 0) return null;
      if (!selectOutputNamesUnique(columns)) return null;
      return {
        baseTable: tableParsed.name,
        allColumns: false,
        columns: columns,
        whereConditions: whereConditions,
      };
    }

    function hideUiError() {
      if (!uiError) return;
      uiError.hidden = true;
      uiError.textContent = '';
    }

    function showUiError(message) {
      if (!uiError) return;
      uiError.textContent = message;
      uiError.hidden = false;
    }

    function parseVisualState(definition) {
      const trimmed = definition.trim();
      if (!trimmed) {
        return { supported: true, parsed: null };
      }
      const parsed = parseSimpleSelect(trimmed);
      return { supported: parsed !== null, parsed: parsed };
    }

    function isVisualEditingSupported() {
      return parseVisualState(definitionInput.value).supported;
    }

    function setVisualBuilderVisible(visible) {
      if (builderEl) builderEl.hidden = !visible;
      if (unsupportedNotice) unsupportedNotice.hidden = visible;
    }

    function setActiveTab(tab) {
      activeTab = tab;
      tabButtons.forEach(function (btn) {
        btn.classList.toggle('active', btn.dataset.viewTab === tab);
      });
      sqlPanel.hidden = tab !== 'sql';
      uiPanel.hidden = tab !== 'ui';
      if (tab === 'ui') {
        hideUiError();
      }
    }

    function readColumnState() {
      if (!columnsList) return [];
      return Array.from(columnsList.querySelectorAll('.view-ui-column-item')).map(function (item) {
        const aliasInput = item.querySelector('.view-ui-column-alias-input');
        const whereInput = item.querySelector('.view-ui-column-where-input');
        const aliasValue = aliasInput ? aliasInput.value.trim() : '';
        const whereValue = whereInput ? whereInput.value.trim() : '';
        return {
          name: item.dataset.columnName || '',
          type_key: item.dataset.columnType || 'text',
          alias: aliasValue || null,
          where_condition: whereValue || null,
        };
      });
    }

    function columnTypeByName(apiColumns) {
      const typeByName = {};
      apiColumns.forEach(function (column) {
        typeByName[column.name] = column.type_key;
      });
      return typeByName;
    }

    function toUiColumn(column, typeByName) {
      const aliasValue = column.alias != null ? String(column.alias).trim() : '';
      const whereValue =
        column.where_condition != null ? String(column.where_condition).trim() : '';
      return {
        name: column.name,
        type_key: typeByName[column.name] || column.type_key || 'text',
        alias: aliasValue || null,
        where_condition: whereValue || null,
      };
    }

    function buildColumnsFromParsed(apiColumns, parsed) {
      const typeByName = columnTypeByName(apiColumns);
      const items = parsed.allColumns
        ? apiColumns.map(function (column) {
            return { name: column.name, alias: null };
          })
        : parsed.columns;
      const whereConditions = Array.isArray(parsed.whereConditions)
        ? parsed.whereConditions
        : [];
      const assignedWhere = assignWhereConditionsToSelectColumns(items, whereConditions);
      return items.map(function (item, index) {
        return toUiColumn(
          {
            name: item.name,
            type_key: typeByName[item.name],
            alias: item.alias,
            where_condition: assignedWhere[index] || null,
          },
          typeByName
        );
      });
    }

    function normalizeColumnState(apiColumns, state) {
      const typeByName = columnTypeByName(apiColumns);
      const apiNames = new Set(
        apiColumns.map(function (column) {
          return column.name;
        })
      );
      return state
        .filter(function (column) {
          return column.included !== false;
        })
        .filter(function (column) {
          return apiNames.has(column.name);
        })
        .map(function (column) {
          return toUiColumn(column, typeByName);
        });
    }

    function updateColumnsWrapVisibility() {
      const hasColumns = !!columnsList && columnsList.children.length > 0;
      if (columnsWrap) columnsWrap.hidden = !hasColumns;
      if (emptyEl) emptyEl.hidden = hasColumns;
    }

    function beginColumnsLoad() {
      if (loadingEl) loadingEl.hidden = false;
      if (columnsWrap) columnsWrap.hidden = true;
      if (emptyEl) emptyEl.hidden = true;
      hideUiError();
    }

    function finishColumnsLoad() {
      if (loadingEl) loadingEl.hidden = true;
    }

    function fetchTableColumns(tableName) {
      return fetch(
        '/admin/database/tables/' + encodeURIComponent(tableName) + '/columns'
      )
        .then(function (response) {
          if (!response.ok) {
            throw new Error('カラム一覧の取得に失敗しました');
          }
          return response.json();
        })
        .then(function (data) {
          return Array.isArray(data.columns) ? data.columns : [];
        });
    }

    function ensureColumnNameTooltip() {
      if (!columnNameTooltipEl) {
        columnNameTooltipEl = document.createElement('span');
        columnNameTooltipEl.className = 'view-ui-column-name-tooltip';
        columnNameTooltipEl.setAttribute('role', 'tooltip');
        columnNameTooltipEl.hidden = true;
        document.body.appendChild(columnNameTooltipEl);
      }
      return columnNameTooltipEl;
    }

    function updateColumnNameTooltip(nameWrap) {
      const nameLabel = nameWrap.querySelector('.view-ui-column-name');
      if (!nameLabel) return;
      const truncated = nameLabel.scrollWidth > nameLabel.clientWidth + 1;
      nameWrap.classList.toggle('is-truncated', truncated);
    }

    function updateAllColumnNameTooltips() {
      requestAnimationFrame(function () {
        requestAnimationFrame(function () {
          if (!columnsList) return;
          columnsList.querySelectorAll('.view-ui-column-name-wrap').forEach(function (wrap) {
            updateColumnNameTooltip(wrap);
          });
        });
      });
    }

    function positionColumnNameTooltip(nameWrap) {
      const tooltip = ensureColumnNameTooltip();
      const rect = nameWrap.getBoundingClientRect();
      tooltip.style.left = rect.left + 'px';
      tooltip.style.top = '-9999px';
      tooltip.style.visibility = 'hidden';
      const tooltipHeight = tooltip.offsetHeight;
      const top = Math.max(8, rect.top - tooltipHeight - 6);
      tooltip.style.top = top + 'px';
      tooltip.style.visibility = '';
    }

    function showColumnNameTooltip(nameWrap) {
      if (!nameWrap.classList.contains('is-truncated')) return;
      const nameLabel = nameWrap.querySelector('.view-ui-column-name');
      if (!nameLabel) return;
      const tooltip = ensureColumnNameTooltip();
      tooltip.textContent = nameLabel.textContent;
      tooltip.hidden = false;
      activeColumnNameTooltipWrap = nameWrap;
      positionColumnNameTooltip(nameWrap);
      tooltip.classList.add('is-visible');
    }

    function hideColumnNameTooltip() {
      if (!columnNameTooltipEl) return;
      columnNameTooltipEl.classList.remove('is-visible');
      columnNameTooltipEl.hidden = true;
      activeColumnNameTooltipWrap = null;
    }

    function bindColumnNameTooltip(nameWrap) {
      nameWrap.addEventListener('mouseenter', function () {
        showColumnNameTooltip(nameWrap);
      });
      nameWrap.addEventListener('mouseleave', function () {
        if (activeColumnNameTooltipWrap === nameWrap) {
          hideColumnNameTooltip();
        }
      });
    }

    function updateAddColumnSelect() {
      if (!addColumnSelect) return;

      addColumnSelect.replaceChildren();
      const emptyOption = document.createElement('option');
      emptyOption.value = '';
      emptyOption.textContent =
        tableApiColumns.length === 0 ? 'カラムがありません' : 'カラムを選択';
      addColumnSelect.appendChild(emptyOption);
      tableApiColumns.forEach(function (column) {
        const option = document.createElement('option');
        option.value = column.name;
        option.textContent = column.name;
        addColumnSelect.appendChild(option);
      });

      if (addColumnBtn) {
        addColumnBtn.disabled = tableApiColumns.length === 0;
      }
    }

    function captureColumnItemTops(items) {
      const tops = new Map();
      items.forEach(function (item) {
        tops.set(item, item.getBoundingClientRect().top);
      });
      return tops;
    }

    function clearColumnItemMotionStyles(item) {
      item.style.transform = '';
      item.style.transition = '';
    }

    function animateColumnReorder(items, oldTops) {
      if (prefersReducedMotion) return;
      const duration = '240ms';
      const easing = 'cubic-bezier(0.25, 0.8, 0.25, 1)';
      items.forEach(function (item) {
        if (item.classList.contains('is-dragging')) return;
        const oldTop = oldTops.get(item);
        if (oldTop == null) return;
        const delta = oldTop - item.getBoundingClientRect().top;
        if (Math.abs(delta) < 0.5) return;
        clearColumnItemMotionStyles(item);
        item.style.transform = 'translateY(' + delta + 'px)';
        requestAnimationFrame(function () {
          requestAnimationFrame(function () {
            item.style.transition = 'transform ' + duration + ' ' + easing;
            item.style.transform = '';
            item.addEventListener(
              'transitionend',
              function onTransitionEnd(event) {
                if (event.propertyName !== 'transform') return;
                item.removeEventListener('transitionend', onTransitionEnd);
                clearColumnItemMotionStyles(item);
              }
            );
          });
        });
      });
    }

    function moveDraggedColumn(targetItem, before) {
      if (!dragSourceItem || !columnsList || dragSourceItem === targetItem) return false;
      const referenceNode = before ? targetItem : targetItem.nextSibling;
      if (dragSourceItem === referenceNode) return false;
      const items = Array.from(columnsList.querySelectorAll('.view-ui-column-item'));
      const oldTops = captureColumnItemTops(items);
      columnsList.insertBefore(dragSourceItem, referenceNode);
      animateColumnReorder(items, oldTops);
      return true;
    }

    function resetDragReorderState() {
      lastReorderTarget = null;
      lastReorderBefore = null;
    }

    function hideNativeDragImage(event) {
      if (!emptyDragImage) {
        emptyDragImage = new Image();
        emptyDragImage.src =
          'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
      }
      event.dataTransfer.setDragImage(emptyDragImage, 0, 0);
    }

    function updateDragGhostPosition(event) {
      if (!dragGhostEl) return;
      const x = event.clientX;
      const y = event.clientY;
      if (x === 0 && y === 0) return;
      dragGhostEl.style.left = x - dragPointerOffset.x + 'px';
      dragGhostEl.style.top = y - dragPointerOffset.y + 'px';
    }

    function onDocumentDragOver(event) {
      if (!dragGhostEl) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      updateDragGhostPosition(event);
    }

    function removeDragGhost() {
      document.removeEventListener('dragover', onDocumentDragOver);
      if (dragGhostEl) {
        dragGhostEl.remove();
        dragGhostEl = null;
      }
    }

    function createDragGhost(item, event, dragAnchor) {
      removeDragGhost();
      const anchor = dragAnchor || item;
      const anchorRect = anchor.getBoundingClientRect();
      const itemRect = item.getBoundingClientRect();
      dragPointerOffset = {
        x: event.clientX - anchorRect.left,
        y: event.clientY - anchorRect.top,
      };
      dragGhostEl = item.cloneNode(true);
      dragGhostEl.classList.add('view-ui-column-drag-ghost');
      dragGhostEl.classList.remove('is-dragging');
      dragGhostEl.querySelectorAll('[draggable]').forEach(function (el) {
        el.removeAttribute('draggable');
      });
      dragGhostEl.style.width = itemRect.width + 'px';
      document.body.appendChild(dragGhostEl);
      updateDragGhostPosition(event);
      document.addEventListener('dragover', onDocumentDragOver);
    }

    function finishColumnDrag(item) {
      removeDragGhost();
      if (item) item.classList.remove('is-dragging');
      if (columnsList) columnsList.classList.remove('is-drag-active');
      if (columnsList) {
        columnsList.querySelectorAll('.view-ui-column-item').forEach(function (row) {
          clearColumnItemMotionStyles(row);
        });
      }
      dragSourceItem = null;
      resetDragReorderState();
    }

    function bindColumnListInteractions() {
      if (!columnsList) return;

      columnsList.addEventListener('dragstart', function (event) {
        const dragZone = event.target.closest('.view-ui-column-drag-zone');
        if (!dragZone || !columnsList.contains(dragZone)) return;
        const item = dragZone.closest('.view-ui-column-item');
        if (!item) return;
        dragSourceItem = item;
        resetDragReorderState();
        hideColumnNameTooltip();
        item.classList.add('is-dragging');
        columnsList.classList.add('is-drag-active');
        hideNativeDragImage(event);
        createDragGhost(item, event, dragZone);
        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData('text/plain', item.dataset.columnName || '');
      });

      columnsList.addEventListener('dragend', function (event) {
        const dragZone = event.target.closest('.view-ui-column-drag-zone');
        if (!dragZone || !columnsList.contains(dragZone)) return;
        finishColumnDrag(dragZone.closest('.view-ui-column-item'));
      });

      columnsList.addEventListener('dragover', function (event) {
        const item = event.target.closest('.view-ui-column-item');
        if (!item || !columnsList.contains(item)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        if (!dragSourceItem || dragSourceItem === item) return;
        const rect = item.getBoundingClientRect();
        const before = event.clientY < rect.top + rect.height / 2;
        if (lastReorderTarget === item && lastReorderBefore === before) return;
        if (moveDraggedColumn(item, before)) {
          lastReorderTarget = item;
          lastReorderBefore = before;
        }
      });

      columnsList.addEventListener('drop', function (event) {
        if (!event.target.closest('.view-ui-column-item')) return;
        event.preventDefault();
      });

      columnsList.addEventListener('click', function (event) {
        const removeBtn = event.target.closest('.view-ui-column-remove-btn');
        if (!removeBtn || !columnsList.contains(removeBtn)) return;
        const item = removeBtn.closest('.view-ui-column-item');
        if (!item) return;
        item.remove();
        updateAddColumnSelect();
        updateColumnsWrapVisibility();
      });
    }

    function createColumnRow(column) {
      const item = document.createElement('li');
      item.className = 'view-ui-column-item';
      item.dataset.columnName = column.name;
      item.dataset.columnType = column.type_key || 'text';

      const dragZone = document.createElement('div');
      dragZone.className = 'view-ui-column-drag-zone';
      dragZone.draggable = true;
      dragZone.setAttribute('aria-label', 'カラムの並び順を変更');

      const handle = document.createElement('span');
      handle.className = 'view-ui-drag-handle';
      handle.setAttribute('aria-hidden', 'true');
      handle.textContent = '⠿';

      const nameWrap = document.createElement('span');
      nameWrap.className = 'view-ui-column-name-wrap';

      const nameLabel = document.createElement('span');
      nameLabel.className = 'view-ui-column-name';
      nameLabel.textContent = column.name;

      nameWrap.appendChild(nameLabel);
      bindColumnNameTooltip(nameWrap);
      updateColumnNameTooltip(nameWrap);

      dragZone.appendChild(handle);
      dragZone.appendChild(nameWrap);

      const aliasWrap = document.createElement('div');
      aliasWrap.className = 'view-ui-column-alias';
      const aliasInput = document.createElement('input');
      aliasInput.type = 'text';
      aliasInput.className = 'view-ui-column-alias-input';
      aliasInput.placeholder = '別名（任意）';
      aliasInput.value = column.alias || '';
      aliasWrap.appendChild(aliasInput);

      const whereWrap = document.createElement('div');
      whereWrap.className = 'view-ui-column-where';
      const whereInput = document.createElement('input');
      whereInput.type = 'text';
      whereInput.className = 'view-ui-column-where-input';
      whereInput.placeholder = '例: IS NOT NULL';
      whereInput.value = column.where_condition || '';
      whereWrap.appendChild(whereInput);

      const removeBtn = document.createElement('button');
      removeBtn.type = 'button';
      removeBtn.className = 'view-ui-icon-btn view-ui-column-remove-btn';
      removeBtn.setAttribute('aria-label', 'カラムを削除');
      removeBtn.textContent = '−';

      item.appendChild(dragZone);
      item.appendChild(aliasWrap);
      item.appendChild(whereWrap);
      item.appendChild(removeBtn);
      return item;
    }

    function renderColumns(columns) {
      if (!columnsList) return;
      const fragment = document.createDocumentFragment();
      columns.forEach(function (column) {
        fragment.appendChild(createColumnRow(column));
      });
      columnsList.replaceChildren(fragment);
      updateAllColumnNameTooltips();
      updateAddColumnSelect();
      updateColumnsWrapVisibility();
    }

    function syncUiToSql() {
      if (!baseTableSelect) return;
      const table = baseTableSelect.value;
      if (!table) return;
      const columns = readColumnState();
      if (columns.length === 0) return;
      const columnSql = columns
        .map(formatSimpleViewSelectColumn)
        .join(', ');
      const whereParts = columns
        .filter(function (column) {
          return column.where_condition;
        })
        .map(function (column) {
          return quoteSqlIdentifier(column.name) + ' ' + column.where_condition;
        });
      let sql = 'SELECT ' + columnSql + ' FROM ' + quoteSqlIdentifier(table);
      if (whereParts.length > 0) {
        sql += ' WHERE ' + whereParts.join(' AND ');
      }
      definitionInput.value = sql;
    }

    function resolveColumnsFromApi(apiColumns, columnState, parsed) {
      if (parsed) {
        return buildColumnsFromParsed(apiColumns, parsed);
      }
      if (columnState && columnState.length > 0) {
        return normalizeColumnState(apiColumns, columnState);
      }
      return apiColumns.map(function (column) {
        return {
          name: column.name,
          type_key: column.type_key,
          alias: null,
          where_condition: null,
        };
      });
    }

    function loadColumnsForTable(tableName, columnState, parsed) {
      if (!tableName) {
        tableApiColumns = [];
        renderColumns([]);
        updateColumnsWrapVisibility();
        return Promise.resolve();
      }

      beginColumnsLoad();
      return fetchTableColumns(tableName)
        .then(function (apiColumns) {
          tableApiColumns = apiColumns;
          renderColumns(resolveColumnsFromApi(apiColumns, columnState, parsed));
        })
        .catch(function (err) {
          tableApiColumns = [];
          renderColumns([]);
          showUiError(err.message || 'カラム一覧の取得に失敗しました');
        })
        .finally(finishColumnsLoad);
    }

    function applyParsedUiState(parsed) {
      if (!baseTableSelect) return Promise.resolve();
      baseTableSelect.value = parsed.baseTable;
      return loadColumnsForTable(parsed.baseTable, null, parsed);
    }

    function applyUiSpec(spec) {
      if (!spec || !spec.base_table) return Promise.resolve();
      if (baseTableSelect) baseTableSelect.value = spec.base_table;
      return loadColumnsForTable(spec.base_table, spec.columns || []);
    }

    function refreshVisualTabState() {
      setActiveTab('ui');
      const visualState = parseVisualState(definitionInput.value);
      if (!visualState.supported) {
        setVisualBuilderVisible(false);
        return;
      }
      setVisualBuilderVisible(true);
      if (visualState.parsed) {
        applyParsedUiState(visualState.parsed).catch(function (err) {
          showUiError(err.message || 'ビジュアル編集状態の復元に失敗しました');
        });
      }
    }

    function switchToUiTab() {
      refreshVisualTabState();
    }

    function switchToSqlTab() {
      syncUiToSql();
      if (unsupportedNotice) unsupportedNotice.hidden = true;
      setActiveTab('sql');
    }

    tabButtons.forEach(function (btn) {
      btn.addEventListener('click', function () {
        const tab = btn.dataset.viewTab;
        if (tab === 'ui') {
          switchToUiTab();
        } else {
          switchToSqlTab();
        }
      });
    });

    if (baseTableSelect) {
      baseTableSelect.addEventListener('change', function () {
        if (unsupportedNotice) unsupportedNotice.hidden = true;
        hideUiError();
        loadColumnsForTable(baseTableSelect.value, null);
      });
    }

    bindColumnListInteractions();

    window.addEventListener('resize', function () {
      updateAllColumnNameTooltips();
      if (activeColumnNameTooltipWrap) {
        positionColumnNameTooltip(activeColumnNameTooltipWrap);
      }
    });
    window.addEventListener('scroll', function () {
      if (activeColumnNameTooltipWrap) {
        positionColumnNameTooltip(activeColumnNameTooltipWrap);
      }
    }, true);

    if (addColumnBtn) {
      addColumnBtn.addEventListener('click', function () {
        const name = addColumnSelect ? addColumnSelect.value : '';
        if (!name || !columnsList) return;
        const column = tableApiColumns.find(function (entry) {
          return entry.name === name;
        });
        if (!column) return;
        columnsList.appendChild(
          createColumnRow({
            name: column.name,
            type_key: column.type_key,
            alias: null,
            where_condition: null,
          })
        );
        if (addColumnSelect) addColumnSelect.value = '';
        updateAllColumnNameTooltips();
        updateAddColumnSelect();
        updateColumnsWrapVisibility();
      });
    }

    form.addEventListener('submit', function (event) {
      if (activeTab !== 'ui' || !isVisualEditingSupported()) return;
      hideUiError();
      const table = baseTableSelect ? baseTableSelect.value : '';
      if (!table) {
        event.preventDefault();
        showUiError('元テーブルを選択してください。');
        return;
      }
      const columns = readColumnState();
      if (columns.length === 0) {
        event.preventDefault();
        showUiError('ビューに含めるカラムを1つ以上選択してください。');
        return;
      }
      const outputNameError = validateViewOutputColumnNames(columns);
      if (outputNameError) {
        event.preventDefault();
        showUiError(outputNameError);
        return;
      }
      syncUiToSql();
    });

    if (initialEl && initialEl.textContent.trim()) {
      try {
        const initial = JSON.parse(initialEl.textContent);
        if (initial && initial.base_table) {
          setActiveTab('ui');
          if (isVisualEditingSupported()) {
            setVisualBuilderVisible(true);
            applyUiSpec(initial);
          } else {
            setVisualBuilderVisible(false);
          }
          return;
        }
      } catch (_err) {
        /* ignore invalid initial state */
      }
    }
    refreshVisualTabState();
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
    initTableData();
    initWidgetConfig();
    initDatabaseIndex();
    initTableDuplicate();
    initViewDuplicate();
    initViewForm();
    initLayoutDuplicate();
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
