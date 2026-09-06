/**
 * Rust への移植が数字を変えていないことを固定するための、答え合わせ用の値を書き出す。
 *
 *   node scripts/indeed_golden.js
 *   → tests/fixtures/indeed_golden.json
 *
 * ■ なぜ要るか
 *   V2（Rust）に集計を移すとき、実装が 2 つになる瞬間がある。
 *   そこで数字がずれると、社内タブと顧客レポートで違うことを言い始める。
 *   いま動いている JS の出力をここに固定し、Rust 側のテストで突き合わせる。
 *
 * ■ 何を固定するか
 *   ・変化の判定（fitTrend）… 傾き・ばらつき・一本調子か・外れた月
 *   ・言い回し（trendLabel / shortTrend / describeTrend）
 *   ・全国と業界の定点（求人数・見た人数・募集企業数・指数）
 *   ・分解（分子と分母のどちらが効いたか）
 *   実データに依存する値は「どの月・どの職種か」も一緒に書き、
 *   データが更新されたら作り直せるようにする。
 */
'use strict';

const fs = require('fs');
const path = require('path');
const {
  fitTrend, trendLabel, describeTrend, shortTrend,
} = require('./indeed_trend_fit.js');

let Database = null;
try { Database = require('better-sqlite3'); } catch (e) { /* fallback */ }
const useNative = !Database;
if (useNative) { ({ DatabaseSync: Database } = require('node:sqlite')); }

const REPO = path.dirname(__dirname);
const db = useNative
  ? new Database(path.join(REPO, 'data', 'indeed_insights.db'), { readOnly: true })
  : new Database(path.join(REPO, 'data', 'indeed_insights.db'), { readonly: true });

const GROUPS = [
  { name: '物流・運輸', cats: ['物流・配送', '軽作業', '送迎ドライバー'] },
  { name: '製造・生産', cats: ['製造・生産', '製造・開発 (電気・機械・金属・化学)'] },
  { name: '建設・設備・整備', cats: ['建設・土木', '建築・インテリア・造園', '保全・管理（設備・建物）'] },
  { name: 'サービス・販売', cats: ['接客・販売', '飲食・フード', '清掃', '警備・誘導'] },
  { name: '事務・管理', cats: ['事務・オフィスワーク', '経営・管理・企画・戦略', '営業 無形商材', '会計・監査法人', '法律', '未分類'] },
];

const months = db.prepare("SELECT value v FROM insight_meta WHERE key='full_months'").get().v.split(',');
const I_NOW = months.length - 1;
const I_PREV = months.length - 2;
const I_YOY = months.length >= 13 ? 0 : -1;

const r4 = (v) => (v == null || !isFinite(v) ? null : Math.round(v * 10000) / 10000);

// --- 1. 変化の判定。手で置いた系列なので、データが変わっても動かない ---
const SERIES = {
  // 実データの形をした 3 通り。和歌山県トラックドライバーの求人数（1 点だけ跳ねる）
  bumpy: [45, 55, 62, 72, 85, 78, 80, 85, 173, 130, 68, 65, 85],
  steady_down: Array.from({ length: 13 }, (_, i) => 100 * (0.95 ** i)),
  steady_up: Array.from({ length: 13 }, (_, i) => 100 * (1.08 ** i)),
  noisy: [100, 130, 90, 125, 95, 120, 100, 128, 92, 122, 98, 126, 105],
  flat: [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3],
  short: [10, 12, 11],
};
const W_SEEK = { up: '集まりやすくなって', down: '集まりにくくなって' };
const M13 = ['2025-07', '2025-08', '2025-09', '2025-10', '2025-11', '2025-12',
  '2026-01', '2026-02', '2026-03', '2026-04', '2026-05', '2026-06', '2026-07'];

const trend = {};
for (const [k, v] of Object.entries(SERIES)) {
  const f = fitTrend(v);
  trend[k] = {
    input: v,
    // 突き合わせに使う値は丸めない。丸めた値を厳しい許容で比べると必ず落ちる。
    fit: f ? {
      slopePct: f.slopePct,
      totalPct: f.totalPct,
      scatterPct: f.scatterPct,
      ratio: isFinite(f.ratio) ? f.ratio : null,
      level: f.level,
      steady: f.steady,
      n: f.n,
      outliers: f.outliers.map((o) => ({ index: o.index, ratio: o.ratio })),
    } : null,
    label: trendLabel(f),
    short: shortTrend(f, W_SEEK),
    describe: describeTrend(f, '求人数', v.length === M13.length ? M13 : null),
  };
}

// --- 2. 実データの定点。どの月のものかを一緒に書く ---
const raw = db.prepare(`SELECT p.report_month m, it.display_category c,
  SUM(p.job_count) j, SUM(p.ctk_count) k, SUM(p.employer_count) e
  FROM insight_title_pref p JOIN insight_title it ON it.norm_title = p.norm_title
  GROUP BY 1, 2`).all();
const owner = {};
GROUPS.forEach((g) => g.cats.forEach((c) => { owner[c] = g.name; }));
const zero = () => ({ j: months.map(() => 0), k: months.map(() => 0), e: months.map(() => 0) });
const nation = zero();
const byInd = {};
GROUPS.forEach((g) => { byInd[g.name] = zero(); });
raw.forEach((r) => {
  const i = months.indexOf(r.m);
  if (i < 0) return;
  nation.j[i] += r.j || 0; nation.k[i] += r.k || 0; nation.e[i] += r.e || 0;
  const o = owner[r.c];
  if (o) { byInd[o].j[i] += r.j || 0; byInd[o].k[i] += r.k || 0; byInd[o].e[i] += r.e || 0; }
});

