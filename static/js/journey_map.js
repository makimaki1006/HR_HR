/*!
 * journey_map.js — 応募者ジャーニーマップ描画部品
 *
 *   window.renderJourneyMap(container, data, options) -> instance
 *
 * モック `claudedocs/mockups/journey_map_mock_2026-08-05.html` の描画ロジックを汎用化した
 * もの。DOM 構造とクラス名／ID はモックと同一で、**このファイルは <style> を注入しない**。
 * スタイルは呼び出し側 HTML が持つ前提なので、下の CSS ブロックをページ側にコピーして使う。
 *
 * -------------------------------------------------------------------------
 * 【呼び出し側に必要な CSS】（モックの <style> と同じ。末尾 3 クラスのみ本部品の追加分）
 * -------------------------------------------------------------------------
 * :root{--navy:#10243e;--blue:#1769aa;--ink:#182536;--mut:#5b6b7f;--line:#d9e2ec;--bg:#eef2f7;
 *       --hi:#c0392b;--mid:#b26a00;--ok:#1a7f57;--card:#fff;--laneA:#f7fafd;--laneB:#fdfdf7}
 * .mapwrap{overflow-x:auto;padding-bottom:6px}
 * .jmap{min-width:940px}
 * .jhead{display:grid;grid-template-columns:170px repeat(8,1fr);gap:0;margin-bottom:4px}
 * .jhead .ph{font-size:11.5px;font-weight:800;color:var(--navy);text-align:center;padding:7px 2px;background:#eaf2fb;border-right:1px solid #fff;position:relative}
 * .jhead .ph:first-child{background:none}
 * .jhead .ph small{display:block;font-weight:600;color:var(--mut);font-size:9.5px}
 * .jhead .ph:nth-child(2){border-radius:9px 0 0 9px}.jhead .ph:last-child{border-radius:0 9px 9px 0;border-right:0}
 * .lane{display:grid;grid-template-columns:170px repeat(8,1fr);align-items:center;border-radius:11px;transition:background .18s}
 * .lane:nth-child(odd){background:var(--laneA)}.lane:nth-child(even){background:var(--laneB)}
 * .lane:hover{background:#eaf3ff}
 * .pname{padding:10px 10px;font-size:12px;font-weight:800;color:var(--navy)}
 * .pname small{display:block;font-weight:600;color:var(--mut);font-size:10px}
 * .pname .beh{display:inline-block;margin-top:3px;font-size:9.5px;font-weight:700;border-radius:999px;padding:1px 7px;background:#e3edf7;color:var(--blue)}
 * .cell{position:relative;height:64px;display:grid;place-items:center}
 * .cell::before{content:"";position:absolute;left:0;right:0;top:50%;height:2px;background:#c9d7e6;z-index:0}
 * .node{position:relative;z-index:1;width:26px;height:26px;border-radius:50%;border:3px solid var(--blue);background:#fff;cursor:pointer;transition:transform .15s, box-shadow .15s}
 * .node.injob{background:var(--blue)}
 * .node.outjob{background:linear-gradient(135deg,var(--blue) 50%,#fff 50%)}
 * .node.pHigh{border-color:var(--hi);box-shadow:0 0 0 4px rgba(192,57,43,.15);animation:pulse 1.8s infinite}
 * .node.pMid{border-color:var(--mid)}
 * .node:hover{transform:scale(1.35)}
 * .node.sel{transform:scale(1.35);box-shadow:0 0 0 5px rgba(23,105,170,.25)}
 * @keyframes pulse{0%,100%{box-shadow:0 0 0 4px rgba(192,57,43,.14)}50%{box-shadow:0 0 0 9px rgba(192,57,43,.05)}}
 * .fact{background:#ffffff18;border:1px solid #ffffff36;border-radius:9px;padding:6px 11px;font-size:12px}
 * .fact b{display:block;font-size:10.5px;color:#bcd3e6;font-weight:700}
 * #tip{position:fixed;z-index:50;max-width:290px;background:var(--navy);color:#fff;border-radius:10px;padding:10px 12px;font-size:11.5px;line-height:1.55;pointer-events:none;opacity:0;transition:opacity .12s;box-shadow:0 8px 22px rgba(10,20,40,.35)}
 * #tip b{color:#9ec7ea;display:block;font-size:10px;margin-bottom:1px}
 * #tip .q{color:#ffd9a0}
 * #panel{border:1px solid var(--line);border-radius:13px;margin-top:12px;overflow:hidden;opacity:0;transform:translateY(6px);transition:opacity .22s, transform .22s}
 * #panel.show{opacity:1;transform:none}
 * #panel .phead{background:linear-gradient(135deg,var(--navy),#1b4264);color:#fff;padding:10px 16px;font-size:13px;font-weight:800;display:flex;justify-content:space-between;align-items:center;gap:8px}
 * #panel .pbody{background:#fff;padding:14px 16px;display:grid;grid-template-columns:1fr 1fr;gap:12px 20px}
 * .fld b{display:block;font-size:10.5px;color:var(--mut);font-weight:800;margin-bottom:1px}
 * .fld{font-size:13px}
 * .fld.wide{grid-column:1/-1}
 * .chips{margin-top:3px}
 * .chip{display:inline-block;background:#eaf4fc;color:var(--blue);border-radius:999px;padding:1px 9px;font-size:10.5px;font-weight:700;margin:2px 4px 0 0}
 * .chip.act{background:#fdeee2;color:#b25f00}
 * .dict-term{border-bottom:2px dotted var(--blue);cursor:help;color:var(--navy);font-weight:700}
 * #dictpop{position:fixed;z-index:60;width:300px;background:#fff;border:1px solid var(--line);border-radius:12px;box-shadow:0 12px 30px rgba(10,20,40,.25);opacity:0;pointer-events:none;transition:opacity .15s;font-size:12px;overflow:hidden}
 * #dictpop .dh{background:#eaf2fb;padding:8px 12px;font-weight:800;color:var(--navy);font-size:12.5px}
 * #dictpop .db{padding:10px 12px}
 * #dictpop .db .row{display:flex;justify-content:space-between;gap:10px;border-bottom:1px dashed var(--line);padding:3px 0;font-size:11.5px}
 * #dictpop .db .row span{color:var(--mut)}
 * #dictpop .note{padding:7px 12px;background:#fff8ec;color:#8a5a00;font-size:10.5px}
 * @media(max-width:760px){#panel .pbody{grid-template-columns:1fr}}
 *
 * ---- 本部品の追加分（モックには無い。mind_voice / 検索需要の表示に使う）----
 * .mindvoice{background:#f4f8fd;border:1px solid #dce8f4;border-left:4px solid var(--blue);border-radius:9px;padding:8px 11px}
 * .mindvoice .mv{font-style:italic;color:var(--navy)}
 * #tip .mv{color:#ffe9c9;font-style:italic;margin-top:3px}
 * .kwvol{font-style:normal;font-weight:700;margin-left:5px;color:var(--mut)}
 *
 * -------------------------------------------------------------------------
 * 【data の形】モックの DATA と同形 + journey[].mind_voice
 * -------------------------------------------------------------------------
 *   {
 *     personas: [{
 *       id, label, profile, behavior,
 *       queries: ["検索語", ...] または [{query:"検索語", stage:"求人認知"}, ...],
 *                （stage 付きは該当段階の詳細に、文字列は「自然検索」の詳細に表示）
 *       journey: [{ stage, candidate_action, question_or_expectation, dropoff_trigger,
 *                   countermeasure, channel, evidence: [...],
 *                   mind_voice: "内心のセリフ（任意・新フィールド）" }, ...],
 *       actions: [{ stage, risk, countermeasure, channel, priority, client_confirmation, evidence }]
 *     }],
 *     facts:  { salary: {value, status}, ... },
 *     cohort: { scope, matched }
 *   }
 *
 * 【options】すべて任意
 *   factsTarget      : facts を描くコンテナ（要素 / セレクタ）。既定は container 内の .facts
 *   cohortTarget     : 比較母集団を書き込む要素。既定は container 内の #cohort / [data-jm-cohort]
 *   keywordMap       : { "検索語": { avg_monthly: 1300 } } 検索需要の実測値。
 *                      「自然検索」ノードの詳細で検索語の横に月間検索量を併記する。
 *   dictEndpoints    : { license: "/api/dict/license_card", occupation: "/api/dict/occupation_card" }
 *   dictTerms        : { "中型免許": "license", ... } 辞書ポップアップ対象語の上書き
 *   stageLabels      : { "求人認知": ["認知","求人を見つける"], ... } 見出しラベルの上書き
 *   autoSelect       : 既定 true。優先度「高」の最初のノードを初期選択する
 *   onSelect         : function(personaIndex, stageIndex, persona, stage) 選択時コールバック
 *   stageMatcher     : function(actionStage, journeyStage) -> bool 打ち手とステージの対応判定
 *
 * 【返り値】{ element, showDetail(pi, si), refresh(), destroy() }
 *
 * 注記: 段階数が 8 以外のとき .jhead / .lane の grid-template-columns を
 *       インラインで上書きする（上記 CSS は 8 段階固定のため）。
 */
