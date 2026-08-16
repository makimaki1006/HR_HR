/*
 * P12 解約分析タブ。
 *
 * データ源: GET /api/call-quality/churn (URLは cq:tab-shown イベントの detail.api から取る。
 *   Rust: get_churn_analysis, src/handlers/call_quality/tabs/p12_churn.rs)
 *   引数なし。5シート(解約_理由パターン/コンサル担当別/業界規模マトリクス/active予測/モデル指標)
 *   の集計を**サーバ側で完結**させて1回で返す。
 *
 * GAS版(scripts/gas/call_quality_app/javascript.html _drawP12*)との違い:
 *   GAS版は5回の google.script.run で生データ配列を受け取り、集計・ランキング・
 *   軸合算をブラウザ側でやっていた。このファイルは**再集計しない**。
 *   サーバが返した数値をそのまま描くだけ(データ量が数百セル程度に収まっているため)。
 *
 * ---- 起動契約(2026-08-16 テンプレート実物に合わせて確定) -------------------
 * _layout.html が動的 import() でこのファイルを読み込み、読み込み完了後に
 * document へ `cq:tab-shown` を dispatch する(detail: {tab, api, el})。
 * このファイルは import された時点では何もせず、イベント購読だけ登録する。
 * 同じタブを再表示するたびに同イベントが発火するので、初回だけ fetch するよう
 * el(タブの .cq-page 要素)に data-cq-bound を立てて二重取得を防ぐ。
 *
 * ---- DOM契約(templates/tabs/call_quality/p12.html が正) -----------------
 * ルート: .cq-page[data-cq-tab="p12"]
 * 子要素: [data-cq-table="p12-risk-top20"] (B: KPIサマリ+Top20表を両方ここに差し込む。
 *           テンプレートにKPI専用枠が無いため、テーブルの前にstat-cardを差し込む形にした)
 *         [data-cq-chart="p12-reason-pattern"] (A: 6 stage比較。GAS版は2チャート+表
 *           だったが、テンプレートのチャート枠が1つしか無いため、平均接触量(下軸)と
 *           平均継続期間(上軸)を1チャートに dual x-axis で統合。詳細値はtooltipで補う)
 *         [data-cq-chart="p12-by-consultant"] (C: GAS版は上位10/下位10の2チャートだったが
 *           枠が1つのため、担当5件以上の全員を解約率降順1本のチャートにまとめ、
 *           datazoomスライダーでスクロールできるようにした)
 *         [data-cq-chart="p12-matrix"] (D: ECharts heatmap。軸/指標/最低件数セレクタは
 *           テンプレートに枠が無いため、パネル内にJSで動的挿入する)
 *         [data-cq-table="p12-model-metrics"] (B付録)
 *         [data-cq-truncated] (共通パーシャル _truncated_banner.html)
 *         [data-cq-sources] (共通パーシャル _sources_footer.html)
 *
 * これらは全て el.querySelector() で el(=そのタブの .cq-page)配下に限定して探す
 * (id ではなく data-cq-* 属性で引く。同じ属性値を持つ要素は他タブに存在しないが、
 * 念のため el スコープに閉じることで将来の重複を安全にしてある)。
 */
