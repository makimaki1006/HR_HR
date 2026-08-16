/*
 * P12 解約分析タブ。
 *
 * データ源: GET /api/call-quality/churn
 *   (Rust: get_churn_analysis, src/handlers/call_quality/tabs/p12_churn.rs)
 *   引数なし。5シート(解約_理由パターン/コンサル担当別/業界規模マトリクス/active予測/モデル指標)
 *   の集計を**サーバ側で完結**させて1回で返す。
 *
 * GAS版(scripts/gas/call_quality_app/javascript.html _drawP12*)との違い:
 *   GAS版は5回の google.script.run で生データ配列を受け取り、集計・ランキング・
 *   軸合算をブラウザ側でやっていた。このファイルは**再集計しない**。
 *   サーバが返した数値をそのまま描くだけ(データ量が数百セル程度に収まっているため)。
 *
 * ドロップダウン(D行列の軸/指標/最低件数)は再fetchしない。
 *   segment_matrices に3軸ぶんの集計が最初から全部入っているので、
 *   セレクタ変更はクライアント側の描き直しだけで完結する(サーバ側の分母は変わらない)。
 *
 * ---- 遅延読込(2026-08-16 チームリード確定) -------------------------------
 * index.js が P12 タブを初めて開いたときにこのファイルを <script> 挿入する想定。
 * DOMContentLoaded は挿入時点で既に発火済みのため待たない。読み込まれた瞬間に
 * 自分で bind() まで完了させる(index.js 側からのコールバックは不要な設計)。
 *
 * ---- DOM契約 ---------------------------------------------------------------
 * テンプレート担当が GAS版 index.html の要素IDをそのまま踏襲する前提で実装している。
 * ルート: #page-p12
 * 子要素(GAS版と同じID): p12-b-kpis / p12-prediction-table / p12-b-status /
 *   p12-pattern-contact-chart / p12-pattern-lifetime-chart / p12-pattern-table /
 *   p12-consultant-top-chart / p12-consultant-bottom-chart / p12-consultant-table /
 *   p12-d-axis(select) / p12-d-metric(select) / p12-d-min-n(select) /
 *   p12-segment-heatmap / p12-metrics-table
 *
 * ⚠ 既知の注意点: GAS版はチャート要素が <canvas>(Chart.js用)。ECharts は
 *   div コンテナへ init するのが前提で、<canvas> に init すると子要素が
 *   置換要素の内側に隠れて描画されない(空枠になる)。テンプレートが GAS版を
 *   そのまま踏襲して <canvas> のままだった場合に備え、echartsHost() が
 *   canvas を検出したら同IDの div に自動で差し替える防御を入れている。
 *
 * GAS版に無い要素(このRust移植で新設):
 *   truncated 警告バー・データソース透明性フッターは対応するGAS要素が無いため、
 *   #page-p12 の中に動的に追記する(下記 ensureExtraEl 参照)。
 */
