// コンサルダッシュボードの画面に埋め込んだ JS が、**構文として通るか**を見る。
// 後半では、画面の**動き**（操作したときに壊れないか）も見る。
//
// ------------------------------------------------------------------
// なぜ要るか
// ------------------------------------------------------------------
// 2026-09-21 の実害: テンプレートの JS に構文エラーが1つあり、
// `<script>` 全体が実行されなかった。結果、API が1本も呼ばれず
// 画面は真っ白のまま。それでも
//   - cargo test は 3,393 件すべて通る（Rust 側は無関係）
//   - curl で /api/consulting/* を叩くと 200 が返る（サーバは正常）
//   - /consulting は 200 を返す（HTML は出ている）
// ので、**どの確認にも引っかからなかった**。
//
// 原因は `"<h3 style=\\"...\\">"` と書いていたこと。JS では `\\` が
// 「バックスラッシュ1つ」になるので、その次の `"` で文字列が終わってしまう。
//
// 2026-09-23 の全体レビュー（handover_2026-09-23/07）で、構文は通っているのに
// **操作すると壊れる**ものが13件見つかった（U1〜U13）。どれも Rust のテストからは
// 見えない。後半の「動きの見張り」は、画面の JS を Node の中で実際に動かして、
// その13件と、描き方の規律に反していたもの（N5 N8 N10 V5〜V8 V18 V19）を見る。
//
// ------------------------------------------------------------------
// 使い方
// ------------------------------------------------------------------
//     node tests/consulting_page_js.js
//
// サーバもブラウザも要らない。テンプレートを読んで構文を見て、
// 画面の JS を偽の document の上で動かすだけ。
// 落ちたら終了コード 1 と、何行目か／どの見張りかを出す。
//
// 逆証明用: CS_PAGE_TEMPLATE に別のファイル（直す前の版など）を渡すと、そちらを検査する。
//     git show dea0bda:templates/tabs/cs_dashboard.html > <scratch>/before.html
//     CS_PAGE_TEMPLATE=<scratch>/before.html node tests/consulting_page_js.js
// 直す前の版で「動きの見張り」が落ちなければ、その見張りは何も守っていない。
"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const OVERRIDE = process.env.CS_PAGE_TEMPLATE || "";
const TEMPLATES = [
  OVERRIDE || "templates/tabs/cs_dashboard.html",
];

let failed = 0;
let mainJs = null;   // 1つ目のテンプレートの <script>。下の「動きの見張り」で動かす

for (const rel of TEMPLATES) {
  const file = OVERRIDE ? path.resolve(rel) : path.join(__dirname, "..", rel);
  const html = fs.readFileSync(file, "utf-8");

  // Askama の差し込みは JS から見ると構文エラーになるので、先に潰す。
  // ここで見たいのは**自分が書いた JS**の構文だけ。
  const cleaned = html.replace(/\{\{[^}]*\}\}/g, '"__askama__"');

  const blocks = [...cleaned.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)];
  if (blocks.length === 0) {
    console.error(`FAIL ${rel}: <script> が1つも無い`);
    failed++;
    continue;
  }

  blocks.forEach((m, i) => {
    const js = m[1];
    if (i === 0 && mainJs == null) mainJs = js;
    try {
      // 実行はしない。構文が通るかだけを見る。
      new vm.Script(js, { filename: `${rel}#script[${i}]` });
      const lines = js.split("\n").length;
      console.log(`OK   ${rel} #script[${i}]  ${lines}行 / ${js.length}文字`);
    } catch (e) {
      console.error(`FAIL ${rel} #script[${i}]: ${e.message}`);
      // 何行目かを出す（V8 は filename:line の形で持っている）
      const at = (e.stack || "").split("\n").find((l) => l.includes(rel));
      if (at) console.error(`     ${at.trim()}`);
      failed++;
    }
  });

  // 🔴 今回の原因そのもの。JS の文字列の中で `\\"` と書くと、
  //    バックスラッシュ1つ + 文字列終わり になって以降が崩れる。
  //    HTML 属性を書きたいなら、外側をシングルクォートにすること。
  const bad = cleaned.split("\n")
    .map((l, i) => [i + 1, l])
    .filter(([, l]) => l.includes('\\\\"'));
  for (const [n, l] of bad) {
    console.error(`FAIL ${rel}:${n}: JS の文字列に \\\\" がある。` +
      `外側をシングルクォートにすること -> ${l.trim().slice(0, 90)}`);
    failed++;
  }
}

// ==================================================================
// 動きの見張り
// ==================================================================
// 画面の JS を、偽の document / window / history / fetch の上で丸ごと動かす。
// 偽の DOM は innerHTML を文字列として持つだけで、HTML を解釈しない。
// だから見られるのは「描いた HTML の文字列」「状態の変数」「呼ばれた API の URL」で、
// 見た目（重なり・幅）は見ない。見た目は Playwright で実機を開いて見ること。
//
// 🔴 見張りは、**直す前の版（dea0bda）で落ちる**ことを確かめてある（上の逆証明の手順）。
//    新しく足した関数名に頼ると「直す前は関数が無いので落ちる」だけの見張りになるので、
//    できるだけ前からある入口（render* / wire / go / load / ctlbar）から叩いている。

/** 1つの見張りごとに、まっさらな画面を作る（状態の変数を持ち越さない） */
function boot() {
  const reg = {};        // getElementById が返すもの
  const qs = {};         // querySelector が返すもの
  const qsa = {};        // querySelectorAll が返すもの
  const listeners = {};  // window.addEventListener で登録されたもの
  const fetched = [];    // 呼ばれた URL と、応答を返すための resolve
  const timers = [];
  const hist = { push: 0, replace: 0 };

  class El {
    constructor(id) {
      this.id = id; this.innerHTML = ""; this.style = {}; this.className = "";
      this.dataset = {}; this.value = ""; this.checked = false; this.focused = 0;
    }
    focus() { this.focused++; doc.activeElement = this; }
    getAttribute(k) { return k === "id" ? this.id : null; }
    querySelectorAll() { return []; }
    closest() { return null; }
    setSelectionRange() {}
  }
  const doc = {
    body: { nodeName: "BODY" },
    activeElement: null,
    getElementById: (id) => reg[id] || null,
    querySelector: (s) => qs[s] || null,
    querySelectorAll: (s) => qsa[s] || [],
    createElement: () => new El(""),
  };
  doc.activeElement = doc.body;
  ["cs-fresh", "cs-menu", "cs-side", "cs-main", "cs-error"].forEach((id) => { reg[id] = new El(id); });

  const loc = { hash: "", href: "http://test.local/consulting", pathname: "/consulting" };
  const ctx = {
    console, URL, URLSearchParams, Promise,
    document: doc,
    location: loc,
    history: {
      get length() { return 1 + hist.push; },
      pushState: (_s, _t, h) => { hist.push++; loc.hash = h; },
      replaceState: (_s, _t, h) => { hist.replace++; loc.hash = h; },
    },
    window: {
      addEventListener: (k, f) => { (listeners[k] = listeners[k] || []).push(f); },
      scrollTo: () => {},
    },
    setTimeout: (f) => { timers.push(f); return timers.length; },
    clearTimeout: () => {},
    fetch: (url) => new Promise((resolve) => { fetched.push({ url: String(url), resolve }); }),
  };
  ctx.window.location = loc;
  vm.createContext(ctx);
  vm.runInContext(mainJs, ctx, { filename: "cs_dashboard.html#script" });
  const R = (expr) => vm.runInContext(expr, ctx);
  return { ctx, R, reg, qs, qsa, El, listeners, fetched, timers, hist, doc, loc };
}

/** fetch の応答（JSON） */
const jsonRes = (body, url) => ({
  ok: true, status: 200, redirected: false, url: url || "http://test.local/api",
  headers: { get: (k) => (k.toLowerCase() === "content-type" ? "application/json" : null) },
  json: async () => body,
});
const tick = () => new Promise((r) => setImmediate(r));
const count = (s, re) => (String(s).match(re) || []).length;

const checks = [];
const check = (id, what, fn) => checks.push({ id, what, fn });

/* ---- テスト用のデータ（API の応答の形だけ合わせた小さなもの） ---- */
const deal = (o) => Object.assign({
  deal_id: "d", name: "案件", stage: "定期1", kind: "サブスク", site: "S1",
  start: "2025-01-01", expiration: "2025-12-31", renewal_no: 0, amount: 1200000,
  oubo: null, mensetu: null, syoudaku: null, is_active: true, right_censored: false,
}, o);
function customerPayload(deals, extra) {
  return Object.assign({
    meta: { found: true, houjin: "H1", today: "2026-09-23" },
    customer: { name: "テスト法人", ltv: 9999999, deals: deals.length, sites: 1, active: 1,
                max_renewal_no: 0, last_expiration: "2027-01-31" },
    focus: null, deals: deals, mtgs: [], cpa3: [], cpa_by_site: [], monthly: [],
    handover: [], contacts: [],
    funnel: { oubo: null, mensetu: null, syoudaku: null },
  }, extra || {});
}
const boardRow = (o) => Object.assign({
  deal_id: "b", name: "行", consultant: "担当A", flags: [], n_flags: 0, amount: null,
}, o);
const todayPayload = (rows) => ({
  meta: { filter_rule: "", n_shown: rows.length, n_hit: rows.length, n_active: 604,
          mtg_gap: { bands: [] }, n_not_started: 0, order_rule: "", new_deal_rule: "" },
  rows: rows, expiring_this_week: [], started_this_week: [],
});

