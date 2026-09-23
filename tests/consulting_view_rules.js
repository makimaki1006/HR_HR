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

const html = fs.readFileSync(
  path.join(__dirname, "..", "templates/tabs/cs_dashboard.html"), "utf-8");
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
const ctx = vm.createContext({
  document,
  location: { hash: "", href: "http://localhost/consulting" },
  history: { replaceState() {} },
  window: { addEventListener() {}, scrollTo() {} },
  fetch: () => new Promise(() => {}),   // 取りに行かない（返事が来ないまま）
  setTimeout: () => 0, clearTimeout: () => {},
  URLSearchParams, console,
});
vm.runInContext(js, ctx, { filename: "cs_dashboard.html#script" });
const run = (code) => vm.runInContext(code, ctx);

let failed = 0, passed = 0;
function check(name, fn) {
  try { fn(); passed++; console.log("OK   " + name); }
  catch (e) { failed++; console.error("FAIL " + name + "\n     " + e.message); }
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

check("V2: 接触率の最下位候補から母数が小さい人を外す", () => {
  const p = run("pickWorstContact(__TEAM)");
  ok(p.worst && p.worst.consultant === "h9821a39368fe",
    "最下位が " + (p.worst && p.worst.consultant) + "。母数が小さい h6e0d…（0/11か月）を拾っていないか");
  ok(p.skipped === 5, "外した人数が " + p.skipped + "（期待 5）");
});

check("V2: KPI に外した人数を書き、母数が小さい人の名前を出さない", () => {
  ctx.__D = {
    rows: TEAM_ROWS, meta: { n_consultant: 27, n_active: 604, unknown_owner: 0, retired_deals: 0,
      retired_people: 0, owner_ties: 38, not_counted: "※ 担当者の評価ではありません" },
    contact_rule: "", small_n_rule: "", focus_rule: "", owner_rule: "担当は consultant が正本です",
  };
  const h = run("renderTeam(__D)");
  const kpi = h.split('<div class="kpis">')[1].split("</div>")[0] + h.split('<div class="kpis">')[1].split("</div>")[1];
  ok(kpi.includes("h9821a39368fe"), "KPI に母数の足りる最下位が出ていない");
  ok(!kpi.includes("h6e0d76778594"), "KPI に母数が小さい人（0/11か月）が出ている");
  ok(kpi.includes("母数が小さい 5 名は候補から外しています"), "外した人数が KPI に書かれていない");
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
  // renewal.json の継続9回目は n=6、解約率 0.0%（2026-09-23 実測）
  ctx.__RN2 = { meta: { exclude_right_censored: false, right_censored_n: 0 },
    monthly_retention: { rows: [] }, missingness: [], population: {},
    by_renewal: [{ renewal_no: 9, n: 6, n_active: 0, cancel_rate: 0, cancel_rate_excl_fill: 0,
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
  const d = h.split('<details class="fold"><summary>満了月ごとの内訳')[1].split("</details>")[0];
  ok(!d.includes("上の折れ線と同じ"), "summary が「上の折れ線と同じ数字」のまま（件数は折れ線に無い）");
  ok(d.includes("結果待ち 12 件"), "summary に結果待ちの合計（3+9）が無い");
  ok(d.includes('<div class="scroll"'), "開いた表が scroll の枠に入っていない（400px 幅ではみ出す）");
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
    { site: "拠点1", cpa: 300000, cancel_rate: 50, deals: 4, syoudaku: 2, active: 1, cancel: 2, amount: 1 },
    { site: "拠点2", cpa: 120000, cancel_rate: 0, deals: 1, syoudaku: 1, active: 0, cancel: 0, amount: 1 }] }],
};

check("V20: 本部アプローチの拠点の解約率に母数（取引数）を添える", () => {
  const h = run("renderHq(__HQ)");
  ok(h.includes("解約 50.0%（4件中）"), "解約率に母数が無い");
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
  const h = run("renderTeam(__D)");
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

console.log("\n" + passed + " 件通過 / " + failed + " 件失敗");
if (failed) process.exit(1);