(function () {
  "use strict";

  var ENDPOINT = "/api/call-quality/churn";
  var NA = "—"; // 分母0/値なし(0%やハイフン1文字と混同させない表示)
  var ROOT_ID = "page-p12";

  function byId(id) { return document.getElementById(id); }

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

  var PALETTE = { blue: "#3b82f6" };

  // ---- ECharts 初期化(dark テーマ。既存 charts.js の registerTheme('dark') を使う) ----
  var INSTANCES = [];
  // <canvas> だった場合は同IDの<div>に置換してから返す(ファイル冒頭の注意点参照)
  function echartsHost(id) {
    var el = byId(id);
    if (!el) return null;
    if (el.tagName === "CANVAS") {
      var div = document.createElement("div");
      div.id = id;
      div.className = el.className;
      div.style.cssText = el.style.cssText || "width:100%;height:260px;";
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

  function baseOption(extra) {
    var option = {
      backgroundColor: "transparent",
      grid: { left: "26%", right: "8%", top: "6%", bottom: "8%" },
      aria: { enabled: true, decal: { show: true } }
    };
    Object.keys(extra || {}).forEach(function (k) { option[k] = extra[k]; });
    return option;
  }

  // ---- GAS版に無い要素(status/truncated/sources)を動的に用意する -----------------
  function ensureExtraEl(id, cssText) {
    var el = byId(id);
    if (el) return el;
    var root = byId(ROOT_ID);
    if (!root) return null;
    el = document.createElement("div");
    el.id = id;
    if (cssText) el.style.cssText = cssText;
    root.insertBefore(el, root.firstChild);
    return el;
  }

  // ---- B: 予測KPI + テーブル -------------------------------------------------

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

  function renderPredictionKpis(prediction) {
    var el = byId("p12-b-kpis");
    if (!el) return;
    var c = prediction.counts;
    var aucText = (prediction.auc_bad != null ? prediction.auc_bad.toFixed(2) : NA) + " / " +
                  (prediction.auc_good != null ? prediction.auc_good.toFixed(2) : NA);
    el.innerHTML =
      statCard(PRIORITY_LABEL.CRITICAL_RESCUE, c.critical_rescue + "件", "失敗解約proba ≥ 0.7 → 即介入で救出") +
      statCard(PRIORITY_LABEL.WATCH_BAD, c.watch_bad + "件", "失敗解約proba 0.4-0.7 → 監視") +
      statCard(PRIORITY_LABEL.DIG_DEMAND, c.dig_demand + "件", c.dig_demand ? "充足解約proba ≥ 0.7 → 他求人掘り起こし" : "該当なし(充足解約は予測困難・現データでは常時0件)") +
      statCard(PRIORITY_LABEL.STABLE, c.stable + "件", "継続proba ≥ 0.7") +
      statCard("AUC(失敗/充足)", esc(aucText), (prediction.model_type || "モデル") + " n=" + num(prediction.n_train) +
        "｜AUCは介入可能解約(strict)基準の判別力。0.7前後は粗い絞り込み補助レベルで、自動判断はできません。");
  }

  function renderPredictionTable(prediction) {
    var truncEl = ensureExtraEl("cq-p12-truncated", "display:none;");
    if (truncEl) {
      truncEl.className = "cq-truncated-note";
      if (prediction.truncated) {
        truncEl.style.display = "";
        truncEl.textContent = "⚠ 稼働中Deal " + num(prediction.total_active_deals) + " 件のうち、確率上位20件のみを表示しています(黙って切り詰めていません)。";
      } else {
        truncEl.style.display = "none";
      }
    }
    var statusEl = byId("p12-b-status");
    if (statusEl) {
      statusEl.textContent = "予測対象: " + num(prediction.total_active_deals) + " 稼働中の案件 / 表示: 上位 " + prediction.top20.length + " 件(失敗解約proba降順)";
    }
    var tableEl = byId("p12-prediction-table");
    if (!tableEl) return;
    if (!prediction.top20.length) {
      tableEl.innerHTML = '<div style="padding:14px; color:#94a3b8;">予測データがありません(Pythonバッチ未実行の可能性)。</div>';
      return;
    }
    var html = '<div style="overflow-x:auto;"><table class="data-table"><thead><tr>' +
      "<th>Deal</th><th>担当</th><th>Stage</th><th>業界</th><th>規模</th>" +
      '<th class="text-right">経過月</th><th class="text-right">最終活動(日前)</th>' +
      '<th class="text-right">失敗解約proba</th><th class="text-right">継続proba</th>' +
      "<th>介入優先度</th><th>予測根拠(主要因)</th>" +
      "</tr></thead><tbody>";
    prediction.top20.forEach(function (r) {
      var badgeClass = PRIORITY_BADGE_CLASS[r.intervention_priority] || "cq-badge-mixed";
      // 未知の値(将来コード追加時の保険)だけ「その他」にする。生の英語コードは
      // title属性にだけ残し(cross-reference用)、画面の主表示には出さない。
      var badgeLabel = PRIORITY_LABEL[r.intervention_priority] || "その他";
      html += "<tr>" +
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
    html += "</tbody></table></div>";
    tableEl.innerHTML = html;
  }

  // ---- A: 解約理由パターン ---------------------------------------------------

  function renderPattern(rows) {
    var tableEl = byId("p12-pattern-table");
    if (!rows.length) {
      if (tableEl) tableEl.innerHTML = '<div style="padding:14px; color:#94a3b8;">データなし</div>';
      return;
    }
    var labels = rows.map(function (r) { return r.stage_label || r.stage_id; });

    var contactChart = initChart(echartsHost("p12-pattern-contact-chart"));
    if (contactChart) {
      contactChart.setOption(baseOption({
        tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
        xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10 } },
        yAxis: { type: "category", data: labels.slice().reverse(), axisLabel: { color: "#cbd5e1", fontSize: 10 } },
        series: [{ type: "bar", data: rows.map(function (r) { return r.avg_total_contact; }).slice().reverse(), itemStyle: { color: PALETTE.blue } }]
      }));
    }
    var lifetimeChart = initChart(echartsHost("p12-pattern-lifetime-chart"));
    if (lifetimeChart) {
      lifetimeChart.setOption(baseOption({
        tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
        xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10 } },
        yAxis: { type: "category", data: labels.slice().reverse(), axisLabel: { color: "#cbd5e1", fontSize: 10 } },
        series: [{ type: "bar", data: rows.map(function (r) { return r.avg_customer_lifetime_days; }).slice().reverse(), itemStyle: { color: "#10b981" } }]
      }));
    }
    if (tableEl) {
      var html = '<div style="overflow-x:auto;"><table class="data-table"><thead><tr>' +
        '<th>Stage</th><th class="text-right">Deal数</th><th class="text-right">平均接触量</th>' +
        '<th class="text-right">平均Call</th><th class="text-right">平均Email</th><th class="text-right">平均MTG</th>' +
        '<th class="text-right">平均MTG間隔(日)</th><th class="text-right">平均継続期間(日)</th>' +
        '<th class="text-right">平均NPS</th><th class="text-right">平均継続意向</th>' +
        "</tr></thead><tbody>";
      rows.forEach(function (r) {
        html += "<tr><td>" + esc(r.stage_label) + "</td>" +
          '<td class="text-right">' + num(r.deals_count) + "</td>" +
          '<td class="text-right">' + r.avg_total_contact.toFixed(2) + "</td>" +
          '<td class="text-right">' + r.avg_call.toFixed(2) + "</td>" +
          '<td class="text-right">' + r.avg_email.toFixed(2) + "</td>" +
          '<td class="text-right">' + r.avg_mtg.toFixed(2) + "</td>" +
          '<td class="text-right">' + (r.avg_mtg_interval_days != null ? r.avg_mtg_interval_days.toFixed(1) : NA) + "</td>" +
          '<td class="text-right">' + r.avg_customer_lifetime_days.toFixed(0) + "</td>" +
          '<td class="text-right">' + (r.avg_nps != null ? r.avg_nps.toFixed(2) : NA) + "</td>" +
          '<td class="text-right">' + (r.avg_continue_intent != null ? r.avg_continue_intent.toFixed(2) : NA) + "</td>" +
          "</tr>";
      });
      html += "</tbody></table></div>";
      tableEl.innerHTML = html;
    }
  }

  // ---- C: コンサル担当別 -----------------------------------------------------

  function renderConsultants(section) {
    var tableEl = byId("p12-consultant-table");
    if (!section.all.length) {
      if (tableEl) tableEl.innerHTML = '<div style="padding:14px; color:#94a3b8;">データなし</div>';
      return;
    }
    function barOption(rows) {
      var labels = rows.map(function (r) { return r.consultant_name || r.consultant_id; });
      var values = rows.map(function (r) { return r.churn_rate * 100; });
      return baseOption({
        tooltip: {
          trigger: "axis", axisPointer: { type: "shadow" },
          formatter: function (params) {
            var r = rows[rows.length - 1 - params[0].dataIndex];
            return esc(r.consultant_name || r.consultant_id) + "<br/>解約率 " + (r.churn_rate * 100).toFixed(1) +
              "% (解約 " + r.churn_deals + "件 / 担当 " + r.total_deals + "件)";
          }
        },
        xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10, formatter: "{value}%" } },
        yAxis: { type: "category", data: labels.slice().reverse(), axisLabel: { color: "#cbd5e1", fontSize: 10 } },
        // 中立色: 高解約=赤/低解約=緑という人物評価シグナルは撤去する(GAS版2026-06-11監査是正を踏襲)
        series: [{ type: "bar", data: values.slice().reverse(), itemStyle: { color: "#475569" } }]
      });
    }
    var topChart = initChart(echartsHost("p12-consultant-top-chart"));
    if (topChart) topChart.setOption(barOption(section.top10));
    var botChart = initChart(echartsHost("p12-consultant-bottom-chart"));
    if (botChart) botChart.setOption(barOption(section.bottom10));

    if (tableEl) {
      var html = '<div style="overflow-x:auto;"><table class="data-table"><thead><tr>' +
        '<th>担当</th><th class="text-right">担当数</th><th class="text-right">active</th>' +
        '<th class="text-right">解約</th><th class="text-right">充足</th><th class="text-right">継続済</th>' +
        '<th class="text-right">解約率</th><th class="text-right">継続率</th><th class="text-right">充足率</th>' +
        '<th class="text-right">1案件あたり平均Call</th>' +
        "</tr></thead><tbody>";
      section.all.forEach(function (r) {
        html += "<tr><td>" + esc(r.consultant_name || r.consultant_id) +
          (r.total_deals < 5 ? '<div style="font-size:10px; color:#64748b;">ランキング対象外(担当5件未満)</div>' : "") + "</td>" +
          '<td class="text-right">' + num(r.total_deals) + "</td>" +
          '<td class="text-right">' + num(r.active_deals) + "</td>" +
          '<td class="text-right">' + num(r.churn_deals) + "</td>" +
          '<td class="text-right">' + num(r.sufficiency_deals) + "</td>" +
          '<td class="text-right">' + num(r.continue_deals) + "</td>" +
          '<td class="text-right" style="font-weight:700;">' + pctFromFraction(r.churn_rate) + "</td>" +
          '<td class="text-right">' + pctFromFraction(r.continue_rate) + "</td>" +
          '<td class="text-right">' + pctFromFraction(r.sufficiency_rate) + "</td>" +
          '<td class="text-right">' + r.avg_call_per_deal.toFixed(1) + "</td>" +
          "</tr>";
      });
      html += "</tbody></table></div>";
      tableEl.innerHTML = html;
    }
  }

  // ---- D: 業界×規模マトリクス -------------------------------------------------

  function heatCellColor(rate, total, minN) {
    if (total < minN) return { bg: "#1e293b", fg: "#64748b" };
    if (rate >= 60) return { bg: "#dc2626", fg: "#fff" };
    if (rate >= 40) return { bg: "#ea580c", fg: "#fff" };
    if (rate >= 20) return { bg: "#ca8a04", fg: "#fff" };
    return { bg: "#16a34a", fg: "#fff" };
  }

  var SEGMENT_METRIC_FIELD = { total_churn_rate: "total_rate", bad_churn_rate: "bad_rate", good_churn_rate: "good_rate" };
  var SEGMENT_METRIC_LABEL = { total_churn_rate: "全churn率", bad_churn_rate: "Bad率", good_churn_rate: "Good率" };

  var SEGMENT_MATRICES = null; // 再fetchせず持ちデータを使い回す(表示切替のみのため)

  function renderSegmentHeatmap(matrices) {
    SEGMENT_MATRICES = matrices;
    redrawSegment();
  }

  function redrawSegment() {
    var el = byId("p12-segment-heatmap");
    if (!el || !SEGMENT_MATRICES) return;
    // GAS版のセレクタ値(industry_size等/total_churn_rate等)をそのまま受ける
    var axisSel = byId("p12-d-axis");
    var metricSel = byId("p12-d-metric");
    var minNSel = byId("p12-d-min-n");
    var axis = axisSel ? axisSel.value : "industry_size";
    var metricKey = metricSel ? metricSel.value : "total_churn_rate";
    var minN = minNSel ? parseInt(minNSel.value, 10) || 1 : 5;

    var matrix = SEGMENT_MATRICES.filter(function (m) { return m.axis === axis; })[0];
    if (!matrix || !matrix.cells.length) {
      el.innerHTML = '<div style="padding:14px; color:#94a3b8;">解約マトリクス データなし</div>';
      return;
    }
    var byKey = {};
    matrix.cells.forEach(function (c) { byKey[c.row_key + "|" + c.col_key] = c; });

    var html = '<table style="border-collapse:collapse; font-size:11px;">';
    html += '<thead><tr><th style="position:sticky; left:0; background:#0f172a; padding:6px; border:1px solid #334155; min-width:160px; text-align:left; color:#cbd5e1;">' +
      esc(matrix.row_label) + " ＼ " + esc(matrix.col_label) + "</th>";
    matrix.col_keys.forEach(function (ck) {
      html += '<th style="padding:6px; border:1px solid #334155; min-width:78px; color:#cbd5e1;">' + esc(ck) + "</th>";
    });
    html += "</tr></thead><tbody>";
    matrix.row_keys.forEach(function (rk) {
      html += '<tr><td style="position:sticky; left:0; background:#0f172a; padding:6px; border:1px solid #334155; font-weight:600; color:#e2e8f0;">' + esc(rk) + "</td>";
      matrix.col_keys.forEach(function (ck) {
        var cell = byKey[rk + "|" + ck];
        if (!cell) {
          html += '<td style="padding:6px; border:1px solid #334155; background:#0f172a; color:#475569; text-align:center;">-</td>';
          return;
        }
        var rate = cell[SEGMENT_METRIC_FIELD[metricKey]];
        var color = heatCellColor(rate, cell.total, minN);
        var label = cell.total < minN ? "-" : rate.toFixed(0) + "%";
        var tip = "n=" + cell.total + " / Bad " + cell.bad + "件 / Good " + cell.good + "件 / Bad率 " +
          pctFromPercent(cell.bad_rate) + " / Good率 " + pctFromPercent(cell.good_rate);
        html += '<td style="padding:6px; border:1px solid #334155; background:' + color.bg + "; color:" + color.fg +
          '; text-align:center; font-weight:600;" title="' + esc(tip) + '">' + label +
          '<br><span style="font-size:9px; font-weight:400; opacity:0.85;">n=' + cell.total + "</span></td>";
      });
      html += "</tr>";
    });
    html += "</tbody></table>";
    html += '<div style="font-size:10.5px; color:#64748b; margin-top:6px;">表示中: ' + esc(SEGMENT_METRIC_LABEL[metricKey]) +
      "(" + esc(matrix.row_label) + " × " + esc(matrix.col_label) + ") / 最低件数 n≥" + minN +
      " / セル数 " + (matrix.row_keys.length * matrix.col_keys.length) + "</div>";
    el.innerHTML = html;
  }

  // ---- B付録: モデル指標 ------------------------------------------------------

  function renderMetrics(metrics) {
    var el = byId("p12-metrics-table");
    if (!el) return;
    if (!metrics.entries.length) {
      el.innerHTML = '<div style="padding:14px; color:#94a3b8;">モデル指標 データなし</div>';
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
    el.innerHTML = html;
  }

  // ---- ソース透明性フッター(GAS版に無い新設要素) -------------------------------

  function renderSourcesFooter(sources) {
    var el = ensureExtraEl("cq-p12-sources", "font-size:10px; color:#475569; margin-top:16px;");
    if (!el) return;
    el.textContent = "データソース: " + sources.map(function (s) {
      return s.sheet + "(" + num(s.matched_rows) + "/" + num(s.total_rows) + "行、" +
        (s.from_cache ? "キャッシュ " + s.age_secs + "秒前" : "取得直後") + ")";
    }).join(" / ");
    var root = byId(ROOT_ID);
    if (root) root.appendChild(el); // フッターなので末尾へ(ensureExtraElは先頭挿入のため付け替える)
  }

  // ---- 起動 ------------------------------------------------------------------

  function init() {
    var root = byId(ROOT_ID);
    if (!root || root.getAttribute("data-cq-bound") === "1") return;
    root.setAttribute("data-cq-bound", "1");

    ["p12-d-axis", "p12-d-metric", "p12-d-min-n"].forEach(function (id) {
      var sel = byId(id);
      if (sel) sel.addEventListener("change", redrawSegment);
    });

    var statusEl = ensureExtraEl("cq-p12-status", "font-size:11px; color:#64748b; margin-bottom:8px;");
    if (statusEl) statusEl.textContent = "読み込み中...";

    fetch(ENDPOINT, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        var d = payload.data;
        if (statusEl) statusEl.textContent = "取得完了(サーバ処理 " + payload.elapsed_ms + "ms)";
        renderPredictionKpis(d.prediction);
        renderPredictionTable(d.prediction);
        renderPattern(d.pattern);
        renderConsultants(d.consultants);
        renderSegmentHeatmap(d.segment_matrices);
        renderMetrics(d.metrics);
        renderSourcesFooter(payload.sources);
      })
      .catch(function (err) {
        if (statusEl) statusEl.textContent = "取得失敗: " + (err && err.message ? err.message : err) + "。時間をおいて再読み込みしてください。";
      });
  }

  // 手動再初期化用に公開(index.js が使わなくても、この行までの自己実行だけで動く)
  window.cqChurnInit = init;

  // 遅延読込された時点で DOM は既に存在している前提で、読み込まれ次第すぐ実行する。
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
