// コンサルダッシュボードの「見せ方」の決まりごとを、**画面の JS を実際に動かして**確かめる。
//
// ------------------------------------------------------------------
// なぜ要るか
// ------------------------------------------------------------------
// `consulting_page_js.js` は構文しか見ない。`app_routes_no_conflict.rs` は
// テンプレートの文字列が含まれているかしか見ない。どちらも
// 「KPI が母数の小さい人を最下位として名指しする」（V2）のような
// **描いた結果の誤り**には掛からない。ここではテンプレートの <script> を
// 最小の偽 DOM の上で読み込み、描画関数に入力を渡して、出てきた HTML を見る。
//
// 入力は 2026-09-23 に fixture（tests/fixtures/cs_dashboard/*.tsv.gz、基準日 2026-09-18）から
// 作った応答で実測した値を、必要な列だけ抜き出して書いている（数字の出どころはテストごとに書く）。
//
// ------------------------------------------------------------------
// 使い方
// ------------------------------------------------------------------
//     node tests/consulting_view_rules.js
//
// サーバもブラウザも要らない。落ちたら終了コード 1。
"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");

/* 逆証明用: CS_PAGE_TEMPLATE に別のファイル（直す前の版など）を渡すと、そちらを検査する（consulting_page_js.js と同じ）。
   直す前の版で新しい見張りが落ちなければ、その見張りは何も守っていない */
const html = fs.readFileSync(
  process.env.CS_PAGE_TEMPLATE ? path.resolve(process.env.CS_PAGE_TEMPLATE)
    : path.join(__dirname, "..", "templates/tabs/cs_dashboard.html"), "utf-8");
const js = [...html.replace(/\{\{[^}]*\}\}/g, '"__askama__"')
  .matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join("\n");

/* ---- 最小の偽 DOM。読み込み時に go() → load() が走るので、落ちない程度に受け流す ---- */
function fakeEl() {
  return {
    innerHTML: "", className: "", style: {}, value: "", checked: false, dataset: {},
    querySelectorAll: () => [], querySelector: () => null, setAttribute() {},
    addEventListener() {}, focus() {}, setSelectionRange() {},
  };
}
const els = {};
const document = {
  getElementById: (id) => els[id] || (els[id] = fakeEl()),
  querySelector: () => null,
  querySelectorAll: () => [],
  createElement: () => fakeEl(),
};
/* 読み込み時に window に付けたリスナーを残しておく（影の付け直しがイベントにつながっているかを見る） */
const winListeners = [];
const ctx = vm.createContext({
  document,
  location: { hash: "", href: "http://localhost/consulting" },
  history: { replaceState() {} },
  window: { addEventListener(type, fn, capture) { winListeners.push({ type, fn, capture }); }, scrollTo() {} },
  fetch: () => new Promise(() => {}),   // 取りに行かない（返事が来ないまま）
  setTimeout: () => 0, clearTimeout: () => {},
  URLSearchParams, console,
});
vm.runInContext(js, ctx, { filename: "cs_dashboard.html#script" });
const run = (code) => vm.runInContext(code, ctx);

let failed = 0, passed = 0;
const pendingChecks = [];   /* Promise を返す見張り（fetch の後を見るもの）。最後にまとめて待つ */
function check(name, fn) {
  const good = () => { passed++; console.log("OK   " + name); };
  const bad = (e) => { failed++; console.error("FAIL " + name + "\n     " + e.message); };
  try {
    const r = fn();
    if (r && typeof r.then === "function") pendingChecks.push(r.then(good, bad));
    else good();
  } catch (e) { bad(e); }
}
function ok(cond, msg) { if (!cond) throw new Error(msg); }

/* ================================================================ V2 */
// fixture の担当者 27 名のうち、接触率が出ている人を抜き出したもの（2026-09-23 実測）。
// h6e0d… は 0.0%（0/11か月）で small_n。以前の KPI はこの人を「いちばん低い」として赤で出していた。
// 母数が足りる人の最下位は h9821… の 26.4%（33/125か月）。small_n は 5 名。
const TEAM_ROWS = [
  { consultant: "h6e0d76778594", contact_rate: 0, contact_touched: 0, contact_months: 11, small_n: true, n_active: 10, focus: 0 },
  { consultant: "h14989084e280", contact_rate: 57.14, contact_touched: 4, contact_months: 7, small_n: true, n_active: 1, focus: 0 },
  { consultant: "h4d5a88534c6a", contact_rate: 50, contact_touched: 8, contact_months: 16, small_n: true, n_active: 9, focus: 0 },
  { consultant: "habc7b05f19ea", contact_rate: 100, contact_touched: 1, contact_months: 1, small_n: true, n_active: 1, focus: 0 },
  { consultant: "hb9aeb29c3e60", contact_rate: 100, contact_touched: 6, contact_months: 6, small_n: true, n_active: 4, focus: 0 },
  { consultant: "h9821a39368fe", contact_rate: 26.4, contact_touched: 33, contact_months: 125, small_n: false, n_active: 28, focus: 3 },
  { consultant: "h2e505aa278de", contact_rate: 41.11, contact_touched: 37, contact_months: 90, small_n: false, n_active: 25, focus: 16 },
  { consultant: "h60b499e1c307", contact_rate: 96.92, contact_touched: 63, contact_months: 65, small_n: false, n_active: 22, focus: 2 },
];
ctx.__TEAM = TEAM_ROWS;

// 2026-09-28（S-10）: KPI は「いちばん低い」人の名指しをやめて「40% 未満の人数」にした。母数が小さい人を外す性質（V2）は同じ
check("V2: 接触率 40% 未満の人数から母数が小さい人を外す", () => {
  const p = run("pickLowContact(__TEAM)");
  ok(p.n === 1 && p.deals === 28,
    "40% 未満が " + p.n + " 名・持ち案件 " + p.deals + " 件（期待 1 名・28 件）。母数が小さい h6e0d…（0/11か月）を数えていないか");
  ok(p.skipped === 5, "外した人数が " + p.skipped + "（期待 5）");
});

/* 2026-09-29 組み替え（09 の 6）: 担当者の一覧は「チームと担当」の中の表（担当者 × 状態）になった。応答は routes.rs build_team の束
   （consultants / status / contact / handover）。見張りは前の担当者の一覧の応答（__D など）を束の consultants に入れて描く（teamOf）。
   status は consultants の行から作る（持ち件数と退職者だけ。ほかの数は 0） */
function teamOf(C, o) {
  return Object.assign({
    meta: C.meta, consultants: C,
    status: { rows: (C.rows || []).map((r) => ({ consultant: r.consultant, n_active: r.n_active || 0, n_flags2: 0, mtg_critical: 0,
      no_contact: 0, expiring60: 0, expiring_week: 0, nps_low: 0, retired: !!r.retired })),
      meta: { n_active: C.meta.n_active, n_flags2: 0, expiring_week: 0, min_flags: 2, week_days: 7, unknown: {},
              flag_labels: { no_contact: "接触の記録が無い", expiring60: "満了まで60日以内", nps_low: "NPSが4以下" } } },
    contact: { meta: {}, month: { periods: [], rows: [], team: [] }, week: { periods: [], rows: [], team: [] } },
    handover: { meta: { n: 0, n_active: 0 }, rows: [], reflected_dist: [], to_retired: 0, median_gap_days: null, n_gap: 0,
                source_rule: "", gap_rule: "" },
  }, o || {});
}
ctx.teamOf = teamOf;
/* 見える文字（textOf はこの後ろで定義するので、ここより前の見張りはこちらを使う） */
const plainOf = (h) => String(h).replace(/<svg[\s\S]*?<\/svg>/g, " ").replace(/<[^>]*>/g, "").replace(/&#9660;/g, "▼");

/* 2026-09-29 組み替え（09 の 6）: 札は 4 枚（担当者・名札2本以上・今週満了・退職者のまま）で、どれも件数（名指ししない）。
   前の札「接触率 40% 未満の担当者」は、2 つの定義を並べた説明（表の直下）で人数として言う。母数が小さい人を外す性質（V2）は同じ */
check("V2/S-10: 40% 未満の人数から母数が小さい人を外して書き、札に担当者の名前を一人も出さない（名指しの順位表にしない）", () => {
  ctx.__D = {
    rows: TEAM_ROWS, meta: { n_consultant: 27, n_active: 604, unknown_owner: 0, retired_deals: 0,
      retired_people: 0, owner_ties: 38, not_counted: "※ 担当者の評価ではありません" },
    contact_rule: "", small_n_rule: "", focus_rule: "", owner_rule: "担当は consultant が正本です",
  };
  const h = run("renderTeam(teamOf(__D))");
  const kpi = h.slice(h.indexOf('<div class="kpis">'), h.indexOf("<h2", h.indexOf('<div class="kpis">')));
  for (const r of TEAM_ROWS) ok(!kpi.includes(r.consultant), "KPI に担当者の名前が出ている: " + r.consultant);
  const lbls = [...kpi.matchAll(/<span class="lbl">([^<]*)<\/span>/g)].map((m) => m[1]);
  ok(JSON.stringify(lbls) === JSON.stringify(["担当者", "名札が 2 本以上の案件", "今週満了", "退職者のまま"]), "札が 09 の 6 の 4 枚でない: " + lbls.join(" / "));
  const def = plainOf(h.slice(h.indexOf("人ごとの接触は、定義が2つあります")));
  ok(def.includes("いま 40% 未満の担当者は 1 名です（持ち案件 28 件）"), "40% 未満の人数（1 名・持ち案件 28 件）が書かれていない");
  ok(def.includes("母数が小さい 5 名は数えていません"), "外した人数が書かれていない");
  /* 40% 未満の人（母数が足りる h9821…）の表の値は、色だけでなく ▼ でも分かる。母数が小さい人（h6e0d… 0.0%）には「母数が小さい」の印 */
  const tb = h.slice(h.indexOf('<table id="team-tbl"'));
  const rowOf = (name) => tb.slice(tb.indexOf(">" + name + "<"), tb.indexOf("</tr>", tb.indexOf(">" + name + "<")));
  ok(rowOf("h9821a39368fe").includes("&#9660; 26.4%"), "40% 未満の値に ▼ が無い（色だけで伝えている）");
  ok(rowOf("h6e0d76778594").includes(">母数が小さい</span>"), "母数が小さい人に印が無い");
  /* 注力案件は札にせず、表の列（件数）で出す */
  ok(/<td class="n">16<\/td>/.test(rowOf("h2e505aa278de")), "注力案件の件数（16 件）が表に無い");
  // 0 名なら持ち案件を書かない
  ctx.__D0 = Object.assign({}, ctx.__D, { rows: TEAM_ROWS.filter((r) => r.small_n || r.contact_rate >= 40) });
  const d0 = plainOf(run("renderTeam(teamOf(__D0))"));
  ok(d0.includes("いま 40% 未満の担当者は 0 名です。") && !/0 名です（持ち案件/.test(d0), "0 名のときの書き方が違う");
});

/* ================================================================ V3 */
check("V3: 内部名を表示名に置き換える（対応表は1か所）", () => {
  ok(run('dispName("oubo")') === "応募数", "oubo が置き換わらない");
  ok(run('dispName("keisaisu")') !== "keisaisu", "keisaisu が置き換わらない");
  ok(run('dispName("deal")') !== "deal" && run('dispName("company")') !== "company",
    "法人番号の出どころ deal / company が置き換わらない");
  ok(run('dispName("（無し）")') === "（無し）", "知らない語を書き換えている");
  // consultants.json の owner_rule そのもの（2026-09-23 実測）
  const t = run('dispText("担当は consultant が正本です（hubspot_owner_id ではありません）")');
  ok(!/consultant|hubspot_owner_id/.test(t), "文の中の内部名が残っている: " + t);
  ok(run('dispText("consultants")') === "consultants", "英単語の一部まで置き換えている");
});

check("V3: データ品質の図に内部名が出ない", () => {
  ctx.__DQ = {
    meta: { n_deals: 3432, n_active: 604 },
    // data-quality.json の houjin_source（2026-09-23 実測: company 874 / deal 2,499 / 無し 59）
    houjin_source: { rows: [{ label: "company", n: 874 }, { label: "deal", n: 2499 }, { label: "（無し）", n: 59 }], note: "" },
    outcome_bias: { note: "", rows: ["oubo", "mensetu", "syoudaku", "saiyomokuhyou", "keisaisu"]
      .map((f) => ({ group: "継続済", field: f, n: 1404, filled: 863, fill_rate: 61.5 })) },
    missing: [], sheets: [],
  };
  const h = run("renderDq(__DQ)");
  for (const k of ["oubo", "mensetu", "syoudaku", "saiyomokuhyou", "keisaisu"])
    ok(!new RegExp(">" + k + "<").test(h), "図のラベルに " + k + " が出ている");
  ok(!/>deal |>company /.test(h) && !/ deal <| company </.test(h), "法人番号の出どころに deal / company が出ている");
});

/* ================================================================ V9 */
check("V9: 目標の帯は 赤=まずい / 緑=良い を守る", () => {
  // outcome.json の goal_act.bands の並び（2026-09-23 実測）。以前は並び順で SERIES を回し、
  // 50〜100% が赤、1〜50% が緑になっていた。
  // 🔴 期待値を変えた（2026-09-23 検証 V9）: 以前は「0% は赤」を固定していたが、母集団は
  // 稼働中の契約で達成率は伸びる途中の値（右側打ち切り）。0% を赤（■まずい）・1〜50% を
  // 山吹（▲注意）で塗ると途中の値を確定した悪い結果として読ませるので、判定の色を使わない
  const c = (l) => run("goalBandColor(" + JSON.stringify(l) + ")");
  ok(c("100%以上") === "var(--midori)", "100%以上 が緑でない");
  for (const l of ["0%（実績ゼロ）", "1〜50%", "50〜100%"]) {
    ok(!["var(--hi)", "var(--ki)", "var(--midori)"].includes(c(l)), l + " に判定の色 " + c(l) + " を使っている");
    ok(run("goalBandOpen(" + JSON.stringify(l) + ")") === true, l + " が未確定（中空）になっていない");
  }
  ok(!run('goalBandOpen("100%以上")'), "100%以上（確定）を中空にしている");
  ok(c("未記入（目標が無い）") === "var(--ghost)", "未記入が灰でない（値が無い）");
  ok(c("承諾数が空（目標はある）") === "var(--ghost)", "承諾数が空が灰でない（値が無い）");
});

/* renderOutcome に渡す最小の応答。帯の件数は fixture の goal_act（2026-09-23 実測:
   pop 604 / has_goal 351 / both 345 / 0% 198 / 承諾数が空 6 / 未記入 253）。
   100%以上・50〜100%・1〜50% は合計が 345 - 198 = 147 になるように置いた値
   （色と形を見るだけで、件数は検査しない）。
   efficiency の n は fixture の 継続 1282 / 解約 529 / 充足 142（検証の指摘より） */
const obox = (n) => ({ n, min: 0, q1: 1, median: 2, q3: 3, max: 9, mean: 2.5 });
ctx.__OUT = {
  meta: { not_counted: "※ 成約率ではありません", today: "2026-09-18" },
  goal_act: { pop: 604, has_goal: 351, both: 345, median: 0, fill_rate: 58.1, bands: [
    { label: "100%以上", n: 60 }, { label: "50〜100%", n: 40 }, { label: "1〜50%", n: 47 },
    { label: "0%（実績ゼロ）", n: 198 }, { label: "承諾数が空（目標はある）", n: 6 },
    { label: "未記入（目標が無い）", n: 253 }] },
  goal_all: { fill_rate: 33.4 },
  efficiency: { caveat: "", has_keisaisu_act: 500, n_act: 604, groups: [
    { label: "継続した", box: obox(1282) }, { label: "解約した", box: obox(529) },
    { label: "充足", box: obox(142) }] },
  risk: { n_act: 604, bands: [], ax3: { rule: "" }, ax4: { rule: "" }, top: [], order_note: "" },
  contact_source: {},
};

check("V9: 目標の帯を renderOutcome が実際に中空・判定の色なしで描く", () => {
  const h = run("renderOutcome(__OUT)");
  const g = h.split("<figcaption>達成率 ＝ 承諾数 ÷ 採用目標数")[1].split("</figure>")[0];
  const rects = [...g.matchAll(/<rect [^>]*>/g)].map((m) => m[0]);
  ok(rects.length >= 6, "帯の数が足りない: " + rects.length);
  ok(!g.includes("var(--hi)") && !g.includes("var(--ki)"), "目標の帯に赤・山吹を使っている");
  const dashed = rects.filter((r) => r.includes("stroke-dasharray")).length;
  // 帯 3 つ ＋ 凡例の粒 3 つ
  ok(dashed === 6, "未確定の帯（0% / 1〜50% / 50〜100%）が中空・破線になっていない: " + dashed);
  ok(g.includes("途中の値"), "稼働中なので途中の値だという断り書きが無い");
  ok(g.includes("承諾数が空の 6 件"), "目標はあるが承諾数が空の件数を書いていない（NEW）");
});

check("V9: 応募効率の箱ひげで群の並び順に色を回さない（解約=赤・充足=緑にしない）", () => {
  const h = run("renderOutcome(__OUT)");
  const g = h.split("<figcaption>応募数 ÷ 掲載数")[1].split("</figure>")[0];
  ok(!g.includes("var(--hi)") && !g.includes("var(--midori)"), "箱ひげに赤または緑が入っている");
});

check("V9: 継続回数ごとの箱ひげで行ごとに色を回さない（継続1だけ赤にしない）", () => {
  const box = { n: 40, min: 0, q1: 1, median: 2, q3: 3, max: 9, mean: 2.5 };
  const row = (no) => ({ renewal_no: no, n: 100, n_active: 1, cancel_rate: 30, cancel_rate_excl_fill: 20,
    oubo: box, mensetu: box, syoudaku: box, oubo_per_posting: box, amount: box });
  ctx.__RN = { meta: { exclude_right_censored: false, right_censored_n: 0 },
    monthly_retention: { rows: [] }, by_renewal: [row(0), row(1), row(2)], missingness: [], population: {} };
  const h = run("renderRenewal(__RN)");
  const boxes = h.split("の散らばり（継続回数ごと）").slice(1).join("");
  ok(!boxes.includes("stroke:var(--hi)"), "箱ひげのどれかが赤で描かれている");
});

/* ================================================================ V10 */
check("V10: 整数の軸で目盛りが重複しない（2,2,1,1,0 にならない）", () => {
  const tk = run("ticks(0, 2, 4, minStepOf(F.int))").map((v) => run("F.int(" + v + ")"));
  ok(new Set(tk).size === tk.length, "目盛りが重複: " + tk.join(","));
});

check("V10: 折れ線の最大値が最上段の目盛りを超えない", () => {
  // (伏字)2007 の応募 19 に対して、目盛りが 15 までしか無かった（2026-09-23 目視）
  const svg = run('svgLine({ x: ["1","2"], series: [{ pts: [{ v: 19 }, { v: 3 }] }], yFmt: F.int })');
  const ticksTxt = [...svg.matchAll(/text-anchor="end">([0-9,]+)<\/text>/g)].map((m) => +m[1].replace(/,/g, ""));
  ok(Math.max(...ticksTxt) >= 19, "最上段の目盛りが " + Math.max(...ticksTxt) + "（最大値 19 を覆っていない）");
});

/* ================================================================ V12 */
check("V12: 表の枠の上に行数と列数・スクロールの案内を出す", () => {
  const h = run('scroll(table([{ t: "a" }, { t: "b" }, { t: "c" }], [[1,2,3],[4,5,6]]), 400)');
  ok(h.includes("scroll-cap"), "案内が無い");
  ok(h.includes("全 <b>2</b> 行 × 3 列"), "行数・列数が合っていない: " + h.slice(0, 160));
});

check("V12: 途中で切った表は「全 N 行」と言わず、元の件数を書く", () => {
  const t = 'table([{ t: "a" }], [[1],[2],[3]])';
  const cut = run("scroll(" + t + ", 400, 250)");
  ok(!cut.includes("全 <b>3</b> 行"), "3 行で切った表に「全 3 行」と出している");
  ok(cut.includes("<b>3</b> 行を出しています（全 250 件のうち）"), "元の件数が無い: " + cut.slice(0, 160));
  const all = run("scroll(" + t + ", 400, 3)");
  ok(all.includes("全 <b>3</b> 行"), "切っていない表の書き方が変わった");
});

check("V12: NPS が低い顧客の表は 200 件で切ったとき元の件数を枠の上に出す", () => {
  // 件数は 200 件を超える場合を作るために置いた値（fixture の実数は 200 件以下の可能性がある）
  ctx.__FO = { meta: { today: "2026-09-18" },
    nps_low: { threshold: 4, n: 230, n_have_nps: 300, n_act: 604, coverage: 49.7, dist: [], note: "",
      rows: Array.from({ length: 230 }, (_, i) => ({ deal_id: "d" + i, stage: "", nps: 1, nps_month: "2026-09",
        amount: 1, days_to_expiry: 10, n_contact: 1 })) },
    cpa: { worse: 0, judged: 0, skipped_censored: 0, rows: [], note: "" },
    mtg_layers: { neither: 0, n_act: 604, both: 0, only_recording: 0, only_mail: 0, note: "",
      fact_recording: { n: 0, rate: 0 }, estimated_mail: { n: 0, rate: 0 } },
    shape: { ltv: null, display_label: "", n_all: 0, n_display: 0, multi_site: 0, multi_site_note: "" } };
  const h = run("renderFocus(__FO)");
  ok(h.includes("<b>200</b> 行を出しています（全 230 件のうち）"), "NPS低: 切った後の件数を全件の顔で出している");
  ok(!h.includes("全 <b>200</b> 行"), "NPS低: lede と枠の上の件数が食い違っている");
});

check("V12: 法人の一覧と今日動く先が、切る前の件数を枠の上に出す", () => {
  // 法人の一覧は 200 法人で切る。250 法人を渡す
  ctx.__IX = { index: Array.from({ length: 250 }, (_, i) => ({ houjin: "h" + i, deals: 1, sites: 1,
    active: 1, ltv: 1, last_expiration: "2026-01-01" })) };
  const ix = run('custIndex(__IX, "問い", "")');
  ok(ix.includes("<b>200</b> 行を出しています（全 250 件のうち）"),
    "法人の一覧: 切った後の件数を全件の顔で出している");
  // 今日動く先: サーバが n_hit 件から 24 件に絞る（routes.rs KEEP）。
  // fixture の n_hit は控えていないので 57 を置いた（行は空。件数の書き方だけ見る）
  ctx.__TD = { rows: [], meta: { n_hit: 57, n_shown: 0, filter_rule: "", order_rule: "", mtg_gap: {} } };
  const td = run("renderToday(__TD)");
  ok(td.includes("全 57 件のうち"), "今日動く先: 絞る前の n_hit 件を出していない");
  ok(/<div class="note def"><span class="hd">この並びについて/.test(td), "並びの決まりごとが def の枠でない");
});

/* ================================================================ V15 */
check("V15: 継続回数の表で n<30 の行に印を付ける", () => {
  const box = { n: 40, min: 0, q1: 1, median: 2, q3: 3, max: 9, mean: 2.5 };
  // renewal.json の継続9回目は n=6、解約率 0.0%（2026-09-23 実測）。
  // 解約率の分母は決着済み denom（fix/cs-rust N1）。稼働中0件なので denom も 6。印も denom で判定する
  ctx.__RN2 = { meta: { exclude_right_censored: false, right_censored_n: 0 },
    monthly_retention: { rows: [] }, missingness: [], population: {},
    by_renewal: [{ renewal_no: 9, n: 6, n_active: 0, keep: 6, pending: 0, denom: 6,
      cancel_rate: 0, cancel_rate_excl_fill: 0,
      oubo: box, mensetu: box, syoudaku: box, oubo_per_posting: box, amount: box }] };
  const h = run("renderRenewal(__RN2)");
  ok(/継続9 <span class="tag few">n&lt;30<\/span>/.test(h), "n=6 の行に印が無い");
});

/* ================================================================ V16 */
check("V16: 記入率の図は継続回数×成果に重ねて出さない（データ品質に1つだけ）", () => {
  const h = run("renderRenewal(__RN2)");
  ok(!h.includes("結果ごとの記入率<") && !h.includes("svgMatrix") && !h.includes("<figcaption>結果ごとの記入率"),
    "継続回数×成果に記入率の図が残っている");
  ok(h.includes("データ品質"), "図の在りかを案内していない");
});

check("V15/V17: 満了月ごとの内訳は枠に入れ、「折れ線と同じ」と書かない", () => {
  const box = { n: 40, min: 0, q1: 1, median: 2, q3: 3, max: 9, mean: 2.5 };
  ctx.__RN3 = { meta: { exclude_right_censored: false, right_censored_n: 0 }, missingness: [], population: {},
    monthly_retention: { rows: [
      { month: "2026-01", keep: 5, cancel: 2, fill: 1, denom: 8, rate: 62.5, pending: 3 },
      { month: "2026-02", keep: 4, cancel: 1, fill: 0, denom: 5, rate: 80, pending: 9 }] },
    by_renewal: [{ renewal_no: 0, n: 40, n_active: 0, cancel_rate: 0, cancel_rate_excl_fill: 0,
      oubo: box, mensetu: box, syoudaku: box, oubo_per_posting: box, amount: box }] };
  const h = run("renderRenewal(__RN3)");
  /* 2026-09-29 組み替え（09 の 7）: 満了月ごとの内訳（約60行の表）は外した。表で読ませていた月ごとの件数と結果待ちの合計を
     黙って消していないこと（図の下の文と点の説明に移した）を見る。どちらの月も n<30 で点が無いので、文に全部書く */
  ok(!h.includes("満了月ごとの内訳") && !h.includes("上の折れ線と同じ"), "満了月ごとの内訳の表・「折れ線と同じ」が残っている");
  const t = plainOf(h);
  ok(t.includes("結果待ちは分母に入れていません（全部で 12 件）"), "結果待ちの合計（3+9）が無い");
  ok(t.includes("2026-01 62.5%（n=8。継続 5・解約 2・充足 1・結果待ち 3）") && t.includes("2026-02 80.0%（n=5。継続 4・解約 1・充足 0・結果待ち 9）"),
    "点を打たない月の率と件数が書かれていない");
});

/* ================================================================ V20 / V21 */
check("V21: 末尾の枠に同じ文を2回出さない・本文が空の枠を作らない", () => {
  const said = run('foot({ not_counted: "※ 数えていない", today: "2026-09-18" }, true)');
  ok(!said.includes("数えていない"), "頭で書いた文を末尾にもう一度出している");
  const empty = run('foot({ today: "2026-09-18" })');
  ok(!empty.includes("この画面で数えていないもの"), "書くものが無いのに「数えていないもの」の見出しを出している");
  ok(empty.includes("2026-09-18"), "基準日が出ていない");
});

check("V20: 図の読み上げ名を既定値（横棒 など）のままにしない", () => {
  const h = run('fig("接触率", "", svgBarH({ rows: [{ label: "a", v: 1 }] }))');
  ok(!h.includes('aria-label="横棒"'), "読み上げ名が「横棒」のまま");
  ok(h.includes('aria-label="接触率"'), "図の見出しが読み上げ名になっていない");
});

check("V20: 注力でない点に 1.20:1 の --rule を使わない", () => {
  const svg = run("svgDots({ total: 3, groups: [{ v: 1, color: C.ai, label: \"x\" }] })");
  ok(!svg.includes("fill:var(--rule)\""), "注力でない点が --rule（1.20:1）のまま");
});

check("V20: 図の見出しに $& などがあっても読み上げ名が壊れない", () => {
  ctx.__CAP = "拠点$&名$'";
  const h = run('fig(__CAP, "", svgBarH({ rows: [{ label: "a", v: 1 }] }))');
  ok(h.includes('aria-label="拠点$&amp;名$\'"'), "置き換えの特殊パターンとして解釈された: " +
    (h.match(/aria-label="[^"]*"/) || [""])[0]);
});

/* 本部アプローチの最小の応答。not_counted は routes.rs build_headquarters の文そのもの */
ctx.__HQ = {
  meta: { n_houjin: 1, today: "2026-09-18",
    not_counted: "※ 親法人の合計ではありません。事業所ごとに出しています。決裁は事業所単位なので、まとめると行き先が消えます" },
  multi_site: 1,
  rows: [{ houjin: "法人A", sites: 2, deals: 5, active: 1, spread: 2.5, rows: [
    /* 解約率 =（解約＋充足）÷ 決着済み denom（fix/cs-rust N4）。拠点1 は決着済み 4 のうち解約1・充足1 */
    { site: "拠点1", cpa: 300000, cancel_rate: 50, deals: 5, syoudaku: 2, active: 1, cancel: 1, fill: 1, keep: 2, denom: 4, amount: 1 },
    { site: "拠点2", cpa: 120000, cancel_rate: 0, deals: 1, syoudaku: 1, active: 0, cancel: 0, fill: 0, keep: 1, denom: 1, amount: 1 }] }],
};

check("V20: 本部アプローチの拠点の解約率に母数（決着済みの件数）を添える", () => {
  const h = run("renderHq(__HQ)");
  /* 母数は解約率の分母（決着済み denom=4）。取引数 deals=5 を書くと検算できない（fix/cs-rust N4） */
  ok(h.includes("解約・充足 50.0%（決着済み 4件中）"), "解約率に母数が無い");
});

check("V21: 本部アプローチは頭の枠を def にし、サーバの not_counted を出す", () => {
  const h = run("renderHq(__HQ)");
  const head = h.split('<div class="note ')[1] || "";
  ok(head.startsWith("def"), "頭の枠が def でない: " + head.slice(0, 20));
  ok(head.includes("決裁は事業所単位なので、まとめると行き先が消えます"), "サーバの not_counted が頭の枠に無い");
  ok((h.match(/決裁は事業所単位なので/g) || []).length === 1, "同じ文を2回出している");
});

check("V11: 点が1つの拠点の採用単価で、未確定の値を確定値と分ける", () => {
  ctx.__CB = { meta: { found: true }, cpa_by_site: [
    { site: "拠点X", points: [{ cpa: 500000, censored: true }] },
    { site: "拠点Y", points: [{ cpa: 200000, censored: false }] }] };
  const h = run('custBlocks(__CB, new Set(["cpasite"]))');
  const lineX = h.split("拠点X")[1].split("<br>")[0];
  const lineY = h.split("拠点Y")[1].split("<br>")[0];
  ok(/<span class="muted">[^<]*（未確定・稼働中）<\/span>/.test(lineX), "未確定の値に印が無い: " + lineX);
  ok(!lineY.includes("未確定"), "確定の値にまで未確定の印を付けている: " + lineY);
});

check("V3: 表示名は HubSpot 画面のラベルそのまま（探して見つかる名前）", () => {
  // platform-data-quirks/references/hubspot-deals.md: consultant＝「コンサル担当」、keisaisu＝「掲載求人数」
  ok(run('dispName("consultant")').includes("「コンサル担当」"), "consultant の表示名が HubSpot のラベルでない");
  ok(run('dispName("keisaisu")') === "掲載求人数", "keisaisu の表示名が HubSpot のラベルでない");
  // renderTeam が owner_rule を dispText に通していること（上の V2 の __D を使う）
  const h = run("renderTeam(teamOf(__D))");
  ok(h.includes("担当は HubSpot の「コンサル担当」欄 が正本です"), "renderTeam が owner_rule の内部名を置き換えていない");
  // foot(D.meta, true): 末尾の枠は基準日だけで、頭で出した not_counted をもう一度出さない（V21）
  const tailBox = h.split("集計の基準日と件数")[1];
  ok(tailBox !== undefined && !tailBox.includes("評価ではありません"), "renderTeam が not_counted を末尾でもう一度出している");
});

/* ================================================================ V1 */
check("V1: 母集団の注記は1行目だけを出して内訳を畳む", () => {
  const h = run("popline({ population: { active: 604, active_all: 703, active_option: 99, deals: 3432, deals_all: 3656, deals_option: 224 } })");
  ok(/^<details class="popnote fold"><summary>稼働中 <b>604<\/b>/.test(h), "1行目が summary になっていない: " + h.slice(0, 80));
});

/* ================================================================ 統合後レビューの残り（2026-09-23） */
/* 担当の交代の最小の応答。meta / rows の形は routes.rs build_handover のまま。
   deal_id は 11桁（HubSpot の取引ID の桁数）。件数は見張りのために置いた値 */
const hoRow = (o) => Object.assign({ deal_id: "40123456789", name: "", date: "2026-09-01",
  from_label: "前任", to_label: "後任", to_retired: false, reflected: "反映済み",
  record_gap_days: 3, is_active: false, consultant: "後任", state_label: "決着済" }, o);
ctx.__HO = {
  source_rule: "", gap_rule: "", to_retired: 0, median_gap_days: 3, n_gap: 3, reflected_dist: [],
  meta: { today: "2026-09-18", n: 3, n_active: 1, n_option_excluded: 0, n_unknown_deal: 1, not_counted: "" },
  rows: [
    hoRow({ deal_id: "40123456789", name: "", state_label: "取引が見つからない" }),
    hoRow({ deal_id: "40987654321", name: "案件A", is_active: true, state_label: "稼働中" }),
    hoRow({ deal_id: "40555555555", name: "案件B", state_label: "決着済" })],
};

check("N7: 担当の交代で、取引が見つからない行を「決着済」にせず、11桁の ID も出さない", () => {
  const h = run("renderHandover(__HO)");
  /* 2026-09-29 組み替え（09 の 3・N7）: 案件名は案件の詳細へのリンクになった。取引IDはリンクの行き先（href）の中だけで、画面の文字には出さない。
     取引が見つからない行はリンクにしない（開く詳細が無い）ので、その ID は HTML のどこにも出ない */
  ok(!/40123456789|40987654321|40555555555/.test(plainOf(h)), "取引ID（11桁）が画面の文字に出ている");
  ok(!h.includes("40123456789"), "取引が見つからない行の ID を出している（リンクにしている）");
  ok(h.includes('<a class="deallink" href="#deal/detail?id=40987654321">案件A</a>'), "案件名が案件の詳細へのリンクでない");
  ok(!/40987654321|40555555555/.test(h.replace(/href="#deal\/detail\?id=\d+"/g, "")), "取引IDがリンクの行き先の外に出ている");
  const body = h.split("<tbody>")[1] || "";
  const first = body.split("</tr>")[0];
  ok(first.includes("取引が見つからない"), "取引が見つからない行に、その言葉が出ていない: " + first.slice(0, 200));
  ok(!first.includes("決着済"), "取引が見つからない行を「決着済」と出している");
  ok((body.match(/決着済/g) || []).length === 1, "決着済の行が1行でない");
  ok(h.includes("取引が見つからない交代が 1 件"), "n_unknown_deal の件数を KPI に出していない");
});

check("色: 拠点を見分ける色に判定の色（赤・山吹・緑）を使わない", () => {
  const cols = run('siteSeries(["a", "b", "c", "d", "e"], () => [], () => 1).map((s) => s.color)');
  ok(cols.length === 5, "系列の数が 5 でない: " + cols.length);
  for (const c of cols)
    ok(!["var(--hi)", "var(--ki)", "var(--midori)"].includes(c), "拠点の色に判定の色 " + c + " を使っている");
});

check("V10: 積み上げ縦棒の整数の軸で目盛りが重複しない（0,1,1,2,2 にならない）", () => {
  // 採用数 1〜2 の拠点が積まれる図（法人の採用数の月ごとの合計）と同じ形
  const svg = run('svgColStack({ x: ["1", "2"], yFmt: F.int, series: [{ label: "a", color: C.ai, vals: [{ v: 1 }, { v: 2 }] }] })');
  const tk = [...svg.matchAll(/text-anchor="end">([0-9,.]+)<\/text>/g)].map((m) => m[1]);
  ok(tk.length >= 2, "目盛りが取れない: " + tk.join(","));
  ok(new Set(tk).size === tk.length, "目盛りが重複: " + tk.join(","));
});

check("立ち上がり: 帯ごとの解約率で特定の帯（61日超）を赤にしない", () => {
  /* 入力はどちらも全画面共通の線（CANCEL_HI＝40%）の上。この図は線でも色を分けない（5f11d15 の決め事。
     2026-09-24 に一度値で緋にしたのを戻した）。線で緋にしないことは下の「解約率の色の線」でも見る */
  ctx.__RU = { meta: {}, phase: { rows: [], rule: "" },
    first_mtg: { n: 100, pre_contract: 0, stats: null, buckets: [
      { label: "14日以内", n: 60, denom: 50, cancel_rate: 40 },
      { label: "61日超", n: 45, denom: 40, cancel_rate: 45 }] },
    no_mtg: { n: 0, first_active: 0, rate: null, note: "", rows: [] } };
  const h = run("renderRampup(__RU)");
  const g = h.split("<figcaption>帯ごとの、その後の解約率")[1].split("</figure>")[0];
  ok(!g.includes("var(--hi)"), "帯ごとの解約率の図（棒か凡例）に赤を使っている");
});

check("案件一覧: 採用単価はどの帯の中央と比べたかを帯の名前で出し、何ヶ月目の立ち位置と分ける", () => {
  ctx.__BR = { amount: 5000000, cpa: 900000, cpa_band_median: 500000, cpa_vs_band: 1.8,
    cpa_band: "契約の前半（0〜50%）", band: "序盤", months: 2, period: 12 };
  const c = run("cpaCell(__BR)");
  ok(c.includes("契約の前半（0〜50%）"), "採用単価の欄に比べた帯の名前が無い: " + c);
  const p = run("pos(__BR)");
  ok(p.includes("立ち位置 序盤"), "何ヶ月目の横の帯が採用単価の帯と区別されていない: " + p);
  // cpa_band_rule（routes.rs deal_rows の meta）を画面に出す
  ctx.__TD2 = { rows: [], expiring_this_week: [], started_this_week: [],
    meta: { n_hit: 0, n_shown: 0, filter_rule: "", order_rule: "", mtg_gap: {},
            cpa_band_rule: "採用単価は、稼働中の契約どうしを契約の進み具合で比べています" } };
  const t = run("renderToday(__TD2)");
  ok(t.includes("採用単価は、稼働中の契約どうしを契約の進み具合で比べています"), "cpa_band_rule を画面に出していない");
  ok(t.includes("0.34 / 0.67"), "立ち位置の区切りとは別だと書いていない");
});

check("いま見るべき顧客: 拠点キーが空で判定から外した件数（skipped_no_site）を出す", () => {
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  fo.cpa.skipped_no_site = 7;
  ctx.__FO2 = fo;
  const h = run("renderFocus(__FO2)");
  ok(h.includes("拠点が入っていない取引 7 件"), "skipped_no_site の件数が画面に無い");
});

check("案件一覧: 採用目標に対しての欄で、稼働中の 0〜50% を赤にしない（途中の値）", () => {
  for (const r of [0, 0.25, 0.6]) {
    const g = run("goalCell(" + JSON.stringify({ saiyomokuhyou: 4, rate_tassei: r, syoudaku: r * 4 }) + ")");
    ok(!g.includes("var(--hi)") && !g.includes("var(--ki)"), "達成率 " + r + " に判定の色: " + g);
    ok(g.includes("途中"), "達成率 " + r + " に途中の値だという印が無い: " + g);
  }
  const g = run('goalCell({ saiyomokuhyou: 4, rate_tassei: 1, syoudaku: 4 })');
  ok(g.includes("var(--midori)") && !g.includes("途中"), "100% の確定を途中として出している: " + g);
});

check("本部・悪化の図: site_name が無い拠点を照合用のキーで埋めない", () => {
  const hq = JSON.parse(JSON.stringify(ctx.__HQ));
  hq.rows[0].rows[0].site = "kyotenkey_aaa";
  hq.rows[0].rows[0].site_name = null;
  hq.rows[0].rows[1].site_name = "拠点2の名前";
  ctx.__HQ2 = hq;
  const h = run("renderHq(__HQ2)");
  ok(!h.includes("kyotenkey_aaa"), "本部アプローチに照合用のキーが出ている");
  ok(h.includes("拠点名が無い"), "本部アプローチに「拠点名が無い」が出ていない");
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  fo.cpa = { worse: 1, judged: 1, skipped_censored: 0, rows: [
    { site: "kyotenkey_bbb", site_name: null, prev: 100, last: 200, ratio: 2 }], note: "" };
  ctx.__FO3 = fo;
  const f = run("renderFocus(__FO3)");
  ok(!f.includes("kyotenkey_bbb"), "採用単価の悪化の図に照合用のキーが出ている");
  ok(f.includes("拠点名が無い"), "採用単価の悪化の図に「拠点名が無い」が出ていない");
});

check("本部アプローチ: cancel_rule / cpa_rule を上位10法人の図ごとに繰り返さない", () => {
  const hq = JSON.parse(JSON.stringify(ctx.__HQ));
  hq.meta.cancel_rule = "解約率のきまりXYZ";
  hq.meta.cpa_rule = "採用単価のきまりXYZ";
  hq.rows = [0, 1, 2].map((i) => Object.assign({}, ctx.__HQ.rows[0], { houjin: "法人" + i }));
  ctx.__HQ3 = hq;
  const h = run("renderHq(__HQ3)");
  ok((h.match(/解約率のきまりXYZ/g) || []).length === 1, "cancel_rule が " + (h.match(/解約率のきまりXYZ/g) || []).length + " 回出ている");
  ok((h.match(/採用単価のきまりXYZ/g) || []).length === 1, "cpa_rule が " + (h.match(/採用単価のきまりXYZ/g) || []).length + " 回出ている");
});

/* ================================================================ 図の部品（2026-09-23 デプロイ後の実機確認）
   デプロイ後に Playwright で見た「図の文字が重なる・切れる・読めない」を、描いた SVG の文字の
   位置と幅から数で確かめる。幅はこのテストの側で持つ見積もり（chromium で BIZ UDPGothic 11px を
   測った 1 文字あたりの幅: 全角 11.1 / 数字 8.3 / 英大文字 8.5 / 英小文字 6.9 / 記号 5.7）で、
   画面の JS の textW には頼らない（頼ると、見積もりを間違えたときに両方そろって通ってしまう）。 */
const TW = (s) => [...String(s)].reduce((a, ch) =>
  a + (ch.charCodeAt(0) > 0xff ? 11.1 : /[0-9]/.test(ch) ? 8.3 : /[A-Z%mw]/.test(ch) ? 8.5
       : /\s/.test(ch) ? 3.5 : /[.,:;()\-/=|!'_]/.test(ch) ? 5.7 : 6.9), 0);
const unq = (s) => s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&amp;/g, "&");
/** SVG の中の文字を箱にする（x の揃えを見て左右を出す。縦は 11px の字の高さ） */
function textBoxes(svg) {
  return [...svg.matchAll(/<text class="(ax|axl|vl)" x="([-\d.]+)" y="([-\d.]+)"([^>]*)>([^<]*)/g)].map((m) => {
    const x = +m[2], y = +m[3], s = unq(m[5]), w = TW(s) * (m[1] === "vl" ? 1.04 : 1);
    const anc = (m[4].match(/text-anchor="(\w+)"/) || [0, "start"])[1];
    const x0 = anc === "end" ? x - w : anc === "middle" ? x - w / 2 : x;
    return { s, x0, x1: x0 + w, y0: y - 9, y1: y + 2 };
  });
}
function overlaps(svg) {
  const b = textBoxes(svg), out = [];
  for (let i = 0; i < b.length; i++) for (let j = i + 1; j < b.length; j++) {
    const ox = Math.min(b[i].x1, b[j].x1) - Math.max(b[i].x0, b[j].x0);
    const oy = Math.min(b[i].y1, b[j].y1) - Math.max(b[i].y0, b[j].y0);
    if (ox > 1 && oy > 1) out.push(b[i].s + " と " + b[j].s);
  }
  return out;
}
/* 帯を縦に積む図・時間軸の図は、左のラベルだけの SVG（sticklab）を前に重ねている。幅は図そのもの（stickmain）で見る */
const vbW = (svg) => +((svg.match(/<svg class="stickmain"[^>]*viewBox="0 0 ([\d.]+)/) ||
  svg.match(/viewBox="0 0 ([\d.]+)/) || [0, 0])[1]);
const vbH = (svg) => +((svg.match(/<svg class="stickmain"[^>]*viewBox="0 0 [\d.]+ ([\d.]+)/) ||
  svg.match(/viewBox="0 0 [\d.]+ ([\d.]+)/) || [0, 0, 0])[1]);
/** 文字がすべて SVG の範囲（viewBox）の中にあるか。外に出た文字の一覧を返す */
const outside = (svg) => {
  const W = vbW(svg), H = vbH(svg);
  return textBoxes(svg).filter((b) => b.x0 < -0.5 || b.x1 > W + 0.5 || b.y0 < -0.5 || b.y1 > H + 0.5)
    .map((b) => b.s + "(" + b.x0.toFixed(0) + "," + b.y0.toFixed(0) + ")");
};
const firstSvg = (h) => { const a = h.indexOf("<svg"); return h.slice(a, h.indexOf("</svg>", a) + 6); };

check("図の部品(1): 狭い画面で図を縮めきらず、枠の中で横に動かす（文字 10px を下限にする）", () => {
  // CSS: 図の最小幅を描いた幅（--fw）から決める。min-width は max-width:100% より強い
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/figure\.fig \.figbody > svg\{\s*min-width:calc\(var\(--fw, 0px\) \* \.92\)/.test(css),
    "図の最小幅（--fw の .92 倍）の CSS が無い。400px 幅で 11px の文字が 4〜5px に縮む");
  ok(/figure\.fig \.figbody\{[^}]*overflow-x:auto/.test(css), "図の枠が横にスクロールしない（ページ本体が広がる, V17）");
  /* 文の案内は「枠の幅 < 図の最小幅」のときだけ出す（2026-09-23 検証）。
     前は @media (max-width:600px) でだけ出していて、601〜1000px 前後でスクロールする図に案内が無かった。
     画面幅の @media に戻したら落ちる。枠の幅（100cqw）と図の最小幅（--minw）で決めていること */
  ok(/figure\.fig\{ container-type:inline-size; \}/.test(css), "figure が問い合わせの入れ物（container-type）になっていない");
  ok(/\.figscroll\{[^}]*height:clamp\(0px, calc\(\(var\(--minw, 0px\) - 100cqw\) \* 999\), 20px\)/.test(css),
    "横スクロールの案内の出し分けが、枠の幅と図の最小幅の比較になっていない");
  ok(!/\.figscroll\{[^}]*display:none/.test(css), "横スクロールの案内を display:none で隠している（@media でしか出ない形に戻っている）");
  // どの図の道具も --fw を持つ。持たない図だけが 400px で縮む
  // 🔴 matrix / funnel / stack / dots も見る（dq の「結果ごとの記入率」は幅 638px。2026-09-23 検証で見張りの外だった）
  const svgs = {
    matrix: run('svgMatrix({ padL: 150, cw: 120, cols: ["a","b","c","d"], rows: [{ label: "r", cells: [{ v: .5 }, { v: .5 }, { v: null }, { v: 1 }] }] })'),
    funnel: run('svgFunnel({ steps: [{ label: "応募", v: 10 }, { label: "面接", v: 4 }] })'),
    stack: run('svgStack({ w: 680, parts: [{ label: "a", v: 3 }, { label: "b", v: 1 }] })'),
    dots: run('svgDots({ total: 50, per: 37, groups: [{ v: 10, color: "red", label: "a" }] })'),
    line: run('svgLine({ x: ["a","b"], series: [{ pts: [{ v: 1 }, { v: 2 }] }] })'),
    bar: run('svgBarH({ rows: [{ label: "a", v: 1 }] })'),
    box: run('svgBoxH({ rows: [{ label: "a", med: 2, q1: 1, q3: 3, min: 0, max: 4, n: 40 }] })'),
    lanes: run('svgStackLanes({ w: 940, months: ["1","2"], lanes: [{ label: "a", type: "line", color: "red", pts: [{ v: 1 }, { v: 2 }] }] })'),
    col: run('svgColStack({ x: ["a"], series: [{ label: "s", color: "red", vals: [{ v: 1 }] }] })'),
    tl: run('svgTimeline({ lanes: [{ label: "a", marks: [{ d: "2025-01-01" }] }] })'),
    sc: run('svgScatter({ pts: [{ x: 1, y: 1 }, { x: 2, y: 3 }] })'),
    hist: run('svgHist({ values: [1, 2, 3] })'),
  };
  for (const [k, v] of Object.entries(svgs))
    ok(/style="--fw:\d+px"/.test(v), k + " の図に --fw（描いた幅）が無い");
  const f = run('fig("題", "", svgBarH({ w: 700, rows: [{ label: "a", v: 1 }] }))');
  ok(f.includes('class="figscroll" style="--minw:644px"'), "700px の図の横スクロールの案内に、図の最小幅（700×.92=644px）が無い");
  // 横にスクロールしうる枠には候補の印（data-cap）だけ付ける。tabindex・読み上げ名は描いた後に
  // 実際のはみ出しを測って付ける（2026-09-23 レビュー F1: minW > 330 だけで付けていて、
  // 1440px で横に動かない図まで全部 Tab で止まっていた）
  ok(/<div class="figbody" data-cap="題">/.test(f), "横にスクロールしうる枠に候補の印（data-cap）が無い");
  ok(!/tabindex|横にスクロールできる枠/.test(f),
    "描いた時点（はみ出しを測る前）で tabindex / 「横にスクロールできる枠」を付けている（F1。1440px でも Tab で止まる）");
  const small = run('fig("小", "", svgStack({ w: 300, parts: [{ label: "a", v: 1 }] }))');
  ok(!/figscroll|tabindex|data-cap/.test(small), "330px に収まる図にまでスクロールの案内・tabindex を付けている");
});

check("図の部品(1b): 図の枠が実際にはみ出しているときだけ Tab で止まり、案内も同じ条件で出す（F1）", () => {
  /* 偽の枠。markFig が付け外しする属性とクラスを覚える */
  const attrs = { "data-cap": "題" }, cls = new Set();
  const b = { scrollWidth: 1000, clientWidth: 400,
    setAttribute: (k, v) => { attrs[k] = v; }, removeAttribute: (k) => { delete attrs[k]; },
    getAttribute: (k) => (k in attrs ? attrs[k] : null),
    parentNode: { classList: { add: (c) => cls.add(c), toggle: (c, on) => { if (on) cls.add(c); else cls.delete(c); } } } };
  ctx.__FB = b;
  run("markFig(__FB)");   // 400px 幅: はみ出している
  ok(attrs.tabindex === "0" && attrs.role === "group" && attrs["aria-label"] === "題（横にスクロールできる枠）",
    "はみ出している枠に tabindex / 読み上げ名が無い: " + JSON.stringify(attrs));
  ok(cls.has("fig-measured") && cls.has("fig-over"), "はみ出している枠で案内を出す印（fig-over）が無い: " + [...cls]);
  b.clientWidth = 1000; run("markFig(__FB)");   // 1440px 幅: 収まった
  ok(!("tabindex" in attrs) && !("role" in attrs) && !("aria-label" in attrs),
    "はみ出していない枠に tabindex / 読み上げ名が残っている（1440px でも Tab で止まる, F1）: " + JSON.stringify(attrs));
  ok(cls.has("fig-measured") && !cls.has("fig-over"), "はみ出していない枠で案内を出す印が残っている: " + [...cls]);
  // 案内の CSS は、測った後は fig-over だけで出し分ける（tabindex と同じ条件）
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/figure\.fig\.fig-measured \.figscroll\{ height:0; \}/.test(css) &&
     /figure\.fig\.fig-measured\.fig-over \.figscroll\{ height:20px; \}/.test(css),
    "測った後の案内の出し分けが、はみ出しの測定（fig-over）になっていない");
  // 描いた後・窓の幅が変わったとき（markScrollAll）に測り直している
  ok(/function markScrollAll\(\) \{[^}]*\.figbody\[data-cap\]"\)\.forEach\(markFig\)/.test(html),
    "markScrollAll（描いた後・resize・details の開閉）で図の枠を測り直していない");
});

check("図の部品(2): 左のラベルが欄より長いとき、省略記号で切り、全文を title に残す", () => {
  // houjin の採用単価（shortName(…, 20)）、拠点の開き、dq の欠測の件数で頭が切れていた
  const long = "ケアサポートかがやき居宅介護支援事業所ステップアップ継続①";
  ctx.__LL = [long, "右側打ち切り（結果が確定していない直近の契約）"];
  const hs = {
    bar: run('svgBarH({ w: 700, rows: [{ label: __LL[0], v: 1 }, { label: __LL[1], v: 2 }] })'),
    box: run('svgBoxH({ w: 680, rows: [{ label: __LL[0], med: 2, q1: 1, q3: 3, min: 0, max: 4, n: 40 }] })'),
    tl: run('svgTimeline({ w: 940, lanes: [{ label: __LL[0], marks: [{ d: "2025-01-01" }, { d: "2025-06-01" }] }] })'),
    lanes: run('svgStackLanes({ w: 940, months: ["1","2"], lanes: [{ label: __LL[0], type: "line", color: "red", pts: [{ v: 1 }, { v: 2 }] }] })'),
  };
  for (const [k, h] of Object.entries(hs)) {
    const labs = textBoxes(h).filter((b) => b.s.includes("…"));
    ok(labs.length >= 1, k + ": 長いラベルに省略記号が付いていない（頭が黙って切れる）");
    labs.forEach((b) => ok(b.x0 >= -0.5, k + ": ラベル「" + b.s + "」の頭が SVG の左端より外（" + b.x0.toFixed(1) + "）"));
    ok(h.includes("<title>" + long + "</title>"), k + ": 切ったラベルの全文が title に無い");
    ok(labs.some((b) => b.s.startsWith("ケア") && b.s.endsWith("継続①")),
      k + ": 頭（社名）と末尾（継続①）の両方が残っていない: " + labs.map((b) => b.s).join(" / "));
  }
  // 収まるラベルは切らない・title も足さない
  const short = run('svgBarH({ rows: [{ label: "初回", v: 1 }] })');
  ok(!short.includes("…") && !short.includes("<title>初回</title>"), "収まるラベルまで切っている");

  /* ---- 2026-09-23 検証の指摘 ----
     (a) houjin の拠点ラベルは shortName(…, 20) で頭が削られ「…」で始まる。fitLab はその流儀で
         頭を削り足す（真ん中を削ると「…会 特…継続①」と省略記号が2つになる）
     (b) title と全文の一覧に入るのは shortName を掛ける前の名前（行の full）。前は「…会 …」しか残らなかった
     (c) shortName だけで削られ、fitLab では切らなかった行にも全文を残す
     (d) 全文は図の外（<details>）に並ぶ。title はホバーでしか出ず、role="img" の中は読み上げに出ない */
  const orig = "社会福祉法人みどり会 特別養護老人ホームさくら苑 継続①";
  ctx.__HJ = orig;
  const hj = run('svgBarH({ w: 680, fmt: F.man, rows: [{ label: shortName(__HJ, 20), full: __HJ, v: 1200000, txt: "120万" }], padL: 150 })');
  const hl = textBoxes(hj).find((b) => b.s.endsWith("継続①"));
  ok(hl && hl.s.startsWith("…") && (hl.s.match(/…/g) || []).length === 1,
    "(a) 頭を削ったラベル（…始まり）を真ん中で削っている: " + (hl ? hl.s : "(無い)"));
  ok(hl && hl.x0 >= -0.5, "(a) 頭を削ったラベルが SVG の左端より外に出る");
  ok(hj.includes("<title>" + orig + "</title>") && hj.includes('data-full="' + orig + '"'),
    "(b) title / 全文の一覧に、shortName を掛ける前の名前が入っていない");
  const onlyTail = run('svgBarH({ w: 680, rows: [{ label: tail(__HJ, 10), full: __HJ, v: 1 }] })');
  ok(!textBoxes(onlyTail).some((b) => /….*…/.test(b.s)) && onlyTail.includes('data-full="' + orig + '"'),
    "(c) shortName / tail だけで削った行（fitLab では切らない行）に全文が残っていない");
  const fg = run('fig("採用単価", "", svgBarH({ w: 680, rows: [{ label: shortName(__HJ, 20), full: __HJ, v: 1 }, { label: tail(__HJ, 10), full: __HJ, v: 2 }] }))');
  const det = fg.slice(fg.indexOf("</svg>"));
  ok(/<details class="fold figfull"><summary>省略した名前の全文（1 件）<\/summary>/.test(det) && det.includes(orig),
    "(d) 省略した名前の全文が図の外（タップ・読み上げで読める所）に無い（同じ名前は1件にまとめる）");
  // 「株式会社」を落としただけ（省略記号が無い）は切ったと数えない
  ctx.__KK = "株式会社みどり";
  const kk = run('fig("x", "", svgBarH({ rows: [{ label: shortName(__KK, 20), full: __KK, v: 1 }] }))');
  ok(!/figfull|data-full/.test(kk), "法人格を落としただけの名前まで「省略した名前」に並べている");
  // ラベル欄の幅は字の実寸で見積もる（labW）。全角 15 字は上限 210px に収まるので切らない。
  // 前は 1 字 10.6px と数えて欄が 177px になり、168px の文字が入らず切れていた
  const l15 = run('svgBarH({ rows: [{ label: "ケアサポートかがやき居宅介護支", v: 1 }] })');
  ok(!l15.includes("…"), "全角 15 字のラベルが、欄に収まるのに切られている（ラベル欄の幅の見積もりが粗い）");
  ok(textBoxes(l15).every((b) => b.x0 >= -0.5), "全角 15 字のラベルが SVG の左端より外に出る");
});

/** 継続回数 × 成果の、月次の継続率の節から後ろ */
const retPart = (h) => h.slice(h.indexOf("月次の継続率（満了月ベース"));
check("図の部品(3): 月次継続率の右端でラベルが重ならず、n=0 の月に点も線も作らない", () => {
  // fixture の monthly_retention（2026-09-23 実測）の末尾: 26-10 n=8 / 26-11 n=0 / 26-12 n=0 /
  // 27-01 n=1 / 27-02 以降 n=0（末尾は図から外す）。前は「26-11」「27-01」と「n=0」「n=1」が重なり、
  // n=0 の月の 0 の高さに灰色の短い線が出ていた
  const rows = [];
  for (let i = 0; i < 44; i++) {
    const y = 2023 + Math.floor((i + 1) / 12), m = (i + 1) % 12 + 1;
    rows.push({ month: y + "-" + String(m).padStart(2, "0"), denom: 60, rate: 50, pending: 0, keep: 30, cancel: 20, fill: 10 });
  }
  const tailRows = [["2026-10", 8, 100, 105], ["2026-11", 0, null, 118], ["2026-12", 0, null, 88],
    ["2027-01", 1, 0, 49], ["2027-02", 0, null, 57], ["2027-03", 0, null, 55]];
  const all = rows.filter((r) => r.month < "2026-10").concat(tailRows.map(([month, denom, rate, pending]) =>
    ({ month, denom, rate, pending, keep: 0, cancel: 0, fill: 0 })));
  ctx.__RN = { monthly_retention: { rows: all }, by_renewal: [], population: { deals_option: 99 },
    meta: { n_deals: 3432, right_censored_n: 419, exclude_right_censored: false, not_counted: "" }, missingness: [] };
  /* 2026-09-29 組み替え（09 の 7）: 継続回数ごとの解約率が先頭の図になったので、月次の継続率の図から見る */
  const svg = firstSvg(retPart(run("renderRenewal(__RN)")));
  const ov = overlaps(svg);
  ok(!ov.length, "月次継続率の文字が重なる: " + ov.slice(0, 4).join(" / "));
  ok(!/値が無い（0 ではない）/.test(svg), "n=0 の月に 0 の高さの灰色の線を描いている（凡例に無い印）");
  ok(svg.includes(">27-01<") && svg.includes(">n=1<"), "最後の月（27-01 n=1）のラベルが無い");
  // 🔴 率に母数（2026-09-23 検証）。xPick・間引きで横軸の2段目（n=）を出さない月でも、点の title で母数が読める
  const tt = [...svg.matchAll(/<circle [^>]*><title>([^<]*)<\/title>/g)].map((m) => m[1]);
  ok(tt.length > 0 && tt.every((t) => /（n=\d+）/.test(t)),
    "点の title に母数（n=）が無い月がある: " + tt.filter((t) => !/（n=\d+）/.test(t)).slice(0, 3).join(" / "));
  const hidden = tt.filter((t) => !svg.includes(">" + t.split("（")[0] + "<"));
  ok(hidden.length > 0, "この入力では横軸のラベルを出さない月があるはず（見張りの前提が崩れている）");
  // 値が無い月（n=0）で線が切れることを、凡例で説明している（fig の data-gap）
  ok(/<svg [^>]*data-gap="1"/.test(svg), "n=0 の月で線が切れるのに、図に「切れ目あり」の印（data-gap）が無い");
  // 🔴 n<30 の月は値があっても点を打たないので、決まり文句（その月の値が無い）ではなく
  //    この図の理由で説明する（ループ4 renewal の見張りで文の中身を見る）
  const rnTxt = run("renderRenewal(__RN)");
  ok(/線が途切れているところは、[^<]*（0 ではありません）/.test(rnTxt),
    "月次継続率の図の凡例に、線の切れ目の説明が無い");
});

check("図の部品(4): 系列を縦に並べる図で、NPS の「定期N」・右の目盛り・帯の境目の目盛りが重ならない", () => {
  // fixture の series（2026-09-23 実測）で「定期5 と 0」「定期NPS と 定期1」が重なっていた形
  const h = run(`svgStackLanes({ w: 940, months: ["4ヶ月","5ヶ月","6ヶ月","7ヶ月"], lanes: [
    { label: "応募", type: "line", color: "blue", pts: [{ v: 3 }, { v: 12 }, { v: 13 }, { v: 13, carry: true }] },
    { label: "面接", type: "line", color: "blue", pts: [{ v: 0 }, { v: 4 }, { v: 4, carry: true }, { v: 9 }] },
    { label: "定期NPS", type: "dots", pts: [{ v: 1, round: 1 }, { v: 10, round: 5 }, { v: 0, round: 5 }, { v: 1, round: 5 }] },
    { label: "接触", type: "bars", color: "gray", fillLabel: "接触", outLabel: "MTG", pts: [null, { fill: 2 }, { fill: 1 }, null] } ] })`);
  const ov = overlaps(h);
  ok(!ov.length, "文字が重なる: " + ov.slice(0, 5).join(" / "));
  const W = vbW(h);
  textBoxes(h).forEach((b) => ok(b.x0 >= -0.5 && b.x1 <= W + 0.5, "「" + b.s + "」が SVG の外に出る"));

  /* 🔴 2026-09-23 検証: 上の2つは、NPS の帯の高さ・点を内側に描く幅・右の目盛りの位置を
     全部戻しても通っていた（この入力では文字どうしが重ならないため）。是正の中身そのものを見る:
     (a) どの文字も帯の境目（各帯の下端の線）をまたがない。NPS の値・「定期N」が帯の中に収まる
     (b) 右の目盛りは、その帯の線・点の実際の上端・下端の高さにある（数字が指す点と同じ高さ）。
         前は帯の上端 +12 / 下端 −2 に置いていて、目盛りの数字と点の高さがずれていた */
  const axY = [...h.matchAll(/<line class="axisline" x1="[\d.]+" y1="([\d.]+)"/g)].map((m) => +m[1]);
  ok(axY.length === 4, "帯の下端の線が " + axY.length + " 本（4 のはず）");
  const cross = textBoxes(h).filter((b) => axY.some((y) => y > b.y0 + 0.5 && y < b.y1 - 0.5));
  ok(!cross.length, "(a) 帯の境目をまたぐ文字: " + cross.map((b) => b.s + "(" + b.y0.toFixed(0) + "〜" + b.y1.toFixed(0) + ")").join(" / "));
  const cy = [...h.matchAll(/<circle cx="[\d.]+" cy="([\d.]+)"/g)].map((m) => +m[1]);
  const tickX = 940 - 68 + 12;   // 右の目盛りの位置（w − padR + 12）
  const tks = textBoxes(h).filter((b) => Math.abs(b.x0 - tickX) < 0.01);
  ok(tks.length === 7, "右の目盛りが x=" + tickX + " に " + tks.length + " 個（応募2・面接2・NPS2・接触1 の 7 のはず）: " + tks.map((b) => b.s).join(","));
  // (c) NPS の 0〜10 を描く高さ。帯を 62px にして上下 18px を文字に取っても、0 と 10 の点は 24px 以上離す
  //     （帯を 48px に戻すと 12px になり、1 点ぶんの差が 1.2px で見分けられない）
  const nps = [...h.matchAll(/<circle cx="[\d.]+" cy="([\d.]+)" r="5"[^>]*><title>[^:]*: 定期\d+ の回答 (\d+)<\/title>/g)].map((m) => [+m[2], +m[1]]);
  const y10 = (nps.find((q) => q[0] === 10) || [0, NaN])[1], y0 = (nps.find((q) => q[0] === 0) || [0, NaN])[1];
  ok(y0 - y10 >= 24, "(c) NPS の 0 と 10 の点の高さの差が " + (y0 - y10) + "px（24px 未満。帯が低すぎて点の差が読めない）");
  tks.filter((b) => b.y1 < axY[2] + 0.5).forEach((b) => {
    const y = b.y0 + 9 - 4;   // 目盛りの文字は点の高さ +4 に置く
    ok(cy.some((c) => Math.abs(c - y) < 0.6), "(b) 右の目盛り「" + b.s + "」が、どの点の高さにも合っていない（y=" + y.toFixed(1) + "）");
  });
});

check("図の部品(5): 軸の題名と最上段の目盛りが重ならず、整数の軸の目盛りが等間隔", () => {
  const ln = run('svgLine({ x: ["a","b","c"], series: [{ pts: [{ v: 0.2 }, { v: 1 }, { v: 0.5 }] }], yFmt: F.pct, yLab: "継続率" })');
  ok(!overlaps(ln).length, "折れ線: " + overlaps(ln).join(" / "));
  const cs = run('svgColStack({ w: 940, x: ["26-01","26-02"], series: [{ label: "s", color: "red", vals: [{ v: 20 }, { v: 18 }] }], yLab: "採用数（累計）" })');
  ok(!overlaps(cs).length, "積み上げ縦棒: " + overlaps(cs).join(" / "));
  // 整数の軸（件数）で 0〜10 を4段に切ると刻みが 2.5 になり「0,3,5,8,10」と出ていた
  const tk = run("ticks(0, 10, 4, 1)");
  const st = tk.slice(1).map((v, i) => v - tk[i]);
  ok(tk.every((v) => Number.isInteger(v)), "整数の軸の目盛りに端数がある: " + tk.join(","));
  ok(st.every((d) => d === st[0]), "目盛りが等間隔でない: " + tk.join(","));
  // LTV の横軸で最後の2つ（26-07 と 26-09）がくっついていた形。38 か月を 940px に並べる
  ctx.__X = [];
  for (let i = 0; i < 38; i++) { const y = 23 + Math.floor((i + 6) / 12), m = (i + 6) % 12 + 1; ctx.__X.push(y + "-" + String(m).padStart(2, "0")); }
  const ltv = run('svgColStack({ w: 940, x: __X, series: [{ label: "s", color: "red", vals: __X.map((_, i) => ({ v: 1000000 + i * 10000 })) }], yFmt: F.man, yLab: "累計金額（万円）" })');
  ok(!overlaps(ltv).length, "LTV の横軸: " + overlaps(ltv).slice(0, 3).join(" / "));
  // 箱ひげの「最大 609」と右端の「n=1083」の間を空ける（rampup の初回MTGまで, fixture 実測）
  const bx = run('svgBoxH({ w: 680, rows: [{ label: "初回MTGまで", med: 14, q1: 4, q3: 43, min: 0, max: 609, mean: 40, n: 1083 }] })');
  const b = textBoxes(bx), mx = b.find((q) => q.s.startsWith("最大")), nn = b.find((q) => q.s.startsWith("n="));
  ok(mx && nn && nn.x0 - mx.x1 >= 20, "「最大 609」と「n=1083」の間が " + (mx && nn ? (nn.x0 - mx.x1).toFixed(1) : "?") + "px（20px 未満）");
  /* 🔴 2026-09-23 検証: 散布図・ヒストグラムの題名の余白は見張りの外だった。
     また折れ線の余白だけを戻すと題名が y=2 に来て SVG の上端の外で切れるが、重なりしか見ていなかった。
     題名のある 4 つの図すべてで、重ならないことと SVG の中にあることを見る */
  const sc = run('svgScatter({ pts: [{ x: 1, y: 100 }, { x: 2, y: 300 }], yLab: "金額" })');
  const hs = run('svgHist({ values: [1, 2, 2, 3, 3, 3], yLab: "件数" })');
  for (const [k, v] of Object.entries({ 折れ線: ln, 積み上げ縦棒: cs, 散布図: sc, ヒストグラム: hs })) {
    ok(!overlaps(v).length, k + ": 題名と目盛りが重なる: " + overlaps(v).join(" / "));
    ok(!outside(v).length, k + ": SVG の外に出る文字: " + outside(v).join(" / "));
  }
  /* 🔴 整数の軸は刻み 2.5 を 5 にするので、最大 7〜9 だと目盛りが [0,5] で止まる（2026-09-23 検証:
     ticks(0,9,4,1) → [0,5]）。縦軸を持つ図は、最大値を覆うところまで目盛りを足す（coverTicks） */
  const topTick = (svg) => Math.max(...textBoxes(svg).filter((b) => /^\d+$/.test(b.s) && b.x1 < 60).map((b) => +b.s));
  const c9 = run('svgColStack({ x: ["a","b"], series: [{ label: "s", color: "red", vals: [{ v: 9 }, { v: 7 }] }] })');
  ok(topTick(c9) >= 9, "積み上げ縦棒: 最大 9 に対して目盛りが " + topTick(c9) + " で止まる");
  const h9 = run('svgHist({ values: [1,1,1,1,1,1,1,1,1, 5], bins: 4 })');
  ok(topTick(h9) >= 9, "ヒストグラム: 度数 9 に対して目盛りが " + topTick(h9) + " で止まる");
  const s9 = run('svgScatter({ pts: [{ x: 1, y: 9 }, { x: 2, y: 3 }] })');
  ok(topTick(s9) >= 9, "散布図: 最大 9 に対して目盛りが " + topTick(s9) + " で止まる");
});

check("図の部品(6): 接触率の図の目盛りが最大値を覆い、率と母数が棒の近くにある", () => {
  // fixture の接触率（母数が足りる担当者）の最大は 96.92%。前は目盛りが 75% で止まっていた
  const h = run(`svgBarH({ w: 720, fmt: F.pp, rh: 24, rows: [
    { label: "h9821a39368fe", v: 26.4, txt: "26.4%", note: "33/125 か月　案件28" },
    { label: "h60b499e1c307", v: 96.92, txt: "96.9%", note: "63/65 か月　案件22" },
    { label: "h14989084e280", v: 57.14, txt: "57.1%", note: "4/7 か月" } ] })`);
  const tks = textBoxes(h).filter((b) => /^\d+%$/.test(b.s)).map((b) => parseFloat(b.s));
  ok(Math.max(...tks) >= 96.92, "目盛りの最大が " + Math.max(...tks) + "%（96.9% の棒が目盛りの先へ伸びる）");
  const b = textBoxes(h), v = b.find((q) => q.s === "96.9%"), n = b.find((q) => q.s.startsWith("63/65"));
  ok(n.x0 - v.x1 <= 60, "いちばん長い棒の率から母数まで " + (n.x0 - v.x1).toFixed(0) + "px 離れている（前は約200px）");
  /* 🔴 2026-09-23 検証: 上の 60px は、注記の置き方を全部戻しても通っていた（入力の注記がほぼ同じ長さで、
     右端ぞろえでも左ぞろえでも位置が変わらなかった）。長さの違う注記を混ぜ、
     注記が値ラベルの欄のすぐ右に**左ぞろえで1列に**並ぶこと・どの行も率から 40px 以内に始まることを見る */
  const notes = b.filter((q) => / か月/.test(q.s));
  ok(notes.length === 3 && notes.every((q) => Math.abs(q.x0 - notes[0].x0) < 0.5),
    "注記が左ぞろえの1列になっていない（右端ぞろえに戻っている）: " + notes.map((q) => q.x0.toFixed(0)).join(","));
  ok(notes[0].x0 - v.x1 <= 40, "いちばん長い棒の率から注記の列まで " + (notes[0].x0 - v.x1).toFixed(0) + "px");
  // 右の欄は注記の実寸で空ける（1字 10.6px と数えると、いちばん長い注記の右に余白が余り、そのぶん棒が短くなる）
  const slack = vbW(h) - Math.max(...notes.map((q) => q.x1));
  ok(slack <= 16, "いちばん長い注記の右に " + slack.toFixed(0) + "px の余白（右の欄を空けすぎて棒が短くなる）");
  // 棒の短い行は、値から注記まで細い線でつなぐ。**実線**にする（破線・点線は「未確定・持ち越し」の意味）
  const leaders = [...h.matchAll(/<line data-leader="1"[^>]*>/g)].map((m) => m[0]);
  ok(leaders.length >= 1, "短い棒の行に、注記までつなぐ線が無い");
  ok(leaders.every((l) => !/stroke-dasharray/.test(l)), "注記までつなぐ線が点線・破線（未確定の印と読める）");
  ok(!overlaps(h).length, "文字が重なる: " + overlaps(h).join(" / "));
  /* 長い注記（幅 326px）。前は右の欄の上限が max(320, w×.5)=360px で足りず、右端に寄せた注記が
     値ラベル「96.9%」に重なった（2026-09-23 検証）。重ならず、SVG の中に収まる */
  const lg2 = run(`svgBarH({ w: 720, fmt: F.pp, rows: [
    { label: "a", v: 26.4, txt: "26.4%", note: "1/2 か月" },
    { label: "b", v: 96.92, txt: "96.9%", note: "63/65 か月　案件22　担当交代あり 2 回（直近 2026-08）" } ] })`);
  ok(!overlaps(lg2).length, "長い注記が値ラベルに重なる: " + overlaps(lg2).join(" / "));
  ok(!outside(lg2).length, "長い注記が SVG の外に出る: " + outside(lg2).join(" / "));
});

check("図の部品(7): 推移の図で、値が変わった月へ向かう線は実線・持ち越しへ向かう線だけ破線", () => {
  const h = run('svgLine({ x: ["5ヶ月","6ヶ月","7ヶ月","8ヶ月"], series: [{ color: "blue", pts: [{ v: 30 }, { v: 30, carry: true }, { v: 74 }, { v: null }] }] })');
  const segs = [...h.matchAll(/<path d="M[^"]*" fill="none"[^>]*>/g)].map((m) => m[0]);
  ok(segs.length === 2, "線の本数が " + segs.length + "（2 のはず）");
  ok(/stroke-dasharray/.test(segs[0]), "持ち越しの点（6ヶ月）へ向かう線が破線でない");
  ok(!/stroke-dasharray/.test(segs[1]), "値が変わった月（7ヶ月・塗りの点）へ向かう線が破線になっている（凡例「破線＝持ち越し」と食い違う）");
  ok(!/<line [^>]*style="stroke:var\(--ghost\)"/.test(h), "値が無い月（8ヶ月）に、凡例に無い灰色の短い線を 0 の位置へ描いている");
  const ln = run(`svgStackLanes({ w: 940, months: ["1","2","3"], lanes: [
    { label: "応募", type: "line", color: "blue", pts: [{ v: 30 }, { v: 30, carry: true }, { v: 74 }] } ] })`);
  const s2 = [...ln.matchAll(/<path d="M[^"]*" fill="none"[^>]*>/g)].map((m) => m[0]);
  ok(s2.length === 2 && /stroke-dasharray/.test(s2[0]) && !/stroke-dasharray/.test(s2[1]),
    "系列を縦に並べる図でも、値が変わった月へ向かう線が破線になっている");
});

check("図の部品(8): 見張りの外だった是正（横軸の間引き・目盛りの延長・右端の月・左のラベル・帯の目盛り）", () => {
  /* 2026-09-23 検証: 次の是正は、戻しても 1 件も落ちなかった。1 つずつ見る */
  // (a) 系列を縦に並べる図の横軸: 29 か月を 940px に並べると 3 か月おき＋最後。27 と 28 がくっつく形
  ctx.__M29 = Array.from({ length: 29 }, (_, i) => (i + 1) + "ヶ月");
  const ln = run('svgStackLanes({ w: 940, months: __M29, lanes: [{ label: "応募", type: "line", color: "blue", pts: __M29.map((_, i) => ({ v: i })) }] })');
  ok(!overlaps(ln).length, "(a) 帯を縦に積む図の横軸の最後の2つが重なる: " + overlaps(ln).join(" / "));
  // (b) 折れ線の横軸の2段目（n=）。1段目は短く収まるが2段目だけがぶつかる形（20 点を 660px に）
  ctx.__X20 = Array.from({ length: 20 }, (_, i) => String(i + 1));
  const sub = run('svgLine({ x: __X20, xSub: __X20.map(() => "n=1000"), series: [{ pts: __X20.map((_, i) => ({ v: i })) }] })');
  ok(!overlaps(sub).length, "(b) 折れ線の横軸の2段目（n=）が重なる: " + overlaps(sub).join(" / "));
  // (c) 左右に伸びる横棒（diverging）も、最大値を覆うところまで目盛りを足す
  const dv = run('svgBarH({ diverging: true, fmt: F.d1, rows: [{ label: "a", v: -0.3 }, { label: "b", v: 0.9 }] })');
  // 目盛りの文字だけを見る（棒の先の値「0.9」も同じ形なので、下端の目盛りの段に絞る）
  const dt = textBoxes(dv).filter((q) => /^-?\d+\.\d$/.test(q.s) && q.y1 > vbH(dv) - 12).map((q) => +q.s);
  ok(Math.max(...dt) >= 0.9, "(c) 左右に伸びる横棒の目盛りが " + Math.max(...dt) + " で止まり、0.9 の棒が先へ伸びる");
  // (d) 時間軸の図の右端の月（線は描くが文字は SVG の外に出るので出さない）
  const tl = run('svgTimeline({ w: 940, lanes: [{ label: "a", marks: [{ d: "2025-01-01" }, { d: "2025-11-17" }] }] })');
  ok(!outside(tl).length, "(d) 時間軸の図で SVG の外に出る文字: " + outside(tl).join(" / "));
  ok((tl.match(/<line class="gridline"/g) || []).length > textBoxes(tl).filter((q) => /^\d{4}-\d\d$/.test(q.s)).length,
    "(d) この入力では右端の月の線だけを描く形のはず（見張りの前提が崩れている）");
  // (e) 充足率の面・ファネルの左のラベルも、欄より長ければ省略記号で切る
  ctx.__LB = "右側打ち切り（結果が確定していない直近の契約）の件数";
  const mx = run('svgMatrix({ padL: 150, cw: 120, cols: ["a"], rows: [{ label: __LB, cells: [{ v: .5 }] }] })');
  const fn = run('svgFunnel({ steps: [{ label: "応募（媒体と紹介の合計）", v: 10 }, { label: "面接", v: 4 }] })');
  for (const [k, v] of Object.entries({ 充足率: mx, ファネル: fn })) {
    const lb = textBoxes(v).filter((q) => q.s.includes("…"));
    ok(lb.length === 1 && lb[0].x0 >= -0.5, "(e) " + k + ": 長いラベルを切っていない、または左端の外に出る");
    ok(/data-full="/.test(v), "(e) " + k + ": 切ったラベルの全文が残っていない");
  }
  // (f) 棒の帯の目盛りは、右端の棒（X(n−1) ± 棒の半幅）から離して置く。前は棒の右端から 0.5px だった
  const br = run('svgStackLanes({ w: 940, months: ["1","2","3"], lanes: [{ label: "接触", type: "bars", color: "gray", fillLabel: "接触", outLabel: "MTG", pts: [{ fill: 1 }, { fill: 2 }, { out: 3, fill: 1 }] }] })');
  const bx = Math.max(...[...br.matchAll(/<rect x="([\d.]+)" y="[\d.]+" width="([\d.]+)"/g)].map((m) => +m[1] + +m[2]));
  const bt = textBoxes(br).find((q) => q.s === "3" && q.y1 < 40);   // 月の軸の「3」ではなく帯の上端の目盛り
  ok(bt && bt.x0 - bx >= 4, "(f) 棒の帯の目盛り「3」が右端の棒から " + (bt ? (bt.x0 - bx).toFixed(1) : "?") + "px（4px 未満）");
});

check("図の部品(9): 横にスクロールしても左のラベルが残り、「値が無い」の見せ方を凡例で説明する", () => {
  /* 🔴 2026-09-23 検証: 400px で帯を縦に積む図（約 2.6 画面ぶん）を右へ動かすと、左のラベルも流れて
     どの帯の点か分からなかった。ラベルだけの SVG を重ね、CSS の position:sticky で左に残す。
     ここでは重ねる SVG が「スクロールしていないときに下のラベルとぴったり重なる」形かを数で見る
     （貼り付く動きそのものはブラウザでしか見られない） */
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/\.stickwrap > svg\.sticklab\{[^}]*position:sticky; left:0;/.test(css), "左のラベルを貼り付ける CSS（sticky）が無い");
  ok(/\.sticklab text\{ paint-order:stroke; stroke:var\(--panel\);/.test(css), "重ねるラベルに縁取りが無い（線や点の上で読めない）");
  // 3 つ目は欄（上限 240px）に入らない長さにして、切ったラベル（data-full 付き）も重ねる側に通す
  ctx.__LL9 = ["応募", "定期NPS", "接触（MTG・60秒超の通話）とそれ以外の連絡をすべて足した回数"];
  const lanes = run(`svgStackLanes({ w: 940, months: ["1ヶ月","2ヶ月","3ヶ月"], lanes: [
    { label: __LL9[0], type: "line", color: "blue", pts: [{ v: 1 }, { v: null }, { v: 3 }] },
    { label: __LL9[1], type: "dots", pts: [{ v: 3, round: 1 }, { v: 9, round: 2 }, { v: null }] },
    { label: __LL9[2], type: "bars", color: "gray", fillLabel: "接触", outLabel: "MTG", pts: [{ fill: 1 }, { v: 0, fill: 0 }, { fill: 2 }] } ] })`);
  const tl = run('svgTimeline({ w: 940, lanes: [{ label: "契約A", marks: [{ d: "2025-01-01" }] }, { label: "契約B", marks: [{ d: "2025-06-01" }] }] })');
  for (const [k, v] of Object.entries({ 帯を縦に積む図: lanes, 時間軸の図: tl })) {
    const wrap = v.match(/<div class="stickwrap" style="--fw:(\d+)px;--lw:([\d.]+)%">/);
    const lab = v.match(/<svg [^>]*class="sticklab" viewBox="0 0 ([\d.]+) ([\d.]+)" width="[\d.]+" height="[\d.]+" aria-hidden="true"/);
    ok(wrap && lab, k + ": 左のラベルを貼り付ける形（stickwrap / sticklab）になっていない");
    const padL = +lab[1], labH = +lab[2];
    ok(Math.abs(+wrap[2] - padL / +wrap[1] * 100) < 1e-3, k + ": 重ねる SVG の幅（--lw）がラベル欄の幅と合わない（縮尺がずれる）");
    // 重ねるラベルは、下の図のラベルと同じ文字・同じ位置（スクロールしていないときに二重に見えない）
    const main = [...v.matchAll(/<text class="axl" x="([\d.]+)" y="([\d.]+)" text-anchor="end"[^>]*>([^<]*)/g)].map((m) => m.slice(1).join("|"));
    const dup = [...v.matchAll(/<text data-sticky="1" class="axl" x="([\d.]+)" y="([\d.]+)" text-anchor="end"[^>]*>([^<]*)/g)].map((m) => m.slice(1).join("|"));
    ok(dup.length >= 2 && dup.every((d) => main.includes(d)), k + ": 重ねるラベルが下の図のラベルと位置・文字で一致しない");
    ok(!/data-sticky="1"[^>]*data-full=/.test(v), k + ": 重ねるラベル（読み上げから外した複製）に data-full が残っている。全文の一覧の元は下の図のラベルだけにする");
    // 重ねる SVG は下の軸（月・年月）を覆わない。地の色の四角は敷かない（400px では見えている幅の 2/3 を隠した）
    const plot = textBoxes(v).filter((q) => q.x0 > padL - 20);
    ok(plot.filter((q) => q.y1 > labH + 0.5).length > 0 && plot.every((q) => q.y1 <= labH + 0.5 || q.y0 >= labH - 0.5),
      k + ": 重ねる SVG の高さ（" + labH + "）が下の軸の文字にかかる");
    const inLab = plot.filter((q) => q.y0 < labH);   // 重ねる SVG の高さの中にある、図の中の文字
    const labSvg = v.slice(v.indexOf('class="sticklab"'), v.indexOf("</svg>", v.indexOf('class="sticklab"')));
    ok(!/<rect /.test(labSvg), k + ": 重ねる SVG に地の四角があり、スクロールした先の図（" +
      inLab.map((q) => q.s).slice(0, 3).join(",") + " など）を隠す");
  }
  // 「値が無い」の見せ方を凡例で説明する（折れ線は線の切れ目・棒は灰色の短い線）
  const fl = run('fig("推移", "", svgLine({ x: ["1","2","3"], series: [{ pts: [{ v: 1 }, { v: null }, { v: 3 }] }] }), lg("line", "blue", "応募"))');
  ok(fl.includes("線が途切れているところは、その月の値が無いところです"), "折れ線の切れ目（値が無い月）を凡例で説明していない");
  const f0 = run('fig("推移", "", svgLine({ x: ["1","2"], series: [{ pts: [{ v: 1 }, { v: 3 }] }] }))');
  ok(!f0.includes("線が途切れている"), "切れ目の無い折れ線にまで切れ目の説明を出している");
  const fc = run('fig("柱", "", svgColStack({ x: ["a","b","c"], series: [{ label: "s", color: "red", vals: [{ v: 1 }, { v: null }, { v: 2 }] }] }))');
  ok(fc.includes("灰色の短い線＝その月の記録が無い"), "積み上げ縦棒の灰色の印（値が無い月）を凡例で説明していない");
  const fs2 = run('fig("帯", "", svgStackLanes({ w: 940, months: ["1","2","3"], lanes: [{ label: "接触", type: "bars", color: "gray", fillLabel: "接触", outLabel: "MTG", pts: [{ fill: 1 }, { v: 0, fill: 0 }, { fill: 2 }] }, { label: "応募", type: "line", color: "blue", pts: [{ v: 1 }, { v: null }, { v: 3 }] }] }))');
  ok(fs2.includes("灰色の短い線＝その月の記録が無い") && fs2.includes("線が途切れているところは"),
    "帯を縦に積む図の「値が無い」の印（棒の帯の灰色の線・線の帯の切れ目）を凡例で説明していない");
});

/* ================================================================ 第2弾: 文言と凡例（2026-09-23 デプロイ後の実機確認） */
/* 描いた HTML から文字だけを取り出す（タグ・SVG を落とす） */
/* 数字と単位を折れない塊にした <span class="nw">（fig() の keepNum）は、画面では字の間に何も挟まない。
   ほかのタグと同じく空白に置き換えると「同じ 3ヶ月 目でも」と、画面に無い空白で文が割れるので、外すだけにする */
/* 図の数値の一覧（fig の figSay。段2 M-11: ul.sr と「数字で読む」の畳み）は SVG の吹き出し（title）をそのまま並べたもので、
   SVG と同じく本文の文ではないので落とす（担当期間「（60日）」のような行ごとの値が、文言の見張りに掛からないように） */
const textOf = (h) => String(h).replace(/<svg[\s\S]*?<\/svg>/g, " ")
  .replace(/<ul class="sr">[\s\S]*?<\/ul>/g, " ").replace(/<details class="fold figsay">[\s\S]*?<\/details>/g, " ")
  .replace(/<span class="nw">([^<]*)<\/span>/g, "$1").replace(/<[^>]+>/g, " ");

check("英語: 成果とリスク・定義と検証・電話に英語の用語を出さない", () => {
  // 電話の reach.note は routes.rs build_phone の文そのもの（直した後の文）
  ctx.__PH = { meta: { n_active: 604, threshold_sec: 60, today: "2026-09-18", not_counted: "" },
    reach: { no_call: 61, no_contact: 81, no_call_rate: 10.1, no_contact_rate: 13.4, option_rows_excluded: 0,
      note: "1本の通話が複数の取引に結び付いていることがあります。同じ通話を取引ごとに数えるので、行数は実際の通話の本数より多くなります" },
    days_since: null, transcript: { rate: 1, n: 1, rows: 100, note: "" },
    monthly: [{ month: "2026-08", calls: 10, contacts: 5 }, { month: "2026-09", calls: 8, contacts: 4 }],
    silent: { n: 0, rule: "", rows: [] } };
  for (const [name, code] of [["成果とリスク", "renderOutcome(__OUT)"], ["定義と検証", "renderDefs()"],
                              ["電話", "renderPhone(__PH)"]]) {
    const t = textOf(run(code));
    const hit = t.match(/StratifiedKFold|AUC|churn|Call は|Deal に|多対多/);
    ok(!hit, name + " に英語の用語「" + (hit && hit[0]) + "」が残っている");
  }
});

/* 担当者ごとの案件の最小の応答。行は deals.json の形（担当 2 名・名札つき 1 件）。件数は見張りのために置いた値 */
ctx.__BD = { meta: { today: "2026-09-18", n_active: 3, order_rule: "並びのきまりXYZ", flag_counts: [],
    not_counted: "※ 予測ではありません" },
  rows: [
    { deal_id: "d1", name: "案件1", consultant: "田中", flags: ["NPSが4以下"], focus: true, days_left: 10 },
    { deal_id: "d2", name: "案件2", consultant: "田中", flags: [], focus: false, days_left: 200 },
    { deal_id: "d3", name: "案件3", consultant: "佐藤", flags: [], focus: false, days_left: 20 }] };

check("byowner: 担当を選ぶ前は絞り込みの欄と「N 件中 N 件を表示」を出さず、名前を押して選べる", () => {
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "" };');
  try {
    const h = run("renderBoard(__BD)");
    ok(h.includes('id="bf-consultant"'), "担当の選択欄が無い");
    for (const id of ["bf-flag", "bf-expiry", "bf-q", "bf-clear"])
      ok(!h.includes('id="' + id + '"'), "担当を選ぶ前に " + id + " が出ている（件数の表しか無いのに絞り込めるように見える）");
    ok(!/件中 .* 件<\/b>を表示/.test(h) && !h.includes("board-count"), "担当を選ぶ前に「N 件中 N 件を表示」が出ている");
    const tbl = h.split('<table id="owner-tbl"')[1];
    ok(tbl !== undefined, "持ち件数の表に id が無い（名前を押す仕掛けを付けられない）");
    ok(/<a href="#" class="drill" data-c="田中"/.test(tbl) && /<a href="#" class="drill" data-c="佐藤"/.test(tbl),
      "持ち件数の表の担当者名が押せない（team の表と同じ a.drill でない）");
    // 名前を押したら担当で絞る。wireBoard が付けた onclick を呼ぶ
    const a = { dataset: { c: "佐藤" }, onclick: null };
    const qsa = ctx.document.querySelectorAll;
    ctx.document.querySelectorAll = (sel) => (sel === "#owner-tbl a.drill" ? [a] : []);
    try {
      run("boardCache = __BD; wireBoard()");
      ok(typeof a.onclick === "function", "持ち件数の表の名前に onclick が付かない");
      a.onclick({ preventDefault() {} });
      ok(run("boardFilter.consultant") === "佐藤", "名前を押しても担当で絞られない: " + run("boardFilter.consultant"));
    } finally { ctx.document.querySelectorAll = qsa; }
    // 選んだ後は絞り込みの欄と件数の行が出る。並びの決まりは表の側に1回だけ
    const h2 = run("renderBoard(__BD)");
    ok(h2.includes('id="bf-flag"') && h2.includes("board-count"), "担当を選んだ後に絞り込みの欄・件数の行が無い");
    ok((h2.match(/並びのきまりXYZ/g) || []).length === 1, "担当を選んだ後の画面に並びの決まりが1回だけ出ていない");
  } finally {
    run('boardFilter = { consultant: "", flag: "", expiry: "", q: "" }; cur = { menu: "deal", view: "today" };');
  }
});

check("board: 並びの決まり（order_rule）を1回だけ出す（名札の図の凡例に重ねない）", () => {
  run('cur = { menu: "deal", view: "board" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "" };');
  const h = run("renderBoard(__BD)");
  run('cur = { menu: "deal", view: "today" };');
  ok((h.match(/並びのきまりXYZ/g) || []).length === 1, "order_rule が " + (h.match(/並びのきまりXYZ/g) || []).length + " 回出ている");
});

/* 法人番号で見るの cpa3（routes.rs build_customer の形）。fixture の h00f31a786bac（2026-09-23 実測）では
   稼働中 2 件のうち censored は 1 件だけで、総額が出ている稼働中の 1 件（censored=false, 720000）が藍で描かれていた。
   その形を写し、中央値の位置を数値で確かめられるよう、帯の中央値を確定の契約の総額と同じ 900000 に置いた */
ctx.__C3 = { meta: { found: true }, cpa3: [
  { deal_id: "a", name: "確定の契約", total: 900000, monthly: 600000, band: null, band_median: null,
    syoudaku: 2, censored: false, active: false },
  { deal_id: "b", name: "稼働中の契約", total: 720000, monthly: 480000, band: "終盤（75〜100%）",
    band_median: 900000, syoudaku: 10, censored: false, active: true },
  { deal_id: "c", name: "打ち切り", total: null, monthly: null, band: "契約の前半（0〜50%）",
    band_median: 750000, syoudaku: null, censored: true, active: true }] };
const cpa3Fig = () => run('custBlocks(__C3, new Set(["cpa3"]))')
  .split("<figcaption>同じ契約でも")[1].split("</figure>")[0];

check("houjin 採用単価: 稼働中の契約は中空・破線の棒（薄い塗りにしない）、凡例も同じ描き方", () => {
  const g = cpa3Fig();
  // 中まで塗った棒（svgBarH の塗りの形）は確定の 1 本だけ
  const solid = [...g.matchAll(/<rect x="[0-9.]+" y="[0-9.]+" width="[0-9.]+" height="[0-9.]+" rx="2" style="fill:([^"]+)" opacity="([.0-9]+)">/g)]
    .map((m) => ({ fill: m[1], op: m[2] }));
  ok(solid.length === 1, "塗った棒が 1 本でない: " + solid.length + "（稼働中の契約を塗りで描いている）");
  ok(solid[0].fill === "var(--ai)" && solid[0].op === ".82", "確定の契約が藍・.82 でない: " + JSON.stringify(solid[0]));
  ok(!/opacity="\.34"/.test(g.split('<div class="figlegend">')[0]), "薄く塗った棒（.34）が残っている（濃さだけで未確定を伝えている）");
  // 稼働中（censored=false, active=true）の棒は中空・破線
  const open = [...g.matchAll(/<rect [^>]*style="fill:var\(--panel\);stroke:([^"]+)"[^>]*stroke-dasharray="[^"]+" data-open="1">/g)];
  ok(open.length === 1, "中空・破線の棒が 1 本でない: " + open.length);
  ok(open[0][1] === "var(--ai)", "未確定の棒の枠が藍でない: " + open[0][1]);
  const leg = g.split('<div class="figlegend">')[1];
  const a = legendSwatch(leg, "総額 ÷ 採用数（確定）");
  ok(a && a.fill === solid[0].fill && a.op === solid[0].op, "確定の凡例の粒 " + JSON.stringify(a) + " が棒と違う");
  const i = leg.indexOf("</svg>同（未確定・稼働中");
  const sw = i >= 0 ? leg.slice(leg.lastIndexOf("<svg", i), i) : "";
  ok(/style="fill:var\(--panel\);stroke:var\(--ai\)"[^>]*stroke-dasharray/.test(sw), "未確定の凡例の粒が中空・破線の藍でない: " + sw);
});

check("houjin 採用単価: 破線は未確定だけに使い、月割りは墨の縦の線（凡例と同じ色・位置は軸の尺度どおり）", () => {
  const g = cpa3Fig();
  const fig = g.split('<div class="figlegend">')[0], leg = g.split('<div class="figlegend">')[1];
  // 図の中の破線は、中空の棒（data-open）と中央値の印（data-mark="band…"）だけ
  const dashed = [...fig.matchAll(/<(?:rect|line|path|circle) [^>]*stroke-dasharray[^>]*>/g)].map((m) => m[0]);
  const other = dashed.filter((d) => !/data-open="1"|data-mark="band/.test(d));
  ok(!other.length, "未確定でないものに破線を使っている（破線が2つの意味を持つ）: " + other.join(" "));
  const ms = [...fig.matchAll(/<line x1="([0-9.]+)" y1="[0-9.]+" x2="([0-9.]+)" y2="[0-9.]+" style="stroke:([^"]+)" stroke-width="[0-9.]+" data-mark="monthly">/g)];
  ok(ms.length === 2, "月割りの線が 2 本でない: " + ms.length + "（月割りが出ている契約は 2 件）");
  const t = leg.indexOf("</svg>月割り");
  const sw = t >= 0 ? leg.slice(leg.lastIndexOf("<svg", t), t) : "";
  const lc = (sw.match(/style="stroke:([^";]+)"/) || [])[1];
  for (const m of ms) ok(m[3] === lc, "月割りの線の色 " + m[3] + " が凡例の粒 " + lc + " と違う（行の色で描いている）");
  ok(lc === "var(--ink)", "月割りの凡例が墨でない: " + lc);
  // 確定の契約: 総額 900000 の棒の右端に対し、月割り 600000 の線は 2/3 の位置
  const r0 = fig.match(/<rect x="([0-9.]+)" y="[0-9.]+" width="([0-9.]+)" height="[0-9.]+" rx="2" style="fill:var\(--ai\)"/);
  const want = +r0[1] + +r0[2] * 600000 / 900000;
  ok(Math.abs(+ms[0][1] - want) < 0.6, "確定の契約の月割りの線 x " + ms[0][1] + " が 600000 の位置 " + want.toFixed(1) + " と合わない");
});

check("houjin 採用単価: 進捗帯の中央値は中空・破線の丸で、位置が軸の尺度に合う（軸の外は見える文字でも書く）", () => {
  const g = cpa3Fig();
  const marks = [...g.matchAll(/<circle cx="([0-9.]+)" cy="([0-9.]+)" r="4.5" data-mark="band" style="([^"]+)"([^>]*)>/g)];
  ok(marks.length === 1, "中央値の印が 1 つでない: " + marks.length + "（総額が出ていて帯の中央値がある棒は 1 本）");
  ok(/fill:var\(--panel\)/.test(marks[0][3]) && /stroke-dasharray/.test(marks[0][4]),
    "稼働中どうしの中央値（未確定）を中まで塗った丸で描いている: " + marks[0][0]);
  // 中央値 900000 は「確定の契約」の総額と同じ。印の x はその棒の右端と一致するはず（数値で確かめる）
  const r0 = g.match(/<rect x="([0-9.]+)" y="([0-9.]+)" width="([0-9.]+)" height="([0-9.]+)" rx="2" style="fill:var\(--ai\)"/);
  const right = +r0[1] + +r0[3];
  ok(Math.abs(+marks[0][1] - right) < 0.6, "印の x " + marks[0][1] + " が 900000 の棒の右端 " + right.toFixed(1) + " と合わない");
  // y は2本目（稼働中の契約）の棒の中心
  const r1 = g.match(/<rect x="[0-9.]+" y="([0-9.]+)" width="[0-9.]+" height="([0-9.]+)" rx="2" style="fill:var\(--panel\)[^"]*"[^>]*data-open="1"/);
  ok(r1 && Math.abs(+marks[0][2] - (+r1[1] + +r1[2] / 2)) < 0.6, "印の y " + marks[0][2] + " が稼働中の棒の中心と合わない");
  const leg = g.split('<div class="figlegend">')[1];
  const t = leg.indexOf("</svg>同じ進捗帯の中央値");
  const sw = t >= 0 ? leg.slice(leg.lastIndexOf("<svg", t), t) : "";
  ok(/<circle [^>]*fill:var\(--panel\)[^>]*stroke-dasharray/.test(sw), "中央値の凡例が中空・破線の丸でない: " + sw);
  ok(!g.includes("軸の外"), "軸の中に収まっているのに「軸の外」と書いている");
  // 中央値が軸の最大（900000）を超えるとき: 右端に三角を置き、行の注記（見える文字）にも書く
  const c = JSON.parse(JSON.stringify(ctx.__C3));
  c.cpa3[1].band_median = 2000000;
  ctx.__C3o = c;
  const go = run('custBlocks(__C3o, new Set(["cpa3"]))').split("<figcaption>同じ契約でも")[1].split("</figure>")[0];
  ok(!/data-mark="band"/.test(go), "軸の外の中央値を、軸の中と同じ丸で描いている");
  const tri = go.match(/<path d="M([0-9.]+) [0-9.]+L([0-9.]+) [0-9.]+L[^"]+" data-mark="band-out" style="fill:var\(--panel\)[^"]*"[^>]*stroke-dasharray/);
  ok(tri, "軸の外の中央値の印（中空・破線の三角）が無い");
  // 軸の端は、最大値 900000 を覆うまで1段足した最後の目盛り（svgBarH の coverTicks）。三角の先はその目盛りの線の上。
  // 900000 の棒の右端より右にあること（軸の中に描いていないこと）も見る
  const e = go.match(/<rect x="([0-9.]+)" y="[0-9.]+" width="([0-9.]+)" height="[0-9.]+" rx="2" style="fill:var\(--ai\)"/);
  const gxe = Math.max(...[...go.matchAll(/<line class="gridline" x1="([0-9.]+)"/g)].map((m) => +m[1]));
  ok(Math.abs(+tri[2] - gxe) < 0.6, "三角の先 " + tri[2] + " が軸の右端（最後の目盛り " + gxe.toFixed(1) + "）にない");
  ok(+tri[2] >= +e[1] + +e[2] - 0.6, "三角の先 " + tri[2] + " が最大の棒の右端 " + (+e[1] + +e[2]).toFixed(1) + " より左にある");
  ok(/<text class="ax"[^>]*>[^<]*中央値 200万（軸の外）/.test(go), "「軸の外」が見える文字（行の注記）に無い（title だけだとスマホで見えない）");
});

check("定義と検証: 文の表は折り返す（「なぜ」が右端で切れない）", () => {
  const h = run("renderDefs()");
  // 見出しの直後の表だけを見る（後ろの「3つのいつ」の表の prose に引っ張られない）
  const t = h.split("この画面が守っていること")[1].split("</table>")[0];
  ok(/<table class="prose">/.test(t), "「この画面が守っていること」の表が折り返す表（prose）になっていない");
  ok(/table\.prose td\{[^}]*white-space:normal/.test(html), "table.prose td に white-space:normal が無い");
  ok(/\.scroll > table\.prose\{[^}]*min-width:0/.test(html), "prose の表に min-width:max-content が残る（折り返さずに伸びる）");
});

/* 100%帯の区間の塗りと濃さを、描いた SVG から読む（svgStack の出力の形） */
function stackRects(svg) {
  return [...String(svg).matchAll(/<rect x="[0-9.]+" y="6" width="[0-9.]+" height="[0-9.]+" rx="2" style="fill:([^"]+)" opacity="([.0-9]+)"><title>([^:]+):/g)]
    .map((m) => ({ fill: m[1], op: m[2], label: m[3] }));
}
/* 凡例のラベル直前の粒の塗りと濃さ。opacity が無ければ "1" */
function legendSwatch(leg, label) {
  const i = String(leg).indexOf("</svg>" + label);
  if (i < 0) return null;
  const sv = leg.slice(leg.lastIndexOf("<svg", i), i);
  const m = sv.match(/style="fill:([^";]+)[^"]*"(?: opacity="([.0-9]+)")?/);
  return m ? { fill: m[1], op: m[2] || "1" } : null;
}

check("凡例: 100%帯の凡例の粒は帯と同じ塗り・濃さ（いま見るべき顧客の MTG 記録。focus.json の件数）", () => {
  ctx.__SP = [
    { label: "録画もメールもある", v: 257, color: "var(--midori)" },
    { label: "録画だけ（事実）", v: 17, color: "var(--ai)" },
    { label: "メールだけ（推定）", v: 215, color: "var(--ki)", faint: true },
    { label: "どちらも無い", v: 115, color: "var(--ghost)", faint: true }];
  const rs = stackRects(run("svgStack({ parts: __SP, w: 680 })")), leg = run("stackLegend(__SP)");
  ok(rs.length === 4, "帯の区間が取れない: " + rs.length);
  for (const r of rs) {
    const s = legendSwatch(leg, r.label);
    ok(s && s.fill === r.fill && s.op === r.op, r.label + " の凡例 " + JSON.stringify(s) + " が帯 " + JSON.stringify(r) + " と違う");
  }
});

check("凡例: 目標の帯の「承諾数が空」は斜線（値が無い）、「未記入」は薄い塗りで見分け、未確定の帯より強く描かない", () => {
  const h = run("renderOutcome(__OUT)");
  const g = h.split("<figcaption>達成率 ＝ 承諾数 ÷ 採用目標数")[1].split("</figure>")[0];
  const fig = g.split('<div class="figlegend">')[0], leg = g.split('<div class="figlegend">')[1];
  // 承諾数が空の帯は斜線の模様（同じ図の中に模様の定義がある）
  const hb = fig.match(/<rect [^>]*style="fill:url\(#([a-z-]+)\);stroke:var\(--ghost\)"[^>]*data-fill="hatch"><title>承諾数が空/);
  ok(hb, "承諾数が空の帯が斜線になっていない");
  ok(new RegExp('<pattern id="' + hb[1] + '"').test(fig), "斜線の模様の定義が図の中に無い");
  // 中まで塗った区間は 100%以上（確定の達成）と、薄い塗りの未記入だけ。値が無い帯を濃く塗らない
  const rs = stackRects(fig);
  ok(rs.map((r) => r.label).join("|") === "100%以上|未記入（目標が無い）",
    "中まで塗った帯が 100%以上 と 未記入 だけでない: " + rs.map((r) => r.label + "=" + r.op).join(", "));
  const rb = rs.find((r) => r.label.startsWith("未記入"));
  ok(rb.op === ".3", "未記入が薄い塗り（.3）でない: " + rb.op);
  // 凡例: 承諾数が空は斜線の粒、未記入は帯と同じ濃さ
  const i = leg.indexOf("</svg>承諾数が空（目標はある）");
  const sw = i >= 0 ? leg.slice(leg.lastIndexOf("<svg", i), i) : "";
  ok(/<path d="M1\.5 9\.5L7\.5 3\.5/.test(sw) && /stroke:var\(--ghost\)/.test(sw), "承諾数が空の凡例が斜線の粒でない: " + sw);
  const b = legendSwatch(leg, "未記入（目標が無い）");
  ok(b && b.fill === rb.fill && b.op === rb.op, "未記入の凡例 " + JSON.stringify(b) + " が帯 " + JSON.stringify(rb) + " と違う");
  // 帯の中の文字（6件は細いので出ないが、出るときに白文字にしない）: 斜線の上は墨系
  const big = JSON.parse(JSON.stringify(ctx.__OUT));
  big.goal_act.bands[4].n = 300;
  ctx.__OUTh = big;
  const g2 = run("renderOutcome(__OUTh)").split("<figcaption>達成率 ＝ 承諾数 ÷ 採用目標数")[1].split("</figure>")[0];
  const tx = g2.match(/data-fill="hatch"><title>[^<]*<\/title><\/rect><text [^>]*style="fill:([^;]+);/);
  ok(tx && tx[1] === "var(--ink-2)", "斜線の帯の上の文字が白（var(--panel)）のまま: " + (tx && tx[1]));
});

check("凡例: MTGのリスク（判定不可 / 未判定）と立ち上がり（終盤 / 出せない）の粒が見分けられる", () => {
  // mtg-quality.json の risk_dist（2026-09-23 実測）
  ctx.__MQ = { meta: { n_mtg: 3124, today: "2026-09-18", not_counted: "" }, linked: { rate: 90, n: 1 },
    filled: [{ field: "やること", n: 10, rate: 10 }], filled_note: "", hosts: [], monthly: [],
    risk_dist: [{ label: "高", n: 34 }, { label: "中", n: 115 }, { label: "低", n: 300 },
                { label: "判定不可", n: 27 }, { label: "（未判定）", n: 2648 }] };
  const q = run("renderMtgQ(__MQ)");
  const x = legendSwatch(q, "判定不可"), y = legendSwatch(q, "（未判定）");
  ok(x && y && (x.fill !== y.fill || x.op !== y.op), "判定不可と未判定の凡例が同じ " + JSON.stringify(x));
  // rampup.json の phase.rows（2026-09-23 実測）
  const r = JSON.parse(JSON.stringify(ctx.__RU));
  r.phase = { rule: "契約長に対する割合", rows: [{ label: "序盤", n: 263 }, { label: "中盤", n: 172 },
    { label: "終盤", n: 153 }, { label: "満了超過", n: 15 }, { label: "出せない", n: 1 }] };
  ctx.__RU2 = r;
  const rh = run("renderRampup(__RU2)");
  const e = legendSwatch(rh, "終盤"), f = legendSwatch(rh, "出せない");
  ok(e && f && (e.fill !== f.fill || e.op !== f.op), "終盤と出せないの凡例が同じ " + JSON.stringify(e));
});

/* 成果とリスクで最優先が1件ある形（散布図と表が出る）。order_note は routes.rs risk() の文 */
ctx.__OUT2 = JSON.parse(JSON.stringify(ctx.__OUT));
ctx.__OUT2.risk.top = [{ name: "x", amount: 100, days_to_expiry: 10, never_after_start: true, ax3w: "", n_contact: 0, stage: "" }];
ctx.__OUT2.risk.ax3 = { "赤": 1, "白": 0, "未測定": 0, rule: "" };
ctx.__OUT2.risk.ax4 = { "赤": 1, "白": 0, "未測定": 0, rule: "" };
ctx.__OUT2.risk.order_note = "この並びは機械が付けた順（金額順）";

check("凡例: 散布図の点と箱ひげの「四分位」は図と同じ濃さ", () => {
  const sc = run('svgScatter({ pts: [{ x: 1, y: 1, color: C.hi }, { x: 2, y: 2, color: C.hi }], w: 300 })');
  const pop = (sc.match(/<circle [^>]*style="fill:var\(--hi\)" opacity="([.0-9]+)"/) || [])[1];
  ok(pop && legendSwatch(run('lg("dot", C.hi, "x", PT_OP)'), "x").op === pop, "散布図の点の濃さ " + pop + " と PT_OP が違う");
  const bx = run('svgBoxH({ rows: [{ label: "a", med: 2, q1: 1, q3: 3, min: 0, max: 4, n: 40, color: C.ai }] })');
  const bop = (bx.match(/<rect [^>]*style="fill:var\(--ai\)" opacity="([.0-9]+)"/) || [])[1];
  ok(bop && legendSwatch(run('lg("quart", C.ai, "四分位")'), "四分位").op === bop, "箱ひげの箱の濃さ " + bop + " と「四分位」の粒が違う");
  // 画面の凡例がその粒を使っていること
  ok(!/lg\("box", C\.ai, "四分位"\)/.test(html), "「四分位」を不透明の四角（box）で出している呼び出しが残っている");
  const g = run("renderOutcome(__OUT2)").split("<figcaption>2軸とも赤の")[1].split("</figure>")[0];
  const s = legendSwatch(g, "契約後に一度も接触していない");
  const p = (g.match(/<circle [^>]*style="fill:var\(--hi\)" opacity="([.0-9]+)"/) || [])[1];
  ok(s && p && s.op === p, "散布図の凡例 " + JSON.stringify(s) + " と点の濃さ " + p + " が違う");
});

check("読み方の枠: 末尾の見出しを中身に合わせる（予測ではありません…に「数えていないもの」と付けない）", () => {
  const f = run('foot({ not_counted: "※ 予測ではありません", today: "2026-09-18", n_active: 604 })');
  ok(!f.includes("この画面で数えていないもの"), "断り書きに「この画面で数えていないもの」の見出しが付いている");
  ok(f.includes("読むときの注意"), "断り書きの見出しが無い");
  const d = run('foot({ today: "2026-09-18" })');
  ok(!d.includes("件数"), "件数が無いのに見出しに「件数」と書いている: " + d);
});

check("読み方の枠: 成果とリスクの並びの注記・立ち上がりの「同じ3ヶ月目でも」を2回出さない", () => {
  /* 段B（2026-09-29）で最優先の表を案件一覧の見方に移したので、並びの注記（表の頭）は成果と継続には 0 回。
     散布図の側に戻して 1 回に見せかけない（散布図には並びが無い） */
  const h = run("renderOutcome(__OUT2, 'rs-outcome')");
  ok((h.match(/この並びは機械が付けた順/g) || []).length === 0,
    "order_note が " + (h.match(/この並びは機械が付けた順/g) || []).length + " 回出ている（表は見方に移した）");
  const r = JSON.parse(JSON.stringify(ctx.__RU));
  // rampup.json の phase.rule（2026-09-23 実測）
  r.phase = { rule: "契約長に対する割合。< 0.34 序盤 / < 0.67 中盤 / <= 1.05 終盤 / 超 満了超過。同じ3ヶ月目でも、3ヶ月契約なら満了・12ヶ月契約なら序盤",
    rows: [{ label: "序盤", n: 1 }] };
  ctx.__RU3 = r;
  const t = textOf(run("renderRampup(__RU3)"));
  const n = (t.match(/同じ「?3ヶ月目」?でも/g) || []).length;
  ok(n === 1, "「同じ3ヶ月目でも」が " + n + " 回出ている");
});

check("顧客（前の法人番号で見る）: 末尾の「集計の基準日と件数」に件数を書く", () => {
  const h = run('foot({ today: "2026-09-18" }, false, houjinCounts([{ is_active: true }, { is_active: false }], new Set(["a"])))');
  ok(h.includes("集計の基準日と件数") && h.includes("この法人の取引 2 件（稼働中 1 件）"), "件数が無い: " + h);
  const body = html.split("function renderCustomer(D)")[1].split("\nfunction ")[0];
  /* ループ4: not_counted は「法人」の節の粒度の枠で出すので、末尾は said=true（基準日と件数だけ）。
     2026-09-29 顧客の1画面にしてから、明細の末尾は1つ（案件が選ばれていないときも法人の節の中で止め、末尾は共通） */
  ok((body.match(/foot\(D\.meta, true, houjinCounts\(all, ids\)\)/g) || []).length === 1,
    "renderCustomer の末尾が件数を渡していない");
});

check("KPI: 最終満了を折り返さない・電話の61件の色をそろえる・退職者の補足に別の話を混ぜない", () => {
  ok(/\.kpi\.is-date \.big\{[^}]*white-space:nowrap/.test(html), "日付の KPI に white-space:nowrap が無い");
  // 🔴 2026-09-23 統合後の実測（400px）: 担当名（h9821a39368fe、29px）が KPI の箱（中身 144px）から 263px まで
  // 伸び、ページ本体が 486px に広がっていた（86px のはみ出し）。箱は中身で広がらず、長い語は箱の幅で折り返す
  const kcss = html.slice(0, html.indexOf("</style>"));
  ok(/\.kpi\{ min-width:0; \}/.test(kcss), "KPI の箱に min-width:0 が無い（長い名前で格子の列が広がり、ページ本体がはみ出す）");
  ok(/\.kpi \.lbl, \.kpi \.big, \.kpi \.fine\{ overflow-wrap:anywhere; \}/.test(kcss),
    "KPI の名前・値が箱の幅で折り返さない（400px で担当名が箱から 86px はみ出す）");
  const c = run('custBlocks({ meta: { found: true, houjin: "H" }, customer: { name: "法人", deals: 1, active: 1, sites: 1, ltv: 1, max_renewal_no: 0, last_expiration: "2027-02-28" } }, new Set(["head"]))');
  ok(/<div class="kpi is-date"><span class="lbl">最終満了/.test(c), "最終満了の KPI が日付の型（is-date）になっていない");
  const ph = run("renderPhone(__PH)");
  /* 札は押せる button（段2 S-2 の残り）。見るのは色（is-bad）で、前と同じ */
  const card = ph.match(/<(?:div|button type="button") class="kpi( is-[a-z]+)?"[^>]*><span class="lbl">電話が1本も無い/);
  ok(card && card[1] === " is-bad", "電話が1本も無いの KPI が赤でない: " + (card && card[1]));
  ok(/style="fill:var\(--hi\)"[^>]*><title>電話が1本も無い/.test(ph), "内訳の帯の「電話が1本も無い」が赤でない（KPI と色がそろわない）");
  const tm = run("renderTeam(teamOf(__D))");
  const k = tm.split('<span class="lbl">退職者のまま</span>')[1].split("</div>")[0];
  ok(!k.includes("割れ") && !k.includes("38"), "退職者のままの補足に担当の割れ（38件）が混ざっている: " + k);
  ok(tm.includes("担当が割れている稼働中の案件が 38 件"), "担当の割れ（38件）がどこにも出ていない（黙って消した）");
  // 🔴 2026-09-23 レビュー F2: owner_rule（routes.rs）と画面で「後に来る行を採る」を2回書いていた。
  // owner_rule は consultants.json の実物（routes.rs の文そのもの）を入れて数える
  const D2 = JSON.parse(JSON.stringify(ctx.__D));
  D2.owner_rule = "担当は consultant が正本です（hubspot_owner_id ではありません）。" +
    "取引ごとに、担当履歴のいちばん新しい行を採っています。" +
    "同じ日に複数行ある取引では、シートで後に来る行（＝追記順で新しい方）を採っています";
  ctx.__D2 = D2;
  /* 2026-09-29 組み替え: チームと担当の中では「担当の交代」にも「この一覧の決まりごと」があるので、担当者の表の決まりごとは
     「担当者 × 状態の決まりごと」 */
  const rule = textOf(run("renderTeam(teamOf(__D2))")).split("担当者 × 状態の決まりごと")[1] || "";
  const dup = (rule.match(/シートで後に来る行/g) || []).length;
  ok(dup === 1, "「担当者 × 状態の決まりごと」で採り方（シートで後に来る行）が " + dup + " 回出ている（F2）");
});

check("担当の交代: 交代のうち稼働中の件数を、全体の「稼働中 N 件」と同じ言葉で書かない", () => {
  const h = run("renderHandover(__HO)");
  ok(!/（稼働中 1 件）/.test(h) && !/うち稼働中の案件 1 件/.test(h), "交代のうち稼働中の件数を「稼働中 N 件」とだけ書いている");
  ok((h.match(/案件がいまも稼働中のもの 1 件/g) || []).length === 2, "KPI と末尾で何の件数かを書いていない");
});

check("いま見るべき顧客: LTV の注記で「拠点が2つ以上ある法人」を続けて2回書かない", () => {
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  // focus.json の shape（2026-09-23 実測）
  fo.shape = { ltv: { n: 517, median: 1, q1: 1, q3: 2, min: 0, max: 3, mean: 1 }, n_all: 1649, n_display: 517,
    display_label: "稼働中の取引を持つ法人", multi_site: 136,
    multi_site_note: "拠点が2つ以上ある法人。決裁は事業所単位なので、1本の線にまとめない" };
  ctx.__FO4 = fo;
  const t = textOf(run("renderFocus(__FO4)"));
  const n = (t.match(/拠点が2つ以上ある法人/g) || []).length;
  ok(n === 1, "「拠点が2つ以上ある法人」が " + n + " 回出ている");
  ok(t.includes("決裁は事業所単位なので"), "注記の後半まで消している");
});

check("いま見るべき顧客: 「法人 N」は本部アプローチ・法人番号で見ると同じ母数（houjin_population, F4）", () => {
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  // focus.json の shape（2026-09-23 実測。n_all は CS_顧客 の行数、n_houjin は houjin_population の main）
  fo.shape = { ltv: { n: 517, median: 1, q1: 1, q3: 2, min: 0, max: 3, mean: 1 }, n_all: 1649,
    n_houjin: 1646, n_houjin_option_only: 3, n_display: 517,
    display_label: "稼働中の取引を持つ法人", multi_site: 136, multi_site_note: "" };
  ctx.__FO5 = fo;
  const t = textOf(run("renderFocus(__FO5)"));
  ok(!t.includes("1,649"), "「法人 N」に CS_顧客 の行数（1,649）を出している（本部の 1,646 と合わない）");
  ok(/法人 1,646（オプション契約しか持たない 3 法人を除く）/.test(t), "「法人 1,646」と除いた法人の数が出ていない: " +
    (t.match(/法人 [^／]*/) || [""])[0]);
});

check("V12 の残り: 表の枠の端に、横の続きがある側だけ影を出す", () => {
  const h = run('scroll(table([{ t: "a" }], [[1]]), 400)');
  ok(/<div class="scroll-wrap"><div class="scroll"/.test(h), "枠が影を描く包み（scroll-wrap）に入っていない");
  ok(/\.scroll-wrap\.more-r::after\{ opacity:1; \}/.test(html), "右に続きがあるときの影（::after）を出す CSS が無い");
  // 🔴 左端の帯（::before）は出さない。S-12 で1列目を貼り付けてから、貼り付いた＝隠れていない列の上に「下に列が隠れている」印が
  //    乗っていた（2026-09-28 検証: scrollLeft=300 で td の左端の画素 (221,221,221)、同じ列の th は地のまま）。
  //    左に隠れている列は貼り付けた列の右端の影（more-l の box-shadow）で示す
  ok(!/\.scroll-wrap(\.more-l)?::before/.test(html), "枠の左端の帯（::before）が残っている（貼り付けた1列目の上に被る）");
  ok(/\.scroll-wrap\.more-l th:first-child, \.scroll-wrap\.more-l td:first-child\{\s*box-shadow:4px 0 8px -4px var\(--edge-shade\)/.test(html),
    "左に続きがあるとき、貼り付けた1列目の右端に影が無い");
  const cls = new Set();
  const inner = { scrollWidth: 1000, clientWidth: 400, scrollLeft: 0 };
  ctx.__W = { querySelector: () => inner,
    classList: { toggle: (c, on) => { if (on) cls.add(c); else cls.delete(c); } } };
  run("markScroll(__W)");
  ok(cls.has("more-r") && !cls.has("more-l"), "左端にいるのに右の影が無い／左の影がある: " + [...cls]);
  inner.scrollLeft = 600; run("markScroll(__W)");
  ok(!cls.has("more-r") && cls.has("more-l"), "右端まで動かしたのに右の影が残る: " + [...cls]);
  inner.scrollWidth = 400; inner.scrollLeft = 0; run("markScroll(__W)");
  ok(!cls.size, "はみ出していない表に影を出している: " + [...cls]);
  ok(/markScrollAll\(\);[^\n]*\n\}/.test(html.split("function wire(v)")[1] || ""), "描いた後（wire）に影を付けていない");
});

check("byowner: 担当を選ぶ前の持ち件数は、見えない絞り込み（名札・満了・案件名）で減らさない", () => {
  // 案件そのものの画面で名札・案件名を絞ってから byowner に来た形。選択欄は隠れている
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "", flag: "NPSが4以下", expiry: "", q: "案件1" };');
  try {
    const h = run("renderBoard(__BD)");
    const tbl = h.split('<table id="owner-tbl"')[1].split("</table>")[0];
    const cnt = (name) => ((tbl.match(new RegExp('data-c="' + name + '"[^>]*>' + name + "</a></td><td[^>]*>([0-9,]+)<")) || [])[1]);
    ok(cnt("田中") === "2" && cnt("佐藤") === "1",
      "持ち件数が隠れた絞り込みで減っている: 田中 " + cnt("田中") + " / 佐藤 " + cnt("佐藤") + "（稼働中の全件は 2 / 1）");
    /* ループ4: 見出しから「稼働中 N 件」を外した（頭の1行と重なる）。人数が隠れた絞り込みで減らないことを見る */
    ok(h.includes("担当者ごとの持ち件数（2 名）"), "見出しの人数が稼働中の全件の担当者数でない");
  } finally {
    run('boardFilter = { consultant: "", flag: "", expiry: "", q: "" }; cur = { menu: "deal", view: "today" };');
  }
});

check("byowner: 持ち件数の表で名前を押したら、描き直した後に担当の選択欄へフォーカスを移す", () => {
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "" };');
  const a = { dataset: { c: "佐藤" }, onclick: null };
  const qsa = ctx.document.querySelectorAll, qs = ctx.document.querySelector;
  let focused = null;
  const sel = { focus() { focused = "#bf-consultant"; } };
  ctx.document.querySelectorAll = (s) => (s === "#owner-tbl a.drill" ? [a] : []);
  ctx.document.querySelector = (s) => (s === "#bf-consultant" ? sel : null);
  try {
    run("boardCache = __BD; wireBoard()");
    a.onclick({ preventDefault() {} });
    ok(focused === "#bf-consultant", "名前を押した後のフォーカスの行き先が担当の選択欄でない（押したリンクは描き直しで消える）");
  } finally {
    ctx.document.querySelectorAll = qsa; ctx.document.querySelector = qs;
    run('boardFilter = { consultant: "", flag: "", expiry: "", q: "" }; cur = { menu: "deal", view: "today" };');
  }
});

/* 影の付け直しがイベント・差し込みにつながっているか。偽の枠 1 つ（右にはみ出している）を用意し、
   それぞれの経路で more-r が付くかを見る */
function fakeWrap() {
  const cls = new Set();
  const inner = { scrollWidth: 1000, clientWidth: 400, scrollLeft: 0 };
  return { cls, inner, el: { querySelector: () => inner,
    classList: { toggle: (c, on) => { if (on) cls.add(c); else cls.delete(c); } } } };
}
function withWraps(w, fn) {
  const qsa = ctx.document.querySelectorAll;
  ctx.document.querySelectorAll = (s) => (s === ".scroll-wrap" ? [w.el] : []);
  try { return fn(); } finally { ctx.document.querySelectorAll = qsa; }
}

check("V12 の残り: 枠を横に動かす（scroll の捕捉）・窓の幅・details を開く、で影を付け直す", () => {
  const find = (t) => winListeners.filter((l) => l.type === t);
  const sc = find("scroll");
  ok(sc.length === 1 && sc[0].capture === true, "scroll を捕捉側で受けるリスナーが無い（scroll は泡立たない）");
  const w = fakeWrap();
  const target = { classList: { contains: (c) => c === "scroll" }, parentNode: w.el };
  sc[0].fn({ target });
  ok(w.cls.has("more-r") && !w.cls.has("more-l"), "scroll で左端の枠に影が付かない: " + [...w.cls]);
  w.inner.scrollLeft = 600; sc[0].fn({ target });
  ok(!w.cls.has("more-r") && w.cls.has("more-l"), "右端まで動かしたのに影が付け直されない: " + [...w.cls]);
  for (const [t, cap] of [["resize", undefined], ["toggle", true]]) {
    const ls = find(t);
    ok(ls.length === 1 && (cap === undefined || ls[0].capture === cap), t + " のリスナーが無い" + (cap ? "（捕捉側で）" : ""));
    const v = fakeWrap();
    withWraps(v, () => ls[0].fn({}));
    ok(v.cls.has("more-r"), t + " で影を付け直していない");
  }
});

/* 本物の renderHq。V12 の残りの見張りは fetch の後まで renderHq を差し替えたままにするので、
   後の見張りが差し替え中の値を「元」として覚えて戻すと、差し替えが残って以降の見張りが空振りする。
   renderHq を戻すときは、差し替え前に覚えたこの値を使う */
const HQ_ORIG = run("renderHq");

check("V12 の残り: 本部アプローチの枠を差し込んだ後（持っているとき・取りに行った後）に影を付ける", () => {
  const saved = { rh: run("renderHq"), rj: run("readJson"), fetch: ctx.fetch, qsa: ctx.document.querySelectorAll };
  const restore = () => {
    ctx.__rh = saved.rh; ctx.__rj = saved.rj;
    run("renderHq = __rh; readJson = __rj; hqCache = null;");
    ctx.fetch = saved.fetch; ctx.document.querySelectorAll = saved.qsa; delete els["hq-box"];
  };
  ctx.__noop = () => "";
  run("renderHq = __noop; readJson = (r) => r.json();");
  els["hq-box"] = fakeEl();
  try {
    // 持っているとき（hqCache）
    const w1 = fakeWrap();
    run("hqCache = { x: 1 }");
    withWraps(w1, () => run("wireHq()"));
    ok(w1.cls.has("more-r"), "hqCache から差し込んだ後に影を付けていない");
  } catch (e) { restore(); throw e; }
  // 取りに行ったとき（fetch の後）
  run("hqCache = null");
  const w2 = fakeWrap();
  ctx.fetch = () => Promise.resolve({ status: 200, json: () => Promise.resolve({ ok: 1 }) });
  ctx.document.querySelectorAll = (s) => (s === ".scroll-wrap" ? [w2.el] : []);
  run("wireHq()");
  const tick = () => new Promise((res) => setImmediate(res));
  return tick().then(tick).then(() => {
    restore();
    ok(w2.cls.has("more-r"), "本部アプローチを取りに行って差し込んだ後に影を付けていない");
  }, (e) => { restore(); throw e; });
});

check("V12 の残り: 暗い表示でも枠の端の影が地と見分けられる（明るい表示と同じくらいの差）", () => {
  const css = html.split("<style>")[1].split("</style>")[0];
  // 右端の影（::after）と、左に続きがあるときの貼り付けた1列目の影（box-shadow）。左端の帯（::before）は S-12 で外した
  ok(/linear-gradient\(to left, var\(--edge-shade\)/.test(css) && /td:first-child\{\s*box-shadow:[^;]*var\(--edge-shade\)/.test(css),
    "影の色がテーマの値（--edge-shade）でなく固定の色");
  // 3 か所（明るい :root / prefers-color-scheme:dark / data-theme="dark"）の --panel と --edge-shade を読む
  const [light, rest] = [css.split("@media (prefers-color-scheme:dark)")[0], css.split("@media (prefers-color-scheme:dark)")[1]];
  const blocks = [light, rest.split(':root[data-theme="dark"]')[0], rest.split(':root[data-theme="dark"]')[1].split("}")[0]];
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const lum = (c) => {
    const f = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2];
  };
  blocks.forEach((b, i) => {
    const p = (b.match(/--panel:(#[0-9a-f]{6})/) || [])[1];
    const e = b.match(/--edge-shade:rgba\((\d+),(\d+),(\d+),([.0-9]+)\)/);
    ok(p && e, "ブロック " + i + " に --panel か --edge-shade が無い");
    const bg = hex(p), a = +e[4];
    const mix = bg.map((v, k) => v * (1 - a) + +e[k + 1] * a);
    const L1 = lum(bg), L2 = lum(mix);
    const r = (Math.max(L1, L2) + 0.05) / (Math.min(L1, L2) + 0.05);
    // 明るい表示（白の上の 20% の黒）で約 1.6:1。以前の暗い表示（#1d2026 の上の 20% の黒）は約 1.1:1
    ok(r >= 1.4, "ブロック " + i + " の影と地の差 " + r.toFixed(2) + ":1 が小さい（ほとんど見えない）");
  });
});

check("凡例: MTG の項目の埋まり具合の「15%未満」の粒は棒と同じ濃さ", () => {
  const q = run("renderMtgQ(__MQ)");
  const g = q.split("<figcaption>抽出の進み具合")[1].split("</figure>")[0];
  // __MQ の「やること」は 10%（15%未満）
  const bop = (g.match(/<rect [^>]*style="fill:var\(--ghost\)" opacity="([.0-9]+)"><title>やること/) || [])[1];
  const s = legendSwatch(g.split('<div class="figlegend">')[1], "15%未満");
  ok(bop && s && s.fill === "var(--ghost)" && s.op === bop, "15%未満の凡例 " + JSON.stringify(s) + " が棒の濃さ " + bop + " と違う");
});

check("定義と検証: 「3つのいつ」の表も折り返す（prose）", () => {
  const h = run("renderDefs()");
  const t = h.split("3つの「いつ」は別物")[1].split("</table>")[0];
  ok(/<table class="prose">/.test(t), "「3つのいつ」の表が折り返す表（prose）になっていない");
});

/* 読み方の枠の見出しが、本文の頭の文と同じ（または本文の頭が見出しそのもの）になっていないか */
function noteHeadDup(h) {
  const out = [];
  for (const m of String(h).matchAll(/<span class="hd">([^<]*)<\/span><p>([\s\S]*?)<\/p><\/div>/g)) {
    const head = m[1].trim();
    const first = m[2].replace(/<[^>]+>/g, "").replace(/^[※\s]+/, "").split("。")[0].trim();
    if (head && first && (first.startsWith(head) || head.includes(first))) out.push(head);
  }
  return out;
}
check("読み方の枠: MTG の品質・電話で、見出しと本文の頭に同じ文を出さない（サーバの文そのもので見る）", () => {
  // routes.rs build_mtg_quality の not_counted と build_phone の transcript.note（直した時点の文）
  const mq = JSON.parse(JSON.stringify(ctx.__MQ));
  mq.meta.not_counted = "※ MTG の良し悪しを採点していません。何が記録されているかだけを出しています";
  ctx.__MQd = mq;
  const ph = JSON.parse(JSON.stringify(ctx.__PH));
  ph.transcript.note = "電話の中身はまだ読めていません。文字起こしを取れているのはごく一部です";
  ctx.__PHd = ph;
  const rs = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/routes.rs"), "utf-8");
  ok(rs.includes(mq.meta.not_counted) && rs.includes(ph.transcript.note), "サーバの文が変わった（この見張りの入力を合わせ直す）");
  for (const [name, code] of [["MTG の品質", "renderMtgQ(__MQd)"], ["電話", "renderPhone(__PHd)"]]) {
    const d = noteHeadDup(run(code));
    ok(!d.length, name + " の枠「" + d.join("」「") + "」の見出しと本文の頭が同じ文");
  }
});

/* ================================================================ 図の部品（第3弾, 2026-09-24） */
/* 折れ線の点の高さを読む（svgLine の点の形。title の頭は「月: 値」） */
function lineDots(svg) {
  return [...String(svg).matchAll(/<circle cx="([\d.]+)" cy="([\d.]+)" r="[\d.]+"(?: data-shifted="(-?\d+)")?[^>]*>\s*<title>([^<]*)<\/title>/g)]
    .map((m) => ({ cx: +m[1], cy: +m[2], shifted: m[3] != null ? +m[3] : 0, title: m[4] }));
}
check("ずらし: 5系列とも 0 の月でも、5つの点が全部別の高さに描かれ、軸より下に出ない（軸の端で反転して重ならない）", () => {
  // 🔴 横断レビュー (a): 前は上下交互で、軸の端では下の番が上へ返るので 0,-4,-4,-8,-8 になり、見える線は 3 本だった
  const svg = run('svgLine({ x: ["25-07","25-08"], series: [0,1,2,3,4].map(() => ({ pts: [{ v: 0 }, { v: 0 }] })), h: 250 })');
  const ds = lineDots(svg);
  ok(ds.length === 10, "点が 10 個（5系列 × 2か月）でない: " + ds.length);
  const axisY = Math.max(...ds.map((d) => d.cy));   // いちばん下の点（ずらしていない 1 本目）＝ 0 の位置
  for (const cx of new Set(ds.map((d) => d.cx))) {
    const ys = ds.filter((d) => d.cx === cx).map((d) => d.cy);
    ok(new Set(ys.map((y) => y.toFixed(1))).size === 5, "同じ月の 5 点のうち重なっているものがある: " + ys.join(","));
    ok(ys.every((y) => y <= axisY + .05), "軸（0 の線）より下に描いた点がある: " + ys.join(",") + " 軸=" + axisY);
  }
  ok(/<svg [^>]*data-shift="edge"/.test(svg), "軸の端でずらしたことの印（data-shift=\"edge\"）が無い");
  const sh = ds.filter((d) => d.shifted);
  ok(sh.length === 8 && sh.every((d) => /ずらして表示/.test(d.title)), "ずらした点（8 個）の title にずらしたことが書かれていない");
  const f = run('fig("採用数", "", svgLine({ x: ["a","b"], series: [0,1,2].map(() => ({ pts: [{ v: 0 }, { v: 0 }] })) }))');
  ok(f.includes("軸の内側へずらしている") && f.includes("値は変えていません"),
    "軸の端でずらした図の凡例に、0 の線が 0 より上に見えることの断りが無い");
});
check("ずらし: 軸の途中で重なった 3 本は上と下に分かれ、全部が別の高さ。重ならない図には印も断りも出さない", () => {
  const svg = run('svgLine({ x: ["a","b"], series: [10,20,30].map((v0) => ({ pts: [{ v: v0 }, { v: 50 }] })).concat([{ pts: [{ v: 100 }, { v: 100 }] }]) })');
  const mid = lineDots(svg).filter((d) => /^b: 50/.test(d.title));
  ok(mid.length === 3, "50 の点が 3 つ取れない: " + mid.length);
  ok(new Set(mid.map((d) => d.cy)).size === 3, "50 の 3 点のうち重なっているものがある: " + mid.map((d) => d.cy));
  const sh = mid.map((d) => d.shifted).sort((a, b) => a - b);
  ok(sh[0] < 0 && sh[1] === 0 && sh[2] > 0, "軸の途中なのに上下に分かれていない: " + sh);
  ok(/<svg [^>]*data-shift="1"/.test(svg), "途中でずらした図に data-shift=\"1\" が無い");
  const f = run('fig("x", "", svgLine({ x: ["a","b"], series: [{ pts: [{ v: 1 }, { v: 2 }] }, { pts: [{ v: 3 }, { v: 4 }] }] }))');
  ok(!/data-shift|ずらして/.test(f), "重なっていない図にずらしの印・断りが出ている");
});
check("ずらし: lineShift は r が違えば別の位置を返し、枠の外に出さない（上端・下端・途中・片側が狭い）", () => {
  for (const [up, down] of [[0, 200], [200, 0], [100, 100], [6, 200], [200, 6]]) {
    const ds = run(`[1,2,3,4,5,6].map((r) => lineShift(r, ${up}, ${down}, 4))`);
    ok(new Set(ds).size === 6 && ds.every((d) => d !== 0), `up=${up} down=${down} で同じ位置・ずらし 0 がある: ` + ds);
    ok(ds.every((d) => -d <= up + .5 && d <= down + .5), `up=${up} down=${down} で枠の外に出る: ` + ds);
  }
});

check("compact: 400px の枠（334px）では、横棒の注記を棒の下に回して枠の幅に収める（値と注記が横スクロールの奥に行かない）", () => {
  const code = 'svgBarH({ w: 680, fmt: F.pct, rows: [' +
    '{ label: "ケアサポートかがやき", v: 58.1, txt: "58.1%", note: "解約・充足 100%（決着済み 12件中）　3人" },' +
    '{ label: "拠点B", v: 12, txt: "12.0%", note: "解約・充足 12.0%（決着済み 25件中）　8人" }] })';
  const wide = run("FIGFIT.seq = 0; FIGFIT.avail = null; " + code);
  ok(/viewBox="0 0 680 /.test(wide) && !/data-under/.test(wide), "枠が分からないとき（1回目）は描いた幅のまま・注記は右");
  const narrow = run("FIGFIT.seq = 0; FIGFIT.avail = { 0: 334 }; try { " + code + " } finally { FIGFIT.avail = null; }");
  ok(/viewBox="0 0 334 /.test(narrow), "334px の枠に合わせて描いていない: " + (narrow.match(/viewBox="[^"]*"/) || [""])[0]);
  const under = [...narrow.matchAll(/<text class="ax" data-under="1" x="([\d.]+)"[^>]*>([^<]*)</g)];
  ok(under.length >= 2, "注記が棒の下の行に回っていない");
  const textW = run("textW");
  ok(under.every((m) => +m[1] + textW(m[2]) <= 334 + 1), "棒の下の注記が枠の右からはみ出す: " +
    under.map((m) => m[2]).join(" | "));
  ok(narrow.includes("58.1%") && narrow.includes("100%"), "値（58.1%）・注記（100%）を落としている");
});

check("paintFigs: 1回目に測った枠の幅で図を描き直し（2回描き）、畳んだ枠の図には data-unfit を付ける", () => {
  let calls = 0;
  const svgs = [];
  const el = {
    id: "x", clientWidth: 334,
    set innerHTML(v) {
      this._h = v; svgs.length = 0;
      for (const m of v.matchAll(/<svg [^>]*viewBox="0 0 ([\d.]+) [^"]*"[^>]*data-fk="(\d+)"/g)) {
        const k = svgs.length;
        svgs.push({ attrs: { "data-fk": m[2] }, viewBox: { baseVal: { width: +m[1] } },
          closest: () => ({ clientWidth: k === 0 ? 334 : 0 }),   // 2つ目の図は畳んだ枠の中（幅 0）
          getAttribute(a) { return this.attrs[a]; }, setAttribute(a, b) { this.attrs[a] = b; } });
      }
    },
    get innerHTML() { return this._h; },
    querySelectorAll: (s) => (s === "svg[data-fk]" ? svgs : []),
  };
  ctx.__EL = el;
  ctx.__MK = () => { calls++; return run('svgStack({ w: 680, parts: [{ label: "a", v: 3 }, { label: "b", v: 1 }] }) + svgStack({ w: 680, parts: [{ label: "a", v: 1 }] })'); };
  run("paintFigs(__EL, __MK)");
  ok(calls === 2, "枠と描いた幅が違うのに描き直していない（make の呼び出し " + calls + " 回）");
  ok(/viewBox="0 0 334 /.test(el.innerHTML), "描き直した図が枠の幅（334）になっていない");
  ok(/viewBox="0 0 680 /.test(el.innerHTML), "畳んだ枠の図（幅を測れない）まで幅を変えている");
  ok(svgs[1].attrs["data-unfit"] === "1" && !svgs[0].attrs["data-unfit"], "畳んだ枠の図だけに data-unfit が付いていない");
  // 枠と描いた幅が合っていれば 1 回で終わる
  calls = 0;
  ctx.__MK = () => { calls++; return run('svgStack({ w: 334, parts: [{ label: "a", v: 3 }] })'); };
  run("paintFigs(__EL, __MK)");
  ok(calls === 1, "幅が合っている図まで描き直している（" + calls + " 回）");
});

check("figToData: 横にスクロールする図は、開いた直後の位置をデータのある側へ合わせる（左にもデータが見える図は動かさない）", () => {
  const body = (x0) => {
    const b = { scrollWidth: 1000, clientWidth: 334, scrollLeft: 0 };
    b.querySelector = () => ({ viewBox: { baseVal: { width: 1000 } }, getBoundingClientRect: () => ({ width: 1000 }),
      getAttribute: (a) => (a === "data-x0" ? String(x0) : null) });
    return b;
  };
  const far = body(800), near = body(100);
  ctx.__EL = { querySelectorAll: () => [far, near] };
  run("figToData(__EL)");
  ok(far.scrollLeft === 666, "データが右の奥にある図の位置が合っていない: " + far.scrollLeft + "（期待 666 = 右端）");
  ok(near.scrollLeft === 0, "左にデータが見えている図を動かした: " + near.scrollLeft);
});

check("畳んだ枠: details を開いたとき、幅を測れずに描いた図があれば、開いたまま描き直す（横断レビュー (b)）", () => {
  const tg = winListeners.filter((l) => l.type === "toggle");
  ok(tg.length, "toggle を受ける処理が無い");
  const calls = [];
  ctx.__RD0 = run("redrawMain");
  ctx.__RD = (D, keep) => calls.push(keep);
  run("redrawMain = __RD; lastPayload = {};");
  const main = document.getElementById("cs-main");
  const qsa0 = main.querySelectorAll;
  try {
    const d0 = { open: false }, d1 = { open: true };
    main.querySelectorAll = (s) => (s === "details" ? [d0, d1] : []);
    main.contains = (d) => d === d1 || d === d0;
    const fire = (d) => tg.forEach((l) => l.fn({ target: d }));
    d1.querySelector = (s) => (s === "svg[data-unfit]" ? {} : null);
    fire(d1);
    ok(calls.length === 1, "幅を測れずに描いた図がある枠を開いても描き直さない（" + calls.length + " 回）");
    ok(JSON.stringify(calls[0]) === "[1]", "開いている details を開いたまま描き直していない: " + JSON.stringify(calls[0]));
    // 描き直した後（data-unfit が無い）・閉じたときは描き直さない（開き直しの toggle で繰り返さない）
    d1.querySelector = () => null; fire(d1);
    d0.querySelector = (s) => (s === "svg[data-unfit]" ? {} : null); fire(d0);
    ok(calls.length === 1, "描き直す必要の無い toggle でも描き直している（" + calls.length + " 回）");
  } finally {
    main.querySelectorAll = qsa0; delete main.contains;
    run("redrawMain = __RD0; lastPayload = null;");
  }
});
check("畳んだ枠: paintFigs は keep の details を開き直してから幅を測る（描き直しで畳まない）", () => {
  const ds = [{ open: false }, { open: false }];
  ctx.__EL = { id: "x", innerHTML: "", querySelectorAll: (s) => (s === "details" ? ds : []) };
  run('paintFigs(__EL, () => "<details></details><details></details>", [1])');
  ok(!ds[0].open && ds[1].open, "覚えた details を開き直していない: " + ds.map((d) => d.open));
});

check("図の部品: 案件・拠点を見分ける色に灰色どうし・判定の色を並べない（houjin の線 5 本・拠点の積み上げ）", () => {
  const cs = run("CASE_COLORS");
  ok(cs.length === 5 && new Set(cs).size === 5, "5 色が別々でない: " + cs);
  for (const bad of ["var(--ink-2)", "var(--ink-3)", "var(--ghost)", "var(--hi)", "var(--ki)", "var(--midori)"])
    ok(!cs.includes(bad), "見分けの色に灰色・判定の色 " + bad + " がある");
  ok(run("SITE_COLORS") === cs, "拠点の積み上げが案件の線と違う並びを使っている");
});
check("図の部品: 折れ線の 0 の点（○）が横軸の月の字に重ならない", () => {
  const svg = run('svgLine({ x: ["25-07","25-08"], series: [{ pts: [{ v: 0 }, { v: 2 }] }] })');
  const d = lineDots(svg).find((p) => /^25-07/.test(p.title));
  const lab = svg.match(/<text class="axl" x="[\d.]+" y="([\d.]+)"[^>]*>25-07</);
  ok(d && lab, "点か月の字が取れない");
  // 点の下端（半径 4.2）と月の字の上端（ベースラインから約 9px 上）の間を 3px 以上あける
  ok(+lab[1] - 9 - (d.cy + 4.2) >= 3, "0 の点と月の字の間が詰まっている: 点 " + d.cy + " / 字 " + lab[1]);
});
check("図の部品: 接触の帯が全部の月で記録なしのとき、目盛り「1」を出さない", () => {
  const lanes = (pts) => run(`svgStackLanes({ months: ["25-01","25-02"], lanes: [{ type: "bars", label: "接触", color: "blue", fillLabel: "60秒超", outLabel: "全体", pts: ${JSON.stringify(pts)} }] })`);
  const none = lanes([{ fill: 0, out: 0 }, { fill: 0, out: 0 }]);
  ok(!/<text class="ax" x="[\d.]+" y="[\d.]+">1<\/text>/.test(none), "棒が 1 本も無い帯に目盛り「1」が出る");
  const some = lanes([{ fill: 1, out: 2 }, { fill: 0, out: 0 }]);
  ok(/<text class="ax" x="[\d.]+" y="[\d.]+">2<\/text>/.test(some), "棒がある帯の目盛り（2）が消えた");
});
check("図の部品: 前回の値が 0 付近でも、前回の破線の枠を潰さず、本当の位置に縦の印を置く", () => {
  /* 🔴 ループ4（2026-09-24 実機, focus）: 前は最低 5px の枠を描いていたが、破線の目が2つしか入らず
     潰れた塊に見えた（本当の位置より右まで伸びてもいた）。狭い枠は描かず、縦の破線だけにする */
  const svg = run('svgBarH({ w: 680, fmt: F.int, rows: [{ label: "a", v: 900000, v0: 0 }] })');
  const minW = run("V0_MIN_W");
  const rs = [...svg.matchAll(/<rect x="[\d.]+" y="[\d.]+" width="([\d.]+)"[^>]*stroke-dasharray="3 2.4"/g)];
  ok(minW >= 8, "枠で描く最小の幅が小さすぎる（潰れた枠を描く）: " + minW);
  ok(rs.every((m) => +m[1] >= minW), "前回の破線の枠が潰れている（幅 " + rs.map((m) => m[1]) + "）");
  ok(/<line data-v0tick="1"[^>]*><title>前回: 0<\/title>/.test(svg), "前回の本当の位置の縦の印が無い");
  // 前回が 0 から離れていれば、枠は本当の幅で描く（縦の破線は出さない）
  const wide = run('svgBarH({ w: 680, fmt: F.int, rows: [{ label: "a", v: 900000, v0: 450000 }] })');
  const wr = wide.match(/<rect x="([\d.]+)" y="[\d.]+" width="([\d.]+)"[^>]*stroke-dasharray="3 2.4"/);
  ok(wr && +wr[2] > 100 && !/data-v0tick/.test(wide), "0 から離れた前回の枠が本当の幅で描かれていない: " + (wr && wr[2]));
  // 棒の下に回した注記（compact）は、前回の印（棒の上下にはみ出す）にかからない（focus の「38.40倍」・renewal の n=）
  const nar = drawAt('svgBarH({ w: 680, fmt: F.man, rows: [{ label: "三菱ケミカルテクニカ", v: 4320000, v0: 110000, txt: "432万", note: "38.40倍" }, { label: "宮崎商会 鹿児島工場", v: 680000, v0: 400000, txt: "68万", note: "13.60倍" }] })', 319);
  const unders = [...nar.matchAll(/<text class="ax" data-under="1" x="[\d.]+" y="([\d.]+)"[^>]*>([^<]*)</g)];
  const marks = [...nar.matchAll(/<line data-v0tick="1"[^>]*y2="([\d.]+)"|<rect x="[\d.]+" y="([\d.]+)" width="[\d.]+" height="([\d.]+)"[^>]*stroke-dasharray="3 2.4"/g)]
    .map((m) => (m[1] != null ? +m[1] : +m[2] + +m[3]));
  ok(unders.length === 2 && marks.length === 2, "注記か前回の印が取れない: " + unders.length + " / " + marks.length);
  // 字の上端はベースラインから約 9px 上。前回の印の下端との間を 2px 以上あける
  unders.forEach((u, i) => ok(+u[1] - 9 - marks[i] >= 2, "棒の下の注記「" + u[2] + "」が前回の印にかかる: 字 " + u[1] + " / 印の下端 " + marks[i]));
});

/* ================================================================ 図の部品（第3弾の検証の指摘, 2026-09-24） */
/* 図の部品を「2回目の描き（paintFigs が枠の幅を渡した）」として描く。avail = 枠の幅（null は1回目） */
function drawAt(code, avail) {
  ctx.__AV = avail == null ? null : { 0: avail };
  return run("FIGFIT.seq = 0; FIGFIT.avail = __AV; try { " + code + " } finally { FIGFIT.avail = null; FIGFIT.seq = 0; }");
}
/* 図の幅（viewBox）。左にラベルを貼り付けた図（stickyLabels）は、貼った側ではなく図そのもの（stickmain）の幅 */
const figW = (svg) => { const m = String(svg).match(/<svg (?:class="stickmain" )?[^>]*?viewBox="0 0 ([\d.]+) /g);
  const main = (m || []).find((t) => !/class="sticklab"/.test(t));
  return main ? +main.match(/viewBox="0 0 ([\d.]+) /)[1] : NaN; };
const monthsN = (n) => Array.from({ length: n }, (_, i) => "m" + i);

check("ずらし: 0 で重なった線が2本だけでも、軸の端でずらした印（data-shift=\"edge\"）と断りを出す（上端も）", () => {
  // 🔴 検証の指摘: r=1（2本目）の候補 -4 は上に収まるので「飛ばした候補」が無く、edge にならなかった
  //    （series の「面接と採用がどちらも 0 の月」）。点が枠の端にあってずらしたら edge
  let called = 0;
  ctx.__ON = () => { called++; };
  ok(run("lineShift(1, 196, 0, 4, __ON)") === -4 && called === 1, "0（下端）で 2 本目をずらしても onEdge が呼ばれない: " + called);
  called = 0;
  ok(run("lineShift(1, 0, 196, 4, __ON)") === 4 && called === 1, "上端で 2 本目をずらしても onEdge が呼ばれない: " + called);
  called = 0;
  run("lineShift(1, 100, 100, 4, __ON)");
  ok(called === 0, "軸の途中でずらしただけで端の扱いになった");
  const two = run('svgLine({ x: ["a","b"], series: [0,1].map(() => ({ pts: [{ v: 0 }, { v: 0 }] })) })');
  ok(/<svg [^>]*data-shift="edge"/.test(two), "0 で重なった 2 本の図に data-shift=\"edge\" が無い");
  const f = run('fig("x", "", svgLine({ x: ["a","b"], series: [{ pts: [{ v: 0 }, { v: 2 }] }, { pts: [{ v: 0 }, { v: 2 }] }] }))');
  ok(f.includes("0 の線も軸より少し上") && f.includes("いちばん上の線は少し下"), "端でずらした図の凡例に、0 と上端の見え方の断りが無い");
  ok(f.includes("線が少し傾いて見える"), "重なった月の点だけずらすので線が傾くことの断りが無い");
});

check("compact: 空白の無い長い注記も、400px の枠（334px）の中で折り返す（deal/houjin の進捗帯の中央値）", () => {
  // 🔴 検証の指摘: wrapText は空白と「/」でしか切らず、1語が幅を超えると SVG の右端で切れていた
  const note = "進捗帯「後半にさしかかり（50〜75%）」の中央値 12.3万円（軸の外）　3人";
  const svg = drawAt('svgBarH({ w: 680, fmt: F.man, rows: [{ label: "（伏字）2745", v: 50, txt: "50.0万", note: ' + JSON.stringify(note) + ' }] })', 334);
  ok(figW(svg) === 334, "334px の枠で描いていない: " + figW(svg));
  const under = [...svg.matchAll(/<text class="ax" data-under="1" x="([\d.]+)"[^>]*>([^<]*)</g)];
  const tw = run("textW");
  ok(under.length >= 2, "長い注記が1行のまま: " + under.map((m) => m[2]).join(" | "));
  ok(under.every((m) => +m[1] + tw(m[2]) <= 334 + 1), "注記が枠の右からはみ出す: " + under.map((m) => m[2]).join(" | "));
  ok(under.map((m) => m[2]).join("").replace(/\s/g, "") === note.replace(/\s/g, ""), "折り返しで字を落とした: " + under.map((m) => m[2]).join(" | "));
  ok(!under.slice(1).some((m) => /^[、。）」%％]/.test(m[2])), "閉じ括弧・句読点を行の頭に置いた: " + under.map((m) => m[2]).join(" | "));
  // 切れ目にちょうど閉じ括弧が来る文（5 字ぶんの幅で「あいうえお）」）でも、閉じ括弧を行の頭に置かない
  const ls = run('wrapText("あいうえお）かきくけこ", textW("あいうえお"))');
  ok(ls.join("") === "あいうえお）かきくけこ" && !ls.slice(1).some((l) => /^[）」、。]/.test(l)), "閉じ括弧を行の頭に置いた: " + ls.join(" | "));
});

check("枠に合わせる: 1440px の枠（1116px）では、どの図の部品も枠の幅まで広げる（右が空かない）", () => {
  const parts = {
    svgLine: 'svgLine({ x: ["a","b","c"], series: [{ pts: [{ v: 1 }, { v: 2 }, { v: 3 }] }] })',
    svgStackLanes: 'svgStackLanes({ months: ["a","b"], lanes: [{ type: "line", label: "応募", color: "blue", pts: [{ v: 1 }, { v: 2 }] }] })',
    svgTimeline: 'svgTimeline({ lanes: [{ label: "a", marks: [{ d: "2025-01-10" }, { d: "2025-06-10" }] }] })',
    svgColStack: 'svgColStack({ x: ["a","b"], series: [{ label: "s", color: "blue", vals: [{ v: 1 }, { v: 2 }] }] })',
    svgHist: 'svgHist({ values: [1,2,3,4,5] })',
    svgScatter: 'svgScatter({ pts: [{ x: 1, y: 2 }, { x: 3, y: 4 }] })',
    svgBoxH: 'svgBoxH({ rows: [{ label: "a", med: 5, q1: 3, q3: 7, min: 1, max: 9, n: 20 }] })',
    svgBarH: 'svgBarH({ rows: [{ label: "a", v: 5, note: "注記" }] })',
    svgStack: 'svgStack({ parts: [{ label: "a", v: 3 }, { label: "b", v: 1 }] })',
  };
  for (const [name, code] of Object.entries(parts)) {
    const w1 = figW(drawAt(code, null)), w2 = figW(drawAt(code, 1116));
    ok(w1 < 1116 && w2 === 1116, name + " が 1116px の枠で広がらない（1回目 " + w1 + " → 2回目 " + w2 + "）");
  }
});

check("枠に合わせる: 400px の枠（334px）では、狭めても読める図は枠の幅で描き、月の多い推移の図は狭めない", () => {
  const narrow = {
    svgLine: 'svgLine({ x: ["a","b","c"], series: [{ pts: [{ v: 1 }, { v: 2 }, { v: 3 }] }] })',
    svgColStack: 'svgColStack({ x: ["a","b","c"], series: [{ label: "s", color: "blue", vals: [{ v: 1 }, { v: 2 }, { v: 3 }] }] })',
    svgHist: 'svgHist({ values: [1,2,3,4,5] })',
    svgScatter: 'svgScatter({ pts: [{ x: 1, y: 2 }, { x: 3, y: 4 }] })',
    svgBoxH: 'svgBoxH({ rows: [{ label: "a", med: 5, q1: 3, q3: 7, min: 1, max: 9, n: 20 }] })',
    svgStack: 'svgStack({ parts: [{ label: "a", v: 3 }, { label: "b", v: 1 }] })',
  };
  for (const [name, code] of Object.entries(narrow))
    ok(figW(drawAt(code, 334)) === 334, name + " が 334px の枠で狭まらない: " + figW(drawAt(code, 334)));
  // 月の多い推移の図は狭めない（点と月の字が詰まる）
  const ones = JSON.stringify(monthsN(24).map(() => ({ v: 1 })));
  const line24 = drawAt('svgLine({ x: ' + JSON.stringify(monthsN(24)) + ', series: [{ pts: ' + ones + ' }] })', 334);
  ok(figW(line24) === 660, "24 か月の折れ線を 334px に狭めた: " + figW(line24));
  const col24 = drawAt('svgColStack({ x: ' + JSON.stringify(monthsN(24)) + ', series: [{ label: "s", color: "blue", vals: ' + ones + ' }] })', 334);
  ok(figW(col24) === 660, "24 か月の積み上げ縦棒を 334px に狭めた: " + figW(col24));
  // 横棒: 枠が描いた幅より狭くても、棒の欄が 160px 残るなら（compact にせず）枠の幅で描く
  const bar = drawAt('svgBarH({ w: 660, rows: [{ label: "a", v: 5, note: "注記" }] })', 600);
  ok(figW(bar) === 600 && !/data-under/.test(bar), "棒の欄が残る 600px の枠で、枠の幅（注記は右）で描いていない: " + figW(bar));
});

check("横スクロールの図: 折れ線・帯を縦に積む図・積み上げ縦棒は、最初にデータがある位置（data-x0）を出し、狭い枠の折れ線は目盛りを左に貼る", () => {
  const x0 = (svg) => { const m = String(svg).match(/<svg [^>]*data-x0="(\d+)"/); return m ? +m[1] : null; };
  const late = JSON.stringify(monthsN(24).map((_, i) => (i >= 20 ? { v: 1 } : null)));
  const mx = JSON.stringify(monthsN(24));
  const line = drawAt('svgLine({ x: ' + mx + ', series: [{ pts: ' + late + ' }] })', null);
  ok(x0(line) > 400, "折れ線の data-x0 が無い・データの位置と違う: " + x0(line));
  const lanes = drawAt('svgStackLanes({ months: ' + mx + ', lanes: [{ type: "line", label: "応募", color: "blue", pts: ' + late + ' }] })', null);
  ok(x0(lanes) > 400, "帯を縦に積む図の data-x0 が無い: " + x0(lanes));
  const col = drawAt('svgColStack({ x: ' + mx + ', series: [{ label: "s", color: "blue", vals: ' + late + ' }] })', null);
  ok(x0(col) > 400, "積み上げ縦棒の data-x0 が無い: " + x0(col));
  ok(/data-x0="123"/.test(run('svgHead(500, 100, "a", { x0: 123 })')), "svgHead が data-x0 を出さない");
  // 狭い枠で横スクロールになる折れ線は、縦軸の目盛りを左に貼り付け、data-x0 を貼った欄のぶん手前にする
  const code = 'svgLine({ x: ' + mx + ', series: [{ pts: ' + late + ' }] })';
  const st = drawAt(code, 334);
  ok(/<div class="stickwrap"/.test(st) && /class="sticklab"/.test(st), "狭い枠の折れ線で縦軸の目盛りを左に貼り付けていない");
  ok(x0(st) === x0(line) - 58, "貼り付けた目盛りの欄のぶん data-x0 を手前にしていない: " + x0(st) + " / " + x0(line));
  ok(!/stickwrap/.test(drawAt(code, 1116)), "広い枠の折れ線にまで目盛りを貼り付けた");
});

check("100%帯: 細い区間も 2px 以上で描く（最優先の 26 件が線に潰れない）", () => {
  const svg = run('svgStack({ w: 660, parts: [{ label: "大", v: 10000 }, { label: "最優先", v: 1 }] })');
  const m = svg.match(/<rect x="[\d.]+" y="6" width="([\d.]+)"[^>]*><title>最優先/);
  ok(m && +m[1] >= 2, "細い区間の幅 " + (m && m[1]) + " が 2px 未満");
});

check("暗い表示: 案件を見分ける牡丹・空（--botan/--sora）を暗い地の上で見える色に置き換える（3 か所）", () => {
  const css = html.split("<style>")[1].split("</style>")[0];
  const [light, rest] = [css.split("@media (prefers-color-scheme:dark)")[0], css.split("@media (prefers-color-scheme:dark)")[1]];
  const blocks = [light, rest.split(':root[data-theme="dark"]')[0], rest.split(':root[data-theme="dark"]')[1].split("}")[0]];
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const lum = (c) => {
    const f = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2];
  };
  blocks.forEach((b, i) => {
    const p = (b.match(/--panel:(#[0-9a-f]{6})/) || [])[1];
    for (const k of ["botan", "sora"]) {
      const c = (b.match(new RegExp("--" + k + ":(#[0-9a-f]{6})")) || [])[1];
      ok(p && c, "ブロック " + i + " に --" + k + " が無い（暗い表示で明るい表示の色のまま）");
      const [a, z] = [lum(hex(p)), lum(hex(c))];
      const r = (Math.max(a, z) + 0.05) / (Math.min(a, z) + 0.05);
      ok(r >= 3, "ブロック " + i + " の --" + k + " と地の差 " + r.toFixed(2) + ":1 が 3:1 未満");
    }
  });
});

check("描き直し: 表示・絞り込み・本部アプローチ・窓の幅の変化は paintFigs で描き、幅の描き直しは開いた details を渡す", () => {
  const saved = { pf: run("paintFigs"), rd: run("redrawMain"), wire: run("wire"), rh: HQ_ORIG, cur: run("JSON.stringify(cur)"),
    st: ctx.setTimeout, fetch: ctx.fetch };
  const calls = [];
  ctx.__PF = (el, make, keep) => { calls.push({ el, keep }); };
  const main = document.getElementById("cs-main");
  const qsa0 = main.querySelectorAll, qs0 = main.querySelector;
  const restore = () => {
    ctx.__rs = saved;
    run("paintFigs = __rs.pf; redrawMain = __rs.rd; wire = __rs.wire; renderHq = __rs.rh; cur = JSON.parse(__rs.cur); hqCache = null; hqKeep = null; lastPayload = null; paintedW = 0;");
    ctx.setTimeout = saved.st; ctx.fetch = saved.fetch;
    main.querySelectorAll = qsa0; main.querySelector = qs0; delete main.clientWidth; delete els["hq-box"];
  };
  try {
    run("paintFigs = __PF; wire = () => {}; renderHq = () => '';");
    // 定義と検証（API の無い項目）の load
    // 2026-09-29 組み替え 段A で API を持たない画面（定義と検証）は「記録と数字の信頼度」の節になり、MENUS から無くなった。
    // load の API 無しの分岐は残っているので、仮の画面を 1 つ足して同じ性質を見る
    run('MENUS[2].views.push({ key: "zz-noapi", label: "API の無い仮の画面", path: null, render: () => renderDefs() }); cur = { menu: "monthly", view: "zz-noapi" }; load(); MENUS[2].views.pop()');
    ok(calls.length === 1 && calls[0].el === main, "API の無い項目の load が paintFigs で描いていない");
    // 絞り込み・幅の描き直し（redrawMain）は keep をそのまま渡す
    calls.length = 0;
    run("redrawMain({}, [2])");
    ok(calls.length === 1 && JSON.stringify(calls[0].keep) === "[2]", "redrawMain が keep を paintFigs に渡していない: " + JSON.stringify(calls[0] && calls[0].keep));
    // 本部アプローチ（持っているとき）
    calls.length = 0;
    els["hq-box"] = fakeEl();
    run("hqCache = { x: 1 }; wireHq()");
    ok(calls.length === 1 && calls[0].el === els["hq-box"], "本部アプローチの枠を paintFigs で描いていない");
    // 窓の幅が変わった（resize → refitSoon → redrawMain(lastPayload, openDetails(main))）
    const rd = [];
    ctx.__RD = (D, keep) => rd.push(keep);
    run("redrawMain = __RD; lastPayload = {}; paintedW = 334;");
    ctx.setTimeout = (fn) => { fn(); return 1; };
    main.clientWidth = 1116;
    main.querySelector = (s) => (s === "svg[data-fk]" ? {} : null);
    main.querySelectorAll = (s) => (s === "details" ? [{ open: false }, { open: true }] : []);
    winListeners.filter((l) => l.type === "resize").forEach((l) => l.fn({}));
    ok(rd.length === 1, "窓の幅が変わっても図を描き直さない（" + rd.length + " 回）");
    ok(JSON.stringify(rd[0]) === "[1]", "幅の描き直しで開いている details を渡していない: " + JSON.stringify(rd[0]));
  } catch (e) { restore(); throw e; }
  restore();
  /* ここから先は応答を待つ。先に走った見張り（本部アプローチの取得）が終わってから差し替える。
     終わる前に差し替えると、その見張りの paintFigs・#hq-box の片付けと混ざる */
  return Promise.all(pendingChecks.slice()).then(() => {
    // API のある項目の load（応答の後）も paintFigs で描く
    const calls2 = [];
    ctx.__PF = (el) => { calls2.push(el); };
    run("paintFigs = __PF; wire = () => {}; renderHq = () => '';");
    ctx.fetch = () => Promise.resolve({ ok: true, status: 200, url: "/api/x", headers: { get: () => "application/json" }, json: () => Promise.resolve({ meta: {} }) });
    run('cur = { menu: "deal", view: "today" }');
    const p = run("load()");
    // 本部アプローチを取りに行った後も paintFigs で描く
    const hb = fakeEl();
    els["hq-box"] = hb;
    run("hqCache = null; wireHq()");
    const tick = () => new Promise((res) => setImmediate(res));
    return p.then(tick).then(tick).then(() => {
      restore();
      ok(calls2.filter((el) => el === main).length === 1, "応答の後の load が paintFigs で描いていない");
      ok(calls2.includes(hb), "本部アプローチを取りに行った後、paintFigs で描いていない");
    }, (e) => { restore(); throw e; });
  });
});

check("本部アプローチ: 幅の描き直しで、#hq-box の中の開いた details を数え違えず、描き直した後に開き直す", () => {
  // 🔴 検証の指摘: openDetails(main) が #hq-box の中の details も数え、本文の描き直しの時点では #hq-box が空なので
  //    番号がずれ、#hq-box の中身（wireHoujin が keep 無しで描く）は畳まれていた
  const hqd = { open: true }, own = { open: true }, own0 = { open: false };
  const hqEl = { id: "hq-box", querySelectorAll: (s) => (s === "details" ? [hqd] : []) };
  hqd.closest = (s) => (s === "[data-paint-own]" ? hqEl : null);
  own.closest = own0.closest = () => null;
  ctx.__M = { querySelectorAll: (s) => (s === "details" ? [own0, hqd, own] : []) };
  ok(run("JSON.stringify(openDetails(__M))") === "[1]", "本文の details の番号に #hq-box の中の details が混ざっている: " + run("JSON.stringify(openDetails(__M))"));
  ctx.__H = hqEl;
  ok(run("JSON.stringify(openDetails(__H))") === "[0]", "#hq-box 自身の details を数えていない");
  ok(html.includes('<div id="hq-box" data-paint-own="1">'), "#hq-box に data-paint-own が無い（本文の details に数えられる）");
  // redrawMain(keep) が #hq-box の開いた details を覚え、wireHoujin がその keep で描く
  const saved = { pf: run("paintFigs"), wire: run("wire"), rh: HQ_ORIG };
  const calls = [];
  ctx.__PF = (el, make, keep) => calls.push({ el, keep });
  const hb = fakeEl(); hb.querySelectorAll = (s) => (s === "details" ? [{ open: false }, { open: true }] : []);
  els["hq-box"] = hb;
  try {
    run("paintFigs = __PF; wire = () => {}; renderHq = () => ''; hqCache = { x: 1 };");
    run("redrawMain({}, [])");
    els["hq-box"] = fakeEl();   // 本文を描き直すと #hq-box は新しい枠になる
    run("wireHq()");
    const hq = calls.find((c) => c.el === els["hq-box"]);
    ok(hq && JSON.stringify(hq.keep) === "[1]", "#hq-box を開いていた details のまま描き直していない: " + JSON.stringify(hq && hq.keep));
    calls.length = 0;
    els["hq-box"] = hb;
    run("redrawMain({})");   // 絞り込みの描き直しは覚えない（行が変わると番号が別の行を指す）
    els["hq-box"] = fakeEl();
    run("wireHq()");
    const hq2 = calls.find((c) => c.el === els["hq-box"]);
    ok(hq2 && !(hq2.keep && hq2.keep.length), "絞り込みの描き直しでも #hq-box の details を開き直している");
  } finally {
    ctx.__rs = saved;
    run("paintFigs = __rs.pf; wire = __rs.wire; renderHq = __rs.rh; hqCache = null; hqKeep = null;");
    delete els["hq-box"];
  }
});

/* ================================================================ 第3弾（2026-09-24 実機）: 文言と表 */
/* 本文に出る JS の文だけを見る（コメントは経緯として古い番号・語を残してよい） */
const jsNoComment = js.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

check("解約率の色の線: 全画面で1つ（40%）。継続回数×成果・立ち上がり・本部アプローチで同じ値が同じ色", () => {
  /* 線引きは藤巻さんの確認待ち。根拠が見つからなかったので renewal が使ってきた 40% に揃えた。
     線を動かすときは、この見張りの数字も一緒に直す（黙って動かさない） */
  ok(run("CANCEL_HI") === 40, "解約率の線が 40% でない: " + run("CANCEL_HI"));
  ok(run("cancelColor(40)") === "var(--hi)" && run("cancelColor(39.9)") === "var(--ai)", "40% の境で色が切り替わらない");
  ok(run("cancelColor(null)") === "var(--ai)", "値が無いのに緋にしている");
  const box = { n: 40, min: 0, q1: 1, median: 2, q3: 3, max: 9, mean: 2.5 };
  // 45.1% は本番（2026-09-23）の継続1。線を 50% にすると、ここが藍に変わる
  ctx.__RNc = { meta: { exclude_right_censored: false, right_censored_n: 0 }, monthly_retention: { rows: [] },
    missingness: [], population: {},
    by_renewal: [{ renewal_no: 1, n: 100, n_active: 0, denom: 100, cancel_rate: 45.1, cancel_rate_excl_fill: 30,
      oubo: box, mensetu: box, syoudaku: box, oubo_per_posting: box, amount: box }] };
  const rn = run("renderRenewal(__RNc)").split("<figcaption>").find((x) => x.includes("解約率 40%以上")) || "";
  ok(/fill:var\(--hi\)/.test(rn.split('<div class="figlegend">')[0]), "継続回数×成果で 45.1% が緋でない");
  ok(!rn.includes("解約率 40%未満"), "図に無い色（40%未満）を凡例に出している");
  // 立ち上がり: 49.7% は本番の帯の最大。この図だけは線で色を分けない（帯どうしの比較で、向きは読めない）。
  // 線を使っていないことは凡例に文で書く。棒は凡例を外してから見る（凡例の粒で通らないように）
  const ru = JSON.parse(JSON.stringify(ctx.__RU));
  ru.first_mtg.buckets = [{ label: "14日以内", n: 60, denom: 50, cancel_rate: 49.7 },
                          { label: "61日超", n: 45, denom: 40, cancel_rate: 20 }];
  ctx.__RUc = ru;
  const g = run("renderRampup(__RUc)").split("<figcaption>帯ごとの、その後の解約率")[1].split("</figure>")[0];
  const gBars = g.split('<div class="figlegend">')[0];
  ok(/fill:var\(--ai\)/.test(gBars), "立ち上がりの棒が取れない（見張りが空振りする）");
  ok(!gBars.includes("var(--hi)"), "立ち上がりで 49.7% の帯だけ緋にしている（帯を判定の色で強調しない）");
  ok(!g.includes("var(--hi)"), "立ち上がりの凡例に、図で使っていない緋の粒を出している");
  ok(g.includes("「解約率 40%以上を緋」の線で色を分けていません"), "立ち上がりで線を使っていないことを書いていない");
  // 本部アプローチ: 注記の色も同じ線。
  // 上の「V12 の残り: 本部アプローチの枠…」が renderHq を差し替えたまま fetch の後に戻すので、それを待ってから描く
  return Promise.all(pendingChecks.slice()).then(() => {
    const hq2 = JSON.parse(JSON.stringify(ctx.__HQ));
    hq2.rows[0].rows[0].cancel_rate = 45;
    ctx.__HQc = hq2;
    const hq = run("renderHq(__HQc)");
    ok(hq.includes("解約率が 40% 以上の拠点は注記を緋で出しています"), "本部アプローチの説明の線が 40% でない");
    ok(/fill:var\(--hi\)"?[^>]*>解約・充足 45\.0%/.test(hq), "本部アプローチで 45% の拠点の注記が緋でない");
  });
});

check("goLink: 本文の「別の画面へ」は、行き先がすべて MENUS にあり、名前で出る（丸数字を使わない）", () => {
  /* テンプレートの goLink("…", "…") を全部拾って、行き先が MENUS に実在するかを見る。
     画面の key を変えたり画面を消したりしてリンクが死んだら、ここで落ちる */
  const calls = [...jsNoComment.matchAll(/goLink\("([a-z]+)",\s*"([a-z0-9]+)"\)/g)].map((m) => [m[1], m[2]]);
  ok(calls.length >= 10, "goLink の呼び出しが拾えていない: " + calls.length);
  ok(!/goLink\((?!"[a-z]+",\s*"[a-z0-9]+"\))/.test(jsNoComment.replace(/function goLink\(/, "")),
    "goLink に文字列の直書き以外を渡している（この見張りで行き先を確かめられない）");
  /* 2026-09-29 組み替え 段A: 消えた画面の鍵（renewal など）は移り先の画面（と節 ?at=）へのリンクになる（resolveView）。
     href の区切り/画面が MENUS に実在し、文がその画面の名前で始まることを見る */
  const keysOf = JSON.parse(run("JSON.stringify(MENUS.map((m) => [m.key, m.views.map((x) => [x.key, x.label])]))"));
  for (const [m, v] of calls) {
    const a = run("goLink(" + JSON.stringify(m) + ", " + JSON.stringify(v) + ")");
    const mm = a.match(/^<a class="golink" href="#([a-z]+)\/([a-z]+)(\?at=[a-z-]+)?">([^<]+)<\/a>$/);
    const grp = mm && keysOf.find((g) => g[0] === mm[1]);
    const view = grp && grp[1].find((x) => x[0] === mm[2]);
    ok(view && mm[4].startsWith(view[1]), "行き先が MENUS に無い: " + m + "/" + v + " → " + a);
  }
  ok(run('goLink("study", "renewal")') === '<a class="golink" href="#monthly/results?at=rs-renewal">成果と継続 → 継続回数 × 成果</a>',
    "消えた画面へのリンクが「移り先の画面 → 前の画面の節」の名前になっていない: " + run('goLink("study", "renewal")'));
  ok(run('goLink("monthly", "trust")') === '<a class="golink" href="#monthly/trust">記録と数字の信頼度</a>',
    "リンクの文が画面の名前になっていない: " + run('goLink("monthly", "trust")'));
  /* 2026-09-29 組み替え（09 の 7）: 成果と継続（画面の鍵 results）へは、前の区切りの鍵（study）で呼ばれても区切りを直してリンクにする */
  ok(run('goLink("study", "results")') === '<a class="golink" href="#monthly/results">成果と継続</a>',
    "成果と継続へのリンクが画面の名前になっていない: " + run('goLink("study", "results")'));
  ok(!run('goLink("study", "nope")').includes("<a"), "行き先が無いのにリンクにしている");
  // 区切りの鍵は省いてよくなった（画面の鍵は全区切りで一意）ので、区切りだけ無い形ではなく、画面も無い形で見る
  for (const bad of ['goLink("study", "nope")', 'goLink("nope", "nope2")'])
    ok(!/[a-z]{3,}/.test(run(bad)), "行き先が無いときに内部の key（英字）を本文に出している: " + run(bad));
  // サーバが作って画面に出す文（routes.rs の文字列）にも、古い丸数字を残さない。
  // goLink は JS の文しか直さないので、サーバの文は別に見る（2026-09-24: houjin の既定の理由に「①今日動く先」）
  const rsText = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/routes.rs"), "utf-8")
    .replace(/^\s*\/\/.*$/gm, "");
  const circled = rsText.match(/.*[①-⑳].*/g) || [];
  ok(!circled.length, "サーバの文に丸数字が残っている: " + circled.map((l) => l.trim()).join(" / "));
  ok(/a\.golink\{/.test(html), "a.golink の CSS が無い");
  // 並べ替える前の番号「⑧成果」「①で」「③顧客詳細」や、メニューのたどり方の古い書き方を本文に残さない
  for (const w of ["⑧成果", "は①で", "③顧客詳細", "③放置", "④収益", "③ 放置", "「②担当者の一覧」", "「集計」の中の",
                   "「案件」の中の", "左の「"])
    ok(!jsNoComment.includes(w), "本文に古い番号・たどり方「" + w + "」が残っている");
});

// 🔴 図「MTG が途絶えている先」は 2026-09-29 の組み替え（段A、handover 09 の 3章 1）で今日から外した（案件一覧の帯の絞り込みと同じ中身）。
//    図にだけ書いていたこと（線引き・帯を付けていない件数・母数）は畳み「MTG 途絶の数え方と母数」（todayMtgNote）に移したので、
//    同じ性質（中の仕組みの名前を出さない・出す帯は件数のあるものだけ）をそこで見る。凡例の色（紫にしない）は図と一緒に無くなった
check("today: MTG 途絶の数え方に中の仕組みの名前（GAS・no_mtg_alerter）を出さない。件数 0 の帯は書かない", () => {
  ctx.__TDg = { rows: [], meta: { n_hit: 0, n_shown: 0, filter_rule: "", order_rule: "",
    mtg_gap: { rule: "", no_record_note: "", source_note: "", coverage: {},
      bands: [{ band: "critical", label: "重大 90日以上", n: 3, alert: true },
              { band: "yellow", label: "注意 30〜59日", n: 5, alert: true },
              { band: "red", label: "警告 60〜89日", n: 0, alert: true }] } } };
  const h = run("renderToday(__TDg)");
  ok(!/GAS|no_mtg_alerter/.test(h), "today に内部の仕組みの名前が出ている");
  ok(h.includes("毎朝 Slack に届く MTG 途絶の警告と同じ"), "線引きが何と同じかを現場の言葉で書いていない");
  ok(!h.includes("<figcaption>最終MTGからの経過日数で分けた帯"), "外した図（MTG 途絶の帯）が残っている");
  const g = h.slice(h.indexOf('id="td-mtg-note"'), h.indexOf("</details>", h.indexOf('id="td-mtg-note"')));
  ok(g.includes("重大 90日以上 3件") && g.includes("注意 30〜59日 5件"), "件数のある帯が数え方の畳みに無い: " + g);
  ok(!g.includes("60〜89日"), "件数 0 の帯を書いている");
  ok(!run('mtgCell({ mtg_band: "yellow", mtg_days: 40 })').includes("murasaki"), "表の「最後のMTG」で注意を紫にしている");
});

// 2026-09-29 組み替え 段A（handover 09 の 3章 8）: 「データ品質」「MTG の品質」「定義と検証」を 1 画面「記録と数字の信頼度」に。
// 🔴 中身は全部残す。前の 3 画面の描画の中身（見出しを除く）がそのまま入り、画面の問いは 1 つ、節は 09 の順（欠け・偏り → MTG → 定義）
check("記録と数字の信頼度: データ品質・MTG の品質・定義と検証の中身を全部、09 の順で 1 画面に。画面の問いは 1 つで、節へ飛ぶ目次がある", () => {
  ctx.__TRd = Object.assign({}, ctx.__DQ, { _more: { mtgq: ctx.__MQ } });
  const h = run("viewOf('monthly', 'trust').render(__TRd)");
  /* 図の番号（data-fk。描いた順の通し番号）は描くたびに変わるので外して比べる */
  const noHead = (s) => s.replace(/<h2 [^>]*>[\s\S]*?<\/h2>/g, "").replace(/ data-fk="\d+"/g, "");
  for (const [name, code] of [["データ品質", "renderDq(__DQ)"], ["MTG の品質", "renderMtgQ(__MQ)"], ["定義と検証", "renderDefs()"]]) {
    const part = run(code);
    ok(noHead(h).includes(noHead(part)), name + " の中身が欠けている（見出し以外が一致しない）");
    for (const m of part.matchAll(/<h2 [^>]*>(?:<span class="no">[^<]*<\/span>)?([^<]+)<\/h2>/g))
      ok(h.includes(m[1] + "</h2>"), name + " の見出し「" + m[1] + "」が無い");
  }
  ok((h.match(/<h2 class="sec mincho"/g) || []).length === 1, "画面の問い（h2.sec.mincho で .mid でないもの）が 1 つでない");
  const ids = ["trust-dq", "trust-mtgq", "trust-defs"].map((id) => h.indexOf('<h2 class="sec mincho mid" id="' + id + '"'));
  ok(ids.every((i) => i > 0) && ids[0] < ids[1] && ids[1] < ids[2], "節（trust-dq → trust-mtgq → trust-defs）の順が違うか、節の見出しが無い: " + ids);
  for (const id of ["trust-dq", "trust-mtgq", "trust-defs"])
    ok(h.indexOf('data-jump="' + id + '"') > 0 && h.indexOf('data-jump="' + id + '"') < ids[0], "頭の目次に " + id + " への行き先が無い");
  ok(/\.toc\{/.test(html) && /h2\.sec\.mincho\.mid\{/.test(html), "目次（.toc）・節の見出し（h2.sec.mincho.mid）の CSS が無い");
  // MTG の品質の応答が無いとき（形の違う応答）は、黙って節を消さず、無いと書く
  const h2 = run("viewOf('monthly', 'trust').render(__DQ)");
  ok(h2.includes('id="trust-mtgq"') && h2.includes("MTG の品質 のデータがありません"), "MTG の品質の応答が無いのに黙って節を消している");
});

// 2026-09-29 組み替え 段A の検証: 前の画面を節として並べた画面（顧客・チームと担当・成果と継続・記録と数字の信頼度）で、
// 画面の問いがすぐ下の節の問いとほぼ同じ文だと、続けて 2 回読ませる（顧客「この法人は拠点ごとにどう違うか」、
// チームと担当「どこに手が回っていないか」）。画面の問いと、並べた節の描画の問いが、7 字以上続けて同じにならないこと。
// 🔴 句読点・中黒・空白は外して比べる（「継続を重ねると、成果は落ちるのか」と「継続を重ねると成果は落ちるか」を同じと見る）
check("組み替えた画面の問いが、並べた節の問いと同じ文になっていない（続けて 2 回読ませない）", () => {
  const bodyOf = (name) => {
    const i = html.indexOf("\nfunction " + name + "(");
    ok(i >= 0, "関数 " + name + " が無い");
    return html.slice(i + 1).split("\nfunction ")[0];
  };
  /* 問いは文字列か、定数（顧客の画面の CUST_Q）で書かれる。定数ならその値を読む */
  const firstQ = (body) => {
    const m = body.match(/sec\("問い", (?:"([^"]+)"|([A-Z_]+))[,)]/);
    if (!m) return "";
    if (m[1]) return m[1];
    const c = html.match(new RegExp("^const " + m[2] + " = \"([^\"]+)\"", "m"));
    return c ? c[1] : "";
  };
  const norm = (q) => q.replace(/[、。・\s—-]/g, "");
  const common = (a, b) => {
    let best = "";
    for (let i = 0; i < a.length; i++)
      for (let j = i + best.length + 1; j <= a.length; j++) { if (b.includes(a.slice(i, j))) best = a.slice(i, j); else break; }
    return best;
  };
  const screens = [
    /* 顧客は 2026-09-29 の統合で仮のつなぎ（前の 2 画面を並べる customerInterim）から renderCustomer に替わった。
       問いを持つ節（本部アプローチ hqSection）を並べていたが、成果と継続へ移したので顧客からは外した（2026-09-29 横断レビュー）。
       顧客の画面に並べる節で問いを持つものは無い */
    /* チームと担当・成果と継続も段A の統合で仮のつなぎ（teamInterim / resultsInterim）から各チームの描画に替わった。
       並べる節は描画の中で呼ぶもの */
    ["renderTeam", ["teamContact", "renderHandover"]],
    ["renderResults", ["renderRenewal", "renderOutcome", "renderRampup"]],
    ["renderTrust", ["renderDq", "renderMtgQ", "renderDefs"]],
  ];
  for (const [scr, parts] of screens) {
    const body = bodyOf(scr), q = firstQ(body);
    ok(q, scr + " に画面の問いが無い");
    for (const pt of parts) {
      ok(body.includes(pt + "("), "前提: " + scr + " が " + pt + " を並べていない");
      const pq = firstQ(bodyOf(pt));
      ok(pq, "前提: " + pt + " に問いが無い");
      const c = common(norm(q), norm(pq));
      ok(c.length < 7, scr + " の問い「" + q + "」が、節 " + pt + " の問い「" + pq + "」と「" + c + "」まで同じ");
    }
  }
});

check("凡例: MTG の品質・データ品質で、図に出ていない色を凡例に出さない", () => {
  const q = run("renderMtgQ(__MQ)");   // filled は rate 10% の1項目だけ
  const qf = q.split("<figcaption>抽出の進み具合")[1].split("</figure>")[0];
  ok(qf.includes("15%未満") && !qf.includes("50%以上") && !qf.includes("15〜50%"), "抽出の進み具合の凡例に使っていない帯がある");
  const dq = JSON.parse(JSON.stringify(ctx.__DQ));
  dq.missing = [{ label: "応募数", n: 5, rate: 3, note: "" }];
  ctx.__DQm = dq;
  const d = run("renderDq(__DQm)");
  ok(d.includes("20%未満") && !d.includes("20%以上"), "データ品質の凡例に使っていない山吹（20%以上）がある");
});

check("focus: NPS が低い表は全行に同じ印（▲）を付け、LTV の軸に単位を書く", () => {
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  fo.nps_low.rows = [0, 1].map((v, i) => ({ deal_id: "d" + i, name: "案件" + i, stage: "", nps: v,
    nps_month: "2026-09", amount: 1, days_to_expiry: 10, n_contact: 1 }));
  fo.shape.ltv = { n: 40, min: 1e5, q1: 2e5, median: 3e5, q3: 4e5, max: 9e5, mean: 3.5e5 };
  ctx.__FOn = fo;
  const h = run("renderFocus(__FOn)");
  ok(h.includes('<b style="color:var(--hi)">&#9650; 0</b>') && h.includes('<b style="color:var(--hi)">&#9650; 1</b>'),
    "NPS 0 と 1 で印が違う（1 が黒く見える）");
  ok(h.includes("LTV（万円）"), "LTV の軸に単位が無い");
  ok(!/Q3 /.test(h), "KPI に英字の略号 Q3 が残っている");
});

check("単位: 金額・採用単価の縦軸の題名に（万円）を書く", () => {
  ok(!/yLab: "(金額|採用単価)"/.test(jsNoComment), "縦軸の題名に単位が無い（目盛りは万円の数字だけ）");
  ok(jsNoComment.includes('yLab: "採用単価（万円）"') && jsNoComment.includes('yLab: "金額（万円）"'), "単位を付けた題名が無い");
});

check("表: 長い文字の列だけ折り返し（1440px で右端が切れない）、数字の列は1行のまま", () => {
  const t = run('table([{ t: "案件", w: "l" }, { t: "担当", w: "s" }, { t: "金額", n: 1 }, { t: "日付" }], [["あ", "い", "1", "2026-01-01"]])');
  ok(t.includes('<td class="wl">あ</td>') && t.includes('<td class="ws">い</td>'), "折り返す列に wl / ws が付かない");
  ok(t.includes('<td class="n">1</td>') && t.includes("<td>2026-01-01</td>"), "数字・日付の列の扱いが変わった");
  ok(/td\.wl\{[^}]*white-space:normal/.test(html) && /td\.ws\{[^}]*white-space:normal/.test(html), "wl / ws の CSS が無い");
  const ho = run("renderHandover(__HO)");
  ok(ho.includes('<th class="wl">案件</th>'), "担当の交代の表で「案件」の列が折り返さない");
  for (const c of ["前の担当", "次の担当", "いまの担当"])
    ok(ho.includes('<th class="ws">' + c + "</th>"), "担当の交代の表で「" + c + "」が折り返さない");
  // いま見るべき顧客の表と、見方で外れた案件の表（段B で成果とリスクの最優先の表の代わりになったもの）も、取引名の列を折り返す
  ok(run("renderFocus(__FO)").includes('<th class="wl">取引</th>'), "いま見るべき顧客の表で取引名が折り返さない");
  ctx.__WLV = { rows: [], meta: { act_views: [{ key: "top", label: "L", rule: "R", n: 0 }],
    act_view_diff: [{ key: "top", n: 0, old: { where: "W", n: 1, kept: 0, dropped: 1, added: 0,
      dropped_rows: [{ deal_id: "d1", name: "外れた案件", consultant: "田中", reason: "理由" }] } }] } };
  const saved = run("JSON.stringify(boardFilter)");
  try {
    run('boardFilter = Object.assign({}, boardFilter, { view: "top" });');
    ok(run("boardViewNote(__WLV)").includes('<th class="wl">取引</th>'), "見方で外れた案件の表で取引名が折り返さない");
  } finally { run("boardFilter = " + saved + ";"); }
});

check("series: 推移の図で、契約の頭に変更履歴が無い月を黙って欠けさせない", () => {
  // fixture でも 2025-03-09 開始の契約は変更履歴が 25-07 からの1点だけ（2026-09-24 実測）
  const g = run('histGap({ start: "2025-03-09" }, [5, 6])');
  ok(g.includes("記録は 2025-07 からです") && g.includes("2025-03〜2025-06"), "記録が無い期間を書いていない: " + g);
  // データ全体の履歴が無いと決めつけない（その契約の値が入っていなかっただけのこともある）
  ok(!g.includes("しかありません"), "記録が無い理由を「変更履歴が無い」と決めつけている: " + g);
  ok(g.includes("0 ではありません"), "描いていない月を 0 と読ませない断りが無い");
  ok(run('histGap({ start: "2025-03-09" }, [2, 3])').includes("2025-03 は記録が無い"), "1か月だけ無いときの書き方が崩れる");
  ok(run('histGap({ start: "2025-03-09" }, [1, 2])') === "", "始月から記録があるのに断り書きを出している");
  ok(run('histGap({ start: "" }, [3])') === "", "開始日が無いのに断り書きを作っている");
  ok(/const gap = histGap\(mm, months\)/.test(jsNoComment), "推移の図が histGap を使っていない");
});

check("採用単価の図: 採れた人数を値の横に括弧で付ける（「162万2人」とくっつけない）", () => {
  const g = cpa3Fig();
  ok(g.includes(run("yen(900000)") + "（2人）"), "採れた人数が値の横の括弧に無い");
  ok(!/[万円]2人/.test(g) && !/>　2人/.test(g), "採れた人数が値にくっついている / 注記の頭に残っている");
});

check("team: 稼働中の件数を KPI と末尾で繰り返さず、40% 未満の人数に「母数が小さい人を除く」と書く", () => {
  const h = run("renderTeam(teamOf(__D))");
  ok(!h.includes("稼働中 604 件（オプション契約を除く）"), "稼働中の件数を画面の中で繰り返している（頭の1行に任せる）");
  ok(textOf(h).includes("母数が小さい 5 名は数えていません"), "40% 未満の人数が、表の母数が小さい人（0.0%）と食い違って見える");
  ok(h.split('<div class="note def">').pop().includes("担当者 27 名"), "末尾の枠にこの画面の数（担当者の人数）が無い");
  ok(h.includes("この担当の案件だけを「担当者ごとの案件」で見る"), "名前の title が行き先の画面名と合っていない");
});

check("色と印の意味: ▲▼ は良し悪しの向きで、表の見出しの ▲▼（並び順）とは別だと書く", () => {
  // M-1 の (3)（2026-09-29）: 「色と印の意味」はヘッダの畳みから定義と検証の表へ 1 か所に。断りはその表で見る
  const dl = run("renderDefs()");
  const dLeg = dl.slice(dl.indexOf("色と印の意味"), dl.indexOf("この画面が守っていること"));
  ok(/&#9650;<\/td><td[^>]*>まずい \/ 悪化。<b>値の上がり下がりではなく良し悪しの向き<\/b>/.test(dLeg) || dLeg.includes("まずい / 悪化。<b>値の上がり下がりではなく良し悪しの向き</b>"),
    "凡例（定義と検証の色と印の意味）に ▲ の意味の断りが無い");
  ok(dLeg.includes("表の見出しの &#9650; / &#9660;") && dLeg.includes("並び順（小さい順 / 大きい順）。良し悪しではありません"), "凡例に表の見出しの ▲▼ の断りが無い");
  ok(html.includes('<a class="golink" id="cs-legend" href="#monthly/trust?at=trust-defs" title="記録と数字の信頼度の「色と印の意味」の表へ">色と印の意味 →</a>'), "ヘッダから凡例（定義と検証）へのリンクが無い");
  // その注記（<i class="full">）が1行まるごと使う。.figlegend 用の定義しか無く、横に並んでいた（2026-09-24 検証）
  ok(/\.legend i\.full\{[^}]*flex:1 0 100%/.test(html), "「色と印の意味」の注記（.legend i.full）が1行を占める CSS が無い");
  const d = run("renderDefs()");
  ok(d.includes("値の上がり下がりではなく良し悪しの向き") && d.includes("並び順（小さい順 / 大きい順）"), "定義と検証の表に ▲▼ の断りが無い");
});

check("言い回し: 「結べた」「結べていない」を画面の文に出さない（「紐づいた」）", () => {
  ok(!/結べ/.test(jsNoComment), "画面の JS の文に「結べ」が残っている");
  const rs = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/routes.rs"), "utf-8")
    .replace(/^\s*\/\/.*$/gm, "");
  ok(!/結べ/.test(rs), "サーバ（routes.rs）の文に「結べ」が残っている");
  ok(run("renderMtgQ(__MQ)").includes("取引に紐づいた"), "MTG の品質の KPI が「取引に紐づいた」でない");
});

check("互換: 正規表現の後読みを使わない（Safari 16.4 より前は <script> 全体が構文エラーで動かない）", () => {
  ok(!/\(\?<[=!]/.test(jsNoComment), "画面の JS に後読み (?<= / (?<! の正規表現がある");
  const lines = run('JSON.stringify(wrapText("あいう えお/かき", 9999))');
  ok(lines && lines.includes("あいう えお/かき"), "wrapText が1行に収まる文をそのまま返さない: " + lines);
});

/* ================================================================ 図の部品（ループ4 の実機確認, 2026-09-24） */
check("ループ4: ラベルを省略するとき、見分けの語（日数・拠点名の固有部分）を残す", () => {
  const w = 90;
  ctx.__DS = ["MTGが90日以上途絶", "MTGが60〜89日途絶", "MTGが30〜59日途絶"];
  const cut = run("__DS.map((s) => fitLab(s, " + w + "))");
  ok(cut.every((s, i) => s.includes(["90日", "60〜89日", "30〜59日"][i])), "日数が消えた: " + cut.join(" / "));
  ok(new Set(cut).size === 3, "省略した結果が同じになった: " + cut.join(" / "));
  const tw = run("textW");
  ok(cut.every((s) => tw(s) <= w), "幅に収まっていない: " + cut.join(" / "));
  ctx.__SN = ["サブスク継続②＿ニッコン 富山営業所", "サブスク継続②＿ニッコン 岡山営業所", "ロンコ・ジャパンプラス事業部", "ロンコ・ジャパンコール事業部"];
  const sn = run("__SN.map((s) => fitLab(s, " + w + "))");
  ok(sn[0].includes("富山営業所") && sn[1].includes("岡山営業所"), "拠点名（最後の語）が残っていない: " + sn.join(" / "));
  ok(sn[2].includes("プラス事業部") && sn[3].includes("コール事業部"), "区切りの無い拠点名で、末尾の語の手前が残っていない: " + sn.join(" / "));
  ok(new Set(sn).size === 4, "省略した結果が同じになった: " + sn.join(" / "));
});

check("ループ4: 狭い枠（319px）の帯のラベルは、省略せずに2行に折り返す（today の「MTGが90日以上途絶」）", () => {
  const code = "svgBarH({ w: 680, rows: [" + ["MTGが90日以上途絶", "MTGが60〜89日途絶", "直近30日にMTGあり", "立ち上がり期（契約開始30日以内）"]
    .map((l, i) => '{ label: "' + l + '", v: ' + (10 + i) + ', txt: "' + (10 + i) + '件", note: "手を打つ" }').join(",") + "] })";
  const svg = drawAt(code, 319);
  ok(figW(svg) === 319 && /data-under/.test(svg), "319px の枠で注記を下に回して描いていない");
  const parts = [...svg.matchAll(/<text class="axl"[^>]*data-wrap="2"[^>]*><tspan[^>]*>([^<]*)<\/tspan><tspan[^>]*>([^<]*)<\/tspan>/g)];
  const labs = parts.map((m) => m[1] + m[2]);
  ok(labs.length >= 3, "長いラベルが2行に折り返されていない: " + labs.join(" / "));
  ok(labs.includes("MTGが90日以上途絶") && labs.includes("MTGが60〜89日途絶"), "折り返したラベルで字が落ちた: " + labs.join(" / "));
  // 2行目の頭に閉じ括弧を置かない・数字の途中で切らない・行は欄に収まる
  const tw = run("textW");
  ok(parts.every((m) => !/^[）」、。]/.test(m[2]) && !(/[0-9〜]$/.test(m[1]) && /^[0-9〜日%]/.test(m[2]))), "折り返しの位置が数字・括弧の途中: " +
    parts.map((m) => m[1] + " | " + m[2]).join(" / "));
  ok(!/…/.test(labs.slice(0, 3).join("")), "2行に入るラベルを省略した: " + labs.join(" / "));
  ok(parts.every((m) => tw(m[1]) <= 319 * .36), "折り返した行がラベルの欄に入っていない");
  // 1行に入るラベルは折り返さない
  ok(!/data-wrap/.test(drawAt('svgBarH({ w: 680, rows: [{ label: "初回", v: 1, note: "n=3" }] })', 319)), "収まるラベルまで折り返している");
});

check("ループ4: 前回の値が 0 付近で枠を描かなかった行も、houjin の採用単価では月割りの縦の線（墨）になる", () => {
  // svgBarH は枠の代わりに縦の破線（data-v0tick）を置く。cpa3Bars がそれを月割りの線に描き替えないと、破線が残る
  const rows = [{ label: "a", v: 9000000, v0: 20000, txt: "900万", color: "var(--ai)" }];
  ctx.__R3 = rows;
  const svg = run("cpa3Bars(svgBarH({ w: 680, fmt: F.man, rows: __R3 }), __R3)");
  ok(!/data-v0tick/.test(svg), "月割りが 0 付近の行に、前回の縦の破線が残っている");
  const m = svg.match(/<line x1="([\d.]+)"[^>]*data-mark="monthly">/);
  const r = svg.match(/<rect x="([\d.]+)" y="[\d.]+" width="([\d.]+)" height="[\d.]+" rx="2" style="fill:/);
  ok(m && r && Math.abs(+m[1] - (+r[1] + +r[2] * 20000 / 9000000)) < 0.6, "月割りの線が本当の位置に無い: " + (m && m[1]));
});

check("ループ4: 注記の折り返しは、数字・単位・括弧の途中で切らない（「50〜7 / 5%」「6.7 / 倍」「（決着済み / 1件中）」）", () => {
  const W = (s, w) => run("wrapText(" + JSON.stringify(s) + ", " + w + ")");
  const s1 = "進捗帯「後半にさしかかり（50〜75%）」の中央値 60万", s2 = "解約・充足 100%（決着済み 1件中）　2人";
  const a = W(s1, 150), b = W(s2, 120), c = W("拠点3 / 取引6 / 開き 6.7倍", 60);
  ok(a.some((l) => l.includes("50〜75%")), "「50〜75%」が割れた: " + a.join(" | "));
  ok(b.some((l) => l.includes("（決着済み 1件中）")), "括弧の中で折れた: " + b.join(" | "));
  ok(c.some((l) => l.includes("6.7倍")), "「6.7倍」が割れた: " + c.join(" | "));
  // 切れ目が数字の途中・括弧の中にちょうど来る幅でも割らない
  const d = run('wrapText("ああああ12.5倍いいい", textW("ああああ12."))');
  ok(d.some((l) => l.includes("12.5倍")), "幅の境目が数字の途中に来ると「12.5倍」が割れる: " + d.join(" | "));
  const e = run('wrapText("解約・充足 100%（決着済み 1件中）　2人", textW("解約・充足 100%（決着済み ") + 4)');
  ok(e.some((l) => l.includes("（決着済み 1件中）")), "括弧の途中まで行に入る幅だと括弧の中で折れる: " + e.join(" | "));
  ok(a.join("").replace(/\s/g, "") === s1.replace(/\s/g, "") && b.join("").replace(/\s/g, "") === s2.replace(/\s/g, ""),
    "折り返しで字を落とした: " + a.join(" | ") + " / " + b.join(" | "));
});

check("ループ4: 箱ひげは 319px の枠に収め、軸の外の最大値は軸の右端の途切れの印と「最大 N（軸の外）」で示す。◇ を凡例で説明する", () => {
  const rows = '[{ label: "継続した", med: 7.4, q1: 3.3, q3: 11.4, min: 0.1, max: 163.7, mean: 12.9, n: 742 },' +
    '{ label: "解約した", med: 5.1, q1: 2, q3: 8.2, min: 0, max: 74, mean: 8.3, n: 529 },' +
    '{ label: "充足（採れて終わった）", med: 9.8, q1: 5, q3: 17.3, min: 0.5, max: 121, mean: 16.4, n: 142 }]';
  const svg = drawAt("svgBoxH({ w: 680, xFmt: F.d1, rows: " + rows + " })", 319);
  ok(figW(svg) === 319, "319px の枠に収まっていない（横スクロールで箱と n= が奥に隠れる）: " + figW(svg));
  ok(["n=742", "n=529", "n=142"].every((t) => svg.includes(">" + t + "<")), "n= が欠けた");
  const boxes = textBoxes(svg);
  ok(boxes.every((b) => b.x1 <= 319 + 1 && b.x0 >= -0.5), "枠の外に出る文字: " + boxes.filter((b) => b.x1 > 320 || b.x0 < -0.5).map((b) => b.s).join(" / "));
  ok(!overlaps(svg).length, "文字が重なる: " + overlaps(svg).join(" / "));
  // 軸の右端（いちばん右の目盛りの線）より右に途切れの印を置く
  const gx = Math.max(...[...svg.matchAll(/<line class="gridline" x1="([\d.]+)"/g)].map((m) => +m[1]));
  const marks = [...svg.matchAll(/<path data-boxout="1" d="M([\d.]+) /g)].map((m) => +m[1]);
  ok(marks.length === 3 && marks.every((x) => x >= gx), "途切れの印が軸の右端に無い（軸の途中に置くと最大値の位置に見える）: " + marks + " / 軸の端 " + gx);
  const mx = boxes.filter((b) => b.s.startsWith("最大"));
  ok(mx.length === 3 && mx.every((b) => b.s.includes("軸の外")), "「最大 N（軸の外）」になっていない: " + mx.map((b) => b.s).join(" / "));
  // 凡例: ◇（平均）と、軸の外の印の意味
  const f = run('fig("応募数 ÷ 掲載数", "", svgBoxH({ w: 680, xFmt: F.d1, rows: ' + rows + " }))");
  ok(f.includes("平均（") && f.includes("最大値が軸の右の外"), "箱ひげの凡例に ◇（平均）か軸の外の印の説明が無い");
  // ひげの外でも軸の中に収まる最大値は、本当の位置に小さな丸を置く（軸の外とは書かない）
  const inAx = run('svgBoxH({ w: 680, rows: [{ label: "a", med: 5, q1: 4, q3: 6, min: 3, max: 20, n: 40 }, { label: "b", med: 20, q1: 10, q3: 30, min: 0, max: 60, n: 40 }] })');
  ok(/data-boxmax="1"/.test(inAx) && !/軸の外/.test(inAx.replace(/<title>[^<]*<\/title>/g, "")), "軸の中の最大値を「軸の外」と書いた・印が無い");
});

check("ループ4: 時間軸の図（契約の連なり・LTV・月次の継続率）は、開いた直後を新しい側に合わせる（data-xr）", () => {
  const xr = (svg) => { const m = String(svg).match(/<svg [^>]*data-xr="(\d+)"/); return m ? +m[1] : null; };
  const tl = drawAt('svgTimeline({ w: 940, lanes: [{ label: "a", note: "3回目 108万", marks: [{ d: "2025-01-01" }, { d: "2026-06-01" }] }] })', 319);
  ok(xr(tl) === 940, "契約の連なりの data-xr が右端（新しい側・右の注記）でない: " + xr(tl));
  const mx = JSON.stringify(monthsN(24));
  const all = JSON.stringify(monthsN(24).map((_, i) => ({ v: 1000000 * (i + 1) })));
  const col = drawAt("svgColStack({ w: 940, x: " + mx + ', series: [{ label: "s", color: "blue", vals: ' + all + ' }], yFmt: F.man, yLab: "累計金額（万円）" })', 319);
  ok(xr(col) > 900, "LTV の推移の data-xr が最新の柱でない: " + xr(col));
  const sl = col.match(/class="sticklab" viewBox="0 0 (\d+)/);
  ok(sl && +sl[1] >= run('textW("累計金額（万円）")'), "新しい側に合わせた積み上げ縦棒で、縦軸の目盛りと題名を左に貼り付けていない（題名が切れる）");
  const line = drawAt("svgLine({ x: " + mx + ", series: [{ pts: " + all + ' }], yLab: "継続率" })', 319);
  ok(xr(line) > 600, "月次の継続率（折れ線）の data-xr が最新の月でない: " + xr(line));
  // figToData は data-xr を x0 より優先し、その位置を右端に見せる
  const b = { scrollWidth: 1000, clientWidth: 334, scrollLeft: 0 };
  b.querySelector = () => ({ viewBox: { baseVal: { width: 1000 } }, getBoundingClientRect: () => ({ width: 1000 }),
    getAttribute: (a) => (a === "data-xr" ? "900" : a === "data-x0" ? "10" : null) });
  ctx.__EL = { querySelectorAll: () => [b] };
  run("figToData(__EL)");
  ok(b.scrollLeft === 900 - 334, "data-xr の位置を右端に見せていない: " + b.scrollLeft);
});

check("ループ4: 月が少ない帯を縦に積む図（甲賀＝1か月）は枠の幅で描き、「記録がありません」が左・右にはみ出さない", () => {
  const svg = drawAt('svgStackLanes({ w: 940, months: ["25-09"], lanes: [' +
    '{ type: "line", label: "応募", color: "blue", pts: [{ v: 19 }] },' +
    '{ type: "dots", label: "定期NPS", empty: true, pts: [] },' +
    '{ type: "bars", label: "接触（MTG・60秒超の通話）", color: "gray", fillLabel: "a", outLabel: "b", pts: [{ fill: 1, out: 2 }] }] })', 319);
  ok(figW(svg) === 319, "1か月の図を枠の幅で描いていない: " + figW(svg));
  ok(!/<svg [^>]*data-x0=/.test(svg), "枠の幅で描いた図に、横スクロールの位置合わせ（data-x0）が残っている");
  const pl = +(svg.match(/class="sticklab" viewBox="0 0 ([\d.]+)/) || [0, 0])[1];
  const em = [...svg.matchAll(/<text class="ax" data-empty="1" x="([\d.]+)"[^>]*>([^<]*)</g)];
  const tw = run("textW");
  ok(em.length >= 1 && em.every((m) => +m[1] >= pl && +m[1] + tw(m[2]) <= 319 + 1), "「記録がありません」がラベルの欄か枠の右にはみ出す: " +
    em.map((m) => m[1] + ":" + m[2]).join(" / ") + "（ラベル欄 " + pl + "）");
  const vals = textBoxes(svg).filter((b) => /^\d+$/.test(b.s));
  ok(vals.length && vals.every((b) => b.x1 <= 319 + 1), "右端の目盛りの値が枠の外: " + vals.map((b) => b.s + "@" + b.x0.toFixed(0)).join(" "));
});

check("ループ4: 注力の点の図は枠の幅に合わせて並べる（1440px で左半分で終わらない・400px で右が奥に隠れない）", () => {
  const code = 'svgDots({ total: 517, per: 37, groups: [{ v: 116, color: "blue", label: "注力" }] })';
  const wide = figW(drawAt(code, 1116)), narrow = figW(drawAt(code, 319));
  ok(wide > 1116 * .9 && wide <= 1116, "1116px の枠で点の図が広がらない: " + wide);
  ok(narrow <= 319, "319px の枠から点の図がはみ出す: " + narrow);
  ok((drawAt(code, 319).match(/<circle /g) || []).length === 517, "点の数が変わった");
});

check("ループ4: 散布図の横軸は 0 から始め、いちばん右の点を枠の端に置かない（outcome の最優先の広がり）", () => {
  const svg = drawAt('svgScatter({ w: 680, pts: [{ x: 3, y: 60 }, { x: 57, y: 90 }, { x: 25, y: 300 }], xLab: "満了まで（日）" })', 319);
  const tb = textBoxes(svg);
  const axisY = Math.max(...tb.map((b) => b.y1));
  const xt = [...svg.matchAll(/<text class="ax" x="[\d.]+" y="([\d.]+)"[^>]*>([^<]*)</g)].filter((m) => +m[1] + 2 >= axisY - 30).map((m) => m[2]);
  ok(xt.includes("0"), "横軸に 0 が無い: " + xt.join(","));
  const cx = Math.max(...[...svg.matchAll(/<circle cx="([\d.]+)"/g)].map((m) => +m[1]));
  ok(cx <= 319 - 18 - 8, "いちばん右の点が枠の右端に寄っている（右の目盛りまで伸ばしていない）: " + cx);
  ok(tb.every((b) => b.x1 <= 319 + 1), "目盛りの字が枠の外に出る");
});

/* ================================================================ 図の部品（ループ4 の検証の指摘, 2026-09-24） */
check("ループ4 検証: labWrap は括弧の中・語の途中・助詞の手前で切らない（2行目が入るなら語の境目を選ぶ）", () => {
  const W = (s, w) => { const r = run("labWrap(" + JSON.stringify(s) + ", " + w + ")"); return r ? r.join(" / ") : null; };
  // 385px の実測で切れていた位置（outcome の箱ひげ・today の帯・名札の図）。枠 319px の帯のラベル欄はおよそ 115px
  ok(W("充足（採れて終わった）", 114.8) === "充足 / （採れて終わった）", "括弧の中で切った: " + W("充足（採れて終わった）", 114.8));
  ok(W("充足（採れて終わった）", 100) === "充足（採れて / 終わった）", "括弧の外で切れないとき、括弧の中でも語の境目を選んでいない: " + W("充足（採れて終わった）", 100));
  ok(W("採用単価が同じ進捗帯の1.5倍以上", 114.8) === "採用単価が同じ / 進捗帯の1.5倍以上", "語の途中で切った: " + W("採用単価が同じ進捗帯の1.5倍以上", 114.8));
  ok(W("立ち上がり期（契約開始30日以内）", 114.8) === "立ち上がり期（契約 / 開始30日以内）", "漢字の熟語の途中で切った: " + W("立ち上がり期（契約開始30日以内）", 114.8));
  ok(W("満了90日前でMTGが30日以上途絶", 114.8) === "満了90日前で / MTGが30日以上途絶", "行の頭に助詞を置いた: " + W("満了90日前でMTGが30日以上途絶", 114.8));
  // 助詞の後ろ（「が」）を優先する。英字・「以上」は割らない
  ok(W("MTGが90日以上途絶", 100) === "MTGが / 90日以上途絶", "助詞の後ろで切っていない: " + W("MTGが90日以上途絶", 100));
  // カタカナの語の途中（「アン / ケート」）より、語の境目（「アンケート / 未回答率」）を選ぶ
  ok(W("NPSアンケート未回答率", 90) === "NPSアンケート / 未回答率", "カタカナの語の途中で切った: " + W("NPSアンケート未回答率", 90));
  // 英字の語・「以上」は割らない（wordGlue）
  ok(W("満了90日以上途絶", 65) === "満了90日 / 以上途絶", "「以 / 上」で割った: " + W("満了90日以上途絶", 65));
  ok(!/[A-Za-z] \/ [A-Za-z]/.test(String(W("HubSpot登録", 50))), "英字の語の途中で切った: " + W("HubSpot登録", 50));
});

check("ループ4 検証: 1か月の図の「記録がありません」は語の途中で折らず、月の縦線が文字の上を通らない", () => {
  const svg = drawAt('svgStackLanes({ w: 940, months: ["25-09"], lanes: [' +
    '{ type: "line", label: "応募", color: "blue", pts: [{ v: 19 }] },' +
    '{ type: "dots", label: "定期NPS", empty: true, pts: [] }] })', 319);
  const em = [...svg.matchAll(/<text class="ax" data-empty="1" x="([\d.]+)" y="([\d.]+)"[^>]*>([^<]*)</g)];
  ok(em.map((m) => m[3]).join(" / ") === "— この顧客では / 記録がありません", "折り返しの位置が語の途中: " + em.map((m) => m[3]).join(" / "));
  // 月の縦線（x=X(0)）が文字の範囲に入るなら、文字の下に地の四角を敷く
  const gx = [...svg.matchAll(/<line class="gridline" x1="([\d.]+)"/g)].map((m) => +m[1]);
  const tw = run("textW");
  const bgs = [...svg.matchAll(/<rect data-emptybg="1" x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)"/g)];
  em.forEach((m) => {
    const x0 = +m[1], x1 = x0 + tw(m[3]), y = +m[2];
    if (!gx.some((x) => x > x0 && x < x1)) return;
    ok(bgs.some((b) => +b[1] <= x0 && +b[1] + +b[3] >= x1 && +b[2] <= y - 9 && +b[2] + +b[4] >= y + 2),
      "月の縦線が「" + m[3] + "」の上を通る（地の四角が無い）");
  });
  ok(bgs.length === em.length, "「記録がありません」の地の四角が行の数と合わない: " + bgs.length);
});

check("ループ4 検証: 箱ひげを狭い枠（319px）で描くとき、ラベル欄に入らないラベルは省略せずに2行に折り返す（outcome の「充足」）", () => {
  const svg = drawAt('svgBoxH({ w: 680, xFmt: F.d1, rows: [' +
    '{ label: "継続した", med: 7.4, q1: 3.3, q3: 11.4, min: 0.1, max: 163.7, n: 742 },' +
    '{ label: "充足（採れて終わった）", med: 9.8, q1: 5, q3: 17.3, min: 0.5, max: 121, n: 142 }] })', 319);
  const wr = [...svg.matchAll(/<text class="axl"[^>]*data-wrap="2"[^>]*><tspan[^>]*>([^<]*)<\/tspan><tspan[^>]*>([^<]*)<\/tspan>/g)];
  ok(wr.length === 1 && wr[0][1] + wr[0][2] === "充足（採れて終わった）", "「充足（採れて終わった）」を2行に折り返していない: " +
    [...svg.matchAll(/<text class="axl"[^>]*>(.*?)<\/text>/g)].map((m) => m[1].replace(/<[^>]*>/g, "|")).join(" / "));
  // 行の高さ: 2行に折り返した行は 32px、軸の外の最大値がある行は「最大 N（軸の外）」の行のぶん 12px 足す
  // （27 + 12 = 39）。足りないと2行目・「最大 N」が下の行や目盛りにかかる
  const vh = (t) => +(t.match(/viewBox="0 0 [\d.]+ ([\d.]+)"/) || [0, 0])[1];
  ok(vh(svg) === 22 + 26 + 39 + 39, "軸の外の行・2行のラベルの行の高さが足りない: " + vh(svg));
  const inAx = drawAt('svgBoxH({ w: 680, xFmt: F.d1, rows: [' +
    '{ label: "継続した", med: 7.4, q1: 3.3, q3: 11.4, min: 0.1, max: 20, n: 742 },' +
    '{ label: "充足（採れて終わった）", med: 9.8, q1: 5, q3: 17.3, min: 0.5, max: 25, n: 142 }] })', 319);
  ok(vh(inAx) === 22 + 26 + 27 + 32, "2行のラベルの行が 32px になっていない: " + vh(inAx));
  // 軸の外の最大値: ひげの先から軸の右端（途切れの印）までを細い線でつなぐ（線が無いと、ひげの先が最大値に見える）
  const outs = [...svg.matchAll(/<path data-boxout="1" d="M([\d.]+) ([\d.]+)/g)].map((m) => ({ x: +m[1] - 1, y: +m[2] - 4.5 }));
  const thin = [...svg.matchAll(/<line x1="([\d.]+)" y1="([\d.]+)" x2="([\d.]+)" y2="[\d.]+"[^>]*opacity="\.3"\/>/g)];
  ok(outs.length === 2 && outs.every((o) => thin.some((t) => Math.abs(+t[3] - o.x) < .2 && Math.abs(+t[2] - o.y) < .2 && +t[1] < o.x - 5)),
    "ひげの先から軸の端までの細い線が無い: 印 " + JSON.stringify(outs) + " / 線 " + thin.map((t) => t[1] + "→" + t[3]).join(","));
});

check("ループ4 検証: 最初の月からデータがある折れ線（renewal の月次の継続率）も、右端で開くなら縦軸の目盛りを左に貼り付ける", () => {
  // x0 が小さい（最初の月から値がある）図では、貼り付けるかどうかを data-xr だけが決める
  const mx = JSON.stringify(monthsN(24));
  const all = JSON.stringify(monthsN(24).map((_, i) => ({ v: .5 + i / 100 })));
  const line = drawAt("svgLine({ x: " + mx + ", series: [{ pts: " + all + ' }], yFmt: F.pct, yLab: "継続率" })', 319);
  ok(+(line.match(/<svg [^>]*data-x0="(\d+)"/) || [0, 99])[1] < 60, "最初の月から値があるのに data-x0 が小さくない（条件を確かめられない）");
  ok(/class="sticklab"/.test(line), "右端で開く折れ線で、縦軸の目盛りを左に貼り付けていない（開いた直後に目盛りが流れて見えない）");
  // 貼り付けた目盛りの欄には地の色の四角を敷く（点・線が「50%」の字に重ならない）。
  // 欄は目盛りの字を覆い、図の左端（最初の点 = 縦軸の線の位置）にはかからない
  const bgOf = (svg) => { const st = (svg.match(/<svg [^>]*class="sticklab"[\s\S]*?<\/svg>/) || [""])[0];
    const r = st.match(/<rect data-stickbg="1" x="0" y="0" width="([\d.]+)" height="([\d.]+)"/);
    return { r: r ? { w: +r[1], h: +r[2] } : null, ticks: textBoxes(st).filter((b) => /^\d/.test(b.s)) }; };
  const firstX = (svg) => Math.min(...[...svg.replace(/<svg [^>]*class="sticklab"[\s\S]*?<\/svg>/, "").matchAll(/<circle cx="([\d.]+)"/g)].map((m) => +m[1]));
  const lb = bgOf(line);
  ok(lb.r && lb.ticks.length && lb.ticks.every((b) => b.x1 <= lb.r.w + 1) && lb.r.w < firstX(line),
    "折れ線の貼り付けた目盛りに地の四角が無い・字を覆わない・最初の点にかかる: " + JSON.stringify(lb.r) + " 最初の点 " + firstX(line));
  const col = drawAt("svgColStack({ w: 940, x: " + mx + ', series: [{ label: "s", color: "blue", vals: ' +
    JSON.stringify(monthsN(24).map((_, i) => ({ v: 1000000 * (i + 1) }))) + ' }], yFmt: F.man, yLab: "累計金額（万円）" })', 319);
  const cb = bgOf(col);
  const firstBar = Math.min(...[...col.replace(/<svg [^>]*class="sticklab"[\s\S]*?<\/svg>/, "").matchAll(/<rect x="([\d.]+)" y="[\d.]+" width="[\d.]+" height="[\d.]+"[^>]*fill:blue/g)].map((m) => +m[1]));
  ok(cb.r && cb.ticks.length && cb.ticks.every((b) => b.x1 <= cb.r.w + 1) && cb.r.w < firstBar,
    "LTV の貼り付けた目盛りに地の四角が無い・字を覆わない・最初の棒にかかる: " + JSON.stringify(cb.r) + " 最初の棒 " + firstBar);
  // 枠に収まる幅（1440px）では貼り付けない
  ok(!/class="sticklab"/.test(drawAt("svgLine({ x: " + JSON.stringify(monthsN(6)) + ", series: [{ pts: " +
    JSON.stringify(monthsN(6).map(() => ({ v: .5 }))) + ' }], yFmt: F.pct, yLab: "継続率" })', 1116)), "枠に収まる折れ線まで目盛りを貼り付けた");
});

check("ループ4 検証: 箱ひげの凡例（◇ 平均・軸の外の印）は「四分位 / 中央値」のすぐ後ろに並べ、説明の段落の後ろに回さない", () => {
  ctx.__BX = 'svgBoxH({ w: 680, xFmt: F.d1, rows: [{ label: "a", med: 7.4, q1: 3.3, q3: 11.4, min: 0.1, max: 163.7, mean: 12.9, n: 742 }] })';
  const f = run('fig("応募数 ÷ 掲載数", "", eval(__BX), lg("quart", C.ai, "四分位") + lg("line", C.ai, "中央値") +' +
    ' \'<i class="full">段落その1</i><i class="full">段落その2</i>\')');
  const at = (t) => f.indexOf(t);
  ok(at("中央値") >= 0 && at("平均（") > at("中央値") && at("最大値が軸の右の外") > at("中央値"), "箱ひげの凡例が出ていない");
  ok(at("平均（") < at("段落その1") && at("最大値が軸の右の外") < at("段落その1"), "◇ 平均・軸の外の印の説明が、段落の後ろに離れている");
  // 段落の無い凡例・凡例の無い図では、これまでどおり最後に付く
  ok(run('fig("x", "", eval(__BX))').includes("平均（"), "凡例の無い図で箱ひげの凡例が出ない");
});

check("ループ4 検証: 箱ひげの軸の端を目盛りに合わせても、広い枠では軸を伸ばしすぎない（rampup の初回MTGまで: 0〜150 にしない）", () => {
  // ひげの先 = 47 + 1.5 × 37 = 102.5。刻み 50 のまま1段足すと 0〜150 になり、箱とひげが左 2/3 に縮む
  const code = 'svgBoxH({ w: 680, rows: [{ label: "初回MTGまで", med: 20, q1: 10, q3: 47, min: 0, max: 300, mean: 40, n: 500 }] })';
  const axis = (svg) => [...svg.matchAll(/<line class="gridline" x1="[\d.]+"[^>]*\/><text class="ax"[^>]*>([^<]*)</g)].map((m) => +m[1].replace(/,/g, ""));
  const wide = drawAt(code, 1116), tk = axis(wide);
  ok(tk[tk.length - 1] >= 102.5 && tk[tk.length - 1] <= 120, "1116px の枠で軸の端がひげの先から離れすぎ: " + tk.join(","));
  ok(!overlaps(wide).length, "目盛りの字が重なる: " + overlaps(wide).join(" / "));
  // 狭い枠（319px）では字が詰まらないよう、刻みを細かくしない
  const nar = drawAt(code, 319);
  ok(!overlaps(nar).length && axis(nar).length <= 5, "319px の枠で目盛りが詰まった: " + axis(nar).join(","));
});

check("ループ4 検証: wrapText は語の中の開き括弧の手前で切り（続く開き括弧もまとめる）、1行に入らない語は今の行に続けてから切る", () => {
  const W = (s, w) => run("wrapText(" + JSON.stringify(s) + ", " + w + ")");
  // 空白の無い語の途中に括弧がある。幅の境目が括弧の中に来ても、括弧の手前で切る
  const a = W("立ち上がり期（契約開始30日以内）の件数", run('textW("立ち上がり期（契約開始")'));
  ok(a[0] === "立ち上がり期", "括弧の中で折れた: " + a.join(" | "));
  // 開き括弧が2つ続く（「（）は2つ目だけで切ると、1行目の終わりに「「」が残る
  const b = W("あいう「（えおかきくけこさしすせそ）」", run('textW("あいう「（えおか")'));
  ok(b[0] === "あいう", "開き括弧で行を終えた: " + b.join(" | "));
  // 次の語が1行に入らないときは、今の行に続けてから字の境目で切る（「合計」だけの短い行を作らない）
  const c = W("合計 とてもながいながいながいながいことばです", 120);
  ok(c[0].startsWith("合計 と"), "1行に入らない語の前で行を改め、短い行が残った: " + c.join(" | "));
  ok(c.join("").replace(/\s/g, "") === "合計とてもながいながいながいながいことばです", "折り返しで字を落とした: " + c.join(" | "));
});

check("ループ4 検証: fitLab は省略したラベルの頭に1〜2字だけの切れ端（「サ…」「サブ…」）を残さない", () => {
  const s = "サブスク継続②＿ニッコン 富山営業所";
  [80, 85, 90].forEach((w) => {
    const r = run("fitLab(" + JSON.stringify(s) + ", " + w + ")");
    ok(!/^[^…]{1,2}…/.test(r) && r.includes("富山営業所"), w + "px で頭に字の切れ端が残る・拠点名が消えた: " + r);
  });
});

check("ループ4 検証: 散布図の右端の目盛りは、中央ぞろえで枠の外に出るなら右ぞろえにする", () => {
  const svg = drawAt('svgScatter({ w: 680, pts: [{ x: 300, y: 60 }, { x: 9500, y: 90 }], xLab: "金額" })', 319);
  const last = [...svg.matchAll(/<text class="ax" x="([\d.]+)" y="[\d.]+" text-anchor="(\w+)">10,000</g)];
  ok(last.length === 1 && last[0][2] === "end" && +last[0][1] <= 319, "右端の目盛り「10,000」を右ぞろえにしていない: " + JSON.stringify(last.map((m) => m.slice(1))));
  ok(textBoxes(svg).every((b) => b.x1 <= 319 + 1), "目盛りの字が枠の外に出る");
});

check("ループ4 検証: 2行に折り返して省略したラベルも、図の下の「省略した名前の全文」に見えている字をつないで並べる", () => {
  const body = '<svg viewBox="0 0 10 10"><text class="axl" x="9" y="9" text-anchor="end" data-wrap="2" data-full="長い拠点の名前の全文">' +
    '<tspan x="9">長い拠点の</tspan><tspan x="9" dy="13">名前…全文</tspan><title>長い拠点の名前の全文</title></text></svg>';
  ctx.__FB = body;
  const f = run('fig("x", "", __FB)');
  ok(/省略した名前の全文（1 件）/.test(f) && f.includes('<span class="muted">長い拠点の名前…全文</span> … 長い拠点の名前の全文'),
    "2行のラベルが全文の一覧に出ない・見えている字が空: " + (f.match(/<li>.*?<\/li>/) || ["なし"])[0]);
});

/* ================================================================ ループ4: 文言と表（2026-09-24 実機） */
check("ループ4 phone: 多対多の注記は1回だけ。沈黙している取引の表は取引・ステージを折り返す", () => {
  const D = JSON.parse(JSON.stringify(ctx.__PH));
  D.silent = { n: 1, rule: "接触が1本も無い", excluded_marketing: 0, rows: [{ deal_id: "1", name: "サブスク継続①＿山陰パナソニック株式会社 モバイルソリューション部門",
    stage: "ロヨミ:20%（継続意思不明だが提案中）", amount: 3600000, n_calls: 0, n_contact: 0, last_contact: null, days_since: null }] };
  ctx.__PH4 = D;
  const h = run("renderPhone(__PH4)");
  const n = textOf(h).split("複数の取引に結び付いて").length - 1;
  ok(n === 1, "「1本の通話が複数の取引に結び付いて…」が " + n + " 回出ている（内訳の図の1回にする）");
  const head = h.slice(h.lastIndexOf("<thead>"), h.lastIndexOf("</thead>"));
  ok(head.includes('<th class="wl">取引</th>') && head.includes('<th class="ws">ステージ</th>'),
    "沈黙している取引の表で、取引・ステージが折り返す列になっていない（1440px で右端が切れる）");
});

check("ループ4 houjin: 本部アプローチは見出しと本文を重ねず、10法人の図ごとの注記と基準日を繰り返さない", () => {
  const hq = JSON.parse(JSON.stringify(ctx.__HQ));
  hq.rows = [0, 1, 2].map((i) => Object.assign({}, ctx.__HQ.rows[0], { houjin: "法人" + i }));
  ctx.__HQ4 = hq;
  const h = run("renderHq(__HQ4)");
  const n = (h.match(/率だけで判断しないでください/g) || []).length;
  ok(n === 1, "「解約率が 40% 以上の拠点は…率だけで判断しないでください」が " + n + " 回出ている（1回にする）");
  ok(!/<span class="hd">親法人の合計ではありません<\/span><p>親法人の合計ではありません/.test(h),
    "枠の見出しと本文の頭が同じ文（親法人の合計ではありません）");
  // 頭の枠は畳み（M-5 の foldNote: summary が見出し、中の箱は見出しを持たない）。summary の文と本文（<p>）の 1 文目が同じでないこと
  const head = (h.match(/<details class="fold notefold"><summary>([^<　]*)[\s\S]*?<div class="note [^"]*"><p>([^<。]*)/) || []);
  ok(head[1] && head[2] && !head[2].startsWith(head[1]), "枠の見出しと本文の1文目が同じ: " + head[1]);
  ok(!h.includes("集計の基準日"), "本部アプローチの末尾にも基準日の枠がある（法人番号で見るの末尾と2つ続く）");
  ok(!h.includes('<span class="no">問い</span>'), "本部アプローチが自分の問いの見出しを出している（法人番号で見るの見出しの下が空に見える）");
  ok(h.includes("採用単価（万円）"), "事業所どうしを比べる図で採用単価の単位（万円）が分からない");
});

check("ループ4 focus: どちらも無い（灰の帯）の KPI を山吹にしない・採用単価の単位・KPI 見出しの折り返し", () => {
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  fo.mtg_layers.neither = 115;
  fo.cpa = { worse: 1, judged: 1, skipped_censored: 0, note: "",
    rows: [{ site: "k1", site_name: "拠点1", prev: 1000000, last: 2000000, ratio: 2 }] };
  ctx.__FO4 = fo;
  const h = run("renderFocus(__FO4)");
  const k = h.split('<div class="kpi').find((x) => x.includes("MTG の記録がどちらも無い")) || "";
  ok(!/^ is-(warn|bad)/.test(k), "「MTG の記録がどちらも無い」の KPI に色が付いている（帯では灰）: " + k.slice(0, 30));
  ok(h.includes("今回（万円）") && h.includes("横軸は万円"), "採用単価の悪化の図で単位（万円）が分からない");
  ok(/\.kpi \.lbl\{[^}]*text-wrap:balance/.test(html), "KPI の見出しが最後の1文字だけ次の行に落ちうる（text-wrap:balance が無い）");
});

check("ループ4 team: 読み方の見出しと本文を重ねず、件数で見ない理由・担当の割れを1回ずつにする", () => {
  const D = JSON.parse(JSON.stringify(ctx.__D));
  D.meta.not_counted = "※ 担当者の評価ではありません。手が足りていない場所を見つけるための画面です。順位を付けていますが、良し悪しの判断は人がします";
  // routes.rs build_consultants の contact_rule / owner_rule そのもの
  D.contact_rule = "接触 ＝ MTG または60秒超の通話（メールは数えない）。接触率 ＝ 接触があった月 ÷（案件 × 経過月）。" +
    "件数ではなく率で見るのは、件数だと持ち案件が多い人ほど大きく出て、手が回っているかが分からなくなるため";
  D.owner_rule = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/routes.rs"), "utf-8")
    .split('"owner_rule": "')[1].split('",')[0].replace(/\\\r?\n\s*/g, "");
  ok(!/[\\\r\n]/.test(D.owner_rule) && D.owner_rule.includes("シートで後に来る行"), "routes.rs の owner_rule を読み取れない: " + D.owner_rule);
  ctx.__D4 = D;
  const h = run("renderTeam(teamOf(__D4))");
  const t = textOf(h);
  ok(!/これは担当者の評価ではありません\s+担当者の評価ではありません/.test(t), "読み方の見出しと本文の1文目が同じ文");
  const n = (t.match(/持ち案件が多い人ほど大きく出て/g) || []).length;
  ok(n === 1, "件数で見ない理由が " + n + " 回出ている（1回にする）");
  ok(!/件数では見ていません/.test(t), "「件数では見ていません」が理由（contact_rule）と別に出ている");
  const rule = t.split("担当者 × 状態の決まりごと")[1] || "";
  ok(!rule.includes("その件数を出しています"), "決まりごとで「その件数を出しています」と「担当が割れている…N 件あります」が2文続く");
  ok(rule.includes("担当が割れている稼働中の案件が 38 件"), "担当の割れの件数が決まりごとから消えた");
});

check("ループ4 byowner: 「稼働中 N 件」を表の見出しと末尾で繰り返さない（担当を選ぶ前は末尾の1か所だけ）", () => {
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "" };');
  try {
    const h = run("renderBoard(__BD)");
    // 🔴 担当を選ぶ前の頭の1行には件数が無い。見出しと末尾から消すと0か所になっていた（2026-09-24 検証）
    const nAct = (textOf(h).match(/稼働中 3 件/g) || []).length;
    ok(nAct === 1, "担当を選ぶ前の画面で「稼働中 3 件」が " + nAct + " 回出ている（1回にする）");
    ok(h.includes("担当者ごとの持ち件数（2 名）") && h.includes("担当者 2 名"), "表の見出し・末尾にこの画面の数（担当者の人数）が無い");
    run('boardFilter.consultant = "田中";');
    const h2 = run("renderBoard(__BD)");
    ok(!/稼働中 3 件/.test(textOf(h2)), "担当を選んだ後の末尾に「稼働中 3 件」が出ている（頭の1行・N 件中 M 件と重なる）");
  } finally {
    run('boardFilter = { consultant: "", flag: "", expiry: "", q: "" }; cur = { menu: "deal", view: "today" };');
  }
});

check("ループ4 dq: 記入率の偏りを図の注記と「まずい」の箱で2回言わない", () => {
  const D = JSON.parse(JSON.stringify(ctx.__DQ));
  // routes.rs build_data_quality の outcome_bias.note そのもの
  D.outcome_bias.note = "うまくいかなかった契約ほど数字が記録されていない可能性があります。「継続するほど成果が良い」という見え方を押し上げる方向に効きます";
  ctx.__DQ4 = D;
  const t = textOf(run("renderDq(__DQ4)"));
  const n = (t.match(/うまくいかなかった契約ほど/g) || []).length;
  ok(n === 1, "「うまくいかなかった契約ほど…」が " + n + " 回出ている（まずいの箱の1回にする）");
  const m = (t.match(/押し上げる方向に効きます/g) || []).length;
  ok(m === 1, "「押し上げる方向に効きます」が " + m + " 回出ている（箱の見出しと本文で重ねない）");
});

check("ループ4 handover: 担当者一覧に無い担当を表では「氏名不明」と短く出し、意味を title と表の上で補う", () => {
  const HO = JSON.parse(JSON.stringify(ctx.__HO));
  // routes.rs person_label の文そのもの
  HO.rows[1].from_label = "氏名が分からない担当（HubSpotの担当者一覧に無い）";
  HO.rows[1].from_unresolved = true;
  ctx.__HO4 = HO;
  const h = run("renderHandover(__HO4)");
  const body = h.slice(h.lastIndexOf("<tbody>"));
  ok(!body.includes("HubSpotの担当者一覧に無い）"), "表の中に長い「氏名が分からない担当（…）」が出ている（行が高くなる）");
  ok(/<span class="n0" title="HubSpot の担当者一覧に無い担当[^"]*">氏名不明<\/span>/.test(body), "表の「氏名不明」に意味の title が無い");
  ok(textOf(h.slice(0, h.lastIndexOf("<table"))).includes("「氏名不明」は HubSpot の担当者一覧に無い担当"), "表の上に「氏名不明」の意味が書いていない");
  ok(!run("renderHandover(__HO)").includes("「氏名不明」は"), "氏名不明の行が無いのに説明の1文を出している");
});

check("ループ4 handover: 「次の担当」の側も、担当者一覧に無い担当は表で「氏名不明」と短く出す", () => {
  // 上の見張りは「前の担当」の側しか入れていなかった（to 側を元の長い表記に戻しても通っていた。2026-09-24 検証）
  const HO = JSON.parse(JSON.stringify(ctx.__HO));
  HO.rows[1].to_label = "氏名が分からない担当（HubSpotの担当者一覧に無い）";
  HO.rows[1].to_unresolved = true;
  ctx.__HO5 = HO;
  const h = run("renderHandover(__HO5)");
  const body = h.slice(h.lastIndexOf("<tbody>"));
  ok(!body.includes("HubSpotの担当者一覧に無い）"), "「次の担当」に長い「氏名が分からない担当（…）」が出ている");
  ok(/<span class="n0" title="HubSpot の担当者一覧に無い担当[^"]*">氏名不明<\/span>/.test(body), "「次の担当」の「氏名不明」に意味の title が無い");
  ok(textOf(h.slice(0, h.lastIndexOf("<table"))).includes("「氏名不明」は HubSpot の担当者一覧に無い担当"), "次の担当だけが氏名不明のとき、表の上に意味が書いていない");
});

check("ループ4 renewal: 月次継続率で n<30 の月（右端 27-01 n=1 の 0%）に点を打たず、打たない理由を書く", () => {
  const h = run("renderRenewal(__RN)");
  const svg = firstSvg(retPart(h));
  const tt = [...svg.matchAll(/<circle [^>]*><title>([^<]*)<\/title>/g)].map((m) => m[1]);
  ok(tt.length > 0 && tt.some((t) => t.startsWith("26-09")), "見張りの前提: 点の title が「月…」で始まっていない: " + tt.slice(-2).join(" / "));
  ok(!tt.some((t) => t.startsWith("27-01") || t.startsWith("26-10")), "n<30 の月（26-10 n=8 / 27-01 n=1）に点を打っている: " + tt.slice(-3).join(" / "));
  ok(svg.includes(">27-01<") && svg.includes(">n=1<"), "n<30 の月を横軸から消している（月と件数は残す）");
  ok(textOf(h).includes("30 件に届かない 2 か月は点を打っていません"), "n<30 で点を打たなかった月のことが書かれていない");
});

check("ループ4 renewal: n<30 で点を打たない月があるとき、線の切れ目を「値が無い」とだけ言わない", () => {
  const h = run("renderRenewal(__RN)");
  const t = textOf(h);
  ok(t.includes("30 件に届かない 2 か月は点を打っていません"), "見張りの前提: n<30 の月が無い入力になっている");
  ok(!t.includes("線が途切れているところは、その月の値が無いところです"),
    "n<30 の月（値はある）があるのに「その月の値が無いところです」と書いている（点を打たない理由と食い違う）");
  ok(t.includes("線が途切れているところは、点を打っていない月（決着が 30 件に届かない月）か、率が出せない月です"),
    "線の切れ目に n<30 の月が入ることを書いていない");
  // 決まり文句そのものは、ほかの図では残す（fig の gapNote）
  ok(run('fig("x", "", svgLine({ w: 300, h: 120, x: ["a","b","c"], series: [{ color: "#000", pts: [{ v: 1 }, null, { v: 2 }] }] }), "")')
    .includes("線が途切れているところは、その月の値が無いところです"), "ほかの図から「値が無い」の決まり文句まで消えた");
});

/* 🔴 段B（2026-09-29）: 最優先の表は案件一覧の見方に移し、成果とリスクの表を描く分岐（outcomeTopTable）は外した。
   この表だけにあった「放置の軸」「接触の記録（すべて契約前）」の列は見方には無い（段B で失ったもの）。
   前の見張り（行ごとに「すべて契約前」と書く）は届く画面が無くなったので、表が戻っていないことと、行き先を見張る */
check("ループ4 outcome: 最優先の表は成果と継続に出さず、見方へ案内する（接触の記録の列は段B で外れた）", () => {
  const O = JSON.parse(JSON.stringify(ctx.__OUT));
  // routes.rs build_outcome の top の形。n_contact は契約前も含めた件数
  O.risk.top = [
    { name: "案件A", stage: "定期1", amount: 1800000, days_to_expiry: 40, ax3w: "契約後に一度も接触していない", n_contact: 23, never_after_start: true },
    { name: "案件B", stage: "定期2", amount: 900000, days_to_expiry: 20, ax3w: "契約後の最終接触から 45日", n_contact: 5, never_after_start: false }];
  ctx.__OUT4 = O;
  const h = run("renderOutcome(__OUT4)");
  ok(!h.includes("<tbody>") && !textOf(h).includes("すべて契約前"), "最優先の表が成果と継続に戻っている");
  ok(h.includes('href="' + run('esc(hashFor("board", { view: "top" }))') + '"') && textOf(h).includes("この 2 件（2軸とも赤）の表は"),
    "最優先の件数と見方への行き先が無い");
});

check("ループ4 outcome: 契約開始日が空の行（no_start）では「すべて契約前」と言い切らず、散布図の色も分ける", () => {
  const O = JSON.parse(JSON.stringify(ctx.__OUT));
  // routes.rs risk() の開始日が空の行の形（never_after_start は立たない。tests.rs で見張る）
  O.risk.top = [
    { name: "案件C", stage: "定期1", amount: 1200000, days_to_expiry: 30, ax3w: "契約開始日が空で、契約後の接触を切り出せない",
      n_contact: 7, never_after_start: false, no_start: true }];
  ctx.__OUT5 = O;
  /* 表（行ごとの「すべて契約前」）は段B で見方に移した（上の見張り）。散布図の凡例と点の側だけが残る */
  const h = run("renderOutcome(__OUT5)");
  ok(!textOf(h).includes("すべて契約前"), "開始日が空の行で接触が契約前だと言い切っている");
  ok(textOf(h).includes("契約開始日が空（契約後を切り出せない）"), "散布図の凡例に開始日が空の点の意味が無い");
  ok(!textOf(run("renderOutcome(__OUT4)")).includes("契約開始日が空（"), "開始日が空の行が無いのに凡例を出している");
});

check("ループ4 採用単価の棒: 万円の目盛りを出す svgBarH の呼び出しは、軸の題名（xLab）を渡す", () => {
  // 🔴 svgBarH がまだ xLab を描かない（部品の側で対応中）ため、画面では副題と凡例の「万円」だけが効いている。
  //    部品が描くようになったとき、呼び出し側から xLab が消えていると単位が出ないので、呼び出し側を見張る
  //    （2026-09-24 検証: 副題だけを見ていて、xLab だけを消しても通っていた）
  const calls = [...html.matchAll(/svgBarH\(\{[^\n]*/g)].map((m) => m[0]).filter((x) => x.includes("fmt: F.man"));
  ok(calls.length >= 3, "見張りの前提: 万円の svgBarH の呼び出しが 3 つ見つからない（" + calls.length + "）");
  const bad = calls.filter((x) => !/xLab: "採用単価（万円）"/.test(x));
  ok(!bad.length, "万円の svgBarH に軸の題名（xLab）を渡していない: " + bad.join(" / "));
});

check("ループ4 series: 契約の系列の表は折り返しの幅を詰め（table.ser）、1440px の本文に余白を残す", () => {
  // 11列が本文 1,117px にちょうど収まるだけで、長い取引名・拠点名・ステージで右端（状態）が切れた
  // （2026-09-24 検証。40字・20字・15字にすると 47px 超）。既定の 22em / 11em より狭くする
  ctx.__SER = { meta: { found: true, houjin: "H1", today: "2026-09-23" },
    customer: { name: "x", ltv: 1, deals: 1, sites: 1, active: 1, max_renewal_no: 0, last_expiration: "2027-01-31" },
    focus: null, mtgs: [], cpa3: [], cpa_by_site: [], monthly: [], handover: [], contacts: [], funnel: {},
    deals: [{ deal_id: "d1", name: "取引", stage: "定期1", site: "拠点", kind: "定期", start: "2025-01-01",
      expiration: "2025-12-31", renewal_no: 1, amount: 100, oubo: 1, mensetu: 1, syoudaku: 1, is_active: true }] };
  const h = run('custBlocks(__SER, new Set(["deals"]))');
  ok(/<table class="ser"><thead><tr><th class="wl">取引<\/th>/.test(h), "契約の系列の表に class=\"ser\" が付いていない");
  const em = (re) => { const m = html.match(re); return m ? parseFloat(m[1]) : NaN; };
  const l = em(/table\.ser td\.wl\{\s*max-width:([\d.]+)em/), s2 = em(/table\.ser td\.ws\{\s*max-width:([\d.]+)em/);
  ok(l <= 16 && s2 <= 9, "契約の系列の表の折り返しの幅が詰まっていない（wl " + l + "em / ws " + s2 + "em）");
});

check("ループ4統合: 横棒（svgBarH）は xLab を軸の題名として描く（採用単価の単位）", () => {
  const svg = run('svgBarH({ w: 680, fmt: F.man, xLab: "採用単価（万円）", rows: [{ label: "A", v: 500000, txt: "50万" }] })');
  ok(/<text[^>]*data-xlab="1"[^>]*>採用単価（万円）<\/text>/.test(svg), "横棒に軸の題名「採用単価（万円）」が出ていない");
  const plain = run('svgBarH({ w: 680, fmt: F.man, rows: [{ label: "A", v: 500000, txt: "50万" }] })');
  ok(!/data-xlab/.test(plain), "xLab を渡していないのに軸の題名が出ている");
});

check("ループ4統合: 注記の折り返しで、とうに閉じた括弧の手前まで遡って切らない（1行目が極端に短くならない）", () => {
  const lines = JSON.parse(run('JSON.stringify(wrapText("採用単価（万円）は同じ進捗帯の中央値と比べて1.5倍以上のときに赤で出しています", 300))'));
  ok(lines[0] !== "採用単価" && lines[0].length > 8, "1行目が極端に短い: " + JSON.stringify(lines));
  ok(lines.join("") === "採用単価（万円）は同じ進捗帯の中央値と比べて1.5倍以上のときに赤で出しています", "字が落ちている: " + JSON.stringify(lines));
  const br = JSON.parse(run('JSON.stringify(wrapText("解約・充足 100%（決着済み 12件中）", 120))'));
  ok(br.some(l => l.startsWith("（決着済み")), "まだ閉じていない括弧の手前では切る動きが壊れた: " + JSON.stringify(br));
});

/* ================================================================ ループ5（本番 1586140 の実測の残り, 2026-09-24） */
check("ループ5: 帯を縦に積む図（series の案件ごとの図）も、開いた直後を新しい側に合わせる（data-xr）。左のラベルは貼り付けたまま", () => {
  const xr = (svg) => { const m = String(svg).match(/<svg [^>]*data-xr="(\d+)"/); return m ? +m[1] : null; };
  const mx = JSON.stringify(monthsN(24));
  const all = JSON.stringify(monthsN(24).map((_, i) => ({ v: i })));
  const svg = drawAt('svgStackLanes({ w: 940, months: ' + mx + ', lanes: [{ type: "line", label: "応募", color: "blue", pts: ' + all + ' }] })', 319);
  ok(xr(svg) >= 930, "帯を縦に積む図の data-xr が最新の月（右端）でない（開いた直後が古い側になる）: " + xr(svg));
  ok(/class="sticklab"/.test(svg), "左のラベルを貼り付けていない");
  // 最後の数か月に値が無い図は、値がある最後の月を右端に見せる（右の空の月だけが見えない）
  const early = JSON.stringify(monthsN(24).map((_, i) => (i <= 12 ? { v: i } : null)));
  const e = drawAt('svgStackLanes({ w: 940, months: ' + mx + ', lanes: [{ type: "line", label: "応募", color: "blue", pts: ' + early + ' }] })', 319);
  ok(xr(e) > 400 && xr(e) < 800, "値がある最後の月に data-xr を合わせていない: " + xr(e));
  // 枠の幅で描いた図（月が少ない）は横にスクロールしないので付けない
  const one = drawAt('svgStackLanes({ w: 940, months: ["25-09"], lanes: [{ type: "line", label: "応募", color: "blue", pts: [{ v: 1 }] }] })', 319);
  ok(xr(one) == null, "枠の幅で描いた図に data-xr が付いた");
});

check("ループ5: ファネル（houjin の応募 → 面接 → 採用）は 319px の枠に収め、「前段の N%」が枠の外に出ない", () => {
  const code = 'svgFunnel({ steps: [{ label: "応募", v: 12345 }, { label: "面接", v: 2345, pair: { num: 2345, den: 12345, n: 40 } },' +
    ' { label: "採用", v: 345, pair: { num: 345, den: 2345, n: 38 } }] })';
  const svg = drawAt(code, 319);
  ok(figW(svg) === 319, "ファネルが 319px の枠に収まっていない（420px 固定で 64px はみ出す）: " + figW(svg));
  const tb = textBoxes(svg);
  ok(tb.filter((b) => b.s.startsWith("前段の")).length === 2, "「前段の N%」が2つ出ていない");
  ok(tb.every((b) => b.x0 >= -0.5 && b.x1 <= 319 + 1), "枠の外に出る文字: " + tb.filter((b) => b.x1 > 320).map((b) => b.s).join(" / "));
  ok(!overlaps(svg).length, "文字が重なる: " + overlaps(svg).join(" / "));
  // 縦のスクロールバーなどでさらに狭い枠（285px, chromium 実測）でも収める
  const s285 = drawAt(code, 285);
  ok(figW(s285) === 285 && textBoxes(s285).every((b) => b.x1 <= 285 + 1), "285px の枠でファネルが収まらない: " + figW(s285));
  // 広い枠では広げない（段が3つの図を 1,000px に伸ばしても読みやすくならない）。1回目（枠が分からない）は 420px のまま
  ok(figW(drawAt(code, 1116)) === 420 && figW(drawAt(code, null)) === 420, "広い枠・1回目で 420px のままでない");
  ok(/<svg [^>]*data-fk="\d+"/.test(drawAt(code, null)), "枠の幅を測る印（data-fk）が無い（paintFigs が描き直さない）");
});

check("ループ5: 箱ひげは広い枠（1440px, 1116px）ではラベルを省略しない（outcome の「充足（採れて終わった）」）", () => {
  const rows = '[{ label: "継続した", med: 7.4, q1: 3.3, q3: 11.4, min: 0.1, max: 163.7, n: 742 },' +
    '{ label: "充足（採れて終わった）", med: 9.8, q1: 5, q3: 17.3, min: 0.5, max: 121, n: 142 }]';
  const wide = drawAt("svgBoxH({ w: 680, xFmt: F.d1, rows: " + rows + " })", 1116);
  const labs = [...wide.matchAll(/<text class="axl"[^>]*>([^<]*)</g)].map((m) => m[1]);
  ok(labs.includes("充足（採れて終わった）") && !labs.some((t) => t.includes("…")),
    "1116px の枠でラベルを省略した: " + labs.join(" / "));
  const tb = textBoxes(wide);
  ok(tb.every((b) => b.x0 >= -0.5) && !overlaps(wide).length, "ラベルが左端の外に出る・文字が重なる: " + overlaps(wide).join(" / "));
  // 狭い枠（319px）は前と同じく2行に折り返す（省略しない）
  const nar = drawAt("svgBoxH({ w: 680, xFmt: F.d1, rows: " + rows + " })", 319);
  ok(/data-wrap="2"/.test(nar), "319px の枠で2行の折り返しが壊れた");
});

check("ループ5: 図の見出しの補足（.hint）で、数字と単位（「6.7倍」「12 件」）を折れない塊にする", () => {
  const f = run('fig("x", "拠点3 / 取引6 / 開き 6.7倍。母数 12 件、<b data-n=\\"9件\\">2 法人</b>（&plusmn;1日で83.3%）契約 2025-12-18〜2026-06-17", "")');
  const hint = (f.match(/<span class="hint">([\s\S]*?)<\/span><\/figcaption>/) || [0, ""])[1];
  const nw = [...hint.matchAll(/<span class="nw">([^<]*)<\/span>/g)].map((m) => m[1]);
  ["6.7倍", "12 件", "2 法人", "1日", "83.3%"].forEach((t) =>
    ok(nw.includes(t), "「" + t + "」を折れない塊にしていない: " + nw.join(" | ")));
  // タグの属性と文字参照の中は触らない
  ok(hint.includes('<b data-n="9件">') && hint.includes("&plusmn;"), "タグの属性・文字参照を書き換えた: " + hint);
  ok(/figcaption \.hint \.nw\{\s*white-space:nowrap/.test(html), ".hint .nw に white-space:nowrap の CSS が無い");
});

check("ループ5 outcome: 契約開始日が空（no_start）の点は、赤に数えているとおり赤系の中空で描く（灰＝記録なしの色にしない）", () => {
  const O = JSON.parse(JSON.stringify(ctx.__OUT));
  // routes.rs risk() は開始日が空の行を放置の軸の赤に数え、2軸とも赤（最優先）の行に入れる
  O.risk.top = [
    { name: "案件C", stage: "定期1", amount: 1200000, days_to_expiry: 30, ax3w: "契約開始日が空で、契約後の接触を切り出せない",
      n_contact: 7, never_after_start: false, no_start: true },
    { name: "案件D", stage: "定期1", amount: 900000, days_to_expiry: 40, ax3w: "最後の接触から45日",
      n_contact: 3, never_after_start: false, no_start: false }];
  ctx.__OUT6 = O;
  const h = run("renderOutcome(__OUT6)");
  const hi = run("C.hi"), ghost = run("C.ghost");
  const c = h.match(/<circle[^>]*>(?=<title>案件C)/);
  ok(c, "散布図に開始日が空の点（案件C）が無い");
  ok(!c[0].includes(ghost), "開始日が空の点を灰（記録なし・未確定の色）で描いている: " + c[0]);
  ok(c[0].includes("stroke:" + hi) && c[0].includes("fill:var(--panel)") && !/stroke-dasharray/.test(c[0]),
    "開始日が空の点が赤の中空・実線の丸になっていない（赤に数えた件数と見た目が合わない）: " + c[0]);
  // 開始日がある行（接触から30日超）は塗りの点のまま
  const d = h.match(/<circle[^>]*>(?=<title>案件D)/);
  ok(d && /style="fill:/.test(d[0]) && !/data-open/.test(d[0]), "開始日がある行の点まで中空にした: " + (d && d[0]));
  // 凡例も同じ印（赤の中空）で、赤に数えていることを書く
  const lgs = [...h.matchAll(/<i><svg [^>]*>((?:(?!<\/svg>)[\s\S])*)<\/svg>([^<]*)<\/i>/g)].filter((m) => m[2].startsWith("契約開始日が空"));
  ok(lgs.length === 1 && lgs[0][1].includes("stroke:" + hi) && !lgs[0][1].includes(ghost) && lgs[0][2].includes("赤に数えています"),
    "凡例の印・説明が点と合っていない: " + (lgs[0] ? lgs[0][0] : "無し"));
});

/* ================================================================ 担当者ごとの接触（2026-09-24 追加）
   応答の形は src/handlers/cs_dashboard/contact_trend.rs のとおり。not_counted と各 rule は
   サーバの文そのもの（fixture の応答から写した）。期間と数は見張りのために作った小さなもの:
     p0 = 通話の記録が始まる前（calls_missing）。A は 50/5 件 ＝ 1件あたり 10 回（出してはいけない）
     p1・p2 = 確定。A の p2 は持ち案件 2 件（small_n）、B の p1 は持ち案件 0 件（分母0）
     p3 = いまの月（provisional）
     C = どの期間も持ち案件 3 件未満（図にしない人） */
{
  const cell = (d, c) => ({ deals: d, contacts: c, avg: d ? c / d : null, small_n: d > 0 && d < 3 });
  const per = (key, prov, miss) => ({ key, label: key, start: key + "-01", end: key + "-28",
    provisional: prov, calls_missing: miss });
  ctx.__CT = {
    meta: { today: "2026-09-18", min_deals: 3, call_from: "2026-03-23", n_no_span: 3, n_no_history: 0,
      cutoff: "2026-09-14",
      not_counted: "※ 接触は検知専用です。多いほど良いという評価ではありません（担当者の評価ではありません）。もめている案件ほど電話が増えることもあり、接触の多い少ないが良い悪いのどちらに向くかは、このデータでは決まっていません" },
    contact_rule: "接触 ＝ MTG または60秒超の通話（メールは数えない）。",
    denom_rule: "1件あたりの接触 ＝ …", owner_rule: "担当は consultant が正本です。",
    undetermined_rule: "担当履歴の最初の行より前の日は、担当が決められません。",
    provisional_rule: "いまの週・月は途中なので未確定です。",
    calls_missing_rule: "通話の記録が始まる前の期間は、MTG しか数えられないので出していません。",
    attach_rule: "付いている取引の契約期間の外の接触を、同じ拠点の本体案件に付け直して数えています。",
    month: {
      periods: [per("2026-03", false, true), per("2026-06", false, false), per("2026-07", false, false), per("2026-09", true, false)],
      rows: [
        { consultant: "担当A", retired: false, cells: [cell(5, 50), cell(10, 20), cell(2, 6), cell(10, 5)] },
        /* B の 4.0 回が全員の縦軸の上端を決める（A だけなら 2.0 で足りる） */
        { consultant: "担当B", retired: true, cells: [cell(0, 0), cell(0, 0), cell(6, 24), cell(6, 3)] },
        { consultant: "担当C", retired: false, cells: [cell(1, 1), cell(2, 2), cell(1, 0), cell(0, 0)] },
        /* 出さない期間（p0）にだけ持っていた人。図にも表にも出さない */
        { consultant: "担当Z", retired: false, cells: [cell(4, 4), cell(0, 0), cell(0, 0), cell(0, 0)] },
      ],
      team: [cell(10, 55), cell(12, 22), cell(9, 18), cell(16, 8)],
      undetermined: [cell(0, 0), cell(1, 2), cell(0, 0), cell(0, 0)],
      shared: [0, 0, 1, 0],
      moved: [9, 4, 3, 0],
    },
    week: { periods: [], rows: [], team: [], undetermined: [], shared: [] },
  };
}
const ctFigs = (h) => [...h.matchAll(/<figure class="fig"><figcaption>([^<]*)[\s\S]*?<\/figure>/g)]
  .map((m) => ({ cap: m[1], body: m[0] }));
/* 2026-09-29 組み替え（09 の 6）: 担当者ごとの接触は、チームと担当の節「接触の推移」（teamContact）になった。
   1 人 1 枚の小さな図（27 枚）はやめ、全体の線に選んだ担当（teamPick）の線を 1 本だけ重ねる。値は下の表（期間 × 担当者）に全員分。
   見張りは前と同じ入力（__CT）をチームと担当の応答の contact に入れて描く。前の「その人の図」の性質は「その人を選んだときの線」で見る */
const ctRun = (code, pick) => run("teamPick = " + JSON.stringify(pick || "") + "; " + code);
/* 選んだ担当の線の点の説明（title）。点の説明は「担当者: 名前・持ち案件 …」で始まる（contactPts の who） */
const ctTitles = (body, who) => [...body.matchAll(/<circle [^>]*><title>([^<]*)<\/title>/g)].map((m) => m[1])
  .filter((t) => t.includes(who));
const ctTops = (body) => Math.max(...[...body.matchAll(/<text class="ax" [^>]*text-anchor="end">([0-9.]+)<\/text>/g)].map((m) => +m[1]));

check("担当者ごとの接触: 頭の枠で「検知専用・多いほど良いではない・評価ではない」を言う", () => {
  const h = ctRun('contactUnit = "month"; teamContact({ contact: __CT })');
  const head = h.slice(0, h.indexOf('<figure'));
  ok(head.includes("接触は検知専用です") && head.includes("多いほど良いという評価ではありません") &&
     head.includes("担当者の評価ではありません"), "図より前に検知専用・評価ではないの断りが無い");
});

check("担当者ごとの接触: 分母0は —、分母が小さい期間は印を付けて点を打たない、未確定は中空・破線", () => {
  const h = ctRun('contactUnit = "month"; teamContact({ contact: __CT })', "担当A");
  const tb = h.slice(h.indexOf('<table id="ct-tbl"'));
  const rowOf = (name) => tb.slice(tb.indexOf(name), tb.indexOf("</tr>", tb.indexOf(name)));
  const b = rowOf("担当B");
  ok(/<td[^>]*><span class="n0">/.test(b.slice(b.indexOf("</td>"))), "分母0の期間を — にしていない: " + b);
  ok(!b.includes("0.00 "), "分母0の期間を 0.00 と書いている");
  const a = rowOf("担当A");
  ok(/3\.00 <span class="muted small">6\/2件<\/span> <span class="tag"[^>]*>少<\/span>/.test(a),
    "持ち案件 2 件の期間に「少」の印が無い: " + a);
  const figs = ctFigs(h);
  ok(figs.length === 1, "図が全体の 1 枚でない（1 人 1 枚に戻っている）: " + figs.map((f) => f.cap).join(" / "));
  const fa = figs[0].body;
  const ta = ctTitles(fa, "担当者: 担当A");
  ok(ta.length > 0, "担当A を選んだのに、担当A の線の点が無い");
  ok(!ta.some((t) => /^26-07: /.test(t)), "持ち案件 2 件の期間に点を打っている");
  ok(/<circle [^>]*stroke-dasharray[^>]*><title>26-09: [^<]*担当者: 担当A[^<]*未確定/.test(fa), "いまの月を中空・破線の点で描いていない");
  /* A はいまの月の前が点を打たない期間なので線が無い。前の月に点がある B で線を見る（選んだ人の線の色で見分ける） */
  const fb = ctFigs(ctRun('contactUnit = "month"; teamContact({ contact: __CT })', "担当B"))[0].body;
  ok(/<path [^>]*style="stroke:var\(--murasaki\)"[^>]*stroke-dasharray="5 4"/.test(fb), "いまの月へ向かう線が破線でない");
  ok(tb.includes("2026-09（途中）"), "表の列見出しにいまの月が途中だと書いていない");
});

check("担当者ごとの接触: 通話の記録が始まる前の期間は図にも表にも出さず、縦軸は誰を選んでもそろえる", () => {
  const h = ctRun('contactUnit = "month"; teamContact({ contact: __CT })');
  const tb = h.slice(h.indexOf('<table id="ct-tbl"'));
  ok(!tb.includes(">2026-03<"), "通話の記録が無い期間を表に出している");
  ok(!/<title>26-03: /.test(h), "通話の記録が無い期間を図に出している");
  ok(textOf(h).includes("通話の記録は 2026-03-23 からです"), "出していない理由（通話の記録の始まり）を書いていない");
  ok(!h.includes("担当Z"), "出す期間に何も持っていない人を出している（表・重ねる担当の選択肢）");
  ok(/<option value="担当A">担当A<\/option>/.test(h) && h.includes('<option value="担当C">担当C</option>'), "重ねる担当の選択肢に出す期間の担当がいない");
  /* 持ち案件が少ない人（担当C）は、選んでも線を引かず、理由と名前を書く */
  const hc = ctRun('contactUnit = "month"; teamContact({ contact: __CT })', "担当C");
  ok(!ctTitles(ctFigs(hc)[0].body, "担当者: 担当C").length, "持ち案件が少ない人の線を引いている");
  ok(textOf(hc).includes("担当者: 担当C の線は引いていません") && textOf(hc).includes("3 件未満"), "線を引かない人の名前と理由を書いていない");
  /* 縦軸の目盛りのいちばん上が、誰を選んでも同じ（10 回＝出さない期間の値に引っぱられていない） */
  const tops = ["", "担当A", "担当B", "担当C"].map((p) => ctTops(ctFigs(ctRun('contactUnit = "month"; teamContact({ contact: __CT })', p))[0].body));
  ok(tops.every((x) => x === tops[0]), "縦軸の目盛りが選んだ人ごとに違う: " + tops.join(", "));
  ok(tops[0] < 10, "出さない期間の値で縦軸が伸びている: " + tops[0]);
  ok(textOf(h).includes("退職者のまま"), "退職者のままの印が無い");
});

check("担当者ごとの接触: 担当が決められない行・決まりごと（分母・付け直し・担当の正本・未確定・読めない案件・交代へのリンク）・全体の行を出す", () => {
  /* 2026-09-24 検証: どれを消しても見張りが落ちなかった（J1・J2・J8・J13〜J15・J17・J18） */
  const h = ctRun('contactUnit = "month"; teamContact({ contact: __CT })');
  const tb = h.slice(h.indexOf('<table id="ct-tbl"'), h.indexOf("</table>", h.indexOf('<table id="ct-tbl"')));
  const rowOf = (name) => tb.slice(tb.indexOf(name), tb.indexOf("</tr>", tb.indexOf(name)));
  ok(tb.includes("担当が決められない"), "表に「担当が決められない」の行が無い");
  ok(rowOf("担当が決められない").includes("案件 1 件・接触 2 回"), "担当が決められない件数（p1: 案件1件・接触2回）を出していない");
  const all = rowOf("<b>全体</b>");
  ok(tb.includes("<b>全体</b>") && all.includes("1.83 ") && all.includes("22/12件"), "表の「全体」の行が無いか、値が違う: " + all);
  const t = textOf(h.slice(h.indexOf("接触の推移の決まりごと")));
  ok(t.includes("接触の推移の決まりごと"), "決まりごとの枠が無い");
  for (const [k, why] of [["denom_rule", "分母の定義"], ["attach_rule", "接触の付け直し"], ["owner_rule", "担当の正本"],
    ["undetermined_rule", "担当が決められないときの扱い"], ["provisional_rule", "未確定の説明"]]) {
    ok(t.includes(run(k === "owner_rule" ? "dispText(__CT.owner_rule)" : "__CT." + k)), "決まりごとに" + why + "（" + k + "）が無い");
  }
  ok(t.includes("付け直して数えた接触はのべ 7 回"), "付け直して数えた接触の数（出す期間の合計 4+3）を書いていない");
  ok(t.includes("契約の開始日か満了日が読めない案件 3 件"), "開始日・満了日が読めない案件の数（n_no_span）を書いていない");
  /* 担当の交代は同じ画面の節（チームと担当の中）。そこへ移るボタン */
  ok(h.slice(h.indexOf("接触の推移の決まりごと")).includes('<button type="button" class="tojump" data-jump="tm-ho-h">担当の交代</button>'),
    "担当の交代（同じ画面の節）への行き先が無い");
});

check("担当者ごとの接触: 持ち案件があって接触0回のますは 0.00（— にしない）", () => {
  /* — は「持ち案件が無い」の印。接触0回を — にすると意味が逆に読まれる（J16） */
  const h = ctRun('contactUnit = "month"; teamContact({ contact: __CT })');
  const tb = h.slice(h.indexOf('<table id="ct-tbl"'));
  const c = tb.slice(tb.indexOf("担当C"), tb.indexOf("</tr>", tb.indexOf("担当C")));
  ok(/0\.00 <span class="muted small">0\/1件<\/span>/.test(c), "持ち案件1件・接触0回のますが 0.00 になっていない: " + c);
});

check("担当者ごとの接触: 縦軸の上端は、点を打たない値（small_n）と未確定の値では決めない", () => {
  /* 2026-09-24 検証: small_n を上端から外す処理（J5）が見張られておらず、未確定の途中の値1つで全員の軸が 0〜10 になっていた */
  const T = JSON.parse(JSON.stringify(ctx.__CT));
  const cell = (d, c) => ({ deals: d, contacts: c, avg: d ? c / d : null, small_n: d > 0 && d < 3 });
  T.month.rows[0].cells[2] = cell(2, 30);  /* A の p2: 15.0 回だが small_n */
  T.month.rows[1].cells[3] = cell(6, 60);  /* B のいまの月: 10.0 回（未確定） */
  ctx.__CT2 = T;
  const tops = ["", "担当A", "担当B"].map((p) => ctTops(ctFigs(ctRun('contactUnit = "month"; teamContact({ contact: __CT2 })', p))[0].body));
  ok(tops.every((x) => x === tops[0]), "縦軸の目盛りが選んだ人ごとに違う: " + tops.join(", "));
  ok(tops[0] < 10, "未確定または small_n の値で縦軸が伸びている: " + tops.join(", "));
  const fb = ctFigs(ctRun('contactUnit = "month"; teamContact({ contact: __CT2 })', "担当B"))[0].body;
  ok(/<title>26-09: [^<]*担当者: 担当B[^<]*10\.00 回[^<]*上端/.test(fb), "上端に置いた未確定の点に実際の値を書いていない");
  ok(textOf(fb).includes("10.00 回は縦軸の上端より大きいので"), "上端に置いたことを図の下に書いていない");
  /* 上端を超えないときは書かない */
  ok(!textOf(ctRun('contactUnit = "month"; teamContact({ contact: __CT })', "担当B")).includes("縦軸の上端より大きいので"), "上端を超えていないのに上端に置いたと書いている");
});

/* ---- ループ5（2026-09-24 藤巻さん「タイトルが意味わからなくなってる」）----
   貼られた文: 「hd26f422ffda0 / 直近の確定した週（9/7〜9/13）: 持ち案件 34 件 / ← 図を横にスクロールできます → /
   0.0 2.0 4.0 6.0 0.0 2.0 4.0 6.0 / 6/29 7/20 8/10 8/31 9/14」。週ごと（12 週）の図を枠 約319px に描いたもの。
   2026-09-29 組み替えで図は 1 枚（全体＋選んだ担当）になった。同じ性質（枠の幅で描く・目盛りは 1 回・整数の刻み・名前と持ち案件の書き方）を
   その 1 枚で見る */
{
  const cell = (d, c) => ({ deals: d, contacts: c, avg: d ? c / d : null, small_n: d > 0 && d < 3 });
  const T = JSON.parse(JSON.stringify(ctx.__CT));
  const wk = Array.from({ length: 13 }, (_, i) => {
    const d = new Date(Date.UTC(2026, 5, 22 + 7 * i)), e = new Date(d.getTime() + 6 * 864e5);
    const f = (x) => (x.getUTCMonth() + 1) + "/" + x.getUTCDate();
    return { key: d.toISOString().slice(0, 10), label: f(d) + "〜" + f(e), start: "", end: "",
      provisional: i === 12, calls_missing: false };
  });
  const cells = (k) => wk.map((_, i) => cell(30 + i, Math.round((30 + i) * (k + (i % 4)) / 2)));
  T.week = { periods: wk,
    rows: [{ consultant: "hd26f422ffda0", retired: false, cells: cells(1) },
           { consultant: "担当R", retired: true, cells: cells(2) }],
    team: cells(1), undetermined: wk.map(() => cell(0, 0)), shared: wk.map(() => 0), moved: wk.map(() => 0) };
  ctx.__CTW = T;
}
/* 全部の図に同じ枠の幅を渡して描く（paintFigs の2回目）。null は1回目（枠の幅が分からない） */
function ctDraw(code, avail, pick) {
  ctx.__AVALL = avail == null ? null : Object.fromEntries(Array.from({ length: 60 }, (_, k) => [k, avail]));
  return run("teamPick = " + JSON.stringify(pick || "") + "; FIGFIT.seq = 0; FIGFIT.avail = __AVALL; try { " + code + " } finally { FIGFIT.avail = null; FIGFIT.seq = 0; }");
}
const ctAxis = (body) => [...body.matchAll(/<text class="ax" [^>]*text-anchor="end">([^<]*)<\/text>/g)].map((m) => m[1]);

check("担当者ごとの接触（ループ5 a）: 図は枠の幅で描き、どの幅でも横スクロールにしない", () => {
  for (const av of [null, 250, 285, 319, 334, 360, 520]) {
    for (const pick of ["", "hd26f422ffda0"]) {
      const h = ctDraw('contactUnit = "week"; teamContact({ contact: __CTW })', av, pick);
      const figs = ctFigs(h);
      ok(figs.length === 1, "週ごとの図が 1 枚でない（枠 " + av + "）: " + figs.length);
      figs.forEach((f) => {
        ok(!f.body.includes('class="figscroll"'), "「図を横にスクロールできます」が出る（枠 " + av + "px, " + f.cap + "）");
        ok(!f.body.includes("data-cap="), "横にスクロールする枠の印（data-cap）が付いている（枠 " + av + "px）");
        const w = figW(f.body);
        if (av != null) ok(w <= av, "図の幅 " + w + "px が枠 " + av + "px より広い（" + f.cap + "）");
        /* CSS の最小幅（--fw の .92 倍）も枠に収まる */
        const fw = Math.max(...[...f.body.matchAll(/--fw:(\d+)px/g)].map((m) => +m[1]));
        if (av != null) ok(fw * .92 <= av, "図の最小幅 " + fw * .92 + "px が枠 " + av + "px より広い");
        /* 1回目（枠の幅が分からない）も 400px 幅の枠（約 330px）に収まる幅で描く（描き直す前に一瞬はみ出さない） */
        if (av == null) ok(fw * .92 <= 330, "1回目の図の最小幅 " + fw * .92 + "px が 330px を超える");
      });
    }
  }
});

check("担当者ごとの接触（ループ5 b）: 縦軸の目盛りは1回だけ描く（左に貼り付けた複製を作らない）", () => {
  for (const av of [null, 285, 319, 360]) {
    ctFigs(ctDraw('contactUnit = "week"; teamContact({ contact: __CTW })', av, "hd26f422ffda0")).forEach((f) => {
      ok(!f.body.includes('class="sticklab"') && !f.body.includes("stickwrap"), "目盛りを左に貼り付けた複製がある（枠 " + av + "px, " + f.cap + "）");
      const t = ctAxis(f.body);
      ok(t.length >= 2 && new Set(t).size === t.length, "縦軸の目盛りが2回出る（枠 " + av + "px）: " + t.join(" "));
    });
  }
});

check("担当者ごとの接触（ループ5 c）: 目盛りは整数で表せる刻みなら整数（値は小数2桁のまま）", () => {
  const h = ctDraw('contactUnit = "month"; teamContact({ contact: __CT })', 319, "担当B");
  const t = ctAxis(ctFigs(h)[0].body);
  ok(t.join(" ") === t.map((x) => String(parseInt(x, 10))).join(" "), "整数の刻みなのに小数で出している: " + t.join(" "));
  ok(t.includes("0") && t.includes("4"), "目盛りが 0〜4 でない: " + t.join(" "));
  ok(/<title>26-07: 4\.00 \//.test(ctFigs(h)[0].body), "点の値を小数2桁で出していない");
  /* 刻みが整数でないときは、必要な桁だけ小数で出す（そろえる） */
  const T = JSON.parse(JSON.stringify(ctx.__CT));
  T.month.rows[1].cells[2] = { deals: 10, contacts: 12, avg: 1.2, small_n: false };
  T.month.team = T.month.team.map((c) => Object.assign({}, c, { avg: c.avg == null ? null : Math.min(c.avg, 1.2) }));
  T.month.rows[0].cells[1] = { deals: 10, contacts: 10, avg: 1.0, small_n: false };
  ctx.__CT3 = T;
  const t3 = ctAxis(ctFigs(ctDraw('contactUnit = "month"; teamContact({ contact: __CT3 })', 319))[0].body);
  ok(t3.some((x) => /\.5$/.test(x)) && t3.every((x) => /^\d+\.\d$/.test(x)), "0.5 刻みの目盛りを小数1桁でそろえていない: " + t3.join(" "));
});

check("担当者ごとの接触（ループ5 d）: 選んだ人は「担当者: 名前」、持ち案件はどの週の件数かを添え、退職者は印", () => {
  const h = ctDraw('contactUnit = "week"; teamContact({ contact: __CTW })', 319, "hd26f422ffda0");
  const f = ctFigs(h)[0];
  const leg = textOf(f.body.slice(f.body.indexOf("</svg>")));
  ok(leg.includes("担当者: hd26f422ffda0・持ち案件 41 件（直近の確定した週 9/7〜9/13）"),
    "選んだ人の凡例に「担当者:」と、どの週の持ち案件かが無い: " + leg.slice(0, 300));
  ok(leg.includes("全体（担当が決まった案件の合計）") && !/担当者: [^・]*全体/.test(leg), "全体の線に「担当者:」を付けている、または全体の凡例が無い");
  const r = ctFigs(ctDraw('contactUnit = "week"; teamContact({ contact: __CTW })', 319, "担当R"))[0];
  ok(/担当者: 担当R（退職者のまま）/.test(textOf(r.body.slice(r.body.indexOf("</svg>")))), "退職者の凡例に印が無い");
  ok(/<option value="担当R">担当R（退職者のまま）<\/option>/.test(h), "重ねる担当の選択肢に退職者の印が無い");
});

check("ループ5統合: 接触の推移の見出しの2行目も、数字と単位を折れない塊にする（keepNum と両立）", () => {
  /* fix5/cs-residual の keepNum（fig() の入口で hint の数字と単位を <span class="nw"> で包む）と、見出しの 2 行目
     「全体 と 担当者: 名前。全体は持ち案件 N 件（直近の確定した週 …）」を合わせたときの形 */
  const h = ctDraw('contactUnit = "week"; teamContact({ contact: __CTW })', 319, "hd26f422ffda0");
  const f = ctFigs(h)[0];
  ok(f && f.cap === "持ち案件1件あたりの接触（週ごと）", "図の見出しが違う: " + (f && f.cap));
  const hint = (f.body.match(/<span class="hint">([\s\S]*?)<\/span><\/figcaption>/) || [])[1] || "";
  ok(/^全体 と 担当者: hd26f422ffda0。全体は持ち案件 <span class="nw">\d+ 件<\/span>（直近の確定した週 9\/7〜9\/13）$/.test(hint),
    "2行目の「N 件」が折れない塊になっていないか、形が崩れた: " + hint);
  /* 名前は keepNum の対象外（ハッシュの数字を包まない） */
  ok(!/hd26f422ffda0<\/span>|<span class="nw">[^<]*hd26/.test(hint), "名前まで包んでいる");
});

/* ================================================================ 担当の交代 × 交代の前後の接触（2026-09-24 藤巻さんの要望） */
/* 入力の形は routes.rs build_handover の contact_cmp / rows[].contact のまま。値は見張りのために置いたもの。
   - 拠点 S1 の交代（2026-07-01）が2行（同じ event）。比べられた・減った
   - S2 の交代（2026-06-01）は比べられた・増えた
   - 短い（通話の記録の前）1行・途中 1行・数えていない（null）1行
   - 引き継いだ側: 担当P（比べられた 6件 → 図に出す）/ 担当Q（1件 → 少）/ 氏名不明が2人 */
{
  /* 窓は担当期間の全体（通期。2026-09-24）。start / end は数えた最初の日・最後の日 */
  const cw = (d, c, st, en) => ({ days: d, contacts: c, per30: d ? c * 30 / d : null, start: st || null, end: en || null });
  const cmp = (status, why, b, a, ev, dir, ongoing) => ({ status, why, before: b, after: a, event: ev, ongoing: !!ongoing,
    change: b.days && a.days ? a.per30 - b.per30 : null, dir: dir || null });
  const HC = JSON.parse(JSON.stringify(ctx.__HO));
  HC.rows = [
    hoRow({ deal_id: "40000000001", name: "拠点S1の前の契約", date: "2026-07-01", contact: cmp("ok", null, cw(60, 3, "2026-05-02", "2026-06-30"), cw(60, 1, "2026-07-01", "2026-08-29"), "site:S1|2026-07-01", "down") }),
    hoRow({ deal_id: "40000000002", name: "拠点S1の継続の契約", date: "2026-07-01", contact: cmp("ok", null, cw(60, 3, "2026-05-02", "2026-06-30"), cw(60, 1, "2026-07-01", "2026-08-29"), "site:S1|2026-07-01", "down") }),
    /* S2 は後の担当がまだ担当中（締め日までの通期で比べた） */
    hoRow({ deal_id: "40000000003", name: "拠点S2の契約", date: "2026-06-01", contact: cmp("ok", null, cw(50, 1, "2026-04-01", "2026-05-31"), cw(60, 2, "2026-06-01", "2026-09-13"), "site:S2|2026-06-01", "up", true) }),
    hoRow({ deal_id: "40000000004", name: "拠点S3の契約", date: "2026-04-10", contact: cmp("short", "calls", cw(18, 0), cw(60, 4), "site:S3|2026-04-10", "up") }),
    hoRow({ deal_id: "40000000005", name: "拠点S4の契約", date: "2026-08-01", contact: cmp("provisional", null, cw(60, 2), cw(45, 1), "site:S4|2026-08-01", "down") }),
    hoRow({ deal_id: "40000000006", name: "拠点S5の契約", date: "2026-08-02", contact: null }),
  ];
  HC.meta.n = HC.rows.length;
  const person = (o) => Object.assign({ label: "", unresolved: false, unresolved_no: null, n_events: 1, n_ok: 1,
    n_up: 0, n_down: 1, n_same: 0, n_short: 0, n_provisional: 0, median_change: -1, small: true }, o);
  HC.contact_cmp = {
    n_events: 4, n_ok: 2, n_up: 1, n_down: 1, n_same: 0, n_short: 1, n_provisional: 1,
    short_why: { calls: 1, before: 0, after: 0, overlap: 0, gap: 0 }, mean_change: -0.25, median_change: -0.3, n_ongoing: 1,
    overlap_days: 10, overlap_events: 1,
    by_to: [
      person({ label: "担当P", n_events: 7, n_ok: 6, n_up: 2, n_down: 4, mean_change: -0.65, median_change: -0.8, small: false }),
      person({ label: "担当Q" }),
      person({ label: "氏名が分からない担当（HubSpotの担当者一覧に無い）", unresolved: true, unresolved_no: 1 }),
      person({ label: "氏名が分からない担当（HubSpotの担当者一覧に無い）", unresolved: true, unresolved_no: 2, median_change: 0.4, n_up: 1, n_down: 0 }),
    ],
    by_from: [person({ label: "担当R", n_ok: 2, n_events: 2, n_up: 1, n_down: 1, median_change: -0.3 })],
    meta: { window: "tenure", min_window_days: 30, per_days: 30, min_person_n: 5, call_from: "2026-03-23",
      last_day: "2026-09-13", n_unavailable_rows: 1, n_ambiguous: 0 },
    /* routes.rs contact_cmp_json の文そのもの */
    not_causal: "交代が接触を減らした・増やした証拠ではありません。危ない案件だから担当を替えた可能性もあり、向きは決まりません（過去の検証で、同じ取引に偽の交代日を置いた比較対象と並べると、前の接触の量をそろえたところで差が消えました）。接触は検知専用で、多いほど良いという評価でもありません",
    rule: "交代ごとに、それぞれの担当期間の全体（通期）の接触を30日あたりに直して比べています。",
    dedupe_rule: "まとめでは、同じ拠点・同じ交代日の行を1件の交代と数えています。",
    dir_rule: "比べられた交代が 5 件未満の人には印を付け、図には出していません",
  };
  ctx.__HOC = HC;
}
/* 交代の前後の節（問い〜末尾）だけを切り出す。2026-09-28（S-8）に表「交代の記録」・図「反映されたか」の後ろへ
   移したので、後ろに残るのは末尾の枠（foot）だけ。前は「反映されたか」の手前で切っていた */
const hocPart = (h) => h.slice(h.indexOf("交代の前後で、接触は増えたか減ったか"));
/* 表「交代の記録」（#ho-tbl）だけを切り出す。前は最後の <table> を取っていたが、S-8 で交代の前後の節（表を含む）が後ろに来た */
const hoTable = (h) => { const i = h.indexOf('<table id="ho-tbl"'); return i < 0 ? "" : h.slice(i, h.indexOf("</table>", i)); };

check("交代の前後: 図より前に「交代の効果の証拠ではない・向きは決まらない」と断る", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  ok(p.length > 0, "交代の前後の節が無い");
  const at = p.indexOf("証拠ではありません");
  ok(at >= 0 && at < p.indexOf("<figure"), "図より前に断りが無い");
  const t = textOf(p.slice(0, p.indexOf("<figure")));
  for (const w of ["危ない案件だから担当を替えた可能性", "向きは決まりません", "差が消えました", "検知専用"])
    ok(t.includes(w), "断りに「" + w + "」が無い");
  ok(p.indexOf('<div class="note warn">') >= 0 && p.indexOf('<div class="note warn">') < p.indexOf("<figure"), "断りが注意の枠に入っていない");
});

check("交代の前後: 増減を良し悪しの色（赤・緑・山吹）や ▲▼ で示さず、符号と言葉で書く", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  ok(!/var\(--(hi|midori|ki)\)/.test(p), "判定の色（赤・緑・山吹）を使っている");
  ok(!/&#9650;|&#9660;|▲|▼|is-bad|is-good|note bad|note good/.test(p), "▲▼ や良し悪しの印を使っている");
  const svgs = [...p.matchAll(/<svg[\s\S]*?<\/svg>/g)].map((m) => m[0]);
  ok(svgs.length >= 1, "図が無い");
  const fills = new Set(svgs.join("").match(/<rect [^>]*style="fill:([^"]+)"/g) || []);
  ok([...fills].every((f) => f.includes("var(--ai)")), "棒の色が1色（藍）でない: " + [...fills].join(" "));
  ok(/−1\.00 減った/.test(svgs[0]) && /\+0\.40 増えた/.test(svgs[0]), "棒の値に符号と言葉が無い");
});

check("交代の前後: 同じ交代の行は1本にし、短い・途中・数えていない交代は図に出さない", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const svg = firstSvg(p);
  const bars = (svg.match(/<rect [^>]*style="fill:var\(--ai\)"/g) || []).length;
  ok(bars === 2, "交代ごとの棒が " + bars + " 本（期待 2。S1 の2行は1本）");
  ok(!/拠点S3|拠点S4|拠点S5/.test(svg), "短い・途中・数えていない交代を図に出している");
  ok(p.includes("交代ごとの変化（比べられた 2 件）"), "図の見出しに比べられた件数が無い");
  const t = textOf(p);
  ok(t.includes("比べるには短い 1 件") && t.includes("通話の記録が始まる前の日 1 件"), "短い交代の件数と理由を書いていない");
  ok(t.includes("本体案件が重なる日が多い") && t.includes("いちばん多い理由"), "短い理由に「重なり」が無い、または理由の決め方を書いていない");
  ok(t.includes("窓から外した日が、交代 1 件で合わせて 10 日"), "重なりで外した日数を書いていない");
  ok(t.includes("途中 1 件") && t.includes("2026-09-13 まで"), "途中の件数と、数えた最後の日を書いていない");
  /* 2026-09-25: 注記は「前 → 後（日数 / 日数）」だけにし、担当期間（開始〜終了）は棒の説明（title）に回した（右端からはみ出したため） */
  ok(/>前 1\.50 → 後 0\.50（60日 \/ 60日）<\/text>/.test(svg), "注記が「前 → 後（前の日数 / 後の日数）」の形でない");
  ok(/<title>[^<]*前の担当期間 2026-05-02〜2026-06-30（60日）→ 後の担当期間 2026-07-01〜2026-08-29（60日）<\/title><\/rect>/.test(svg), "棒の説明（title）に担当期間（開始〜終了）と日数が無い");
  ok(!/<text[^>]*>[^<]*2026-05-02〜/.test(svg), "担当期間（開始〜終了）を注記に残している");
});

check("交代の前後: 担当者のまとめは母数を添え、少ない人は印を付けて図に出さない。氏名不明は番号で分ける", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const i = p.indexOf("引き継いだ側（次の担当）（変化の中央値）");
  ok(i >= 0, "引き継いだ側のまとめが無い");
  ok(p.indexOf("引き継がれた側（前の担当）（変化の中央値）") > i, "引き継がれた側のまとめが無い");
  const part = p.slice(i, p.indexOf("引き継がれた側（前の担当）"));
  const svg = firstSvg(part);
  ok(svg.includes("担当P") && !svg.includes("担当Q") && !svg.includes("氏名不明"), "母数が 5 件未満の人を図に出している、または足りる人を出していない");
  const tb = part.slice(part.indexOf("<tbody>"));
  const rowOf = (name) => tb.slice(tb.indexOf(name), tb.indexOf("</tr>", tb.indexOf(name)));
  ok(/>少<\/span>/.test(rowOf("担当Q")), "母数が小さい人に「少」の印が無い");
  ok(!/>少<\/span>/.test(rowOf("担当P")), "母数が足りる人に「少」の印が付いている");
  ok(/6 <span class="muted small">\/ 7件<\/span>/.test(rowOf("担当P")), "比べられた件数に母数（交代の件数）を添えていない");
  ok(rowOf("担当P").includes("−0.80"), "変化の中央値に符号が無い");
  ok(/氏名不明 1<\/span>/.test(tb) && /氏名不明 2<\/span>/.test(tb), "氏名の分からない担当が2人いるのに番号で分けていない");
  ok(!part.includes("HubSpotの担当者一覧に無い）"), "長い「氏名が分からない担当（…）」をそのまま出している");
  ok(!/@/.test(p), "メールアドレスが出ている");
});

/* 検証の指摘（2026-09-24）: 見出しの数字「比べられた交代」から母数（交代 N 件のうち）を消しても落ちなかった */
check("交代の前後: 見出しの数字「比べられた交代」に母数（交代 N 件のうち）と外した件数を添える", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const k = p.slice(p.indexOf('<div class="kpis">'), p.indexOf("交代ごとの変化"));
  const box = k.slice(k.indexOf("比べられた交代"), k.indexOf("減った / 増えた"));
  ok(box.length > 0, "「比べられた交代」の枠が無い");
  ok(textOf(box).includes("交代 4 件のうち"), "比べられた交代に母数（交代 4 件のうち）が無い: " + textOf(box));
  ok(textOf(box).includes("比べるには短い 1 件") && textOf(box).includes("途中 1 件"), "外した件数（短い・途中）が無い");
  ok(textOf(k).includes("比べられた 2 件のうち"), "減った / 増えた に母数が無い");
});

/* 2026-09-24 藤巻さんの判断: 窓は前後60日ではなく、それぞれの担当期間の全体（通期）。まとめは平均と中央値（最頻値は出さない） */
check("交代の前後: 窓は「それぞれの担当期間の全体（通期）」と書き、「前後60日」と書かない", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const t = textOf(p);
  ok(t.includes("それぞれの担当期間の全体（通期）"), "窓の定義（通期）を書いていない");
  ok(t.includes("一つ前の交代") && t.includes("次の交代の前日"), "担当期間の区切り（一つ前の交代・次の交代の前日）を書いていない");
  ok(!/60 ?日/.test(t.replace(/（60日・/g, "").replace(/前 60日 \/ 後 60日/g, "")), "「60日」の窓の言葉が残っている");
  ok(t.includes("後の担当期間 − 前の担当期間"), "図の見出しが担当期間の差になっていない");
  ok(!/undefined|NaN/.test(p), "undefined か NaN が出ている");
});

check("交代の前後: 変化の平均と中央値を両方出し、最頻値は出さない理由を書く", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const k = p.slice(p.indexOf('<div class="kpis">'), p.indexOf("交代ごとの変化"));
  const box = textOf(k.slice(k.indexOf("変化の平均 / 中央値")));
  ok(box.includes("−0.25 / −0.30"), "見出しの数字に平均と中央値の両方が無い: " + box);
  ok(box.includes("最頻値は出していません"), "最頻値を出さないことを書いていない");
  const part = p.slice(p.indexOf("引き継いだ側（次の担当）（変化の中央値）"), p.indexOf("引き継がれた側（前の担当）"));
  const head = part.slice(part.indexOf("<thead"), part.indexOf("</thead>"));
  ok(head.includes(">変化の平均（30日あたり）<") && head.includes(">変化の中央値（30日あたり）<"), "担当者の表に平均と中央値の列が無い");
  const tb = part.slice(part.indexOf("<tbody>"));
  const row = tb.slice(tb.indexOf("担当P"), tb.indexOf("</tr>", tb.indexOf("担当P")));
  ok(/−0\.65<\/td><td class="n">−0\.80/.test(row), "担当者の行に平均（−0.65）と中央値（−0.80）が並んでいない: " + row);
});

check("交代の前後: 後の担当がまだ担当中の交代は、締め日までの通期で比べたと書く。交代どうしの間が短い理由も出す", () => {
  const p = hocPart(run("renderHandover(__HOC)"));
  const t = textOf(p);
  ok(t.includes("うち 1 件は後の担当はまだ担当中（締め日までの通期）"), "比べられたうち担当中の件数を書いていない");
  const svg = firstSvg(p);
  /* 狭い幅では注記が折り返されるので、文字だけをつないで見る */
  const flat = svg.replace(/<[^>]+>/g, "").replace(/\s+/g, "");
  /* 注記には短い印（後は担当中）、棒の説明（title）には断りの全文 */
  ok(flat.includes("前0.60→後1.00（50日/60日・後は担当中）"), "担当中の交代の注記に短い印が無い: " + flat);
  ok(flat.includes("後の担当期間2026-06-01〜2026-09-13（60日）。後の担当はまだ担当中（締め日までの通期）"), "担当中の交代の棒の説明に断りが無い: " + flat);
  ok(!flat.includes("後0.50（60日/60日・後は担当中）") && !flat.includes("2026-08-29（60日）。後の担当"), "担当中でない交代に断りが付いている");
  ok(textOf(p).includes("後は担当中」の交代は、後の担当はまだ担当中"), "凡例に短い印「後は担当中」の意味を書いていない");
  ok(t.includes("同じ拠点の交代どうしの間が短い"), "短い理由に「交代どうしの間が短い」が無い");
});

/* 検証の指摘（2026-09-24）: 氏名不明の番号が、表は番号なし・まとめは側ごとに別の振り方だった */
check("交代の前後: 氏名不明の番号はサーバの番号をそのまま出し、交代の表にも同じ番号を出す", () => {
  const H = JSON.parse(JSON.stringify(ctx.__HOC));
  H.meta.n_unresolved_people = 2;
  H.rows[0].to_unresolved = true; H.rows[0].to_label = "氏名が分からない担当（HubSpotの担当者一覧に無い）"; H.rows[0].to_unresolved_no = 2;
  H.contact_cmp.by_from = [{ label: "氏名が分からない担当（HubSpotの担当者一覧に無い）", unresolved: true, unresolved_no: 2, n_events: 1, n_ok: 1,
    n_up: 0, n_down: 1, n_same: 0, n_short: 0, n_provisional: 0, median_change: -1, small: true }];
  ctx.__HOU = H;
  const h = run("renderHandover(__HOU)");
  /* 引き継がれた側に氏名不明が1人だけでも、全体で2人いれば番号を出す（側ごとに数えない） */
  const from = h.slice(h.indexOf("引き継がれた側（前の担当）（変化の中央値）"), h.indexOf("前後の比べ方"));
  ok(/氏名不明 2<\/span>/.test(from), "引き継がれた側の氏名不明に、サーバの番号（2）を出していない");
  const tbl = hoTable(h);
  ok(tbl.length > 0, "交代の記録の表（#ho-tbl）が無い");
  const body = tbl.slice(tbl.indexOf("<tbody>"));
  const row = body.slice(body.indexOf("拠点S1の前の契約"), body.indexOf("</tr>", body.indexOf("拠点S1の前の契約")));
  ok(/氏名不明 2<\/span>/.test(row), "交代の表の氏名不明に番号が無い: " + row);
  ok(h.includes("上の担当者ごとのまとめと同じ人に同じ番号"), "番号が表とまとめで共通だと書いていない");
  /* 1人だけなら番号は出さない */
  H.meta.n_unresolved_people = 1; ctx.__HOU = H;
  ok(!/氏名不明 [0-9]/.test(run("renderHandover(__HOU)")), "氏名不明が1人なのに番号を出している");
});

check("交代の前後: 交代の表に「接触の前後」の列を足し、既存の列（記録の遅れ・反映・状態）は残す", () => {
  const h = run("renderHandover(__HOC)");
  const tbl = hoTable(h);
  ok(tbl.length > 0, "交代の記録の表（#ho-tbl）が無い");
  const head = tbl.slice(0, tbl.indexOf("</thead>"));
  for (const c of ["交代日", "前の担当", "次の担当", "いまの担当", "接触の前後（30日あたり）", "記録の遅れ", "反映", "状態"])
    ok(head.includes(">" + c + "<"), "表の列「" + c + "」が無い");
  const body = tbl.slice(tbl.indexOf("<tbody>"));
  const rowOf = (name) => body.slice(body.indexOf(name), body.indexOf("</tr>", body.indexOf(name)));
  /* 「接触の前後」のます（「→」を含む数の列）だけを取り出す */
  const cmpCell = (name) => (rowOf(name).match(/<td class="n">([^<]*→[\s\S]*?)<\/td>/) || [])[1] || "";
  ok(/1\.50 → 0\.50 <span class="muted small" title="前 2026-05-02〜2026-06-30 \/ 後 2026-07-01〜2026-08-29">前 60日 \/ 後 60日<\/span> 減った −1\.00$/.test(cmpCell("拠点S1の前の契約")), "比べられた行の前後（日数と期間）が出ていない: " + rowOf("拠点S1の前の契約"));
  ok(/増えた \+0\.40 <span class="tag" title="後の担当はまだ担当中（締め日までの通期）">担当中<\/span>/.test(rowOf("拠点S2")), "後の担当がまだ担当中の行に印が無い: " + rowOf("拠点S2"));
  ok(!/担当中/.test(cmpCell("拠点S1の前の契約")), "担当中でない行に担当中の印が付いている");
  ok(/>短い<\/span>/.test(rowOf("拠点S3")) && /前 18日/.test(rowOf("拠点S3")), "短い行に印と日数が無い");
  ok(/>途中<\/span>/.test(rowOf("拠点S4")), "途中の行に印が無い");
  ok(rowOf("拠点S5").includes("数えていない") && !/0\.00/.test(rowOf("拠点S5")), "数えていない行を 0 回として出している");
  /* 前後の接触が無い応答（古いサーバ）でも落ちない */
  ok(!run("renderHandover(__HO)").includes("交代の前後で、接触は増えたか減ったか"), "contact_cmp が無いのに節を出している");
});

check("交代の前後: 左へ長く伸びた負の棒の値をラベル欄に重ねず、0 の線の右に置く（400px 実機）", () => {
  const svg = run('svgBarH({ w: 680, fmt: F.d1, diverging: true, rows: [' +
    '{ label: "2026-06-01 (伏字)1109", v: -7, txt: "−7.00 減った", color: C.ai },' +
    '{ label: "2026-05-20 (伏字)2582", v: -0.53, txt: "−0.53 減った", color: C.ai },' +
    '{ label: "2026-07-08 (伏字)3241", v: 1, txt: "+1.00 増えた", color: C.ai }] })');
  const x0 = +(/<line class="axisline" x1="([0-9.]+)"/.exec(svg) || [])[1];
  const at = (t) => { const m = new RegExp('<text class="vl" x="([0-9.]+)"[^>]*text-anchor="(start|end)">' + t).exec(svg); return m && { x: +m[1], a: m[2] }; };
  const big = at("−7.00"), small = at("−0.53");
  ok(big && small && x0 > 0, "値の文字か 0 の線が取れない");
  ok(big.a === "start" && big.x > x0, "左端まで伸びた棒の値をラベル欄側（棒の左）に置いている: x=" + big.x + " / 0 の線 " + x0);
  ok(small.a === "end" && small.x < x0, "短い負の棒の値は今までどおり棒の左に置く");
});

/* 検証の指摘（2026-09-24, 400px）: 0 を中心にした軸の目盛りが「-10.0-5.0 0.0 5.0 10.0」と詰まってつながり、
   負号が ASCII のハイフンで棒の値の「−」と揃っていなかった */
check("0 を中心にした横棒: 目盛りの負号は「−」で、狭い幅でも隣の目盛りの文字と重ねない", () => {
  for (const w of [360, 400, 480, 680, 1000]) {
    const svg = run('svgBarH({ w: ' + w + ', padL: 120, fmt: F.d1, diverging: true, rows: [' +
      '{ label: "a", v: -7, txt: "−7.00 減った", color: C.ai },' +
      '{ label: "b", v: 1, txt: "+1.00 増えた", color: C.ai }] })');
    const labs = [...svg.matchAll(/<text class="ax" x="([0-9.]+)" y="[0-9.]+" text-anchor="middle">([^<]*)<\/text>/g)]
      .map((m) => ({ x: +m[1], s: m[2] }));
    ok(labs.length >= 3, "w=" + w + ": 目盛りの文字が両端と 0 より少ない");
    ok(labs.every((l) => !/^-/.test(l.s)), "w=" + w + ": 目盛りの負号が ASCII のハイフン: " + labs.map((l) => l.s).join(" "));
    ok(labs.some((l) => /^−/.test(l.s)), "w=" + w + ": 負の目盛りが無い");
    for (let i = 1; i < labs.length; i++) {
      const gap = labs[i].x - labs[i - 1].x - (run("textW")(labs[i].s) + run("textW")(labs[i - 1].s)) / 2;
      ok(gap >= 4, "w=" + w + ": 目盛り「" + labs[i - 1].s + "」と「" + labs[i].s + "」が詰まっている（間 " + gap.toFixed(1) + "px）");
    }
  }
});

/* 検証の指摘（2026-09-24, 1440px）: 棒の左に余白があるのに「−7.00 減った」だけが 0 の線の右に出た。
   ラベル欄の右端（padL − 9）から 5px 空けて入るなら棒の左、入らないときだけ 0 の線の右 */
check("0 を中心にした横棒: 負の値の文字は、入るなら棒の左に置き、入らないときだけ 0 の線の右に置く", () => {
  let left = 0, right = 0;
  for (let w = 400; w <= 1440; w += 20) {
    const padL = 200;
    const svg = run('svgBarH({ w: ' + w + ', padL: ' + padL + ', fmt: F.d1, diverging: true, rows: [' +
      '{ label: "2026-06-01 (伏字)1109", v: -7, txt: "−7.00 減った", color: C.ai },' +
      '{ label: "2026-07-08 (伏字)3241", v: 7.4, txt: "+7.40 増えた", color: C.ai }] })');
    const x0 = +(/<line class="axisline" x1="([0-9.]+)"/.exec(svg) || [])[1];
    const bar = +(/<rect x="([0-9.]+)"[^>]*style="fill:var\(--ai\)"/.exec(svg) || [])[1];
    const m = /<text class="vl" x="([0-9.]+)"[^>]*text-anchor="(start|end)">−7\.00/.exec(svg);
    ok(m && x0 > 0 && bar > 0, "w=" + w + ": 値の文字・0 の線・棒が取れない");
    const fits = bar - 6 - run("textW")("−7.00 減った") >= padL - 4;
    if (fits) {
      left++;
      ok(m[2] === "end" && +m[1] < x0, "w=" + w + ": 棒の左に入るのに 0 の線の右に置いている（棒の左端 " + bar + "）");
    } else {
      right++;
      ok(m[2] === "start" && +m[1] > x0, "w=" + w + ": 棒の左に入らないのに棒の左（ラベル欄側）に置いている");
    }
  }
  ok(left > 0 && right > 0, "左に置く幅と右に置く幅の両方を通っていない（left " + left + " / right " + right + "）");
});

/* 🔴 2026-09-25 本番（394bfda）実測: 担当の交代「交代ごとの接触の変化」で、行の注記
   「前 1.69（124日・2026-04-22〜2026-08-23）→ 後 1.00（30日・2026-…）」が SVG の右端からはみ出し、
   日付の終わりが切れていた（1440px で SVG 幅 1104 に対し注記の右端 1117〜1131 が 14 か所、400px で 21 か所）。
   注記の text が viewBox の外に出ないことを、1440 相当（avail 1104）と 400 相当（319）で確かめる。
   文字幅は画面と同じ textW で見積もる（折り返した tspan は1行ずつ見る） */
const noteOverflow = (svg) => {
  const W = figW(svg);
  const tw = run("textW");
  const out = [];
  for (const m of svg.matchAll(/<text class="ax"(?![^>]*text-anchor)([^>]*)>([\s\S]*?)<\/text>/g)) {
    const x = +(/ x="([0-9.]+)"/.exec(m[1]) || [])[1];
    const lines = /<tspan/.test(m[2])
      ? [...m[2].matchAll(/<tspan([^>]*)>([^<]*)<\/tspan>/g)].map((t) => ({ x: +((/ x="([0-9.]+)"/.exec(t[1]) || [])[1] || x), s: t[2] }))
      : [{ x, s: m[2].replace(/<[^>]+>/g, "") }];
    lines.forEach((l) => {
      const s = l.s.replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'");
      const right = l.x + tw(s);
      if (!(right <= W + 0.5)) out.push(s + "（右端 " + right.toFixed(1) + " / 幅 " + W + "）");
    });
  }
  return { W, out };
};
{
  const cw = (d, c, st, en) => ({ days: d, contacts: c, per30: d ? c * 30 / d : null, start: st || null, end: en || null });
  const H = JSON.parse(JSON.stringify(ctx.__HOC));
  const names = ["（伏字）ケアサポートかがやき居宅介護支援事業所 継続②", "（伏字）三菱ケミカルテクニカ株式会社 鹿児島工場",
    "（伏字）社会福祉法人さくら会 特別養護老人ホームさくらの里", "（伏字）宮崎商会 鹿児島工場 本体"];
  H.rows = names.map((nm, i) => hoRow({ deal_id: "5000000000" + i, name: nm, date: "2026-08-2" + i,
    contact: { status: "ok", why: null, event: "site:L" + i + "|2026-08-2" + i, ongoing: i % 2 === 0,
      before: cw(124, 7, "2026-04-22", "2026-08-23"), after: cw(30, 1, "2026-08-24", "2026-09-22"),
      change: 30 / 30 - 7 * 30 / 124, dir: "down" } }));
  /* 変化が2桁の行（値の文字が長く、400px 相当で注記を棒の下に回せない幅）も入れる */
  H.rows.push(hoRow({ deal_id: "50000000009", name: "（伏字）医療法人 あおば会 あおば訪問看護ステーション", date: "2026-08-19",
    contact: { status: "ok", why: null, event: "site:L9|2026-08-19", ongoing: true,
      before: cw(124, 50, "2026-04-22", "2026-08-23"), after: cw(30, 1, "2026-08-24", "2026-09-22"),
      change: 30 / 30 - 50 * 30 / 124, dir: "down" } }));
  ctx.__HOL = H;
}
check("交代の前後: 長い注記の行でも、注記が図（viewBox）の右端から出ない（1440 相当 / 400 相当）", () => {
  for (const av of [1104, 319]) {
    ctx.__AV = Object.fromEntries(Array.from({ length: 40 }, (_, i) => [i, av]));
    const h = run("FIGFIT.seq = 0; FIGFIT.avail = __AV; try { renderHandover(__HOL) } finally { FIGFIT.avail = null; FIGFIT.seq = 0; }");
    const at = h.lastIndexOf("<svg", h.indexOf('aria-label="交代ごとの接触の変化"'));
    ok(h.includes('aria-label="交代ごとの接触の変化"') && at >= 0, "avail " + av + ": 図「交代ごとの接触の変化」が無い");
    const svg = firstSvg(h.slice(at));
    ok(/前 1\.69/.test(svg), "avail " + av + ": 見張りの前提: 注記（前 1.69 …）が図に無い");
    const r = noteOverflow(svg);
    ok(r.W > 0, "avail " + av + ": 図の幅が取れない");
    ok(!r.out.length, "avail " + av + ": 注記が図の右端から出ている " + r.out.length + " か所: " + r.out.slice(0, 3).join(" / "));
  }
});
check("横棒（svgBarH）: 右の注記が枠に入らないときも、どの幅でも注記を図の外に出さない（汎用の歯止め）", () => {
  const long = "前 1.69（124日・2026-04-22〜2026-08-23）→ 後 1.00（30日・2026-08-24〜2026-09-22）。後の担当はまだ担当中（締め日までの通期）";
  for (const av of [1104, 900, 680, 480, 400, 319]) {
    const svg = drawAt('svgBarH({ w: 680, fmt: F.d1, diverging: true, rows: [' +
      '{ label: "2026-08-23 （伏字）ケアサポート", v: -0.69, txt: "−0.69 減った", note: ' + JSON.stringify(long) + ' },' +
      '{ label: "2026-07-01 （伏字）宮崎商会", v: 0.4, txt: "+0.40 増えた", note: "前 1.50 → 後 1.90（60日 / 60日）" },' +
      '{ label: "2026-08-19 （伏字）あおば会", v: -11.1, txt: "−11.10 減った", note: "前 12.10 → 後 1.00（124日 / 30日・後は担当中）" }] })', av);
    const r = noteOverflow(svg);
    ok(r.W > 0, "avail " + av + ": 図の幅が取れない");
    ok(!r.out.length, "avail " + av + ": 注記が図の右端から出ている: " + r.out.join(" / "));
    const flat = svg.replace(/<[^>]+>/g, "").replace(/\s+/g, "");
    ok(flat.includes(long.replace(/\s+/g, "")), "avail " + av + ": 注記の文字が欠けている（折り返しで落とした）");
  }
});

/* ================================================================ 案件の詳細（2026-09-26） */
/* 入力の形は deal_detail.rs build_deal_detail のまま。値は見張りのために置いたもの（名前は合成） */
function ddPayload() {
  const at = (state, extra) => Object.assign({ state, in_span: false, moved_from: null, moved_to: null }, extra || {});
  const call = (id, a, x) => Object.assign({ kind: "call", date: "2026-09-10", time: "10:00", fact: true,
    source_label: "通話記録（事実）", call_id: id, duration_sec: 90, contact: true, direction: "inbound",
    handler: null, owner: "担当B", has_transcript: false, summary: null, attach: a }, x || {});
  return {
    meta: { today: "2026-09-18", found: true, deal_id: "80000000001", reason: null, summary_sheet: "ok" },
    deal: { deal_id: "80000000001", name: "見張りの案件 <b>太字</b>", site: null, stage: "解約済", kind: "(新規)",
      start: "2026-01-01", expiration: "2026-06-30", period: null, consultant: null, consultant_retired: false,
      amount: null, renewal_no: 0, is_active: false, right_censored: false, flags: null },
    chain: { has_site: false, position: 0, prev: null, next: null, rows: [] },
    counts: { mtg: 1, mtg_extracted: 0, mail_mtg: 2, mail_not_held: [], call: 4, call_contact: 4, call_transcript: 0,
      call_summarized: 0, handover: 0, call_moved_in: 1, call_outside: 3 },
    events: [
      call("k1", at("moved_in", { in_span: true, moved_from: { deal_id: "80000000002", name: "継続の契約", relation: "later" } }), { date: "2026-06-01" }),
      call("k2", at("moved_out", { moved_to: { deal_id: "80000000002", name: "継続の契約" } }), { date: "2026-08-01" }),
      call("k3", at("ambiguous"), { date: "2026-07-20" }),
      call("k4", at("outside"), { date: "2025-12-01" }),
      { kind: "mail_mtg", date: "2026-03-10", time: null, fact: false, source_label: "メール由来（推定）",
        certainty: "推定(±1日 83.3%)", same_day_recording: false },
      { kind: "mail_mtg", date: "2026-03-11", time: null, fact: false, source_label: "メール由来（推定）",
        certainty: "推定(±1日 83.3%)", same_day_recording: true },
      { kind: "mtg", date: "2026-03-10", time: "10:00", fact: true, source_label: "Zoom 録画（事実）", subject: null,
        host: null, minutes: null, mtg_type: null, extracted: false, todo: null, concern: null, positive: null,
        risk: null, risk_reason: null, next: null, attach: at("own", { in_span: true }) },
    ],
    rules: { fact: "事実と推定の決まり", attach: "付け直しの決まり" },
  };
}
const ddItems = (h) => h.slice(h.indexOf('<ol class="tl"'), h.indexOf("</ol>")).split("<li ").slice(1);

check("案件の詳細: 推定（メール由来）の行は必ず推定の印と確かさを付け、事実の行には付けない", () => {
  ctx.__DD = ddPayload();
  const h = run("detailAllCalls = true; detailOlder = true; try { renderDetail(__DD) } finally { detailAllCalls = false; detailOlder = false; }");
  const items = ddItems(h);
  ok(items.length === 7, "7 行のはずが " + items.length);
  for (const it of items) {
    const est = /^class="[^"]*\best\b/.test(it);
    const isMail = it.indexOf("MTG（推定）") >= 0;
    ok(est === isMail, "推定の印（est）とメール由来が一致しない: " + it.slice(0, 80));
    /* M-3 (4)（2026-09-29）: 確かさと断りは凡例に1回。行には札（MTG（推定））と◇、札の title に同じ文 */
    if (isMail) ok(/<span class="mark" title="[^"]*推定\(±1日 83\.3%\)[^"]*録画のような中身はありません[^"]*">MTG（推定）<\/span>/.test(it),
      "メール由来の行の札に確かさか断り（title）が無い");
    if (isMail) ok(it.replace(/<[^>]*>/g, "").indexOf("メールの文面から起こした") < 0, "メール由来の行ごとに定型の断りを繰り返している");
    else ok(it.indexOf("推定") < 0, "事実の行に「推定」が混ざっている: " + it.slice(0, 80));
  }
  // 凡例は形と文で（色だけにしない）
  ok(/<div class="legend"><i>&#9679; 事実[^<]*<\/i><i>&#9671; 推定/.test(h), "凡例に事実（●）と推定（◇）の文が無い");
  // 確かさと断りは凡例に1回だけ（見える文字として。行の title は数えない）
  const vis = h.replace(/<[^>]*>/g, "");
  ok((vis.match(/メールの文面から起こした実施日です（推定\(±1日 83\.3%\)）。録画のような中身はありません/g) || []).length === 1,
    "凡例に確かさと断りが1回だけ出ていない");
});

check("案件の詳細: 付け直しの印は4通りを言い分け、そのままの行には何も付けない", () => {
  ctx.__DD = ddPayload();
  const items = ddItems(run("detailAllCalls = true; detailOlder = true; try { renderDetail(__DD) } finally { detailAllCalls = false; detailOlder = false; }"));
  const by = (d) => items.find((x) => x.indexOf("<b>" + d + "</b>") >= 0) || "";
  ok(/付け直し: 継続の取引「<a class="deallink" href="#deal\/detail\?id=80000000002">継続の契約<\/a>」/.test(by("2026-06-01")),
    "付け直して来た行に、元の取引へのリンクが無い");
  ok(by("2026-08-01").indexOf("の期間の記録として数えています") >= 0, "出ていった行の印が無い");
  ok(by("2026-07-20").indexOf("決められない") >= 0, "決められない行の印が無い");
  ok(by("2025-12-01").indexOf("その日に動いている同じ拠点の契約がありません") >= 0, "期間の外の行の印が無い");
  const own = by("2026-03-10");
  ok(own && own.indexOf('class="via">&#8618;') < 0 && own.indexOf("&#9888;") < 0, "そのままの行に付け直しの印が付いている");
});

check("案件の詳細: 抽出前の MTG は「未抽出」と出し、空の項目を — で並べない（記録が無いと読ませない）", () => {
  ctx.__DD = ddPayload();
  const items = ddItems(run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }"));
  const m = items.find((x) => x.indexOf('<span class="mark">MTG</span>') >= 0);
  ok(m, "録画の MTG の行が無い");
  ok(m.indexOf("未抽出") >= 0 && m.indexOf("記録が無いのではありません") >= 0, "未抽出と書いていない");
  ok(m.indexOf("<dl>") < 0, "未抽出なのに項目の表（— の並び）を出している");
});

check("案件の詳細: 名前はエスケープし、値の無い項目は — や断りで出す（undefined・null・取引IDを出さない）", () => {
  ctx.__DD = ddPayload();
  const h = run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }");
  ok(h.indexOf("<b>太字</b>") < 0 && h.indexOf("&lt;b&gt;太字&lt;/b&gt;") >= 0, "案件名をエスケープしていない");
  ok(!/undefined|NaN|>null</.test(h), "undefined / NaN / null が出ている");
  ok(h.indexOf("稼働中の案件にだけ付けています") >= 0, "名札が無い理由を書いていない");
  ok(h.indexOf("拠点名なし") >= 0 && h.indexOf("前後の契約はたどれません") >= 0, "拠点が無いときの断りが無い");
  ok(!/>[^<]*80000000001[^<]*</.test(h), "取引ID が画面の文字に出ている");
  ok(h.indexOf("初回") >= 0, "継続回数 0 を「初回」と出していない");
});

check("案件の詳細: 表の案件名（案件そのもの・継続を追いかける・法人番号で見る）は案件の詳細へのリンク", () => {
  const b = run('BOARD_COLS.find((c) => c.k === "name").fmt({ deal_id: "80000000001", name: "A&B" })');
  ok(b.indexOf('<a class="deallink" href="#deal/detail?id=80000000001">A&amp;B</a>') === 0, "案件の立ち位置の案件名がリンクでない: " + b);
  ctx.__CD = { meta: { found: true, houjin: "H1" }, customer: null, deals: [{ deal_id: "80000000003", name: "系列の契約",
    stage: "定期1", site: "S", kind: "サブスク", start: "2026-01-01", expiration: "2026-06-30", renewal_no: 0, amount: null,
    oubo: null, mensetu: null, syoudaku: null, is_active: true, right_censored: false }], mtgs: [] };
  const s = run('custBlocks(__CD, new Set(["deals"]))');
  ok(s.indexOf('href="#deal/detail?id=80000000003"') >= 0, "契約の系列の表の案件名がリンクでない");
  // リンクは折り返す（golink の nowrap を使うと長い案件名の列が伸びて右端の列が切れる）
  ok(/a\.deallink\{[^}]*overflow-wrap:anywhere/.test(html) && !/a\.deallink\{[^}]*nowrap/.test(html),
    "案件名のリンクが折り返さない");
});

check("案件の詳細: 付け直しの元は関係で言い分け、オプション契約はリンクにしない（押しても詳細が無い）", () => {
  const P = ddPayload();
  const mk = (rel, id, name) => ({ state: "moved_in", in_span: true, moved_to: null,
    moved_from: { deal_id: id, name: name, relation: rel } });
  P.events = [
    Object.assign({}, P.events[0], { call_id: "r1", date: "2026-06-04", attach: mk("later", "80000000002", "継続の契約") }),
    Object.assign({}, P.events[0], { call_id: "r2", date: "2026-06-03", attach: mk("earlier", "80000000000", "前の契約") }),
    Object.assign({}, P.events[0], { call_id: "r3", date: "2026-06-02", attach: mk("option", "80000000009", "求人追加の契約") }),
  ];
  ctx.__DD = P;
  const items = ddItems(run("detailAllCalls = true; detailOlder = true; try { renderDetail(__DD) } finally { detailAllCalls = false; detailOlder = false; }"));
  const by = (d) => items.find((x) => x.indexOf("<b>" + d + "</b>") >= 0) || "";
  ok(/付け直し: 継続の取引「<a class="deallink" href="#deal\/detail\?id=80000000002">/.test(by("2026-06-04")), "継続先の文かリンクが無い");
  ok(/付け直し: 前の契約の取引「<a class="deallink" href="#deal\/detail\?id=80000000000">/.test(by("2026-06-03")),
    "前の契約から来た行を「前の契約」と書いていない");
  const o = by("2026-06-02");
  ok(o.indexOf("付け直し: 同じ拠点のオプション契約「求人追加の契約」") >= 0, "オプション契約から来た行をそう書いていない");
  ok(o.indexOf("80000000009") < 0 && o.indexOf("継続の取引") < 0, "オプション契約をリンクにしている、または「継続の取引」と書いている");
});

check("案件の詳細: 接触に数えない行（60秒以下の電話・メール由来の MTG）には「数えています」と書かない", () => {
  const P = ddPayload();
  const out = { state: "moved_out", in_span: false, moved_from: null, moved_to: { deal_id: "80000000002", name: "継続の契約" } };
  P.events = [
    Object.assign({}, P.events[0], { call_id: "s1", date: "2026-08-03", duration_sec: 30, contact: false, attach: out }),
    Object.assign({}, P.events[0], { call_id: "s2", date: "2026-08-02", duration_sec: 90, contact: true, attach: out }),
    { kind: "mail_mtg", date: "2026-08-01", time: null, fact: false, source_label: "メール由来（推定）",
      certainty: "推定(±1日 83.3%)", same_day_recording: false, attach: out },
    Object.assign({}, P.events[0], { call_id: "s3", date: "2026-07-31", duration_sec: 20, contact: false,
      attach: { state: "ambiguous", in_span: false, moved_from: null, moved_to: null } }),
  ];
  ctx.__DD = P;
  const items = ddItems(run("detailAllCalls = true; detailOlder = true; try { renderDetail(__DD) } finally { detailAllCalls = false; detailOlder = false; }"));
  const by = (d) => items.find((x) => x.indexOf("<b>" + d + "</b>") >= 0) || "";
  ok(by("2026-08-03").indexOf("数えています") < 0 && by("2026-08-03").indexOf("60秒以下なので、接触には数えていません") >= 0,
    "60秒以下の電話に「数えています」と書いている");
  ok(by("2026-08-02").indexOf("の期間の記録として数えています") >= 0, "60秒超の電話の文が変わっている");
  ok(by("2026-08-01").indexOf("数えています") < 0 && by("2026-08-01").indexOf("メール由来の MTG なので") >= 0,
    "メール由来の MTG に「数えています」と書いている、または付け先の印が無い");
  ok(by("2026-07-31").indexOf("60秒以下なので") >= 0, "決められない 60秒以下の電話の理由が違う");
});

check("案件の詳細: 録画 MTG の取引への結び付けが確度「中」なら印と文を付け、「高」には付けない", () => {
  const P = ddPayload();
  const base = P.events.find((e) => e.kind === "mtg");
  P.events = [
    Object.assign({}, base, { date: "2026-03-12", link_certainty: "中", link_reason: "件名が近い" }),
    Object.assign({}, base, { date: "2026-03-11", link_certainty: "高", link_reason: "取引名と一致" }),
  ];
  ctx.__DD = P;
  const items = ddItems(run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }"));
  const by = (d) => items.find((x) => x.indexOf("<b>" + d + "</b>") >= 0) || "";
  ok(by("2026-03-12").indexOf("確度 中") >= 0 && by("2026-03-12").indexOf("結び付けは推定") >= 0 &&
    by("2026-03-12").indexOf("件名が近い") >= 0, "確度が中の録画 MTG に印か文が無い");
  ok(by("2026-03-11").indexOf("確度") < 0, "確度が高の録画 MTG に印が付いている");
});

check("案件の詳細: 話した人はそろえた表示名（handler_label）を出し、元の書き方は title に残す", () => {
  const P = ddPayload();
  P.events = [Object.assign({}, P.events[0], { handler: "リクロジ＿見張り 太郎", handler_label: "見張り太郎", attach: { state: "own" } })];
  ctx.__DD = P;
  const it = ddItems(run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }"))[0] || "";
  ok(it.indexOf("話した人 見張り太郎") >= 0, "そろえた表示名を出していない: " + it.slice(0, 200));
  ok(it.indexOf('title="Zoom の表示名: リクロジ＿見張り 太郎"') >= 0, "元の表示名を title に残していない");
});

check("案件の詳細: 電話の AI 要約には「誤りを含むことがあります」の断りを付ける（取り違えは機械の検証で防げない）", () => {
  ok(jsNoComment.includes("AI 要約（誤りを含むことがあります）"), "要約の見出しに AI 要約の断りが無い");
});

/* ================================================================ 段1（2026-09-28、08_UIUX改善案の S-3） */
check("S-3: 今日動く先の3表は8列（案件・名札・担当・満了まで・最後の接触・最後のMTG・定期NPS・金額）で、名札が2列目", () => {
  const cols = run("TODAY_COLS.map((c) => c.t)");
  ok(JSON.stringify(cols) === JSON.stringify(["案件", "名札", "担当", "満了まで", "最後の接触", "最後のMTG", "定期NPS", "金額"]),
    "今日動く先の列が違う: " + cols.join(" / "));
  // 案件そのものは 15 列のまま。名札（並びの根拠）だけ案件名の隣へ
  ok(run("BOARD_COLS[1].k") === "n_flags", "案件そのものの名札の列が2列目でない");
  ok(run("BOARD_COLS.length") === 15, "案件そのものの列数が 15 でない（列を落としていないか）");
  const row = { deal_id: "a", name: "案件A", consultant: "担当A", flags: ["x", "y"], n_flags: 2, amount: 100000, days_left: 12, nps: 3 };
  ctx.__TD3 = { rows: [row], expiring_this_week: [row], started_this_week: [row],
    meta: { n_hit: 1, n_shown: 1, filter_rule: "", order_rule: "", mtg_gap: {}, new_deal_rule: "" } };
  const h = run("renderToday(__TD3)");
  for (const id of ["today-tbl", "soon-tbl", "new-tbl"]) {
    const at = h.indexOf('<table id="' + id + '"');
    ok(at >= 0, id + " の表が無い");
    const t = h.slice(at, h.indexOf("</table>", at));
    const ths = (t.split("</thead>")[0].match(/<th[\s>]/g) || []).length;
    ok(ths === 8, id + " の列数が " + ths + "（8 でない）");
    // 名札は折り返す列。段2 A で wl（22em）から今日動く先だけの wf（25.5em・短い名札）に変えた。折り返す性質は同じ
    ok(/<th class="wf sortable"[^>]*><button[^>]*data-k="n_flags"/.test(t), id + " の名札の列が折り返す列（wf）でない");
    const tds = t.split("<tbody>")[1].split("</tr>")[0];
    ok(/<td class="wl"><a class="deallink"/.test(tds), id + " の案件名の列が折り返す列（wl）でない: " + tds.slice(0, 120));
  }
  // 今日動く先の枠だけ高さの制限を外す（24 行を一望）。今週満了・今週始まったは 320px の枠のまま
  const capOf = (id) => h.slice(h.lastIndexOf('<div class="scroll-cap">', h.indexOf('id="' + id + '"')), h.indexOf('id="' + id + '"'));
  ok(capOf("today-tbl").includes('<div class="scroll" style="max-height:none">'), "今日動く先の枠に高さの制限が残っている");
  ok(!capOf("today-tbl").includes("縦・横にスクロール"), "高さを制限していない枠に「縦にスクロール」と書いている");
  ok(capOf("soon-tbl").includes('style="max-height:320px"') && capOf("soon-tbl").includes("縦・横にスクロール"),
    "今週満了の枠が 320px の枠でない");
  const b = run('boardTable([__TD3.rows[0]], { key: "n_flags", asc: false }, "board-tbl")');
  const bh = b.split("</thead>")[0];
  ok((bh.match(/<th[\s>]/g) || []).length === 15, "案件そのものの表が 15 列でない");
  ok(bh.indexOf('data-k="n_flags"') < bh.indexOf('data-k="consultant"'), "案件そのものの表で名札が担当より後ろにある");
  // 名札だけは折り返す列（wl）。2026-09-28 検証（fixture 1440px）: 2 列目に移した名札を折り返さないままにしたら列の幅が
  // 799px になり、枠（1,149px）に最初から見える列が 8 列 → 3 列に減った。案件名ほかの 14 列は折り返さない（横スクロールのまま）
  ok(/<th class="wl sortable"[^>]*><button[^>]*data-k="n_flags"/.test(bh), "案件そのものの名札の列が折り返す列（wl）でない（名札が 799px の 1 行になる）");
  ok((bh.match(/class="wl sortable"/g) || []).length === 1, "案件そのものの表で名札以外の列まで折り返す列にしている");
  ok(!/<td class="wl"><a class="deallink"/.test(b), "案件そのものの案件名の列まで折り返す列にしている（15 列は枠の横スクロールのまま）");
});

/* ================================================================ 段1 S-1 / S-2 / D-1a（2026-09-28） */
// fixture（2026-09-18）の形を小さくしたもの。候補（名札2本以上）は 4 件・担当 3 名で、上位 2 件だけを rows に（サーバの keep）
const TD_ROW = (o) => Object.assign({ deal_id: "d", name: "案件", consultant: "担当A", flags: ["x", "y"], n_flags: 2,
  amount: 100000, days_left: 30, mtg_band: "recent" }, o);
function todayFixture() {
  const cand = [
    TD_ROW({ deal_id: "c1", name: "候補1", consultant: "担当A", flags: ["x", "y", "z"], n_flags: 3, amount: 300000, mtg_band: "critical" }),
    TD_ROW({ deal_id: "c2", name: "候補2", consultant: "担当B", amount: 200000 }),
    TD_ROW({ deal_id: "c3", name: "候補3", consultant: "担当C", amount: 150000, mtg_band: "critical" }),
    TD_ROW({ deal_id: "c4", name: "候補4", consultant: "担当C", amount: 100000 }),
  ];
  return {
    rows: cand.slice(0, 2), candidates: cand,
    expiring_this_week: [TD_ROW({ deal_id: "e1", name: "満了1", consultant: "担当C", days_left: 3 })],
    started_this_week: [TD_ROW({ deal_id: "s1", name: "新規1", consultant: "担当A", start: "2026-09-15" })],
    meta: { n_hit: 4, n_shown: 2, keep: 2, n_active: 604, n_not_started: 5, filter_rule: "絞った条件の文", order_rule: "並びの文",
      new_deal_rule: "新規の文", not_counted: "※ 予測ではありません", today: "2026-09-18",
      mtg_gap: { rule: "", no_record_note: "", source_note: "", coverage: {}, forced_by_expiry: 1, n_judged: 0,
        bands: [{ band: "critical", label: "MTGが90日以上途絶", n: 9, alert: true },
                { band: "recent", label: "直近30日にMTGあり", n: 40, alert: false },
                { band: "onboarding", label: "立ち上がり期", n: 7, alert: false }] } },
  };
}

// 2026-09-29 組み替え 段A（handover 09 の 3章 1）: 図 2 つ（名札の内訳・MTG 途絶の帯）を外し、数え方は畳みに、その下に自分の接触
check("S-1: 今日は 問い → 担当の欄 → KPI → 表 今日動く先 → 表 今週満了 → 今週始まった（畳み） → MTG 途絶の数え方（畳み） → 自分の接触 → 読むときの注意 の順", () => {
  run("todayConsultant = '';");   // todayStartedOpen は触らない（既定で閉じていることを見る）
  ctx.__TD5 = todayFixture();
  const h = run("renderToday(__TD5)");
  const at = (s) => { const i = h.indexOf(s); ok(i >= 0, "「" + s + "」が無い"); return i; };
  const order = ["今日・今週、どこに連絡するか", 'id="td-consultant"', '<div class="kpis">', 'id="td-today-h"', 'id="today-tbl"',
    'id="td-soon-h"', 'id="soon-tbl"', '<details class="fold" id="td-started"', 'id="new-tbl"', 'id="td-mtg-note"',
    "担当を選ぶと、その人の持ち案件1件あたりの接触", "読むときの注意"];
  ok(!h.includes("何で上がってきたか") && !h.includes("MTG が途絶えている先"), "外した図が残っている");
  const pos = order.map(at);
  for (let i = 1; i < pos.length; i++) ok(pos[i] > pos[i - 1], "順が違う: 「" + order[i] + "」が「" + order[i - 1] + "」より前にある");
  // 決まりごとの箱は表の見出しの直下の畳みの中（表より前に開いた箱で出さない）。母数は畳みの 1 行目（summary）に残す
  const rule = h.indexOf('<details class="fold" id="td-rule">');
  ok(rule > at('id="td-today-h"') && rule < at('id="today-tbl"'), "決まりごとの畳みが表の見出しの直下に無い");
  const ruleBox = h.slice(rule, h.indexOf("</details>", rule));
  ok(ruleBox.includes("絞った条件の文") && ruleBox.includes("並びの文"), "絞った条件・並びの決まりが畳みの中に無い");
  // 母数（4 件から 2 件）は畳まず表の上に出す。言うのは枠の案内（scroll-cap の「2 行を出しています（全 4 件のうち）」）の 1 回だけで、
  // 畳みの summary は同じ 1 行（caprow）に並べて繰り返さない（2026-09-28 検証: summary と案内が同じ「243 件から 24 件」を
  // 40px の間に 2 回言い、その分 表の見出し行が下がって 1 画面目に完全な行が 0 行だった）
  const caprow = h.slice(h.indexOf('<div class="caprow">'), at('id="today-tbl"'));
  ok(caprow.startsWith('<div class="caprow"><details class="fold" id="td-rule"><summary>'), "決まりごとの畳みが枠の案内と同じ行（caprow）の先頭に無い");
  ok(/<\/details><div class="scroll-cap"><b>2<\/b> 行を出しています（全 4 件のうち）/.test(caprow), "枠の案内（母数）が畳みの直後・同じ行に無い");
  ok((caprow.match(/全 4 件のうち|4 件から 2 件/g) || []).length === 1, "母数（4 件 → 2 件）が表の上に 2 回出ている");
  ok(/<summary>名札の本数順（同じ本数なら金額順）　<span class="when-closed">決まりごとを開く<\/span>/.test(ruleBox), "畳みの 1 行目（summary）が並びの決まりでない: " + ruleBox.slice(0, 160));
  ok(h.indexOf('<div class="note def"><span class="hd">絞った条件') > rule, "絞った条件の箱が畳みの外（表より前）に出ている");
  // 今週始まった契約は畳み（summary に件数）。既定は閉じている
  ok(!/<details class="fold" id="td-started" open/.test(h), "今週始まった契約の畳みが既定で開いている");
  const st = h.slice(h.indexOf('id="td-started"'), h.indexOf("</details>", h.indexOf('id="td-started"')));
  ok(st.includes("<summary>今週始まった契約（1 件）"), "summary に件数が無い");
  ok(st.includes('id="new-tbl"'), "今週始まった契約の表が畳みの中に無い");
  // 赤は「増えるとまずい件数」の MTG 途絶だけ。今日出す先は赤にしない
  const kp = h.slice(h.indexOf('<div class="kpis">'), h.indexOf('id="td-today-h"'));
  const cards = kp.split(/<(?:button|div)[^>]*class="kpi[" ]/).slice(1);
  ok(cards.length === 5, "KPI が 5 枚でない: " + cards.length);
  ok(!/^[^>]*is-bad/.test(cards[0]) && cards[0].includes("今日出す先"), "今日出す先が赤（is-bad）");
  ok(/^[^>]*is-bad/.test(cards[1]) && cards[1].includes("MTGが90日以上途絶"), "MTG 途絶が赤（is-bad）でない");
  // 帯を付けていない帯の件数も、図を外した後の畳みに残す（黙って落とさない）。手を打つ帯と区別して書く
  const g = h.slice(h.indexOf('id="td-mtg-note"'), h.indexOf("</details>", h.indexOf('id="td-mtg-note"')));
  ok(g.includes("MTGが90日以上途絶") && !/MTGが90日以上途絶 \d+件（帯を付けていない）/.test(g), "手を打つ帯が数え方の畳みに無い（または帯を付けていない扱い）");
  ok(g.includes("直近30日にMTGあり 40件（帯を付けていない）") && g.includes("立ち上がり期 7件（帯を付けていない）"), "帯を付けていない帯の件数が畳みに無い: " + g);
});

check("S-2: 数字の札は押せる（button.kpi）。今日出す先・今週満了・今週始まったは同じ画面の表へ、MTG 途絶は案件そのものを帯で絞って開く", () => {
  run("todayConsultant = '';");
  ctx.__TD5 = todayFixture();
  const h = run("renderToday(__TD5)");
  ok(/<button type="button" class="kpi" data-jump="td-today-h"><span class="lbl">今日出す先/.test(h), "今日出す先の札が表へ飛ぶ button でない");
  ok(/<button type="button" class="kpi is-bad" data-band="critical"><span class="lbl">MTGが90日以上途絶/.test(h), "MTG 途絶の札が帯で絞る button でない");
  ok(/<button type="button" class="kpi is-warn" data-jump="td-soon-h"><span class="lbl">今週満了する/.test(h), "今週満了の札が表へ飛ぶ button でない");
  ok(/<button type="button" class="kpi" data-jump="td-started"><span class="lbl">今週始まった契約/.test(h), "今週始まった契約の札が畳みを開く button でない");
  ok(/<div class="kpi"><span class="lbl">稼働中の全件[\s\S]*?<a class="golink"/.test(h), "稼働中の全件はリンクを添えた div のまま（button の中に a を入れない）");
  ok(!/<button[^>]*class="kpi[^>]*>(?:(?!<\/button>)[\s\S])*<a /.test(h), "button の中に a がある（押せるものの入れ子）");
  ok((h.match(/<span class="act">/g) || []).length === 4, "行き先の小さな文（.act）が 4 枚に付いていない");
  // 表の見出しは部品の題（class part。M-5 で問い＝mincho と分けた）。id と tabindex=-1 が付いていることが要点
  ok(h.includes('<h2 class="sec part" id="td-today-h" tabindex="-1">') && h.includes('<h2 class="sec part" id="td-soon-h" tabindex="-1">'),
    "飛ぶ先の見出しに id / tabindex が無い");
  // 行き先の無い札（act 無し）は div のまま。見出しに id を渡さないときは前と同じ形
  ok(run('kpi("LTV 中央値", "1", "", "")') === '<div class="kpi"><span class="lbl">LTV 中央値</span><span class="big">1</span></div>', "act 無しの kpi が div でない");
  ok(run('sec("問い", "x")') === '<h2 class="sec mincho"><span class="no">問い</span>x</h2>', "id 無しの sec の形が変わった");
  // 案件そのものの絞り込みに MTG 途絶の帯が加わり、行の mtg_band で落とす。内部の鍵は画面の言葉に出さない
  run('boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "critical" };');
  const kept = run("boardApply(__TD5.candidates).map((r) => r.deal_id).join(',')");
  ok(kept === "c1,c3", "帯で絞れていない: " + kept);
  ok(run("boardFilterOn()") === true, "帯だけの絞り込みが「絞り込みなし」になる");
  const words = run("boardFilterWords(__TD5)");
  ok(words === "MTG途絶 MTGが90日以上途絶", "絞り込みの言葉に帯の名前が無い: " + words);
  ok(run("boardFilterWords({})") === "MTG途絶 の帯", "帯の名前が引けないときに内部の鍵（critical）を出している: " + run("boardFilterWords({})"));
  const bar = run("boardFilterBar(__TD5)");
  ok(/<select id="bf-band"><option value="">すべて<\/option><option value="critical" selected>MTGが90日以上途絶（9）<\/option>/.test(bar),
    "MTG 途絶の選択欄が無い、または帯の名前と件数でない");
  run('boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "" };');
});

check("D-1a: 担当を選ぶと、その人の候補（名札2本以上）を全件出し、今週満了・今週始まった・KPI も追従する。未選択は今までどおり上位 keep 件", () => {
  ctx.__TD5 = todayFixture();
  run("todayConsultant = '';");
  const all = run("renderToday(__TD5)");
  ok(/<select id="td-consultant"><option value="" selected>全員<\/option><option value="担当A">担当A<\/option><option value="担当B">担当B<\/option><option value="担当C">担当C<\/option><\/select>/.test(all),
    "担当の選択欄が候補の全員（rows の 2 名ではなく、候補・満了・新規の 3 名）でない");
  ok(all.includes("今日動く先（2 件）") && all.includes("全 4 件のうち"), "未選択のときにサーバの上位 keep 件でない");
  ok(!all.includes("候補3"), "未選択のときに候補の全件を出している");
  run("todayConsultant = '担当C';");
  const mine = run("renderToday(__TD5)");
  ok(mine.includes("今日動く先（2 件）") && mine.includes("候補3") && mine.includes("候補4") && !mine.includes("候補1"),
    "担当で絞ると、その人の候補を全件（keep 件に切る前の candidates から）出していない");
  ok(!mine.includes("件のうち"), "担当で絞った表に「N 件のうち」（切っている顔）が残っている");
  ok(mine.includes('<option value="担当C" selected>'), "選んだ担当が欄で選ばれていない");
  const kp = mine.slice(mine.indexOf('<div class="kpis">'), mine.indexOf('id="td-today-h"'));
  const big = [...kp.matchAll(/<span class="big">(\d+)<span class="u">件<\/span><\/span>/g)].map((m) => m[1]);
  ok(big.join(",") === "2,1,1,0,604", "担当で絞ったときの KPI の数字（今日出す先・MTG途絶・今週満了・今週始まった・全件）が違う: " + big.join(","));
  ok(textOf(kp).includes("全社では 9 件") && textOf(kp).includes("全社は 4 件から 2 件"), "全社の数を添えていない");
  ok(mine.includes("満了1") && !mine.includes("新規1"), "今週満了・今週始まった契約が担当で絞られていない");
  // 覚えている担当が今日の候補に無い: 欄に残し、0 件の理由を書く
  run("todayConsultant = '担当Z';");
  const none = run("renderToday(__TD5)");
  ok(none.includes('<option value="担当Z" selected>'), "覚えている担当が候補に無いときに欄から消えている");
  ok(none.includes("この担当には名札が2本以上ついた案件がありません") && none.includes("今日動く先（0 件）"), "0 件の理由が無い");
  // 古い応答（candidates が無い）でも rows から絞れる
  const old = todayFixture(); delete old.candidates; ctx.__TD6 = old;
  run("todayConsultant = '担当A';");
  ok(run("renderToday(__TD6)").includes("候補1"), "candidates の無い応答で rows から絞れていない");
  run("todayConsultant = '';");
});

check("S-8: 表が主役の画面（案件そのもの・担当者ごとの案件・担当者の一覧・担当の交代）では表を図より先に出す", () => {
  const before = (h, a, b, msg) => {
    const i = h.indexOf(a), j = h.indexOf(b);
    ok(i >= 0 && j >= 0, msg + "（" + (i < 0 ? a : b) + " が無い）");
    ok(i < j, msg);
  };
  run('cur = { menu: "deal", view: "board" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "" };');
  const b = run("renderBoard(__BD)");
  before(b, 'id="board-filter"', 'id="board-count"', "案件そのもの: 絞り込み → 件数行 の順でない");
  before(b, 'id="board-count"', 'id="board-tbl"', "案件そのもの: 件数行 → 表 の順でない");
  before(b, 'id="board-tbl"', "<figure", "案件そのもの: 表が図の下");
  before(b, 'id="board-tbl"', "並びのきまりXYZ", "案件そのもの: 並びの決まりが表の前にある");
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "田中", flag: "", expiry: "", q: "", band: "" };');
  const o = run("renderBoard(__BD)");
  before(o, 'id="board-count"', 'id="board-tbl"', "担当者ごとの案件: 件数行 → 表 の順でない");
  before(o, 'id="board-tbl"', "<figure", "担当者ごとの案件: 表が図の下");
  run('boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "" }; cur = { menu: "deal", view: "today" };');
  ctx.__TM = { rows: TEAM_ROWS, meta: { n_consultant: 8, n_active: 604, unknown_owner: 0, retired_deals: 0, retired_people: 0,
    owner_ties: 0, not_counted: "※ 担当者の評価ではありません" }, contact_rule: "", small_n_rule: "", focus_rule: "", owner_rule: "" };
  /* 2026-09-29 組み替え: チームと担当では 札 → 担当者 × 状態の表 → 接触の推移の図 → 担当の交代 */
  const t = run('contactUnit = "month"; teamPick = ""; renderTeam(teamOf(__TM, { contact: __CT, handover: __HOC }))');
  before(t, '<div class="kpis">', 'id="team-tbl"', "チームと担当: KPI → 表 の順でない");
  before(t, 'id="team-tbl"', "<figure", "チームと担当: 表が図の下");
  before(t, "<figure", 'id="tm-ho-h"', "チームと担当: 接触の推移が担当の交代の下");
  const hv = run("renderHandover(__HOC)");
  before(hv, '<div class="kpis">', "交代の記録（", "担当の交代: KPI → 表 の順でない");
  before(hv, "交代の記録（", "<figure", "担当の交代: 表が図の下");
  // 決まりごとの箱は表の直下（2026-09-28 検証: 問いと KPI の間に 142px の箱があり、表の見出し行が y=813 だった）
  before(hv, 'id="ho-tbl"', "この一覧の決まりごと", "担当の交代: 決まりごとの箱が表より前にある（問い1 → KPI → 表 の間に入る）");
  before(hv, "この一覧の決まりごと", "<figure", "担当の交代: 決まりごとの箱が図より後ろ");
  before(hv, "HubSpot の担当者欄に反映されたか", "交代の前後で、接触は増えたか減ったか", "担当の交代: 2 つ目の問い（前後比較）が末尾でない");
  ok(hv.lastIndexOf('<div class="note def">') > hv.indexOf("交代の前後で、接触は増えたか減ったか"), "担当の交代: 末尾の枠（基準日と件数）が消えている");
});

/* ================================================================ 段1 D-1a の検証（2026-09-28）で見つかった分 */
check("D-1a の検証: 担当の選択欄の顔ぶれは選んだ担当で変わらない（絞る前の応答から集める）。サーバの consultants があれば候補 0 件の担当も選べる", () => {
  const F = todayFixture();
  // 担当D は候補（candidates）に無く、今週始まった契約にだけ居る（fixture の habc7b05f19ea と同じ形。稼働 1 件・名札 1 本）
  F.started_this_week.push(TD_ROW({ deal_id: "s2", name: "新規2", consultant: "担当D", start: "2026-09-16", flags: ["x"], n_flags: 1 }));
  ctx.__TD7 = F;
  const opts = (h) => [...h.slice(h.indexOf('<select id="td-consultant">'), h.indexOf("</select>")).matchAll(/<option value="([^"]*)"/g)].map((m) => m[1]).join(",");
  run("todayConsultant = '';");
  ok(opts(run("renderToday(__TD7)")) === ",担当A,担当B,担当C,担当D", "全員のときの顔ぶれが違う: " + opts(run("renderToday(__TD7)")));
  run("todayConsultant = '担当A';");
  const a = run("renderToday(__TD7)");
  // 2026-09-28 検証（fixture）: 絞った後の応答を渡していて、担当A を選んだ瞬間に担当D が欄から消えた（25 択 → 24 択）
  ok(opts(a) === ",担当A,担当B,担当C,担当D", "担当A を選ぶと欄から他の担当が消える（絞った後の応答から集めている）: " + opts(a));
  ok(!a.includes("新規2") && a.includes("新規1"), "担当A を選んでいるのに今週始まった契約が担当で絞られていない");
  // サーバの consultants（稼働中の全件の担当）があればそれが顔ぶれ。候補 0 件の担当E も選べ、選ぶと 0 件の理由が出る
  F.consultants = ["担当A", "担当B", "担当C", "担当D", "担当E"];
  run("todayConsultant = '担当B';");
  const b = run("renderToday(__TD7)");
  ok(opts(b) === ",担当A,担当B,担当C,担当D,担当E", "consultants の顔ぶれになっていない: " + opts(b));
  run("todayConsultant = '担当E';");
  const e = run("renderToday(__TD7)");
  ok(e.includes('<option value="担当E" selected>') && e.includes("この担当には名札が2本以上ついた案件がありません") && e.includes("今日動く先（0 件）"),
    "候補 0 件の担当を選んだときに、選ばれた状態と 0 件の理由が無い");
  run("todayConsultant = '';");
});

check("D-1a の検証: 担当を選んだときの KPI「MTGが90日以上途絶」はサーバの担当ごとの実数（critical_by_consultant）で、押すとその担当のその帯へ。絞った条件にはこの表が何かを 1 文", () => {
  const F = todayFixture();
  // 担当C の候補の中の critical は 1 件（候補3）だが、帯の実数は 3 件（名札が MTG途絶の 1 本だけの行は候補に入らない）
  F.meta.mtg_gap.critical_by_consultant = { "担当A": 1, "担当C": 3 };
  ctx.__TD8 = F;
  run("todayConsultant = '担当C';");
  const h = run("renderToday(__TD8)");
  const kp = h.slice(h.indexOf('<div class="kpis">'), h.indexOf('id="td-today-h"'));
  ok(/<button type="button" class="kpi is-bad" data-band="critical" data-consultant="担当C"><span class="lbl">MTGが90日以上途絶<\/span><span class="big">3<span class="u">件/.test(kp),
    "担当を選んだときの MTG 途絶が担当ごとの実数（3）で、担当を添えた button になっていない: " + kp.slice(kp.indexOf("MTGが90日以上途絶") - 110, kp.indexOf("MTGが90日以上途絶") + 120));
  ok(textOf(kp).includes("この担当の稼働中の案件で。全社では 9 件") && textOf(kp).includes("この担当の一覧を見る"), "母集団（稼働中の案件）と行き先（この担当の一覧）が書いていない");
  // 担当を選んでいるときの「絞った条件」: サーバの文（全社の説明）の前に、この表が全件・切っていないことを 1 文
  const rb = h.slice(h.indexOf('id="td-rule"'), h.indexOf("</details>", h.indexOf('id="td-rule"')));
  ok(rb.includes("<b>担当C を選んでいるので、この担当の名札2本以上の候補を全件（2 件）出しています。2 件には切っていません。</b>全社の決まり: 絞った条件の文"),
    "担当を選んだときの絞った条件が全社の説明のまま: " + rb.slice(rb.indexOf("絞った条件"), rb.indexOf("絞った条件") + 160));
  ok(/<summary>担当C の候補（名札2本以上、2 件に切らず全件）。名札の本数順　<span class="when-closed">/.test(rb), "担当を選んだときの summary が違う: " + rb.slice(0, 160));
  // 全員のときは今までどおり帯の件数（9）・全社の一覧（担当を添えない）
  run("todayConsultant = '';");
  const all = run("renderToday(__TD8)");
  ok(/<button type="button" class="kpi is-bad" data-band="critical"><span class="lbl">MTGが90日以上途絶<\/span><span class="big">9<span class="u">件/.test(all) && textOf(all).includes("全社の一覧を見る"),
    "全員のときの MTG 途絶が帯の件数（9）でない、または担当を添えている");
  // 古い応答（critical_by_consultant が無い）は候補の中で数え、そう断る。行き先は全社（担当を添えない）
  ctx.__TD9 = todayFixture();
  run("todayConsultant = '担当C';");
  const old = run("renderToday(__TD9)");
  ok(/data-band="critical"><span class="lbl">MTGが90日以上途絶<\/span><span class="big">1<span class="u">件/.test(old) && textOf(old).includes("この担当の候補（名札2本以上）の中で。全社では 9 件"),
    "古い応答で候補の中の件数（1）と、その断りが無い");
  run("todayConsultant = '';");
});

check("畳みの summary の文は開閉で替わる（表を開く ↔ 表を閉じる、決まりごとを開く ↔ 閉じる）。JS でなく CSS の [open] で見せる方を替える", () => {
  run("todayConsultant = ''; todayStartedOpen = false;");
  ctx.__TDs = todayFixture();
  const h = run("renderToday(__TDs)");
  ok(/<details class="fold" id="td-started"><summary>今週始まった契約（1 件）　<span class="when-closed">表を開く<\/span><span class="when-open">表を閉じる<\/span><\/summary>/.test(h),
    "今週始まった契約の summary に開閉の 2 つの文が無い");
  ok(/<details class="fold" id="td-rule"><summary>[^<]*<span class="when-closed">決まりごとを開く<\/span><span class="when-open">決まりごとを閉じる<\/span><\/summary>/.test(h),
    "決まりごとの summary に開閉の 2 つの文が無い");
  ok(/summary \.when-open\{\s*display:none/.test(html) && /details\[open\] > summary \.when-open\{\s*display:inline/.test(html) &&
     /details\[open\] > summary \.when-closed\{\s*display:none/.test(html), "開閉で見せる方を替える CSS（when-open / when-closed）が無い");
  // 0 件のときは「表を開く」を出さない（開いても表が無い）
  const F = todayFixture(); F.started_this_week = []; ctx.__TDz = F;
  ok(/<details class="fold" id="td-started"><summary>今週始まった契約（0 件）<\/summary>/.test(run("renderToday(__TDz)")), "0 件でも「表を開く」が出ている");
});

check("KPI から Enter で飛んだ先の見出し（tabindex=-1 の h2.sec）にも藍の focus-visible を定義する（UA 既定の黒い太枠を出さない）", () => {
  ok(/h2\.sec:focus-visible\{\s*outline:2px solid var\(--ai\)/.test(html), "h2.sec:focus-visible の定義が無い（2026-09-28 検証: この画面で唯一の黒枠だった）");
});

/* ================================================================ UI/UX 改善 段1（2026-09-28、handover 08 の S-4〜S-13） */
check("S-4: 集計の4表（NPS低・沈黙・MTG未実施・最優先）の案件名が案件の詳細へのリンク", () => {
  /* 直す前は 4 か所とも esc(r.name || r.deal_id) の文字だけで行き止まりだった（08 の S-4）。
     入力は上の __FO / __PH / __RU / __OUT を複製し、行だけ差し替える（形は routes.rs の build_* のまま） */
  const link = (id, name) => '<a class="deallink" href="#deal/detail?id=' + id + '">' + name + "</a>";
  const fo = JSON.parse(JSON.stringify(ctx.__FO));
  fo.nps_low.rows = [{ deal_id: "80000000011", name: "NPS低の案件", stage: "定期1", nps: 0, nps_month: "2026-09",
    amount: 100000, days_to_expiry: 10, n_contact: 1 }];
  fo.nps_low.n = 1;
  ctx.__S4FO = fo;
  const f = run("renderFocus(__S4FO)");
  ok(f.includes(link("80000000011", "NPS低の案件")), "いま見るべき顧客（NPS低）の案件名がリンクでない");
  const ph = JSON.parse(JSON.stringify(ctx.__PH));
  ph.silent = { n: 1, rule: "", rows: [{ deal_id: "80000000012", name: "沈黙の案件", stage: "定期1", amount: 100000,
    n_calls: 0, n_contact: 0, last_contact: null, days_since: null }] };
  ctx.__S4PH = ph;
  ok(run("renderPhone(__S4PH)").includes(link("80000000012", "沈黙の案件")), "電話（沈黙している取引）の案件名がリンクでない");
  const ru = JSON.parse(JSON.stringify(ctx.__RU));
  ru.no_mtg = { n: 1, first_active: 10, rate: 10, note: "", rows: [{ deal_id: "80000000013", name: "MTG未実施の案件",
    stage: "定期1", amount: 100000, days_since_start: 40 }] };
  ctx.__S4RU = ru;
  /* 立ち上がり（MTG未実施）と成果とリスク（最優先）の表は段B で案件一覧の見方に移した。案件名は見方の「外れた案件」の表で
     案件の詳細へのリンク（段B 案件一覧の見張り）。ここでは節が見方へ案内していることを見る */
  const hr = run("renderRampup(__S4RU)");
  ok(!hr.includes("MTG未実施の案件") && hr.includes('href="' + run('esc(hashFor("board", { view: "no_mtg" }))') + '"'),
    "立ち上がり（MTG未実施）の表が残っているか、見方への行き先が無い");
  const out = JSON.parse(JSON.stringify(ctx.__OUT));
  out.risk.top = [{ deal_id: "80000000014", name: "最優先の案件", stage: "定期1", amount: 100000, days_to_expiry: 10,
    ax3w: "放置", never_after_start: false, n_contact: 0 }];
  ctx.__S4OUT = out;
  const ho = run("renderOutcome(__S4OUT)");
  // 散布図の点の title には案件名が出る（図の側）。表の行（案件の詳細へのリンク）が無いことを見る
  ok(!ho.includes(link("80000000014", "最優先の案件")) && !ho.includes("<tbody>") && ho.includes('href="' + run('esc(hashFor("board", { view: "top" }))') + '"'),
    "成果とリスク（最優先）の表が残っているか、見方への行き先が無い");
  /* 取引名が空の行は、前は取引ID（内部ID）をそのまま画面に出していた。dealLink は「取引名なし」と書く */
  fo.nps_low.rows[0].name = null;
  ctx.__S4FO2 = fo;
  const f2 = run("renderFocus(__S4FO2)");
  ok(!/>[^<]*80000000011[^<]*</.test(f2) && f2.includes("取引名なし"), "取引名が空の行で取引IDが画面の文字に出ている");
  /* 契約の系列（継続を追いかける／法人番号で見る）と案件そのもの（BOARD_COLS）も同じ。前は呼び出し側が
     `d.name || d.deal_id` で取引IDを名前に流し込んでいて、dealLink の「取引名なし」に届かなかった（2026-09-28 検証の指摘） */
  const ser = JSON.parse(JSON.stringify(ctx.__SER));
  ser.deals[0].deal_id = "80000000015"; ser.deals[0].name = "";
  ctx.__S4SER = ser;
  const s = run('custBlocks(__S4SER, new Set(["deals"]))');
  ok(s.includes(link("80000000015", "取引名なし")) && !/>[^<]*80000000015[^<]*</.test(s),
    "契約の系列で取引名が空の行に取引IDが画面の文字に出ている");
  const bc = run('BOARD_COLS.find((c) => c.k === "name").fmt({ deal_id: "80000000016", name: null, focus: false })');
  ok(bc.includes(link("80000000016", "取引名なし")) && !/>[^<]*80000000016[^<]*</.test(bc),
    "案件そのものの表で取引名が空の行に取引IDが画面の文字に出ている: " + bc);
});

check("S-5: 表の案件名の横に「HS」、案件の詳細に「HubSpot で開く」。取引IDは href の中だけ・新しいタブ・rel=noopener", () => {
  /* portal_id は API の meta.hubspot_portal_id（routes.rs freshen、既定 23708633）から load() が覚える。
     ここでは変数に直接入れる。URL の形は取引ページ https://app.hubspot.com/contacts/<portal>/record/0-3/<deal_id>/ */
  const HS = 'href="https://app.hubspot.com/contacts/23708633/record/0-3/80000000001/"';
  run('hsPortal = "23708633"');
  try {
    const b = run('dealLink("80000000001", "A&B")');
    ok(b.indexOf('<a class="deallink" href="#deal/detail?id=80000000001">A&amp;B</a>') === 0, "案件名のリンクが変わった: " + b);
    const hs = b.slice(b.indexOf("<a class=\"hslink"));
    ok(hs.includes(HS), "HS の href が取引ページの形でない: " + hs);
    ok(/target="_blank"/.test(hs) && /rel="noopener"/.test(hs), "HS が新しいタブ＋rel=noopener でない: " + hs);
    /* 読み上げ名は見た目の文字「HS」で始める（WCAG 2.5.3 Label in Name。音声操作で「HS」と言って一致する）。
       前は「HubSpot でこの取引を開く（新しいタブ）」で見た目の語を含まなかった（2026-09-28 検証の指摘） */
    ok(/>HS<\/a>/.test(hs) && /aria-label="HS: HubSpot でこの取引を開く（新しいタブ）"/.test(hs), "表の横の印が小さな「HS」（読み上げは「HS: …」で始まる aria-label）でない: " + hs);
    /* 「HS」の意味は title だけでなく、他の印と同じく「色と印の意味」に載せる（タッチでは title が出ない）。
       M-1 の (3)（2026-09-29）: ヘッダの畳みは定義と検証へのリンクになったので、どの画面からも 1 押しでその表へ行けることを見る */
    ok(html.includes('<a class="golink" id="cs-legend" href="#monthly/trust?at=trust-defs" title="記録と数字の信頼度の「色と印の意味」の表へ">色と印の意味 →</a>'), "ヘッダから「色と印の意味」（定義と検証）へのリンクが無い");
    const defs = run("renderDefs()");
    const dtab = defs.slice(defs.indexOf("色と印の意味"), defs.indexOf("この画面が守っていること"));
    ok(/<td[^>]*><span class="hslink">HS<\/span><\/td><td[^>]*>案件名の横。HubSpot で/.test(dtab), "定義と検証の「色と印の意味」に HS の行が無い");
    ok(/a\.hslink, span\.hslink\{/.test(html), "凡例の見本（span.hslink）に HS と同じ見た目が付いていない");
    /* 🔴 .sr（position:absolute）を表の行に入れると、表の枠（.scroll）に切られずページの高さを伸ばす
       （2026-09-28 実測: 案件そのもの 604 行で全高 2,280px → 25,656px）。読み上げ用の文は属性で持つ */
    ok(!hs.includes('class="sr"'), "HS の中に絶対配置の読み上げ用 span がある（表の枠を突き抜けてページが伸びる）: " + hs);
    ok(!/>[^<]*80000000001[^<]*</.test(hs), "取引IDが画面の文字に出ている（href の中だけにする）: " + hs);
    /* 数字でない ID には HubSpot の URL を作らない（HubSpot のオブジェクトIDは数字） */
    ok(!run('dealLink("abc", "x")').includes("hslink"), "数字でない取引IDに HubSpot のリンクを作っている");
    /* 案件の詳細: 見出しの直下に「HubSpot で開く」 */
    ctx.__DD = ddPayload();
    const h = run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }");
    const open = h.slice(h.indexOf("<a class=\"hslink lg\""), h.indexOf("</a>", h.indexOf("<a class=\"hslink lg\"")) + 4);
    ok(open.includes(HS) && open.includes("HubSpot で開く"), "案件の詳細に「HubSpot で開く」が無い: " + open);
    ok(/aria-label="HubSpot で開く（新しいタブ）"/.test(open), "「HubSpot で開く」の読み上げ名が見た目の文字で始まっていない（矢印は含めない）: " + open);
    ok(open.indexOf("<a class") < h.indexOf('<div class="dd-kv">'), "「HubSpot で開く」が取引の基本より下にある");
    ok(!/>[^<]*80000000001[^<]*</.test(h), "案件の詳細で取引IDが画面の文字に出ている");
    /* 文の中の付け直しの注記（「継続の取引「…」に付いていた記録」）には HS を混ぜない */
    const items = ddItems(run("detailAllCalls = true; detailOlder = true; try { renderDetail(__DD) } finally { detailAllCalls = false; detailOlder = false; }"));
    const moved = items.find((x) => x.indexOf("<b>2026-06-01</b>") >= 0) || "";
    ok(moved.includes("継続の契約</a>」") && !moved.includes("hslink"), "付け直しの注記の文の中に HS が混ざっている");
  } finally { run('hsPortal = ""'); }
  /* portal_id を覚える前（API の応答がまだ無い）はリンクを出さない。推測で埋めない */
  ok(!run('dealLink("80000000001", "A")').includes("hslink"), "portal_id が無いのに HubSpot のリンクを出している");
  ok(!run("detailOlder = true; try { renderDetail(__DD) } finally { detailOlder = false; }").includes("HubSpot で開く"), "portal_id が無いのに「HubSpot で開く」を出している");
});

check("S-6: 画面名は1つ。表の見出しに「案件の立ち位置」を出さず、今日動く先の「絞った条件」は全件への行き先を名前で添える", () => {
  /* 直す前: 左「案件そのもの」／表の見出し「案件の立ち位置」／サーバの文「「案件」の中の「案件の立ち位置」」の3つ */
  run('cur = { menu: "deal", view: "board" }');
  const b = run("renderBoard(__BD)");
  ok(!textOf(b).includes("案件の立ち位置"), "案件そのものの表の見出しが「案件の立ち位置」のまま");
  ok(/<h2 class="sec part"><span class="no">表<\/span>稼働中の案件/.test(b), "表の見出しが無い（消しただけになっている）");
  /* 画面に出る文字列（コメントを除いた JS のリテラル）に、この名前を残さない */
  ok(!/["'][^"'\n]*案件の立ち位置/.test(jsNoComment), "JS の文字列（画面に出るもの）に「案件の立ち位置」が残っている");
  const td = run('renderToday({ rows: [], meta: { n_hit: 0, n_shown: 0, filter_rule: "名札が 2 本以上ついた 243 件から", order_rule: "", mtg_gap: {} } })');
  const box = td.slice(td.indexOf("絞った条件"), td.indexOf("</p>", td.indexOf("絞った条件")));
  ok(box.includes('<a class="golink" href="#deal/board">案件一覧</a>'), "絞った条件に全件への行き先（名前のリンク）が無い: " + box);
  ok(box.includes("名札が 2 本以上ついた 243 件から"), "サーバの文（filter_rule）を落としている");
  ok(box.includes("243 件から。全件は "), "サーバの文と行き先の間に句点が無い");
  /* サーバの文が空・無いとき、句点から始めない（前は「。全件は …」。2026-09-28 検証の指摘） */
  const t0 = run("todayFilterRule({ filter_rule: \"\" })"), t1 = run("todayFilterRule({})");
  ok(t0.startsWith("全件は ") && t1.startsWith("全件は "), "サーバの文が無いとき「。」から始まる: " + t0 + " / " + t1);
});

check("S-7: 左の項目名の後ろの番号と、区切りの丸数字を出さない（1 列・3 区切り・11 画面）", () => {
  // 2026-09-29 組み替え 段A: 上のメニューは無くし、左に 1 列で 3 区切り（見出し）と 11 画面を並べる（handover 09 の 3章・10章）
  ok(!html.includes('id="cs-menu"'), "上のメニュー（#cs-menu）が残っている");
  run('cur = { menu: "research", view: "phone" }; document.getElementById("cs-side").innerHTML = ""; drawSide();');
  const side = run('document.getElementById("cs-side").innerHTML');
  const heads = [...side.matchAll(/<h2 id="side-[a-z]+">([^<]+)<\/h2>/g)].map((m) => m[1]);
  ok(JSON.stringify(heads) === JSON.stringify(["毎日", "調べる", "月1・確かめる"]), "区切りの見出しが 毎日 / 調べる / 月1・確かめる でない: " + heads);
  ok(!side.includes('class="no"') && !/[①-⑩]/.test(side), "区切りに丸数字が残っている: " + side);
  const nBtn = (side.match(/<button /g) || []).length;
  ok(nBtn === 11, "左の画面が 11 でない: " + nBtn);
  ok(side.includes(">電話</button>") && side.includes(">記録と数字の信頼度</button>"), "左の項目名が出ていない: " + side);
  // 600px 以下は ul の箱を外して、区切りの見出しと項目を同じ流れに並べる（display:contents。見出しを別の行にすると 5 行 229px）。
  // 一覧であることは role="list" で読み上げに残す。
  // 🔴 区切りの箱は外さない（2026-09-29 検証 400px: 3 区切りを 1 つの流れにしていたので、境目が行の途中に来て
  //    「月1・確かめる」が「チームと担当 電話」と同じ行に付いた）。区切りごとに行を改め、区切りの間に線を引く
  ok((side.match(/<ul role="list" aria-labelledby="side-[a-z]+">/g) || []).length === 3, "区切りの ul に role=list と見出しとの結び付けが無い");
  const narrow = html.split("@media (max-width:600px){").slice(1).map((x) => x.split("\n}\n")[0]).join("\n");
  ok(/\.sidegrp ul\{ display:contents; \}/.test(narrow), "600px 以下で見出しと項目を同じ流れに並べる CSS が無い（400px でメニューが 5 行 229px になる）");
  // 🔴 2026-09-29 磨き込み: 区切りごとの行でも 4 行 170px で、今日の表の頭が 400px で y=1,228 だった。メニューは 1 行にして横に流す。
  //    区切りの箱は外さない（見出しと項目の組は区切りごとのまま）。区切りの境目は行の途中に来るので、縦の線で分ける
  ok(!/\.sidegrp\s*,[^{]*\{ display:contents/.test(narrow) && /\.sidegrp\{ display:flex; flex-wrap:nowrap; flex:none;/.test(narrow),
    "600px 以下で区切りの箱を外している（区切りの境目が分からない）か、区切りの中で折り返している（メニューが 2 行以上になる）");
  ok(/\.sidegrp \+ \.sidegrp\{ border-left:1px solid var\(--rule-strong\);/.test(narrow), "600px 以下で区切りの間に線が無い（区切りが字の大きさでしか分からない）");
  // 1 行で横に流す。続きがある側の端に影（図の枠と同じ local / scroll の重ね）。項目は消さない・畳まない
  const sideCss = (narrow.match(/\.side\{ display:flex;[^}]*\}/) || [""])[0];
  ok(/flex-wrap:nowrap/.test(sideCss) && /overflow-x:auto/.test(sideCss), "600px 以下でメニューを 1 行にして横に流していない: " + sideCss);
  ok(/no-repeat local/.test(sideCss) && /no-repeat scroll/.test(sideCss), "600px 以下のメニューに、続きがある側の影が無い（右の項目があることに気づけない）");
  ok(!/\.side[^{]*\{[^}]*display:none/.test(narrow) && !/\.sidegrp[^{]*\{[^}]*display:none/.test(narrow), "600px 以下でメニューの項目を隠している");
  ok(!side.includes('class="n"') && !/>\d+<\/span>/.test(side), "左の項目名の後ろに番号が残っている（件数に見える）: " + side);
  ok(/aria-current="page">電話</.test(side), "いま見ている項目の印（aria-current）が無い");
  run('cur = { menu: "deal", view: "today" }');
});

check("S-11: 「読み直す」は操作列の右端の文字リンクで、代償（20 秒・他の人も止まる）を書き、鮮度が緑でないときだけ目立つ", () => {
  const v = run('viewOf("deal", "today")');
  const fresh = run('ctlbar(viewOf("deal", "today"), { meta: { source_as_of: "2026-09-27 21:30", source_age_days: 1 } })');
  ok(v.path, "前提: 今日動く先は API を持つ");
  const sp = fresh.slice(fresh.indexOf('<span class="reload'), fresh.lastIndexOf("</span></span>") + 14);
  ok(sp.includes('id="cs-reload"') && sp.includes(">読み直す</button>"), "読み直すのボタンが無い: " + fresh);
  ok(/title="[^"]*20 秒[^"]*他の人の画面[^"]*"/.test(sp), "title に代償（20 秒・他の人の画面）が無い: " + sp);
  ok(/<span class="muted small">[^<]*20 秒[^<]*他の人の画面[^<]*<\/span>/.test(sp), "隣の小さな文に代償が無い: " + sp);
  ok(sp.startsWith('<span class="reload">'), "鮮度が緑（昨日）なのに目立たせている: " + sp.slice(0, 40));
  /* 右端: 読み直すの後ろには「キャッシュから表示」の注記以外を置かない */
  ok(/<\/span><\/span>(<span class="muted small">キャッシュから表示<\/span>)?<\/div>$/.test(fresh), "読み直すが操作列の右端でない: " + fresh.slice(-120));
  ok(/\.ctlbar \.reload\{[^}]*margin-left:auto/.test(html), "CSS で右端に寄せていない（margin-left:auto）");
  ok(/\.ctlbar \.reload button\.act\.link\{[^}]*text-decoration:underline/.test(html), "文字リンクの見た目でない");
  /* 4日前（bad）・何日前か分からない（warn）→ 目立たせる。応答がまだ無い（定義と検証を最初に開いた）→ 目立たせない */
  ok(run('ctlbar(viewOf("deal", "today"), { meta: { source_as_of: "2026-09-14 22:00", source_age_days: 4 } })').includes('<span class="reload urge">'),
    "4日前のデータなのに読み直すを目立たせていない");
  ok(run('ctlbar(viewOf("deal", "today"), { meta: { generated_at: "2026-09-14 22:00", source_age_days: null } })').includes('<span class="reload urge">'),
    "何日前か分からないのに読み直すを目立たせていない");
  ok(!run('ctlbar(viewOf("deal", "today"), {})').includes("urge"), "応答が無いのに目立たせている");
  /* 帯が赤（meta はあるが source_as_of も generated_at も無い＝CS_メタ が読めない）→ 目立たせる。
     前は「応答が無い」と一緒に false にしていて、赤い帯のときだけ目立たなかった（2026-09-28 検証の指摘） */
  ok(run('ctlbar(viewOf("deal", "today"), { meta: { today: "2026-09-28", n_active: 604 } })').includes('<span class="reload urge">'),
    "帯が赤（いつのものか分からない）なのに読み直すを目立たせていない");
  /* API を持たない「定義と検証」には読み直すを置かない（読み直すものが無い） */
  // 2026-09-29 組み替え 段A で API の無い画面（定義と検証）は節になった。API の無い画面の形（path: null）で同じ性質を見る
  ok(!run('ctlbar({ key: "zz-noapi", path: null }, {})').includes("cs-reload"), "API の無い画面に読み直すが出ている");
  /* 右端は DOM の順序でも守る: 他の操作部品（案件の詳細の探す欄）がある画面で、読み直すがその後ろにあること。
     今日動く先だけの見張りでは、読み直すの塊を操作列の先頭へ移しても落ちなかった（2026-09-28 逆証明） */
  const det = run('ctlbar(viewOf("deal", "detail"), { meta: { source_as_of: "2026-09-27 21:30", source_age_days: 1 } })');
  ok(det.indexOf('id="dd-go"') >= 0 && det.indexOf('id="dd-go"') < det.indexOf('<span class="reload'),
    "案件の詳細で読み直すが探す欄より前にある（右端でない）: " + det.slice(det.indexOf('<div class="ctlbar">'), det.indexOf('<div class="ctlbar">') + 160));
  ok(/<\/span><\/span>(<span class="muted small">キャッシュから表示<\/span>)?<\/div>$/.test(det), "案件の詳細で読み直すが操作列の末尾でない: " + det.slice(-120));
});

check("S-13: 今日動く先に Ctrl+クリックの案内、担当者ごとの接触は「マウスを重ねる」を前提にしない", () => {
  const td = run('renderToday({ rows: [], meta: { n_hit: 0, n_shown: 0, filter_rule: "", order_rule: "", mtg_gap: {} } })');
  const lede = td.slice(td.indexOf('<div class="lede">'), td.indexOf("</div>", td.indexOf('<div class="lede">')));
  ok(lede.includes("Ctrl+クリック") && lede.includes("&#8984;"), "今日動く先の lede に Ctrl+クリック（Mac は ⌘）の案内が無い: " + lede);
  /* Ctrl+クリックはタッチ端末に当てはまらず、400px では1画面目の2行を使った（2026-09-28 検証の指摘）。≤600px では出さない */
  ok(/<span class="pconly">案件名は Ctrl\+クリック[^<]*<\/span>/.test(lede), "Ctrl+クリックの案内が PC だけの印（.pconly）に入っていない: " + lede);
  ok(/@media \(max-width:600px\)\{ \.pconly\{ display:none; \} \}/.test(html), "CSS が ≤600px で .pconly を隠していない");
  /* 「読み直す」の隣の文は 400px で同じ行に収まる長さ（31 字は必ず折れて操作列が 28px → 58px になった） */
  const rl = run('ctlbar(viewOf("deal", "today"), { meta: { source_as_of: "2026-09-27 21:30", source_age_days: 1 } })');
  const noteTxt = (rl.match(/<span class="muted small">([^<]*20 秒[^<]*)<\/span>/) || [])[1] || "";
  ok(noteTxt && noteTxt.length <= 27, "読み直すの隣の文が長い（" + noteTxt.length + " 字。400px で次の行に折れる）: " + noteTxt);
  const ct = textOf(run('contactUnit = "month"; teamPick = ""; teamContact({ contact: __CT })'));
  ok(!ct.includes("点にマウスを重ねると"), "接触の推移の図の注記が「点にマウスを重ねると」のまま（タッチでは title が出ない）");
  ok(ct.includes("下の表にあります"), "値のある場所（下の表）を案内していない");
});

/* ================================================================ S-9（2026-09-28 UI/UX 改善・段1） */
// 鮮度の帯。日次更新を 2026-09-28 にタスクスケジューラ（毎日 21:30）へ登録したので、
// 「更新は手で回しています（自動ではありません）」は翌朝から嘘になる。予定の時刻は meta.update_schedule
// （CS_メタ か環境変数。無ければ null）から出し、コードに直書きしない。
// 緑（今日／昨日）は1行（1440px 実測で帯は 65px → 約 37px）、黄・赤は今までどおり2行。
/* 600px 以下の @media の中身（inside）と外（outside）を分ける。
   🔴 前は /@media \(max-width:600px\)\{([\s\S]*?)\n\}/ で「改行 + } 」を閉じと見ていた。統合（2026-09-28）で
   1 行の @media（.pconly / .ctlbar .reload）が入ると、その行の閉じ } は行頭に無いので次の行頭の } まで飲み込み、
   外にある .scroll-cap .cap-sp{ display:none; } が「中」に数えられて S-12 の見張りが空回りした。波括弧の対応で切る */
function media600(css) {
  const inside = [], keep = [];
  const head = "@media (max-width:600px){";
  let i = 0;
  while (i < css.length) {
    const at = css.indexOf(head, i);
    if (at < 0) { keep.push(css.slice(i)); break; }
    keep.push(css.slice(i, at));
    let k = at + head.length, depth = 1;
    for (; k < css.length && depth > 0; k++) { if (css[k] === "{") depth++; else if (css[k] === "}") depth--; }
    inside.push(css.slice(at + head.length, k - 1));
    i = k;
  }
  return { inside, outside: keep.join("") };
}

check("S-9: 鮮度の帯。緑（今日／昨日）は1行、黄・赤は2行のまま、文言は自動更新（タスクスケジューラ）に合わせる", () => {
  const box = ctx.document.getElementById("cs-fresh");
  run('setFresh({ today: "2026-09-29", generated_at: "2026-09-28 21:41:00", source_as_of: "2026-09-28 21:30:00", source_age_days: 1, update_schedule: "毎日 21:30" })');
  let h = box.innerHTML;
  ok(box.className === "fresh ok", "昨日のデータが緑でない: " + box.className);
  // 「（HubSpot から落とした時刻）」は span.src。600px 以下では出さない（400px で帯が2行 58px になっていた。内訳の1文目が同じことを言う）
  ok(/^<b>昨日　09-28 21:30<\/b> 時点のデータ<span class="src">（HubSpot から落とした時刻）<\/span><details class="fold inl">/.test(h),
    "緑の1行目が「昨日 09-28 21:30 時点のデータ（HubSpot から落とした時刻）▸ ほかの時刻」の形でない: " + h.slice(0, 200));
  ok(/元データを落としたのは 2026-09-28 21:30:00。/.test(h), "1行目を短くした分の全文（年・秒）を内訳に残していない（黙って削っている）");
  ok(/シートを作り直したのは 2026-09-28 21:41:00。計算の基準日は 2026-09-29/.test(h), "内訳の3つの時刻が出ていない: " + h);
  ok(/自動更新: 毎日 21:30（タスクスケジューラ）。/.test(h), "meta の更新の予定が出ていない");
  ok(!/手で回して|自動ではありません/.test(h), "文言が古い運用（手で回す）のまま");
  ok(/更新が止まると、この帯が赤くなります/.test(h), "止まったときの見え方（赤くなる）を書いていない");
  // 予定が無いとき: 時刻を推測で埋めない（「21:30」と直書きしていたら落ちる）
  run('setFresh({ today: "2026-09-29", generated_at: "2026-09-29 06:10:00", source_as_of: "2026-09-29 06:00:00", source_age_days: 0, update_schedule: null })');
  h = box.innerHTML;
  ok(/^<b>今日　09-29 06:00<\/b> 時点のデータ/.test(h), "今日のデータの1行目が違う: " + h.slice(0, 80));
  // 🔴 「更新は自動で回しています」も出さない。自動かどうかは運用の事実で、HTML の定数に書くとスケジューラを外した日から嘘になる
  //    （S-9「無ければ文言を出さない」。2026-09-28 検証で直書きが残っていた）
  ok(!/21:30|自動更新:|自動で回して|タスクスケジューラ/.test(h),
    "予定（meta.update_schedule）が無いのに、時刻か「自動で回している」旨を HTML の直書きで出している: " + h);
  ok(/更新が止まると、この帯が赤くなります（元データが4日以上前）。/.test(h), "予定が無くても、止まったときの見え方（赤くなる）は書く");
  // 赤（4日前）: 今までどおり2行。1行目は全文の時刻、抜けている日数を太字で
  run('setFresh({ today: "2026-09-28", generated_at: "2026-09-24 06:10:00", source_as_of: "2026-09-24 06:00:00", source_age_days: 4, update_schedule: "毎日 21:30" })');
  h = box.innerHTML;
  ok(box.className === "fresh bad", "4日前が赤でない: " + box.className);
  ok(/^このデータは <b>4日前　2026-09-24 06:00:00<\/b> 時点のものです（HubSpot から落とした時刻）。<b>4日分の動きが入っていません。<\/b><details class="fold"><summary>ほかの時刻（シートの作成・計算の基準日）<\/summary>/.test(h),
    "赤の2行の形が変わっている（1行目は全文の時刻、次に畳み）: " + h.slice(0, 220));
  ok(!/fold inl/.test(h), "赤のときに内訳を行の続きに畳んでいる（2行のままにする）");
  ok(!/元データを落としたのは/.test(h), "赤のとき、1行目に出ている全文の時刻を内訳でも繰り返している");
  // すでに赤いときは「止まると赤くなります」（仮定形）ではなく、いま赤い理由を言う（2026-09-28 検証で仮定形のままだった）
  ok(/自動更新: 毎日 21:30（タスクスケジューラ）。元データが4日以上前なので、この帯を赤くしています（予定「毎日 21:30」の更新が入っていません）。/.test(h),
    "赤のときに、更新の予定と、いま赤い理由（4日以上前・予定の更新が入っていない）を出していない: " + h);
  ok(!/更新が止まると/.test(h), "すでに赤いのに「更新が止まると赤くなります」と仮定形で書いている");
  // 赤で予定も無いとき: 自動とは言わず、止まっている可能性だけ書く
  run('setFresh({ today: "2026-09-28", source_as_of: "2026-09-24 06:00:00", source_age_days: 4 })');
  h = box.innerHTML;
  ok(!/自動|タスクスケジューラ/.test(h) && /元データが4日以上前なので、この帯を赤くしています（更新が止まっている可能性があります）。/.test(h),
    "赤で予定が無いときの文が違う: " + h);
  // 黄（2日前）も2行のまま
  run('setFresh({ today: "2026-09-28", source_as_of: "2026-09-26 06:00:00", source_age_days: 2 })');
  ok(box.className === "fresh warn" && !/fold inl/.test(box.innerHTML) &&
     /^このデータは <b>2日前　2026-09-26 06:00:00<\/b> 時点のものです/.test(box.innerHTML),
    "2日前（黄）の形が違う: " + box.className + " " + box.innerHTML.slice(0, 120));
  // 分からないとき（CS_メタ が読めない）は今までどおり「分かりません」
  run('setFresh({ today: "2026-09-28" })');
  ok(box.className === "fresh bad" && /このデータがいつのものか分かりません/.test(box.innerHTML), "メタが読めないときに「分かりません」と言わない");
  // 時刻の形が違えば縮めずにそのまま出す（推測で切らない）
  ok(run('shortWhen("2026-09-28 21:30:00")') === "09-28 21:30" && run('shortWhen("9/28 夜")') === "9/28 夜" && run("shortWhen(null)") === "",
    "shortWhen が yyyy-MM-dd HH:mm:ss 以外の形を壊す");
  // CSS: inl は行の続き（inline）。緑以外の details は今までどおり（.fresh details.fold）。
  // summary は inline-block で上下 6px・左右 4px の余白を負の margin で打ち消す: 行の高さは変えず、押せる範囲だけ広げる
  // （inline のままだと 72×12px で指では押しにくかった。2026-09-28 検証）
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/\.fresh details\.fold\.inl\{ display:inline;/.test(css), "緑の畳みを行の続きにする CSS（.inl）が無い");
  ok(/\.fresh details\.fold\.inl > summary\{ display:inline-block; padding:6px var\(--space-1\); margin:-6px calc\(-1 \* var\(--space-1\)\); \}/.test(css),
    "緑の「▸ ほかの時刻」の押せる範囲が広がっていない（inline-block＋余白＋負の margin）");
  // 600px 以下では「（HubSpot から落とした時刻）」を1行目から外す（帯を1行に。内訳の1文目に同じことが書いてある）
  const sp = media600(css).inside.join("\n");
  ok(/\.fresh\.ok \.src\{ display:none; \}/.test(sp), "600px 以下で緑の帯の「（HubSpot から落とした時刻）」を外していない（帯が2行になる）");
  ok(!/\.src\{ display:none/.test(media600(css).outside), "PC でも「（HubSpot から落とした時刻）」を消している");
});

/* ================================================================ S-12（2026-09-28 UI/UX 改善・段1） */
// スマホと操作の安全な手当て。CSS 中心、PC の見た目は変えない。
// 診断（400px）: 表を横に送ると案件名が消える／枠内の縦スクロールの罠／入力欄 12.5px で iOS が拡大／
// 詳細の dl が左列 200px／サイドの項目 30px・操作列 28px はタップに小さい／≤900px で先頭へ戻れない。
check("S-12: 表の1列目を左に貼り付け、行の地の色（縞・ホバー）も貼り付けた列に持たせる", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/th:first-child, td:first-child\{ position:sticky; left:0; z-index:1; background:var\(--panel\); \}/.test(css),
    "1列目を左に貼り付ける CSS が無い（横に送ると案件名が消える）");
  ok(/th:first-child\{ z-index:3; background:var\(--panel-3\); \}/.test(css), "左上の角（見出し行の1列目）が上の見出し（z-index:2）の下に潜る");
  ok(/tbody tr:nth-child\(even\) td:first-child\{ background:var\(--panel-2\); \}/.test(css), "縞の行で貼り付けた列の地が行と違う（下の列が透ける）");
  ok(/tbody tr:hover td:first-child\{ background:var\(--ai-soft\); \}/.test(css), "ホバーした行で貼り付けた列だけ色が変わらない");
  ok(/\.scroll-wrap\.more-l th:first-child, \.scroll-wrap\.more-l td:first-child\{\s*box-shadow:/.test(css),
    "横に送っている間、貼り付けた列の右端に影が無い（下に列が隠れていると分からない）");
  ok(/\.scroll:focus-visible\{ outline:2px solid var\(--ai\)/.test(css), "表の枠にキーボードで止まったときの見え方（outline）が無い");
  ok(/a\.backlink:focus-visible\{ outline:2px solid var\(--ai\)/.test(css), "「ダッシュボードへ戻る」に focus-visible が無い");
});

check("S-12: 600px 以下だけ、枠内の縦スクロールをやめ・入力欄 16px・タップの的 40px・詳細の dl を縦に積む。PC の見た目は変えない", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  const blocks = media600(css).inside;
  ok(blocks.length >= 2, "600px 以下の @media が見つからない: " + blocks.length);
  const inside = blocks.join("\n");
  ok(/\.scroll\{ max-height:none !important; \}/.test(inside), "600px 以下で枠内の縦スクロール（max-height）を外していない（scroll() の直書きに勝つには !important）");
  ok(/\.ctl input, \.ctl select, \.ctlbar input, \.ctlbar select, \.ctlbar input\.act, \.ctlbar select\.act\{ font-size:16px; \}/.test(inside),
    "600px 以下で入力欄が 16px でない（iOS が自動で拡大する）");
  // 枠の上の案内: PC 用（縦・横とも枠の中・見出しは残る）と 600px 以下用（横だけ枠の中・1列目は残る・見出しは残らない）を言い分ける
  ok(/\.scroll-cap \.cap-pc\{ display:none; \}/.test(inside) && /\.scroll-cap \.cap-sp\{ display:inline; \}/.test(inside),
    "600px 以下で枠の上の案内を入れ替えていない（「縦・横にスクロールします（見出しは上に残ります）」のままなら2つとも事実と違う）");
  // 貼り付けた1列目が枠の大半を取らない（400px 実測: 担当者の一覧 246/367px、案件の詳細 192/367px）。40vw を上限に折り返す
  ok(/td:first-child\{ white-space:normal; overflow-wrap:anywhere; max-width:40vw; \}/.test(inside) &&
     /td\.wl:first-child\{ min-width:min\(12em,40vw\); max-width:40vw; \}/.test(inside),
    "600px 以下で貼り付けた1列目の幅を抑えていない（残りの列を見る幅が 121px しか残らない）");
  ok(/\.side button\{[^}]*min-height:40px/.test(inside), "600px 以下でサイドの項目が 40px に届かない");
  ok(/\.ctlbar select, \.ctlbar input, \.ctlbar button\.act\{ min-height:40px; \}/.test(inside), "600px 以下で操作列の部品が 40px に届かない");
  ok(/th,td\{ padding:10px 12px; \}/.test(inside) && /th button\.sort\{ padding:10px 12px; \}/.test(inside),
    "600px 以下で表のセル（並び替えの見出しも）の当たりを広げていない");
  ok(/\.tl dl\{ grid-template-columns:1fr; \}/.test(inside), "600px 以下で詳細の要約（dl）を縦に積んでいない（左列が 200px を取る）");
  // 🔴 PC の見た目は変えない: これらは @media の外に書かない
  const outside = media600(css).outside;
  ok(!/font-size:16px/.test(outside), "16px の入力欄が PC にも効いている");
  ok(/\.scroll-cap \.cap-sp\{ display:none; \}/.test(outside) && !/\.cap-pc\{ display:none/.test(outside),
    "PC で枠の上の案内が 600px 以下用の文になっている、または PC 用の文が消えている");
  ok(!/td:first-child\{ white-space:normal/.test(outside), "1列目の折り返し（40vw の上限）が PC にも効いている");
  ok(!/max-height:none !important/.test(outside), "枠の縦スクロールを PC でも外している（640px の枠に 24 行を収める設計が崩れる）");
  ok(!/min-height:40px/.test(outside), "40px の当たりが PC にも効いている");
  ok(/\.side button\{[^}]*min-height:34px/.test(outside) && /\.ctlbar select, \.ctlbar input, \.ctlbar button\.act\{[^}]*min-height:28px/.test(outside),
    "PC のサイドの項目 34px・操作列 28px が変わっている");
});

// 🔴 CSS の文字列があるだけでは足りない（2026-09-28 検証: .ctlbar .act{ font-size:12.5px }（詳細度 0,2,0）が
//    .ctlbar select{ font-size:16px }（0,1,1）に勝ち、要素自身に class="act" を持つ #cs-houjin・#dd-q は 12.5px のままだった）。
//    ここでは簡易の cascade（詳細度 → 書いた順）で、実際の要素の連なりに当たる font-size の勝ちを決めて見る。
//    対応するのは子孫結合子（空白・>）とタグ・クラス・#id・[attr=値] だけ。疑似クラスを含む選択子は「当たらない」と扱う（font-size を持つものは無い）
function cssRules(css) {
  css = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const out = []; let order = 0;
  const walk = (s, media) => {
    let depth = 0, selStart = 0, sel = "", bodyStart = 0;
    for (let k = 0; k < s.length; k++) {
      const ch = s[k];
      if (ch === "{") { if (depth === 0) { sel = s.slice(selStart, k).trim(); bodyStart = k + 1; } depth++; }
      else if (ch === "}") {
        depth--;
        if (depth === 0) {
          const body = s.slice(bodyStart, k);
          if (/^@media/.test(sel)) walk(body, sel.replace(/^@media\s*/, ""));
          else if (!/^@/.test(sel)) out.push({ sel, body, media, order: order++ });
          selStart = k + 1;
        }
      }
    }
  };
  walk(css, null);
  return out;
}
function specificity(sel) {
  const pe = (sel.match(/::[\w-]+/g) || []).length;
  let s = sel.replace(/::[\w-]+/g, "");
  const a = (s.match(/#[\w-]+/g) || []).length;
  const b = (s.match(/\.[\w-]+|\[[^\]]*\]|:[\w-]+(\([^)]*\))?/g) || []).length;
  s = s.replace(/#[\w-]+|\.[\w-]+|\[[^\]]*\]|:[\w-]+(\([^)]*\))?/g, " ");
  const c = (s.match(/(^|[\s>+~])([a-zA-Z][\w-]*)/g) || []).length + pe;
  return a * 10000 + b * 100 + c;
}
/* 1つの複合選択子（例 "select.act"）が要素 el {tag, classes, attrs} に当たるか */
function compoundMatches(comp, el) {
  if (/:/.test(comp)) return false;
  const tag = (comp.match(/^[a-zA-Z][\w-]*|^\*/) || [""])[0];
  if (tag && tag !== "*" && tag !== el.tag) return false;
  for (const m of comp.matchAll(/\.([\w-]+)/g)) if (!(el.classes || []).includes(m[1])) return false;
  for (const m of comp.matchAll(/#([\w-]+)/g)) if ((el.attrs || {}).id !== m[1]) return false;
  for (const m of comp.matchAll(/\[([\w-]+)(?:=["']?([^"'\]]*)["']?)?\]/g)) {
    const v = (el.attrs || {})[m[1]];
    if (v == null || (m[2] != null && v !== m[2])) return false;
  }
  return true;
}
/* 選択子（子孫結合子だけ）が要素の連なり chain（先祖 → 対象）に当たるか。右から左へ、先祖は飛ばしてもよい */
function selectorMatches(sel, chain) {
  const comps = sel.trim().split(/\s*>\s*|\s+/).filter(Boolean);
  if (!compoundMatches(comps[comps.length - 1], chain[chain.length - 1])) return false;
  let ci = chain.length - 2;
  for (let i = comps.length - 2; i >= 0; i--) {
    while (ci >= 0 && !compoundMatches(comps[i], chain[ci])) ci--;
    if (ci < 0) return false;
    ci--;
  }
  return true;
}
/* chain に当たる font-size の勝ち。narrow=true なら (max-width:600px) の @media も効く */
function winningFontSize(rules, chain, narrow) {
  let best = null;
  for (const r of rules) {
    if (r.media && !(narrow && /max-width:\s*600px/.test(r.media))) continue;
    const fs = r.body.match(/(?:^|;)\s*font-size:\s*([^;!]+)(!important)?/);
    if (!fs) continue;
    const hit = r.sel.split(",").filter((s) => selectorMatches(s, chain));
    if (!hit.length) continue;
    const spec = Math.max(...hit.map(specificity)) + (fs[2] ? 1e6 : 0);
    if (!best || spec > best.spec || (spec === best.spec && r.order > best.order)) best = { spec, order: r.order, value: fs[1].trim(), sel: r.sel };
  }
  return best;
}

check("S-12: 入力欄 16px は、要素自身に class=\"act\" を持つ欄（法人を選ぶ・案件名か拠点名で探す）にも cascade で勝つ。PC は 12.5px のまま", () => {
  const rules = cssRules(html.slice(html.indexOf("<style>") + 7, html.indexOf("</style>")));
  ok(rules.length > 100, "CSS の規則が読めていない: " + rules.length);
  const bar = { tag: "div", classes: ["ctlbar"] };
  const chains = {
    "法人を選ぶ #cs-houjin（select.act）": [bar, { tag: "select", classes: ["act"], attrs: { id: "cs-houjin" } }],
    "案件名か拠点名で探す #dd-q（input.act）": [bar, { tag: "input", classes: ["act"], attrs: { id: "dd-q", type: "search" } }],
    "案件そのもの #bf-consultant（label.act の中の select）": [bar, { tag: "label", classes: ["act"] }, { tag: "select", attrs: { id: "bf-consultant" } }],
    "案件そのもの #bf-q（label.act の中の input）": [bar, { tag: "label", classes: ["act"] }, { tag: "input", attrs: { id: "bf-q", type: "search" } }],
  };
  for (const [name, chain] of Object.entries(chains)) {
    const pc = winningFontSize(rules, chain, false), sp = winningFontSize(rules, chain, true);
    ok(pc && pc.value === "12.5px", name + " の PC の font-size が 12.5px でない: " + JSON.stringify(pc));
    ok(sp && sp.value === "16px", name + " の 600px 以下の font-size が 16px でない（iOS が自動で拡大する）: " + JSON.stringify(sp));
  }
  // 実際の操作列に、その要素があること（選択子だけ合っていても要素が別の形なら意味が無い）
  run("detailQ = ''");
  const detail = run('ctlbar({ key: "detail", path: "/api/consulting/deal-detail" }, { meta: {} })');
  ok(/<input type="search" id="dd-q" class="act"/.test(detail), "案件の詳細の探す欄が input.act の形でない: " + detail.slice(0, 300));
  const series = run('ctlbar({ key: "customer", path: "/api/consulting/customer" }, { meta: {} })');
  ok(/<select id="cs-houjin" class="act"/.test(series), "法人を選ぶ欄が select.act の形でない: " + series.slice(0, 300));
  // 簡易 cascade そのものの見張り（詳細度の数え方が壊れると上の判定が空回りする）
  ok(specificity(".ctlbar .act") === 200 && specificity(".ctlbar select") === 101 && specificity(".ctlbar select.act") === 201 &&
     specificity("#cs-houjin") === 10000 && specificity("th button.sort:focus-visible") === 202,
    "詳細度の数え方が違う: " + [".ctlbar .act", ".ctlbar select", ".ctlbar select.act", "#cs-houjin", "th button.sort:focus-visible"].map(specificity));
  ok(selectorMatches(".ctlbar select.act", chains["法人を選ぶ #cs-houjin（select.act）"]) &&
     !selectorMatches(".ctlbar select.act", chains["案件そのもの #bf-consultant（label.act の中の select）"]) &&
     selectorMatches(".ctlbar .act select", chains["案件そのもの #bf-consultant（label.act の中の select）"]),
    "選択子の当たり判定が違う");
});

check("S-12: 枠の上の案内（scrollCap）は PC 用と 600px 以下用の両方の文を出し、事実に合う方だけ CSS で見せる", () => {
  const h = run('scroll(table([{ t: "a" }, { t: "b" }], [[1, 2], [3, 4]]), 640)');
  ok(/<span class="cap-pc">入り切らない分は枠の中で縦・横にスクロールします（見出しは上に残ります）。<\/span>/.test(h),
    "PC 用の文（縦・横とも枠の中、見出しは上に残る）が無い: " + h.slice(0, 400));
  ok(/<span class="cap-sp">横に入り切らない分は枠の中で横にスクロールします（1列目は左に残ります）。縦はページと一緒に流れます（見出しの行は残りません）。<\/span>/.test(h),
    "600px 以下用の文（横だけ枠の中、1列目は残る、見出しの行は残らない）が無い: " + h.slice(0, 400));
  ok(/全 <b>2<\/b> 行 × 2 列。<span class="cap-pc">/.test(h), "件数の文の直後に案内が続いていない");
});

check("S-12: 900px 以下ではサイドバーを上に貼り付け（sticky）、どこまで送っても項目を切り替えられる", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  const m = css.match(/@media \(max-width:900px\)\{([\s\S]*?)\n  \}/);
  ok(m, "900px 以下の @media が見つからない");
  ok(/\.side\{ position:sticky; top:0; z-index:5; background:var\(--paper\);/.test(m[1]),
    "900px 以下でサイドバーが上に貼り付いていない（static だと先頭まで戻るしかない）、または地の色が無い（本文が透ける）");
  ok(!/\.side\{[^}]*position:static/.test(m[1]), "サイドバーが static に戻っている");
  // PC は左の列に居座る今までどおり
  ok(/\.side\{ position:sticky; top:var\(--space-3\); align-self:start;/.test(css), "PC のサイドバー（左の列に sticky）が変わっている");
});

check("S-12: 表の枠は、実際にはみ出しているときだけ Tab で止まり、直前の見出しの読み上げ名が付く（図の枠と同じ条件）", () => {
  const attrs = {}, cls = new Set();
  const inner = { scrollWidth: 1000, clientWidth: 400, scrollLeft: 0, scrollHeight: 300, clientHeight: 300,
    setAttribute: (k, v) => { attrs[k] = v; }, removeAttribute: (k) => { delete attrs[k]; } };
  /* 枠の前には scroll-cap（件数の1行）が挟まり、その前に h2 の見出しがある */
  const cap = { tagName: "DIV", previousElementSibling: { tagName: "H2", textContent: " 表 \n 今日動く先（24 件） " } };
  ctx.__W3 = { querySelector: () => inner, previousElementSibling: cap,
    classList: { toggle: (c, on) => { if (on) cls.add(c); else cls.delete(c); } } };
  run("markScroll(__W3)");   // 400px 幅: 横にはみ出している
  ok(attrs.tabindex === "0" && attrs.role === "region" && attrs["aria-label"] === "表 今日動く先（24 件）（スクロールできる表の枠）",
    "はみ出している枠に tabindex / 読み上げ名が無い、または見出しを拾えていない: " + JSON.stringify(attrs));
  ok(cls.has("more-r"), "影の付け外し（more-r）が止まっている: " + [...cls]);
  inner.scrollWidth = 400; run("markScroll(__W3)");   // 1440px 幅: 収まった
  ok(!("tabindex" in attrs) && !("role" in attrs) && !("aria-label" in attrs),
    "はみ出していない枠に tabindex が残っている（キーボードの移動が1つ増える, F1 と同じ）: " + JSON.stringify(attrs));
  inner.scrollHeight = 900; run("markScroll(__W3)");   // PC の枠 640px に 24 行: 縦にはみ出す
  ok(attrs.tabindex === "0" && attrs.role === "region", "縦にはみ出している枠で Tab で止まれない: " + JSON.stringify(attrs));
  // 見出しが見つからなければ「表」
  ctx.__W4 = { querySelector: () => inner, classList: { toggle() {} } };
  run("markScroll(__W4)");
  ok(attrs["aria-label"] === "表（スクロールできる表の枠）", "見出しが無いときの読み上げ名: " + attrs["aria-label"]);
  // 見出しは「表」の札（span）と題が並ぶ。子ごとに区切って空白でつなぐ（textContent だと「表案件の立ち位置」と続く。2026-09-28 実測）
  ctx.__W5 = { querySelector: () => inner, classList: { toggle() {} },
    previousElementSibling: { tagName: "H2", textContent: "表案件の立ち位置",
      childNodes: [{ textContent: "表" }, { textContent: "\n  " }, { textContent: "案件の立ち位置" }] } };
  run("markScroll(__W5)");
  ok(attrs["aria-label"] === "表 案件の立ち位置（スクロールできる表の枠）", "札と題が続けて読まれる: " + attrs["aria-label"]);
  // 🔴 図の見出しの下に表が続く場所（担当の交代の担当者のまとめ: h2「図 …」→ figure → scroll-cap → 枠）は
  //    「図 …（スクロールできる表の枠）」と読み上げていた（2026-09-28 検証）。札が「図」なら「<題> の表」にする
  const figHead = { tagName: "H2", childNodes: [{ textContent: "図" }, { textContent: "引き継いだ側（次の担当）（変化の中央値）" }] };
  const figure = { tagName: "FIGURE", previousElementSibling: figHead };
  ctx.__W6 = { querySelector: () => inner, classList: { toggle() {} },
    previousElementSibling: { tagName: "DIV", previousElementSibling: figure } };
  run("markScroll(__W6)");
  ok(attrs["aria-label"] === "引き継いだ側（次の担当）（変化の中央値） の表（スクロールできる表の枠）",
    "図の見出しの下の表が「図 …」と読み上げられる: " + attrs["aria-label"]);
  // 🔴 畳み（details）の中の表（継続回数×成果の満了月ごとの内訳: h2「表 …」→ details > summary + 枠）は同じ階層に見出しが無く
  //    「表」になっていた。見つからなければ親の階層で探し直す（#cs-main まで）
  const tblHead = { tagName: "H2", childNodes: [{ textContent: "表" }, { textContent: "満了月ごとの内訳" }] };
  const details = { tagName: "DETAILS", previousElementSibling: tblHead, parentNode: { id: "cs-main" } };
  ctx.__W7 = { querySelector: () => inner, classList: { toggle() {} },
    previousElementSibling: { tagName: "SUMMARY" }, parentNode: details };
  run("markScroll(__W7)");
  ok(attrs["aria-label"] === "表 満了月ごとの内訳（スクロールできる表の枠）", "畳みの中の表の読み上げ名が親の見出しを拾わない: " + attrs["aria-label"]);
  // 本文の入れ物（#cs-main）より上へは探しに行かない（別の画面の見出しを拾わない）
  ctx.__W8 = { querySelector: () => inner, classList: { toggle() {} },
    parentNode: { id: "cs-main", previousElementSibling: { tagName: "H2", childNodes: [{ textContent: "問い" }, { textContent: "別の見出し" }] } } };
  run("markScroll(__W8)");
  ok(attrs["aria-label"] === "表（スクロールできる表の枠）", "#cs-main の外の見出しを拾っている: " + attrs["aria-label"]);
  // 描いた時点（scroll()）では付けない。枠の大きさは描いた後にしか測れない
  ok(!/tabindex|role="region"/.test(run('scroll(table([{ t: "a" }], [[1]]), 400)')), "描いた時点で tabindex / role を付けている");
});

/* ================================================================ UI/UX 改善 段2「状態と URL」（2026-09-28、handover 08 の M-6 / M-7） */
check("M-6/M-7: 本文へ飛ぶ・読み上げの領域（#cs-status）・main の tabindex/aria-busy・失敗の枠の role=alert・骨組みは数字を出さない", () => {
  /* 2026-09-28 診断: aria-live / aria-busy / role=status / role=alert が 0 件、スキップリンクが無く、本題まで Tab 10〜15 回 */
  ok(/<a class="skip" id="cs-skip" href="#cs-main">本文へ飛ぶ<\/a>/.test(html), "先頭に「本文へ飛ぶ」が無い");
  ok(/<div id="cs-status" class="sr" role="status" aria-live="polite"><\/div>/.test(html), "読み上げの領域（#cs-status、role=status / aria-live=polite）が無い");
  ok(/<main class="pane on" id="cs-main" tabindex="-1" aria-busy="false">/.test(html), "本文（main）に tabindex=-1 / aria-busy が無い（移動の後に focus() で止まれない）");
  ok(/<div id="cs-error" style="display:none" role="alert" tabindex="-1">/.test(html), "失敗の枠に role=alert / tabindex=-1 が無い");
  ok(/a\.skip:focus\{/.test(html) && /#cs-main:focus-visible/.test(html), "「本文へ飛ぶ」の focus 時の見え方、本文の focus-visible の定義が無い");
  /* 読み上げの領域は本文（#cs-main）の外に置く。中に置くと innerHTML の差し替えで消えて、変化が伝わらない */
  ok(html.indexOf('id="cs-status"') < html.indexOf('id="cs-main"'), "読み上げの領域が本文の中にある、または本文より後にある");
  const sk = run('cur = { menu: "deal", view: "today" }; skeleton(viewOf("deal", "today"), false)');
  ok(sk.includes('<h2 class="sec mincho"><span class="no">毎日</span>今日</h2>'), "骨組みに画面名の見出しが無い: " + sk);
  ok(sk.includes('<div class="loading" id="cs-loading">今日 を読み込み中…</div>'), "骨組みに状態の 1 行が無い: " + sk);
  ok(sk.includes('<div class="skel" aria-hidden="true">'), "骨組みの空箱が読み上げに出る（aria-hidden が無い）");
  ok(!/\d/.test(sk.replace(/<[^>]+>/g, "")), "骨組みに数字が出ている（空箱に 0 を出すと「0 件」と読まれる）: " + sk.replace(/<[^>]+>/g, ""));
  const sk2 = run('skeleton(viewOf("deal", "today"), true)');
  ok(sk2.includes("取り直し中") && sk2.includes("20 秒"), "取り直し中の骨組みに待つ理由と長さ（20 秒）が無い（S-11）");
});

/* ================================================================ 段2 枠と見出し（M-1 / M-5 / M-12 / 段1レビュー B・C・F、2026-09-28） */
// 診断（fixture 1440×900）: 題字 75px・鮮度 65px・色と印の意味 40px・上のメニュー 45px・母集団 40px・操作列 40px で本題が y≈357。
// 400px では S-12 で枠の高さ制限を外した結果、案件そのものの全高が 37,579px・担当の交代 58,806px。側柱 125px が常に画面を占める。
check("M-1: 題字の行に鮮度（緑）と「色と印の意味」を並べ、黄・赤の帯は題字の下の行に戻す。母集団の 1 行は操作列の中", () => {
  const head = html.slice(html.indexOf('<header class="masthead">'), html.indexOf("</header>"));
  ok(head.includes('<div id="cs-fresh" class="fresh"></div>'), "鮮度の帯（#cs-fresh）が題字の行（header.masthead）の中に無い");
  // M-1 の (3): 「色と印の意味」は全画面の畳みをやめ、定義と検証への 1 語のリンク（2026-09-29 検証: 畳みのまま残っていて案と違った）
  ok(head.includes('<a class="golink" id="cs-legend" href="#monthly/trust?at=trust-defs" title="記録と数字の信頼度の「色と印の意味」の表へ">色と印の意味 →</a>'), "「色と印の意味」が題字の行の中の定義と検証へのリンクでない");
  ok(!/<details[^>]*id="cs-legend"/.test(html) && !head.includes('<div class="legend">'), "「色と印の意味」の畳み（凡例の中身）がヘッダに残っている");
  // 2026-09-29 組み替え 段A: 定義と検証は「記録と数字の信頼度」の最後の節（id=trust-defs）。リンク先の画面と節が実在すること
  ok(run('MENUS.find((m) => m.key === "monthly").views.some((v) => v.key === "trust")'), "リンク先（#monthly/trust）の画面が無い");
  const tr = run("renderTrust(Object.assign({}, __DQ, { _more: { mtgq: __MQ } }))");
  const at = tr.indexOf('id="trust-defs"');
  ok(at >= 0 && tr.indexOf("色と印の意味", at) > at, "リンク先の節（id=trust-defs、色と印の意味の表）が記録と数字の信頼度に無い");
  ok(head.indexOf('id="cs-fresh"') < head.indexOf('id="cs-legend"') && head.indexOf('id="cs-legend"') < head.indexOf('class="stamp"'),
    "題字の行の並びが 題字 → 鮮度 → 色と印の意味 → 利用者 でない");
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/\.masthead h1\{ font-size:18px;/.test(css), "題字が 18px でない（27px の行が 75px を取っていた）");
  ok(/\.masthead \.fresh\.ok\{ flex:0 1 auto;/.test(css), "緑の帯が題字の行の中の 1 語（flex の項目）でない");
  // 🔴 黄・赤は消さない: 全幅の行にして題字の下へ（規律「いつのデータか」）
  ok(/\.masthead \.fresh\.warn, \.masthead \.fresh\.bad\{ flex:1 1 100%; order:5; \}/.test(css), "黄・赤の帯が題字の下の全幅の行に戻らない");
  ok(/\.masthead > a\.golink\{ flex:0 0 auto;/.test(css) && !/\.masthead > details\.fold/.test(css),
    "「色と印の意味」のリンクが題字の行の 1 語（折り返さない flex の項目）でない、または使わなくなった畳みの CSS が残っている");
  ok(/\.tabs\{ display:flex; gap:0; flex-wrap:wrap; margin:0 0 var\(--space-3\);/.test(css), "上のメニューの下の余白が 26px のまま");
  ok(/\.rule-thin\{[^}]*margin:0 0 var\(--space-2\);/.test(css), "二重罫の下の余白が 18px のまま");
  ok(/\.layout\{ display:grid; grid-template-columns:160px minmax\(0,1fr\);/.test(css), "側柱が 160px でない（M-12）");
  // 母集団の 1 行（popline）は操作列の先頭。畳み方（1 行目は summary）は V1 のまま
  const bar = run('ctlbar(viewOf("deal", "today"), { meta: { source_as_of: "2026-09-27 21:30", source_age_days: 1 }, population: { active: 604, active_all: 703, active_option: 99, deals: 3432, deals_all: 3656, deals_option: 224 } })');
  ok(bar.startsWith('<div class="ctlbar"><details class="popnote fold"><summary>稼働中 <b>604</b>'), "母集団の 1 行が操作列（.ctlbar）の先頭に無い: " + bar.slice(0, 120));
  ok(bar.indexOf("</details>") < bar.indexOf('id="cs-reload"'), "母集団の畳みが読み直すより後ろにある");
  ok(/\.ctlbar > details\.popnote\.fold\{ margin:0; flex:0 1 auto;/.test(css) && /\.ctlbar > details\.popnote\.fold\[open\]\{ flex-basis:100%; \}/.test(css),
    "操作列の中の母集団の畳みの CSS（開いたら全幅）が無い");
  ok(/\.pane > \.ctlbar \+ h2\.sec\{ margin-top:var\(--space-3\); \}/.test(css), "操作列と問いの間が 12px でない");
  // 400px: 題字は 16px（20px だった）。h1 の中の span は無くなったので、その CSS も残さない
  ok(/\.masthead h1\{ font-size:16px; \}/.test(media600(css).inside.join("\n")), "400px の題字が 16px でない");
  ok(!/\.masthead h1 span/.test(css), "無くなった h1 の中の span の CSS が残っている");
});

check("M-5: 問い・案件は 24px 明朝（画面の題）、図・表・顧客は 15px ゴシック（部品の題）。sec() の第 1 引数で分ける", () => {
  ok(run('sec("問い", "x")') === '<h2 class="sec mincho"><span class="no">問い</span>x</h2>', "問いの形が変わった（見張り S-2 と同じ形）");
  ok(run('sec("案件", "x")').startsWith('<h2 class="sec mincho">'), "案件の詳細の題（案件 …）が画面の題（mincho）でない");
  for (const no of ["図", "表", "顧客"])
    ok(run("sec(" + JSON.stringify(no) + ", \"x\", \"i1\")") === '<h2 class="sec part" id="i1" tabindex="-1"><span class="no">' + no + "</span>x</h2>",
      no + " の見出しが部品の題（class part）でない、または id / tabindex が付かない: " + run("sec(" + JSON.stringify(no) + ", \"x\", \"i1\")"));
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/h2\.sec\.mincho\{ font-size:24px;/.test(css), "問いが 24px でない");
  ok(/h2\.sec\.part\{ font-size:15px; font-weight:700;/.test(css) && /h2\.sec\.part::after\{ display:none; \}/.test(css),
    "図・表の見出しが 15px ゴシック太字（罫線なし）でない");
  // 400px では問いを 20px に（24px だと本文幅 368px で 2 行に折れた。2026-09-28 検証）
  ok(/h2\.sec\.mincho\{ font-size:20px; \}/.test(media600(css).inside.join("\n")), "600px 以下で問いが 20px でない");
});

check("M-5: 決まりごと・読み方の箱を畳む（foldNote）。畳まない 1 文（評価ではありません）は summary に残し、本文で繰り返さない。外した件数を含む箱は畳まない", () => {
  // 部品そのもの
  const f = run('foldNote("def", "見出し<", "本文", "残す文", "fid", "<i>後ろ</i>")');
  ok(f === '<details class="fold notefold" id="fid"><summary>見出し&lt;<span class="keep">残す文</span>　<span class="when-closed">決まりごとを開く</span><span class="when-open">決まりごとを閉じる</span></summary>' +
           '<div class="note def nohd"><p>本文</p></div><i>後ろ</i></details>', "foldNote の形が違う: " + f);
  ok(run('foldNote("info", "h", "b")').includes("読み方を開く") && !run('foldNote("info", "h", "b")').includes('class="keep"'), "読み方の箱の summary が「読み方を開く」でない、または keep 無しで空の span を出す");
  ok(JSON.stringify(run('firstSentence("一文目。二文目。")')) === '["一文目","二文目。"]' && JSON.stringify(run('firstSentence("句点なし")')) === '["句点なし",""]',
    "firstSentence が最初の句点で分けない");
  // 担当者の一覧: 「担当者の評価ではありません」は畳まず summary に。本文は残りの文＋contact_rule で、同じ文を繰り返さない
  const D = JSON.parse(JSON.stringify(ctx.__D));
  D.meta.not_counted = "※ 担当者の評価ではありません。手が足りていない場所を見つけるための画面です";
  D.contact_rule = "接触 ＝ MTG または60秒超の通話";
  ctx.__M5T = D;
  const t = run("renderTeam(teamOf(__M5T))");
  const box = t.slice(t.indexOf('<details class="fold notefold">'), t.indexOf("</details>", t.indexOf('<details class="fold notefold">')));
  ok(box.startsWith('<details class="fold notefold"><summary>何のための画面か<span class="keep">担当者の評価ではありません</span>　<span class="when-closed">読み方を開く</span>'),
    "担当者の一覧の読み方の箱が畳みでない、または「評価ではありません」が summary に無い: " + box.slice(0, 200));
  ok(box.includes('<div class="note info nohd"><p>手が足りていない場所を見つけるための画面です<br>接触 ＝ MTG または60秒超の通話</p></div>'),
    "畳んだ本文が「残りの文 + contact_rule」でない（1 文目を繰り返している、または見出し .hd を持つ）: " + box);
  ok((t.match(/担当者の評価ではありません/g) || []).length === 1, "「担当者の評価ではありません」が summary と本文で 2 回出ている");
  ok(t.indexOf('<details class="fold notefold">') < t.indexOf('<div class="kpis">'), "読み方の畳みが KPI より後ろ");
  // 畳んだ画面: 事業所・法人（粒度の 1 文が summary）、成果とリスク・立ち上がり（数えていないもの）、担当の交代（一覧の決まりごと）、本部アプローチ
  ok(run("renderCustomer(__SER)").includes('<details class="fold notefold"><summary>ここから下は拠点をまたいで並べています　<span class="when-closed">読み方を開く'),
    "顧客の画面の粒度の箱（法人の節）が畳みでない（粒度の 1 文は summary に残る）");
  // 成果とリスク・立ち上がりは summary に規律の 1 文（keep）が付く（下の「担当者ごとの接触・立ち上がり・成果とリスク」の見張り）
  ok(run("renderOutcome(__OUT)").includes('<details class="fold notefold"><summary>この画面で数えていないもの<span class="keep">'),
    "成果とリスクの決まりごとが畳みでない");
  ok(run("renderRampup(__RU)").includes('<details class="fold notefold"><summary>この画面で数えていないもの<span class="keep">'), "立ち上がりの決まりごとが畳みでない");
  const hv = run("renderHandover(__HOC)");
  ok(hv.includes('<details class="fold notefold"><summary>この一覧の決まりごと　') && hv.indexOf('<summary>この一覧の決まりごと') > hv.indexOf('id="ho-tbl"'),
    "担当の交代の決まりごとが畳みでない、または表より前");
  ok(run("renderHq(__HQ)").startsWith('<details class="fold notefold"><summary>比べる単位は事業所　'), "本部アプローチの頭の箱が畳みでない");
  run('cur = { menu: "deal", view: "board" }; boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "" };');
  const b = run("renderBoard(__BD)");
  run('cur = { menu: "deal", view: "today" };');
  ok(b.includes('<details class="fold notefold"><summary>この並びについて　') && b.indexOf("<summary>この並びについて") > b.indexOf('id="board-tbl"'),
    "案件そのものの並びの決まりが畳みでない、または表より前");
  // 🔴 畳まないもの: 外した件数を書いた箱（電話の「オプション契約の通話 N 行は数えていません」、MTG の品質）はそのまま
  const ph = JSON.parse(JSON.stringify(ctx.__PH)); ph.reach.option_rows_excluded = 12; ctx.__M5P = ph;
  const p = run("renderPhone(__M5P)");
  ok(/<div class="note def"><span class="hd">接触 ＝ 60 秒より長い通話<\/span><p>[^<]*<b>/.test(p) || p.includes('<div class="note def"><span class="hd">接触 ＝ 60 秒より長い通話</span>'),
    "電話の頭の箱（外した件数を含む）まで畳んでいる");
  ok(p.indexOf("オプション契約の通話 12 行は数えていません") > 0 && p.lastIndexOf('<details class="fold notefold">', p.indexOf("オプション契約の通話 12 行")) < 0 ||
     p.lastIndexOf("</details>", p.indexOf("オプション契約の通話 12 行")) > p.lastIndexOf('<details class="fold notefold">', p.indexOf("オプション契約の通話 12 行")),
    "外した件数（オプション契約の通話 12 行）が畳みの中に入っている");
  ok(run("renderMtgQ(__MQ)").includes('<div class="note def"><span class="hd">読むときの注意</span>'), "MTG の品質の頭の箱（外した件数を含む）まで畳んでいる");
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/details\.fold\.notefold > summary \.keep\{ color:var\(--ink\); font-weight:400;/.test(css), "summary の残す文（keep）の CSS が無い（本文と同じ濃さにする）");
});

// 🔴 2026-09-29 検証: 担当者の一覧しか見ていなかったので、担当者ごとの接触（最初の句点で切って評価の文が畳みの中）と
// 立ち上がり（keep 無し）で「評価ではありません」が閉じた畳みの中に入っても落ちなかった。成果とリスクの「処方には使いません」も丸ごと畳まれていた。
// 閉じたままでも見える summary（keep）に規律の 1 文があること、本文で繰り返さないことを見る
check("M-5: 担当者ごとの接触・立ち上がり・成果とリスクでも「評価ではありません」「処方には使いません」は閉じた畳みの summary に出る", () => {
  const keepOf = (h) => {
    const i = h.indexOf('<details class="fold notefold">');
    ok(i >= 0, "頭の決まりごとが畳みでない: " + h.slice(0, 200));
    const sum = h.slice(i, h.indexOf("</summary>", i));
    return { sum, keep: (sum.match(/<span class="keep">([\s\S]*?)<\/span>/) || [])[1] || "",
      body: h.slice(h.indexOf("</summary>", i), h.indexOf("</details>", i)) };
  };
  const ct = keepOf(run('contactUnit = "month"; teamPick = ""; teamContact({ contact: __CT })'));
  ok(ct.keep.includes("接触は検知専用です") && ct.keep.includes("担当者の評価ではありません"),
    "担当者ごとの接触の summary に「担当者の評価ではありません」が無い（畳みの中に隠れている）: " + ct.sum);
  ok(!ct.body.includes("評価ではありません") && ct.body.includes("もめている案件ほど"),
    "担当者ごとの接触の本文が評価の文を繰り返している、または残りの文が無い: " + ct.body);
  const ru = keepOf(run("renderRampup(__RU)"));
  ok(ru.keep.includes("担当者の評価ではありません") && ru.keep.includes("良し悪しの判断は人がします"),
    "立ち上がりの summary に「担当者の評価ではありません」が無い（畳みの中に隠れている）: " + ru.sum);
  ok(!ru.body.includes("評価ではありません") && ru.body.includes("最初の MTG までに何日かかったか"),
    "立ち上がりの本文が評価の文を繰り返している、または何を見ているかが無い: " + ru.body);
  const oc = keepOf(run("renderOutcome(__OUT)"));
  ok(oc.keep.includes("接触は検知にだけ使っています。処方には使いません"), "成果とリスクの summary に「処方には使いません」が無い: " + oc.sum);
  ok(!oc.body.includes("処方には使いません"), "成果とリスクの本文が「処方には使いません」を繰り返している");
  // 部品: upTo の語を含む文まで残す。語が無ければ最初の句点
  ok(JSON.stringify(run('firstSentence("一。二に評価。三。", "評価")')) === '["一。二に評価","三。"]' &&
     JSON.stringify(run('firstSentence("一。二。", "無い語")')) === '["一","二。"]', "firstSentence の upTo が効かない");
});

check("段1レビュー B: 600px 以下では表の先頭 20 行だけ出し、残りは「残り N 行を出す」で出す。高さを制限しない表・行の少ない表・PC では何もしない", () => {
  const cols = [{ t: "名前" }, { t: "値", n: 1 }];
  const rows = (n) => Array.from({ length: n }, (_, i) => ["行" + i, i]);
  ctx.__RB = rows(40); ctx.__RC = cols;
  const h = run("scroll(table(__RC, __RB), 640)");
  ok(run("RC_CAP") === 20, "先頭に出す行数が 20 でない: " + run("RC_CAP"));
  ok(/<div class="scroll-wrap rc-cut" data-rck="[^"]+">/.test(h), "隠す行のある枠に rc-cut（と押したことを覚える印 data-rck）が付かない: " + h.slice(0, 400));
  ok((h.match(/<tr data-rc="1">/g) || []).length === 20 && (h.match(/<tr>/g) || []).length === 21, "21 行目からに data-rc が付かない（見出しの 1 行 + 本文 20 行は付けない）: " + h.slice(0, 300));
  ok(h.indexOf('<tr data-rc="1">') > h.indexOf("行19</td>") && h.indexOf('<tr data-rc="1">') < h.indexOf("行20</td>"), "data-rc の付き始めが 21 行目でない");
  ok(/<\/div><button type="button" class="rc-more">残り 20 行を出す（ページが長くなります）<\/button><\/div>$/.test(h), "「残り 20 行を出す」のボタンが枠の直後に無い: " + h.slice(-160));
  ok(/<span class="cap-rc">先頭 20 行を出しています（残り 20 行は表の下のボタンで）。<\/span><\/div>/.test(h), "枠の案内に隠している行数が無い（黙って隠さない）: " + h.slice(0, 400));
  // 隠せるのが 10 行以下（25 行）・高さを制限しない表（今日動く先）・表が 1 つでない枠には付けない
  ctx.__RB2 = rows(30);
  const h2 = run("scroll(table(__RC, __RB2), 640)");
  ok(!h2.includes("data-rc") && !h2.includes("rc-more") && !h2.includes("cap-rc"), "30 行（隠せるのは 10 行）の表にも先頭 20 行の仕組みを付けている");
  const h3 = run('scroll(table(__RC, __RB), "none")');
  ok(!h3.includes("data-rc") && !h3.includes("rc-more"), "高さを制限しない表（今日動く先の 24 行を一望する）まで 20 行で切っている");
  const h4 = run("scroll(table(__RC, __RB) + table(__RC, __RB), 640)");
  ok(!h4.includes("data-rc"), "表が 2 つある枠に付けている（行数が数えられない）");
  // CSS: 隠すのは 600px 以下だけ。PC では文もボタンも出さない
  const css = html.slice(0, html.indexOf("</style>"));
  const inside = media600(css).inside.join("\n"), outside = media600(css).outside;
  ok(/\.scroll-wrap\.rc-cut:not\(\.rc-open\) tr\[data-rc\]\{ display:none; \}/.test(inside), "600px 以下で 21 行目からを隠す CSS が無い");
  ok(/\.scroll-wrap\.rc-cut:not\(\.rc-open\) > \.rc-more\{ display:block;[^}]*min-height:40px/.test(inside), "600px 以下でボタンを出す CSS（40px の的）が無い");
  ok(/\.scroll-cap \.cap-rc\{ display:inline; \}/.test(inside) && /\.scroll-cap\.rc-open \.cap-rc\{ display:none; \}/.test(inside), "600px 以下で案内の文を出し、開いたら消す CSS が無い");
  ok(/\.rc-more\{ display:none; \}/.test(outside) && /\.scroll-cap \.cap-rc\{ display:none; \}/.test(outside), "PC でボタン・案内の文を隠す CSS が無い");
  ok(!/tr\[data-rc\]/.test(outside), "PC でも 21 行目からを隠している");
  ok(/\.scroll\{ max-height:none !important; \}/.test(inside), "枠内の縦スクロールをやめる S-12 の CSS が消えている（先頭 20 行はその代わり）");
  // 押したら枠と案内に rc-open が付く（window の捕捉で受ける。描き直しで作り直されるため）
  const wrapCls = new Set(), capCls = new Set();
  const inner = { scrollWidth: 400, clientWidth: 400, scrollLeft: 0, scrollHeight: 300, clientHeight: 300, setAttribute() {}, removeAttribute() {}, getAttribute: () => null };
  const cap = { classList: { contains: (c) => c === "scroll-cap", toggle: (c, on) => { if (on) capCls.add(c); else capCls.delete(c); }, add: (c) => capCls.add(c) } };
  const wrap = { querySelector: () => inner, previousElementSibling: cap,
    classList: { add: (c) => wrapCls.add(c), toggle: (c, on) => { if (on) wrapCls.add(c); else wrapCls.delete(c); } } };
  const btn = { closest: (s) => (s === ".rc-more" ? btn : s === ".scroll-wrap" ? wrap : null) };
  ctx.__EV = { target: btn };
  run("rcMoreClick(__EV)");
  ok(wrapCls.has("rc-open") && capCls.has("rc-open"), "押しても枠と案内に rc-open が付かない: " + [...wrapCls] + " / " + [...capCls]);
  const clicks = winListeners.filter((l) => l.type === "click");
  ok(clicks.length === 1 && clicks[0].capture === true && clicks[0].fn === run("rcMoreClick"), "ボタンの click を window の捕捉で受けていない");
  ok(!run("rcMoreClick({ target: { closest: () => null } })"), "ボタン以外を押したときに落ちる");
});

// 🔴 2026-09-29 検証（Playwright 400px、#deal/board）: 「残り N 行を出す」で 604 行を出した後、列の見出しで並べ替えると
// 描き直しで rc-open が消えて 20 行に戻った。押したことを覚えて（rcOpen）、描き直しても開いたまま描く
check("段1レビュー B の残り: 「残り N 行を出す」を押した表は、並べ替え・絞り込みで描き直しても開いたまま（別の表・別の画面には移らない）", () => {
  const cols = [{ t: "名前" }, { t: "値", n: 1 }];
  ctx.__RO = Array.from({ length: 40 }, (_, i) => ["行" + i, i]); ctx.__ROC = cols;
  run('rcOpen.clear(); cur = { menu: "deal", view: "board" };');
  const before = run('scroll(table(__ROC, __RO, "", "ro-tbl"), 640)');
  const key = (before.match(/data-rck="([^"]+)"/) || [])[1];
  ok(key && !before.includes("rc-open"), "押す前から開いている、または印が無い: " + before.slice(0, 300));
  // ボタンを押す（偽の枠。data-rck を返す）
  const wrap = { querySelector: () => null, previousElementSibling: null, classList: { add() {}, toggle() {} },
    getAttribute: (a) => (a === "data-rck" ? key.replace(/&amp;/g, "&") : null) };
  const btn = { closest: (s) => (s === ".rc-more" ? btn : s === ".scroll-wrap" ? wrap : null) };
  ctx.__EVO = { target: btn };
  run("rcMoreClick(__EVO)");
  // 並べ替えた後の描き直し（行の順が変わっても同じ表）
  ctx.__RO2 = ctx.__RO.slice().reverse();
  const after = run('scroll(table(__ROC, __RO2, "", "ro-tbl"), 640)');
  ok(/<div class="scroll-wrap rc-cut rc-open" data-rck=/.test(after), "描き直すと 20 行に戻る（枠に rc-open が付かない）: " + after.slice(0, 300));
  ok(after.includes('<div class="scroll-cap rc-open">'), "描き直すと案内に「先頭 20 行を出しています」が戻る（案内に rc-open が付かない）");
  // id の無い表は見出しの行で見分ける。別の表・別の画面には移らない
  ok(!run('scroll(table(__ROC, __RO, "", "other-tbl"), 640)').includes("rc-open"), "押していない別の表まで開いている");
  run('cur = { menu: "deal", view: "today" };');
  ok(!run('scroll(table(__ROC, __RO, "", "ro-tbl"), 640)').includes("rc-open"), "別の画面の同じ id の表まで開いている");
  run("rcOpen.clear()");
});

check("段1レビュー F: 枠の上のスクロールの案内は、実際にはみ出しているときだけ見せる（markScroll が .fit を付け外し）。マウスの案内は PC だけ", () => {
  const capCls = new Set();
  const inner = { scrollWidth: 400, clientWidth: 400, scrollLeft: 0, scrollHeight: 300, clientHeight: 300, setAttribute() {}, removeAttribute() {} };
  const cap = { classList: { contains: (c) => c === "scroll-cap", toggle: (c, on) => { if (on) capCls.add(c); else capCls.delete(c); } } };
  ctx.__FW = { querySelector: () => inner, previousElementSibling: cap, classList: { toggle() {} } };
  run("markScroll(__FW)");   // はみ出していない
  ok(capCls.has("fit"), "はみ出していない枠の案内に fit が付かない（「入り切らない分は…」が常に出る）");
  inner.scrollWidth = 1000; run("markScroll(__FW)");   // 横にはみ出す
  ok(!capCls.has("fit"), "横にはみ出しているのに fit が残る（案内が消える）");
  inner.scrollWidth = 400; inner.scrollHeight = 900; run("markScroll(__FW)");   // 縦にはみ出す（PC の 640px の枠）
  ok(!capCls.has("fit"), "縦にはみ出しているのに fit が残る");
  // 今日動く先: 案内は .caprow の中（畳みと同じ行）。そこでも見つける
  inner.scrollHeight = 300;
  const capCls2 = new Set();
  const cap2 = { classList: { contains: (c) => c === "scroll-cap", toggle: (c, on) => { if (on) capCls2.add(c); else capCls2.delete(c); } } };
  ctx.__FW2 = { querySelector: () => inner, classList: { toggle() {} },
    previousElementSibling: { classList: { contains: (c) => c === "caprow" }, querySelector: (s) => (s === ".scroll-cap" ? cap2 : null) } };
  run("markScroll(__FW2)");
  ok(capCls2.has("fit"), "caprow の中の案内に fit が付かない");
  // 見出しなど classList の無い前の要素でも落ちない（S-12 の見張りの偽の枠と同じ形）
  ctx.__FW3 = { querySelector: () => inner, classList: { toggle() {} }, previousElementSibling: { tagName: "H2", textContent: "表" } };
  run("markScroll(__FW3)");
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/\.scroll-cap\.fit \.cap-pc, \.scroll-cap\.fit \.cap-sp, \.scroll-cap\.fit \.cap-x\{ display:none; \}/.test(css), "fit のときに案内の文を隠す CSS が無い");
  // 高さを制限しない枠の文も span（cap-x）に入れる（隠せるように）。件数の文はそのまま
  const h = run('scroll(table([{ t: "a" }, { t: "b" }], [[1, 2]]), "none")');
  ok(h.includes('全 <b>1</b> 行 × 2 列。<span class="cap-x">横に入り切らない分は枠の中で横にスクロールします（1列目は左に残ります）。</span>'), "高さを制限しない枠の案内が span（cap-x）でない: " + h.slice(0, 300));
  // 図の「点の吹き出し（マウスを重ねると出る title）」の案内は PC だけ（.pconly。タッチでは title が出ない）。
  // 2026-09-29 統合: 段2 M-11 の「本当の値は…図の下の「数字で読む」にあります」と合わせた。吹き出しの語は .pconly の中だけ、「数字で読む」は常に出す
  const fig = run('fig("c", "", \'<svg data-shift="1" style="--fw:600px"></svg>\', "")');
  ok(fig.includes('本当の値は<span class="pconly">点の吹き出しと、</span>図の下の「数字で読む」にあります。'),
    "ずらしの断りの吹き出しの案内が .pconly に入っていない: " + fig.slice(fig.indexOf("figlegend")));
  ok(!/吹き出し|マウス/.test(fig.replace(/<span class="pconly">[^<]*<\/span>/g, "")), "吹き出し・マウスの案内が .pconly の外にもある（タッチでは事実と違う）");
});

check("段1レビュー C: 900px 以下の側柱は、下へ送っている間は上へ引き上げて隠し、上へ戻したら出す（sideAwayOnScroll）。PC では何もしない", () => {
  const cls = new Set();
  const side = ctx.document.getElementById("cs-side");
  side.classList = { add: (c) => cls.add(c), remove: (c) => cls.delete(c), contains: (c) => cls.has(c) };
  side.offsetHeight = 125;
  let narrow = true;
  ctx.matchMedia = () => ({ matches: narrow });
  const at = (y) => { ctx.window.scrollY = y; run("sideAwayOnScroll()"); return cls.has("side-away"); };
  run("sideLastY = 0");
  ok(!at(0) && !at(100), "開いた直後（側柱の高さ + 80px より上）で隠している");
  ok(at(300), "300px 下へ送っても隠れない");
  ok(at(304), "4px の揺れで出てしまう（8px の遊びが無い）");
  ok(!at(280), "上へ戻しても出てこない");
  ok(at(600) && !at(0), "先頭へ戻しても出てこない");
  narrow = false;
  at(50); at(600);
  ok(!cls.has("side-away"), "PC（>900px）でも隠している");
  ok(html.includes('document.addEventListener("scroll", sideAwayOnScroll, { passive: true })'), "ページのスクロールを document で受けていない");
  const m = html.match(/@media \(max-width:900px\)\{([\s\S]*?)\n  \}/);
  ok(m && /\.side\.side-away\{ transform:translateY\(-100%\);/.test(m[1]) && /\.side\.side-away:focus-within\{ transform:none; \}/.test(m[1]),
    "900px 以下で側柱を引き上げる CSS（キーボードで入っているときは出す）が無い");
  ok(!/\.side\.side-away/.test(html.slice(0, html.indexOf("@media (max-width:900px){"))), "側柱を隠す CSS が PC にも効いている");
  delete ctx.matchMedia; delete side.classList; delete side.offsetHeight;
});

check("M-12: 強制カラーで「いま見ている項目」と「まずい KPI」に形と文字を足す", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  const m = css.match(/@media \(forced-colors:active\)\{([\s\S]*?)\n  \}/);
  ok(m, "forced-colors の @media が無い");
  ok(/\.side button\[aria-current="page"\], \.tabs button\.on\{ text-decoration:underline; outline:2px solid CanvasText;/.test(m[1]), "いま見ている項目に下線と枠が付かない");
  ok(/\.kpi\.is-bad \.big::after\{ content:" \\25B2";/.test(m[1]), "まずい KPI に ▲（色と印の意味と同じ印）が付かない");
});

// 🔴 2026-09-29 検証: M-12 の「帯の中の白文字（11.5px）が空色 --sora の上で 3.57:1」が手付かずだった。
// 空の上の文字だけ --sora-text にする。3 つの色のブロック（明るい・暗い（OS）・暗い（指定））で、塗りの .85 を掛けた空の上で 4.5:1 以上
check("M-12: 帯（svgStack）の中の文字は、空（--sora）の上でも 4.5:1 以上（空の上だけ --sora-text）", () => {
  const svg = run('svgStack({ w: 660, parts: [{ label: "空", v: 50, color: C.sora }, { label: "藍", v: 50, color: C.ai }] })');
  const texts = [...svg.matchAll(/<text [^>]*style="fill:([^;]+);font-size:11\.5px/g)].map((x) => x[1]);
  ok(texts.length === 2 && texts[0] === "var(--sora-text)" && texts[1] === "var(--panel)", "空の上の文字が --sora-text でない（藍の上は白のまま）: " + texts);
  const css = html.split("<style>")[1].split("</style>")[0];
  const [light, rest] = [css.split("@media (prefers-color-scheme:dark)")[0], css.split("@media (prefers-color-scheme:dark)")[1]];
  const blocks = [light, rest.split(':root[data-theme="dark"]')[0], rest.split(':root[data-theme="dark"]')[1].split("}")[0]];
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const lum = (c) => {
    const f = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); });
    return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2];
  };
  const val = (b, k) => (b.match(new RegExp("--" + k + ":(#[0-9a-f]{6}|var\\(--[a-z0-9-]+\\))")) || [])[1];
  blocks.forEach((b, i) => {
    const panel = val(b, "panel"), sora = val(b, "sora");
    let t = val(b, "sora-text");
    ok(panel && sora && t, "ブロック " + i + " に --sora-text が無い");
    if (/^var\(/.test(t)) t = val(b, t.slice(6, -1));
    const bg = hex(sora).map((v, j) => v * 0.85 + hex(panel)[j] * 0.15);   /* 帯の塗りは opacity .85 */
    const [a, z] = [lum(bg), lum(hex(t))];
    const r = (Math.max(a, z) + 0.05) / (Math.min(a, z) + 0.05);
    ok(r >= 4.5, "ブロック " + i + " の空の上の文字 " + r.toFixed(2) + ":1 が 4.5:1 未満");
  });
});

/* ================================================================ 段2 表と図（2026-09-28）: A / M-8 / M-11 / S-2 の残り */
check("段2 A: 今日動く先の名札は短い書き方（正式名は title と決まりごとの畳み）を詰めた枡（tag c）で 25.5em の列（wf）に並べる。案件そのものは正式な名札のまま", () => {
  const row = TD_ROW({ deal_id: "a1", name: "案件A", n_flags: 5, flags: ["NPSが4以下", "接触が30日以上空いている",
    "採用単価が同じ進捗帯の1.5倍以上", "MTGが90日以上途絶", "採用目標の半分に届いていない"] });
  const odd = TD_ROW({ deal_id: "a2", name: "案件B", n_flags: 1, flags: ["見たことのない名札"] });
  ctx.__A1 = { rows: [row, odd], expiring_this_week: [], started_this_week: [],
    meta: { n_hit: 2, n_shown: 2, n_active: 604, filter_rule: "", order_rule: "", new_deal_rule: "", mtg_gap: {} } };
  run("todayConsultant = '';");
  const h = run("renderToday(__A1)");
  const t = h.slice(h.indexOf('<table id="today-tbl"'), h.indexOf("</table>", h.indexOf('<table id="today-tbl"')));
  // 短い名札。正式な名札と分類の言葉を title に持つ abbr。色は図の分類（flagGroup）と同じ（MTG 途絶は緋）
  ok(/<td class="wf"><abbr class="tag c" style="border-color:var\(--hi\);color:var\(--hi\)" title="NPSが4以下（関係が危ない）">NPS4以下<\/abbr>/.test(t),
    "NPSが4以下 が短い名札（abbr.tag.c、title に正式名と分類）になっていない: " + t.slice(t.indexOf('<td class="wf">'), t.indexOf('<td class="wf">') + 200));
  ok(/<abbr class="tag c" style="border-color:var\(--hi\);color:var\(--hi\)" title="MTGが90日以上途絶（関係が危ない）">MTG90日以上途絶<\/abbr>/.test(t),
    "MTGが90日以上途絶 の枡が図の分類（緋・関係が危ない）と食い違う（前は紫だった）");
  ok(/title="接触が30日以上空いている（時間が迫っている）">接触30日以上空き<\/abbr>/.test(t) &&
     /title="採用目標の半分に届いていない（成果が出ていない）">採用目標の半分未満<\/abbr>/.test(t) &&
     /title="採用単価が同じ進捗帯の1.5倍以上（成果が出ていない）">採用単価1.5倍以上<\/abbr>/.test(t), "短い名札の書き方が違う");
  // 表に無い名札は略さず・落とさず、そのまま（span）。分類の言葉は title に
  ok(/<span class="tag c" style="[^"]*" title="成果が出ていない">見たことのない名札<\/span>/.test(t), "知らない名札を略している・落としている");
  ok(!t.includes('class="tag"'), "今日動く先に詰めていない枡（.tag）が残っている");
  // 決まりごとの畳みに略し方の一覧（短い ＝ 正式）。表に出ている名札だけ
  const rule = h.slice(h.indexOf('id="td-rule"'), h.indexOf("</details>", h.indexOf('id="td-rule"')));
  ok(rule.includes("名札の略し方") && rule.includes("NPS4以下 ＝ NPSが4以下") && rule.includes("MTG90日以上途絶 ＝ MTGが90日以上途絶"),
    "決まりごとの畳みに名札の略し方が無い");
  ok(!rule.includes("満了60日以内 ＝"), "表に出ていない名札まで略し方に並べている");
  ok(rule.includes("色が見分けられなくても読めます"), "色だけで伝えていないことが書かれていない");
  // 案件そのもの（BOARD_COLS）は正式な名札のまま。枡の色は今日動く先と同じ決め方（flagGroup）
  const b = run('boardTable(__A1.rows, { key: "n_flags", asc: false }, "board-tbl")');
  ok(b.includes('<td class="wl"><span class="tag" style="border-color:var(--hi);color:var(--hi)" title="関係が危ない">NPSが4以下</span>'),
    "案件そのものの名札が正式名のままでない、または枡の色が図の分類と違う");
  ok(b.includes('style="border-color:var(--hi);color:var(--hi)" title="関係が危ない">MTGが90日以上途絶</span>'),
    "案件そのものの MTG 途絶の枡が図の分類（緋）でない");
  ok(run('tags(["満了90日前でMTGが30日以上途絶"])').includes('border-color:var(--hi)'), "満了90日前でMTGが30日以上途絶 の枡が図の分類（緋。MTG を先に見る）と食い違う");
  ok(!b.includes("tag c") && !b.includes("<abbr"), "案件そのものまで短い名札にしている（15 列の表は横スクロールなので要らない）");
  // 名札の文の正本はサーバ。Rust が出せる名札は全部 FLAG_SHORT にある（新しい名札が正式名のまま長く出るのを見張る）
  const rs = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/routes.rs"), "utf-8");
  const md = fs.readFileSync(path.join(__dirname, "..", "src/handlers/cs_dashboard/mod.rs"), "utf-8");
  const labels = [...rs.matchAll(/flags\.push\("([^"]+)"\)/g)].map((m) => m[1])
    .concat([...md.matchAll(/MtgBand::(?:Critical|Red|Yellow) => "([^"]+)"/g)].map((m) => m[1]));
  ok(labels.length >= 10, "Rust から名札の文が拾えない（形が変わった？）: " + labels.length);
  const missing = labels.filter((l) => !run("FLAG_SHORT[" + JSON.stringify(l) + "]"));
  ok(!missing.length, "サーバの名札に短い書き方が無い: " + missing.join(" / "));
  // 短い書き方は条件の範囲（数と「以上・以下・以内」）を落とさない（2026-09-29 検証: 「満了前MTG30日途絶」で 90日前・以上が落ちていた）
  const short = run("FLAG_SHORT");
  for (const f of Object.keys(short)) {
    const nums = f.match(/[0-9.]+/g) || [];
    ok(JSON.stringify(short[f].match(/[0-9.]+/g) || []) === JSON.stringify(nums), "短い名札で数が落ちている・変わっている: " + f + " → " + short[f]);
    for (const w of ["以上", "以下", "以内"]) if (f.includes(w)) ok(short[f].includes(w), "短い名札で「" + w + "」が落ちている: " + f + " → " + short[f]);
  }
  // 決まりごとの文は、分類の言葉がどこにあるか（下の図）を言う。表の枡に分類の言葉があるとは言わない
  ok(!rule.includes("言葉でも書いているので") && rule.includes("分類の言葉は下の図の棒の右に書いています"), "決まりごとが表の枡に分類の言葉があるように読める");
  // CSS: 列（wf）と枡（tag c）の定義がある。文字は 11px を下回らない
  const css = html.split("<style>")[1].split("</style>")[0];
  ok(/td\.wf\{[^}]*white-space:normal;[^}]*max-width:25\.5em/.test(css), "td.wf の定義が無い（25.5em で折り返す。短い名札の範囲を落とさずに 1 行 54px に収める幅）");
  ok(/@media \(max-width:600px\)\{[^@]*td\.wf\{ max-width:26em; \}/.test(css), "600px 以下で名札の列を 26em に広げていない（枡の余白が広く 3 段に折れる）");
  ok(/\.tag\.c\{[^}]*padding:0 5px/.test(css) && /abbr\.tag\{[^}]*text-decoration:none/.test(css), ".tag.c / abbr.tag の定義が無い");
});

check("段2 M-8: 案件そのものは既定で上位 100 行＋「残りも出す」。同じ応答・同じ条件なら表を組み直さない（2 回描きでも 1 回）。絞り込みで変わる部分は #board-body", () => {
  const rows = [];
  for (let i = 0; i < 130; i++)
    rows.push({ deal_id: "b" + i, name: "案件" + i, consultant: "担当A", flags: [], n_flags: i % 5, amount: i * 1000, mtg_band: "recent" });
  ctx.__M8 = { rows, meta: { flag_counts: [], mtg_gap: { bands: [] }, order_rule: "並びの文", n_active: 130, today: "2026-09-18" } };
  const reset = 'boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "" }; boardShowAll = false; boardSort = { key: "n_flags", asc: false };';
  run('cur = { menu: "deal", view: "board" }; ' + reset);
  const tblOf = (h) => h.slice(h.indexOf('<table id="board-tbl"'), h.indexOf("</table>", h.indexOf('<table id="board-tbl"')));
  // 行は <tr> と、600px 以下で先頭 20 行より後ろに付く <tr data-rc="1">（段1レビュー B の rowCap。HTML には全行ある）の両方を数える（2026-09-29 統合）
  const nRows = (h) => (tblOf(h).split("<tbody>")[1].match(/<tr[\s>]/g) || []).length;
  const n0 = run("BOARD_TBL_BUILDS");
  const h = run("renderBoard(__M8)");
  ok(nRows(h) === 100, "既定で上位 100 行でない: " + nRows(h));
  ok(h.includes("<b>100</b> 行を出しています（全 130 件のうち）× 15 列"), "枠の案内が「100 行を出しています（全 130 件のうち）」でない");
  ok(/<div class="ctlbar board-more"><button type="button" class="act" id="board-more" data-all="1">残りの 30 行も出す<\/button>/.test(h),
    "表の下に「残りの 30 行も出す」が無い");
  // 名札の本数順（既定）で切るので、名札 0 本の行（案件0, 5, 10, …）は 100 行の外。上位は 4 本の行
  ok(tblOf(h).indexOf(">案件4</a>") >= 0 && tblOf(h).indexOf(">案件0</a>") < 0, "並べてから切っていない（名札 0 本の行が上位 100 行に入っている）");
  // 絞り込みで描き直す部分（件数の行 → 表 → 決まりごと → 図）は #board-body の中。絞り込みの欄はその外
  const at = h.indexOf('<div id="board-body">');
  ok(at >= 0, "#board-body が無い");
  ok(h.indexOf('id="board-filter"') < at, "絞り込みの欄が #board-body の中にある（描き直しで検索欄が消える）");
  const body = h.slice(at);
  ok(body.indexOf('id="board-count"') < body.indexOf('<table id="board-tbl"') && body.indexOf("</table>") < body.indexOf("この並びについて") &&
     body.indexOf("この並びについて") < body.indexOf("名札の分布"), "#board-body の中の順（件数 → 表 → 決まりごと → 図）が違う");
  // 同じ応答・同じ条件では組み直さない（paintFigs の 2 回描き）
  const h2 = run("renderBoard(__M8)");
  ok(run("BOARD_TBL_BUILDS") === n0 + 1, "同じ応答・同じ条件で表を組み直している: " + (run("BOARD_TBL_BUILDS") - n0) + " 回");
  ok(h2 === h, "同じ応答・同じ条件で出力が変わる");
  ok(run("boardApply(__M8.rows) === boardApply(__M8.rows)"), "boardApply が同じ条件で別の配列を返す（表の覚え書きが効かない）");
  // 並びを変えると組み直し、切る前に並べる（金額の大きい順なら先頭は 案件129）
  run('boardSort = { key: "amount", asc: false };');
  const h3 = run("renderBoard(__M8)");
  ok(run("BOARD_TBL_BUILDS") === n0 + 2, "並びを変えたのに表を組み直さない");
  const t3 = tblOf(h3).split("<tbody>")[1];
  ok(t3.indexOf(">案件129</a>") >= 0 && t3.indexOf(">案件129</a>") < t3.indexOf(">案件128</a>") && t3.indexOf(">案件0</a>") < 0, "並び替えが上位 100 行に効いていない");
  // 「残りも出す」で全件。案内は「全 130 行」、ボタンは戻す側に
  run("boardShowAll = true;");
  const h4 = run("renderBoard(__M8)");
  ok(nRows(h4) === 130, "残りも出すで全件にならない: " + nRows(h4));
  ok(h4.includes("全 <b>130</b> 行 × 15 列") && /id="board-more" data-all="0">上位 100 行だけにする</.test(h4), "全件のときの案内・戻すボタンが違う");
  // 絞り込みは切る前の件数で言い、100 行に満たなければ「残りも出す」を出さない
  run('boardShowAll = false; boardFilter.q = "案件12";');
  const h5 = run("renderBoard(__M8)");
  ok(h5.includes("<b>130 件中 11 件</b>を表示") && h5.includes("全 <b>11</b> 行 × 15 列") && !h5.includes('id="board-more"'),
    "絞り込み後の件数・案内が違う");
  // 別の応答（別の配列）なら組み直す（古い表を出さない）
  ctx.__M8b = { rows: rows.slice(0, 3), meta: ctx.__M8.meta };
  run(reset);
  const b1 = run("BOARD_TBL_BUILDS"); run("renderBoard(__M8b)");
  ok(run("BOARD_TBL_BUILDS") === b1 + 1 && nRows(run("renderBoard(__M8b)")) === 3, "別の応答で表を組み直さない");
  // 今日動く先の表（TODAY_COLS）は上限を渡していないので切らない
  run(reset);
  const td = run('boardTable(__M8.rows, { key: "n_flags", asc: false }, "today-tbl", TODAY_COLS)');
  ok((td.split("<tbody>")[1].match(/<tr>/g) || []).length === 130, "今日動く先の表まで切っている");
});

check("段2 M-11: 図の値を読み上げ用の一覧（ul.sr）と「数字で読む」の畳みで出す。折れ線は凡例の色から系列名を引いて 1 系列 1 行。省略したラベルの全文は混ぜない", () => {
  const srOf = (h) => (h.match(/<ul class="sr">([\s\S]*?)<\/ul>/) || ["", ""])[1];
  // 横棒: 1 本 1 行（吹き出しと同じ書き方。値の無い行は吹き出しが無いので出ない）
  // 右の注記（note。接触率の母数「33/125 か月」）は吹き出しにも一覧にも入る（率に母数を添える）
  const bar = run('fig("横棒の図", "", svgBarH({ rows: [{ label: "甲", v: 3, n: 10, note: "33/125 か月" }, { label: "乙", v: 1, tip: "補足" }, { label: "丙", v: null }], w: 600, fmt: F.int }), lg("box", C.ai, "件数"))');
  ok(srOf(bar) === "<li>甲: 3 (n=10) / 33/125 か月</li><li>乙: 1 ／ 補足</li>", "横棒の読み上げ用の一覧が違う: " + srOf(bar));
  ok(bar.includes('<details class="fold figsay"><summary>数字で読む（2 件）</summary><ul><li>甲: 3 (n=10) / 33/125 か月</li><li>乙: 1 ／ 補足</li></ul></details>'),
    "「数字で読む」の畳みが無い、または一覧が読み上げ用と違う");
  const fb = bar.indexOf('class="figbody"');
  ok(bar.indexOf('<ul class="sr">') > bar.indexOf("</div>", fb) && bar.indexOf("figsay") < bar.indexOf('class="figlegend"'), "一覧の位置が図の直後・凡例の前でない");
  // 折れ線: 系列ごとに 1 行、凡例の名前を頭に。未確定の点はそう書く（吹き出しと同じ）
  const line = run('fig("折れ線", "", svgLine({ x: ["25-07", "25-08"], series: [{ color: C.ai, pts: [{ v: 1 }, { v: 2, censored: true }] }, { color: C.midori, pts: [{ v: 5 }, { v: 6 }] }], yFmt: F.int }), lg("line", C.ai, "通話") + lg("line", C.midori, "接触（60秒超）"))');
  ok(srOf(line) === "<li>通話: 25-07: 1、25-08: 2 / 未確定</li><li>接触（60秒超）: 25-07: 5、25-08: 6</li>",
    "折れ線の一覧が系列ごとでない、または系列名が無い: " + srOf(line));
  // 同じ色に 2 つの名前がある凡例からは名前を引かない（間違った名前を付けない）
  const amb = run('fig("折れ線", "", svgLine({ x: ["a", "b"], series: [{ color: C.ai, pts: [{ v: 1 }, { v: 2 }] }], yFmt: F.int }), lg("line", C.ai, "甲") + lg("dash", C.ai, "乙"))');
  ok(srOf(amb) === "<li>a: 1、b: 2</li>", "同じ色に 2 つの名前があるのに系列名を付けている: " + srOf(amb));
  // 省略したラベルの全文（labText の title）は数字の一覧に混ぜない（「省略した名前の全文」の畳みが別にある）
  const longLab = run('fig("長い名前", "", svgBarH({ rows: [{ label: "とても長い長い長い長い長い長い長い長い長い長い名前", v: 2 }], w: 300, padL: 60 }), "")');
  ok(longLab.includes("省略した名前の全文（1 件）"), "前提が崩れている（ラベルが省略されていない）");
  ok(srOf(longLab) === "<li>とても長い長い長い長い長い長い長い長い長い長い名前: 2</li>", "省略したラベルの全文が数字の一覧に混ざる: " + srOf(longLab));
  // 帯: 区分ごとに件数と %。箱ひげ: 1 行に中央値・四分位・最小最大・n
  const st = run('fig("帯", "", svgStack({ parts: [{ label: "あ", v: 3, color: C.ai }, { label: "い", v: 1, color: C.ki }], w: 400 }), "")');
  ok(srOf(st) === "<li>あ: 3 (75.0%)</li><li>い: 1 (25.0%)</li>", "帯の一覧が違う: " + srOf(st));
  const bx = run('fig("箱", "", svgBoxH({ rows: [{ label: "経過日数", med: 10, q1: 5, q3: 20, min: 0, max: 30, n: 40, color: C.ai }], w: 500, xFmt: F.int }), lg("quart", C.ai, "四分位") + lg("line", C.ai, "中央値"))');
  ok(srOf(bx) === "<li>経過日数: 中央値 10 / 四分位 5–20 / 最小 0 最大 30 / n=40</li>", "箱ひげの一覧が違う: " + srOf(bx));
  // 同じ文が続く点（svgDots の群）は ×N にまとめる
  const dots = run('fig("点", "", svgDots({ total: 5, groups: [{ label: "注力", v: 3, color: C.ai }, { label: "それ以外", v: 2, color: C.ghost }] }), lg("dot", C.ai, "注力") + lg("dot", C.ghost, "それ以外"))');
  ok(srOf(dots) === "<li>注力（×3）</li><li>それ以外（×2）</li>", "点の図で同じ文の点を ×N にまとめていない、または名前を重ねている: " + srOf(dots));
  // 値の無い図（empty）には付けない
  ok(!run('fig("空", "", "<div class=\\"empty\\">x</div>", "")').includes("figsay"), "値の無い図に一覧を付けている");
  // 重なった線をずらした断りは「マウスを重ねる」を前提にしない（タッチでは吹き出しが出ない）
  const sh = run('fig("重なる", "", svgLine({ x: ["a", "b"], series: [{ color: C.ai, pts: [{ v: 1 }, { v: 1 }] }, { color: C.ki, pts: [{ v: 1 }, { v: 1 }] }], yFmt: F.int }), "")');
  ok(sh.includes("data-shift"), "前提が崩れている（重なった線がずらされていない）");
  ok(!sh.includes("マウスを重ねる") && sh.includes("図の下の「数字で読む」"), "ずらした断りが「マウスを重ねる」のまま");
  // ずらした断り（描き方の話）は吹き出しに残し、数字の一覧には入れない（2026-09-29 検証）
  ok(sh.includes("ずらして表示</title>") && !srOf(sh).includes("ずらして表示"), "ずらした断りが数字の一覧に入っている: " + srOf(sh));
  // 帯を縦に積む図: 名前は凡例でなく左のラベルにある。各行の頭に帯の名前（2026-09-29 検証: 「25-09: 19 || 25-09: 3」と名前が無かった）
  const lanes = run('fig("帯を縦に", "", svgStackLanes({ w: 600, months: ["25-08", "25-09"], lanes: [' +
    '{ label: "応募", color: C.ai, type: "line", pts: [{ v: 19 }, { v: 3 }], empty: false, noneLabel: "記録がありません", fillLabel: "応募" },' +
    '{ label: "面接", color: C.ki, type: "line", pts: [{ v: 2 }, { v: null }], empty: false, noneLabel: "記録がありません", fillLabel: "面接" },' +
    '{ label: "接触", color: C.ink2, type: "bars", pts: [{ v: null }, { v: null }], empty: true, noneLabel: "接触の記録がありません", fillLabel: "接触" }] }), "")');
  const lanesSay = srOf(lanes);
  ok(lanesSay.length > 0, "前提が崩れている（帯を縦に積む図に一覧が無い）");
  const lanesRows = [...lanesSay.matchAll(/<li>([\s\S]*?)<\/li>/g)].map((m) => m[1]);
  lanesRows.forEach((r) => ok(/^(応募|面接|接触)/.test(r), "帯を縦に積む図の一覧に帯の名前が無い行がある: " + r + " / 全体 " + lanesSay));
  ok(lanesRows.some((r) => r.indexOf("応募") === 0) && lanesRows.some((r) => r.indexOf("面接") === 0), "応募・面接の行が揃っていない: " + lanesSay);
  // 時間軸の図: 行の名前と、何の日付か（開始・満了）
  const tl = run('fig("連なり", "", svgTimeline({ w: 600, lanes: [{ label: "契約A", color: C.ai, marks: [{ d: "2024-06-10", t: "開始" }, { d: "2025-06-09", t: "満了" }] }, { label: "契約B", color: C.ai, marks: [{ d: "2025-06-10", t: "開始" }] }] }), "")');
  ok(srOf(tl) === "<li>契約A 開始 2024-06-10</li><li>契約A 満了 2025-06-09</li><li>契約B 開始 2025-06-10</li>", "時間軸の図の一覧に行の名前か日付の意味が無い: " + srOf(tl));
  ok(/marks: \[\{ d: d\.start, t: "開始" \}, \{ d: d\.expiration, t: "満了" \}\]/.test(html), "契約の連なりの印に開始・満了の言葉が無い");
  // 前の値（中空の棒）は直前の棒の行に括弧で添える（独立した「前回: 11」の行にしない）
  const v0 = run('fig("前回つき", "", svgBarH({ rows: [{ label: "初回", v: 58, v0: 47, n: 1650 }, { label: "継続1", v: 45, v0: 34 }], w: 600, fmt: F.int }), "")');
  ok(srOf(v0) === "<li>初回: 58 (n=1650)（前回: 47）</li><li>継続1: 45（前回: 34）</li>", "前の値が直前の棒の行に添えられていない: " + srOf(v0));
  // CSS
  const css = html.split("<style>")[1].split("</style>")[0];
  ok(/details\.fold\.figsay\{/.test(css) && /\.figsay ul\{/.test(css), "figsay の CSS が無い");
});

check("段2 S-2 の残り: 担当者の一覧・いま見るべき顧客・電話の KPI も、同じ画面に行き先がある札は button で、飛ぶ先の id が同じ画面にある。行き先の無い札は div のまま", () => {
  const jumps = (h) => [...h.matchAll(/<button type="button" class="kpi[^"]*" data-jump="([^"]+)"><span class="lbl">([^<]*)</g)].map((m) => [m[2], m[1]]);
  const hasId = (h, id) => h.includes(' id="' + id + '" tabindex="-1"');
  /* 2026-09-29 組み替え（09 の 6）: チームと担当の札は 担当者・名札2本以上（→ 表）・今週満了（div）・退職者のまま */
  const tm = run("renderTeam(teamOf(__D))");
  const tj = jumps(tm);
  /* 退職者のまま: 0 件のときは飛ばない（飛んだ先の表に印が無い。2026-09-29 検証）。1 件以上のときだけ表へ */
  ok(run("__D.meta.retired_deals") === 0, "前提が崩れている（fixture の退職者のままが 0 件でない）");
  ok(JSON.stringify(tj) === JSON.stringify([["担当者", "tm-tbl-h"], ["名札が 2 本以上の案件", "tm-tbl-h"]]),
    "チームと担当の札の行き先が違う（退職者のまま 0 件で飛んでいないか）: " + JSON.stringify(tj));
  ok(/<div class="kpi"><span class="lbl">退職者のまま<\/span>/.test(tm), "退職者のまま 0 件の札が div でない");
  ok(/<div class="kpi"><span class="lbl">今週満了<\/span>/.test(tm), "行き先の無い札（今週満了）を button にしている");
  tj.forEach(([, id]) => ok(hasId(tm, id), "チームと担当: 飛ぶ先 " + id + " が本文に無い"));
  const tmR = run("renderTeam(teamOf(Object.assign({}, __D, { meta: Object.assign({}, __D.meta, { retired_deals: 3, retired_people: 1 }) })))");
  ok(JSON.stringify(jumps(tmR).slice(-1)) === JSON.stringify([["退職者のまま", "tm-tbl-h"]]), "退職者のまま 3 件で表へ飛ばない: " + JSON.stringify(jumps(tmR)));
  ok(tm.indexOf('id="tm-tbl-h"') < tm.indexOf('<table id="team-tbl"'), "飛ぶ先の見出しが表の直前でない");
  const fo = run("renderFocus(__FO)");
  const fj = jumps(fo);
  ok(JSON.stringify(fj) === JSON.stringify([["定期NPS が 4 以下", "fc-nps-tbl-h"], ["採用単価が悪化した拠点", "fc-cpa-h"],
    ["MTG の記録がどちらも無い", "fc-mtg-h"], ["LTV 中央値", "fc-shape-h"]]), "いま見るべき顧客の札の行き先が違う: " + JSON.stringify(fj));
  fj.forEach(([, id]) => ok(hasId(fo, id), "いま見るべき顧客: 飛ぶ先 " + id + " が本文に無い"));
  ok(/<div class="kpi"><span class="lbl">NPS が入っている/.test(fo), "行き先の無い札（NPS が入っている）を button にしている");
  const ph = run("renderPhone(__PH)");
  const pj = jumps(ph);
  ok(JSON.stringify(pj) === JSON.stringify([["電話が1本も無い", "ph-reach-h"], ["接触が1本も無い", "ph-reach-h"],
    ["最後に話してから", "ph-days-h"], ["文字起こしがある", "ph-trans"]]), "電話の札の行き先が違う: " + JSON.stringify(pj));
  pj.forEach(([, id]) => ok(hasId(ph, id), "電話: 飛ぶ先 " + id + " が本文に無い"));
  ok(/<div class="kpi-target" id="ph-trans" tabindex="-1"><div class="note warn"><span class="hd">文字起こしの状況/.test(ph), "文字起こしの札の行き先（状況の枠）に id が無い");
  // 行き先の小さな文（.act）は札ごとに 1 つ。button の中に a を入れない（押せるものの入れ子）
  ok((tm.match(/<span class="act">/g) || []).length === 2 && (fo.match(/<span class="act">/g) || []).length === 4 && (ph.match(/<span class="act">/g) || []).length === 4,
    "行き先の小さな文（.act）の数が札の数と合わない");
  for (const h of [tm, fo, ph]) ok(!/<button[^>]*class="kpi[^>]*>(?:(?!<\/button>)[\s\S])*<a /.test(h), "button.kpi の中に a がある");
  const css = html.split("<style>")[1].split("</style>")[0];
  ok(/\.kpi-target:focus-visible\{/.test(css), "見出しでない行き先（.kpi-target）の focus-visible が無い");
});

/* ================================================================ 09 の組み替え（2026-09-29）: チームと担当・成果と継続 */
check("09 の 6 チームと担当: 画面の並び（担当者の一覧・担当の交代・担当者ごとの接触を1画面に）と、残す画面へのリンク", () => {
  const m = run("JSON.stringify(MENUS.map((x) => [x.key, x.views.map((v) => [v.key, v.label, v.path])]))");
  const M = JSON.parse(m);
  /* 段A の統合（2026-09-29）: メニューは 3 区切り（毎日 / 調べる / 月1・確かめる）。チームと担当は「調べる」、
     成果と継続は「月1・確かめる」の先頭。担当者ごとの案件は「毎日」の案件一覧の次、いま見るべき顧客・電話は「調べる」に残す（10 章⑤） */
  const views = (k) => (M.find((x) => x[0] === k) || [k, []])[1];
  const all = M.flatMap((x) => x[1]);
  ok(JSON.stringify(views("research").find((v) => v[0] === "team")) === JSON.stringify(["team", "チームと担当", "/api/consulting/team"]),
    "調べるの中に「チームと担当」（/api/consulting/team）が無い: " + JSON.stringify(views("research")));
  ok(JSON.stringify(views("deal").find((v) => v[0] === "byowner")) === JSON.stringify(["byowner", "担当者ごとの案件", "/api/consulting/deals"]),
    "毎日の中に「担当者ごとの案件」が無い");
  const st = views("monthly");
  ok(st.length && st[0][0] === "results" && st[0][1] === "成果と継続" && st[0][2] === "/api/consulting/results", "月1・確かめるの先頭が「成果と継続」でない: " + JSON.stringify(st[0]));
  ok(all.some((v) => v[0] === "focus") && all.some((v) => v[0] === "phone"), "残す画面（いま見るべき顧客・電話）が消えた（10 章⑤）");
  ok(!all.some((v) => ["renewal", "outcome", "rampup", "handover", "contact"].includes(v[0])), "まとめた画面がメニューに残っている");
  run('contactUnit = "month"; teamPick = "";');
  const h = run("renderTeam(teamOf(__D, { contact: __CT, handover: __HOC }))");
  /* 目次: 同じ画面の 3 節（行き先の id が本文にある）＋ 残す画面（担当者ごとの案件・電話）へのリンク */
  const toc = h.slice(h.indexOf('<nav class="toc"'), h.indexOf("</nav>"));
  const js = [...toc.matchAll(/data-jump="([^"]+)"/g)].map((x) => x[1]);
  ok(JSON.stringify(js) === JSON.stringify(["tm-tbl-h", "tm-ct-h", "tm-ho-h"]), "目次の行き先が違う: " + js.join(","));
  js.forEach((id) => ok(h.includes(' id="' + id + '" tabindex="-1"'), "目次の行き先 " + id + " が本文に無い"));
  ok(toc.includes('href="#deal/byowner"') && toc.includes('href="#research/phone"'), "目次に担当者ごとの案件・電話へのリンクが無い");
  ok(h.indexOf('<nav class="toc"') < h.indexOf('<div class="kpis">'), "目次が冒頭に無い");
  /* 仕事量の一覧で、評価ではない: 閉じた畳みでも見える summary に出る */
  const sum = h.slice(h.indexOf("<summary>何のための画面か"), h.indexOf("</summary>", h.indexOf("<summary>何のための画面か")));
  ok(sum.includes('<span class="keep">担当者の評価ではありません</span>'), "「評価ではありません」が畳みの中に隠れている: " + sum);
});

check("09 の 6 チームと担当: 担当者 × 状態の表は名前順が既定・母数（持ち件数）つき・人ごとの接触の2定義を名前で分けて並べる・人ごとの金額は出さない", () => {
  /* 開いた直後の並び（teamSort の初期値）と、入り直したときの既定（URL_STATE の reset）が、どちらも名前の昇順 */
  ok(run("JSON.stringify(teamSort)") === '{"key":"consultant","asc":true}', "開いた直後の並びが名前順でない: " + run("JSON.stringify(teamSort)"));
  run("URL_STATE.team.sort.reset()");
  ok(run("JSON.stringify(teamSort)") === '{"key":"consultant","asc":true}', "入り直したときの既定の並びが名前順でない");
  run('contactUnit = "month"; teamPick = "";');
  const D = teamOf(ctx.__D, { contact: ctx.__CT });
  /* 並びの既定: 接触率の低い順（前）ではなく、名前の順 */
  ctx.__TMS = D;
  const h = run("renderTeam(__TMS)");
  const names = [...h.slice(h.indexOf('<table id="team-tbl"'), h.indexOf("</table>", h.indexOf('<table id="team-tbl"')))
    .matchAll(/title="この担当の案件だけを「担当者ごとの案件」で見る">([^<]*)<\/a>/g)].map((x) => x[1]);
  ok(names.length === TEAM_ROWS.length, "表の行の数が担当者の数と違う: " + names.length);
  ok(JSON.stringify(names) === JSON.stringify(names.slice().sort((a, b) => a.localeCompare(b, "ja"))), "既定の並びが名前順でない: " + names.join(","));
  const head = h.slice(h.indexOf('<table id="team-tbl"'), h.indexOf("</thead>", h.indexOf('<table id="team-tbl"')));
  const cols = [...head.matchAll(/<button type="button" class="sort" data-k="([^"]+)">([^<]*)/g)].map((x) => [x[1], x[2]]);
  ok(JSON.stringify(cols.map((c) => c[0])) === JSON.stringify(["consultant", "n_active", "n_flags2", "mtg_critical", "no_contact", "expiring60",
    "nps_low", "contact_rate", "per_deal", "focus"]), "列が違う（2 つの接触の列は並べる）: " + cols.map((c) => c[0]).join(","));
  ok(cols.find((c) => c[0] === "contact_rate")[1] === "接触した月の割合" && cols.find((c) => c[0] === "per_deal")[1] === "1件あたりの接触回数",
    "人ごとの接触の2定義が名前で分かれていない: " + JSON.stringify(cols.slice(-2)));
  ok(textOf(h).includes("「1件あたりの接触回数」は確定した最新の月（2026-07）の値です"), "1件あたりがどの月の値かを書いていない（確定した最新の月 2026-07）");
  ok(!/ATV|atv_max/.test(h), "人ごとの金額（ATV 最大）が出ている（判断④ 金額は会社全体だけ）");
  ok(textOf(h).includes("人ごとの接触は、定義が2つあります（名前で区別しています）"), "2つの定義の違いを表の直下で言っていない");
});

check("09 の 6 チームと担当: 表の数は押すとその担当とその印で絞った一覧が開く。0 件はリンクにしない", () => {
  const D = teamOf(ctx.__D);
  D.status.rows[0] = Object.assign({}, D.status.rows[0], { n_flags2: 3, mtg_critical: 2, no_contact: 1, expiring60: 0, nps_low: 4 });
  ctx.__TMD = D;
  const h = run("renderTeam(__TMD)");
  const who = D.status.rows[0].consultant;
  const row = h.slice(h.indexOf(">" + who + "</a>"), h.indexOf("</tr>", h.indexOf(">" + who + "</a>")));
  /* 属性の中なので & は &amp;（esc） */
  const href = (v, p) => run("esc(hashFor(" + JSON.stringify(v) + ", " + JSON.stringify(p) + "))");
  ok(h.includes('href="' + href("byowner", { c: who }) + '"'), "担当者名が担当者ごとの案件（その担当）へのリンクでない");
  ok(row.includes('<a class="drill" href="' + href("today", { c: who }) + '"') && row.includes(">3</a>"), "名札2本以上が今日動く先（その担当）へのリンクでない: " + row);
  ok(row.includes('href="' + href("byowner", { band: "critical", c: who }) + '"'), "MTG 途絶がその担当・重大の帯へのリンクでない");
  ok(row.includes('href="' + href("byowner", { c: who, flag: "接触の記録が無い" }) + '"'), "接触の記録が無いが名札で絞ったリンクでない");
  ok(row.includes('href="' + href("byowner", { c: who, flag: "NPSが4以下" }) + '"'), "NPS 4以下が名札で絞ったリンクでない");
  ok(!row.includes(href("byowner", { c: who, flag: "満了まで60日以内" })), "0 件（満了まで60日以内）をリンクにしている");
});

check("09 の 6 チームと担当: 接触の推移は全体の線に選んだ担当を1本だけ重ね、選んだ担当と期間の単位は URL に載る", () => {
  run('contactUnit = "month"; teamPick = "";');
  const h0 = run("teamContact({ contact: __CT })");
  ok(!/stroke:var\(--murasaki\)/.test(h0), "担当を選んでいないのに人の線がある");
  ok(ctFigs(h0).length === 1, "図が 1 枚でない");
  run('teamPick = "担当B";');
  const h1 = run("teamContact({ contact: __CT })");
  ok(/stroke:var\(--murasaki\)/.test(h1) && /<option value="担当B" selected>/.test(h1), "選んだ担当の線・選択が出ていない");
  ok(run("JSON.stringify(stateParams('team'))") === JSON.stringify({ c: "担当B" }), "選んだ担当が URL に載らない: " + run("JSON.stringify(stateParams('team'))"));
  run('contactUnit = "week";');
  ok(JSON.parse(run("JSON.stringify(stateParams('team'))")).unit === "week", "週ごとが URL に載らない");
  run('contactUnit = "month"; teamPick = "";');
});

check("09 の 7 成果と継続: 答え（解約率）を先頭に、成果とリスク・顧客の図・立ち上がり・本部アプローチ・手を打つ先の表の順。目次と基準日の枠は1つ", () => {
  ctx.__RS = { meta: { today: "2026-09-18", exclude_right_censored: false }, population: { deals_option: 99 },
    renewal: ctx.__RN, outcome: ctx.__OUT, focus: ctx.__FO, rampup: ctx.__RU, headquarters: ctx.__HQ, phone: ctx.__PH };
  const h = run("renderResults(__RS)");
  const at = (s) => { const i = h.indexOf(s); ok(i >= 0, "「" + s + "」が無い"); return i; };
  const order = [at('id="rs-renewal"'), at('<span class="no">図</span>継続回数ごとの解約率'), at("月次の継続率（満了月ベース"), at('id="rs-outcome"'),
    at('id="rs-focus"'), at("定期NPS の散らばり"), at('id="rs-rampup"'), at('id="rs-hq"'), at('id="rs-act"')];
  ok(order.every((x, i) => !i || order[i - 1] < x), "節の並びが 09 の 7 と違う: " + order.join(","));
  const toc = h.slice(h.indexOf('<nav class="toc"'), h.indexOf("</nav>"));
  const js = [...toc.matchAll(/data-jump="([^"]+)"/g)].map((x) => x[1]);
  ok(JSON.stringify(js) === JSON.stringify(["rs-renewal", "rs-outcome", "rs-focus", "rs-rampup", "rs-hq", "rs-act"]), "目次の行き先が違う: " + js.join(","));
  js.forEach((id) => ok(h.includes(' id="' + id + '" tabindex="-1"'), "目次の行き先 " + id + " が本文に無い"));
  ok(h.indexOf('<nav class="toc"') < order[0], "目次が冒頭に無い");
  ok(!h.includes("満了月ごとの内訳"), "満了月ごとの内訳（約60行の表）が残っている（09 の 7）");
  ok((h.match(/<span class="hd">(読むときの注意|集計の基準日と件数|集計の基準日)<\/span>/g) || []).length === 1, "基準日の枠が 1 つでない（節ごとに出している）");
  /* まとめた画面への古いリンクを残さない（行き先の無い goLink は「集計」の文字だけになる） */
  ok(!/>集計<\/|集計 → 継続回数|集計 → 成果とリスク|集計 → 立ち上がり/.test(h), "無くなった画面へのリンク・名前が残っている");
});

check("段B 成果と継続: 手を打つ先の表3つは消し、案件一覧の見方（名札に寄せた4つ）へのリンクにする。節からも見方へ行ける", () => {
  /* 2026-09-29 段B（09 の 10 章②）: 表は定義を名札に寄せて案件一覧の見方に移した。成果と継続に表を残すと、定義の違う2つの一覧が並ぶ。
     見方の名前はサーバ（routes.rs build_results の meta.act_views）から来る */
  ctx.__RS.meta.nps_flag = "NPSが4以下";
  ctx.__RS.meta.act_views = [
    { key: "top", label: "最優先X", old: "前の出どころA" }, { key: "silent", label: "接触90日超X", old: "前の出どころB" },
    { key: "no_mtg", label: "初回MTG無しX", old: "前の出どころC" }, { key: "mtg_risk", label: "MTGリスク高X", old: null }];
  const h = run("renderResults(__RS)");
  const act = h.slice(h.indexOf('id="rs-act"'), h.indexOf("集計の基準日", h.indexOf('id="rs-act"')));
  ok(!/<table/.test(act) && !/<details class="fold"><summary>(最優先|電話で沈黙|契約開始から MTG)/.test(act), "手を打つ先の表が畳んで残っている");
  for (const lede of ['<div class="lede">金額が大きい順。', '<div class="lede">稼働中の初回契約 '])
    ok(!h.includes(lede), "手を打つ先の表が成果と継続のどこかに残っている: " + lede);
  ok(!h.includes('data-jump="rs-act"') || (h.match(/data-jump="rs-act"/g) || []).length === 1, "節から消えた畳みへの行き先が残っている");
  for (const [k, label] of [["top", "最優先X"], ["silent", "接触90日超X"], ["no_mtg", "初回MTG無しX"], ["mtg_risk", "MTGリスク高X"]]) {
    const href = run("esc(hashFor('board', { view: " + JSON.stringify(k) + " }))");
    ok(act.includes('href="' + href + '">見方「' + label + "」で絞った案件一覧</a>"), "見方 " + k + " へのリンクが無い");
  }
  ok(textOf(act).includes("前は 前の出どころA"), "前の定義の出どころを書いていない");
  /* 2026-09-29 段B 統合後の撮影: 見方へのリンクは文が長く、golink の nowrap のままだと 400px で横スクロールが出た（右端 454px）。
     1 本で 1 行を占めるリンクは折り返す印（golink wrap）を付け、その印の CSS が nowrap を打ち消していること */
  const actLinks = act.match(/<a class="[^"]*" href="[^"]*view=[^"]*">見方「/g) || [];
  ok(actLinks.length === 4 && actLinks.every((a) => a.startsWith('<a class="golink wrap"')), "見方へのリンクが折り返せない（400px で横にはみ出す）");
  ok(/a\.golink\.wrap\{[^}]*white-space:normal/.test(html), "golink wrap の CSS が無い（nowrap のまま）");
  ok(textOf(act).includes("前の定義から外れた案件は、見方を押すと件数と一覧で出ます"), "外れた案件の行き先を言っていない（黙って消す）");
  /* 節（成果とリスク・立ち上がり）の表があった場所から、見方へ行ける */
  const body = h.slice(0, h.indexOf('id="rs-act"'));
  ok(body.includes('href="' + run("esc(hashFor('board', { view: 'top' }))") + '"') &&
     body.includes('href="' + run("esc(hashFor('board', { view: 'no_mtg' }))") + '"'), "節から見方へのリンクが無い");
});

check("段B 案件一覧: 見方のボタン（押している見方は aria-pressed）、定義と母数、外れた件数と外れた案件の表（案件の詳細へ）", () => {
  const V = [{ key: "top", label: "最優先X", rule: "定義T", n: 1 }, { key: "silent", label: "接触90日超X", rule: "定義S", n: 2 },
             { key: "no_mtg", label: "初回MTG無しX", rule: "定義N", n: 0 }, { key: "mtg_risk", label: "MTGリスク高X", rule: "定義R", n: 0 }];
  const diff = [
    { key: "top", n: 1, old: { where: "前の出どころA", n: 1, kept: 1, dropped: 0, added: 0, dropped_rows: [] } },
    { key: "silent", n: 2, old: { where: "前の出どころB", n: 3, kept: 1, dropped: 2, added: 1,
      dropped_rows: [{ deal_id: "d8", name: "外れた案件8", consultant: "田中", reason: "最後の接触から 20日（90日以内）" },
                     { deal_id: "d9", name: "外れた案件9", consultant: "佐藤", reason: "契約開始前（接触の名札を立てない）" }] } },
    { key: "no_mtg", n: 0, old: { where: "前の出どころC", n: 0, kept: 0, dropped: 0, added: 0, dropped_rows: [] } },
    { key: "mtg_risk", n: 0, old: null }];
  ctx.__BV = { meta: Object.assign({}, ctx.__BD.meta, { act_views: V, act_view_diff: diff }),
    rows: [
      { deal_id: "d1", name: "案件1", consultant: "田中", flags: ["NPSが4以下"], views: ["top", "silent"], days_left: 10 },
      { deal_id: "d2", name: "案件2", consultant: "田中", flags: [], views: ["silent"], days_left: 200 },
      { deal_id: "d3", name: "案件3", consultant: "佐藤", flags: [], views: [], days_left: 20 }] };
  const reset = 'boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "", view: "" }; boardShowAll = false;';
  run('cur = { menu: "deal", view: "board" }; ' + reset);
  try {
    const h0 = run("renderBoard(__BV)");
    const bar = h0.slice(h0.indexOf('id="board-views"'), h0.indexOf("</div>", h0.indexOf('id="board-views"')));
    ok(bar.length > 0, "見方のボタンの列が無い");
    const btns = [...bar.matchAll(/data-view="([^"]+)" aria-pressed="(true|false)">([^<]*)<\/button>/g)];
    ok(JSON.stringify(btns.map((b) => b[1])) === JSON.stringify(["top", "silent", "no_mtg", "mtg_risk"]), "見方のボタンが4つでない: " + btns.map((b) => b[1]));
    ok(btns[1][3] === "接触90日超X（2 件）", "ボタンに件数が無い: " + btns[1][3]);
    ok(btns.every((b) => b[2] === "false"), "何も押していないのに押された見方がある");
    ok(!h0.includes("定義を名札に揃えたため"), "見方を押していないのに外れた件数の文が出ている");

    run('boardFilter.view = "silent";');
    ok(run("boardApply(__BV.rows).map((r) => r.deal_id).join()") === "d1,d2", "見方で行が絞られない");
    ok(run("boardFilterOn()") === true && run("boardFilterWords(__BV)").includes("見方 接触90日超X"), "件数の行に見方が出ない（鍵を出している）");
    const h = run("renderBoard(__BV)");
    ok(/data-view="silent" aria-pressed="true"/.test(h), "押している見方が aria-pressed になっていない（色だけで伝えている）");
    const tx = textOf(h);
    ok(tx.includes("定義S") && tx.includes("稼働中 3 件のうち 2 件です"), "定義の1行と母数が出ない");
    ok(tx.includes("定義を名札に揃えたため、以前の前の出どころB 3 件のうち 2 件は外れました。"), "外れた件数の文が無い: " + tx.slice(0, 400));
    ok(tx.includes("新しく入った案件が 1 件あります"), "新しく入った件数を書いていない");
    const fold = h.slice(h.indexOf('id="board-view-dropped"'));
    ok(fold.includes("外れた 2 件を確かめる"), "外れた案件の畳みが無い");
    ok(fold.includes('href="#deal/detail?id=d8"') && fold.includes('href="#deal/detail?id=d9"'), "外れた案件が案件の詳細へのリンクでない");
    ok(fold.includes("契約開始前（接触の名札を立てない）"), "外れた理由が無い");
    /* URL に載る（?view=）。hashFor の形 */
    ok(run("JSON.stringify(stateParams('board'))") === JSON.stringify({ view: "silent" }), "見方が URL に載らない: " + run("JSON.stringify(stateParams('board'))"));
    ok(run("hashFor('board', { view: 'silent' })") === "#deal/board?view=silent", "hashFor の形が違う");

    /* 前に一覧が無い見方: 外れた件数は出さず、前は一覧が無かったと書く */
    run('boardFilter.view = "mtg_risk";');
    const hr = textOf(run("renderBoard(__BV)"));
    ok(hr.includes("前は一覧が無かった見方です") && !hr.includes("は外れました"), "前に一覧が無い見方で外れた件数を作っている");
    /* 🔴 MTG でリスク高が 0 件のとき、判定のある案件の数を母数として添える（「稼働中 3 件のうち 0 件」だけだと、判定が無いのか
       「高」が無いのかが分からない。2026-09-29 横断レビュー）。判定のある行（mtg_risk）は 3 件中 2 件 */
    ctx.__BV.rows[0].mtg_risk = "中"; ctx.__BV.rows[2].mtg_risk = "低";
    const hm = textOf(run("renderBoard(__BV)"));
    delete ctx.__BV.rows[0].mtg_risk; delete ctx.__BV.rows[2].mtg_risk;
    ok(hm.includes("稼働中 3 件のうち、リスク判定のある案件は 2 件、そのうちいちばん新しい判定が「高」の案件は 0 件です"),
      "MTG でリスク高で、判定のある案件の数（母数）が出ない: " + hm.slice(0, 400));
    /* 他の見方は判定の数を添えない（判定と関係の無い見方に母数を混ぜない） */
    run('boardFilter.view = "silent";');
    ok(!textOf(run("renderBoard(__BV)")).includes("リスク判定のある案件は"), "MTG と関係の無い見方にリスク判定の数を添えている");
    /* 外れた件数 0 のときは 0 件と書き、空の畳みを出さない */
    run('boardFilter.view = "top";');
    const ht = run("renderBoard(__BV)");
    ok(textOf(ht).includes("1 件のうち 0 件は外れました") && !ht.includes('id="board-view-dropped"'), "外れた 0 件の扱いが違う");
    /* 知らない見方（貼られた URL）は黙って 0 件にしない */
    run('boardFilter.view = "zzz";');
    const hz = textOf(run("renderBoard(__BV)"));
    ok(hz.includes("この見方は見つかりません") && run("boardFilterWords(__BV)").includes("見方 （見つからない印）"), "知らない見方を黙って 0 件にしている");
  } finally {
    run(reset + ' cur = { menu: "deal", view: "today" };');
  }
});

check("段B 検証の指摘: 担当者ごとの案件でも見方の定義と外れた件数を出し、外れた案件はその担当者の分に絞る", () => {
  /* 2026-09-29 検証: byowner は ?view= で絞れるのに、件数の行に「見方 X」と出るだけで外れた案件を知る手段が無かった */
  const reset = 'boardFilter = { consultant: "", flag: "", expiry: "", q: "", band: "", view: "" }; boardShowAll = false;';
  try {
    run('cur = { menu: "consultant", view: "byowner" }; ' + reset + ' boardFilter.consultant = "田中"; boardFilter.view = "silent";');
    const h = run("renderBoard(__BV)");
    const tx = textOf(h);
    ok(!h.includes('id="board-views"'), "担当者ごとの案件に見方のボタンを置いている");
    ok(tx.includes("定義S") && tx.includes("稼働中（全担当） 3 件のうち 2 件です"), "担当者ごとの案件で定義の1行と母数（全担当）が出ない");
    ok(tx.includes("定義を名札に揃えたため、以前の前の出どころB 3 件のうち 2 件は外れました。"), "担当者ごとの案件で外れた件数の文が無い");
    const fold = h.slice(h.indexOf('id="board-view-dropped"'));
    ok(fold.includes("外れた 2 件（全担当）のうち、この担当者の 1 件を確かめる"), "外れた案件をその担当者の分に絞っていない");
    ok(fold.includes('href="#deal/detail?id=d8"') && !fold.includes("d9"), "その担当者の外れた案件だけを出していない");
    /* その担当者の分が 0 件なら、0 件と書いて空の畳みを出さない */
    run('boardFilter.consultant = "鈴木";');
    const h0 = run("renderBoard(__BV)");
    ok(textOf(h0).includes("外れた 2 件（全担当）のうち、この担当者の案件はありません。") && !h0.includes('id="board-view-dropped"'),
      "その担当者の外れた案件が 0 件の扱いが違う");
  } finally {
    run(reset + ' cur = { menu: "deal", view: "today" };');
  }
});

check("段B 検証の指摘: 立ち上がりの節は、見方と件数が違う主な理由（立ち上がり期は入らない）を書く", () => {
  /* 2026-09-29 検証: 「メール由来の実施日も見るので」だけで、fixture で外れた 93 件のうち 38 件の理由（立ち上がり期）を書いていなかった */
  const t = textOf(run("renderRampup(__RU, 'rs-rampup')"));
  ok(t.includes("見方には立ち上がり期（契約開始30日以内）の案件が入りません"), "立ち上がり期が見方に入らないことを書いていない: " + t.slice(-300));
  ok(t.includes("外れた案件を確かめる") && t.includes("メール由来の実施日"), "外れた案件の行き先か、メール由来の理由が無い");
  ok(t.indexOf("立ち上がり期（契約開始30日以内）の案件が入りません") < t.indexOf("メール由来の実施日"), "主な理由（立ち上がり期）を先に書いていない");
});

check("段B 検証の指摘: 電話の「沈黙している取引」は、案件一覧の見方と定義が違うことと行き先を書く", () => {
  /* 2026-09-29 検証: fixture で電話の表 107 件・見方 78 件。定義の違う2つの一覧が説明なしに並んでいた */
  const h = run("renderPhone(__PH)");
  const i = h.indexOf("沈黙している取引（");
  const tail = h.slice(i);
  ok(i >= 0 && tail.includes("案件一覧の見方とは定義が違います"), "電話の沈黙の表に見方との違いの枠が無い");
  ok(tail.includes('href="' + run('esc(hashFor("board", { view: "silent" }))') + '"'), "見方（接触90日超）への行き先が無い");
  ok(textOf(tail).includes("同じ案件の集まりではないので、件数は合いません"), "件数が合わないことを書いていない");
});

check("09 の 7 成果と継続: 定期NPS 4以下は名札と同じ集合なので「別の数え方・段B で揃える」に入れず、名札で絞った案件一覧へ", () => {
  /* 2026-09-29 検証: NPS 4以下の表を段B の仮置きに入れて「名札とは別の数え方」と書いていた（focus.nps_low 41 件と名札 41 件は deal_id 41/41 一致。
     同じ集合であることは tests.rs results_nps_low_is_the_same_set_as_the_deal_flag が見張る） */
  ctx.__RS.meta.nps_flag = "NPSが4以下";
  const h = run("renderResults(__RS)");
  const act = h.slice(h.indexOf('id="rs-act"'));
  ok(!act.includes('<div class="lede">NPS の低い順、同じなら金額の大きい順。') && !/<summary>定期NPS/.test(act),
    "定期NPS 4以下の表を段B の仮置き（名札とは別の数え方）に入れている");
  ok(!h.includes('<div class="lede">NPS の低い順、同じなら金額の大きい順。'), "定期NPS 4以下の表が成果と継続の中に残っている");
  const href = run('esc(hashFor("board", { flag: "NPSが4以下" }))');
  ok(act.includes('href="' + href + '"') && textOf(act).includes("名札「NPSが4以下」と同じ集合"),
    "定期NPS 4以下が名札で絞った案件一覧へのリンクになっていない（黙って消した）");
  ok(act.includes('href="#research/focus"'), "いま見るべき顧客の表への行き先が無い");
  ok(textOf(act).includes("定期NPS が 4 以下の顧客（" + run("__FO.nps_low.n") + " 件）"), "件数を書いていない");
});

check("09 の 7 成果と継続: 満了月ごとの約60行の表の代わりの「数字で読む」は満了月ごとに 1 行（1 行に全部つながない）", () => {
  const h = run("renderRenewal(__RN)");
  const f = h.slice(h.indexOf("月次の継続率（満了月ベース"));
  const fg = f.slice(0, f.indexOf("</figure>"));
  const sum = (fg.match(/<details class="fold figsay"><summary>([^<]*)<\/summary><ul>([\s\S]*?)<\/ul><\/details>/) || []);
  const n = run("__RN.monthly_retention.rows.filter((r, i, a) => a.slice(i).some((x) => x.denom)).length");
  ok(sum.length && sum[1] === "数字で読む（満了月ごと " + n + " か月）", "「数字で読む」の見出しが満了月ごとの数でない: " + sum[1]);
  const li = sum.length ? [...sum[2].matchAll(/<li>([^<]*)<\/li>/g)].map((x) => x[1]) : [];
  ok(li.length === n && li.every((x) => /^\d{4}-\d{2}（n=\d+）: /.test(x)), "満了月ごとに 1 行になっていない: " + li.length + " / " + li.slice(0, 2).join(" | "));
  ok(li.some((x) => x.startsWith("2027-01（n=1）: 0.0% / ") && x.includes("点は打っていません")), "点を打たない月（n<30）が一覧に無い");
  ok(li.some((x) => x.startsWith("2026-11（n=0）: 率なし")), "決着 0 件の月が一覧に無い");
  /* 読み上げ（ul.sr）も同じ 1 行ずつで、図の 1 本の線を 1 項目にまとめた一覧は残さない */
  const sr = (fg.match(/<ul class="sr">([\s\S]*?)<\/ul>/) || [])[1] || "";
  ok((sr.match(/<li>/g) || []).length === n, "読み上げの一覧が満了月ごとでない");
  ok((fg.match(/class="fold figsay"/g) || []).length === 1 && (fg.match(/<ul class="sr">/g) || []).length === 1, "「数字で読む」が二重に出ている");
});

check("09 の 7 成果と継続: 札（月次の継続率は結果がそろった月で分母つき・採用目標に届いた件数は母数つき）。money の無い応答では金額の札を出さない", () => {
  const h = run("renderResults(__RS)");
  const k = h.slice(h.indexOf('<div class="kpis">'), h.indexOf('id="rs-renewal"'));
  ok(h.indexOf('<nav class="toc"') < h.indexOf('<div class="kpis">') && h.indexOf('<div class="kpis">') < h.indexOf('id="rs-renewal"'),
    "札が目次と解約率のあいだに無い");
  /* __RN（上の図の部品の見張り）では 2026-09 が結果がそろって n=60 の最新の月（2026-10 は n=8・2027-01 は n=1） */
  ok(textOf(k).includes("月次の継続率（2026-09 に満了）") && textOf(k).includes("n=60（継続 30・解約 20・充足 10）"),
    "月次の継続率の札が結果のそろった最新の月・分母つきでない: " + textOf(k));
  ok(textOf(k).includes("採用目標に届いた稼働中の契約") && /目標と承諾数が入っている 345 件のうち/.test(textOf(k)),
    "採用目標の札に母数が無い: " + textOf(k));
  ok(!/(^|[^0-9.])0\.0%/.test(textOf(k)), "採用目標の札に中央値 0.0% を出している（達成できていないと読める）");
  /* 段B で金額の札を足した（下の見張り）。money の無い応答（古いキャッシュ）では、数えていない金額を出さない */
  ok(!/金額/.test(textOf(k)), "money が無いのに金額の札が出ている");
});

check("09 の 6 チームと担当: 定義1 は画面の中で 1 つの名前（接触した月の割合）。決まりごとは表のすぐ下", () => {
  run('contactUnit = "month"; teamPick = "";');
  const D = JSON.parse(JSON.stringify(teamOf(ctx.__D, { contact: ctx.__CT, handover: ctx.__HOC })));
  D.consultants.contact_rule = "接触 ＝ MTG または60秒超の通話（メールは数えない）。接触率 ＝ 接触があった月 ÷（案件 × 経過月）。件数ではなく率で見るのは、件数だと…";
  D.contact = Object.assign({}, D.contact, { attach_rule: "…付け直して数えています（担当者の一覧の接触率は付け直していないので、数が違います）。" });
  ctx.__TMN = D;
  const h = run("renderTeam(__TMN)");
  const tx = textOf(h);
  ok(!/接触率 ＝/.test(tx), "「接触率 ＝」が画面に残っている（表の列は「接触した月の割合」）");
  ok(!tx.includes("担当者の一覧の接触率は"), "接触の推移の決まりごとが「担当者の一覧の接触率」のまま");
  ok(tx.includes("接触した月の割合（前の担当者の一覧の「接触率」）＝ 接触があった月"), "何のための画面かで定義1 を列と同じ名前にしていない");
  const at = (s) => h.indexOf(s);
  ok(at("</table>") < at("担当者 × 状態の決まりごと") && at("担当者 × 状態の決まりごと") < at('id="tm-ct-h"'),
    "担当者 × 状態の決まりごとが表のすぐ下に無い（接触の推移・担当の交代の後ろにある）");
});

/* ================================================================ 段B（2026-09-29）: 満了と継続・金額の札（09 の 5・7、10 章④）
   数字は fixture（基準日 2026-09-18）の /api/consulting/renewal-pipe と results.money の実測（money.rs）:
   今月〜再来月 62 / 105 / 118 件（計 285 件・25,859 万）・先月以前に満了日を過ぎてまだ稼働中 10 件・再来月より先 309 件・満了日なし 0 件、
   稼働中 604 件の合計 67,761 万（金額が空 2 件）。ステージと一覧の行は数を減らして抜き出した */
ctx.__RP = {
  meta: { today: "2026-09-18", n_active: 604, window: ["2026-09", "2026-10", "2026-11"],
    amount_basis: "金額は HubSpot の取引の金額（amount）で、契約期間全体の額です（月額ではありません。期間の長い契約ほど大きくなります）。ステージの確度は掛けていません。人ごとの金額は出していません",
    not_counted: "※ 予測ではありません。満了日と今のステージをそのまま数えています。ステージの確度を掛けた見込みの金額は出していません" },
  months: [
    { month: "2026-09", n: 62, amount: 53754000, amount_n: 62, amount_missing: 0,
      stages: [{ label: "求人出稿完了", n: 5 }, { label: "Cヨミ：50％（担当者の継続意思あり）", n: 5 }] },
    { month: "2026-10", n: 105, amount: 93088800, amount_n: 105, amount_missing: 0,
      stages: [{ label: "求人出稿完了", n: 29 }, { label: "Cヨミ：50％（担当者の継続意思あり）", n: 4 }] },
    { month: "2026-11", n: 118, amount: 111750000, amount_n: 118, amount_missing: 0,
      stages: [{ label: "求人出稿完了", n: 45 }, { label: "Cヨミ：50％（担当者の継続意思あり）", n: 3 }] },
  ],
  stages: ["求人出稿完了", "Cヨミ：50％（担当者の継続意思あり）"],
  window_total: { n: 285, amount: 258592800, amount_n: 285, amount_missing: 0 },
  active_total: { n: 604, amount: 677609694, amount_n: 602, amount_missing: 2 },
  overdue_before: { sum: { n: 10, amount: 1271903, amount_n: 10, amount_missing: 0 },
    rows: [{ deal_id: "9", name: "過ぎた案件", expiry: "2026-07-31", days_left: -49, stage: "定期2", amount: 300000, consultant: "担当C", flags: [] }] },
  later: 309, no_expiry: 0,
  rows: [
    { deal_id: "1", name: "案件あ", expiry: "2026-09-10", days_left: -8, stage: "Cヨミ：50％（担当者の継続意思あり）", amount: 900000, consultant: "担当A", flags: ["満了まで60日以内"] },
    { deal_id: "2", name: "案件い", expiry: "2026-09-30", days_left: 12, stage: "求人出稿完了", amount: 450000, consultant: "担当B", flags: [] },
    { deal_id: "3", name: "案件う", expiry: "2026-10-05", days_left: 17, stage: "", amount: null, consultant: "担当A", flags: [] },
  ],
};

check("段B 満了と継続: メニューは /api/consulting/renewal-pipe を読み、仮のつなぎ（renewalPipeInterim）は残っていない", () => {
  const M = JSON.parse(run("JSON.stringify(MENUS.map((x) => [x.key, x.views.map((v) => [v.key, v.label, v.path])]))"));
  const v = M.find((x) => x[0] === "research")[1].find((x) => x[0] === "renewalpipe");
  ok(v && v[1] === "満了と継続" && v[2] === "/api/consulting/renewal-pipe", "満了と継続の API が違う: " + JSON.stringify(v));
  ok(run("typeof renewalPipeInterim") === "undefined", "仮のつなぎが残っている");
});

check("段B 満了と継続: 満了月ごとの件数・金額（空の件数つき）と、母数の内訳（どこにも入らない契約が無い）を畳まずに出す", () => {
  const h = run("renderRenewalPipe(__RP)");
  const tx = textOf(h);
  const k = h.slice(h.indexOf('<div class="kpis">'), h.indexOf('<div class="note'));
  const lbls = [...k.matchAll(/<span class="lbl">([^<]*)<\/span>/g)].map((m) => m[1]);
  ok(JSON.stringify(lbls) === JSON.stringify(["今月 2026-09 に満了", "来月 2026-10 に満了", "再来月 2026-11 に満了", "先月以前に満了日を過ぎて、まだ稼働中"]),
    "札が今月・来月・再来月・先月以前でない: " + lbls.join(" / "));
  ok(textOf(k).includes("5,375万（金額が空 0 件）。うち満了日を過ぎてまだ稼働中 1 件"), "今月の札に金額・空の件数・過ぎた件数が無い: " + textOf(k));
  /* 母数の箱は note（畳まない）。4 つの行き先の件数を全部書く */
  const i = h.indexOf("何を数えているか（母数）");
  ok(i >= 0 && !/<details[^>]*>(?:(?!<\/details>)[\s\S])*$/.test(h.slice(0, i)), "母数の箱が無い、または畳まれている");
  const bt = textOf(h.slice(i, h.indexOf("</div>", i)));
  for (const s of ["稼働中 604 件", "今月〜再来月に満了 285 件", "先月以前に満了日を過ぎてまだ稼働中 10 件", "再来月より先に満了 309 件", "満了日が入っていない 0 件",
    "契約期間全体の額です（月額ではありません", "確度は掛けていません"])
    ok(bt.includes(s), "母数の箱に「" + s + "」が無い: " + bt);
  ok((tx.match(/予測ではありません/g) || []).length === 1, "予測ではない、を書いていない、または 2 回書いている（頭と末尾）");
});

check("案件一覧の絞り込み: 部品の名前（名札・MTG途絶）を折らず、縮めるのは select。長い文のチェックボックスには掛けない", () => {
  /* 🔴 2026-09-29 横断レビュー（400px の案件一覧）: 「名札」「MTG途絶」の字が縦に折れていた（Playwright 実測: 名前の文字の行 2・枠 46px）。
     操作列の label.act 全部に掛けると、成果と継続の「右側打ち切り」のチェックボックスの文が折れず 400px で 438px にはみ出したので、
     #board-filter に限る。見た目そのものは Playwright で見る（ここは規則が消えたこと・広がったことを捕まえる） */
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/#board-filter label\.act\{[^}]*white-space:nowrap/.test(css), "案件一覧の絞り込みの名前が折れる（white-space:nowrap が無い）");
  ok(/#board-filter label\.act > select\{[^}]*min-width:0/.test(css), "案件一覧の絞り込みの select が縮まない（min-width:0 が無い）");
  ok(!/(^|[\s,}])\.ctlbar label\.act\{[^}]*white-space:nowrap/.test(css), "操作列の label.act 全部を折れなくしている（長い文のチェックボックスがはみ出す）");
});

check("段B 満了と継続: ステージは件数だけ（確度を掛けない・名前の％は掛けていないと書く）。人ごとの金額を出さない", () => {
  const h = run("renderRenewalPipe(__RP)");
  const st = h.slice(h.indexOf('<table id="rp-stage-tbl"'), h.indexOf("</table>", h.indexOf('<table id="rp-stage-tbl"')));
  const rows = [...st.matchAll(/<tr>([\s\S]*?)<\/tr>/g)].map((m) => [...m[1].matchAll(/<t[hd][^>]*>([\s\S]*?)<\/t[hd]>/g)].map((c) => textOf(c[1]).trim()));
  ok(rows.length === 1 + 2, "ステージの表の行が 見出し＋2 ステージ でない（計を表の行に入れると枠の案内が 1 行多く数える）: " + rows.length);
  ok(JSON.stringify(rows[1]) === JSON.stringify(["求人出稿完了", "5", "29", "45", "79"]), "ステージの件数が月ごとに並んでいない: " + rows[1]);
  /* 🔴 2026-09-29 検証: 計の行が tbody にあり、枠の案内が「全 19 行」（ステージは 18 種）と出ていた。案内はステージの数と同じ */
  const sti = h.indexOf('id="rp-stage"');
  const cap = textOf(h.slice(h.indexOf('<div class="scroll-cap">', sti), h.indexOf('<table id="rp-stage-tbl"')));
  ok(cap.replace(/\s+/g, "").includes("全2行×5列"), "ステージの表の枠の案内がステージの数（2）でない: " + cap);
  const after = textOf(h.slice(h.indexOf("</table>", h.indexOf('<table id="rp-stage-tbl"')), h.indexOf("件数だけを数えています")));
  ok(after.replace(/\s+/g, "").includes("計:今月2026-0962件・来月2026-10105件・再来月2026-11118件、3か月で285件（ステージ2種）"),
    "表の下の計が月の件数と合わない: " + after);
  ok(!/万/.test(textOf(st)), "ステージの表に金額（確度を掛けた見込みに読める）が入っている");
  ok(textOf(h).includes("件数にも金額にも掛けていません"), "ステージ名の％を掛けていないと書いていない");
  /* 担当ごとに金額を足した数を出さない。一覧の金額は取引 1 件の値だけ */
  const lt = (i) => h.slice(h.indexOf('<table id="rp-tbl-' + i + '"'), h.indexOf("</table>", h.indexOf('<table id="rp-tbl-' + i + '"')));
  ok((lt(0).match(/<tr>/g) || []).length === 1 + 2 && (lt(1).match(/<tr>/g) || []).length === 1 + 1, "月ごとの一覧の行数が違う");
  /* 担当の名前が出てよいのは取引 1 件ずつの一覧（rp-tbl-N・rp-over-tbl）の中だけ。札・母数の箱・ステージの表（足した数）には出さない */
  const outside = h.replace(/<table id="rp-(?:over-tbl|tbl-\w)"[\s\S]*?<\/table>/g, "");
  ok(!/担当[ABC]/.test(outside), "一覧の外（足した数の場所）に担当の名前が出ている");
  ok(textOf(h).includes("担当ごとの合計は出していません"), "担当ごとの金額を出していないと書いていない");
});

check("段B 満了と継続: 一覧は満了日・満了まで（過ぎたものは文字で）・案件（詳細へ）・担当・ステージ・金額・名札。先月以前の分は畳んで残す", () => {
  const h = run("renderRenewalPipe(__RP)");
  const list = h.slice(h.indexOf('<table id="rp-tbl-0"'), h.indexOf("</table>", h.indexOf('<table id="rp-tbl-0"')));
  const th = [...list.matchAll(/<th[^>]*>([^<]*)<\/th>/g)].map((m) => m[1]);
  ok(JSON.stringify(th) === JSON.stringify(["満了日", "満了まで", "案件", "担当", "ステージ", "金額", "名札"]), "一覧の列が違う: " + th.join(","));
  ok(list.includes("満了を8日過ぎている"), "満了を過ぎた行を文字で言っていない（色だけ）");
  ok(list.includes('href="#deal/detail?id=1"'), "案件名が案件の詳細へのリンクでない");
  const l1 = h.slice(h.indexOf('<table id="rp-tbl-1"'), h.indexOf("</table>", h.indexOf('<table id="rp-tbl-1"')));
  ok(l1.includes("ステージ名なし") && !/>\d{6,}</.test(list + l1), "ステージが空の行・内部IDの扱いが違う");
  ok(list.indexOf("案件あ") >= 0 && list.indexOf("案件あ") < list.indexOf("案件い"), "満了の近い順でない");
  /* 🔴 2026-09-29 検証: 3 枚の札がどれも 3 か月を 1 つにした一覧の頭へ飛び、来月の行は 63 行目からだった。
     札はその月の表へ飛び、表にはその月の行だけがある */
  const kj = [...h.slice(h.indexOf('<div class="kpis">'), h.indexOf('<div class="note')).matchAll(/data-jump="([^"]+)"/g)].map((x) => x[1]);
  ok(JSON.stringify(kj) === JSON.stringify(["rp-m0", "rp-m1", "rp-m2", "rp-over"]), "札の行き先が月ごとでない: " + kj.join(","));
  ok(!list.includes("案件う") && l1.includes("案件う") && !l1.includes("案件あ"), "月の表に別の月の行が入っている");
  ok(textOf(h).includes("来月 2026-10 に満了する契約（満了の近い順、1 件）"), "月の表の見出しに月と件数が無い");
  ok(h.includes('id="rp-m2"') && textOf(h).includes("この月に満了する稼働中の契約はありません"), "行の無い月の節が無い（札の行き先が消える）");
  ok(!h.includes('id="rp-stray"'), "どの月にも合う行なのに、合わない行の節が出ている");
  const ov = h.slice(h.indexOf('id="rp-over"'));
  ok(ov.includes('<details class="fold"><summary>表を開く') && ov.includes("過ぎた案件"), "先月以前に満了日を過ぎた契約を畳んで残していない");
  const toc = h.slice(h.indexOf('<nav class="toc"'), h.indexOf("</nav>"));
  const js = [...toc.matchAll(/data-jump="([^"]+)"/g)].map((x) => x[1]);
  ok(JSON.stringify(js) === JSON.stringify(["rp-m0", "rp-m1", "rp-m2", "rp-over", "rp-stage"]), "目次の行き先が違う: " + js.join(","));
  js.forEach((id) => ok(h.includes(' id="' + id + '" tabindex="-1"'), "目次の行き先 " + id + " が本文に無い"));
  /* 先月以前が 0 件なら札も節も目次も出さない（空の畳みを出さない） */
  const z = JSON.parse(JSON.stringify(ctx.__RP));
  z.overdue_before = { sum: { n: 0, amount: null, amount_n: 0, amount_missing: 0 }, rows: [] };
  ctx.__RPZ = z;
  const hz = run("renderRenewalPipe(__RPZ)");
  ok(!hz.includes('id="rp-over"') && !hz.includes('data-jump="rp-over"'), "0 件なのに先月以前の節がある");
  /* 一覧（毎週の仕事）がステージの内訳より前（ステージ 18 種を先に置くと 1440px で一覧が 1 画面目の外だった） */
  ok(h.indexOf('id="rp-m0"') < h.indexOf('id="rp-stage"'), "一覧がステージの内訳より後ろにある");
});

check("段B 成果と継続: 金額の札（稼働中・今月〜再来月に満了・金額で見た継続率）は会社全体だけ。継続率は件数の札と同じ月で、満了した金額を並べる", () => {
  const RS = JSON.parse(JSON.stringify(ctx.__RS));
  RS.money = {
    active_total: { n: 604, amount: 677609694, amount_n: 602, amount_missing: 2 },
    window: { months: ["2026-09", "2026-10", "2026-11"], sum: { n: 285, amount: 258592800, amount_n: 285, amount_missing: 0 } },
    overdue_before: { n: 10, amount: 1271903, amount_n: 10, amount_missing: 0 },
    retention: { rows: [
      { month: "2026-06", keep: 52614000, cancel: 30237000, fill: 8700000, denom: 91551000, pending: 0, settled_n: 107, settled_missing: 0, rate: 57.4696 },
      { month: "2026-09", keep: 40000000, cancel: 15000000, fill: 5504000, denom: 60504000, pending: 0, settled_n: 60, settled_missing: 1, rate: 66.11 },
    ] },
    amount_basis: "",
  };
  ctx.__RSM = RS;
  const h = run("renderResults(__RSM)");
  const k = h.slice(h.indexOf('<div class="kpis">'), h.indexOf('id="rs-renewal"'));
  const lbls = [...k.matchAll(/<span class="lbl">([^<]*)<\/span>/g)].map((m) => m[1]);
  ok(JSON.stringify(lbls) === JSON.stringify(["稼働中の契約の金額（会社全体）", "今月〜再来月に満了する金額", "月次の継続率（2026-09 に満了）",
    "金額で見た継続率（2026-09 に満了）", "採用目標に届いた稼働中の契約"]), "札の顔ぶれ・並びが違う: " + lbls.join(" / "));
  const tk = textOf(k);
  ok(tk.includes("67,761万") && tk.includes("稼働中 604 件の合計・金額が空の 2 件は足していません") && tk.includes("月額ではありません"),
    "稼働中の金額に母数・空の件数・月額でないことが無い: " + tk);
  ok(tk.includes("25,859万") && tk.includes("285 件（2026-09〜2026-11 に満了）") && tk.includes("確度は掛けていません"), "満了する金額の札: " + tk);
  ok(k.includes('href="#research/renewalpipe"'), "満了する金額から満了と継続へ行けない");
  /* 🔴 2026-09-29 検証: 先月以前に満了日を過ぎてまだ稼働中の契約（fixture 10 件）を外していることが札に無かった */
  ok(tk.includes("先月以前に満了日を過ぎてまだ稼働中の 10 件（127万）は入れていません"), "満了する金額の札が、外した先月以前の分を書いていない: " + tk);
  ok(tk.includes("66.1%") && tk.includes("満了した金額 6,050万（継続 4,000万・解約 1,500万・充足 550万）") && tk.includes("金額が空の 1 件は入れていません"),
    "金額の継続率が件数の札と同じ月（2026-09）・満了した金額の内訳つきでない: " + tk);
  ok(!/見込み/.test(tk), "札に見込み（確度を掛けた金額に読める）と書いている");
  ok(!/<button[^>]*class="kpi[^>]*>(?:(?!<\/button>)[\s\S])*<a /.test(k), "button.kpi の中に a がある");
});

/* ================================================================ 磨き込み「見た目とスマホ」（2026-09-29、pol/layout） */
// 🔴 fixture 400×900 の実測: 今日の表の頭が y=1,228（題字と鮮度 183px・メニュー 4 行 170px・札 5 枚が 2 列 3 段 425px・担当の欄 74px）で、
//    1 画面目に表が入らなかった。直した後は表の 1 行目の下端が y=868（鮮度が赤の 2 行のとき。緑ならさらに上）
check("磨き込み(1): 400px の今日の 1 画面目に表の見出しと 1 行目 — メニューは 1 行で横に流し、札は 1 行で横に流し、担当の欄は 1 行", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  const inside = media600(css).inside.join("\n"), outside = media600(css).outside;
  // 札: 600px 以下は 1 行で横に流す。札の幅は半分未満（3 枚目の端が見えて、続きがあると分かる）。中身は削らない
  const kpis = (inside.match(/\.kpis\{ display:flex;[^}]*\}/) || [""])[0];
  ok(/overflow-x:auto/.test(kpis) && /no-repeat local/.test(kpis), "600px 以下で札を横に流していない、または続きの影が無い: " + kpis);
  const basis = +((inside.match(/\.kpis > \.kpi\{ flex:0 0 (\d+)%/) || [])[1] || 0);
  ok(basis > 0 && basis < 50, "600px 以下の札の幅が半分以上（3 枚目の端が見えず、続きがあると分からない）: " + basis);
  ok(!/\.kpi[^{]*\{[^}]*display:none/.test(inside) && !/\.kpi \.fine\{[^}]*(display:none|-webkit-line-clamp)/.test(inside), "600px 以下で札の中身（母数の文など）を隠している");
  ok(!/\.kpis\{ display:flex/.test(outside), "PC でも札を横に流している（PC は格子のまま）");
  // 担当の欄: 欄と案内の文を 1 行に（前は 2 段で 74px）。案内の文は消さない
  ok(/#today-filter\{ flex-wrap:nowrap;/.test(inside) && /#today-filter > \.muted\.small\{[^}]*min-width:0;/.test(inside), "600px 以下で担当の欄と案内の文が 1 行でない");
  ok(!/#today-filter[^{]*\{[^}]*display:none/.test(inside), "600px 以下で担当の欄の案内の文を隠している");
  ok(!/#today-filter/.test(outside), "担当の欄の詰め方が PC にも効いている");
});

// 🔴 メニューを 1 行にすると、右の方の画面（記録と数字の信頼度など）を開いたとき、その項目が画面の外にあってどこにいるか読めない。
//    描いたとき・印を付け替えたときに、いま見ている項目を行の中へ送る（ページは動かさない: scrollIntoView は使わない）
check("磨き込み(1): メニューを 1 行で横に流しても、いま見ている項目は行の中に見える（sideShowCurrent）", () => {
  const mk = (left, right) => ({ getBoundingClientRect: () => ({ left, right, width: right - left }) });
  const btn = mk(520, 640);
  const box = Object.assign(mk(16, 384), { scrollWidth: 1100, clientWidth: 368, scrollLeft: 0, querySelector: (q) => (q === 'button[aria-current="page"]' ? btn : null) });
  ctx.__SB = box;
  run("sideShowCurrent(__SB)");
  // ボタンの中心を枠の中心へ: 520 - 16 - (368 - 120) / 2 = 380
  ok(box.scrollLeft === 380, "いま見ている項目が行の外のまま（scrollLeft " + box.scrollLeft + "）");
  box.scrollLeft = 0; ctx.__SB2 = Object.assign({}, box, { querySelector: () => mk(60, 120) });
  run("sideShowCurrent(__SB2)");
  ok(ctx.__SB2.scrollLeft === 0, "見えている項目なのに行を動かしている");
  ctx.__SB3 = Object.assign({}, box, { scrollWidth: 368, scrollLeft: 0 });
  run("sideShowCurrent(__SB3)");
  ok(ctx.__SB3.scrollLeft === 0, "はみ出していない（PC の縦の列）のに動かしている");
  ok(!/scrollIntoView/.test(run("sideShowCurrent.toString()")), "scrollIntoView を使っている（ページまで縦に動く）");
  ok(/sideShowCurrent\(box\); return; \}/.test(run("drawSide.toString()")) && /\n  sideShowCurrent\(box\);\n\}$/.test(run("drawSide.toString()")),
    "drawSide が印の付け替え・組み直しのどちらかで sideShowCurrent を呼んでいない");
});

// 🔴 400px の案件一覧で見方のボタン 4 つが 1 つずつの行に落ちて 196px（fixture 実測。直した後 119px）
check("磨き込み(2): 400px の案件一覧の見方のボタンは 2 列（4 つとも見えたまま。畳まない・隠さない）", () => {
  const css = html.slice(0, html.indexOf("</style>"));
  const inside = media600(css).inside.join("\n"), outside = media600(css).outside;
  ok(/#board-views\{ display:grid; grid-template-columns:repeat\(2,minmax\(0,1fr\)\);/.test(inside), "600px 以下で見方のボタンが 2 列でない");
  ok(/#board-views > button\.act\{ white-space:normal;/.test(inside), "600px 以下で見方のボタンの中で名前を折り返していない（2 列に入らない）");
  ok(!/#board-views[^{]*\{[^}]*(display:none|overflow:hidden)/.test(inside), "600px 以下で見方のボタンを隠している");
  ok(!/#board-views/.test(outside), "見方の 2 列が PC にも効いている（PC は 1 行）");
  const bar = run('boardViewBar({ meta: { act_views: [{ key: "a", label: "甲", n: 1 }, { key: "b", label: "乙", n: 2 }, { key: "c", label: "丙", n: 3 }, { key: "d", label: "丁", n: 4 }] } })');
  ok((bar.match(/<button /g) || []).length === 4, "見方のボタンが 4 つ出ていない: " + bar);
});

Promise.all(pendingChecks).then(() => {
  console.log("\n" + passed + " 件通過 / " + failed + " 件失敗");
  if (failed) process.exit(1);
});
