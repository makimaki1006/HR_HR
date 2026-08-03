/*
 * 媒体分析タブ「データ探索（動的）」パネル (2026-08-04)。
 *
 * レポートHTMLへの一方通行だった集計を、アプリ内で操作しながら見られるようにする。
 * データ源は /api/survey/report?session_id=… — アップロード時にサーバーへ保存した
 * 集計そのもの (survey_agg_{session_id}) を返すため、レポートと数字が食い違わない。
 * このファイルは再計算をせず、既に集計済みの配列の並べ替え・絞り込み・ビン幅変更だけを行う。
 *
 * 初期化: #survey-explore[data-session-id] が DOM に現れたら1回だけ bind する
 * (htmx:afterSettle と DOMContentLoaded の両方を監視。app.js の data-chart-config
 *  自動初期化とは独立させるため、チャート要素は .echart クラスを使わない)。
 */
(function () {
  "use strict";

  var PALETTE = ["#5b8ff9", "#5ad8a6", "#f6bd16", "#e8684a", "#6dc8ec", "#9270ca", "#ff9d4d", "#269a99"];
  var INSTANCES = [];

  function yen(v) {
    if (v == null || isNaN(v)) return "—";
    return Number(v).toLocaleString("ja-JP") + "円";
  }

  function initChart(el) {
    if (typeof echarts === "undefined" || !el) return null;
    var existing = echarts.getInstanceByDom(el);
    if (existing) existing.dispose();
    var chart = echarts.init(el, "dark");
    INSTANCES.push(chart);
    return chart;
  }

  // 探索パネルは app.js の ResizeObserver 対象 (.echart) から意図的に外れているため、
  // リサイズ追随を自前で持つ (2026-08-04 レビュー指摘)。
  var resizeTimer = null;
  window.addEventListener("resize", function () {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () {
      INSTANCES = INSTANCES.filter(function (chart) { return !chart.isDisposed(); });
      INSTANCES.forEach(function (chart) { chart.resize(); });
    }, 200);
  });

  function baseOption(extra) {
    var option = {
      backgroundColor: "transparent",
      tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
      grid: { left: "12%", right: "5%", top: "12%", bottom: "18%" },
      aria: { enabled: true, decal: { show: true } }
    };
    Object.keys(extra || {}).forEach(function (k) { option[k] = extra[k]; });
    return option;
  }

  // ---- 各チャート ----------------------------------------------------------

  function renderMunicipality(root, agg) {
    var el = root.querySelector("[data-explore-chart='municipality']");
    var metricSel = root.querySelector("[data-explore-control='muni-metric']");
    var orderSel = root.querySelector("[data-explore-control='muni-order']");
    if (!el) return;
    var rows = (agg.by_municipality_salary || []).slice();
    if (!rows.length) { el.parentElement.hidden = true; return; }

    function draw() {
      var metric = metricSel ? metricSel.value : "count";
      var sorted = rows.slice().sort(function (a, b) {
        var d = (b[metric] || 0) - (a[metric] || 0);
        return d !== 0 ? d : a.name.localeCompare(b.name, "ja");
      });
      if (orderSel && orderSel.value === "asc") sorted.reverse();
      var labels = sorted.map(function (r) { return r.name; });
      var values = sorted.map(function (r) { return r[metric] || 0; });
      var isCount = metric === "count";
      var chart = initChart(el);
      if (!chart) return;
      chart.setOption(baseOption({
        tooltip: {
          trigger: "axis", axisPointer: { type: "shadow" },
          formatter: function (params) {
            var row = sorted[params[0].dataIndex];
            return row.prefecture + " " + row.name +
              "<br/>件数: " + row.count + "件" +
              "<br/>給与中央値: " + yen(row.median_salary) +
              "<br/>給与平均: " + yen(row.avg_salary);
          }
        },
        xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10, formatter: isCount ? "{value}件" : function (v) { return (v / 10000).toFixed(0) + "万"; } } },
        yAxis: { type: "category", data: labels.slice().reverse(), axisLabel: { color: "#cbd5e1", fontSize: 10 } },
        series: [{ type: "bar", data: values.slice().reverse(), itemStyle: { color: PALETTE[0] } }]
      }));
    }
    if (metricSel) metricSel.addEventListener("change", draw);
    if (orderSel) orderSel.addEventListener("change", draw);
    draw();
  }

  function renderHistogram(root, agg) {
    var el = root.querySelector("[data-explore-chart='histogram']");
    var binSel = root.querySelector("[data-explore-control='bin-width']");
    if (!el) return;
    var values = (agg.salary_values || []).filter(function (v) { return typeof v === "number" && v > 0; });
    if (!values.length) { el.parentElement.hidden = true; return; }

    function draw() {
      var bin = binSel ? Number(binSel.value) : 50000;
      // Math.min.apply は数万件で引数上限の RangeError になるためループで求める
      var rawMin = Infinity, max = -Infinity;
      for (var vi = 0; vi < values.length; vi++) {
        if (values[vi] < rawMin) rawMin = values[vi];
        if (values[vi] > max) max = values[vi];
      }
      var min = Math.floor(rawMin / bin) * bin;
      var buckets = [];
      for (var b = min; b <= max; b += bin) buckets.push(0);
      values.forEach(function (v) {
        var index = Math.floor((v - min) / bin);
        if (index >= 0 && index < buckets.length) buckets[index] += 1;
      });
      var labels = buckets.map(function (_, i) { return ((min + i * bin) / 10000).toFixed(1) + "万"; });
      var chart = initChart(el);
      if (!chart) return;
      chart.setOption(baseOption({
        tooltip: { trigger: "axis", axisPointer: { type: "shadow" }, formatter: "{b}円台<br/>件数: {c}" },
        xAxis: { type: "category", data: labels, axisLabel: { color: "#94a3b8", fontSize: 9, rotate: 45 } },
        yAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10 } },
        series: [{ type: "bar", data: buckets, barCategoryGap: "10%", itemStyle: { color: PALETTE[2] } }]
      }));
    }
    if (binSel) binSel.addEventListener("change", draw);
    draw();
  }

  function renderEmployment(root, agg) {
    var el = root.querySelector("[data-explore-chart='employment']");
    if (!el) return;
    var rows = agg.by_emp_type_salary || [];
    if (!rows.length) { el.parentElement.hidden = true; return; }
    var chart = initChart(el);
    if (!chart) return;
    chart.setOption({
      backgroundColor: "transparent",
      aria: { enabled: true, decal: { show: true } },
      tooltip: {
        trigger: "item",
        formatter: function (p) {
          var row = rows[p.dataIndex];
          return row.emp_type + "<br/>件数: " + row.count + "件<br/>給与中央値: " + yen(row.median_salary);
        }
      },
      legend: { bottom: 0, textStyle: { color: "#94a3b8", fontSize: 10 } },
      series: [{
        type: "pie", radius: ["38%", "62%"], center: ["50%", "44%"],
        label: { color: "#cbd5e1", fontSize: 10, formatter: "{b}\n{c}件" },
        data: rows.map(function (r, i) {
          return { name: r.emp_type, value: r.count, itemStyle: { color: PALETTE[i % PALETTE.length] } };
        })
      }]
    });
  }

  function renderTags(root, agg) {
    var el = root.querySelector("[data-explore-chart='tags']");
    var minCountSel = root.querySelector("[data-explore-control='tag-min-count']");
    if (!el) return;
    var rows = (agg.by_tag_salary || []).slice();
    if (!rows.length) { el.parentElement.hidden = true; return; }

    function draw() {
      var minCount = minCountSel ? Number(minCountSel.value) : 5;
      var filtered = rows.filter(function (r) { return r.count >= minCount; })
        .sort(function (a, b) { return b.diff_from_avg - a.diff_from_avg; })
        .slice(0, 14);
      var chart = initChart(el);
      if (!chart) return;
      chart.setOption(baseOption({
        tooltip: {
          trigger: "axis", axisPointer: { type: "shadow" },
          formatter: function (params) {
            var row = filtered[params[0].dataIndex];
            return row.tag + "<br/>件数: " + row.count + "件<br/>平均給与: " + yen(row.avg_salary) +
              "<br/>全体平均との差: " + (row.diff_from_avg >= 0 ? "+" : "") + yen(row.diff_from_avg);
          }
        },
        grid: { left: "22%", right: "8%", top: "6%", bottom: "10%" },
        // 差分は数千円規模もあるため 0.1万円単位で表示 (整数万だと全目盛りが「0万」になる)
        xAxis: { type: "value", axisLabel: { color: "#94a3b8", fontSize: 10, formatter: function (v) { return (v / 10000).toFixed(1) + "万"; } } },
        yAxis: { type: "category", data: filtered.map(function (r) { return r.tag; }).reverse(), axisLabel: { color: "#cbd5e1", fontSize: 10 } },
        series: [{
          type: "bar",
          data: filtered.map(function (r) {
            return { value: r.diff_from_avg, itemStyle: { color: r.diff_from_avg >= 0 ? PALETTE[1] : PALETTE[3] } };
          }).reverse()
        }]
      }));
    }
    if (minCountSel) minCountSel.addEventListener("change", draw);
    draw();
  }

  // ---- 起動 ---------------------------------------------------------------

  function bind(root) {
    if (!root || root.getAttribute("data-bound") === "1") return;
    root.setAttribute("data-bound", "1");
    var sessionId = root.getAttribute("data-session-id") || "";
    var status = root.querySelector("[data-explore-status]");
    if (!sessionId) return;
    // チャートライブラリが読めていない場合は空枠でなくメッセージを出す
    // (SRI 検証失敗や CDN 障害で silent に4枠が空になるのを防ぐ)
    if (typeof echarts === "undefined") {
      if (status) status.textContent = "チャート描画ライブラリを読み込めませんでした。ページを再読み込みしてください。";
      return;
    }
    fetch("/api/survey/report?session_id=" + encodeURIComponent(sessionId), { credentials: "same-origin" })
      .then(function (res) { return res.json(); })
      .then(function (data) {
        if (data.status !== "ok" || !data.aggregation) {
          if (status) status.textContent = data.message || "集計を取得できませんでした。CSVを再アップロードしてください。";
          return;
        }
        var agg = data.aggregation;
        if (status) {
          if (agg.is_hourly) {
            // 時給モードのレポートはネイティブ値(円/時)で表記されるため、
            // 換算方法の違いを明示する (「レポートと同じ数字」の例外)
            status.textContent = "時給中心のデータのため、このパネルは月給換算（×167時間）で表示しています。レポート本体は時給表記です。";
          } else {
            status.hidden = true;
          }
        }
        renderMunicipality(root, agg);
        renderHistogram(root, agg);
        renderEmployment(root, agg);
        renderTags(root, agg);
      })
      .catch(function () {
        if (status) status.textContent = "集計の取得に失敗しました。時間をおいて再読み込みしてください。";
      });
  }

  function scan(scope) {
    (scope || document).querySelectorAll("#survey-explore[data-session-id]").forEach(bind);
  }

  // アップロード完了ハンドラ (render.rs) が innerHTML 挿入後に明示的に呼ぶ入口。
  // innerHTML + htmx.process() は htmx:afterSettle を発火しないため、
  // イベント監視だけではパネルが一度も初期化されない (2026-08-04 レビューで判明)。
  window.surveyExploreScan = scan;

  document.addEventListener("DOMContentLoaded", function () { scan(document); });
  document.body.addEventListener("htmx:afterSettle", function (event) {
    scan(event.detail && event.detail.target ? event.detail.target.parentElement || document : document);
  });
})();
