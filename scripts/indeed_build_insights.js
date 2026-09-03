/**
 * 収集した生データから、3つの出し口（営業資料 / ダッシュボード / 社内分析）が
 * 共通で使う分析結果を計算して DB に書き出す。通信はしない。
 *
 *   node scripts/indeed_build_insights.js
 *
 * ■ 設計方針
 *   計算は 1 か所に集約する。営業資料とダッシュボードで数字が食い違うと信用を失う。
 *   各テーブルに「どれだけのデータに基づくか」を持たせ、
 *   下流が母数を知らないまま断定的に表示できないようにする。
 *
 * ■ 出力テーブル
 *   insight_title        職種ごとの全国像（市場規模・属性シェア・検索のされ方）
 *   insight_title_pref   職種 × 県（採りやすさ・全国順位・上位検索語）
 *   insight_pref_unique  県ごとに突出している検索語
 *   insight_meta         計算時点のデータ充足率（下流の注記に使う）
 */
'use strict';

const path = require('path');

let Database = null;
try { Database = require('better-sqlite3'); } catch (e) { /* fallback */ }
if (!Database) { ({ DatabaseSync: Database } = require('node:sqlite')); }

const REPO = path.dirname(__dirname);

// クローラが走っている間、生データ DB は書き込みロックされていて読めない
// （既定のジャーナルモードでは writer が reader を締め出す）。
// 収集を止めずに分析したいので、スナップショットを取ってそちらを読む。
// 分析結果も別ファイルに書く（生データと派生データを分ける意味もある）。
const fs = require('fs');
const RAW = path.join(REPO, 'data', 'indeed_market_api.db');
const SNAP = path.join(REPO, 'data', 'indeed_market_api.snapshot.db');
let db = null;
for (let i = 0; i < 3 && !db; i += 1) {
  try {
    fs.copyFileSync(RAW, SNAP);
    db = new Database(SNAP);
    db.prepare('SELECT COUNT(*) c FROM indeed_api_monthly').get();   // 壊れていないか確認
  } catch (e) {
    console.log(`スナップショット取得に失敗（${i + 1}/3）: ${String(e.message || e).split('\n')[0]}`);
    db = null;
  }
}
if (!db) {
  console.error('生データを読み取れませんでした。クローラを一時停止してから再実行してください。');
  process.exit(1);
}
const snapAge = Math.round((Date.now() - fs.statSync(RAW).mtimeMs) / 1000);
console.log(`スナップショット取得（生データの最終更新 ${snapAge} 秒前）`);
const out = new Database(path.join(REPO, 'data', 'indeed_insights.db'));
const now = new Date().toISOString();

// 検索語の分類は scripts/indeed_kw_classify.js に集約している。
// Indeed が返すのは語とクリック数だけで、層の情報は無い。分類はこちらの判断なので、
// 定義とテストを 1 か所に置き、全ての出し口が同じ判断に乗るようにしている。
// eslint-disable-next-line global-require
const KW = require('./indeed_kw_classify.js');

out.exec(`
DROP TABLE IF EXISTS insight_title;
CREATE TABLE insight_title (
  norm_title TEXT PRIMARY KEY,
  display_category TEXT,
  months TEXT,
  prefs_with_data INTEGER,
  job_count INTEGER,
  ctk_count INTEGER,
  employer_count INTEGER,
  seekers_per_posting REAL,
  mobile_pct REAL,
  difficulty REAL,               -- Indeed が出す採用難易度の平均（0〜1）
  kw_clicks INTEGER,
  kw_prefs INTEGER,
  pct_condition REAL,
  pct_senior REAL, pct_homemaker REAL, pct_student REAL,
  pct_foreign REAL, pct_inexperienced REAL,
  search_style TEXT,
  built_at TEXT NOT NULL
);

DROP TABLE IF EXISTS insight_title_pref;
CREATE TABLE insight_title_pref (
  norm_title TEXT NOT NULL, prefecture TEXT NOT NULL, report_month TEXT NOT NULL,
  display_category TEXT,
  job_count INTEGER, ctk_count INTEGER, employer_count INTEGER,
  seekers_per_posting REAL, mobile_pct REAL,
  difficulty REAL,               -- Indeed が出す採用難易度（0〜1）
  rank_in_country INTEGER,
  prefs_compared INTEGER,
  vs_national REAL,
  top_keywords TEXT,
  kw_available INTEGER,
  built_at TEXT NOT NULL,
  PRIMARY KEY (norm_title, prefecture, report_month)
);

DROP TABLE IF EXISTS insight_pref_unique;
CREATE TABLE insight_pref_unique (
  prefecture TEXT NOT NULL, search_term TEXT NOT NULL,
  clicks INTEGER, lift REAL, titles INTEGER,
  built_at TEXT NOT NULL,
  PRIMARY KEY (prefecture, search_term)
);

DROP TABLE IF EXISTS insight_meta;
CREATE TABLE insight_meta (
  key TEXT PRIMARY KEY, value TEXT, built_at TEXT NOT NULL
);
`);

