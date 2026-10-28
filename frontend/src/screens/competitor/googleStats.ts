// Google検索需要タブの計算。月は "YYYY-MM"。欠測 (null) は 0 にせず、足し算・平均から外す。
import type { GoogleKeyword } from '../../generated/GoogleKeyword';
import type { LineInput } from './charts/multiLine';

export interface Ym {
  year: number;
  month: number;
}

/** "YYYY-MM" を読む。形式が違えば null。 */
export function parseYm(s: string): Ym | null {
  const m = /^(\d{4})-(\d{2})$/.exec(s);
  if (m === null) return null;
  const month = Number(m[2]);
  if (month < 1 || month > 12) return null;
  return { year: Number(m[1]), month };
}

const idx = (y: Ym): number => y.year * 12 + (y.month - 1);
const num = (v: number | null | undefined): v is number => v !== null && v !== undefined && Number.isFinite(v);

/** 月の索引 (年*12+月-1) → 値。読めない月は捨てる。 */
function byIndex(k: GoogleKeyword): Map<number, number | null> {
  const out = new Map<number, number | null>();
  for (const m of k.monthly) {
    const y = parseYm(m.month);
    if (y !== null) out.set(idx(y), m.search_volume);
  }
  return out;
}

const label = (i: number): string => `${String(Math.floor(i / 12))}-${String((i % 12) + 1).padStart(2, '0')}`;

/** 全検索語の月の和集合 (古い順)。 */
export function allMonths(keywords: readonly GoogleKeyword[]): string[] {
  const set = new Set<number>();
  for (const k of keywords) for (const i of byIndex(k).keys()) set.add(i);
  return [...set].sort((a, b) => a - b).map(label);
}

/** 全検索語を重ねる折れ線用。months は allMonths の順。 */
export function keywordLines(keywords: readonly GoogleKeyword[], months: readonly string[]): LineInput[] {
  return keywords.map((k) => {
    const map = byIndex(k);
    return {
      label: k.keyword,
      values: months.map((m) => {
        const y = parseYm(m);
        return y === null ? null : (map.get(idx(y)) ?? null);
      }),
    };
  });
}

export interface YearOverlay {
  /** "1月" 〜 "12月"。 */
  months: string[];
  /** 暦年ごとの線。名前は西暦。古い年から。 */
  lines: LineInput[];
  years: number[];
}

/** 1〜12 月を横軸に、暦年ごとに 1 本。 */
export function yearOverlay(k: GoogleKeyword): YearOverlay {
  const map = byIndex(k);
  const years = [...new Set([...map.keys()].map((i) => Math.floor(i / 12)))].sort((a, b) => a - b);
  return {
    months: Array.from({ length: 12 }, (_, i) => `${String(i + 1)}月`),
    years,
    lines: years.map((y) => ({
      label: String(y),
      values: Array.from({ length: 12 }, (_, m) => map.get(y * 12 + m) ?? null),
    })),
  };
}

/**
 * 月平均 (回/月) がこの値未満の検索語は、Keyword Planner の丸めの底に張り付く。折れ線は描くが、
 * 前年比・ピーク月は出さず、動きは目安だと添える。旧画面 (Rust の `MIN_MONTHLY_MEAN`) と同じ値。
 */
export const MIN_MONTHLY_MEAN = 100;

/** 取得できた月 (全期間) の月平均。値のある月が無ければ null。 */
export function monthlyMean(k: GoogleKeyword): number | null {
  const v = k.monthly.map((m) => m.search_volume).filter(num);
  return v.length === 0 ? null : v.reduce((a, b) => a + b, 0) / v.length;
}

export function isLowVolume(k: GoogleKeyword): boolean {
  const m = monthlyMean(k);
  return m === null || m < MIN_MONTHLY_MEAN;
}

/** カードの見出し行に添える 1 行。 */
export function lowVolumeNote(k: GoogleKeyword): string {
  const m = monthlyMean(k);
  return m === null
    ? '月ごとの検索数を取得できていないため、推移は出せません'
    : `月平均 ${Math.round(m).toLocaleString('en-US')} 回（丸め値が多く、動きは目安）`;
}

/** 前年比の上昇・下降・横ばいの境 (%)。旧画面の `YOY_FLAT_PCT` と同じ。 */
export const YOY_FLAT_PCT = 5;

export function yoyWord(pct: number): '上昇' | '下降' | '横ばい' {
  return pct > YOY_FLAT_PCT ? '上昇' : pct < -YOY_FLAT_PCT ? '下降' : '横ばい';
}

export interface Pivot {
  years: number[];
  /** rows[0] = 1 月。各行は years と同じ順。 */
  rows: (number | null)[][];
}

/** 行 = 1〜12 月、列 = 年。 */
export function monthYearPivot(k: GoogleKeyword): Pivot {
  const o = yearOverlay(k);
  return { years: o.years, rows: Array.from({ length: 12 }, (_, m) => o.lines.map((l) => l.values[m] ?? null)) };
}

export interface KeywordSummary {
  keyword: string;
  avgMonthly: number | null;
  competition: string;
  /** 直近 12 か月が全て数値のときだけ。 */
  last12Total: number | null;
  /** 直近 12 か月とその前の 12 か月が全て数値で、前の合計が 0 でないときだけ (%)。 */
  yoyPct: number | null;
  /** 暦月ごとの平均が最大の月 (1〜12)。同値は早い月。数値が無ければ null。 */
  peakMonth: number | null;
}

/** 最新月から数えた直近 12 か月と、その前の 12 か月の合計 (全て数値のときだけ)。 */
function windowTotals(map: Map<number, number | null>): { recent: number | null; prior: number | null } {
  if (map.size === 0) return { recent: null, prior: null };
  const latest = Math.max(...map.keys());
  const sum = (from: number): number | null => {
    let t = 0;
    for (let i = from; i < from + 12; i += 1) {
      const v = map.get(i);
      if (!num(v)) return null;
      t += v;
    }
    return t;
  };
  return { recent: sum(latest - 11), prior: sum(latest - 23) };
}

export function summarize(k: GoogleKeyword): KeywordSummary {
  const map = byIndex(k);
  const { recent, prior } = windowTotals(map);
  const yoyPct = recent !== null && prior !== null && prior > 0 ? (recent / prior - 1) * 100 : null;
  const sums = Array.from({ length: 12 }, () => ({ t: 0, n: 0 }));
  for (const [i, v] of map) {
    if (!num(v)) continue;
    const s = sums[i % 12];
    if (s) {
      s.t += v;
      s.n += 1;
    }
  }
  let peakMonth: number | null = null;
  let best = -1;
  sums.forEach((s, m) => {
    if (s.n === 0) return;
    const avg = s.t / s.n;
    if (avg > best) {
      best = avg;
      peakMonth = m + 1;
    }
  });
  return {
    keyword: k.keyword,
    avgMonthly: k.avg_monthly,
    competition: k.competition,
    last12Total: recent,
    yoyPct,
    peakMonth,
  };
}
