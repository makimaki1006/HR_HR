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
  ok(!/40123456789|40987654321|40555555555/.test(h), "取引ID（11桁）が画面に出ている");
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

/* ================================================================ 第2弾: 文言と凡例（2026-09-23 デプロイ後の実機確認） */
/* 描いた HTML から文字だけを取り出す（タグ・SVG を落とす） */
const textOf = (h) => String(h).replace(/<svg[\s\S]*?<\/svg>/g, " ").replace(/<[^>]+>/g, " ");

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

check("houjin 採用単価: 稼働中の契約を未確定の見た目にし、凡例と棒の塗り・濃さをそろえる", () => {
  const g = cpa3Fig();
  const bars = [...g.matchAll(/<rect x="[0-9.]+" y="[0-9.]+" width="[0-9.]+" height="[0-9.]+" rx="2" style="fill:([^"]+)" opacity="([.0-9]+)">/g)]
    .map((m) => ({ fill: m[1], op: m[2] }));
  ok(bars.length === 2, "棒の数が 2 でない: " + bars.length);
  ok(bars[0].fill === "var(--ai)", "確定の契約が藍でない: " + bars[0].fill);
  ok(bars[1].fill === "var(--ghost)", "稼働中（censored=false）の契約を確定と同じ " + bars[1].fill + " で塗っている");
  const leg = g.split('<div class="figlegend">')[1];
  const a = legendSwatch(leg, "総額 ÷ 採用数（確定）"), b = legendSwatch(leg, "同（未確定・稼働中）");
  ok(a && a.fill === bars[0].fill && a.op === bars[0].op, "確定の凡例の粒 " + JSON.stringify(a) + " が棒 " + JSON.stringify(bars[0]) + " と違う");
  ok(b && b.fill === bars[1].fill && b.op === bars[1].op, "未確定の凡例の粒 " + JSON.stringify(b) + " が棒 " + JSON.stringify(bars[1]) + " と違う");
});