/* ================================================================ U1 */
check("U1", "右側打ち切りのチェックが描き直しても残り、外すと含める側で取り直す", async () => {
  const t = boot();
  t.R('cur = { menu: "study", view: "renewal" }');
  const cz = new t.El("cs-censor"); t.reg["cs-censor"] = cz;
  t.R("wire(viewOf('study', 'renewal'))");
  if (typeof cz.onchange !== "function") throw new Error("チェックに onchange が付いていない");
  cz.checked = true; cz.onchange();
  const on = t.R("ctlbar(viewOf('study', 'renewal'), {})");
  if (!/id="cs-censor"[^>]*checked/.test(on))
    throw new Error("押した後に描き直すとチェックが外れた表示になる");
  const u1 = t.fetched[t.fetched.length - 1].url;
  if (u1.indexOf("exclude_right_censored=1") < 0) throw new Error("外す側で取り直していない: " + u1);
  // 描き直した後の要素は checked が外れた新しいもの。そこから外す操作をする
  const cz2 = new t.El("cs-censor"); t.reg["cs-censor"] = cz2;
  t.R("wire(viewOf('study', 'renewal'))");
  cz2.checked = false; cz2.onchange();
  const u2 = t.fetched[t.fetched.length - 1].url;
  if (u2.indexOf("exclude_right_censored") >= 0) throw new Error("「含める」に戻せない: " + u2);
  if (/id="cs-censor"[^>]*checked/.test(t.R("ctlbar(viewOf('study', 'renewal'), {})")))
    throw new Error("外した後もチェックが付いたまま描かれる");
});

/* ================================================================ U2 */
check("U2", "系列を縦に並べる図の接触の帯に棒が立つ", async () => {
  const t = boot();
  const D = customerPayload([deal({ deal_id: "d1", start: "2025-01-01" })], {
    monthly: [{ deal_id: "d1", name: "案件", start: "2025-01-01", period: 12,
      series: { oubo: [{ m: 1, v: 3 }, { m: 2, v: 5 }, { m: 3, v: 5, carry: true }] }, nps: {} }],
    contacts: [{ deal_id: "d1", dates: ["2025-02-03", "2025-02-20", "2025-03-05"] }],
  });
  const h = t.R("renderSeries")(D);
  const lane = h.slice(h.indexOf("接触（MTG・60秒超の通話）"));
  // 横軸は暦の月（2025-01-01 開始なので 2ヶ月目＝25-02）。「Nヶ月」から変えた理由は N18b
  if (count(lane, /<rect [^>]*>\s*<title>25-02: [^<]* 2<\/title>/g) !== 1)
    throw new Error("2ヶ月目（25-02、接触2件）の棒が無い");
  if (count(lane, /<rect [^>]*>\s*<title>25-03: [^<]* 1<\/title>/g) !== 1)
    throw new Error("3ヶ月目（25-03、接触1件）の棒が無い");
  if (/undefined/.test(lane.slice(0, lane.indexOf("</svg>")))) throw new Error("棒の説明に undefined が出る");
});

/* ================================================================ U3 */
check("U3", "素早く切り替えると、前の画面の遅い応答で上書きされない", async () => {
  const t = boot();
  t.R('go("consultant", "team")');
  const slow = t.fetched[t.fetched.length - 1];
  t.R('go("consultant", "byowner")');
  const fast = t.fetched[t.fetched.length - 1];
  if (slow.url.indexOf("/api/consulting/consultants") !== 0 ||
      fast.url.indexOf("/api/consulting/deals") !== 0) throw new Error("想定の URL を叩いていない");
  fast.resolve(jsonRes({ meta: { flag_counts: [] }, rows: [boardRow({})] }));
  await tick(); await tick();
  slow.resolve(jsonRes({ meta: {}, rows: [] }));
  await tick(); await tick();
  const main = t.reg["cs-main"].innerHTML;
  if (main.indexOf("この担当者は、どの案件を持っているか") < 0 ||
      main.indexOf("いま、どこに手が回っていないか") >= 0)
    throw new Error("URL は担当者ごとの案件なのに、中身が担当者の一覧で上書きされた");
});

check("U3", "前の画面の遅い要求が失敗しても、いまの画面を消さずエラーも出さない", async () => {
  const t = boot();
  t.R('go("consultant", "team")');
  const slow = t.fetched[t.fetched.length - 1];
  t.R('go("consultant", "byowner")');
  const fast = t.fetched[t.fetched.length - 1];
  fast.resolve(jsonRes({ meta: { flag_counts: [] }, rows: [boardRow({})] }));
  await tick(); await tick();
  // 遅い方は失敗で返る。🔴 応答を読む段階で投げる形（ゲートウェイの 502 の HTML）にする。
  //    JSON の error で返す形だと、try の中の番号の確かめで先に抜けてしまい、
  //    catch 側の確かめを消しても落ちない
  slow.resolve({ ok: false, status: 502, redirected: false, url: "http://test.local/api",
    headers: { get: () => "text/html" }, json: async () => { throw new SyntaxError("x"); } });
  await tick(); await tick(); await tick();
  if (t.reg["cs-main"].innerHTML.indexOf("この担当者は、どの案件を持っているか") < 0)
    throw new Error("古い要求の失敗で、いまの画面（担当者ごとの案件）が消された");
  if (t.reg["cs-error"].innerHTML.indexOf("HTTP 502") >= 0)
    throw new Error("古い要求の失敗がエラーとして出ている");
});

/* ================================================================ U4 / U8 */
function houjinIndex(n, focusEvery) {
  const out = [];
  for (let i = 0; i < n; i++)
    out.push({ houjin: "H" + i, name: "法人" + i, deals: 1, sites: 1, focus: i % focusEvery === 0 });
  return out;
}
check("U4", "注力だけの絞り込みが「継続を追いかける」の法人選択に持ち越されない", async () => {
  const t = boot();
  const sel = new t.El("cs-houjin"); t.reg["cs-houjin"] = sel;
  t.ctx.__idx = houjinIndex(40, 4);
  t.R("customerIndex = __idx; focusOnly = true;");
  t.R("wire(viewOf('deal', 'series'))");
  const nSeries = count(sel.innerHTML, /<option /g);
  t.R("wire(viewOf('deal', 'houjin'))");
  const nHoujin = count(sel.innerHTML, /<option /g);
  if (nSeries !== 41) throw new Error("継続を追いかけるの選択肢が " + nSeries + "（全40法人＋見出しのはず）");
  if (nHoujin !== 11) throw new Error("法人番号で見るで注力だけに絞れていない: " + nHoujin);
});
check("U8", "法人の選択肢を件数で切らない（517法人すべて選べる）", async () => {
  const t = boot();
  const sel = new t.El("cs-houjin"); t.reg["cs-houjin"] = sel;
  t.ctx.__idx = houjinIndex(517, 1000);
  t.R("customerIndex = __idx; focusOnly = false;");
  t.R("wire(viewOf('deal', 'houjin'))");
  const n = count(sel.innerHTML, /<option /g);
  if (n !== 518) throw new Error("選択肢が " + n + "（517＋見出しのはず）");
});

/* ================================================================ U5 */
check("U5", "画面の中の移動が履歴に積まれ、戻るで前の画面に戻る", async () => {
  const t = boot();
  // 開いた直後（ハッシュ無し）の位置合わせは積まない。積むと戻るを2回押さないとページから出られない
  if (t.hist.push !== 0 || t.hist.replace !== 1)
    throw new Error("開いた直後の位置合わせで履歴を積んでいる（push " + t.hist.push +
      " / replace " + t.hist.replace + "）");
  const base = t.hist.push;
  t.R('go("consultant", "team")');
  t.R('go("study", "renewal")');
  if (t.hist.push - base !== 2) throw new Error("2回移動して履歴が " + (t.hist.push - base) + " 件しか増えない");
  // 戻る: ブラウザが URL を戻してから popstate（hashchange が来ないこともある）を送る
  t.loc.hash = "#consultant/team";
  const pop = (t.listeners.popstate || []).concat(t.listeners.hashchange || []);
  if (!pop.length) throw new Error("戻る・進むを受ける処理が無い");
  pop.forEach((f) => f({}));
  if (t.R("cur.view") !== "team") throw new Error("戻っても画面が描き直されない");
  // 🔴 上の戻るは URL が go() の書く形と同じなので、go() は push も replace も呼ばない。
  //    それだけでは「戻るで積まない」を確かめられない（前の版はここが素通りだった）。
  //    view の無い URL（#study）へ戻った場合は、go() が #study/<既定> に書き直す。そこで積むかを見る
  t.loc.hash = "#study";
  pop.forEach((f) => f({}));
  if (t.R("cur.menu") !== "study") throw new Error("戻っても画面が描き直されない（#study）");
  if (t.hist.push - base !== 2) throw new Error("戻るで履歴を積み直している（戻れなくなる）");
});

/* ================================================================ U6 */
check("U6", "今日動く先の表は見出しを押すと並び替わる", async () => {
  const t = boot();
  const D = todayPayload([
    boardRow({ deal_id: "a", name: "小さい案件", amount: 1000000, n_flags: 3, flags: ["x", "y", "z"] }),
    boardRow({ deal_id: "b", name: "大きい案件", amount: 9000000, n_flags: 2, flags: ["x", "y"] }),
  ]);
  t.ctx.__D = D;
  t.R('cur = { menu: "deal", view: "today" }; lastPayload = __D;');
  t.reg["cs-main"].innerHTML = t.R("renderToday(__D)");
  const btn = new t.El(""); btn.dataset = { k: "amount" };
  t.qsa["#today-tbl th.sortable button.sort"] = [btn];
  t.R("wire(viewOf('deal', 'today'))");
  if (typeof btn.onclick !== "function") throw new Error("今日動く先の表の見出しに操作が付いていない");
  btn.onclick(); // 金額の列は大きい順から
  const main = t.reg["cs-main"].innerHTML;
  const tbl = main.slice(main.indexOf('id="today-tbl"'));
  if (!(tbl.indexOf("大きい案件") >= 0 && tbl.indexOf("大きい案件") < tbl.indexOf("小さい案件")))
    throw new Error("金額の見出しを押しても並びが変わらない");
});

