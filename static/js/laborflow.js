/**
 * 人材フロー（業種別従業員増減）パネル
 * 都道府県選択時にSalesNow企業データから業種別の従業員変動を可視化する。
 * ECharts横棒グラフ + サマリーテーブルで表示。
 */
(function() {
  "use strict";

  var chart = null;

  /**
   * 都道府県を指定して人材フローデータをロード・描画
   * @param {string} prefecture - 都道府県名
   */
  window.loadLaborFlow = function(prefecture) {
    var panel = document.getElementById("jm-labor-flow");
    var chartEl = document.getElementById("jm-labor-flow-chart");
    var tableEl = document.getElementById("jm-labor-flow-table");
    var titleEl = document.getElementById("jm-labor-flow-title");

    if (!panel || !chartEl) return;

    if (!prefecture) {
      panel.style.display = "none";
      return;
    }

    panel.style.display = "block";
    if (titleEl) titleEl.textContent = prefecture + " - 人材フロー（業種別）";

    // ローディング表示
    if (tableEl) tableEl.innerHTML = '<span class="text-gray-400">読み込み中...</span>';

    fetch("/api/jobmap/labor-flow?prefecture=" + encodeURIComponent(prefecture))
      .then(function(r) { return r.json(); })
      .then(function(data) {
        if (data.error) {
          if (tableEl) tableEl.innerHTML = '<span class="text-red-400">' + escapeText(data.error) + '</span>';
          return;
        }
        renderChart(chartEl, data);
        renderTable(tableEl, data);
      })
      .catch(function(err) {
        console.warn("[laborflow] fetch error:", err);
        if (tableEl) tableEl.innerHTML = '<span class="text-red-400">データ取得に失敗しました</span>';
      });
  };

  /**
   * ECharts横棒グラフを描画
   * 正の変動は緑、負の変動は赤で表示
   */
  function renderChart(el, data) {
    if (typeof echarts === "undefined") {
      el.innerHTML = '<span class="text-gray-400 text-xs">ECharts未読込</span>';
      return;
    }

    var industries = data.industries || [];
    if (industries.length === 0) {
      el.innerHTML = '<span class="text-gray-400 text-xs">該当データなし</span>';
      return;
    }

    // 全業種を表示（net_change_1yの絶対値が大きい順）
    var sorted = industries.slice().sort(function(a, b) {
      return Math.abs(b.net_change_1y) - Math.abs(a.net_change_1y);
    });
    var top = sorted;
    // チャート表示用に昇順（下から大きい値）
    top.reverse();

    var names = top.map(function(d) { return d.sn_industry; });
    var values = top.map(function(d) { return d.net_change_1y; });

    // 既存チャートがあれば破棄
    if (chart) {
      chart.dispose();
      chart = null;
    }

    // チャート高さを業種数に応じて動的設定（1業種あたり25px、最低200px）
    var chartHeight = Math.max(200, top.length * 25 + 50);
    el.style.height = chartHeight + "px";
    chart = echarts.init(el);

    var option = {
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "shadow" },
        backgroundColor: "rgba(15,23,42,0.95)",
        borderColor: "#475569",
        textStyle: { color: "#e2e8f0", fontSize: 12 },
        formatter: function(params) {
          var p = params[0];
          var idx = top.length - 1 - p.dataIndex;
          var d = top[top.length - 1 - p.dataIndex];
          if (!d) return "";
          var sign = d.net_change_1y >= 0 ? "+" : "";
          return '<div style="font-weight:bold;">' + escapeText(d.sn_industry) + '</div>'
            + '<div style="margin-top:4px;">'
            + '1Y変動: <span style="color:' + (d.net_change_1y >= 0 ? '#22c55e' : '#ef4444') + ';">'
            + sign + d.net_change_1y.toLocaleString() + '人</span>'
            + '</div>'
            + '<div>3M変動: ' + (d.net_change_3m >= 0 ? '+' : '') + d.net_change_3m.toLocaleString() + '人</div>'
            + '<div>企業数: ' + d.companies.toLocaleString() + '社</div>'
            + '<div>総従業員: ' + d.total_emp.toLocaleString() + '人</div>'
            + '<div>平均変動率: ' + (d.avg_delta_1y >= 0 ? '+' : '') + d.avg_delta_1y + '%</div>';
        }
      },
      grid: {
        left: 120,
        right: 60,
        top: 10,
        bottom: 30
      },
      xAxis: {
        type: "value",
        axisLabel: {
          fontSize: 10,
          color: "#94a3b8",
          formatter: function(v) {
            if (Math.abs(v) >= 1000) return (v / 1000).toFixed(0) + "K";
            return v;
          }
        },
        axisLine: { lineStyle: { color: "#334155" } },
        splitLine: { lineStyle: { color: "#1e293b" } }
      },
      yAxis: {
        type: "category",
        data: names,
        axisLabel: {
          fontSize: 10,
          color: "#cbd5e1",
          width: 100,
          overflow: "truncate"
        },
        axisLine: { lineStyle: { color: "#334155" } },
        axisTick: { show: false }
      },
      series: [{
        type: "bar",
        data: values.map(function(v) {
          return {
            value: v,
            itemStyle: {
              color: v >= 0 ? "#22c55e" : "#ef4444",
              borderRadius: v >= 0 ? [0, 3, 3, 0] : [3, 0, 0, 3]
            }
          };
        }),
        barMaxWidth: 20,
        label: {
          show: true,
          position: "right",
          fontSize: 10,
          color: "#94a3b8",
          formatter: function(p) {
            var v = p.value;
            if (v === 0) return "";
            return (v >= 0 ? "+" : "") + v.toLocaleString();
          }
        }
      }]
    };

    chart.setOption(option);

    // リサイズ対応
    var resizeTimer = null;
    window.addEventListener("resize", function() {
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = setTimeout(function() {
        if (chart && !chart.isDisposed()) chart.resize();
      }, 200);
    });
  }

  /**
   * サマリーテーブルを描画
   */
  function renderTable(el, data) {
    if (!el) return;
    var industries = data.industries || [];
    if (industries.length === 0) {
      el.innerHTML = "";
      return;
    }

    // 全体サマリー行
    var totalEmp = 0, totalChange1y = 0, totalChange3m = 0, totalCompanies = 0;
    industries.forEach(function(d) {
      totalEmp += d.total_emp || 0;
      totalChange1y += d.net_change_1y || 0;
      totalChange3m += d.net_change_3m || 0;
      totalCompanies += d.companies || 0;
    });

    var html = '<div class="flex gap-4 flex-wrap mb-2 text-gray-300">'
      + '<span>対象企業: <strong>' + totalCompanies.toLocaleString() + '社</strong></span>'
      + '<span>総従業員: <strong>' + totalEmp.toLocaleString() + '人</strong></span>'
      + '<span>1Y純増減: <strong style="color:' + (totalChange1y >= 0 ? '#22c55e' : '#ef4444') + ';">'
      + (totalChange1y >= 0 ? '+' : '') + totalChange1y.toLocaleString() + '人</strong></span>'
      + '<span>3M純増減: <strong style="color:' + (totalChange3m >= 0 ? '#22c55e' : '#ef4444') + ';">'
      + (totalChange3m >= 0 ? '+' : '') + totalChange3m.toLocaleString() + '人</strong></span>'
      + '</div>';

    // テーブル（全業種表示）
    html += '<div class="overflow-y-auto" style="max-height:400px;">';
    html += '<table class="w-full text-left border-collapse">';
    html += '<thead><tr class="border-b border-gray-700 text-gray-400 sticky top-0 bg-gray-900">'
      + '<th class="py-1 pr-2">業種</th>'
      + '<th class="py-1 px-2 text-right">企業数</th>'
      + '<th class="py-1 px-2 text-right">従業員</th>'
      + '<th class="py-1 px-2 text-right">1Y増減</th>'
      + '<th class="py-1 px-2 text-right">3M増減</th>'
      + '<th class="py-1 px-2 text-right">平均変動率</th>'
      + '</tr></thead><tbody>';

    for (var i = 0; i < industries.length; i++) {
      var d = industries[i];
      var c1y = d.net_change_1y || 0;
      var c3m = d.net_change_3m || 0;
      html += '<tr class="border-b border-gray-800 hover:bg-gray-800/50">'
        + '<td class="py-1 pr-2 text-gray-200 max-w-[160px] truncate" title="' + escapeAttr(d.sn_industry) + '">' + escapeText(d.sn_industry) + '</td>'
        + '<td class="py-1 px-2 text-right text-gray-300">' + d.companies.toLocaleString() + '</td>'
        + '<td class="py-1 px-2 text-right text-gray-300">' + d.total_emp.toLocaleString() + '</td>'
        + '<td class="py-1 px-2 text-right" style="color:' + (c1y >= 0 ? '#22c55e' : '#ef4444') + ';">'
        + (c1y >= 0 ? '+' : '') + c1y.toLocaleString() + '</td>'
        + '<td class="py-1 px-2 text-right" style="color:' + (c3m >= 0 ? '#22c55e' : '#ef4444') + ';">'
        + (c3m >= 0 ? '+' : '') + c3m.toLocaleString() + '</td>'
        + '<td class="py-1 px-2 text-right text-gray-300">'
        + (d.avg_delta_1y >= 0 ? '+' : '') + d.avg_delta_1y + '%</td>'
        + '</tr>';
    }

    html += '</tbody></table></div>';
    el.innerHTML = html;
  }

  /** テキストをHTMLエスケープ */
  function escapeText(s) {
    if (!s) return "";
    var d = document.createElement("div");
    d.appendChild(document.createTextNode(s));
    return d.innerHTML;
  }

  /** 属性値をHTMLエスケープ */
  function escapeAttr(s) {
    return escapeText(s).replace(/"/g, "&quot;");
  }

})();
