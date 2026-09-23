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
  // 横にスクロールする枠は Tab で止まれる（キーボードの矢印で動かせる）。スクロールしない小さい図には付けない
  ok(/<div class="figbody" tabindex="0" role="group" aria-label="題（横にスクロールできる枠）">/.test(f),
    "横にスクロールする枠に tabindex / 名前が無い（キーボードで動かせない）");
  const small = run('fig("小", "", svgStack({ w: 300, parts: [{ label: "a", v: 1 }] }))');
  ok(!/figscroll|tabindex/.test(small), "330px に収まる図にまでスクロールの案内・tabindex を付けている");
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
  // 🔴 率に母数（2026-09-23 検証）。xPick・間引きで横軸の2段目（n=）を出さない月でも、点の title で母数が読める
  const tt = [...svg.matchAll(/<circle [^>]*><title>([^<]*)<\/title>/g)].map((m) => m[1]);
  ok(tt.length > 0 && tt.every((t) => /（n=\d+）/.test(t)),
    "点の title に母数（n=）が無い月がある: " + tt.filter((t) => !/（n=\d+）/.test(t)).slice(0, 3).join(" / "));
  const hidden = tt.filter((t) => !svg.includes(">" + t.split("（")[0] + "<"));
  ok(hidden.length > 0, "この入力では横軸のラベルを出さない月があるはず（見張りの前提が崩れている）");
  // 値が無い月（n=0）で線が切れることを、凡例で説明している（fig の data-gap）
  ok(/<svg [^>]*data-gap="1"/.test(svg), "n=0 の月で線が切れるのに、図に「切れ目あり」の印（data-gap）が無い");
  ok(run("renderRenewal(__RN)").includes("線が途切れているところは、その月の値が無いところです"),
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

check("byowner: 担当を選ぶ前の持ち件数は、見えない絞り込み（名札・満了・案件名）で減らさない", () => {
  // 案件そのものの画面で名札・案件名を絞ってから byowner に来た形。選択欄は隠れている
  run('cur = { menu: "consultant", view: "byowner" }; boardFilter = { consultant: "", flag: "NPSが4以下", expiry: "", q: "案件1" };');
  try {
    const h = run("renderBoard(__BD)");
    const tbl = h.split('<table id="owner-tbl"')[1].split("</table>")[0];
    const cnt = (name) => ((tbl.match(new RegExp('data-c="' + name + '"[^>]*>' + name + "</a></td><td[^>]*>([0-9,]+)<")) || [])[1]);
    ok(cnt("田中") === "2" && cnt("佐藤") === "1",
      "持ち件数が隠れた絞り込みで減っている: 田中 " + cnt("田中") + " / 佐藤 " + cnt("佐藤") + "（稼働中の全件は 2 / 1）");
    ok(h.includes("稼働中 3 件"), "見出しの件数が稼働中の全件でない");
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
    withWraps(w1, () => run("wireHoujin()"));
    ok(w1.cls.has("more-r"), "hqCache から差し込んだ後に影を付けていない");
  } catch (e) { restore(); throw e; }
  // 取りに行ったとき（fetch の後）
  run("hqCache = null");
  const w2 = fakeWrap();
  ctx.fetch = () => Promise.resolve({ status: 200, json: () => Promise.resolve({ ok: 1 }) });
  ctx.document.querySelectorAll = (s) => (s === ".scroll-wrap" ? [w2.el] : []);
  run("wireHoujin()");
  const tick = () => new Promise((res) => setImmediate(res));
  return tick().then(tick).then(() => {
    restore();
    ok(w2.cls.has("more-r"), "本部アプローチを取りに行って差し込んだ後に影を付けていない");
  }, (e) => { restore(); throw e; });
});

check("V12 の残り: 暗い表示でも枠の端の影が地と見分けられる（明るい表示と同じくらいの差）", () => {
  const css = html.split("<style>")[1].split("</style>")[0];
  ok(/linear-gradient\(to right, var\(--edge-shade\)/.test(css) && /linear-gradient\(to left, var\(--edge-shade\)/.test(css),
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

Promise.all(pendingChecks).then(() => {
  console.log("\n" + passed + " 件通過 / " + failed + " 件失敗");
  if (failed) process.exit(1);
});
