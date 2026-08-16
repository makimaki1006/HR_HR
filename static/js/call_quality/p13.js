/*
 * P13 案件タイムライン(Zoom AI Companion 議事録)タブ。
 *
 * データ源:
 *   GET /api/call-quality/timeline/deals
 *     (Rust: get_deal_index, src/handlers/call_quality/tabs/p13_timeline.rs)
 *     Deal一覧 + 担当者フィルタの選択肢。タブを開いたときに1回だけ取得する。
 *   GET /api/call-quality/timeline/deal?deal_id=...&sort=...&period=...
 *     (Rust: get_deal_detail、同ファイル)
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
 * ---- 遅延読込(2026-08-16 チームリード確定) -------------------------------
 * index.js が P13 タブを初めて開いたときにこのファイルを <script> 挿入する想定。
 * DOMContentLoaded は挿入時点で既に発火済みのため待たない。読み込まれた瞬間に
 * 自分で init() まで完了させる(index.js 側からのコールバックは不要な設計)。
 *
 * ---- DOM契約 ---------------------------------------------------------------
 * テンプレート担当が GAS版 index.html の要素IDをそのまま踏襲する前提で実装している。
 * ルート: #page-p13
 * 子要素(GAS版と同じID): p13-consultant-filter(select) / p13-deal-select(select) /
 *   p13-deal-search(input) / p13-deal-suggest(div) / p13-sort(select) / p13-period(select) /
 *   p13-deal-summary(div) / p13-status(div) / p13-trend-card(div) /
 *   p13-contact-chart / p13-contact-empty-msg / p13-nps-chart / p13-nps-empty-msg /
 *   p13-timeline-list(div)
 *
 * ⚠ 既知の注意点: GAS版はチャート要素が <canvas>(Chart.js用)。ECharts は
 *   div コンテナへ init するのが前提で、<canvas> に init すると子要素が
 *   置換要素の内側に隠れて描画されない(空枠になる)。テンプレートが GAS版を
 *   そのまま踏襲して <canvas> のままだった場合に備え、echartsHost() が
 *   canvas を検出したら同IDの div に自動で差し替える防御を入れている。
 */