check("U6", "今週始まった契約・今週満了の表も見出しを押すと並び替わる", async () => {
  const t = boot();
  const D = todayPayload([boardRow({})]);
  D.started_this_week = [
    boardRow({ deal_id: "s1", name: "小さい新規", amount: 1000000, start: "2026-09-22" }),
    boardRow({ deal_id: "s2", name: "大きい新規", amount: 9000000, start: "2026-09-18" }),
  ];
  D.expiring_this_week = [
    boardRow({ deal_id: "e1", name: "小さい満了", amount: 1000000, days_left: 1 }),
    boardRow({ deal_id: "e2", name: "大きい満了", amount: 9000000, days_left: 5 }),
  ];
  t.ctx.__D = D;
  t.R('cur = { menu: "deal", view: "today" }; lastPayload = __D;');
  for (const [id, big, small] of [["new-tbl", "大きい新規", "小さい新規"], ["soon-tbl", "大きい満了", "小さい満了"]]) {
    t.reg["cs-main"].innerHTML = t.R("renderToday(__D)");
    const m0 = t.reg["cs-main"].innerHTML;
    const before = m0.slice(m0.indexOf('id="' + id + '"'));
    if (!(before.indexOf(small) >= 0 && before.indexOf(small) < before.indexOf(big)))
      throw new Error(id + ": 前提が崩れている（押す前から金額の大きい順）");
    const btn = new t.El(""); btn.dataset = { k: "amount" };
    Object.keys(t.qsa).forEach((k) => { delete t.qsa[k]; });
    t.qsa["#" + id + " th.sortable button.sort"] = [btn];
    t.R("wire(viewOf('deal', 'today'))");
    if (typeof btn.onclick !== "function") throw new Error(id + " の見出しに操作が付いていない");
    btn.onclick();
    const main = t.reg["cs-main"].innerHTML;
    const tbl = main.slice(main.indexOf('id="' + id + '"'));
    if (!(tbl.indexOf(big) >= 0 && tbl.indexOf(big) < tbl.indexOf(small)))
      throw new Error(id + ": 金額の見出しを押しても並びが変わらない");
  }
});

/* ================================================================ U7 */
check("U7", "継続を追いかけるで拠点を絞ると、MTG の履歴も絞られる", async () => {
  const t = boot();
  const D = customerPayload([deal({ deal_id: "d1", site: "S1" }), deal({ deal_id: "d2", site: "S2" })], {
    mtgs: [{ deal_id: "d1", date: "2025-02-01" }, { deal_id: "d2", date: "2025-03-01" },
           { deal_id: "d2", date: "2025-04-01" }],
  });
  t.R('seriesSite = "S1"');
  const h = t.R("renderSeries")(D);
  if (h.indexOf("1 件中 1 件") >= 0) throw new Error("前提が崩れている");
  if (h.indexOf("MTG の履歴（1 件）") < 0)
    throw new Error("拠点 S1（MTG 1件）に絞っても MTG の履歴が全拠点のまま: " +
      (h.match(/MTG の履歴（[^）]*）/) || [""])[0]);
});

/* ================================================================ U9 */
check("U9", "選択欄を操作して描き直しても、フォーカスがその選択欄に戻る", async () => {
  const t = boot();
  t.ctx.__D = { meta: { flag_counts: [] }, rows: [boardRow({ consultant: "担当A" }), boardRow({ consultant: "担当B" })] };
  t.R('cur = { menu: "deal", view: "board" }; boardCache = __D;');
  const sel = new t.El("bf-consultant");
  t.reg["bf-consultant"] = sel;
  t.R("wire(viewOf('deal', 'board'))");
  sel.focus();
  // 描き直すと DOM が入れ替わる。新しい選択欄は別の要素になる
  const sel2 = new t.El("bf-consultant");
  t.qs["#bf-consultant"] = sel2; t.reg["bf-consultant"] = sel2;
  sel.value = "担当B"; sel.onchange();
  if (!sel2.focused) throw new Error("描き直した後、フォーカスが選択欄に戻らない（body に飛ぶ）");
});

/** 法人番号で見るを開き、法人の選択欄にフォーカスがある状態で選び直すところまで */
async function houjinReselect(t) {
  t.ctx.__idx = houjinIndex(3, 1);
  t.R('customerIndex = __idx; customerHoujin = "H0";');
  t.R('go("deal", "houjin")');
  t.fetched[t.fetched.length - 1].resolve(jsonRes(customerPayload([deal({})])));
  await tick(); await tick();
  const sel = new t.El("cs-houjin"); t.reg["cs-houjin"] = sel;
  t.R("wire(viewOf('deal', 'houjin'))");
  sel.focus();
  sel.value = "H2"; sel.onchange();   // → load()。ここで取り直しに行く
  // 描き直すと DOM が入れ替わる。新しい選択欄は別の要素になる
  const sel2 = new t.El("cs-houjin");
  t.qs["#cs-houjin"] = sel2; t.reg["cs-houjin"] = sel2;
  return sel2;
}
check("U9", "法人を選び直して取り直した後も、フォーカスが法人の選択欄に戻る", async () => {
  const t = boot();
  const sel2 = await houjinReselect(t);
  t.fetched[t.fetched.length - 1].resolve(jsonRes(customerPayload([deal({})])));
  await tick(); await tick(); await tick();
  if (!sel2.focused) throw new Error("取り直した後、フォーカスが法人の選択欄に戻らない");
});
check("U9", "応答を待つ間に別の場所へ動かしたフォーカスを、応答が来ても奪わない", async () => {
  const t = boot();
  const sel2 = await houjinReselect(t);
  const other = new t.El("elsewhere"); other.focus();   // 待つ間に人が別の入力欄へ
  t.fetched[t.fetched.length - 1].resolve(jsonRes(customerPayload([deal({})])));
  await tick(); await tick(); await tick();
  if (sel2.focused || t.doc.activeElement !== other)
    throw new Error("応答が来た瞬間に、前の選択欄へフォーカスを引き戻した");
});

/* ================================================================ U10 */
check("U10", "ログインが切れていたら「ログインし直してください」とリンクを出す", async () => {
  const t = boot();
  t.R('go("study", "phone")');
  const f = t.fetched[t.fetched.length - 1];
  f.resolve({
    ok: true, status: 200, redirected: true, url: "http://test.local/login",
    headers: { get: () => "text/html; charset=utf-8" },
    json: async () => { throw new SyntaxError("Unexpected token '<', \"<!DOCTYPE \"... is not valid JSON"); },
  });
  await tick(); await tick(); await tick();
  const err = t.reg["cs-error"].innerHTML;
  if (err.indexOf("ログインし直してください") < 0 || err.indexOf('href="/login"') < 0)
    throw new Error("ログイン切れの案内が出ない: " + err.slice(0, 120));
  if (err.indexOf("Unexpected token") >= 0) throw new Error("JSON の構文エラーがそのまま出ている");
});

check("U10", "401・JSON 以外の応答・本部アプローチの取得でも、ログイン切れ／原因を出す", async () => {
  const html = { get: () => "text/html; charset=utf-8" };
  const badJson = async () => { throw new SyntaxError("Unexpected token '<', \"<!DOCTYPE \"... is not valid JSON"); };
  // ① 401（リダイレクトされずに返る形）
  {
    const t = boot();
    t.R('go("study", "phone")');
    t.fetched[t.fetched.length - 1].resolve({ ok: false, status: 401, redirected: false,
      url: "http://test.local/api/consulting/phone", headers: { get: () => "application/json" },
      json: async () => ({}) });
    await tick(); await tick(); await tick();
    if (t.reg["cs-error"].innerHTML.indexOf("ログインし直してください") < 0)
      throw new Error("401 でログイン切れの案内が出ない");
  }
  // ② JSON 以外（プロキシのエラーページなど。リダイレクトではない 502）
  {
    const t = boot();
    t.R('go("study", "phone")');
    t.fetched[t.fetched.length - 1].resolve({ ok: false, status: 502, redirected: false,
      url: "http://test.local/api/consulting/phone", headers: html, json: badJson });
    await tick(); await tick(); await tick();
    const err = t.reg["cs-error"].innerHTML;
    if (err.indexOf("Unexpected token") >= 0 || err.indexOf("JSON 以外") < 0)
      throw new Error("JSON 以外の応答で、構文エラーの文がそのまま出る: " + err.slice(0, 120));
  }
  // ③ 本部アプローチ（法人番号で見るの下の遅延読み）
  {
    const t = boot();
    const box = new t.El("hq-box"); t.reg["hq-box"] = box;
    t.ctx.__D = customerPayload([deal({})]);
    t.R("lastPayload = __D; hqCache = null;");
    t.R("wireHoujin()");
    const f = t.fetched[t.fetched.length - 1];
    if (f.url.indexOf("/api/consulting/headquarters") !== 0) throw new Error("本部アプローチを取りに行っていない");
    f.resolve({ ok: true, status: 200, redirected: true, url: "http://test.local/login",
      headers: html, json: badJson });
    await tick(); await tick(); await tick();
    if (box.innerHTML.indexOf("ログインし直してください") < 0)
      throw new Error("本部アプローチでログイン切れの案内が出ない: " + box.innerHTML.slice(0, 120));
  }
});

/* ================================================================ U11 */
const KPI_LAST_EXP = /<span class="lbl">最終満了<\/span><span class="big">([^<]*)<\/span>(<span class="fine">([^<]*)<\/span>)?/;
check("U11", "法人の KPI「最終満了」がチェックに追従し、LTV の食い違いに理由が付く", async () => {
  const t = boot();
  const D = customerPayload([
    deal({ deal_id: "d1", expiration: "2025-06-30", amount: 1000000 }),
    deal({ deal_id: "d2", expiration: "2026-12-31", amount: 2000000 }),
  ]);
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  t.R("houjinIds(__D); houjinPick.d2 = false;");
  const h = t.R("renderHoujin(__D)");
  const kp = h.slice(h.indexOf('<div class="kpis">'));
  // 🔴 KPI の大きな数字だけを見る。一覧の値（2027-01-31）は補足の注記に出るようになったので、
  //    「KPI の塊に 2027-01-31 が無いこと」では見られなくなった
  const big = (kp.match(KPI_LAST_EXP) || [])[1];
  if (big !== "2025-06-30") throw new Error("d2 を外しても最終満了が動かない: " + big);
  if (kp.indexOf("オプション契約") < 0) throw new Error("一覧の LTV と違う理由が書かれていない");
});
check("U11", "全部選んでも一覧の「最終満了」と違うとき（オプション契約の方が遅い）、理由を書く", async () => {
  const t = boot();
  // 一覧（CS_顧客）の last_expiration 2027-01-31 はオプション契約の満了日、という形。
  // 画面の取引（本体契約）の最大は 2026-12-31
  const D = customerPayload([
    deal({ deal_id: "d1", expiration: "2025-06-30" }),
    deal({ deal_id: "d2", expiration: "2026-12-31" }),
  ]);
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const m = t.R("renderHoujin(__D)").match(KPI_LAST_EXP);
  if (!m || m[1] !== "2026-12-31") throw new Error("最終満了が本体契約の最大になっていない");
  if (!m[3] || m[3].indexOf("2027-01-31") < 0 || m[3].indexOf("オプション契約") < 0)
    throw new Error("一覧の最終満了（2027-01-31）と違う理由が書かれていない");
  // 一致しているときは注記を出さない
  t.ctx.__D2 = customerPayload([deal({ deal_id: "d1", expiration: "2027-01-31" })]);
  t.R("houjinPick = null;");
  const m2 = t.R("renderHoujin(__D2)").match(KPI_LAST_EXP);
  if (!m2 || m2[3]) throw new Error("一覧と一致しているのに注記が出ている");
});

