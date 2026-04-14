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
  window.loadLaborFlow = function(prefecture, municipality) {
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
    var locationLabel = municipality ? prefecture + " " + municipality : prefecture;
    if (titleEl) titleEl.textContent = locationLabel + " - 人材フロー（業種別）";

    // ローディング表示
    if (tableEl) tableEl.innerHTML = '<span class="text-gray-400">読み込み中...</span>';

    var url = "/api/jobmap/labor-flow?prefecture=" + encodeURIComponent(prefecture);
    if (municipality) url += "&municipality=" + encodeURIComponent(municipality);
    fetch(url)
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
          var d = top[p.dataIndex];
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

    // 現在の都道府県・市区町村を保持（企業一覧取得用）
    var currentPref = data.prefecture || "";
    var currentMuni = data.municipality || "";

    for (var i = 0; i < industries.length; i++) {
      var d = industries[i];
      var c1y = d.net_change_1y || 0;
      var c3m = d.net_change_3m || 0;
      html += '<tr class="border-b border-gray-800 hover:bg-gray-700/50 cursor-pointer" onclick="loadIndustryCompanies(\'' + escapeAttr(currentPref) + '\',\'' + escapeAttr(currentMuni) + '\',\'' + escapeAttr(d.sn_industry) + '\')">'
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

  /**
   * 業種クリック → 企業一覧を表示
   * 企業名は escapeText() でXSSサニタイズ済み
   */
  window.loadIndustryCompanies = function(pref, muni, industry) {
    var tableEl = document.getElementById("jm-labor-flow-table");
    if (!tableEl) return;

    var url = "/api/jobmap/industry-companies?prefecture=" + encodeURIComponent(pref)
            + "&industry=" + encodeURIComponent(industry);
    if (muni) url += "&municipality=" + encodeURIComponent(muni);

    // ローディング表示（テキストのみ）
    while (tableEl.firstChild) tableEl.removeChild(tableEl.firstChild);
    var loadingSpan = document.createElement("span");
    loadingSpan.className = "text-gray-400";
    loadingSpan.textContent = "「" + industry + "」の企業を読み込み中...";
    tableEl.appendChild(loadingSpan);

    fetch(url)
      .then(function(r) { return r.json(); })
      .then(function(data) {
        while (tableEl.firstChild) tableEl.removeChild(tableEl.firstChild);

        if (data.error) {
          var errSpan = document.createElement("span");
          errSpan.className = "text-red-400";
          errSpan.textContent = data.error;
          tableEl.appendChild(errSpan);
          return;
        }

        var companies = data.companies || [];

        // 戻るボタン + タイトル
        var header = document.createElement("div");
        header.className = "mb-2 flex items-center gap-2";
        var backBtn = document.createElement("button");
        backBtn.className = "text-xs text-slate-400 hover:text-white bg-slate-800 px-2 py-1 rounded";
        backBtn.textContent = "\u2190 業種一覧に戻る";
        backBtn.onclick = function() { loadLaborFlow(pref, muni); };
        header.appendChild(backBtn);
        var titleSpan = document.createElement("span");
        titleSpan.className = "text-sm text-white font-medium";
        titleSpan.textContent = industry;
        header.appendChild(titleSpan);
        var countSpan = document.createElement("span");
        countSpan.className = "text-xs text-slate-400";
        countSpan.textContent = companies.length + "\u793e";
        header.appendChild(countSpan);
        tableEl.appendChild(header);

        // 一括操作バー（選択件数 + CSVダウンロード）
        var bulkBar = document.createElement("div");
        bulkBar.className = "mb-2 flex items-center gap-2 text-xs";
        var selInfo = document.createElement("span");
        selInfo.id = "jm-bulk-sel-count";
        selInfo.className = "text-slate-400";
        selInfo.textContent = "0 社選択中";
        var dlBtn = document.createElement("button");
        dlBtn.id = "jm-bulk-dl-btn";
        dlBtn.className = "px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white disabled:bg-slate-700 disabled:text-slate-500 disabled:cursor-not-allowed";
        dlBtn.textContent = "選択した企業をCSVダウンロード";
        dlBtn.disabled = true;
        dlBtn.onclick = function() {
          var checked = Array.prototype.slice
            .call(document.querySelectorAll(".jm-comp-chk:checked"))
            .map(function(el) { return el.value; })
            .filter(Boolean);
          if (checked.length === 0) return;
          var url = "/api/company/bulk-csv?corps=" + encodeURIComponent(checked.join(","));
          // 新規タブでダウンロード
          var a = document.createElement("a");
          a.href = url;
          a.download = "companies_compare.csv";
          document.body.appendChild(a);
          a.click();
          document.body.removeChild(a);
        };
        bulkBar.appendChild(selInfo);
        bulkBar.appendChild(dlBtn);
        tableEl.appendChild(bulkBar);

        function updateBulkUI() {
          var n = document.querySelectorAll(".jm-comp-chk:checked").length;
          selInfo.textContent = n + " \u793e\u9078\u629e\u4e2d"; // 社選択中
          dlBtn.disabled = n === 0;
        }

        // テーブル構築（DOM API使用、XSS安全）
        var scrollDiv = document.createElement("div");
        scrollDiv.className = "overflow-y-auto";
        scrollDiv.style.maxHeight = "400px";
        var table = document.createElement("table");
        table.className = "w-full text-left border-collapse text-xs";
        var thead = document.createElement("thead");
        var headRow = document.createElement("tr");
        headRow.className = "border-b border-gray-700 text-gray-400 sticky top-0 bg-gray-900";
        // 全選択チェックボックス列
        var thChk = document.createElement("th");
        thChk.className = "py-1 pr-1 w-6";
        var thChkInput = document.createElement("input");
        thChkInput.type = "checkbox";
        thChkInput.title = "全選択/全解除";
        thChkInput.onclick = function(e) {
          var checked = e.target.checked;
          document.querySelectorAll(".jm-comp-chk").forEach(function(chk) { chk.checked = checked; });
          updateBulkUI();
          e.stopPropagation();
        };
        thChk.appendChild(thChkInput);
        headRow.appendChild(thChk);

        ["企業名","従業員","1M","3M","1Y","信用"].forEach(function(t, idx) {
          var th = document.createElement("th");
          th.className = idx === 0 ? "py-1 pr-2" : "py-1 px-2 text-right";
          th.textContent = t;
          headRow.appendChild(th);
        });
        thead.appendChild(headRow);
        table.appendChild(thead);

        var tbody = document.createElement("tbody");
        companies.forEach(function(c) {
          var tr = document.createElement("tr");
          tr.className = "border-b border-gray-800 hover:bg-gray-700/50";

          // チェックボックス列（行クリックとは独立）
          var tdChk = document.createElement("td");
          tdChk.className = "py-1 pr-1 text-center";
          var chk = document.createElement("input");
          chk.type = "checkbox";
          chk.className = "jm-comp-chk cursor-pointer";
          chk.value = c.corporate_number;
          chk.onclick = function(e) { updateBulkUI(); e.stopPropagation(); };
          tdChk.appendChild(chk);
          tdChk.onclick = function(e) { e.stopPropagation(); };
          tr.appendChild(tdChk);

          var tdName = document.createElement("td");
          tdName.className = "py-1 pr-2 text-blue-400 cursor-pointer";
          tdName.textContent = c.company_name;
          tdName.onclick = function() {
            window._lastTab = "/tab/jobmap";
            var el = document.getElementById("content");
            if (el && typeof htmx !== "undefined") {
              htmx.ajax("GET", "/api/company/profile/" + encodeURIComponent(c.corporate_number), {target: el, swap: "innerHTML"});
            }
          };
          tr.appendChild(tdName);

          var tdEmp = document.createElement("td");
          tdEmp.className = "py-1 px-2 text-right text-gray-300";
          tdEmp.textContent = c.employee_count > 0 ? c.employee_count.toLocaleString() : "-";
          tr.appendChild(tdEmp);

          [c.employee_delta_1m, c.employee_delta_3m, c.employee_delta_1y].forEach(function(d) {
            var td = document.createElement("td");
            td.className = "py-1 px-2 text-right";
            td.style.color = d >= 0 ? "#22c55e" : "#ef4444";
            td.textContent = d !== 0 ? (d > 0 ? "+" : "") + d.toFixed(1) + "%" : "-";
            tr.appendChild(td);
          });

          var tdCredit = document.createElement("td");
          tdCredit.className = "py-1 px-2 text-right text-gray-300";
          tdCredit.textContent = c.credit_score > 0 ? c.credit_score.toFixed(0) : "-";
          tr.appendChild(tdCredit);

          tbody.appendChild(tr);
        });
        table.appendChild(tbody);
        scrollDiv.appendChild(table);
        tableEl.appendChild(scrollDiv);
      })
      .catch(function(err) {
        while (tableEl.firstChild) tableEl.removeChild(tableEl.firstChild);
        var errSpan = document.createElement("span");
        errSpan.className = "text-red-400";
        errSpan.textContent = "取得失敗";
        tableEl.appendChild(errSpan);
      });
  };

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