// 完全に揃っている月だけを分析に使う（虫食いの月で順位を出さない）
const perMonth = (() => {
  try {
    // eslint-disable-next-line global-require
    const { readSelection } = require('./build_indeed_title_selection.js');
    return readSelection(path.join(REPO, 'claudedocs', 'indeed_title_selection.csv')).size * 47;
  } catch (e) { return 0; }
})();
const monthRows = db.prepare('SELECT report_month m, COUNT(*) c FROM indeed_api_monthly GROUP BY 1 ORDER BY 1').all();
const fullMonths = monthRows.filter((r) => perMonth && r.c >= perMonth * 0.98).map((r) => r.m);
const latest = fullMonths[fullMonths.length - 1];
if (!latest) { console.error('完全に揃っている月がありません。分析はまだ実行できません。'); process.exit(1); }
console.log(`分析に使う月: ${fullMonths.join(', ')}（最新 ${latest}）`);

// --- 職種 × 県 ---
const insTP = out.prepare(`INSERT OR REPLACE INTO insight_title_pref
  (norm_title,prefecture,report_month,display_category,job_count,ctk_count,employer_count,
   seekers_per_posting,mobile_pct,difficulty,rank_in_country,prefs_compared,vs_national,
   top_keywords,kw_available,built_at)
  VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`);

// 検索語は 1 行ずつ引くと数万回のクエリになり、構築に何分もかかる。
// 一度だけ全件読んでメモリ上で引けるようにする。
// （長時間書き込みを続けると、他から読まれただけでロック衝突を起こす。
//   短時間で書き切ることが、そのまま事故防止になる。2026-08-21 に踏んだ）
const kwByKey = new Map();
for (const r of db.prepare(`SELECT norm_title t, prefecture p, report_month m, search_term s, click_count c
  FROM indeed_api_search_terms ORDER BY norm_title, prefecture, report_month, rank`).all()) {
  const k = `${r.t}|${r.p}|${r.m}`;
  if (!kwByKey.has(k)) kwByKey.set(k, []);
  kwByKey.get(k).push(`${r.s}:${r.c}`);
}
console.log(`検索語を読み込み: ${kwByKey.size.toLocaleString()} 組`);

// 月次も一度に読んでから月・職種で分ける
const byMonthTitle = new Map();
for (const r of db.prepare(`SELECT * FROM indeed_api_monthly WHERE job_count IS NOT NULL`).all()) {
  if (!fullMonths.includes(r.report_month)) continue;
  const k = `${r.report_month}|${r.norm_title}`;
  if (!byMonthTitle.has(k)) byMonthTitle.set(k, []);
  byMonthTitle.get(k).push(r);
}

// 書き込みは 1 トランザクションにまとめる（速度と、途中状態を見せないため）
out.exec('BEGIN');
for (const [key, rows] of byMonthTitle) {
  const [m, t] = key.split('|');
  rows.sort((a, b) => b.seekers_per_posting - a.seekers_per_posting);
  const natJob = rows.reduce((a, r) => a + r.job_count, 0);
  const natCtk = rows.reduce((a, r) => a + r.ctk_count, 0);
  const natRatio = natJob ? natCtk / natJob : null;
  rows.forEach((r, i) => {
    const kw = kwByKey.get(`${t}|${r.prefecture}|${m}`) || [];
    insTP.run(t, r.prefecture, m, r.display_category, r.job_count, r.ctk_count, r.employer_count,
      r.seekers_per_posting, r.mobile_click_pct, r.competition_score, i + 1, rows.length,
      natRatio ? r.seekers_per_posting / natRatio : null,
      kw.join(';'), kw.length ? 1 : 0, now);
  });
}
out.exec('COMMIT');
console.log(`insight_title_pref: ${out.prepare('SELECT COUNT(*) c FROM insight_title_pref').get().c} 行`);