check("houjin 採用単価: 進捗帯の中央値を図の上に印で出し、位置が軸の尺度に合う", () => {
  const g = cpa3Fig();
  const marks = [...g.matchAll(/<circle cx="([0-9.]+)" cy="([0-9.]+)" r="4.5" data-mark="band"/g)];
  ok(marks.length === 1, "中央値の印が 1 つでない: " + marks.length + "（総額が出ていて帯の中央値がある棒は 1 本）");
  // 中央値 900000 は「確定の契約」の総額と同じ。印の x はその棒の右端と一致するはず（数値で確かめる）
  const r0 = g.match(/<rect x="([0-9.]+)" y="([0-9.]+)" width="([0-9.]+)" height="([0-9.]+)" rx="2" style="fill:var\(--ai\)"/);
  const right = +r0[1] + +r0[3];
  ok(Math.abs(+marks[0][1] - right) < 0.6, "印の x " + marks[0][1] + " が 900000 の棒の右端 " + right.toFixed(1) + " と合わない");
  // y は2本目（稼働中の契約）の棒の中心
  const r1 = g.match(/<rect x="[0-9.]+" y="([0-9.]+)" width="[0-9.]+" height="([0-9.]+)" rx="2" style="fill:var\(--ghost\)"/);
  ok(Math.abs(+marks[0][2] - (+r1[1] + +r1[2] / 2)) < 0.6, "印の y " + marks[0][2] + " が稼働中の棒の中心と合わない");
  ok(g.includes("同じ進捗帯の中央値"), "印の凡例が無い");
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

check("凡例: 目標の帯の「承諾数が空」と「未記入」を見分けられる塗りにする", () => {
  const h = run("renderOutcome(__OUT)");
  const g = h.split("<figcaption>達成率 ＝ 承諾数 ÷ 採用目標数")[1].split("</figure>")[0];
  const leg = g.split('<div class="figlegend">')[1];
  const a = legendSwatch(leg, "承諾数が空（目標はある）"), b = legendSwatch(leg, "未記入（目標が無い）");
  ok(a && b, "凡例が取れない");
  ok(a.fill !== b.fill || a.op !== b.op, "承諾数が空と未記入の凡例が同じ塗り " + JSON.stringify(a));
  const rs = stackRects(g);
  const ra = rs.find((r) => r.label.startsWith("承諾数が空")), rb = rs.find((r) => r.label.startsWith("未記入"));
  ok(ra && rb && (ra.fill !== rb.fill || ra.op !== rb.op), "帯の中で承諾数が空と未記入が同じ塗り");
  ok(a.op === ra.op && b.op === rb.op, "凡例の濃さが帯と違う");
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
  const h = run("renderOutcome(__OUT2)");
  ok((h.match(/この並びは機械が付けた順/g) || []).length === 1,
    "order_note が " + (h.match(/この並びは機械が付けた順/g) || []).length + " 回出ている");
  const r = JSON.parse(JSON.stringify(ctx.__RU));
  // rampup.json の phase.rule（2026-09-23 実測）
  r.phase = { rule: "契約長に対する割合。< 0.34 序盤 / < 0.67 中盤 / <= 1.05 終盤 / 超 満了超過。同じ3ヶ月目でも、3ヶ月契約なら満了・12ヶ月契約なら序盤",
    rows: [{ label: "序盤", n: 1 }] };
  ctx.__RU3 = r;
  const t = textOf(run("renderRampup(__RU3)"));
  const n = (t.match(/同じ「?3ヶ月目」?でも/g) || []).length;
  ok(n === 1, "「同じ3ヶ月目でも」が " + n + " 回出ている");
});

check("法人番号で見る: 末尾の「集計の基準日と件数」に件数を書く", () => {
  const h = run('foot({ today: "2026-09-18" }, false, houjinCounts([{ is_active: true }, { is_active: false }], new Set(["a"])))');
  ok(h.includes("集計の基準日と件数") && h.includes("この法人の取引 2 件（稼働中 1 件）"), "件数が無い: " + h);
  const body = html.split("function renderHoujin(D)")[1].split("\nfunction ")[0];
  ok((body.match(/foot\(D\.meta, false, houjinCounts\(all, ids\)\)/g) || []).length === 2,
    "renderHoujin の2つの末尾が件数を渡していない");
});

check("KPI: 最終満了を折り返さない・電話の61件の色をそろえる・退職者の補足に別の話を混ぜない", () => {
  ok(/\.kpi\.is-date \.big\{[^}]*white-space:nowrap/.test(html), "日付の KPI に white-space:nowrap が無い");
  const c = run('custBlocks({ meta: { found: true, houjin: "H" }, customer: { name: "法人", deals: 1, active: 1, sites: 1, ltv: 1, max_renewal_no: 0, last_expiration: "2027-02-28" } }, new Set(["head"]))');
  ok(/<div class="kpi is-date"><span class="lbl">最終満了/.test(c), "最終満了の KPI が日付の型（is-date）になっていない");
  const ph = run("renderPhone(__PH)");
  const card = ph.match(/<div class="kpi( is-[a-z]+)?"><span class="lbl">電話が1本も無い/);
  ok(card && card[1] === " is-bad", "電話が1本も無いの KPI が赤でない: " + (card && card[1]));
  ok(/style="fill:var\(--hi\)"[^>]*><title>電話が1本も無い/.test(ph), "内訳の帯の「電話が1本も無い」が赤でない（KPI と色がそろわない）");
  const tm = run("renderTeam(__D)");
  const k = tm.split('<span class="lbl">退職者のまま</span>')[1].split("</div>")[0];
  ok(!k.includes("割れ") && !k.includes("38"), "退職者のままの補足に担当の割れ（38件）が混ざっている: " + k);
  ok(tm.includes("担当が割れている稼働中の案件が 38 件"), "担当の割れ（38件）がどこにも出ていない（黙って消した）");
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

check("V12 の残り: 表の枠の端に、横の続きがある側だけ影を出す", () => {
  const h = run('scroll(table([{ t: "a" }], [[1]]), 400)');
  ok(/<div class="scroll-wrap"><div class="scroll"/.test(h), "枠が影を描く包み（scroll-wrap）に入っていない");
  ok(/\.scroll-wrap\.more-l::before, \.scroll-wrap\.more-r::after\{ opacity:1; \}/.test(html), "影を出す CSS が無い");
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

console.log("\n" + passed + " 件通過 / " + failed + " 件失敗");
if (failed) process.exit(1);
