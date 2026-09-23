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
const vbW = (svg) => +((svg.match(/viewBox="0 0 ([\d.]+)/) || [0, 0])[1]);
const firstSvg = (h) => { const a = h.indexOf("<svg"); return h.slice(a, h.indexOf("</svg>", a) + 6); };

check("図の部品(1): 狭い画面で図を縮めきらず、枠の中で横に動かす（文字 10px を下限にする）", () => {
  // CSS: 図の最小幅を描いた幅（--fw）から決める。min-width は max-width:100% より強い
  const css = html.slice(0, html.indexOf("</style>"));
  ok(/figure\.fig \.figbody > svg\{\s*min-width:calc\(var\(--fw, 0px\) \* \.92\)/.test(css),
    "図の最小幅（--fw の .92 倍）の CSS が無い。400px 幅で 11px の文字が 4〜5px に縮む");
  ok(/figure\.fig \.figbody\{[^}]*overflow-x:auto/.test(css), "図の枠が横にスクロールしない（ページ本体が広がる, V17）");
  ok(/@media \(max-width:600px\)\{[\s\S]*?\.figscroll\{ display:block; \}/.test(css),
    "狭い画面で「横にスクロールできます」の案内を出していない");
  // どの図の道具も --fw を持つ。持たない図だけが 400px で縮む
  const svgs = {
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
  ok(f.includes('class="figscroll"'), "700px の図に横スクロールの案内が付かない");
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
});

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
  const svg = firstSvg(run("renderRenewal(__RN)"));
  const ov = overlaps(svg);
  ok(!ov.length, "月次継続率の文字が重なる: " + ov.slice(0, 4).join(" / "));
  ok(!/値が無い（0 ではない）/.test(svg), "n=0 の月に 0 の高さの灰色の線を描いている（凡例に無い印）");
  ok(svg.includes(">27-01<") && svg.includes(">n=1<"), "最後の月（27-01 n=1）のラベルが無い");
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
});

check("図の部品(6): 接触率の図の目盛りが最大値を覆い、率と母数が棒の近くにある", () => {
  // fixture の接触率（母数が足りる担当者）の最大は 96.92%。前は目盛りが 75% で止まっていた
  const h = run(`svgBarH({ w: 720, fmt: F.pp, rh: 24, rows: [
    { label: "h9821a39368fe", v: 26.4, txt: "26.4%", note: "33/125 か月　案件28" },
    { label: "h60b499e1c307", v: 96.92, txt: "96.9%", note: "63/65 か月　案件22" } ] })`);
  const tks = textBoxes(h).filter((b) => /^\d+%$/.test(b.s)).map((b) => parseFloat(b.s));
  ok(Math.max(...tks) >= 96.92, "目盛りの最大が " + Math.max(...tks) + "%（96.9% の棒が目盛りの先へ伸びる）");
  const b = textBoxes(h), v = b.find((q) => q.s === "96.9%"), n = b.find((q) => q.s.startsWith("63/65"));
  ok(n.x0 - v.x1 <= 60, "いちばん長い棒の率から母数まで " + (n.x0 - v.x1).toFixed(0) + "px 離れている（前は約200px）");
  ok(/stroke-dasharray="1 3"/.test(h), "短い棒の行に、注記までつなぐ点線が無い");
  ok(!overlaps(h).length, "文字が重なる: " + overlaps(h).join(" / "));
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

console.log("\n" + passed + " 件通過 / " + failed + " 件失敗");
if (failed) process.exit(1);
