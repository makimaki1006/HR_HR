/*
 * P13 案件タイムライン(Zoom AI Companion 議事録)タブ。
 *
 * データ源:
 *   GET /api/call-quality/timeline/deals (URLは cq:tab-shown イベントの detail.api から取る)
 *     (Rust: get_deal_index, src/handlers/call_quality/tabs/p13_timeline.rs)
 *     Deal一覧 + 担当者フィルタの選択肢。タブを開いたときに1回だけ取得する。
 *   GET /api/call-quality/timeline/deal?deal_id=...&sort=...&period=...
 *     (Rust: get_deal_detail、同ファイル。パスはチームリード確定分をハードコード。
 *     detail.api は「タブの主API」= 一覧側のパスしか渡ってこないため、
 *     Deal選択時の詳細取得だけは自前で組み立てる)
 *     Deal選択時に取得する詳細(概要カード・接触×NPS推移・MTGタイムライン)。
 *
 * 再fetchの方針(担当チームリード確認済み、2026-08-16):
 *   - 担当者フィルタ・キーワード検索: Deal一覧(deals)を絞り込むだけで、
 *     どの率の分母も変えない「ピッカーの絞り込み」なので**再fetchしない**
 *     (取得済み一覧をクライアント側で filter するだけ)。
 *   - Deal選択・並び順(sort)・期間(period): /timeline/deal を**都度re-fetchする**。
 *     このエンドポイントは deal_id/sort/period をサーバ側の絞り込み条件として
 *     そのまま使う設計(Rust側 DealDetailQuery)であり、カード一覧と件数
 *     (meetings_shown/meetings_total)を1つの整合したペイロードとして返すため、
 *     クライアント側で古い一覧と新しい件数を混在させるリスクが構造的に発生しない。
 *
 * GAS版(javascript.html renderP13MtgTimeline 以下)との違い:
 *   GAS版は5シートを生データのままブラウザに送り、Deal単位のグルーピング・
 *   信頼度フィルタ(host_email_match除外)・週次MTG集計・NPS系列展開を
 *   ブラウザ側でやっていた。このファイルは**再集計しない**。
 *   サーバが返した Deal 一覧 / MeetingCard / 週次点 / NPS点をそのまま描く。
 *
 * ---- 起動契約(2026-08-16 テンプレート実物に合わせて確定) -------------------
 * _layout.html が動的 import() でこのファイルを読み込み、読み込み完了後に
 * document へ `cq:tab-shown` を dispatch する(detail: {tab, api, el})。
 * このファイルは import された時点では何もせず、イベント購読だけ登録する。
 * 同じタブを再表示するたびに同イベントが発火するので、Deal一覧の初回取得だけ
 * el(タブの .cq-page 要素)に data-cq-bound を立てて二重取得を防ぐ
 * (選択中Dealの表示状態はDOMに残ったままなので、2回目以降は何もしなくてよい)。
 *
 * ---- DOM契約(templates/tabs/call_quality/p13.html が正) -----------------
 * ルート: .cq-page[data-cq-tab="p13"]
 * 子要素: [data-cq-control="p13-consultant-filter"] (select)
 *         [data-cq-control="p13-deal-select"] (select)
 *         [data-cq-control="p13-deal-search"] (input)
 *         [data-cq-table="p13-deal-suggest"] (div, class="p13-suggest" 済み)
 *         [data-cq-control="p13-sort"] (select) / [data-cq-control="p13-period"] (select)
 *         [data-cq-table="p13-deal-summary"] (div。.p13-deal-card 構造をJSが差し込む)
 *         [data-cq-status]
 *         [data-cq-table="p13-trend-card"] (パネルごとhidden切替。中に1チャートのみ)
 *         [data-cq-chart="p13-contact-npm"] (接触×NPSを1チャートに統合。GAS版は2チャート
 *           だったが枠が1つのため、上下2グリッドの複合チャートにした)
 *         [data-cq-table="p13-mtg-history"] (div、class="p13-timeline-list" 済み。
 *           .p13-mtg-card をJSが横並びで差し込む)
 *         [data-cq-truncated] / [data-cq-sources] (共通パーシャル)
 */