// --- 職種の全国像 ---
const insT = out.prepare(`INSERT OR REPLACE INTO insight_title
  (norm_title,display_category,months,prefs_with_data,job_count,ctk_count,employer_count,
   seekers_per_posting,mobile_pct,difficulty,kw_clicks,kw_prefs,pct_condition,pct_senior,
   pct_homemaker,pct_student,pct_foreign,pct_inexperienced,search_style,built_at)
  VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`);

// 職種ごとの検索語集計も一度にまとめる
const termsByTitle = new Map();
const prefsByTitle = new Map();
for (const r of db.prepare(`SELECT norm_title t, prefecture p, search_term s, SUM(click_count) c
  FROM indeed_api_search_terms GROUP BY 1,2,3`).all()) {
  if (!termsByTitle.has(r.t)) { termsByTitle.set(r.t, new Map()); prefsByTitle.set(r.t, new Set()); }
  const mm = termsByTitle.get(r.t);
  mm.set(r.s, (mm.get(r.s) || 0) + r.c);
  prefsByTitle.get(r.t).add(r.p);
}
for (const [t, mm] of termsByTitle) {
  termsByTitle.set(t, [...mm].map(([s, c]) => ({ s, c })));
}

const shareOfTerms = (terms) => KW.shares(terms.map((r) => ({ term: r.s, clicks: r.c })));

for (const r of db.prepare(`SELECT norm_title t, display_category c,
    COUNT(*) n, SUM(job_count) j, SUM(ctk_count) s, SUM(employer_count) e,
    AVG(mobile_click_pct) mob, AVG(competition_score) dif
  FROM indeed_api_monthly WHERE report_month=? AND job_count IS NOT NULL GROUP BY 1,2`).all(latest)) {
  const terms = termsByTitle.get(r.t) || [];
  const kwPrefs = (prefsByTitle.get(r.t) || new Set()).size;
  const clicks = terms.reduce((a, x) => a + x.c, 0);
  const sh = shareOfTerms(terms);
  const pctCond = sh ? sh.condition : null;
  let style = null;
  if (pctCond !== null) {
    if (pctCond >= 55) style = '条件で探される';
    else if (pctCond <= 25) style = '職種名で探される';
    else style = '混在';
  }
  insT.run(r.t, r.c, fullMonths.join(','), r.n, r.j, r.s, r.e,
    r.j ? r.s / r.j : null, r.mob, r.dif, clicks, kwPrefs, pctCond,
    sh ? sh.senior : null, sh ? sh.homemaker : null,
    sh ? sh.student : null, sh ? sh.foreign : null,
    sh ? sh.inexperienced : null, style, now);
}
console.log(`insight_title: ${out.prepare('SELECT COUNT(*) c FROM insight_title').get().c} 行`);

// --- 県ごとの特異ワード ---
const natl = {};
let ntot = 0;
for (const r of db.prepare('SELECT search_term t, SUM(click_count) c FROM indeed_api_search_terms GROUP BY 1').all()) {
  natl[r.t] = r.c; ntot += r.c;
}
const pref = db.prepare(`SELECT prefecture p, search_term t, SUM(click_count) c,
  COUNT(DISTINCT norm_title) j FROM indeed_api_search_terms GROUP BY 1,2`).all();
const ptot = {};
pref.forEach((r) => { ptot[r.p] = (ptot[r.p] || 0) + r.c; });
const insU = out.prepare(`INSERT OR REPLACE INTO insight_pref_unique
  (prefecture,search_term,clicks,lift,titles,built_at) VALUES (?,?,?,?,?,?)`);
let uniq = 0;
for (const r of pref) {
  if (r.c < 100) continue;                    // ノイズを落とす
  const lift = (r.c / ptot[r.p]) / (natl[r.t] / ntot);
  if (lift < 3) continue;                     // 全国平均の 3 倍未満は特異と呼ばない
  insU.run(r.p, r.t, r.c, lift, r.j, now);
  uniq += 1;
}
console.log(`insight_pref_unique: ${uniq} 行`);