/* ================================================================ U12 */
check("U12", "別の法人を選んだら「既定で開いています」の注記が消える", async () => {
  const t = boot();
  const sel = new t.El("cs-houjin"); t.reg["cs-houjin"] = sel;
  t.ctx.__idx = houjinIndex(3, 1);
  t.R('cur = { menu: "deal", view: "houjin" }; customerIndex = __idx; customerHoujin = "H0"; customerReason = "取引がいちばん多い法人";');
  t.R("wire(viewOf('deal', 'houjin'))");
  sel.value = "H2"; sel.onchange();
  const h = t.R("custBlocks")(customerPayload([deal({})]), new Set(["head"]));
  if (h.indexOf("この顧客を既定で開いています") >= 0) throw new Error("選び直した後も注記が残る");
});

/* ================================================================ U13 */
check("U13", "拠点が空の取引の選択肢が「すべての拠点」と同じ値にならない", async () => {
  const t = boot();
  const D = customerPayload([deal({ deal_id: "d1", site: "S1" }), deal({ deal_id: "d2", site: "" })]);
  const h = t.R("renderSeries")(D);
  const sel = h.slice(h.indexOf('id="cs-site"'), h.indexOf("</select>"));
  const vals = [...sel.matchAll(/<option value="([^"]*)"/g)].map((m) => m[1]);
  if (vals.length !== 3) throw new Error("選択肢の数が合わない: " + vals.length);
  if (vals.filter((v) => v === "").length !== 1) throw new Error("value が空の選択肢が2つある（拠点が空の取引）");
  const nosite = vals.find((v) => v !== "" && v !== "S1");
  t.ctx.__v = nosite;
  t.R("seriesSite = __v");
  const h2 = t.R("renderSeries")(D);
  if (h2.indexOf("2 件中 1 件") < 0) throw new Error("拠点が空の取引だけに絞れない");
  // 選択肢の文字に内部の値（__no_site__）を出さない
  const texts = [...sel.matchAll(/<option [^>]*>([^<]*)<\/option>/g)].map((m) => m[1]);
  if (texts.some((x) => x.indexOf(nosite) >= 0)) throw new Error("選択肢に内部の値 " + nosite + " がそのまま出る");
  if (!texts.some((x) => x.indexOf("拠点が入っていない") === 0)) throw new Error("拠点が空の選択肢に名前が付いていない");
});

/* ================================================================ N5 */
check("N5", "月次継続率: 結果待ちがある月・n<30 を実線にせず、n=0 の月は点を作らない", async () => {
  const t = boot();
  const D = {
    meta: { today: "2026-09-23" }, population: {},
    // 🔴 2026-07 は「途中にある n=0 の月」で、しかも rate に 0 が入っている形にしてある。
    //    末尾の n=0（2027-02, rate=null）だけだと、!r.denom の条件を消しても rate==null で
    //    点が作られず、見張りが素通りしていた。サーバの rate() は分母0で null を返すので
    //    実データには出ない（可能性の低い形）が、分母0で点を作らない条件そのものを守る
    monthly_retention: { rows: [
      { month: "2026-05", keep: 20, cancel: 15, fill: 5, denom: 40, pending: 0, rate: 50.0 },
      { month: "2026-06", keep: 24, cancel: 12, fill: 4, denom: 40, pending: 2, rate: 60.0 },
      { month: "2026-07", keep: 0, cancel: 0, fill: 0, denom: 0, pending: 3, rate: 0 },
      { month: "2026-08", keep: 7, cancel: 3, fill: 0, denom: 10, pending: 0, rate: 70.0 },
      { month: "2026-09", keep: 30, cancel: 8, fill: 2, denom: 40, pending: 0, rate: 75.0 },
      { month: "2027-02", keep: 0, cancel: 0, fill: 0, denom: 0, pending: 5, rate: null },
    ] },
    by_renewal: [], missingness: [],
  };
  const h = t.R("renderRenewal")(D);
  const svg = h.slice(h.indexOf("<svg"), h.indexOf("</svg>"));
  const solid = count(svg, /<circle [^>]*r="4\.2"/g), hollow = count(svg, /<circle [^>]*r="4\.6"/g);
  if (solid !== 2) throw new Error("確定の点（結果待ち0・n>=30）は2つのはずが " + solid);
  // ループ4: n<30 の月（2026-08, n=10）は点を打たない（規律「n<30 は図に載せない」。前は中空で描いていた）
  if (hollow !== 1) throw new Error("未確定の点（結果待ち2件の月）は1つのはずが " + hollow);
  if (/<circle [^>]*><title>26-08/.test(svg)) throw new Error("n<30 の月（26-08, n=10）に点を打っている");
  if (svg.indexOf(">26-08<") < 0) throw new Error("n<30 の月（26-08）を横軸から消している（月があることは残す）");
  if (h.indexOf("30 件に届かない 1 か月は点を打っていません") < 0 || h.indexOf("2026-08 n=10") < 0)
    throw new Error("n<30 で点を打たなかった月とその理由が書かれていない");
  if (svg.indexOf("27-02") >= 0) throw new Error("n=0 の月（27-02）が図に残っている");
  if (h.indexOf("決着が1件も無い 1 か月") < 0) throw new Error("n=0 で外した月のことが書かれていない");
  // 下の表。図の注記が表へ誘うので、表でも未確定を確定と同じ太字にしない
  const tb = h.slice(h.indexOf("満了月ごとの内訳"));
  const tbl = tb.slice(0, tb.indexOf("</table>"));
  const bold = [...tbl.matchAll(/<b>([\d.]+%)<\/b>/g)].map((m) => m[1]);
  if (bold.join(",") !== "50.0%,75.0%")
    throw new Error("表で太字にしているのが確定の月（50.0% と 75.0%）だけではない: " + bold.join(","));
  if (count(tbl, /未確定（/g) !== 2) throw new Error("表の未確定の月（結果待ち2件・n=10）に「未確定」が付いていない");
  const row07 = tbl.slice(tbl.indexOf("<td>2026-07</td>"), tbl.indexOf("</tr>", tbl.indexOf("<td>2026-07</td>")));
  if (!row07 || row07.indexOf("%") >= 0) throw new Error("n=0 の月（2026-07）の率を 0.0% と出している");
});

/* ================================================================ N8 */
check("N8", "法人の採用数の合計は満了で止め、拠点をまたいで1本の線にしない", async () => {
  const t = boot();
  const pts = (n, v) => Array.from({ length: n }, (_, i) => ({ m: i + 1, v: v, carry: i > 0 }));
  const D = customerPayload([
    deal({ deal_id: "d1", name: "満了した案件", site: "S1", start: "2025-01-01", expiration: "2025-03-31" }),
    deal({ deal_id: "d2", name: "続いている案件", site: "S2", start: "2025-01-01", expiration: "2025-12-31" }),
  ], { monthly: [
    // サーバは満了後も今月まで持ち越して返す（N14）。ここでは 8ヶ月目まで
    { deal_id: "d1", name: "満了した案件", start: "2025-01-01", series: { syoudaku: pts(8, 2) } },
    { deal_id: "d2", name: "続いている案件", start: "2025-01-01", series: { syoudaku: pts(8, 1) } },
  ] });
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const h = t.R("renderHoujin(__D)");
  // 🔴 題名を「採用数の月ごとの合計」から変えた（柱が月々の採用数ではなく累計だと分かるように）
  const a = h.indexOf("契約中の案件の採用数（累計）の月ごとの合計");
  if (a < 0) throw new Error("題名に「累計」が入っていない（月々の採用数に読める）");
  const figH = h.slice(a, h.indexOf("</figure>", a));
  // 柱の区間ごとに <title>月 拠点: 値</title> が付く。満了後の月に S1 の区間があってはいけない
  if (/<title>25-0[4-8] S1: /.test(figH) || !/<title>25-03 S1: 2/.test(figH))
    throw new Error("満了した契約（S1, 2025-03 満了）の採用数が 25-06 にも足されている");
  if (!/<title>25-06 S2: 1/.test(figH)) throw new Error("25-06 の合計が S2 の 1 だけになっていない");
  if (/<path d="M[^"]*L[^"]*"[^>]*stroke-width="2\.2"/.test(figH))
    throw new Error("拠点をまたいだ合計を1本の線でつないでいる");
  // 引き継ぎ（carry）の区間は中空・破線、書き換えのあった月は塗り
  const rectOf = (lab) => (figH.match(new RegExp("<rect [^>]*>\\s*<title>" + lab)) || [""])[0];
  if (rectOf("25-06 S2: 1").indexOf('stroke-dasharray="3 2"') < 0)
    throw new Error("持ち越した値（25-06 の S2）が中空・破線になっていない");
  if (rectOf("25-01 S2: 1").indexOf("stroke-dasharray") >= 0)
    throw new Error("書き換えのあった月（25-01 の S2）まで中空になっている");
  // 凡例の「引き継ぎ」を「その他の拠点」と同じ灰色（--ghost）にしない
  const lgCarry = (figH.match(/<i><svg(?:(?!<\/i>).)*<\/svg>引き継ぎ/) || [""])[0];
  if (!lgCarry || lgCarry.indexOf("--ghost") >= 0)
    throw new Error("凡例の「引き継ぎ」が「その他の拠点」と同じ色");
});
check("N8", "同じ拠点・同じ月に、書き換えのあった契約と持ち越した契約が混ざったら分けて描く", async () => {
  const t = boot();
  const D = customerPayload([
    deal({ deal_id: "d1", site: "S1", start: "2025-01-01", expiration: "2025-12-31" }),
    deal({ deal_id: "d2", site: "S1", start: "2025-01-01", expiration: "2025-12-31" }),
  ], { monthly: [
    // d1 は 3ヶ月目に書き換え（3）。d2 は 1ヶ月目の 2 を持ち越している
    { deal_id: "d1", name: "a", start: "2025-01-01", series: { syoudaku: [{ m: 1, v: 1 }, { m: 3, v: 3 }] } },
    { deal_id: "d2", name: "b", start: "2025-01-01", series: { syoudaku: [{ m: 1, v: 2 }] } },
  ] });
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const h = t.R("renderHoujin(__D)");
  const a = h.indexOf("契約中の案件の採用数（累計）の月ごとの合計");
  const figH = h.slice(a, h.indexOf("</figure>", a));
  const r3 = [...figH.matchAll(/<rect ([^>]*)>\s*<title>25-03 S1: (\d+)/g)].map((m) => [m[2], /stroke-dasharray/.test(m[1])]);
  // 25-03: 書き換えのあった d1 の 3 は塗り、持ち越しの d2 の 2 は中空。合わせて 1本の塗りの 5 にしない
  if (JSON.stringify(r3) !== JSON.stringify([["3", false], ["2", true]]))
    throw new Error("25-03 の S1 が書き換え（塗り 3）と持ち越し（中空 2）に分かれていない: " + JSON.stringify(r3));
});
check("N8", "色を付ける拠点を最後の月の値で選ばない（満了した拠点を「その他」に回さない）", async () => {
  const t = boot();
  // OLD は 2025-03 に満了したが 10人採っている。A〜E は今も続いていて 1人ずつ
  const ds = [deal({ deal_id: "old", site: "OLD", start: "2025-01-01", expiration: "2025-03-31" })];
  const mm = [{ deal_id: "old", name: "old", start: "2025-01-01", series: { syoudaku: [{ m: 1, v: 10 }] } }];
  for (const k of ["A", "B", "C", "D", "E"]) {
    ds.push(deal({ deal_id: k, site: k, start: "2025-01-01", expiration: "2025-12-31" }));
    mm.push({ deal_id: k, name: k, start: "2025-01-01", series: { syoudaku: [{ m: 1, v: 1 }, { m: 8, v: 1 }] } });
  }
  t.ctx.__D = customerPayload(ds, { monthly: mm });
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const h = t.R("renderHoujin(__D)");
  const a = h.indexOf("契約中の案件の採用数（累計）の月ごとの合計");
  const figH = h.slice(a, h.indexOf("</figure>", a));
  if (!/<title>25-01 OLD: 10<\/title>/.test(figH))
    throw new Error("10人採った満了済みの拠点 OLD が「その他の拠点」にまとめられている");
});