(function (global) {
  "use strict";

  // ───────────────── 既定値 ─────────────────

  var DEFAULT_STAGE_LABELS = {
    求人認知: ["認知", "求人を見つける"],
    求人閲覧: ["閲覧", "求人票を読む"],
    自然検索: ["検索", "会社名で検索"],
    他求人比較: ["比較", "他社と比べる"],
    応募判断: ["応募判断", "応募するか決める"],
    応募後連絡: ["応募後", "連絡を待つ"],
    面接: ["面接", "面接を受ける"],
    "オファー・入社判断": ["オファー", "入社を決める"]
  };

  var DEFAULT_DICT_TERMS = {
    中型免許: "license",
    準中型免許: "license",
    大型免許: "license",
    普通免許: "license",
    フォークリフト: "license",
    中型ドライバー: "occupation",
    トラックドライバー: "occupation"
  };

  var DEFAULT_ENDPOINTS = {
    license: "/api/dict/license_card",
    occupation: "/api/dict/occupation_card"
  };

  var DICT_KIND_LABEL = { license: "資格辞書", occupation: "職種辞典" };

  var FACT_LABEL = {
    salary: "給与",
    holidays: "休日",
    working_hours: "勤務時間",
    work_location: "勤務地",
    employment_type: "雇用形態",
    required_qualifications: "必須資格",
    insurance: "保険",
    allowances: "手当"
  };

  var FACT_KEYS = ["salary", "holidays", "working_hours", "required_qualifications"];

  // 辞書カードのレスポンスキャッシュ（found:false も含めて 1 語 1 回だけ引く）
  var dictCache = {};

  // ───────────────── 小物 ─────────────────

  function esc(s) {
    return String(s === null || s === undefined ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  function refLabel(r) {
    // 番号は根拠の識別子なので落とさない（C23とC40は別の実測求人）
    if (/^J\d+$/i.test(r)) return "求人票の記載 " + String(r).toUpperCase();
    if (/^U\d+$/i.test(r)) return "担当者に確認済み " + String(r).toUpperCase();
    if (/^C\d+$/i.test(r)) return "競合求人 " + String(r).toUpperCase();
    if (/^R\d+$/i.test(r)) return "クチコミ " + String(r).toUpperCase();
    if (/^P\d+$/i.test(r)) return "人気求人 " + String(r).toUpperCase();
    if (r === "職種一般仮説") return "職種の一般傾向";
    if (r === "給与比較") return "給与の相対比較";
    return String(r).replace("集計", "の集計");
  }

  function num(v) {
    return typeof v === "number" && isFinite(v);
  }

  /** 内心のセリフは表示側で「」を付けるため、元データに付いていれば外して二重括弧を防ぐ。 */
  function stripQuotes(s) {
    return String(s || "").trim().replace(/^「/, "").replace(/」$/, "");
  }

  function fmtInt(n) {
    return String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  }

  function resolveEl(target, root) {
    if (!target) return null;
    if (typeof target === "string") return (root || document).querySelector(target);
    return target.nodeType === 1 ? target : null;
  }

  /** 打ち手の stage と ジャーニーの stage の対応判定（「〜段階」「A・B」表記を吸収）。 */
  function defaultStageMatcher(actionStage, journeyStage) {
    var norm = function (s) {
      return String(s || "")
        .replace(/段階$/, "")
        .trim();
    };
    var j = norm(journeyStage);
    if (!j) return false;
    var parts = norm(actionStage).split(/[・、,／\/]/);
    for (var k = 0; k < parts.length; k++) {
      var p = norm(parts[k]);
      if (!p) continue;
      if (p === j) return true;
      // 短すぎる語での部分一致は誤対応を生むので 2 文字以上のときだけ許す
      if (p.length >= 2 && j.length >= 2 && (p.indexOf(j) >= 0 || j.indexOf(p) >= 0)) return true;
    }
    return false;
  }

  // ───────────────── 本体 ─────────────────

  function renderJourneyMap(container, data, options) {
    var root = resolveEl(container, document);
    if (!root) throw new Error("renderJourneyMap: container が見つかりません");
    var d = data || {};
    var personas = Array.isArray(d.personas) ? d.personas : [];
    var opt = options || {};

    var stageLabels = opt.stageLabels || DEFAULT_STAGE_LABELS;
    var dictTerms = opt.dictTerms || DEFAULT_DICT_TERMS;
    var endpoints = {
      license: (opt.dictEndpoints && opt.dictEndpoints.license) || DEFAULT_ENDPOINTS.license,
      occupation:
        (opt.dictEndpoints && opt.dictEndpoints.occupation) || DEFAULT_ENDPOINTS.occupation
    };
    var keywordMap = opt.keywordMap || null;
    var stageMatcher = typeof opt.stageMatcher === "function" ? opt.stageMatcher : defaultStageMatcher;
    // 辞書語は長い順にマークして「中型免許」が「中型」に食われないようにする
    var termList = Object.keys(dictTerms).sort(function (a, b) {
      return b.length - a.length;
    });

    // --- 辞書語のマーキング（HTML エスケープ後のテキストに対して行う） ---
    function markTerms(s) {
      var h = esc(s);
      for (var t = 0; t < termList.length; t++) {
        var term = termList[t];
        var kind = dictTerms[term];
        h = h
          .split(term)
          .join(
            '<span class="dict-term" data-term="' +
              esc(term) +
              '" data-kind="' +
              esc(kind) +
              '">' +
              esc(term) +
              "</span>"
          );
      }
      return h;
    }

    // --- 段階（横軸）の決定: 最長ジャーニーの stage 列を採用 ---
    var stages = [];
    personas.forEach(function (p) {
      var j = Array.isArray(p.journey) ? p.journey : [];
      if (j.length > stages.length) {
        stages = j.map(function (x) {
          return x.stage;
        });
      }
    });

    // --- DOM の用意（既存要素があれば再利用、無ければ生成） ---
    var jmap = root.querySelector(".jmap") || root.querySelector("[data-jm-map]");
    if (!jmap) {
      var wrap = document.createElement("div");
      wrap.className = "mapwrap";
      jmap = document.createElement("div");
      jmap.className = "jmap";
      wrap.appendChild(jmap);
      root.appendChild(wrap);
    }

    var panel = root.querySelector("#panel") || document.getElementById("panel");
    if (!panel) {
      panel = document.createElement("div");
      panel.id = "panel";
      panel.innerHTML =
        '<div class="phead"><span id="ptitle"></span><span id="pprio"></span></div>' +
        '<div class="pbody" id="pbody"></div>';
      root.appendChild(panel);
    }
    var ptitle = panel.querySelector("#ptitle") || panel.querySelector(".phead span");
    var pprio = panel.querySelector("#pprio");
    var pbody = panel.querySelector("#pbody") || panel.querySelector(".pbody");

    // ツールチップ / 辞書ポップアップは body 直下の singleton
    var tip = document.getElementById("tip");
    if (!tip) {
      tip = document.createElement("div");
      tip.id = "tip";
      document.body.appendChild(tip);
    }
    var pop = document.getElementById("dictpop");
    if (!pop) {
      pop = document.createElement("div");
      pop.id = "dictpop";
      document.body.appendChild(pop);
    }

    // --- facts / cohort ---
    function renderFacts() {
      // facts はヘッダ側（container の外）に置かれることが多いので document まで探す
      var el =
        resolveEl(opt.factsTarget, document) ||
        root.querySelector(".facts") ||
        document.querySelector("[data-jm-facts]") ||
        document.getElementById("facts");
      if (!el) return;
      var facts = d.facts || {};
      var keys = opt.factKeys || FACT_KEYS;
      var html = "";
      keys.forEach(function (k) {
        var f = facts[k];
        if (!f || f.status !== "verified" || !f.value) return;
        html +=
          '<div class="fact"><b>' +
          esc(FACT_LABEL[k] || k) +
          "（求人票と照合済み）</b>" +
          markTerms(f.value) +
          "</div>";
      });
      el.innerHTML = html;
    }

    function renderCohort() {
      var el =
        resolveEl(opt.cohortTarget, document) ||
        root.querySelector("[data-jm-cohort]") ||
        document.getElementById("cohort");
      if (!el) return;
      var c = d.cohort || {};
      el.textContent = c.scope ? c.scope + " " + (c.matched || 0) + "件" : "—";
    }

    // --- 優先度 ---
    function actionsAt(p, stage) {
      var acts = Array.isArray(p.actions) ? p.actions : [];
      return acts.filter(function (a) {
        return stageMatcher(a.stage, stage);
      });
    }

    function prioAt(p, stage) {
      var acts = actionsAt(p, stage);
      for (var k = 0; k < acts.length; k++) if (acts[k].priority === "高") return "High";
      for (var m = 0; m < acts.length; m++) if (acts[m].priority === "中") return "Mid";
      return "";
    }

    // --- マップ描画 ---
    var gridCols =
      stages.length === 8 || stages.length === 0
        ? null
        : "170px repeat(" + stages.length + ",1fr)";

    function buildMap() {
      var head = '<div class="jhead"><div class="ph"></div>';
      stages.forEach(function (st) {
        var lab = stageLabels[st] || [st, ""];
        head += '<div class="ph">' + esc(lab[0]) + "<small>" + esc(lab[1] || "") + "</small></div>";
      });
      head += "</div>";
      var html = head;

      personas.forEach(function (p, pi) {
        var profile = String(p.profile || "");
        // ペルソナ名にも辞書語マークを掛ける（「中型免許保持の…」等をここから引けるように）
        html +=
          '<div class="lane"><div class="pname">' +
          markTerms(p.label) +
          "<small>" +
          esc(profile.slice(0, 42)) +
          (profile.length > 42 ? "…" : "") +
          '</small><span class="beh">' +
          esc(p.behavior || "") +
          "</span></div>";
        var journey = Array.isArray(p.journey) ? p.journey : [];
        for (var si = 0; si < stages.length; si++) {
          var s = journey[si];
          if (!s) {
            html += '<div class="cell"></div>';
            continue;
          }
          var pr = prioAt(p, s.stage);
          var cls = (s.channel === "求人票" ? "injob" : "outjob") + (pr ? " p" + pr : "");
          html +=
            '<div class="cell"><div class="node ' +
            cls +
            '" data-p="' +
            pi +
            '" data-s="' +
            si +
            '" title=""></div></div>';
        }
        html += "</div>";
      });

      jmap.innerHTML = html;
      if (gridCols) {
        var grids = jmap.querySelectorAll(".jhead, .lane");
        for (var g = 0; g < grids.length; g++) grids[g].style.gridTemplateColumns = gridCols;
      }
    }

    // --- 検索需要（実測値）の併記 ---
    function queryChips(queries) {
      return queries
        .map(function (q) {
          var hit = keywordMap ? keywordMap[q] : null;
          var vol = hit && num(hit.avg_monthly) ? hit.avg_monthly : null;
          return (
            '<span class="chip">' +
            esc(q) +
            (vol !== null ? '<em class="kwvol">月' + fmtInt(vol) + "回</em>" : "") +
            "</span>"
          );
        })
        .join("");
    }

    /** queries は文字列（従来）または {query, stage} を受ける。stage 付きなら該当段階だけに、
     *  文字列なら従来どおり「自然検索」の詳細にだけ表示する。 */
    function queriesFor(p, stage) {
      var items = Array.isArray(p.queries) ? p.queries : [];
      var out = [];
      items.forEach(function (item) {
        var text = typeof item === "string" ? item : item && item.query;
        if (!text) return;
        var qstage = item && typeof item === "object" ? item.stage : null;
        if (qstage ? stageMatcher(qstage, stage) : stage === "自然検索") out.push(text);
      });
      return out;
    }

    function queryBlock(p, stage) {
      var queries = queriesFor(p, stage);
      if (!queries.length) return "";
      var measured = 0;
      if (keywordMap) {
        queries.forEach(function (q) {
          if (keywordMap[q] && num(keywordMap[q].avg_monthly)) measured++;
        });
      }
      var head = keywordMap
        ? "この段階で使いそうな検索語（月間検索量は実測 " + measured + "/" + queries.length + " 語）"
        : "この段階で使いそうな検索語（検索量は未取得・検索仮説）";
      return '<div class="fld wide"><b>' + esc(head) + "</b>" + queryChips(queries) + "</div>";
    }

    // --- 詳細パネル ---
    function showDetail(pi, si) {
      var p = personas[pi];
      if (!p) return;
      var journey = Array.isArray(p.journey) ? p.journey : [];
      var s = journey[si];
      if (!s) return;

      var prev = jmap.querySelectorAll(".node.sel");
      for (var k = 0; k < prev.length; k++) prev[k].classList.remove("sel");
      var node = jmap.querySelector('.node[data-p="' + pi + '"][data-s="' + si + '"]');
      if (node) node.classList.add("sel");

      var acts = actionsAt(p, s.stage);
      if (ptitle) ptitle.textContent = s.stage + " × " + p.label;
      if (pprio) {
        pprio.textContent = acts.length
          ? "優先度 " +
            acts
              .map(function (a) {
                return a.priority;
              })
              .join("・")
          : "";
      }

      var mind = s.mind_voice
        ? '<div class="fld wide mindvoice"><b>この人の内心</b><span class="mv">「' +
          markTerms(stripQuotes(s.mind_voice)) +
          "」</span></div>"
        : "";

      var extra = queryBlock(p, s.stage);

      var actHtml = acts
        .map(function (a) {
          return (
            '<div class="fld wide" style="background:#fdf6ee;border:1px solid #eedcc2;border-radius:9px;padding:9px 11px">' +
            "<b>優先対策（" +
            esc(a.priority) +
            "） — 実施場所: " +
            esc(a.channel) +
            "</b>" +
            markTerms(a.countermeasure) +
            '<div class="chips">' +
            (Array.isArray(a.evidence) ? a.evidence : [])
              .map(function (r) {
                return '<span class="chip act">' + esc(refLabel(r)) + "</span>";
              })
              .join("") +
            "</div>" +
            (a.client_confirmation
              ? '<div style="font-size:11px;color:var(--mut);margin-top:3px">貴社への確認: ' +
                esc(a.client_confirmation) +
                "</div>"
              : "") +
            "</div>"
          );
        })
        .join("");

      if (pbody) {
        pbody.innerHTML =
          mind +
          '<div class="fld"><b>この段階ですること</b>' +
          markTerms(s.candidate_action) +
          "</div>" +
          '<div class="fld"><b>不安・疑問</b>' +
          markTerms(s.question_or_expectation) +
          "</div>" +
          '<div class="fld"><b>離脱の引き金</b>' +
          markTerms(s.dropoff_trigger) +
          "</div>" +
          '<div class="fld"><b>打ち手 — 実施場所: ' +
          esc(s.channel) +
          "</b>" +
          markTerms(s.countermeasure) +
          '<div class="chips">' +
          (Array.isArray(s.evidence) ? s.evidence : [])
            .map(function (r) {
              return '<span class="chip">' + esc(refLabel(r)) + "</span>";
            })
            .join("") +
          "</div></div>" +
          extra +
          actHtml;
      }
      panel.classList.add("show");
      if (typeof opt.onSelect === "function") opt.onSelect(pi, si, p, s);
    }

    // --- イベント: ツールチップ ---
    function onMove(e) {
      var n = e.target.closest ? e.target.closest(".node") : null;
      if (!n) {
        tip.style.opacity = 0;
        return;
      }
      var p = personas[+n.dataset.p];
      if (!p) return;
      var s = (p.journey || [])[+n.dataset.s];
      if (!s) return;
      tip.innerHTML =
        "<b>" +
        esc(s.stage) +
        " × " +
        esc(p.label) +
        "</b>" +
        esc(s.candidate_action) +
        (s.mind_voice ? '<div class="mv">「' + esc(stripQuotes(s.mind_voice)) + "」</div>" : "") +
        '<div class="q">不安・疑問: ' +
        esc(s.question_or_expectation) +
        "</div>";
      tip.style.left = Math.min(e.clientX + 14, window.innerWidth - 310) + "px";
      tip.style.top = e.clientY + 16 + "px";
      tip.style.opacity = 1;
    }

    function onLeave() {
      tip.style.opacity = 0;
    }

    function onClick(e) {
      var n = e.target.closest ? e.target.closest(".node") : null;
      if (!n) return;
      showDetail(+n.dataset.p, +n.dataset.s);
      if (panel.scrollIntoView) panel.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }

    // --- イベント: 辞書ポップアップ ---
    function popupHtml(term, kind, card) {
      var label = DICT_KIND_LABEL[kind] || "辞書";
      var head =
        '<div class="dh">' +
        esc((card && card.found && card.name) || term) +
        ' <span style="font-weight:600;font-size:10px;color:#5b6b7f">｜ ' +
        esc(label) +
        "</span></div>";

      if (!card) {
        return head + '<div class="db">読み込み中…</div>';
      }
      if (!card.found) {
        var why =
          card.reason === "not_configured"
            ? "辞書データベース未接続"
            : card.reason === "query_failed"
              ? "辞書の取得に失敗"
              : "辞書に該当なし";
        return head + '<div class="note">' + esc(why) + "</div>";
      }

      var rows = "";
      function row(k, v) {
        if (v === null || v === undefined || v === "") return;
        rows += '<div class="row"><span>' + esc(k) + "</span><b>" + esc(v) + "</b></div>";
      }

      if (kind === "license") {
        row("関連する職業", num(card.occupation_count) ? card.occupation_count + "職種" : "");
        row(
          "年収の中央値",
          num(card.median_salary_man_yen)
            ? card.median_salary_man_yen + "万円（対象 " + (card.wage_target_n || 0) + "職種）"
            : ""
        );
        row("平均年齢", num(card.avg_age) ? card.avg_age + "歳" : "");
        var co = (card.co_occurring_licenses || [])
          .map(function (x) {
            return x.name;
          })
          .join("、");
        row("よく一緒に持たれる資格", co);
      } else {
        row("分野", card.category_label || card.category);
        row("仕事の内容", card.summary);
        row("平均年収", num(card.annual_salary_man_yen) ? card.annual_salary_man_yen + "万円" : "");
        row("平均年齢", num(card.avg_age) ? card.avg_age + "歳" : "");
      }
      if (!rows) rows = '<div class="row"><span>統計値の登録なし</span></div>';

      var note = card.source
        ? '<div class="note">出典: ' +
          esc(card.source) +
          (card.name && card.query && card.name !== card.query
            ? "／照合: 「" + esc(card.query) + "」→「" + esc(card.name) + "」"
            : "") +
          "</div>"
        : "";
      return head + '<div class="db">' + rows + "</div>" + note;
    }

    function placePop(el) {
      var r = el.getBoundingClientRect();
      pop.style.left = Math.min(r.left, window.innerWidth - 315) + "px";
      pop.style.top = r.bottom + 8 + "px";
      pop.style.opacity = 1;
    }

    function fetchCard(term, kind) {
      var key = kind + "|" + term;
      if (dictCache[key]) return dictCache[key];
      var url = endpoints[kind];
      if (!url) {
        dictCache[key] = Promise.resolve({ found: false, reason: "not_found", query: term });
        return dictCache[key];
      }
      dictCache[key] = fetch(url + "?name=" + encodeURIComponent(term), {
        credentials: "same-origin",
        headers: { Accept: "application/json" }
      })
        .then(function (res) {
          if (!res.ok) throw new Error("HTTP " + res.status);
          return res.json();
        })
        .catch(function () {
          // fetch 失敗も「辞書に該当なし」扱い（画面は壊さない）
          return { found: false, reason: "fetch_failed", query: term };
        });
      return dictCache[key];
    }

    function onDictOver(e) {
      var t = e.target.closest ? e.target.closest(".dict-term") : null;
      if (!t) {
        pop.style.opacity = 0;
        pop.removeAttribute("data-tab-url");
        return;
      }
      var term = t.dataset.term;
      var kind = t.dataset.kind || dictTerms[term];
      if (!term || !kind) return;

      pop.innerHTML = popupHtml(term, kind, null);
      placePop(t);
      fetchCard(term, kind).then(function (card) {
        // ホバー先が変わっていたら描き替えない
        if (pop.style.opacity === "0") return;
        pop.innerHTML = popupHtml(term, kind, card);
        pop.setAttribute("data-tab-url", (card && card.tab_url) || "");
        t.setAttribute("data-tab-url", (card && card.tab_url) || "");
      });
    }

    function onDictClick(e) {
      var t = e.target.closest ? e.target.closest(".dict-term") : null;
      if (!t) return;
      var url = t.getAttribute("data-tab-url");
      // 同一オリジンの絶対パスのみ許可（javascript: 等への遷移を防ぐ）
      if (url && url.charAt(0) === "/" && url.charAt(1) !== "/") {
        e.preventDefault();
        window.location.href = url;
      }
    }

    // --- 組み立て & バインド ---
    function refresh() {
      renderFacts();
      renderCohort();
      buildMap();
      if (opt.autoSelect !== false) {
        var done = false;
        for (var pi = 0; pi < personas.length && !done; pi++) {
          var p = personas[pi];
          var journey = Array.isArray(p.journey) ? p.journey : [];
          for (var si = 0; si < journey.length; si++) {
            if (prioAt(p, journey[si].stage) === "High") {
              showDetail(pi, si);
              done = true;
              break;
            }
          }
        }
        if (!done && personas.length) showDetail(0, 0);
      }
    }

    jmap.addEventListener("mousemove", onMove);
    jmap.addEventListener("mouseleave", onLeave);
    jmap.addEventListener("click", onClick);
    document.addEventListener("mouseover", onDictOver);
    document.addEventListener("click", onDictClick);

    refresh();

    return {
      element: jmap,
      showDetail: showDetail,
      refresh: refresh,
      destroy: function () {
        jmap.removeEventListener("mousemove", onMove);
        jmap.removeEventListener("mouseleave", onLeave);
        jmap.removeEventListener("click", onClick);
        document.removeEventListener("mouseover", onDictOver);
        document.removeEventListener("click", onDictClick);
        jmap.innerHTML = "";
        panel.classList.remove("show");
        tip.style.opacity = 0;
        pop.style.opacity = 0;
      }
    };
  }

  global.renderJourneyMap = renderJourneyMap;
})(typeof window !== "undefined" ? window : this);