// --- メタ情報（下流の注記に使う）---
const meta = out.prepare('INSERT OR REPLACE INTO insight_meta (key,value,built_at) VALUES (?,?,?)');
meta.run('full_months', fullMonths.join(','), now);
meta.run('latest_month', latest, now);
meta.run('kw_combos', String(db.prepare('SELECT COUNT(*) c FROM (SELECT 1 FROM indeed_api_search_terms GROUP BY norm_title,prefecture,report_month)').get().c), now);
meta.run('kw_months', db.prepare('SELECT GROUP_CONCAT(DISTINCT report_month) g FROM indeed_api_search_terms').get().g || '', now);
meta.run('source', 'Indeed 採用市場レポート（求人企業向け）', now);
meta.run('caveat', 'Indeed 上の行動データであり労働市場全体ではない。クリックは応募ではない。検索語は上位10件のみ。', now);
console.log('insight_meta: 完了');
console.log('');
console.log('分析層:', path.join(REPO, 'data', 'indeed_insights.db'));

// ---------------------------------------------------------------------------
// 検索エンジンの月別検索ボリューム（あれば取り込む）
//
// 「職種名」だけの検索は求職以外を多く含む（飲食店 4,090,000 に対し
// 飲食店 求人 5,400 = 757倍）。求職の指標には variant='job' を使い、
// variant='name' は「その言葉が世間でどれだけ使われるか」の参考にとどめる。
// ---------------------------------------------------------------------------
const SV = path.join(REPO, 'data', 'search_volume.db');
out.exec(`
DROP TABLE IF EXISTS insight_search_trend;
CREATE TABLE insight_search_trend (
  norm_title TEXT NOT NULL, variant TEXT NOT NULL,
  months TEXT, series TEXT,
  avg_monthly INTEGER, latest INTEGER, latest_month TEXT,
  yoy_pct REAL, peak_month INTEGER, trough_month INTEGER,
  peak_ratio REAL, competition TEXT, low_bid_yen REAL, high_bid_yen REAL,
  built_at TEXT NOT NULL,
  PRIMARY KEY (norm_title, variant)
);`);

if (fs.existsSync(SV)) {
  const sv = new Database(SV);
  const insS = out.prepare(`INSERT OR REPLACE INTO insight_search_trend
    (norm_title,variant,months,series,avg_monthly,latest,latest_month,yoy_pct,
     peak_month,trough_month,peak_ratio,competition,low_bid_yen,high_bid_yen,built_at)
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`);
  let n = 0;
  for (const k of sv.prepare('SELECT * FROM search_volume_keyword').all()) {
    const rows = sv.prepare(`SELECT year_month m, searches s FROM search_volume
      WHERE norm_title=? AND variant=? ORDER BY m`).all(k.norm_title, k.variant);
    if (!rows.length) continue;
    const vals = rows.map((r) => r.s || 0);

    // 前年同月比: 直近12ヶ月合計 ÷ その前の12ヶ月合計
    const sum = (a) => a.reduce((x, y) => x + y, 0);
    const last12 = vals.slice(-12);
    const prev12 = vals.slice(-24, -12);
    const yoy = prev12.length === 12 && sum(prev12) > 0
      ? ((sum(last12) / sum(prev12)) - 1) * 100 : null;

    // 季節性: 暦月ごとの平均を取り、最大月・最小月と山の高さを出す
    const byMonth = Array.from({ length: 12 }, () => []);
    rows.forEach((r) => byMonth[Number(r.m.slice(5, 7)) - 1].push(r.s || 0));
    const avgM = byMonth.map((a) => (a.length ? sum(a) / a.length : null));
    const valid = avgM.map((v, i) => ({ v, i })).filter((x) => x.v !== null);
    let peak = null; let trough = null; let ratio = null;
    if (valid.length === 12) {
      const mean = sum(avgM) / 12;
      peak = valid.reduce((a, b) => (b.v > a.v ? b : a)).i + 1;
      trough = valid.reduce((a, b) => (b.v < a.v ? b : a)).i + 1;
      ratio = mean > 0 ? Math.max(...avgM) / mean : null;
    }

    insS.run(k.norm_title, k.variant, rows.map((r) => r.m).join(','), vals.join(','),
      k.avg_monthly, vals[vals.length - 1], rows[rows.length - 1].m, yoy,
      peak, trough, ratio, k.competition, k.low_bid_yen, k.high_bid_yen, now);
    n += 1;
  }
  meta.run('search_volume_period',
    `${sv.prepare('SELECT MIN(year_month) a FROM search_volume').get().a} 〜 `
    + `${sv.prepare('SELECT MAX(year_month) b FROM search_volume').get().b}`, now);
  meta.run('search_volume_caveat',
    '検索ボリュームは検索エンジンの推定値で、月ごとに丸められた値です。'
    + '「職種名」だけの検索は求職以外の意図も含むため、求職の指標には「職種名＋求人」を使います。', now);
  console.log(`insight_search_trend: ${n} 行（検索ボリューム）`);
} else {
  console.log('検索ボリュームのデータがないため insight_search_trend は空です');
}