/* ================================================================ N10 */
check("N10", "ファネルの前段比は両方の値がある取引だけで割る", async () => {
  const t = boot();
  const D = customerPayload([
    deal({ deal_id: "d1", oubo: 100, mensetu: null, syoudaku: null }),
    deal({ deal_id: "d2", oubo: 10, mensetu: 8, syoudaku: 2 }),
    deal({ deal_id: "d3", oubo: null, mensetu: 50, syoudaku: 5 }),
    // 🔴 面接はあるが採用が入っていない取引。これが無いと採用÷面接は合計どうしでも
    //    7/58 になり、前段比のやり方を区別できなかった（pair を外しても通っていた）
    deal({ deal_id: "d4", oubo: null, mensetu: 40, syoudaku: null }),
  ]);
  const F2 = t.R("custFilter")(D, new Set(["d1", "d2", "d3", "d4"]));
  const h = t.R("custBlocks")(F2, new Set(["funnel"]));
  // 面接÷応募: 両方あるのは d2 だけ → 8/10 = 80.0%（合計どうしなら 98/110 = 89.1%）
  // 採用÷面接: 両方あるのは d2,d3 → 7/58 = 12.1%（合計どうしなら 7/98 = 7.1%）
  if (h.indexOf("前段の 80.0%") < 0) throw new Error("面接÷応募が 80.0% になっていない");
  if (h.indexOf("前段の 12.1%") < 0) throw new Error("採用÷面接が 12.1% になっていない");
});

/* ================================================================ V5 */
check("V5", "電話・MTG の月次で、途中の今月を中空＋破線で描く", async () => {
  const t = boot();
  const mtg = t.R("renderMtgQ")({
    meta: { n_mtg: 3, today: "2026-09-23", source_as_of: "2026-09-20 22:03:50" },
    linked: { n: 1, rate: 33 }, filled: [], filled_note: "", risk_dist: [], hosts: [],
    monthly: [{ month: "2026-07", n: 30 }, { month: "2026-08", n: 32 }, { month: "2026-09", n: 12 }],
  });
  const a1 = mtg.indexOf("<svg", mtg.indexOf("MTG の実施回数"));
  const s1 = mtg.slice(a1, mtg.indexOf("</svg>", a1));
  if (count(s1, /<circle [^>]*r="4\.6"/g) !== 1 || count(s1, /stroke-dasharray="5 4"/g) !== 1)
    throw new Error("MTG の今月（2026-09）が中空・破線になっていない");
  const ph = t.R("renderPhone")({
    meta: { n_active: 10, threshold_sec: 60, today: "2026-09-23", source_as_of: "2026-09-20 22:03:50" },
    reach: { no_call: 1, no_contact: 2, no_call_rate: 10, no_contact_rate: 20, note: "" },
    days_since: null, transcript: { rate: 1, n: 1, rows: 10, note: "" },
    silent: { n: 0, rule: "", rows: [] },
    monthly: [{ month: "2026-08", calls: 40, contacts: 20 }, { month: "2026-09", calls: 9, contacts: 4 }],
  });
  const s2 = ph.slice(ph.indexOf("<svg", ph.indexOf("月ごとの本数")));
  if (count(s2.slice(0, s2.indexOf("</svg>")), /<circle [^>]*r="4\.6"/g) !== 2)
    throw new Error("電話の今月（通話・接触の2系列）が中空になっていない");
});

check("V5", "締まっていない月は基準日（today）ではなく元データの時刻（source_as_of）で決める", async () => {
  const t = boot();
  // 元データは 8月末に落としたまま、基準日だけ 9月に進んだ形。8月も途中の値
  const meta = { n_mtg: 3, today: "2026-09-01", source_as_of: "2026-08-20 22:03:50" };
  const mtg = t.R("renderMtgQ")({
    meta: meta, linked: { n: 1, rate: 33 }, filled: [], filled_note: "", risk_dist: [], hosts: [],
    monthly: [{ month: "2026-06", n: 30 }, { month: "2026-07", n: 32 }, { month: "2026-08", n: 20 }],
  });
  const a1 = mtg.indexOf("<svg", mtg.indexOf("MTG の実施回数"));
  const s1 = mtg.slice(a1, mtg.indexOf("</svg>", a1));
  if (count(s1, /<circle [^>]*r="4\.6"/g) !== 1)
    throw new Error("元データの月（2026-08）が中空になっていない（基準日の月で決めている）");
  if (mtg.indexOf("2026-08 は元データを落とした時点までの途中の値") < 0)
    throw new Error("途中の月の注記が 2026-08 になっていない");
  // 途中の月が横軸に無いときは、凡例にも「途中の月」を出さない
  const mtg2 = t.R("renderMtgQ")({
    meta: meta, linked: { n: 1, rate: 33 }, filled: [], filled_note: "", risk_dist: [], hosts: [],
    monthly: [{ month: "2026-06", n: 30 }, { month: "2026-07", n: 32 }],
  });
  if (mtg2.indexOf("途中の月（まだ締まっていない）") >= 0) throw new Error("MTG: 図に無い「途中の月」が凡例に残る");
  const ph = t.R("renderPhone")({
    meta: { n_active: 10, threshold_sec: 60, today: "2026-09-01", source_as_of: "2026-08-20 22:03:50" },
    reach: { no_call: 1, no_contact: 2, no_call_rate: 10, no_contact_rate: 20, note: "" },
    days_since: null, transcript: { rate: 1, n: 1, rows: 10, note: "" },
    silent: { n: 0, rule: "", rows: [] },
    monthly: [{ month: "2026-06", calls: 40, contacts: 20 }, { month: "2026-07", calls: 9, contacts: 4 }],
  });
  if (ph.indexOf("途中の月（まだ締まっていない）") >= 0) throw new Error("電話: 図に無い「途中の月」が凡例に残る");
});

/* ================================================================ V6 */
check("V6", "n<30 で図から外した群の件数と理由を書く", async () => {
  const t = boot();
  const h = t.R("renderRampup")({
    meta: {}, phase: { rows: [], rule: "" },
    first_mtg: { n: 100, pre_contract: 0, stats: null, buckets: [
      // サーバは解約率の分母を決着済み（denom）で返す（fix/cs-rust N3）。足切りも denom で見る。
      // n（結果待ちを含む）は 30 以上でも denom が 30 未満なら外れることを見る
      { label: "14日以内", n: 60, denom: 50, cancel_rate: 40 }, { label: "61日超", n: 40, denom: 12, cancel_rate: 70 }] },
    no_mtg: { n: 0, first_active: 0, rate: null, note: "", rows: [] },
  });
  if (!/n が 30 件未満の 1 群を図から外しています[^<]*<\/b>（61日超 n=12）/.test(h))
    throw new Error("立ち上がりの帯で外した「61日超 n=12」が書かれていない");
  const o = t.R("renderOutcome")({
    meta: {},
    goal_act: { bands: [], fill_rate: 0, has_goal: 0, pop: 0, median: null }, goal_all: {},
    efficiency: { groups: [
      { label: "継続済", box: { n: 80, median: 5, q1: 2, q3: 9, min: 0, max: 30, mean: 6 } },
      { label: "解約", box: { n: 7, median: 3, q1: 1, q3: 5, min: 0, max: 8, mean: 3 } }],
      caveat: "", has_keisaisu_act: 0, n_act: 0 },
    risk: { bands: [], n_act: 0, ax3: {}, ax4: {}, top: [], order_note: "" },
    contact_source: {},
  });
  if (!/（解約 n=7）/.test(o)) throw new Error("応募効率で外した「解約 n=7」が書かれていない");
});