(function () {
  "use strict";

  var TAB_ID = "p12";
  var NA = "—"; // 分母0/値なし(0%やハイフン1文字と混同させない表示)

  // ---- 数値整形。既存 window.ChartHelpers を必ず使う(自前実装しない) -------------
  function num(v) {
    return window.ChartHelpers ? window.ChartHelpers.formatNumber(v) : String(v);
  }
  // ChartHelpers.formatPercent は「渡した値をそのまま %表記」するだけで
  // 100倍はしない。0-1の小数(churn_rate等)は事前に*100してから渡す。
  function pctFromFraction(v) {
    if (v == null || isNaN(v)) return NA;
    return window.ChartHelpers ? window.ChartHelpers.formatPercent(v * 100) : (v * 100).toFixed(1) + "%";
  }
  // segment_matrices のセル(bad_rate/good_rate/total_rate)は既に 0-100 のパーセントで来る
  function pctFromPercent(v) {
    if (v == null || isNaN(v)) return NA;
    return window.ChartHelpers ? window.ChartHelpers.formatPercent(v) : v.toFixed(1) + "%";
  }
  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  // ---- ECharts 初期化(dark テーマ。既存 charts.js の registerTheme('dark') を使う) ----
  var INSTANCES = [];
  // GAS版はチャート要素が<canvas>(Chart.js用)。ECharts は div コンテナへの init が前提で、
  // <canvas>に init すると内部で追加する子要素が「置換要素の中身」扱いになり描画されない
  // (エラーも出ずに空枠になる)。本テンプレートは既に div なので通常は素通りするが、
  // 保険として canvas を検出したら div に置換してから使う。
  function echartsHost(el) {
    if (!el) return null;
    if (el.tagName === "CANVAS") {
      var div = document.createElement("div");
      div.id = el.id;
      div.className = el.className;
      div.style.cssText = el.style.cssText || "width:100%;height:280px;";
      el.parentNode.replaceChild(div, el);
      return div;
    }
    return el;
  }
  function initChart(el) {
    if (typeof echarts === "undefined" || !el) return null;
    var existing = echarts.getInstanceByDom(el);
    if (existing) existing.dispose();
    var chart = echarts.init(el, "dark");
    INSTANCES.push(chart);
    return chart;
  }
  // このタブは app.js の .echart[data-chart-config] 自動リサイズの対象外
  // (data-chart-config を使わず直接 setOption するため)。survey_explore.js に倣い自前で持つ。
  var resizeTimer = null;
  window.addEventListener("resize", function () {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () {
      INSTANCES = INSTANCES.filter(function (c) { return !c.isDisposed(); });
      INSTANCES.forEach(function (c) { c.resize(); });
    }, 200);
  });

  // ---- 共通ヘルパ: タブ内のフックを1つ探す -------------------------------------
  function q(el, sel) { return el.querySelector(sel); }

  function setStatus(el, text) {
    var s = q(el, "[data-cq-status]");
    if (s) s.textContent = text; // p12テンプレートには無いので通常 null、有れば更新するだけ
  }

  function setTruncated(el, visible, text) {
    var b = q(el, "[data-cq-truncated]");
    if (!b) return;
    if (visible) {
      b.hidden = false;
      b.textContent = text;
    } else {
      b.hidden = true;
    }
  }

  function setSources(el, sources) {
    var box = q(el, "[data-cq-sources]");
    if (!box) return;
    box.innerHTML = sources.map(function (s) {
      return "<div>" + esc(s.sheet) + "(" + num(s.matched_rows) + "/" + num(s.total_rows) + "行、" +
        (s.from_cache ? "キャッシュ " + s.age_secs + "秒前" : "取得直後") + ")</div>";
    }).join("");
  }

  // ---- B: 予測KPI + Top20テーブル(1つの data-cq-table に両方差し込む) --------------

  var PRIORITY_LABEL = {
    CRITICAL_RESCUE: "🚨 緊急介入",
    WATCH_BAD: "⚠️ 要監視",
    DIG_DEMAND: "⛏️ 需要掘り起こし",
    STABLE: "✅ 安定"
  };
  var PRIORITY_BADGE_CLASS = {
    CRITICAL_RESCUE: "cq-badge-critical",
    WATCH_BAD: "cq-badge-watch",
    DIG_DEMAND: "cq-badge-dig",
    STABLE: "cq-badge-stable"
  };

  function statCard(label, value, note) {
    return '<div class="stat-card"><div class="stat-value">' + value + "</div>" +
      '<div class="stat-label">' + esc(label) + "</div>" +
      '<div style="font-size:10px; color:#64748b; margin-top:4px; line-height:1.4;">' + esc(note) + "</div></div>";
  }

  function renderPrediction(el, prediction) {
    var host = q(el, '[data-cq-table="p12-risk-top20"]');
    if (!host) return;

    var c = prediction.counts;
    var aucText = (prediction.auc_bad != null ? prediction.auc_bad.toFixed(2) : NA) + " / " +
                  (prediction.auc_good != null ? prediction.auc_good.toFixed(2) : NA);
    var kpiHtml = '<div class="grid-stats" style="margin-bottom:12px;">' +
      statCard(PRIORITY_LABEL.CRITICAL_RESCUE, c.critical_rescue + "件", "失敗解約proba ≥ 0.7 → 即介入で救出") +
      statCard(PRIORITY_LABEL.WATCH_BAD, c.watch_bad + "件", "失敗解約proba 0.4-0.7 → 監視") +
      statCard(PRIORITY_LABEL.DIG_DEMAND, c.dig_demand + "件", c.dig_demand ? "充足解約proba ≥ 0.7 → 他求人掘り起こし" : "該当なし(充足解約は予測困難・現データでは常時0件)") +
      statCard(PRIORITY_LABEL.STABLE, c.stable + "件", "継続proba ≥ 0.7") +
      statCard("AUC(失敗/充足)", esc(aucText), (prediction.model_type || "モデル") + " n=" + num(prediction.n_train) +
        "｜AUCは介入可能解約(strict)基準の判別力。0.7前後は粗い絞り込み補助レベルで、自動判断はできません。") +
      "</div>";
    var statusHtml = '<div style="font-size:10.5px; color:#64748b; margin-bottom:8px;">予測対象: ' +
      num(prediction.total_active_deals) + " 稼働中の案件 / 表示: 上位 " + prediction.top20.length + " 件(失敗解約proba降順)</div>";

    if (!prediction.top20.length) {
      host.innerHTML = kpiHtml + statusHtml + '<div style="padding:14px; color:#94a3b8;">予測データがありません(Pythonバッチ未実行の可能性)。</div>';
      setTruncated(el, false);
      return;
    }

    var tableHtml = '<div style="overflow-x:auto;"><table class="data-table"><thead><tr>' +
      "<th>Deal</th><th>担当</th><th>Stage</th><th>業界</th><th>規模</th>" +
      '<th class="text-right">経過月</th><th class="text-right">最終活動(日前)</th>' +
      '<th class="text-right">失敗解約proba</th><th class="text-right">継続proba</th>' +
      "<th>介入優先度</th><th>予測根拠(主要因)</th>" +
      "</tr></thead><tbody>";
    prediction.top20.forEach(function (r) {
      var badgeClass = PRIORITY_BADGE_CLASS[r.intervention_priority] || "cq-badge-mixed";
      var badgeLabel = PRIORITY_LABEL[r.intervention_priority] || "その他";
      tableHtml += "<tr>" +
        "<td>" + esc(r.deal_name || r.deal_id) + '<div style="font-size:10px; color:#64748b;">Deal ' + esc(r.deal_id) + "</div></td>" +
        "<td>" + esc(r.consultant_name || "-") + "</td>" +
        "<td>" + esc(r.stage_label || "-") + "</td>" +
        "<td>" + esc(r.industry_jsic || "-") + "</td>" +
        "<td>" + esc(r.size_band || "-") + "</td>" +
        '<td class="text-right">' + r.deal_age_months.toFixed(1) + "</td>" +
        '<td class="text-right">' + r.days_since_last_activity.toFixed(0) + "</td>" +
        '<td class="text-right" style="font-weight:700;">' + (r.bad_proba * 100).toFixed(0) + "%</td>" +
        '<td class="text-right">' + (r.continue_proba * 100).toFixed(0) + "%</td>" +
        '<td><span class="cq-badge ' + badgeClass + '" title="' + esc(r.intervention_priority || "") + '">' + esc(badgeLabel) + "</span>" +
          (r.nps_alert_msg ? '<div style="font-size:10px; color:#94a3b8; margin-top:2px;">' + esc(r.nps_alert_msg) + "</div>" : "") + "</td>" +
        '<td class="cell-wrap">' + esc((r.top_factors || "-").substring(0, 90)) + "</td>" +
        "</tr>";
    });
    tableHtml += "</tbody></table></div>";
    host.innerHTML = kpiHtml + statusHtml + tableHtml;

    setTruncated(el, prediction.truncated,
      "⚠ 稼働中Deal " + num(prediction.total_active_deals) + " 件のうち、確率上位20件のみを表示しています(黙って切り詰めていません)。");
  }

  // ---- A: 解約理由パターン(dual x-axis 1チャートに統合) ------------------------

  function renderPattern(el, rows) {
    var host = echartsHost(q(el, '[data-cq-chart="p12-reason-pattern"]'));
    if (!host) return;
    if (!rows.length) {
      host.innerHTML = '<div style="padding:14px; color:#94a3b8;">データなし</div>';
      return;
    }
    var labels = rows.map(function (r) { return r.stage_label || r.stage_id; }).reverse();
    var contact = rows.map(function (r) { return r.avg_total_contact; }).reverse();
    var lifetime = rows.map(function (r) { return r.avg_customer_lifetime_days; }).reverse();

    var chart = initChart(host);
    if (!chart) return;
    chart.setOption({
      backgroundColor: "transparent",
      legend: { textStyle: { color: "#cbd5e1" }, top: 0 },
      tooltip: {
        trigger: "axis", axisPointer: { type: "shadow" },
        formatter: function (params) {
          var i = params[0].dataIndex;
          var r = rows[rows.length - 1 - i];
          return esc(r.stage_label) + "<br/>Deal数 " + num(r.deals_count) +
            "<br/>平均接触量 " + r.avg_total_contact.toFixed(2) + "(Call " + r.avg_call.toFixed(2) + " / Email " + r.avg_email.toFixed(2) + " / MTG " + r.avg_mtg.toFixed(2) + ")" +
            "<br/>平均継続期間 " + r.avg_customer_lifetime_days.toFixed(0) + "日" +
            "<br/>平均NPS " + (r.avg_nps != null ? r.avg_nps.toFixed(2) : NA) +
            "<br/>平均継続意向 " + (r.avg_continue_intent != null ? r.avg_continue_intent.toFixed(2) : NA);
        }
      },
      grid: { left: "22%", right: "6%", top: "14%", bottom: "8%" },
      xAxis: [
        { type: "value", position: "bottom", name: "平均接触量(回)", nameTextStyle: { color: "#94a3b8" }, axisLabel: { color: "#94a3b8", fontSize: 10 } },
        { type: "value", position: "top", name: "平均継続期間(日)", nameTextStyle: { color: "#94a3b8" }, axisLabel: { color: "#94a3b8", fontSize: 10 } }
      ],
      yAxis: { type: "category", data: labels, axisLabel: { color: "#cbd5e1", fontSize: 10 } },
      series: [
        { name: "平均接触量(Call+Email+MTG)", type: "bar", xAxisIndex: 0, data: contact, itemStyle: { color: "#3b82f6" } },
        { name: "平均継続期間(日)", type: "bar", xAxisIndex: 1, data: lifetime, itemStyle: { color: "#10b981" } }
      ]
    });
  }

  // ---- C: コンサル担当別(担当5件以上を解約率降順、1チャート+datazoom) -----------

  function renderConsultants(el, section) {
    var host = echartsHost(q(el, '[data-cq-chart="p12-by-consultant"]'));
    if (!host) return;
    var eligible = section.all.filter(function (r) { return r.total_deals >= 5; })
      .sort(function (a, b) { return b.churn_rate - a.churn_rate; });
    if (!eligible.length) {
      host.innerHTML = '<div style="padding:14px; color:#94a3b8;">データなし(担当Deal5件以上の担当者がいません)</div>';
      return;
    }
    var labels = eligible.map(function (r) { return r.consultant_name || r.consultant_id; }).reverse();
    var values = eligible.map(function (r) { return r.churn_rate * 100; }).reverse();
    var chart = initChart(host);
    if (!chart) return;
    // 表示件数が多いとラベルが潰れるため、20件超なら縦スクロール用 dataZoom を付ける
    var needZoom = eligible.length > 20;
    chart.setOption({
      backgroundColor: "transparent",
      tooltip: {
        trigger: "axis", axisPointer: { type: "shadow" },
        formatter: function (params) {
          var r = eligible[eligible.length - 1 - params[0].dataIndex];
          return esc(r.consultant_name || r.consultant_id) + "<br/>解約率 " + (r.churn_rate * 100).toFixed(1) +
            "%(解約 " + r.churn_deals + "件 / 担当 " + r.total_deals + "件)" +
            "<br/>継続率 " + pctFromFraction(r.continue_rate) + " / 充足率 " + pctFromFraction(r.sufficiency_rate) +
            "<br/>1案件あたり平均Call " + r.avg_call_per_deal.toFixed(1);
        }
      },
      grid: { left: "24%", right: "6%", top: "4%", bottom: needZoom ? "10%" : "6%" },
      dataZoom: needZoom ? [{ type: "slider", yAxisIndex: 0, width: 10, right: 2 }, { type: "inside", yAxisIndex: 0 }] : [],
      xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10, formatter: "{value}%" } },
      yAxis: { type: "category", data: labels, axisLabel: { color: "#cbd5e1", fontSize: 10 } },
      // 中立色: 高解約=赤/低解約=緑という人物評価シグナルは撤去する(GAS版2026-06-11監査是正を踏襲)
      series: [{ type: "bar", data: values, itemStyle: { color: "#475569" } }]
    });
  }

  // ---- D: 業界×規模マトリクス(ECharts heatmap + JS動的セレクタ) -----------------

  var SEGMENT_METRIC_FIELD = { total_churn_rate: "total_rate", bad_churn_rate: "bad_rate", good_churn_rate: "good_rate" };
  var SEGMENT_STATE = {}; // el単位で保持(タブが複数回開かれても持ちデータを使い回す)

  function ensureSegmentControls(el, chartHost) {
    var panel = chartHost.parentNode;
    var existing = panel.querySelector('[data-cq-segment-controls="1"]');
    if (existing) return existing;
    var bar = document.createElement("div");
    bar.setAttribute("data-cq-segment-controls", "1");
    bar.className = "filter-group";
    bar.style.marginBottom = "8px";
    bar.innerHTML =
      '<div class="filter-field"><label class="filter-field-label">軸</label>' +
        '<select class="filter-control" data-cq-seg="axis">' +
          '<option value="industry_size" selected>業界 × 規模</option>' +
          '<option value="industry_pref">業界 × 都道府県</option>' +
          '<option value="size_pref">規模 × 都道府県</option>' +
        "</select></div>" +
      '<div class="filter-field"><label class="filter-field-label">種別</label>' +
        '<select class="filter-control" data-cq-seg="metric">' +
          '<option value="total_churn_rate" selected>解約率(失敗3種+充足)</option>' +
          '<option value="bad_churn_rate">うち失敗3種のみ</option>' +
          '<option value="good_churn_rate">うち充足のみ</option>' +
        "</select></div>" +
      '<div class="filter-field"><label class="filter-field-label">最低件数</label>' +
        '<select class="filter-control" data-cq-seg="minn">' +
          '<option value="5" selected>n≥5</option><option value="10">n≥10</option>' +
          '<option value="20">n≥20</option><option value="1">全表示</option>' +
        "</select></div>";
    panel.insertBefore(bar, chartHost);
    return bar;
  }

  function renderSegmentHeatmap(el, matrices) {
    var chartEl = q(el, '[data-cq-chart="p12-matrix"]');
    if (!chartEl) return;
    var host = echartsHost(chartEl);
    SEGMENT_STATE.matrices = matrices;
    var controls = ensureSegmentControls(el, host);
    if (!controls._cqBound) {
      controls._cqBound = true;
      controls.querySelectorAll("select").forEach(function (sel) {
        sel.addEventListener("change", function () { redrawSegment(el); });
      });
    }
    redrawSegment(el);
  }

  function redrawSegment(el) {
    var chartEl = q(el, '[data-cq-chart="p12-matrix"]');
    var host = echartsHost(chartEl);
    var matrices = SEGMENT_STATE.matrices;
    if (!host || !matrices) return;
    var panel = host.parentNode;
    var axis = (panel.querySelector('[data-cq-seg="axis"]') || {}).value || "industry_size";
    var metricKey = (panel.querySelector('[data-cq-seg="metric"]') || {}).value || "total_churn_rate";
    var minN = parseInt((panel.querySelector('[data-cq-seg="minn"]') || {}).value, 10) || 5;

    var matrix = matrices.filter(function (m) { return m.axis === axis; })[0];
    if (!matrix || !matrix.cells.length) {
      host.innerHTML = '<div style="padding:14px; color:#94a3b8;">解約マトリクス データなし</div>';
      return;
    }
    var byKey = {};
    matrix.cells.forEach(function (c) { byKey[c.row_key + "|" + c.col_key] = c; });
    var field = SEGMENT_METRIC_FIELD[metricKey];

    var data = [];
    matrix.row_keys.forEach(function (rk, ri) {
      matrix.col_keys.forEach(function (ck, ci) {
        var cell = byKey[rk + "|" + ck];
        if (!cell) return;
        var below = cell.total < minN;
        var point = { value: [ci, ri, Math.round(cell[field])], _cell: cell };
        if (below) point.itemStyle = { color: "#334155" }; // 最低件数未満は灰色固定(visualMapより優先)
        data.push(point);
      });
    });

    var chart = initChart(host);
    if (!chart) return;
    chart.setOption({
      backgroundColor: "transparent",
      tooltip: {
        formatter: function (p) {
          var c = p.data._cell;
          return matrix.row_keys[p.data.value[1]] + " × " + matrix.col_keys[p.data.value[0]] +
            "<br/>n=" + c.total + " / Bad " + c.bad + "件 / Good " + c.good + "件" +
            "<br/>Bad率 " + pctFromPercent(c.bad_rate) + " / Good率 " + pctFromPercent(c.good_rate) + " / 全churn率 " + pctFromPercent(c.total_rate) +
            (c.total < minN ? "<br/>(最低件数 n≥" + minN + " 未満、参考値)" : "");
        }
      },
      grid: { left: "20%", right: "6%", top: "4%", bottom: "20%" },
      xAxis: { type: "category", data: matrix.col_keys, axisLabel: { color: "#94a3b8", fontSize: 10, interval: 0, rotate: 30 }, splitArea: { show: true } },
      yAxis: { type: "category", data: matrix.row_keys, axisLabel: { color: "#cbd5e1", fontSize: 10 }, splitArea: { show: true } },
      visualMap: {
        min: 0, max: 100, orient: "horizontal", left: "center", bottom: 0,
        textStyle: { color: "#94a3b8" },
        pieces: [
          { min: 60, color: "#dc2626", label: "≥60%" },
          { min: 40, max: 60, color: "#ea580c", label: "40-60%" },
          { min: 20, max: 40, color: "#ca8a04", label: "20-40%" },
          { min: 0, max: 20, color: "#16a34a", label: "<20%" }
        ]
      },
      series: [{
        type: "heatmap",
        data: data,
        label: { show: true, color: "#fff", fontSize: 10, formatter: function (p) { return p.data._cell.total < minN ? "-" : p.value[2] + "%"; } }
      }]
    });
  }

  // ---- B付録: モデル指標 ------------------------------------------------------

  function renderMetrics(el, metrics) {
    var host = q(el, '[data-cq-table="p12-model-metrics"]');
    if (!host) return;
    if (!metrics.entries.length) {
      host.innerHTML = '<div style="padding:14px; color:#94a3b8;">モデル指標 データなし</div>';
      return;
    }
    var html = '<div style="overflow-x:auto;"><table class="data-table"><thead><tr><th>指標</th><th>値</th><th>説明</th></tr></thead><tbody>';
    metrics.entries.forEach(function (e) {
      html += "<tr><td>" + esc(e.key) + "</td><td>" + esc(e.value == null ? "-" : e.value) + "</td><td>" + esc(e.note) + "</td></tr>";
    });
    metrics.top_features.forEach(function (f, i) {
      html += "<tr><td>feature_importance_top" + (i + 1) + "</td><td>" + esc(f.feature) + ": " + f.importance + "</td><td>重要度(LightGBM=gain)</td></tr>";
    });
    html += "</tbody></table></div>";
    host.innerHTML = html;
  }

  // ---- 起動 ------------------------------------------------------------------

  function load(el, apiUrl) {
    if (el.dataset.cqBound === "1") return; // 再表示のたびに cq:tab-shown が飛ぶための二重取得防止
    el.dataset.cqBound = "1";
    setStatus(el, "読み込み中...");

    fetch(apiUrl, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        var d = payload.data;
        setStatus(el, "取得完了(サーバ処理 " + payload.elapsed_ms + "ms)");
        renderPrediction(el, d.prediction);
        renderPattern(el, d.pattern);
        renderConsultants(el, d.consultants);
        renderSegmentHeatmap(el, d.segment_matrices);
        renderMetrics(el, d.metrics);
        setSources(el, payload.sources);
      })
      .catch(function (err) {
        el.dataset.cqBound = ""; // 失敗時は再訪問で再試行できるようにする
        setStatus(el, "取得失敗: " + (err && err.message ? err.message : err) + "。時間をおいて再読み込みしてください。");
      });
  }

  document.addEventListener("cq:tab-shown", function (e) {
    var detail = e.detail || {};
    if (detail.tab !== TAB_ID || !detail.el) return;
    load(detail.el, detail.api);
  });

  // 手動再実行用(コンソールデバッグ等)に公開。通常は cq:tab-shown 経由で呼ばれる。
  window.cqChurnLoad = load;
})();
