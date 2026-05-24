/**
 * 架電クオリティ フロント (Phase 1)
 *
 * 責務:
 *   - /api/call-quality/dashboard を呼び KPI 6 枚 + 月次グラフ 2 枚を描画
 *   - /api/call-quality/prefecture を呼び 都道府県別 アポ率を描画
 *   - /api/call-quality/raw/{sheet_name} を呼びデータブラウザ簡易表示
 *
 * 注意: 本ファイルは ECharts 5.x を前提とする。
 *       GAS 版で使っていた Chart.js とは API が異なるので留意。
 */

(function () {
  'use strict';

  const FETCH_TIMEOUT_MS = 30000;

  // ---- API ヘルパ -------------------------------------------------------
  async function fetchJson(url, opts = {}) {
    const ctl = new AbortController();
    const tid = setTimeout(() => ctl.abort(), FETCH_TIMEOUT_MS);
    try {
      const resp = await fetch(url, {
        ...opts,
        signal: ctl.signal,
        credentials: 'same-origin',
        headers: { 'Accept': 'application/json', ...(opts.headers || {}) },
      });
      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status} on ${url}`);
      }
      return await resp.json();
    } finally {
      clearTimeout(tid);
    }
  }

  function buildDashboardUrl() {
    const params = new URLSearchParams();
    const from = document.getElementById('cq-date-from')?.value;
    const to = document.getElementById('cq-date-to')?.value;
    const pipeline = document.getElementById('cq-pipeline')?.value;
    const prefecture = document.getElementById('cq-prefecture')?.value;
    const memSel = document.getElementById('cq-members');
    const members = memSel
      ? Array.from(memSel.selectedOptions)
          .map((o) => o.value)
          .filter((v) => v && v !== '__all__')
          .join(',')
      : '';

    if (from) params.set('from', from);
    if (to) params.set('to', to);
    if (pipeline && pipeline !== '__all__') params.set('pipeline', pipeline);
    if (prefecture && prefecture !== '__all__') params.set('prefecture', prefecture);
    if (members) params.set('members', members);
    const qs = params.toString();
    return '/api/call-quality/dashboard' + (qs ? `?${qs}` : '');
  }

  // ---- KPI 描画 ---------------------------------------------------------
  function renderKpiCards(kpis) {
    const grid = document.getElementById('cq-kpi-grid');
    if (!grid) return;
    grid.innerHTML = '';
    kpis.forEach((kpi) => {
      const card = document.createElement('div');
      card.className = 'cq-kpi-card';
      const label = document.createElement('div');
      label.className = 'cq-kpi-label';
      label.textContent = kpi.label;
      const value = document.createElement('div');
      value.className = 'cq-kpi-value';
      const fmt = formatNumber(kpi.value);
      value.innerHTML = `${fmt}<span class="cq-kpi-unit">${escapeHtml(kpi.unit)}</span>`;
      card.appendChild(label);
      card.appendChild(value);

      if (kpi.delta_pct != null) {
        const d = document.createElement('div');
        d.className = 'cq-kpi-delta ' + (kpi.delta_pct >= 0 ? 'positive' : 'negative');
        d.textContent = `前月比 ${kpi.delta_pct >= 0 ? '+' : ''}${kpi.delta_pct}%`;
        card.appendChild(d);
      } else if (kpi.delta_ppt != null) {
        const d = document.createElement('div');
        d.className = 'cq-kpi-delta ' + (kpi.delta_ppt >= 0 ? 'positive' : 'negative');
        d.textContent = `前月差 ${kpi.delta_ppt >= 0 ? '+' : ''}${kpi.delta_ppt} ppt`;
        card.appendChild(d);
      }
      grid.appendChild(card);
    });
  }

  function formatNumber(v) {
    if (v == null) return '-';
    if (Number.isInteger(v)) return v.toLocaleString('ja-JP');
    return v.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // ---- ECharts 共通 -----------------------------------------------------
  const chartInstances = new Map();

  function getChart(elId) {
    const el = document.getElementById(elId);
    if (!el || typeof echarts === 'undefined') return null;
    let chart = chartInstances.get(elId);
    if (!chart || chart.isDisposed()) {
      chart = echarts.init(el, null, { renderer: 'canvas' });
      chartInstances.set(elId, chart);
    }
    return chart;
  }

  function renderMonthlyLine(elId, points, opts = {}) {
    const chart = getChart(elId);
    if (!chart) return;
    const months = points.map((p) => p.month);
    const values = points.map((p) => p.value);
    chart.setOption({
      tooltip: {
        trigger: 'axis',
        valueFormatter: (v) => formatNumber(v) + (opts.unit || ''),
      },
      grid: { left: 60, right: 24, top: 24, bottom: 36 },
      xAxis: { type: 'category', data: months, axisLabel: { interval: 0 } },
      yAxis: { type: 'value', name: opts.yName || '' },
      series: [
        {
          type: 'line',
          data: values,
          smooth: true,
          symbol: 'circle',
          symbolSize: 6,
          lineStyle: { width: 2 },
          areaStyle: { opacity: 0.1 },
          itemStyle: { color: opts.color || '#3b82f6' },
        },
      ],
    });
  }

  function renderPrefectureBar(elId, bars) {
    const chart = getChart(elId);
    if (!chart) return;
    const top = bars.slice(0, 20);
    chart.setOption({
      tooltip: {
        trigger: 'axis',
        axisPointer: { type: 'shadow' },
        formatter: (params) => {
          const bar = params[0];
          const callIdx = bars.findIndex((b) => b.prefecture === bar.name);
          if (callIdx < 0) return bar.name;
          const b = bars[callIdx];
          return `<b>${escapeHtml(b.prefecture)}</b><br>
            架電数: ${formatNumber(b.call_count)}<br>
            アポ数: ${formatNumber(b.apo_count)}<br>
            アポ率: ${b.apo_rate}%`;
        },
      },
      grid: { left: 80, right: 36, top: 30, bottom: 60 },
      xAxis: {
        type: 'category',
        data: top.map((b) => b.prefecture),
        axisLabel: { rotate: 45, interval: 0 },
      },
      yAxis: [
        { type: 'value', name: 'アポ率 (%)', position: 'left' },
        { type: 'value', name: '架電数', position: 'right' },
      ],
      legend: { data: ['アポ率', '架電数'] },
      series: [
        {
          name: 'アポ率',
          type: 'bar',
          yAxisIndex: 0,
          data: top.map((b) => b.apo_rate),
          itemStyle: { color: '#10b981' },
        },
        {
          name: '架電数',
          type: 'line',
          yAxisIndex: 1,
          data: top.map((b) => b.call_count),
          symbol: 'circle',
          lineStyle: { color: '#94a3b8', width: 1.5 },
          itemStyle: { color: '#64748b' },
        },
      ],
    });
  }

  // ---- データブラウザ ---------------------------------------------------
  let lastRawRows = [];
  let lastRawSchema = [];
  let lastRawSheetName = '';

  async function loadRawSheet() {
    const sel = document.getElementById('cq-raw-sheet-select');
    if (!sel || !sel.value) return;
    const tbl = document.getElementById('cq-raw-table');
    tbl.innerHTML = '<thead><tr><th>読込中…</th></tr></thead><tbody></tbody>';
    try {
      const data = await fetchJson(`/api/call-quality/raw/${encodeURIComponent(sel.value)}`);
      lastRawRows = data.rows || [];
      lastRawSchema = data.schema || [];
      lastRawSheetName = sel.value;
      renderRawTable(lastRawSchema, lastRawRows);
      document.getElementById('cq-raw-csv-btn').disabled = lastRawRows.length === 0;
    } catch (e) {
      tbl.innerHTML = `<thead><tr><th>取得失敗: ${escapeHtml(e.message)}</th></tr></thead><tbody></tbody>`;
    }
  }

  function renderRawTable(schema, rows) {
    const tbl = document.getElementById('cq-raw-table');
    if (!tbl) return;
    const max = 200;
    const head = `<thead><tr>${schema.map((c) => `<th>${escapeHtml(c)}</th>`).join('')}</tr></thead>`;
    const body =
      `<tbody>` +
      rows
        .slice(0, max)
        .map(
          (r) =>
            `<tr>${schema
              .map((c) => `<td>${escapeHtml(r[c] ?? '')}</td>`)
              .join('')}</tr>`
        )
        .join('') +
      `</tbody>`;
    tbl.innerHTML = head + body;
    if (rows.length > max) {
      const footer = document.createElement('div');
      footer.className = 'cq-raw-table-note';
      footer.textContent = `先頭 ${max} 行を表示 (全 ${rows.length} 行)。CSV ダウンロードは全件出力。`;
      tbl.parentElement.appendChild(footer);
    }
  }

  function downloadCsv() {
    if (!lastRawRows.length) return;
    const esc = (v) => {
      const s = String(v ?? '');
      if (/[",\n]/.test(s)) return `"${s.replace(/"/g, '""')}"`;
      return s;
    };
    const lines = [lastRawSchema.map(esc).join(',')];
    for (const r of lastRawRows) {
      lines.push(lastRawSchema.map((c) => esc(r[c])).join(','));
    }
    const csv = lines.join('\n');
    const blob = new Blob([new TextEncoder().encode(csv)], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${lastRawSheetName}_${Date.now()}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // ---- キャッシュクリア -------------------------------------------------
  async function clearCache() {
    try {
      await fetchJson('/api/call-quality/cache/clear', { method: 'POST' });
      if (window.callQuality && window.callQuality.reload) {
        await window.callQuality.reload();
      }
    } catch (e) {
      console.error('cache clear failed', e);
    }
  }

  // ---- メイン -----------------------------------------------------------
  async function loadDashboard() {
    setUpdating(true);
    try {
      const url = buildDashboardUrl();
      const data = await fetchJson(url);
      renderKpiCards(data.kpis || []);
      renderMonthlyLine('cq-chart-monthly-call', data.monthly_call_trend || [], {
        yName: '件',
        color: '#3b82f6',
      });
      renderMonthlyLine('cq-chart-monthly-apo-rate', data.monthly_apo_rate_trend || [], {
        yName: '%',
        unit: '%',
        color: '#f59e0b',
      });
      setUpdated(data.updatedAt, data.fromCache);
    } catch (e) {
      console.error('dashboard load failed', e);
      setUpdated('取得失敗: ' + e.message, false);
    } finally {
      setUpdating(false);
    }
  }

  async function loadPrefecture() {
    try {
      const data = await fetchJson('/api/call-quality/prefecture');
      renderPrefectureBar('cq-chart-prefecture', data.bars || []);
      populatePrefectureSelect((data.bars || []).map((b) => b.prefecture));
    } catch (e) {
      console.error('prefecture load failed', e);
    }
  }

  function populatePrefectureSelect(prefs) {
    const sel = document.getElementById('cq-prefecture');
    if (!sel) return;
    const existing = new Set(Array.from(sel.options).map((o) => o.value));
    prefs.forEach((p) => {
      if (existing.has(p)) return;
      const opt = document.createElement('option');
      opt.value = p;
      opt.textContent = p;
      sel.appendChild(opt);
    });
  }

  function setUpdated(label, fromCache) {
    const el = document.getElementById('cq-updated');
    if (!el) return;
    el.textContent = `更新: ${label}${fromCache ? ' (cache)' : ''}`;
  }

  function setUpdating(b) {
    const el = document.getElementById('cq-updated');
    if (!el) return;
    if (b) el.textContent = '読み込み中…';
  }

  // ---- ウィンドウリサイズで ECharts を再フィット ----
  window.addEventListener('resize', () => {
    chartInstances.forEach((c) => c.resize());
  });

  // ---- 初期化 -----------------------------------------------------------
  document.addEventListener('DOMContentLoaded', () => {
    document.getElementById('cq-btn-apply')?.addEventListener('click', loadDashboard);
    document.getElementById('cq-btn-refresh')?.addEventListener('click', clearCache);
    document.getElementById('cq-raw-load-btn')?.addEventListener('click', loadRawSheet);
    document.getElementById('cq-raw-csv-btn')?.addEventListener('click', downloadCsv);

    // 公開 API
    window.callQuality = {
      reload: async () => {
        await loadDashboard();
        await loadPrefecture();
      },
    };

    // 初回ロード
    loadDashboard();
    loadPrefecture();
  });
})();