(function () {
  "use strict";

  var TAB_ID = "p13";
  var DEAL_ENDPOINT = "/api/call-quality/timeline/deal";
  var NA = "—";

  function num(v) {
    return window.ChartHelpers ? window.ChartHelpers.formatNumber(v) : String(v);
  }
  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }
  function q(el, sel) { return el.querySelector(sel); }

  var INSTANCES = [];
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
  var resizeTimer = null;
  window.addEventListener("resize", function () {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () {
      INSTANCES = INSTANCES.filter(function (c) { return !c.isDisposed(); });
      INSTANCES.forEach(function (c) { c.resize(); });
    }, 200);
  });

  function setStatus(el, text) {
    var s = q(el, "[data-cq-status]");
    if (s) s.textContent = text;
  }
  // 注: [data-cq-truncated] バナーは p12(Top20切り詰め)向けの共通パーシャルで、
  // p13 の payload(DealIndexData/DealDetail)には「黙って切り詰める」箇所が無いため
  // ここでは使わない(=未使用として消しているだけで、実装漏れではない)。

  function setSources(el, sources) {
    var box = q(el, "[data-cq-sources]");
    if (!box) return;
    box.innerHTML = sources.map(function (s) {
      return "<div>" + esc(s.sheet) + "(" + num(s.matched_rows) + "/" + num(s.total_rows) + "行、" +
        (s.from_cache ? "キャッシュ " + s.age_secs + "秒前" : "取得直後") + ")</div>";
    }).join("");
  }

  // ---- 状態(タブの .cq-page 要素に紐付けて持つ。複数回開いても使い回す) -------------

  function state(el) {
    if (!el._cq) el._cq = { deals: [], byId: {}, currentDealId: "" };
    return el._cq;
  }

  // ---- Deal 一覧: 取得・担当者プルダウン・検索候補 -----------------------------

  function loadDealIndex(el, apiUrl) {
    return fetch(apiUrl, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        var d = payload.data;
        var st = state(el);
        st.deals = d.deals;
        st.byId = {};
        d.deals.forEach(function (x) { st.byId[x.deal_id] = x; });
        setStatus(el, "Dealを選択してください(全 " + num(d.deals.length) + " Deal、MTG " + num(d.total_meetings) + " 件" +
          (d.skipped_low_confidence > 0 ? " / 低信頼紐付け " + num(d.skipped_low_confidence) + " 件を除外" : "") + ")");
        setSources(el, payload.sources);
        populateConsultantOptions(el, d.consultants, d.deals.length);
        populateDealOptions(el, "");
      });
  }

  function populateConsultantOptions(el, consultants, totalDeals) {
    var sel = q(el, '[data-cq-control="p13-consultant-filter"]');
    if (!sel) return;
    var html = '<option value="">全担当者 (' + num(totalDeals) + " Deal)</option>";
    html += consultants.map(function (c) {
      return '<option value="' + esc(c.name) + '">' + esc(c.name) + " (" + num(c.deal_count) + ")</option>";
    }).join("");
    sel.innerHTML = html;
  }

  function populateDealOptions(el, consultantName) {
    var sel = q(el, '[data-cq-control="p13-deal-select"]');
    if (!sel) return;
    var list = state(el).deals.slice();
    if (consultantName) list = list.filter(function (d) { return d.consultant_name === consultantName; });
    list.sort(function (a, b) { return a.customer_label < b.customer_label ? -1 : a.customer_label > b.customer_label ? 1 : 0; });
    var html = '<option value="">― Dealを選択 (' + num(list.length) + '件) ―</option>';
    html += list.map(function (d) {
      return '<option value="' + esc(d.deal_id) + '">' + esc(d.customer_label) + " (" + d.meeting_count + " MTG)</option>";
    }).join("");
    sel.innerHTML = html;
  }

  function updateSuggest(el) {
    var input = q(el, '[data-cq-control="p13-deal-search"]');
    var box = q(el, '[data-cq-table="p13-deal-suggest"]');
    if (!input || !box) return;
    var qv = (input.value || "").trim().toLowerCase();
    var list = state(el).deals.slice().sort(function (a, b) { return b.meeting_count - a.meeting_count; });
    var hits = qv ? list.filter(function (d) {
      return d.deal_id.toLowerCase().indexOf(qv) !== -1 ||
        (d.customer_label || "").toLowerCase().indexOf(qv) !== -1 ||
        (d.consultant_name || "").toLowerCase().indexOf(qv) !== -1;
    }) : list;
    hits = hits.slice(0, 50);
    if (!hits.length) {
      box.innerHTML = '<div class="p13-suggest-empty">該当案件なし</div>';
      box.hidden = false;
      return;
    }
    box.innerHTML = hits.map(function (d) {
      var last = d.latest_start_time ? d.latest_start_time.substring(0, 10) : "-";
      return '<div class="p13-suggest-item" data-deal-id="' + esc(d.deal_id) + '">' +
        "<div>" + esc(d.customer_label) + "</div>" +
        '<div class="p13-suggest-meta">Deal ' + esc(d.deal_id) + " / " + esc(d.consultant_name || "-") + " / " +
        esc(d.stage_label || "-") + " / MTG " + d.meeting_count + "件 / 最新 " + esc(last) + "</div></div>";
    }).join("");
    box.hidden = false;
    box.querySelectorAll(".p13-suggest-item").forEach(function (item) {
      item.addEventListener("click", function () {
        var did = item.getAttribute("data-deal-id");
        var d = state(el).byId[did];
        if (!d) return;
        input.value = d.customer_label + " (Deal " + did + ")";
        box.hidden = true;
        loadDealDetail(el, did);
      });
    });
  }

  // ---- Deal 詳細: 取得・描画 ----------------------------------------------------

  function loadDealDetail(el, dealId) {
    if (!dealId) return;
    state(el).currentDealId = dealId;
    setStatus(el, "読み込み中...");

    var sort = (q(el, '[data-cq-control="p13-sort"]') || {}).value || "desc";
    var period = (q(el, '[data-cq-control="p13-period"]') || {}).value || "all";

    var url = DEAL_ENDPOINT + "?deal_id=" + encodeURIComponent(dealId) +
      "&sort=" + encodeURIComponent(sort) + "&period=" + encodeURIComponent(period);
    fetch(url, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        // 選択が変わった後に古いレスポンスが遅れて返るケースの取り違え防止
        if (state(el).currentDealId !== dealId) return;
        renderDeal(el, payload.data);
      })
      .catch(function (err) {
        setStatus(el, "取得失敗: " + (err && err.message ? err.message : err));
      });
  }

  function renderDeal(el, d) {
    renderSummary(el, d);
    renderTrends(el, d);
    renderTimeline(el, d);
    setStatus(el, "表示: " + d.meetings_shown + " MTG / 全 " + d.meetings_total);
  }

  function renderSummary(el, d) {
    var host = q(el, '[data-cq-table="p13-deal-summary"]');
    if (!host) return;
    var phaseCell = "";
    if (d.phase_kpi) {
      var pk = d.phase_kpi;
      var bg = (pk.overall_flag || "").indexOf("🚨") >= 0 ? "rgba(220,38,38,0.15)"
        : (pk.overall_flag || "").indexOf("⚠") >= 0 ? "rgba(217,119,6,0.15)"
        : (pk.overall_flag || "").indexOf("🟡") >= 0 ? "rgba(202,138,4,0.12)" : "rgba(22,163,74,0.12)";
      var npsTxt = pk.latest_nps == null ? "NPS未取得" :
        "NPS " + pk.latest_nps + "(生存ライン" + (pk.nps_base_line != null ? pk.nps_base_line : NA) + ") " + (pk.nps_flag || "");
      var ppct = pk.phase_pct != null ? Math.round(pk.phase_pct * 100) : null;
      phaseCell = '<div class="p13-cell" style="grid-column:1/-1; background:' + bg + '; border-radius:6px; padding:8px;">' +
        '<div class="p13-cell-label">契約フェーズKPI — 満了の「生存ライン」に対する活動期間中の進捗</div>' +
        '<div class="p13-cell-value">' +
          '<span style="font-weight:700;">' + esc(pk.overall_flag) + "</span>　" +
          esc(pk.contract_type) + " / " + esc(pk.contract_period) + "ヶ月 / フェーズ:" + esc(pk.phase_bucket) +
          (ppct != null ? " (" + ppct + "%)" : "") +
          '<div style="margin-top:4px; font-weight:400; font-size:12px;">' +
            "NPS: " + esc(npsTxt) + "　｜　接触: " + esc(pk.contact_flag || "-") + "　｜　成果: " + esc(pk.seika_status || "-") +
            (pk.alert_msg ? '<br><b style="color:#fca5a5;">⚠ ' + esc(pk.alert_msg) + "</b>" : "") +
          "</div></div></div>";
    }
    var contactCell = "";
    if (d.rollup) {
      var rb = d.rollup;
      contactCell = '<div class="p13-cell"><div class="p13-cell-label">接触件数 全関連(契約期間内)</div>' +
        '<div class="p13-cell-value">Call ' + rb.call_all + " (" + rb.call_post + ") / Mail " +
        rb.email_all + " (" + rb.email_post + ") / MTG " + rb.mtg_all + " (" + rb.mtg_post + ")</div></div>";
    }
    host.innerHTML = '<div class="p13-deal-card">' +
      phaseCell +
      '<div class="p13-cell"><div class="p13-cell-label">案件</div><div class="p13-cell-value">' +
        esc(d.customer_label) + ' <span style="font-weight:400;">(Deal ' + esc(d.deal_id) + ')</span></div></div>' +
      '<div class="p13-cell"><div class="p13-cell-label">担当コンサル</div><div class="p13-cell-value">' + esc(d.consultant_name) + "</div></div>" +
      '<div class="p13-cell"><div class="p13-cell-label">Pipeline / Stage</div><div class="p13-cell-value">' +
        esc(d.pipeline_label || "-") + " / " + esc(d.stage_label || "-") + "</div></div>" +
      contactCell +
      '<div class="p13-cell"><div class="p13-cell-label">MTG件数(表示/全)</div><div class="p13-cell-value">' + d.meetings_shown + " / " + d.meetings_total + "</div></div>" +
      '<div class="p13-cell"><div class="p13-cell-label">最新MTG</div><div class="p13-cell-value">' +
        esc(d.latest_meeting_start ? d.latest_meeting_start.substring(0, 16) : "-") + "</div></div>" +
      "</div>";
  }

  // 接触×NPSを1つのEChartsインスタンスに統合(上グリッド=週次接触量、下グリッド=定期NPS)。
  // 片方しかデータが無ければそのグリッドだけを使う(空グリッドを表示しない)。
  function renderTrends(el, d) {
    var card = q(el, '[data-cq-table="p13-trend-card"]');
    var chartEl = q(el, '[data-cq-chart="p13-contact-npm"]');
    var hasContact = d.contact_trend && d.contact_trend.length > 0;
    var hasNps = d.nps_trend && d.nps_trend.length > 0;
    if (!card || !chartEl) return;
    if (!hasContact && !hasNps) {
      card.hidden = true;
      return;
    }
    card.hidden = false;
    var host = echartsHost(chartEl);

    if (hasContact && !hasNps) {
      drawContactOnly(host, d.contact_trend);
    } else if (!hasContact && hasNps) {
      drawNpsOnly(host, d.nps_trend);
    } else {
      drawCombined(host, d.contact_trend, d.nps_trend);
    }
  }

  function contactSeries(weeks, contact, gridIndex, xAxisIndex, yAxisIndexEmail, yAxisIndexMtg) {
    var mtgMax = 0;
    contact.forEach(function (p) { if (p.mtg_count > mtgMax) mtgMax = p.mtg_count; });
    return {
      mtgMax: mtgMax,
      series: [
        { name: "Email", type: "bar", stack: "contact", xAxisIndex: xAxisIndex, yAxisIndex: yAxisIndexEmail,
          data: contact.map(function (p) { return p.email_count; }), itemStyle: { color: "#60a5fa" } },
        { name: "Call", type: "bar", stack: "contact", xAxisIndex: xAxisIndex, yAxisIndex: yAxisIndexEmail,
          data: contact.map(function (p) { return p.call_count; }), itemStyle: { color: "#34d399" } },
        { name: "MTG(Zoom実施)", type: "scatter", xAxisIndex: xAxisIndex, yAxisIndex: yAxisIndexMtg, symbol: "diamond", symbolSize: 10,
          itemStyle: { color: "#f59e0b" },
          data: contact.map(function (p) { return p.mtg_count > 0 ? p.mtg_count : null; }) }
      ]
    };
  }

  function drawContactOnly(host, contact) {
    var weeks = contact.map(function (p) { return p.week_start; });
    var c = contactSeries(weeks, contact, 0, 0, 0, 1);
    var chart = initChart(host);
    if (!chart) return;
    chart.setOption({
      backgroundColor: "transparent",
      tooltip: { trigger: "axis" },
      legend: { textStyle: { color: "#cbd5e1" }, top: 0 },
      title: { text: "週次 接触量(Email/Call棒 + Zoom MTG実施◆)", left: "center", top: 0, textStyle: { color: "#cbd5e1", fontSize: 12 } },
      grid: { left: "8%", right: "8%", top: "18%", bottom: "10%" },
      xAxis: [{ type: "category", data: weeks, axisLabel: { color: "#94a3b8", fontSize: 9 } }],
      yAxis: [
        { type: "value", name: "Email/Call件数", nameTextStyle: { color: "#94a3b8" }, axisLabel: { color: "#94a3b8" } },
        { type: "value", name: "MTG実施件数", nameTextStyle: { color: "#94a3b8" }, max: c.mtgMax + 1, minInterval: 1, axisLabel: { color: "#94a3b8" }, splitLine: { show: false } }
      ],
      series: c.series
    });
  }

  function drawNpsOnly(host, nps) {
    var chart = initChart(host);
    if (!chart) return;
    chart.setOption(npsChartOption(nps, 0, 0, "16%"));
  }

  function npsChartOption(nps, gridIndex, xAxisIndex, top) {
    return {
      backgroundColor: "transparent",
      tooltip: { trigger: "axis" },
      title: { text: "定期NPS推移(0-10、定期①→最新、下降=顧客状態悪化)", left: "center", top: 0, textStyle: { color: "#cbd5e1", fontSize: 12 } },
      grid: { left: "8%", right: "8%", top: top, bottom: "14%" },
      xAxis: [{ type: "category", data: nps.map(function (p) { return p.label; }), axisLabel: { color: "#94a3b8", fontSize: 10 } }],
      yAxis: [{ type: "value", min: 0, max: 10, interval: 1, axisLabel: { color: "#94a3b8" } }],
      series: [{ type: "line", data: nps.map(function (p) { return p.nps; }), lineStyle: { color: "#ef4444" }, itemStyle: { color: "#ef4444" }, symbolSize: 7 }]
    };
  }

  function drawCombined(host, contact, nps) {
    var weeks = contact.map(function (p) { return p.week_start; });
    var c = contactSeries(weeks, contact, 0, 0, 0, 1);
    var chart = initChart(host);
    if (!chart) return;
    chart.setOption({
      backgroundColor: "transparent",
      tooltip: { trigger: "axis" },
      legend: { textStyle: { color: "#cbd5e1" }, top: 0 },
      grid: [
        { left: "8%", right: "8%", top: "10%", height: "38%" },
        { left: "8%", right: "8%", top: "60%", height: "30%" }
      ],
      title: [
        { text: "週次 接触量(Email/Call棒 + Zoom MTG実施◆)", left: "center", top: "0%", textStyle: { color: "#cbd5e1", fontSize: 11.5 } },
        { text: "定期NPS推移(0-10、下降=顧客状態悪化)", left: "center", top: "50%", textStyle: { color: "#cbd5e1", fontSize: 11.5 } }
      ],
      xAxis: [
        { type: "category", gridIndex: 0, data: weeks, axisLabel: { color: "#94a3b8", fontSize: 9 } },
        { type: "category", gridIndex: 1, data: nps.map(function (p) { return p.label; }), axisLabel: { color: "#94a3b8", fontSize: 10 } }
      ],
      yAxis: [
        { type: "value", gridIndex: 0, name: "Email/Call", nameTextStyle: { color: "#94a3b8", fontSize: 9 }, axisLabel: { color: "#94a3b8", fontSize: 9 } },
        { type: "value", gridIndex: 0, name: "MTG", nameTextStyle: { color: "#94a3b8", fontSize: 9 }, max: c.mtgMax + 1, minInterval: 1, axisLabel: { color: "#94a3b8", fontSize: 9 }, splitLine: { show: false } },
        { type: "value", gridIndex: 1, min: 0, max: 10, interval: 2, axisLabel: { color: "#94a3b8", fontSize: 9 } }
      ],
      series: c.series.concat([
        { name: "NPS", type: "line", xAxisIndex: 1, yAxisIndex: 2, data: nps.map(function (p) { return p.nps; }),
          lineStyle: { color: "#ef4444" }, itemStyle: { color: "#ef4444" }, symbolSize: 6 }
      ])
    });
  }

  // GAS版 style.html の p13-mtg-source-badge / p13-src-* をそのまま流用(dark調整はCSS側で実施)
  var CONF_LABEL = {
    high: '<span class="p13-mtg-source-badge p13-src-high">紐付け: 高</span>',
    mid: '<span class="p13-mtg-source-badge p13-src-mid">紐付け: 中(会社名/要約なし)</span>',
    low: '<span class="p13-mtg-source-badge p13-src-low">紐付け: 低</span>'
  };

  function renderTimeline(el, d) {
    var host = q(el, '[data-cq-table="p13-mtg-history"]');
    if (!host) return;
    if (!d.meetings.length) {
      host.innerHTML = '<div class="p13-mtg-empty">該当期間に MTG がありません</div>';
      return;
    }
    host.innerHTML = d.meetings.map(renderMeetingCard).join("");
  }

  function renderMeetingCard(m) {
    var iso = m.start_time || "";
    var dateLabel = iso.substring(0, 10);
    var timeLabel = iso.length >= 16 ? iso.substring(11, 16) : "";
    var badge = CONF_LABEL[m.confidence] || "";
    var head = '<div class="p13-mtg-head"><div><span class="p13-mtg-date">' + esc(dateLabel) +
      (timeLabel ? " " + esc(timeLabel) : "") + "</span>" + badge + "</div>" +
      '<div class="p13-mtg-topic">' + esc(m.topic || "(無題)") + "</div>" +
      '<div class="p13-mtg-meta">' +
        (m.duration_min != null ? esc(m.duration_min + " 分") : "") +
        (m.host_email ? " / " + esc(m.host_email) : "") +
        (m.zoom_url ? ' / <a href="' + esc(m.zoom_url) + '" target="_blank" rel="noopener">Zoom URL</a>' : "") +
      "</div></div>";

    var overviewHtml = m.summary_overview ? '<div class="p13-mtg-overview">' + esc(m.summary_overview) + "</div>" : "";

    var detailsHtml = "";
    if (m.summary_details && m.summary_details.length) {
      var items = m.summary_details.map(function (dd) {
        return '<div class="p13-mtg-detail">' +
          (dd.label ? '<div class="p13-mtg-detail-label">' + esc(dd.label) + "</div>" : "") +
          '<div class="p13-mtg-detail-body">' + esc(dd.summary) + "</div></div>";
      }).join("");
      detailsHtml = '<div class="p13-mtg-section"><div class="p13-mtg-section-title">議事内容(セクション別)</div><div class="p13-mtg-details">' + items + "</div></div>";
    }

    var nextHtml = "";
    if (m.next_steps && m.next_steps.length) {
      var lis = m.next_steps.map(function (s) { return "<li>" + esc(s) + "</li>"; }).join("");
      nextHtml = '<div class="p13-mtg-section"><div class="p13-mtg-section-title">ネクストアクション</div><div class="p13-mtg-next"><ul>' + lis + "</ul></div></div>";
    }

    if (!m.has_summary && !overviewHtml && !detailsHtml && !nextHtml) {
      overviewHtml = '<div class="p13-mtg-overview">AI Companion の議事録が生成されていません(古い MTG または AI Companion OFF)。</div>';
    }

    return '<div class="p13-mtg-card' + (m.has_summary ? "" : " p13-no-summary") + '">' + head + overviewHtml + detailsHtml + nextHtml + "</div>";
  }

  // ---- 起動 ------------------------------------------------------------------

  function bindControls(el) {
    if (el._cqControlsBound) return;
    el._cqControlsBound = true;

    var searchInput = q(el, '[data-cq-control="p13-deal-search"]');
    var suggestBox = q(el, '[data-cq-table="p13-deal-suggest"]');
    if (searchInput) {
      searchInput.addEventListener("input", function () { updateSuggest(el); });
      searchInput.addEventListener("focus", function () { updateSuggest(el); });
      document.addEventListener("click", function (e) {
        if (!suggestBox) return;
        if (e.target === searchInput || suggestBox.contains(e.target)) return;
        suggestBox.hidden = true;
      });
    }
    var consultantSel = q(el, '[data-cq-control="p13-consultant-filter"]');
    if (consultantSel) consultantSel.addEventListener("change", function () { populateDealOptions(el, consultantSel.value); });
    var dealSel = q(el, '[data-cq-control="p13-deal-select"]');
    if (dealSel) {
      dealSel.addEventListener("change", function () {
        if (!dealSel.value) return;
        loadDealDetail(el, dealSel.value);
      });
    }
    // 並び順・期間はサーバ側の絞り込み条件なので変更のたびに再fetchする(ファイル冒頭の注記参照)
    ["p13-sort", "p13-period"].forEach(function (key) {
      var sel = q(el, '[data-cq-control="' + key + '"]');
      if (sel) sel.addEventListener("change", function () {
        if (state(el).currentDealId) loadDealDetail(el, state(el).currentDealId);
      });
    });
  }

  function load(el, apiUrl) {
    bindControls(el);
    if (el.dataset.cqBound === "1") return; // 再表示のたびに cq:tab-shown が飛ぶための二重取得防止
    el.dataset.cqBound = "1";
    setStatus(el, "読み込み中...");
    loadDealIndex(el, apiUrl).catch(function (err) {
      el.dataset.cqBound = "";
      setStatus(el, "取得失敗: " + (err && err.message ? err.message : err) + "。時間をおいて再読み込みしてください。");
    });
  }

  document.addEventListener("cq:tab-shown", function (e) {
    var detail = e.detail || {};
    if (detail.tab !== TAB_ID || !detail.el) return;
    load(detail.el, detail.api);
  });

  // 手動再実行用(コンソールデバッグ等)に公開。通常は cq:tab-shown 経由で呼ばれる。
  window.cqTimelineLoad = load;
})();
