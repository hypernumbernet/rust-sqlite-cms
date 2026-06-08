(function () {
  'use strict';

  function escapeHtml(text) {
    return String(text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  const CELL_DISPLAY_MAX = 20;

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
    HANDLE: 12,
  };

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
  function dbTableCellWidth(panel, text, isHeader) {
    const tableClass = isHeader ? 'db-table-head-table' : 'db-table-body-table';
    const rowHtml = isHeader
      ? '<tr><th><span class="text-mono">' +
        escapeHtml(String(text)) +
        '</span><span class="db-col-resize-handle" aria-hidden="true"></span></th></tr>'
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
        (isHeader ? DB_COL_WIDTH.HANDLE : 0)
    );
    wrapper.remove();
    return width;
  }

  function dbHeaderCellHtml(columnName) {
    return (
      '<th><span class="text-mono">' +
      escapeHtml(columnName) +
      '</span><span class="db-col-resize-handle" role="separator" aria-orientation="vertical" aria-label="' +
      escapeHtml(columnName) +
      ' 列幅変更"></span></th>'
    );
  }

  function formatCellDisplay(text) {
    const raw = String(text);
    const truncated = raw.length > CELL_DISPLAY_MAX;
    const display = truncated
      ? raw.slice(0, CELL_DISPLAY_MAX) + '...'
      : raw;
    const titleAttr = truncated
      ? ' title="' + escapeHtml(raw) + '"'
      : '';
    return '<td class="text-mono-cell"' + titleAttr + '>' + escapeHtml(display) + '</td>';
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
    const statusEl = document.getElementById('db-data-status');
    const statusTextEl = statusEl ? statusEl.querySelector('.db-data-status-text') : null;
    const countEl = document.getElementById('db-data-row-goto');
    const rowGotoDialog = document.getElementById('db-row-goto-dialog');
    const rowGotoForm = document.getElementById('db-row-goto-form');
    const rowGotoInput = document.getElementById('db-row-goto-input');
    const rowGotoRange = document.getElementById('db-row-goto-range');
    const rowGotoCancel = document.getElementById('db-row-goto-cancel');
    const thead = document.getElementById('db-table-head');
    const tbody = document.getElementById('db-table-body');
    const emptyEl = document.getElementById('db-table-empty');
    const apiUrl = panel.dataset.apiUrl || '';
    const columnWidthsUrl = apiUrl.replace(/\/rows$/, '/column-widths');
    const chunkSize = parseInt(panel.dataset.chunkSize || '1000', 10);
    const overscan = parseInt(panel.dataset.overscan || '3', 10);
    const COLUMN_WIDTH_MIN = 40;

    let generation = 0;
    let abortController = null;
    const cache = new Map();
    const inFlight = new Map();
    let columns = [];
    let totalCount = 0;
    let columnsRendered = false;
    let chunkSizeActual = chunkSize;
    let startIndex = 0;
    let visibleCount = 0;
    let rowHeight = 0;
    let isSyncingScroll = false;
    let scrollRaf = 0;
    let renderPending = false;
    let needsRefresh = false;
    let wheelAccumPx = 0;
    let lastSyncedScrollTop = -1;
    let savedColumnWidths = null;
    let columnWidths = [];
    let activeResize = null;

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

    function submitRowGoto(e) {
      e.preventDefault();
      if (!rowGotoInput) return;
      const rowNum = parseInt(rowGotoInput.value, 10);
      if (!Number.isFinite(rowNum) || rowNum < 1 || rowNum > totalCount) {
        rowGotoInput.setCustomValidity('1 〜 ' + totalCount + ' の整数を入力してください');
        rowGotoInput.reportValidity();
        return;
      }
      rowGotoInput.setCustomValidity('');
      scrollToStartIndex(Math.min(rowNum - 1, maxStartIndex()));
      closeRowGotoDialog();
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
      const headTable = panel.querySelector('.db-table-head-table');
      const bodyTable = panel.querySelector('.db-table-body-table');
      if (!headTable || !bodyTable || widths.length === 0) return;

      [headTable, bodyTable].forEach(function (table) {
        const colgroup = ensureColgroup(table, widths.length);
        for (let i = 0; i < widths.length; i++) {
          colgroup.children[i].style.width = Math.round(widths[i]) + 'px';
        }
      });
    }

    function getRow(rowIndex) {
      const chunkOffset = Math.floor(rowIndex / chunkSizeActual) * chunkSizeActual;
      const chunk = cache.get(chunkOffset);
      if (!chunk) return null;
      return chunk[rowIndex - chunkOffset] || null;
    }

    function chunkOffsetsForRange(start, count) {
      if (totalCount === 0 || count <= 0) return [];
      const end = Math.min(start + count - 1, totalCount - 1);
      const firstChunk = Math.floor(start / chunkSizeActual) * chunkSizeActual;
      const lastChunk = Math.floor(end / chunkSizeActual) * chunkSizeActual;
      const offsets = [];
      for (let offset = firstChunk; offset <= lastChunk; offset += chunkSizeActual) {
        offsets.push(offset);
      }
      return offsets;
    }

    function renderHeader() {
      if (!thead || columnsRendered || columns.length === 0) return;
      thead.innerHTML =
        '<tr>' +
        columns
          .map(function (col) {
            return (
              '<th><span class="text-mono">' +
              escapeHtml(col) +
              '</span><span class="db-col-resize-handle" role="separator" aria-orientation="vertical" aria-label="' +
              escapeHtml(col) +
              ' 列幅変更"></span></th>'
            );
          })
          .join('') +
        '</tr>';
      columnsRendered = true;
    }

    function clearInlineWidths(cells) {
      for (let i = 0; i < cells.length; i++) {
        cells[i].style.width = '';
        cells[i].style.minWidth = '';
        cells[i].style.maxWidth = '';
      }
    }

    function clearColgroups() {
      const headTable = panel.querySelector('.db-table-head-table');
      const bodyTable = panel.querySelector('.db-table-body-table');
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
      const headTable = panel.querySelector('.db-table-head-table');
      const bodyTable = panel.querySelector('.db-table-body-table');
      if (!headTable || !bodyTable || columnWidths.length === 0) return;

      headTable.style.tableLayout = 'fixed';
      bodyTable.style.tableLayout = 'fixed';
      updateColgroups(columnWidths);
      setTableTotalWidth(headTable, bodyTable, columnWidths);
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
        applyColumnWidthsOnly();
        return;
      }

      const headRow = thead ? thead.querySelector('tr') : null;
      const headTable = panel.querySelector('.db-table-head-table');
      const bodyTable = panel.querySelector('.db-table-body-table');
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
      const handle = e.target.closest('.db-col-resize-handle');
      if (!handle || !thead) return;

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

    function syncHorizontalScroll() {
      if (!headerEl || !scrollEl) return;
      headerEl.scrollLeft = scrollEl.scrollLeft;
    }

    function scrollHorizontally(deltaPx) {
      if (!scrollEl || deltaPx === 0) return;
      scrollEl.scrollLeft += deltaPx;
      syncHorizontalScroll();
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
        const eff = effectiveScrollHeight();
        const maxTopPad = Math.max(0, eff - visibleCount * rowHeight);
        topPad = scrollEl ? Math.min(scrollEl.scrollTop, maxTopPad) : 0;
        bottomPad = Math.max(0, eff - topPad - visibleCount * rowHeight);
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
        html += '<tr>';
        if (row) {
          for (let k = 0; k < row.length; k++) {
            html += formatCellDisplay(row[k]);
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
        html += formatCellDisplay(sampleRow[k]);
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

    function calcVisibleCount() {
      if (!scrollEl || rowHeight <= 0) return 10;
      return Math.ceil(scrollEl.clientHeight / rowHeight) + overscan;
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

    async function fetchChunk(offset, gen) {
      if (cache.has(offset)) return chunkResponse(offset);
      if (inFlight.has(offset)) {
        await inFlight.get(offset);
        return cache.has(offset) ? chunkResponse(offset) : null;
      }

      const promise = (async function () {
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
          chunkSizeActual = data.chunk_size || chunkSize;
          savedColumnWidths = data.column_widths || null;
          columnWidths = [];
          columnsRendered = false;
        }

        cache.set(offset, data.rows || []);
        return data;
      })();

      inFlight.set(offset, promise);
      try {
        return await promise;
      } finally {
        inFlight.delete(offset);
      }
    }

    async function ensureRowsForRange(start, count, gen) {
      const offsets = chunkOffsetsForRange(start, count);
      for (let i = 0; i < offsets.length; i++) {
        if (!cache.has(offsets[i])) {
          await fetchChunk(offsets[i], gen);
          if (gen !== generation) return false;
        }
      }
      return true;
    }

    function isRangeCached(start, count) {
      const offsets = chunkOffsetsForRange(start, count);
      for (let i = 0; i < offsets.length; i++) {
        if (!cache.has(offsets[i])) return false;
      }
      return true;
    }

    async function refreshView(gen, showLoading) {
      if (renderPending) {
        needsRefresh = true;
        return;
      }
      renderPending = true;

      try {
        do {
          needsRefresh = false;

          const needsFetch = !isRangeCached(startIndex, visibleCount);
          if (needsFetch && showLoading) {
            setStatus('loading', '読み込み中…', false);
          }

          await ensureRowsForRange(startIndex, visibleCount, gen);
          if (gen !== generation) return;

          renderVisibleRows();

          if (totalCount > 0) {
            setStatus('done', totalCount.toLocaleString() + ' 件', false);
          }
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
      // スケール時は同一インデックスでも topPad が scrollTop に追従する必要が
      // あるため、表示行がビューポートからずれないよう常に再描画する。
      if (newStart === startIndex && !isScaled()) return;

      startIndex = newStart;
      refreshView(generation, true);
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

    function scrollToStartIndex(newStart) {
      if (!scrollEl || rowHeight <= 0 || totalCount === 0) return;
      newStart = Math.max(0, Math.min(newStart, maxStartIndex()));
      if (newStart === startIndex) return;
      startIndex = newStart;
      syncScrollTopFromStartIndex();
      refreshView(generation, true);
    }

    function scrollByRows(deltaRows) {
      if (deltaRows === 0) return;
      scrollToStartIndex(startIndex + deltaRows);
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
      refreshView(generation, false);
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
      chunkSizeActual = chunkSize;
      startIndex = 0;
      rowHeight = 0;
      visibleCount = 0;
      wheelAccumPx = 0;
      lastSyncedScrollTop = -1;
      savedColumnWidths = null;
      columnWidths = [];
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
      setStatus('loading', '読み込み中…', false);

      try {
        const first = await fetchChunk(0, gen);
        if (!first || gen !== generation) return;

        if (first.total_count === 0) {
          setStatus('empty', 'データがありません', false);
          renderVisibleRows();
          return;
        }

        rowHeight = measureRowHeight();
        visibleCount = calcVisibleCount();
        startIndex = 0;

        await ensureRowsForRange(0, visibleCount, gen);
        if (gen !== generation) return;

        renderVisibleRows();
        setStatus('done', totalCount.toLocaleString() + ' 件', false);
      } catch (err) {
        if (err && err.name === 'AbortError') return;
        if (gen !== generation) return;
        setStatus('error', err.message || '取得に失敗しました', true);
      }
    }

    if (scrollEl) {
      scrollEl.addEventListener(
        'scroll',
        rafThrottle(function () {
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
    }

    if (countEl) {
      countEl.addEventListener('click', openRowGotoDialog);
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