(function () {
  "use strict";

  var DEALS_ENDPOINT = "/api/call-quality/timeline/deals";
  var DEAL_ENDPOINT = "/api/call-quality/timeline/deal";
  var NA = "—";
  var ROOT_ID = "page-p13";

  function byId(id) { return document.getElementById(id); }
  function num(v) {
    return window.ChartHelpers ? window.ChartHelpers.formatNumber(v) : String(v);
  }
  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  var INSTANCES = [];
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
  var resizeTimer = null;
  window.addEventListener("resize", function () {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () {
      INSTANCES = INSTANCES.filter(function (c) { return !c.isDisposed(); });
      INSTANCES.forEach(function (c) { c.resize(); });
    }, 200);
  });

  // ---- 状態 -------------------------------------------------------------------

  var STATE = { deals: [], byId: {}, currentDealId: "" };

  // ---- Deal 一覧: 取得・担当者プルダウン・検索候補 -----------------------------

  function loadDealIndex() {
    return fetch(DEALS_ENDPOINT, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        var d = payload.data;
        STATE.deals = d.deals;
        STATE.byId = {};
        d.deals.forEach(function (x) { STATE.byId[x.deal_id] = x; });
        var statusEl = byId("p13-status");
        if (statusEl) {
          statusEl.textContent = "Dealを選択してください(全 " + num(d.deals.length) + " Deal、MTG " + num(d.total_meetings) + " 件" +
            (d.skipped_low_confidence > 0 ? " / 低信頼紐付け " + num(d.skipped_low_confidence) + " 件を除外" : "") + ")";
        }
        populateConsultantOptions(d.consultants, d.deals.length);
        populateDealOptions("");
      });
  }

  function populateConsultantOptions(consultants, totalDeals) {
    var sel = byId("p13-consultant-filter");
    if (!sel) return;
    var html = '<option value="">全担当者 (' + num(totalDeals) + " Deal)</option>";
    html += consultants.map(function (c) {
      return '<option value="' + esc(c.name) + '">' + esc(c.name) + " (" + num(c.deal_count) + ")</option>";
    }).join("");
    sel.innerHTML = html;
  }

  function populateDealOptions(consultantName) {
    var sel = byId("p13-deal-select");
    if (!sel) return;
    var list = STATE.deals.slice();
    if (consultantName) list = list.filter(function (d) { return d.consultant_name === consultantName; });
    // 取引先名の辞書順(Rust既定比較。日本語ロケール完全一致ではないが実データはほぼ全角なので実用上の差は小さい)
    list.sort(function (a, b) { return a.customer_label < b.customer_label ? -1 : a.customer_label > b.customer_label ? 1 : 0; });
    var html = '<option value="">― Dealを選択 (' + num(list.length) + '件) ―</option>';
    html += list.map(function (d) {
      return '<option value="' + esc(d.deal_id) + '">' + esc(d.customer_label) + " (" + d.meeting_count + " MTG)</option>";
    }).join("");
    sel.innerHTML = html;
  }

  function updateSuggest() {
    var input = byId("p13-deal-search");
    var box = byId("p13-deal-suggest");
    if (!input || !box) return;
    var q = (input.value || "").trim().toLowerCase();
    var list = STATE.deals.slice().sort(function (a, b) { return b.meeting_count - a.meeting_count; });
    var hits = q ? list.filter(function (d) {
      return d.deal_id.toLowerCase().indexOf(q) !== -1 ||
        (d.customer_label || "").toLowerCase().indexOf(q) !== -1 ||
        (d.consultant_name || "").toLowerCase().indexOf(q) !== -1;
    }) : list;
    hits = hits.slice(0, 50);
    if (!hits.length) {
      box.innerHTML = '<div class="p13-suggest-empty">該当案件なし</div>';
      box.style.display = "block";
      return;
    }
    box.innerHTML = hits.map(function (d) {
      var last = d.latest_start_time ? d.latest_start_time.substring(0, 10) : "-";
      return '<div class="p13-suggest-item" data-deal-id="' + esc(d.deal_id) + '">' +
        "<div>" + esc(d.customer_label) + "</div>" +
        '<div class="p13-suggest-meta">Deal ' + esc(d.deal_id) + " / " + esc(d.consultant_name || "-") + " / " +
        esc(d.stage_label || "-") + " / MTG " + d.meeting_count + "件 / 最新 " + esc(last) + "</div></div>";
    }).join("");
    box.style.display = "block";
    box.querySelectorAll(".p13-suggest-item").forEach(function (item) {
      item.addEventListener("click", function () {
        var did = item.getAttribute("data-deal-id");
        var d = STATE.byId[did];
        if (!d) return;
        input.value = d.customer_label + " (Deal " + did + ")";
        box.style.display = "none";
        loadDealDetail(did);
      });
    });
  }

  // ---- Deal 詳細: 取得・描画 ----------------------------------------------------

  function loadDealDetail(dealId) {
    if (!dealId) return;
    STATE.currentDealId = dealId;
    var statusEl = byId("p13-status");
    if (statusEl) statusEl.textContent = "読み込み中...";

    var sortSel = byId("p13-sort");
    var periodSel = byId("p13-period");
    var sort = sortSel ? sortSel.value : "desc";
    var period = periodSel ? periodSel.value : "all";

    var url = DEAL_ENDPOINT + "?deal_id=" + encodeURIComponent(dealId) +
      "&sort=" + encodeURIComponent(sort) + "&period=" + encodeURIComponent(period);
    fetch(url, { credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error("HTTP " + res.status);
        return res.json();
      })
      .then(function (payload) {
        // 選択が変わった後に古いレスポンスが遅れて返るケースの取り違え防止
        if (STATE.currentDealId !== dealId) return;
        renderDeal(payload.data);
      })
      .catch(function (err) {
        if (statusEl) statusEl.textContent = "取得失敗: " + (err && err.message ? err.message : err);
      });
  }

  function renderDeal(d) {
    renderSummary(d);
    renderTrends(d);
    renderTimeline(d);
    var statusEl = byId("p13-status");
    if (statusEl) statusEl.textContent = "表示: " + d.meetings_shown + " MTG / 全 " + d.meetings_total;
  }

  function renderSummary(d) {
    var el = byId("p13-deal-summary");
    if (!el) return;
    var phaseCell = "";
    if (d.phase_kpi) {
      var pk = d.phase_kpi;
      var bg = (pk.overall_flag || "").indexOf("🚨") >= 0 ? "#fdecea"
        : (pk.overall_flag || "").indexOf("⚠") >= 0 ? "#fef5e7"
        : (pk.overall_flag || "").indexOf("🟡") >= 0 ? "#fef9e7" : "#eafaf1";
      var npsTxt = pk.latest_nps == null ? "NPS未取得" :
        "NPS " + pk.latest_nps + "(生存ライン" + (pk.nps_base_line != null ? pk.nps_base_line : NA) + ") " + (pk.nps_flag || "");
      var ppct = pk.phase_pct != null ? Math.round(pk.phase_pct * 100) : null;
      phaseCell = '<div class="p13-cell" style="grid-column:1/-1; background:' + bg + '; border-radius:6px; padding:8px; color:#1b2330;">' +
        '<div class="p13-cell-label">契約フェーズKPI — 満了の「生存ライン」に対する活動期間中の進捗</div>' +
        '<div class="p13-cell-value">' +
          '<span style="font-weight:700;">' + esc(pk.overall_flag) + "</span>　" +
          esc(pk.contract_type) + " / " + esc(pk.contract_period) + "ヶ月 / フェーズ:" + esc(pk.phase_bucket) +
          (ppct != null ? " (" + ppct + "%)" : "") +
          '<div style="margin-top:4px; font-weight:400; font-size:12px;">' +
            "NPS: " + esc(npsTxt) + "　｜　接触: " + esc(pk.contact_flag || "-") + "　｜　成果: " + esc(pk.seika_status || "-") +
            (pk.alert_msg ? '<br><b style="color:#c0392b;">⚠ ' + esc(pk.alert_msg) + "</b>" : "") +
          "</div></div></div>";
    }
    var contactCell = "";
    if (d.rollup) {
      var rb = d.rollup;
      contactCell = '<div class="p13-cell"><div class="p13-cell-label">接触件数 全関連(契約期間内)</div>' +
        '<div class="p13-cell-value">Call ' + rb.call_all + " (" + rb.call_post + ") / Mail " +
        rb.email_all + " (" + rb.email_post + ") / MTG " + rb.mtg_all + " (" + rb.mtg_post + ")</div></div>";
    }
    el.innerHTML = '<div class="p13-deal-card">' +
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

  function renderTrends(d) {
    var card = byId("p13-trend-card");
    var hasContact = d.contact_trend && d.contact_trend.length > 0;
    var hasNps = d.nps_trend && d.nps_trend.length > 0;
    if (!hasContact && !hasNps) {
      if (card) card.style.display = "none";
      return;
    }
    if (card) card.style.display = "";

    var contactHost = echartsHost("p13-contact-chart");
    var contactEmptyEl = byId("p13-contact-empty-msg");
    if (hasContact) {
      if (contactEmptyEl) contactEmptyEl.style.display = "none";
      if (contactHost) contactHost.style.display = "";
      var weeks = d.contact_trend.map(function (p) { return p.week_start; });
      var mtgMax = 0;
      d.contact_trend.forEach(function (p) { if (p.mtg_count > mtgMax) mtgMax = p.mtg_count; });
      var chart = initChart(contactHost);
      if (chart) {
        chart.setOption({
          backgroundColor: "transparent",
          tooltip: { trigger: "axis" },
          legend: { textStyle: { color: "#cbd5e1" }, top: 0 },
          grid: { left: "8%", right: "8%", top: "18%", bottom: "14%" },
          xAxis: { type: "category", data: weeks, axisLabel: { color: "#94a3b8", fontSize: 9 } },
          yAxis: [
            { type: "value", name: "Email/Call件数", nameTextStyle: { color: "#94a3b8" }, axisLabel: { color: "#94a3b8" } },
            { type: "value", name: "MTG実施件数", nameTextStyle: { color: "#94a3b8" }, max: mtgMax + 1, minInterval: 1,
              axisLabel: { color: "#94a3b8" }, splitLine: { show: false } }
          ],
          series: [
            { name: "Email", type: "bar", stack: "contact", data: d.contact_trend.map(function (p) { return p.email_count; }), itemStyle: { color: "#60a5fa" } },
            { name: "Call", type: "bar", stack: "contact", data: d.contact_trend.map(function (p) { return p.call_count; }), itemStyle: { color: "#34d399" } },
            { name: "MTG(Zoom実施)", type: "scatter", yAxisIndex: 1, symbol: "diamond", symbolSize: 10,
              itemStyle: { color: "#f59e0b" },
              data: d.contact_trend.map(function (p) { return p.mtg_count > 0 ? p.mtg_count : null; }) }
          ]
        });
      }
    } else {
      if (contactHost) contactHost.style.display = "none";
      if (contactEmptyEl) { contactEmptyEl.style.display = ""; contactEmptyEl.textContent = "このDealは週次接触データ(Email/Call/MTG)がありません"; }
    }

    var npsHost = echartsHost("p13-nps-chart");
    var npsEmptyEl = byId("p13-nps-empty-msg");
    if (hasNps && d.nps_trend.length >= 1) {
      if (npsEmptyEl) npsEmptyEl.style.display = "none";
      if (npsHost) npsHost.style.display = "";
      var chart2 = initChart(npsHost);
      if (chart2) {
        chart2.setOption({
          backgroundColor: "transparent",
          tooltip: { trigger: "axis" },
          grid: { left: "8%", right: "8%", top: "12%", bottom: "16%" },
          xAxis: { type: "category", data: d.nps_trend.map(function (p) { return p.label; }), axisLabel: { color: "#94a3b8", fontSize: 10 } },
          yAxis: { type: "value", min: 0, max: 10, interval: 1, name: "NPS(定期①→最新、下降=顧客状態悪化)", nameTextStyle: { color: "#94a3b8", fontSize: 10 }, axisLabel: { color: "#94a3b8" } },
          series: [{ type: "line", data: d.nps_trend.map(function (p) { return p.nps; }), lineStyle: { color: "#ef4444" }, itemStyle: { color: "#ef4444" }, symbolSize: 7 }]
        });
      }
    } else {
      if (npsHost) npsHost.style.display = "none";
      if (npsEmptyEl) {
        npsEmptyEl.style.display = "";
        npsEmptyEl.textContent = d.nps_trend.length === 1
          ? "定期NPS 1点のみ(推移を見るには2点以上必要): " + esc(d.nps_trend[0].label) + " = " + d.nps_trend[0].nps
          : "このDealは定期NPSデータがありません";
      }
    }
  }

  // GAS版 style.html の p13-mtg-source-badge / p13-src-* をそのまま流用(dark調整はCSS側で実施)
  var CONF_LABEL = {
    high: '<span class="p13-mtg-source-badge p13-src-high">紐付け: 高</span>',
    mid: '<span class="p13-mtg-source-badge p13-src-mid">紐付け: 中(会社名/要約なし)</span>',
    low: '<span class="p13-mtg-source-badge p13-src-low">紐付け: 低</span>'
  };

  function renderTimeline(d) {
    var el = byId("p13-timeline-list");
    if (!el) return;
    if (!d.meetings.length) {
      el.innerHTML = '<div class="p13-mtg-empty">該当期間に MTG がありません</div>';
      return;
    }
    el.innerHTML = d.meetings.map(renderMeetingCard).join("");
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

  function bindControls() {
    var searchInput = byId("p13-deal-search");
    var suggestBox = byId("p13-deal-suggest");
    if (searchInput) {
      searchInput.addEventListener("input", updateSuggest);
      searchInput.addEventListener("focus", updateSuggest);
      document.addEventListener("click", function (e) {
        if (!suggestBox) return;
        if (e.target === searchInput || suggestBox.contains(e.target)) return;
        suggestBox.style.display = "none";
      });
    }
    var consultantSel = byId("p13-consultant-filter");
    if (consultantSel) consultantSel.addEventListener("change", function () { populateDealOptions(consultantSel.value); });
    var dealSel = byId("p13-deal-select");
    if (dealSel) {
      dealSel.addEventListener("change", function () {
        if (!dealSel.value) return;
        loadDealDetail(dealSel.value);
      });
    }
    // 並び順・期間はサーバ側の絞り込み条件なので変更のたびに再fetchする(ファイル冒頭の注記参照)
    ["p13-sort", "p13-period"].forEach(function (id) {
      var sel = byId(id);
      if (sel) sel.addEventListener("change", function () {
        if (STATE.currentDealId) loadDealDetail(STATE.currentDealId);
      });
    });
  }

  function init() {
    var root = byId(ROOT_ID);
    if (!root || root.getAttribute("data-cq-bound") === "1") return;
    root.setAttribute("data-cq-bound", "1");

    bindControls();
    loadDealIndex().catch(function (err) {
      var statusEl = byId("p13-status");
      if (statusEl) statusEl.textContent = "取得失敗: " + (err && err.message ? err.message : err) + "。時間をおいて再読み込みしてください。";
    });
  }

  // 手動再初期化用に公開(index.js が使わなくても、この行までの自己実行だけで動く)
  window.cqTimelineInit = init;

  // 遅延読込された時点で DOM は既に存在している前提で、読み込まれ次第すぐ実行する。
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