// ---------------------------------------------------------------------------
// 検索キーワードの変化
//
// 市場が動いたことは求人数・求職者数で分かるが、「求職者が何を求めているか」が
// どう変わったかは検索語にしか出ない。せっかく 35 万行あるので使う。
//
// クリック数の実数は市場規模とともに動くので、месяц ごとのシェア（その職種の
// その月の総クリックに対する割合）で見る。規模の影響を外すため。
//
// 上位 10 件しか取れない制約があるので、「順位が下がって圏外へ出た」語は
// シェア 0 として扱われる。増減の大きさは過大に出やすい。断定には使わない。
// ---------------------------------------------------------------------------
out.exec(`
DROP TABLE IF EXISTS insight_kw_attr_trend;
CREATE TABLE insight_kw_attr_trend (
  norm_title TEXT NOT NULL, report_month TEXT NOT NULL,
  total_clicks INTEGER,
  pct_condition REAL, pct_senior REAL, pct_homemaker REAL,
  pct_student REAL, pct_foreign REAL, pct_inexperienced REAL,
  pct_language REAL, pct_qualified REAL,
  term_count INTEGER,
  built_at TEXT NOT NULL,
  PRIMARY KEY (norm_title, report_month)
);

DROP TABLE IF EXISTS insight_kw_term_shift;
CREATE TABLE insight_kw_term_shift (
  norm_title TEXT NOT NULL, search_term TEXT NOT NULL,
  share_before REAL, share_after REAL, share_diff REAL,
  clicks_after INTEGER, built_at TEXT NOT NULL,
  PRIMARY KEY (norm_title, search_term)
);`);

{
  // 職種 × 月 × 語 のクリック合計を一度に読む
  const agg = new Map();          // title -> month -> Map(term -> clicks)
  for (const r of db.prepare(`SELECT norm_title t, report_month m, search_term s, SUM(click_count) c
    FROM indeed_api_search_terms GROUP BY 1,2,3`).all()) {
    if (!fullMonths.includes(r.m)) continue;
    if (!agg.has(r.t)) agg.set(r.t, new Map());
    const byM = agg.get(r.t);
    if (!byM.has(r.m)) byM.set(r.m, new Map());
    byM.get(r.m).set(r.s, r.c);
  }

  const insA = out.prepare(`INSERT OR REPLACE INTO insight_kw_attr_trend
    (norm_title,report_month,total_clicks,pct_condition,pct_senior,pct_homemaker,
     pct_student,pct_foreign,pct_inexperienced,pct_language,pct_qualified,term_count,built_at)
    VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)`);
  const insS = out.prepare(`INSERT OR REPLACE INTO insight_kw_term_shift
    (norm_title,search_term,share_before,share_after,share_diff,clicks_after,built_at)
    VALUES (?,?,?,?,?,?,?)`);

  const WIN = 3;                  // 前後それぞれ何か月を平均するか
  out.exec('BEGIN');
  let nA = 0;
  let nS = 0;
  for (const [t, byM] of agg) {
    // --- 属性シェアの月次推移 ---
    for (const m of fullMonths) {
      const terms = byM.get(m);
      if (!terms) continue;
      const sh = KW.shares([...terms].map(([term, clicks]) => ({ term, clicks })));
      if (!sh) continue;
      // 語数も持たせる。Indeed は県ごとに上位 10 語しか返さないので、
      // 語数が少ない職種ほど「0%」が「圏外に落ちた」を意味しやすい。
      insA.run(t, m, sh.total, sh.condition, sh.senior, sh.homemaker,
        sh.student, sh.foreign, sh.inexperienced, sh.language, sh.qualified,
        terms.size, now);
      nA += 1;
    }

    // --- 語ごとのシェア変化（前 3 か月平均 と 直近 3 か月平均）---
    const win = (ms) => {
      const sum = new Map();
      let total = 0;
      let seen = 0;
      for (const m of ms) {
        const terms = byM.get(m);
        if (!terms) continue;
        seen += 1;
        for (const [s, c] of terms) { sum.set(s, (sum.get(s) || 0) + c); total += c; }
      }
      return { sum, total, seen };
    };
    const a = win(fullMonths.slice(0, WIN));
    const b = win(fullMonths.slice(-WIN));
    if (!a.seen || !b.seen || !a.total || !b.total) continue;

    const all = new Set([...a.sum.keys(), ...b.sum.keys()]);
    const diffs = [];
    for (const s of all) {
      const sa = ((a.sum.get(s) || 0) / a.total) * 100;
      const sb = ((b.sum.get(s) || 0) / b.total) * 100;
      // どちらかで 1% 以上ないと、上位 10 件の出入りによる見かけの動きが混ざる
      if (sa < 1 && sb < 1) continue;
      diffs.push({ s, sa, sb, d: sb - sa, ca: b.sum.get(s) || 0 });
    }
    diffs.sort((x, y) => Math.abs(y.d) - Math.abs(x.d));
    for (const x of diffs.slice(0, 14)) {
      insS.run(t, x.s, x.sa, x.sb, x.d, x.ca, now);
      nS += 1;
    }
  }
  out.exec('COMMIT');
  console.log(`insight_kw_attr_trend: ${nA} 行 / insight_kw_term_shift: ${nS} 行`);
}