const pc = (a, b) => (b ? ((a / b) - 1) * 100 : null);
const stat = (d) => ({
  job: d.j[I_NOW],
  seen: d.k[I_NOW],
  employers: d.e[I_NOW],
  momJob: r4(pc(d.j[I_NOW], d.j[I_PREV])),
  yoyJob: I_YOY >= 0 ? r4(pc(d.j[I_NOW], d.j[I_YOY])) : null,
  momSeen: r4(pc(d.k[I_NOW], d.k[I_PREV])),
  yoySeen: I_YOY >= 0 ? r4(pc(d.k[I_NOW], d.k[I_YOY])) : null,
  yoyEmployers: I_YOY >= 0 ? r4(pc(d.e[I_NOW], d.e[I_YOY])) : null,
  sppNow: r4(d.j[I_NOW] ? d.k[I_NOW] / d.j[I_NOW] : null),
  yoySpp: I_YOY >= 0 && d.j[I_YOY]
    ? r4(pc(d.k[I_NOW] / d.j[I_NOW], d.k[I_YOY] / d.j[I_YOY])) : null,
  index: r4(d.j[0] ? (d.j[I_NOW] / d.j[0]) * 100 : null),
  perEmployerNow: r4(d.e[I_NOW] ? d.j[I_NOW] / d.e[I_NOW] : null),
  yoyPerEmployer: I_YOY >= 0 && d.e[I_YOY]
    ? r4(pc(d.j[I_NOW] / d.e[I_NOW], d.j[I_YOY] / d.e[I_YOY])) : null,
});

// --- 3. 職種 × 県の抜き取り。県ごとの集計が合っているかの確認用 ---
const SAMPLES = [
  ['配送ドライバー', '千葉県'],
  ['一般事務', '東京都'],
  ['製造', '愛知県'],
  ['自動車整備士', '兵庫県'],
  ['トラックドライバー', '和歌山県'],
];
const samples = SAMPLES.map(([t, p]) => {
  const rows = db.prepare(`SELECT report_month m, job_count j, ctk_count k, employer_count e
    FROM insight_title_pref WHERE norm_title=? AND prefecture=? ORDER BY 1`).all(t, p);
  const at = (m) => rows.find((x) => x.m === m) || {};
  const now = at(months[I_NOW]);
  const prev = at(months[I_PREV]);
  const f = fitTrend(months.map((m) => {
    const x = at(m);
    return x.j && x.k ? x.k / x.j : null;
  }));
  return {
    title: t,
    prefecture: p,
    month: months[I_NOW],
    job: now.j ?? null,
    seen: now.k ?? null,
    employers: now.e ?? null,
    momJob: r4(pc(now.j, prev.j)),
    spp: r4(now.j ? now.k / now.j : null),
    sppLabel: trendLabel(f),
    sppSteady: f ? f.steady : null,
  };
});

// --- 4. 時給。県間の幅が合っているかの確認用 ---
const salMonth = (db.prepare('SELECT MAX(snapshot_month) m FROM insight_salary').get() || {}).m || null;
const salary = salMonth ? ['販売スタッフ', '一般事務', '配送ドライバー'].map((t) => {
  const rows = db.prepare(`SELECT prefecture p, median_salary v FROM insight_salary
    WHERE norm_title=? AND snapshot_month=? AND salary_period='HOURLY' AND median_salary IS NOT NULL
    ORDER BY v DESC`).all(t, salMonth);
  return {
    title: t,
    month: salMonth,
    prefectures: rows.length,
    highest: rows.length ? { prefecture: rows[0].p, value: rows[0].v } : null,
    lowest: rows.length ? { prefecture: rows[rows.length - 1].p, value: rows[rows.length - 1].v } : null,
  };
}) : [];

const golden = {
  note: 'Rust への移植が数字を変えていないことを確かめるための答え合わせ用。'
    + 'scripts/indeed_golden.js で作り直す。データを更新したら作り直すこと。',
  builtAt: new Date().toISOString(),
  months,
  compare: { now: months[I_NOW], prev: months[I_PREV], yoy: I_YOY >= 0 ? months[I_YOY] : null },
  trend,
  nation: stat(nation),
  industries: GROUPS.map((g) => ({ name: g.name, ...stat(byInd[g.name]) })),
  samples,
  salary,
};

const outDir = path.join(REPO, 'tests', 'fixtures');
fs.mkdirSync(outDir, { recursive: true });
const out = path.join(outDir, 'indeed_golden.json');
fs.writeFileSync(out, `${JSON.stringify(golden, null, 2)}\n`, 'utf8');

console.log(`変化の判定 ${Object.keys(trend).length} 系列 / 業界 ${golden.industries.length}`
  + ` / 抜き取り ${samples.length} 組 / 時給 ${salary.length} 職種`);
console.log(`全国: 求人 ${golden.nation.job.toLocaleString('ja-JP')}`
  + ` / 前年比 ${golden.nation.yoyJob}% / 1求人あたり ${golden.nation.sppNow}`);
console.log(`サイズ ${(fs.statSync(out).size / 1024).toFixed(0)} KB`);
console.log('出力:', out);