/* ================================================================ V7 */
check("V7", "採用単価の悪化の図を20件で切ったら必ず注記する", async () => {
  const t = boot();
  const rows = Array.from({ length: 35 }, (_, i) => ({ site: "拠点" + i, prev: 100, last: 200, ratio: 2 }));
  const h = t.R("renderFocus")({
    meta: {},
    nps_low: { threshold: 4, n: 0, n_have_nps: 0, coverage: null, n_act: 0, dist: [], rows: [], note: "" },
    // サーバは 60件で切ったときだけ truncated を立てる。35件なら false
    cpa: { worse: 35, judged: 80, skipped_censored: 0, truncated: false, rows: rows, note: "" },
    mtg_layers: { both: 0, only_recording: 0, only_mail: 0, neither: 0, n_act: 0, note: "",
                  fact_recording: {}, estimated_mail: {} },
    shape: { ltv: null, display_label: "", n_all: 0, n_display: 0, multi_site: 0, multi_site_note: "" },
  });
  if (!/上位 20 拠点だけ出しています（悪化した拠点は全 35 拠点）/.test(h))
    throw new Error("35拠点のうち20だけ描いているのに注記が無い");
});

/* ================================================================ V8 */
check("V8", "今日動く先の図の注記に件数を直書きしない", async () => {
  const t = boot();
  const h = t.R("renderToday")(todayPayload([boardRow({ flags: ["a"] }), boardRow({ flags: ["b"] }),
                                             boardRow({ flags: ["a"] })]));
  if (h.indexOf("24件") >= 0) throw new Error("「24件」と直書きしている（3件の日）");
  if (h.indexOf("この 3 件だけの内訳") < 0) throw new Error("実際の件数が出ていない");
});

/* ================================================================ V18 */
check("V18", "図の凡例に取引名をエスケープして入れる", async () => {
  const t = boot();
  // 🔴 短い名前にする。shortName() は16〜18文字を超えると頭を削るので、
  //    長い攻撃文字列だと「<img」が削られて、直す前の版でも見張りが通ってしまう
  const evil = "<u>x</u>";
  const D = customerPayload([deal({ deal_id: "d1", name: evil, start: "2025-01-01" })], {
    monthly: [{ deal_id: "d1", name: evil, start: "2025-01-01",
                series: { syoudaku: [{ m: 1, v: 1 }, { m: 2, v: 2 }] } }],
  });
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const h = t.R("renderHoujin(__D)");
  if (h.indexOf(evil) >= 0) throw new Error("取引名が HTML のまま画面に入る");
  if (h.indexOf("&lt;u&gt;x&lt;/u&gt;") < 0) throw new Error("取引名が画面に出ていない（前提が崩れている）");
});

/* ================================================================ V19 */
check("V19", "thick: true の印が stroke-width=\"true\" にならない", async () => {
  const t = boot();
  const s = t.R("svgTimeline")({ lanes: [{ label: "a",
    marks: [{ d: "2026-01-10", thick: true }, { d: "2026-03-10" }] }] });
  if (s.indexOf('stroke-width="true"') >= 0) throw new Error('stroke-width="true" が出ている');
  const ws = [...s.matchAll(/<line [^>]*stroke-width="([\d.]+)" stroke-linecap/g)].map((m) => +m[1]);
  if (!(ws.length === 2 && ws[0] > ws[1])) throw new Error("太い印が普通の印より太くない: " + ws.join(","));
});

/* ================================================================ N18b */
// 2026-09-23 実機: 「7 / 6 か月目」「12 / 12」、推移は「契約 6ヶ月」なのに横軸が「7ヶ月」まで。
// 暦の月で数えていたのが原因（サーバの contract_month に実測）。案件一覧はサーバが契約の月で
// 数え直し、満了日を過ぎてもまだ稼働中のものに past_expiry を付ける。推移は横軸を暦の月で出す。
check("N18b", "満了日を過ぎた稼働中の案件は「何ヶ月目」を「満了後」と出す（15 / 6 と出さない）", async () => {
  const t = boot();
  const pos = t.R("pos");
  const past = pos({ months: 15, period: 6, past_expiry: true, band: "終盤" });
  if (past.indexOf("満了後") < 0) throw new Error("満了後と出ていない: " + past);
  if (/15/.test(past) || / \/ 6/.test(past)) throw new Error("期間を超えた月の数字が出ている: " + past);
  if (past.indexOf("契約 6 か月") < 0) throw new Error("契約期間が添えられていない: " + past);
  const mid = pos({ months: 6, period: 6, past_expiry: false });
  if (mid.indexOf("6 / 6") < 0 || mid.indexOf("満了後") >= 0) throw new Error("満了前の表示が変わった: " + mid);
  // 一覧の表でもこの列に出る
  const h = t.R("renderBoard")({ meta: { flag_counts: [], n_active: 1 },
    rows: [boardRow({ deal_id: "p", months: 15, period: 6, past_expiry: true, days_left: -300 })] });
  if (h.indexOf("満了後") < 0) throw new Error("案件一覧の表に「満了後」が出ていない");
});
check("N18b", "推移の横軸は暦の月で出し、契約期間より多い月にまたがる理由を書く", async () => {
  const t = boot();
  // fixture にある形: 6ヶ月契約 2026-03-19〜2026-09-18（暦では 3月〜9月の7か月）
  const pts = Array.from({ length: 7 }, (_, i) => ({ m: i + 1, v: 10 + i, carry: false }));
  const D = customerPayload([deal({ deal_id: "d1", start: "2026-03-19", expiration: "2026-09-18" })], {
    monthly: [{ deal_id: "d1", name: "案件", start: "2026-03-19", expiration: "2026-09-18",
      period: 6, span_months: 7, series: { oubo: pts }, nps: {} }],
  });
  const h = t.R("renderSeries")(D);
  const a = h.indexOf(" の推移");
  const fig1 = h.slice(a, h.indexOf("</figure>", a));
  if (/\d+ヶ月</.test(fig1) || fig1.indexOf(">7ヶ月<") >= 0)
    throw new Error("横軸に「Nヶ月」が残っている（契約 6ヶ月と食い違う）");
  if (fig1.indexOf(">26-03<") < 0 || fig1.indexOf(">26-09<") < 0)
    throw new Error("横軸が暦の月（26-03〜26-09）になっていない");
  if (fig1.indexOf("横軸は暦の月") < 0 || fig1.indexOf("暦では 7 か月にまたがります") < 0)
    throw new Error("期間 6 と 7 か月の違いの理由が書かれていない");
  const b = h.indexOf("系列を縦に並べる");
  const fig2 = h.slice(b, h.indexOf("</figure>", b));
  if (/>\d+ヶ月</.test(fig2) || fig2.indexOf(">26-09<") < 0)
    throw new Error("系列を縦に並べる図の横軸が暦の月になっていない");
  // 月の頭に始まる契約（期間と暦の月数が同じ）には理由の文を付けない
  const D2 = customerPayload([deal({ deal_id: "d2", start: "2026-04-01" })], {
    monthly: [{ deal_id: "d2", name: "案件", start: "2026-04-01", expiration: "2026-09-30",
      period: 6, span_months: 6, series: { oubo: pts.slice(0, 6) }, nps: {} }],
  });
  const h2 = t.R("renderSeries")(D2);
  if (h2.indexOf("またがります") >= 0) throw new Error("期間と暦の月数が同じなのに理由の文が出る");
});
check("histGap", "推移の図で、契約の頭に記録が無い月を図の副題に1回だけ書く（描いた画面で見る）", async () => {
  const t = boot();
  // fixture にある形: 2025-03-09 開始の契約で、記録が 5 か月目（2025-07）からしか無い
  const pts = [{ m: 5, v: 12, carry: false }, { m: 6, v: 14, carry: false }];
  const D = customerPayload([deal({ deal_id: "g1", start: "2025-03-09", expiration: "2025-09-08" })], {
    monthly: [{ deal_id: "g1", name: "案件", start: "2025-03-09", expiration: "2025-09-08",
      period: 6, span_months: 7, series: { oubo: pts }, nps: {} }],
  });
  const h = t.R("renderSeries")(D);
  const a = h.indexOf(" の推移");
  const fig1 = h.slice(a, h.indexOf("</figure>", a));
  if (fig1.indexOf("記録は 2025-07 からです") < 0 || fig1.indexOf("2025-03〜2025-06") < 0)
    throw new Error("推移の図に、記録が無い期間（2025-03〜2025-06）の断り書きが出ていない");
  const times = h.split("記録は 2025-07 からです").length - 1;
  if (times !== 1) throw new Error("同じ断り書きが1つの契約で " + times + " 回出ている（1回にする）");
});

/* ================================================================ 法人の母数 */
// 2026-09-23 実機: 法人番号で見る画面に「全 1,649 法人」（注力の注記）と「全 1,646 法人」
// （本部アプローチ）が並んでいた。差の3法人はオプション契約しか持たない法人（fixture 実測）。
check("法人数", "注力の注記と本部アプローチの「全 N 法人」が同じ母数を使う", async () => {
  const t = boot();
  const f = { n_all: 1649, n_houjin: 1646, n_houjin_option_only: 3, n_display: 517, n_focus: 116,
    n_focus_all: 223, monthly_over_300k: 58, enterprise: 41, multi_site: 46,
    display_label: "稼働中の取引を持つ法人", rule: "", not_layer: "" };
  const h = t.R("focusSection")(customerPayload([], { focus: f }));
  if (h.indexOf("全 1,649 法人") >= 0) throw new Error("CS_顧客 の行数（1,649）を「全 N 法人」に出している");
  if (h.indexOf("全 1,646 法人まで広げると注力は 223 社です") < 0)
    throw new Error("本部アプローチと同じ 1,646 になっていない");
  // 🔴 文言は「3 法人は数えていません」から「3 法人を除いた全 1,646 法人」に変えた。
  //    図の母数（517）の側にもオプション契約しか持たない法人がいるので、どちらの母数の話かを文の中で分ける
  if (h.indexOf("オプション契約しか持たない 3 法人を除いた全 1,646 法人") < 0)
    throw new Error("外した3法人のことが書かれていない");
  const hq = t.R("renderHq")({ meta: { n_houjin: 1646, n_houjin_option_only: 3, today: "2026-09-18",
    not_counted: "", cpa_rule: "", cancel_rule: "" }, multi_site: 194, truncated: false, rows: [] });
  if (hq.indexOf("全 1,646 法人。オプション契約しか持たない 3 法人は除く") < 0)
    throw new Error("本部アプローチの「全 N 法人」に除いた法人のことが書かれていない");
});