// ---------------------------------------------------------------------------
// 時給（insight_salary）
//
// 生の DB（125MB）は配れないので、必要な列だけ分析層に移す。
// Indeed は時給の月ごとの記録を返さないため、撮った月ごとに 1 行ずつ積み上がる。
// 下流（レポート・V2）はこのテーブルだけを見る。
// ---------------------------------------------------------------------------
out.exec(`
DROP TABLE IF EXISTS insight_salary;
CREATE TABLE insight_salary (
  norm_title     TEXT NOT NULL,
  prefecture     TEXT NOT NULL,
  snapshot_month TEXT NOT NULL,
  salary_period  TEXT NOT NULL,
  median_salary  REAL,
  min_salary     REAL,
  max_salary     REAL,
  built_at       TEXT,
  PRIMARY KEY (norm_title, prefecture, snapshot_month, salary_period)
);
CREATE INDEX idx_salary_title ON insight_salary (norm_title, salary_period, snapshot_month);
`);

let nSal = 0;
try {
  const rows = db.prepare(`SELECT norm_title, prefecture, snapshot_month, salary_period,
    median_salary, min_salary, max_salary
    FROM indeed_api_salary_monthly
    WHERE median_salary IS NOT NULL`).all();
  const insSal = out.prepare(`INSERT OR REPLACE INTO insight_salary
    (norm_title, prefecture, snapshot_month, salary_period,
     median_salary, min_salary, max_salary, built_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)`);
  out.exec('BEGIN');
  for (const r of rows) {
    insSal.run(r.norm_title, r.prefecture, r.snapshot_month, r.salary_period,
      r.median_salary, r.min_salary, r.max_salary, now);
    nSal += 1;
  }
  out.exec('COMMIT');
  const months = out.prepare('SELECT DISTINCT snapshot_month m FROM insight_salary ORDER BY 1').all()
    .map((r) => r.m);
  console.log(`insight_salary: ${nSal} 行（${months.length} 時点: ${months.join(', ')}）`);
} catch (e) {
  console.log(`insight_salary: 生の DB に時給が無いため空です（${e.message}）`);
}

// ---------------------------------------------------------------------------
// 配布用の gz を必ず作り直す。
//
// アプリは data/indeed_insights.db.gz を Docker イメージに積み、起動時に
// data/indeed_insights.db へ展開する。db を作り直したのに gz を古いままに
// すると、本番だけが黙って古い数字を出し続ける。ここで一緒に作ってしまう。
// ---------------------------------------------------------------------------
{
  const zlib = require('zlib');
  const src = path.join(REPO, 'data', 'indeed_insights.db');
  const dst = `${src}.gz`;
  const raw = fs.readFileSync(src);
  fs.writeFileSync(dst, zlib.gzipSync(raw, { level: 9 }));
  const mb = (n) => `${(n / 1048576).toFixed(1)}MB`;
  console.log(`配布用: ${dst}（${mb(raw.length)} → ${mb(fs.statSync(dst).size)}）`);
}