/* ================================================================ N18c */
// N18b の検証で出た残り。月末に始まった契約（3/31〜9/30 など）が満了日の当日だけ「7 / 6」と
// 出ていたのはサーバ（contract_month）で直した。画面側は、満了日が後ろにずれて期間を超える行、
// 満了後の行の並べ替え、推移の横に書く理由の選び方、注力の図の母数を直した。
check("N18c", "満了日が後ろにずれて期間を超えた行は「2 / 1」と出さず、ずれていると書く", async () => {
  const t = boot();
  const pos = t.R("pos");
  // fixture 62465528145: 1ヶ月契約 2026-06-01〜2026-07-31。7/1〜7/31 は2ヶ月目だが満了前
  const late = pos({ months: 2, period: 1, past_expiry: false, band: "終盤" });
  if (late.indexOf("2 / 1") >= 0) throw new Error("期間を超えた分数が出ている: " + late);
  if (late.indexOf("満了日が後ろにずれています") < 0 || late.indexOf("契約 1 か月") < 0)
    throw new Error("満了日がずれていることが書かれていない: " + late);
  const ok = pos({ months: 1, period: 1, past_expiry: false });
  if (ok.indexOf("1 / 1") < 0) throw new Error("期間内の表示が変わった: " + ok);
});
check("N18c", "「何ヶ月目」の並べ替えで、満了後の行は内部の月数ではなく満了を過ぎた日数で後ろに並ぶ", async () => {
  const t = boot();
  const rows = [
    boardRow({ deal_id: "a", name: "満了後300日", months: 15, period: 6, past_expiry: true, days_left: -300 }),
    boardRow({ deal_id: "b", name: "期間内10", months: 10, period: 12, past_expiry: false, days_left: 60 }),
    boardRow({ deal_id: "c", name: "満了後5日", months: 8, period: 6, past_expiry: true, days_left: -5 }),
  ];
  const order = (asc) => {
    const h = t.R("boardTable")(rows, { key: "months", asc: asc }, "x");
    return ["満了後300日", "期間内10", "満了後5日"]
      .map((n) => [n, h.indexOf(n)]).sort((p, q) => p[1] - q[1]).map((p) => p[0]).join(",");
  };
  // 内部の months（15 / 10 / 8）で並べると 満了後300日, 期間内10, 満了後5日 になる
  if (order(false) !== "満了後300日,満了後5日,期間内10") throw new Error("大きい順: " + order(false));
  if (order(true) !== "期間内10,満了後5日,満了後300日") throw new Error("小さい順: " + order(true));
});
check("N18c", "推移の横に書く理由は満了日を比べて選ぶ（1日開始のずれに「月の途中」と書かない）", async () => {
  const t = boot();
  const sh = t.R("spanHint");
  const mid = sh({ start: "2026-03-19", expiration: "2026-09-18", std_expiration: "2026-09-18",
    period: 6, span_months: 7 });
  if (mid.indexOf("月の途中に始まったので") < 0) throw new Error("月の途中の開始: " + mid);
  // fixture 62465528145: 1日に始まり、満了日が1か月後ろ（span = 期間 + 1）
  const late1 = sh({ start: "2026-06-01", expiration: "2026-07-31", std_expiration: "2026-06-30",
    period: 1, span_months: 2 });
  if (late1.indexOf("月の途中") >= 0) throw new Error("1日の開始なのに月の途中と書いている: " + late1);
  if (late1.indexOf("（2026-06-30）より後ろ") < 0) throw new Error("後ろにずれていると書いていない: " + late1);
  // 満了日が2か月以上後ろ（span > 期間 + 1）
  const late2 = sh({ start: "2025-01-10", expiration: "2025-12-09", std_expiration: "2025-07-09",
    period: 6, span_months: 12 });
  if (late2.indexOf("より後ろにあり、暦では 12 か月") < 0) throw new Error("2か月以上後ろ: " + late2);
  // fixture 15873848622: 12ヶ月契約なのに 2025-12-18〜2026-06-17（span < 期間）
  const early = sh({ start: "2025-12-18", expiration: "2026-06-17", std_expiration: "2026-12-17",
    period: 12, span_months: 7 });
  if (early.indexOf("（2026-12-17）より前にあり、暦では 7 か月") < 0) throw new Error("期間より前: " + early);
  // 月の途中の開始で、またがる月数がちょうど期間になる（満了日が前）
  const early2 = sh({ start: "2026-03-19", expiration: "2026-08-10", std_expiration: "2026-09-18",
    period: 6, span_months: 6 });
  if (early2.indexOf("より前にあり") < 0) throw new Error("span = 期間 でも満了日が前: " + early2);
  const plain = sh({ start: "2026-04-01", expiration: "2026-09-30", std_expiration: "2026-09-30",
    period: 6, span_months: 6 });
  if (plain.indexOf("またがります") >= 0) throw new Error("期間どおりなのに理由の文が出る: " + plain);
});
check("N18c", "満了日が数日ずれただけの取引に、ずれを理由として書かない（月の途中の開始が理由, F3）", async () => {
  const t = boot();
  const sh = t.R("spanHint");
  // 2025-12-18 開始の12ヶ月契約で、満了日が標準（2026-12-17）より3日前。暦では 13 か月にまたがるが、
  // 理由は月の途中の開始。満了日のずれで月の数は変わらない（標準の満了日でも 13 か月）
  const few = sh({ start: "2025-12-18", expiration: "2026-12-14", std_expiration: "2026-12-17",
    period: 12, span_months: 13 });
  if (few.indexOf("より前にあり") >= 0) throw new Error("数日のずれを理由にしている: " + few);
  if (few.indexOf("月の途中に始まったので、暦では 13 か月にまたがります") < 0)
    throw new Error("月の途中の開始という本当の理由が書かれていない: " + few);
  // 数日後ろにずれただけ（同じ月の中）も同じ
  const fewLate = sh({ start: "2026-03-19", expiration: "2026-09-25", std_expiration: "2026-09-18",
    period: 6, span_months: 7 });
  if (fewLate.indexOf("より後ろにあり") >= 0) throw new Error("数日のずれを理由にしている: " + fewLate);
  if (fewLate.indexOf("月の途中に始まったので") < 0) throw new Error("月の途中の開始が書かれていない: " + fewLate);
  // 数日のずれでも月をまたいで月の数が変わったときは、ずれが理由（1日開始・満了が翌月2日）
  const cross = sh({ start: "2026-04-01", expiration: "2026-10-02", std_expiration: "2026-09-30",
    period: 6, span_months: 7 });
  if (cross.indexOf("（2026-09-30）より後ろにあり、暦では 7 か月") < 0) throw new Error("月の数を変えたずれ: " + cross);
});
check("N18c", "注力の図の母数にオプション契約だけの法人が入っていることを書く", async () => {
  const t = boot();
  const f = { n_all: 1649, n_houjin: 1646, n_houjin_option_only: 3, n_display: 517,
    n_display_option_only: 1, n_focus: 116, n_focus_all: 223, monthly_over_300k: 58, enterprise: 41,
    multi_site: 46, display_label: "稼働中の取引を持つ法人", rule: "", not_layer: "" };
  const h = t.R("focusSection")(customerPayload([], { focus: f }));
  if (h.indexOf("この 517 社には、オプション契約しか持たない法人 1 社も入っています") < 0)
    throw new Error("図の母数にオプション契約だけの法人がいることが書かれていない");
  if (h.indexOf("数えていません") >= 0) throw new Error("図にも掛かって読める「数えていません」が残っている");
});

/* ================================================================ ループ4: 文言と表（2026-09-24 実機） */
check("L4", "series: 「1つの縦軸に重ねていません」は契約ごとに繰り返さず、最初の図の下の1回だけ", async () => {
  const t = boot();
  const pts = [{ m: 1, v: 3, carry: false }, { m: 2, v: 5, carry: false }];
  const mm = (id) => ({ deal_id: id, name: "案件" + id, start: "2026-04-01", expiration: "2026-09-30",
    period: 6, span_months: 6, series: { oubo: pts }, nps: {} });
  const D = customerPayload([deal({ deal_id: "a" }), deal({ deal_id: "b" }), deal({ deal_id: "c" })],
    { monthly: [mm("a"), mm("b"), mm("c")] });
  const h = t.R("renderSeries")(D);
  if (count(h, /系列を縦に並べる/g) < 3) throw new Error("契約ごとの図が3つ描かれていない（見張りの前提）");
  const n = count(h, /1つの縦軸に重ねていません/g);
  if (n !== 1) throw new Error("「1つの縦軸に重ねていません」の段落が " + n + " 回出ている（1回にする）");
});
check("L4", "series: NPS と接触がある契約の副題で、例文を結論のように書かない（読み方の例と言う）", async () => {
  const t = boot();
  const pts = [{ m: 1, v: 3, carry: false }, { m: 2, v: 5, carry: false }];
  const D = customerPayload([deal({ deal_id: "n1", start: "2026-04-01" })], {
    monthly: [{ deal_id: "n1", name: "案件", start: "2026-04-01", expiration: "2026-09-30",
      period: 6, span_months: 6, series: { oubo: pts }, nps: { nps: [{ m: 1, v: 6 }, { m: 2, v: 9 }] } }],
    contacts: [{ deal_id: "n1", dates: ["2026-04-10"] }],
  });
  const h = t.R("renderSeries")(D);
  const b = h.indexOf("系列を縦に並べる");
  const head = h.slice(b, h.indexOf("<svg", b));
  if (head.indexOf("が読めます") >= 0 && head.indexOf("接触が切れていて、応募も止まっていた」が読めます") >= 0)
    throw new Error("NPS が上がった契約にも「NPS が落ちた月に…が読めます」と結論のように出ている");
  if (head.indexOf("読み方の例") < 0) throw new Error("副題の例文に「読み方の例」と書いていない");
});
check("L4", "series: 契約の図が1つだけのときは「下に続く契約の図も同じです」と書かない", async () => {
  const t = boot();
  const pts = [{ m: 1, v: 3, carry: false }, { m: 2, v: 5, carry: false }];
  const mm = (id) => ({ deal_id: id, name: "案件" + id, start: "2026-04-01", expiration: "2026-09-30",
    period: 6, span_months: 6, series: { oubo: pts }, nps: {} });
  // 変更履歴の無い契約（図にしない）が並んでいても、図が1つなら下には何も続かない
  const one = t.R("renderSeries")(customerPayload([deal({ deal_id: "a" }), deal({ deal_id: "z" })],
    { monthly: [mm("a"), { deal_id: "z", name: "案件z", start: "2026-04-01", series: {}, nps: {} }] }));
  if (one.indexOf("案件a — 系列を縦に並べる") < 0 || one.indexOf("案件z — 系列を縦に並べる") >= 0)
    throw new Error("契約ごとの図が1つ（案件a だけ）になっていない（見張りの前提）");
  if (count(one, /1つの縦軸に重ねていません/g) !== 1) throw new Error("図が1つのときに理由の段落が出ていない");
  if (one.indexOf("下に続く契約の図も同じです") >= 0) throw new Error("図が1つなのに「下に続く契約の図も同じです」と書いている");
  const two = t.R("renderSeries")(customerPayload([deal({ deal_id: "a" }), deal({ deal_id: "b" })],
    { monthly: [mm("a"), mm("b")] }));
  if (two.indexOf("下に続く契約の図も同じです") < 0) throw new Error("図が2つ以上なのに「下に続く契約の図も同じです」が消えた");
});
check("L4", "series: 読み方の例（NPS と接触）は契約ごとに繰り返さず、最初の図の副題の1回だけ", async () => {
  const t = boot();
  const pts = [{ m: 1, v: 3, carry: false }, { m: 2, v: 5, carry: false }];
  const mm = (id) => ({ deal_id: id, name: "案件" + id, start: "2026-04-01", expiration: "2026-09-30",
    period: 6, span_months: 6, series: { oubo: pts }, nps: { nps: [{ m: 1, v: 6 }, { m: 2, v: 9 }] } });
  const D = customerPayload([deal({ deal_id: "a" }), deal({ deal_id: "b" }), deal({ deal_id: "c" })], {
    monthly: [mm("a"), mm("b"), mm("c")],
    contacts: ["a", "b", "c"].map((id) => ({ deal_id: id, dates: ["2026-04-10"] })),
  });
  const h = t.R("renderSeries")(D);
  if (count(h, /系列を縦に並べる/g) < 3) throw new Error("契約ごとの図が3つ描かれていない（見張りの前提）");
  const n = count(h, /読み方の例/g);
  if (n !== 1) throw new Error("「読み方の例」が " + n + " 回出ている（最初の図の1回にする）");
  if (count(h, /同じ月に何が起きていたかが読めます/g) < 3) throw new Error("2つ目以降の図の副題まで消えた");
});
check("L4", "series: 契約の連なりで金額が空の契約に「金額なし」と書く（「3回目」だけにしない）", async () => {
  const t = boot();
  const D = customerPayload([deal({ deal_id: "x1", renewal_no: 3, amount: null }),
                             deal({ deal_id: "x2", renewal_no: 2, amount: 1200000, start: "2024-01-01" })]);
  const h = t.R("renderSeries")(D);
  if (h.indexOf("3回目　金額なし") < 0) throw new Error("金額が空の契約の注記が「3回目」だけになっている");
});
check("L4", "契約の系列の表: 取引・ステージ・拠点を折り返す列にする（1440px で右端が切れない）", async () => {
  const t = boot();
  const h = t.R("custBlocks")(customerPayload([deal({ deal_id: "t1" })]), new Set(["deals"]));
  const head = h.slice(h.indexOf("<thead>"), h.indexOf("</thead>"));
  for (const [c, w] of [["取引", "wl"], ["ステージ", "ws"], ["拠点", "ws"]])
    if (head.indexOf('<th class="' + w + '">' + c + "</th>") < 0)
      throw new Error("契約の系列の「" + c + "」が折り返す列（" + w + "）になっていない");
  // 折るだけでは 1440px の本文（1,117px）に 35px 足りなかった。開始と満了を1つの列に2段で出す
  if (head.indexOf("<th>開始〜満了</th>") < 0 || head.indexOf("<th>開始</th>") >= 0)
    throw new Error("契約の系列で開始と満了が別の列のまま（12列で右端が切れる）");
  if (h.indexOf("2025-01-01<br>〜2025-12-31") < 0) throw new Error("開始〜満了の列に2段で日付が出ていない");
});

check("L4", "houjin: 「拠点をまたいで1本の線にしない」と基準日を1回ずつにし、他の法人と比べるの見出しは1つ", async () => {
  const t = boot();
  const D = customerPayload([deal({ deal_id: "h1" }), deal({ deal_id: "h2", site: "S2" })], {
    meta: { found: true, houjin: "H1", today: "2026-09-24",
      not_counted: "※ 採用単価は拠点ごとに分けています。1本にまとめると拠点間のばらつきが時間の悪化に見えます" },
    cpa3: [{ deal_id: "h1", name: "案件", total: 900000, monthly: 600000, syoudaku: 2 }],
  });
  t.ctx.__D = D;
  t.R('customerHoujin = "H1"; houjinFor = ""; houjinPick = null;');
  const h = t.R("renderHoujin(__D)");
  const n = count(h, /時間の悪化/g);
  if (n !== 1) throw new Error("「1本にまとめると…時間の悪化に見える」が " + n + " 回出ている（頭の枠の1回にする）");
  if (h.indexOf("1本の線にまとめない理由") >= 0) throw new Error("「1本の線にまとめない理由」の枠が残っている");
  const b = count(h, /基準日 2026-09-24/g);
  if (b !== 1) throw new Error("基準日が " + b + " 回出ている");
  const at = h.indexOf("他の法人と比べる");
  if (at < 0 || h.lastIndexOf('<h2 class="sec mincho"><span class="no">問い</span>', at) < h.lastIndexOf("<h2", at))
    throw new Error("「他の法人と比べる」が問いの見出しになっていない（下の本部アプローチの問いと2つ続く）");
  if (h.indexOf("横軸は採用単価（万円）") < 0) throw new Error("採用単価を3つの出し方で見る図に単位（万円）が無い");
});

/* ---------------------------------------------------------------- 実行 */
/* ================================================================ 担当者ごとの接触（2026-09-24 追加） */
/** 担当者ごとの接触の応答（形だけ合わせた小さなもの）。月・週の両方が入っている */
function contactPayload() {
  const cell = (d, c) => ({ deals: d, contacts: c, avg: d ? c / d : null, small_n: d > 0 && d < 3 });
  const per = (key, label, prov) => ({ key, label, start: key, end: key, provisional: prov, calls_missing: false });
  return {
    meta: { today: "2026-09-18", min_deals: 3, call_from: "2026-03-23", n_no_span: 0, n_no_history: 0,
            not_counted: "※ 接触は検知専用です。" },
    month: { periods: [per("2026-08", "2026-08", false), per("2026-09", "2026-09", true)],
             rows: [{ consultant: "担当A", retired: false, cells: [cell(10, 20), cell(10, 5)] }],
             team: [cell(10, 20), cell(10, 5)], undetermined: [cell(0, 0), cell(0, 0)], shared: [0, 0] },
    week: { periods: [per("2026-09-07", "9/7〜9/13", false), per("2026-09-14", "9/14〜9/20", true)],
            rows: [{ consultant: "担当W", retired: false, cells: [cell(8, 4), cell(8, 1)] }],
            team: [cell(8, 4), cell(8, 1)], undetermined: [cell(0, 0), cell(0, 0)], shared: [0, 0] },
  };
}
check("C1", "担当者ごとの接触: 開くと contact-trend を1回だけ取り、週ごとに切り替えても取り直さずに描き直す", async () => {
  const t = boot();
  t.R('go("consultant", "contact")');
  const req = t.fetched[t.fetched.length - 1];
  if (!req || req.url.indexOf("/api/consulting/contact-trend") !== 0)
    throw new Error("担当者ごとの接触で contact-trend を取りに行っていない: " + (req && req.url));
  req.resolve(jsonRes(contactPayload()));
  await tick(); await tick();
  const main = t.reg["cs-main"];
  if (main.innerHTML.indexOf("担当A") < 0 || main.innerHTML.indexOf("担当W") >= 0)
    throw new Error("既定（月ごと）で描いていない");
  if (!/id="ct-unit-month"[^>]*aria-pressed="true"/.test(main.innerHTML))
    throw new Error("月ごとが押された状態で出ていない");
  // 週ごとのボタンを押す
  const wk = new t.El("ct-unit-week"); wk.dataset.u = "week";
  t.qsa["#ct-unit button[data-u]"] = [wk];
  t.R("wire(viewOf('consultant', 'contact'))");
  const n = t.fetched.length;
  if (typeof wk.onclick !== "function") throw new Error("週ごとのボタンに onclick が付いていない");
  wk.onclick();
  if (t.fetched.length !== n) throw new Error("切り替えで取り直している（応答に両方入っている）");
  if (main.innerHTML.indexOf("担当W") < 0 || main.innerHTML.indexOf("担当A") >= 0)
    throw new Error("週ごとに切り替わっていない");
  if (!/id="ct-unit-week"[^>]*aria-pressed="true"/.test(main.innerHTML))
    throw new Error("週ごとが押された状態で出ていない");
});

(async () => {
  if (mainJs == null) {
    console.error("FAIL 動きの見張り: 画面の <script> が取り出せない");
    failed++;
  } else {
    let ok = 0;
    for (const c of checks) {
      try {
        await c.fn();
        ok++;
        console.log(`OK   動き ${c.id.padEnd(4)} ${c.what}`);
      } catch (e) {
        failed++;
        console.error(`FAIL 動き ${c.id.padEnd(4)} ${c.what}\n     ${e && e.message}`);
      }
    }
    console.log(`\n動きの見張り: ${ok} / ${checks.length} 通過`);
  }
  if (failed) {
    console.error(`\n${failed} 件の問題。画面の JS が動かない、または操作すると壊れる状態です。`);
    process.exit(1);
  }
  console.log("\n画面の JS は構文として通り、動きの見張りも通ります。");
})();
