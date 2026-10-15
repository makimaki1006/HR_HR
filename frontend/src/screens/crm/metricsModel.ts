import type { AllCallersResponse } from '../../generated/AllCallersResponse';
import type { CallerMetrics } from '../../generated/CallerMetrics';
import type { MonthAllResponse } from '../../generated/MonthAllResponse';
import type { MonthMetricsResponse } from '../../generated/MonthMetricsResponse';
import type { TodayMetricsResponse } from '../../generated/TodayMetricsResponse';

/**
 * 「今日の架電成績」の表示用の計算。画面と管理者の表で共有する。
 * 定義 (架電数 = 発信だけ、30/90/360 秒以上、割合 = 件数 ÷ 架電数など) は Rust の caller_metrics.rs が正本。
 */

/** ミリ秒を M:SS (1 時間以上は H:MM:SS)。null は「-」 */
export function formatDuration(ms: number | null): string {
  if (ms === null) return '-';
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${String(h)}:${String(m).padStart(2, '0')}:${ss}` : `${String(m)}:${ss}`;
}

/** 0〜1 の割合を「12.3%」に。null は「-」 */
export function formatRate(r: number | null): string {
  return r === null ? '-' : `${(Math.round(r * 1000) / 10).toFixed(1)}%`;
}

/** 件数の平均 (小数 1 位) */
export function formatAvg(n: number | null | undefined): string {
  return n === null || n === undefined ? '-' : (Math.round(n * 10) / 10).toFixed(1);
}

const JST = { timeZone: 'Asia/Tokyo', hour: 'numeric', minute: 'numeric', hour12: false } as const;

/** RFC 3339 の時刻を JST の「H:MM」に。読めなければ null */
export function clockJst(iso: string | null): string | null {
  if (iso === null) return null;
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return null;
  const parts = new Intl.DateTimeFormat('ja-JP', JST).formatToParts(t);
  const h = parts.find(p => p.type === 'hour')?.value ?? '';
  const m = parts.find(p => p.type === 'minute')?.value ?? '';
  return `${String(Number(h))}:${m.padStart(2, '0')}`;
}

/** 「14時05分時点」 */
export function asOfLabel(iso: string): string {
  const c = clockJst(iso);
  if (c === null) return '';
  const [h, m] = c.split(':');
  return `${h ?? ''}時${m ?? ''}分時点`;
}

/** 帯にもともとある項目 (Rust の metric_prefs::BUILTIN_KEYS と同じ) */
export type BuiltinKey = 'outbound' | 'inbound' | 'over_30s' | 'over_60s' | 'over_90s' | 'over_360s' | 'duration' | 'avg_duration' | 'appointments';
export const BUILTIN_KEYS: readonly BuiltinKey[] = ['outbound', 'inbound', 'over_30s', 'over_60s', 'over_90s', 'over_360s', 'duration', 'avg_duration', 'appointments'];
export const BUILTIN_LABELS: Record<BuiltinKey, string> = {
  outbound: '架電数', inbound: '着信', over_30s: '30秒以上', over_60s: '60秒以上', over_90s: '90秒以上', over_360s: '360秒以上',
  duration: '通話時間', avg_duration: '1件平均', appointments: 'アポ',
};
/** 設定の無い人の帯 (Rust の DEFAULT_ITEMS と同じ) */
export const DEFAULT_ITEMS: readonly string[] = ['outbound', 'over_90s', 'duration', 'appointments'];
/** 今月の帯で出せる項目 (今月は秒数の基準と着信を数えていない) */
export const MONTH_KEYS: readonly BuiltinKey[] = ['outbound', 'duration', 'avg_duration', 'appointments'];
export function isBuiltinKey(k: string): k is BuiltinKey {
  return (BUILTIN_KEYS as readonly string[]).includes(k);
}

export interface StripItem {
  /** もともとの項目のキーか、自分の項目の id */
  key: string;
  label: string;
  /** 自分の値。割合・1 件あたりは「・」でつなぐ */
  value: string;
  /** 比較の括弧の中身 (「チーム平均 47.5・3位/8人」) */
  compare: string;
  /** 狭い幅で出す短い比較 (「平均47.5・3位/8」)。意味は compare と同じ */
  compareShort: string;
  /** 括弧の外の補足 (着信) */
  extra: string | null;
  /** 数字の代わりに出す理由 (自分の項目の「件数が多すぎます」など)。あれば value・compare は出さない */
  note?: string | null;
  /** 少し前の値を出している (読み直せなかった) */
  stale?: boolean;
}

function durText(v: number | null | undefined): string {
  return v === null || v === undefined ? '-' : formatDuration(Math.round(v));
}

/** 比べる文 (「チーム平均 47.5・3位/8人」と短い「平均47.5・3位/8」) */
export function compareTexts(avgLabel: string, avg: string, rank: number | null, size: number): { compare: string; compareShort: string } {
  return {
    compare: `${avgLabel} ${avg}・${rank === null ? '順位なし' : `${String(rank)}位/${String(size)}人`}`,
    compareShort: `平均${avg}・${rank === null ? '順位なし' : `${String(rank)}位/${String(size)}`}`,
  };
}

/** 帯に出す項目。並びは `order` (設定) のとおり。各項目は「自分の値（チーム平均 ○○・○位/○人）」にそろえる。
 *  自分の項目 (`c-…`) はここでは作らない (呼び出し側が差し込む) */
export function stripItems(res: TodayMetricsResponse, order: readonly string[] = DEFAULT_ITEMS): StripItem[] | null {
  if (res.me === null) return null;
  const m = res.me.metrics;
  const a = res.team_avg;
  const r = res.me.rank;
  const avgLabel = res.population === 'team' ? 'チーム平均' : '全体平均';
  const item = (key: BuiltinKey, value: string, avg: string, rank: number | null, extra: string | null = null): StripItem =>
    ({ key, label: BUILTIN_LABELS[key], value, ...compareTexts(avgLabel, avg, rank, res.population_size), extra });
  const shown = new Set(order);
  const over = (key: 'over_30s' | 'over_60s' | 'over_90s' | 'over_360s', rate: number | null) =>
    item(key, rate === null ? String(m[key]) : `${String(m[key])}・${formatRate(rate)}`, formatAvg(a?.[key]), r[key]);
  const build = (key: BuiltinKey): StripItem | null => {
    switch (key) {
      // 着信を別の項目で出すときは、架電数に添えない
      case 'outbound': return item(key, String(m.outbound), formatAvg(a?.outbound), r.outbound, m.inbound > 0 && !shown.has('inbound') ? `着信 ${String(m.inbound)}` : null);
      case 'inbound': return item(key, String(m.inbound), formatAvg(a?.inbound), r.inbound);
      case 'over_30s': return over(key, m.rate_30s);
      case 'over_60s': return over(key, m.rate_60s);
      case 'over_90s': return over(key, m.rate_90s);
      case 'over_360s': return over(key, m.rate_360s);
      // 1 件平均を別の項目で出すときは、通話時間に添えない
      case 'duration': return item(key, m.avg_duration_ms === null || shown.has('avg_duration') ? formatDuration(m.total_duration_ms) : `${formatDuration(m.total_duration_ms)}・1件 ${formatDuration(m.avg_duration_ms)}`, a ? formatDuration(Math.round(a.total_duration_ms)) : '-', r.total_duration);
      case 'avg_duration': return item(key, formatDuration(m.avg_duration_ms), durText(a?.avg_duration_ms), r.avg_duration);
      // アポを数えないチームの人には、アポ欄をまるごと出さない (0 や — も出さない)
      case 'appointments': return res.appointments_tracked ? item(key, String(m.appointments), formatAvg(a?.appointments), r.appointments) : null;
    }
  };
  return order.filter(isBuiltinKey).map(build).filter((x): x is StripItem => x !== null);
}

/** 「今月」の帯の項目。新規営業の人向け。今月で数えていない項目 (秒数・着信) は出さない */
export function monthStripItems(res: MonthMetricsResponse, order: readonly string[] = DEFAULT_ITEMS): StripItem[] | null {
  if (res.me === null) return null;
  const m = res.me.metrics;
  const a = res.team_avg;
  const r = res.me.rank;
  const shown = new Set(order);
  const item = (key: BuiltinKey, value: string, avg: string, rank: number | null, extra: string | null = null): StripItem =>
    ({ key, label: BUILTIN_LABELS[key], value, ...compareTexts('チーム平均', avg, rank, res.population_size), extra });
  const avgDur = a ? formatDuration(Math.round(a.total_duration_ms)) : '-';
  const build = (key: BuiltinKey): StripItem | null => {
    switch (key) {
      case 'outbound': return item(key, String(m.outbound), formatAvg(a?.outbound), r.outbound, `今日 ${String(m.today_outbound)}`);
      case 'duration': return item(key, m.avg_duration_ms === null || shown.has('avg_duration') ? formatDuration(m.total_duration_ms) : `${formatDuration(m.total_duration_ms)}・1件 ${formatDuration(m.avg_duration_ms)}`, avgDur, r.total_duration);
      case 'avg_duration': return item(key, formatDuration(m.avg_duration_ms), durText(a?.avg_duration_ms), r.avg_duration);
      case 'appointments': return item(key, String(m.appointments), formatAvg(a?.appointments), r.appointments);
      default: return null;
    }
  };
  return order.filter(isBuiltinKey).map(build).filter((x): x is StripItem => x !== null);
}

/** 「2026-10」→「10月」。読めなければ「今月」 */
export function monthName(month: string): string {
  const m = /^\d{4}-(\d{2})$/.exec(month);
  return m?.[1] === undefined ? '今月' : `${String(Number(m[1]))}月`;
}

export function populationLabel(res: TodayMetricsResponse): string {
  if (res.population === 'team') return `同じチーム（${res.team_names.join('・')}）で今日架電した ${String(res.population_size)} 人`;
  return `今日架電した ${String(res.population_size)} 人`;
}

// --- 管理者の表 ---------------------------------------------------------

export type SortKey =
  | 'name' | 'outbound' | 'inbound' | 'over_30s' | 'over_90s' | 'over_360s'
  | 'rate_90s' | 'total_duration' | 'avg_duration' | 'appointments' | 'first' | 'last';

type Row = AllCallersResponse['callers'][number];

function sortValue(r: Row, key: SortKey): string | number | null {
  const m: CallerMetrics = r.metrics;
  switch (key) {
    case 'name': return r.name;
    case 'outbound': return m.outbound;
    case 'inbound': return m.inbound;
    case 'over_30s': return m.over_30s;
    case 'over_90s': return m.over_90s;
    case 'over_360s': return m.over_360s;
    case 'rate_90s': return m.rate_90s;
    case 'total_duration': return m.total_duration_ms;
    case 'avg_duration': return m.avg_duration_ms;
    case 'appointments': return m.appointments;
    case 'first': return m.first_call_at;
    case 'last': return m.last_call_at;
  }
}

/** 並べ替え。値の無い行 (null) は昇順でも降順でも最後。同じ値は元の順を保つ */
export function sortRows(rows: readonly Row[], key: SortKey, dir: 'asc' | 'desc'): Row[] {
  const sign = dir === 'asc' ? 1 : -1;
  return rows
    .map((r, i) => ({ r, i, v: sortValue(r, key) }))
    .sort((a, b) => {
      if (a.v === null && b.v === null) return a.i - b.i;
      if (a.v === null) return 1;
      if (b.v === null) return -1;
      const c = typeof a.v === 'string' && typeof b.v === 'string' ? a.v.localeCompare(b.v, 'ja') : Number(a.v) - Number(b.v);
      return c !== 0 ? sign * c : a.i - b.i;
    })
    .map(x => x.r);
}

export function filterByTeam(rows: readonly Row[], team: string): Row[] {
  return team === '' ? [...rows] : rows.filter(r => r.teams.includes(team));
}

// --- 管理者の表 (今月) ---------------------------------------------------

export type MonthSortKey = 'name' | 'outbound' | 'today_outbound' | 'total_duration' | 'avg_duration' | 'appointments';
type MonthRow = MonthAllResponse['callers'][number];

function monthSortValue(r: MonthRow, key: MonthSortKey): string | number | null {
  const m = r.metrics;
  switch (key) {
    case 'name': return r.name;
    case 'outbound': return m.outbound;
    case 'today_outbound': return m.today_outbound;
    case 'total_duration': return m.total_duration_ms;
    case 'avg_duration': return m.avg_duration_ms;
    case 'appointments': return m.appointments;
  }
}

/** 並べ替え。値の無い行 (null) は昇順でも降順でも最後。同じ値は元の順を保つ */
export function sortMonthRows(rows: readonly MonthRow[], key: MonthSortKey, dir: 'asc' | 'desc'): MonthRow[] {
  const sign = dir === 'asc' ? 1 : -1;
  return rows
    .map((r, i) => ({ r, i, v: monthSortValue(r, key) }))
    .sort((a, b) => {
      if (a.v === null && b.v === null) return a.i - b.i;
      if (a.v === null) return 1;
      if (b.v === null) return -1;
      const c = typeof a.v === 'string' && typeof b.v === 'string' ? a.v.localeCompare(b.v, 'ja') : Number(a.v) - Number(b.v);
      return c !== 0 ? sign * c : a.i - b.i;
    })
    .map(x => x.r);
}

// --- 架空サンプル --------------------------------------------------------

const BASE: CallerMetrics = {
  outbound: 0, inbound: 0, over_30s: 0, over_60s: 0, over_90s: 0, over_360s: 0, rate_30s: null, rate_60s: null, rate_90s: null, rate_360s: null,
  talked: 0, total_duration_ms: 0, avg_duration_ms: null, appointments: 0, first_call_at: null, last_call_at: null,
};

function metrics(outbound: number, o30: number, o90: number, o360: number, talked: number, totalSec: number, apo: number, inbound: number, first: string, last: string): CallerMetrics {
  return {
    ...BASE, outbound, inbound, over_30s: o30, over_60s: Math.round((o30 + o90) / 2), over_90s: o90, over_360s: o360, talked,
    rate_30s: o30 / outbound, rate_60s: Math.round((o30 + o90) / 2) / outbound, rate_90s: o90 / outbound, rate_360s: o360 / outbound,
    total_duration_ms: totalSec * 1000, avg_duration_ms: Math.round((totalSec * 1000) / talked), appointments: apo,
    first_call_at: first, last_call_at: last,
  };
}

export const FIXTURE_AS_OF = '2026-10-13T05:05:00Z';

export const FIXTURE_TODAY: TodayMetricsResponse = {
  loading: false,
  as_of: FIXTURE_AS_OF,
  day: '2026-10-13',
  stale: false,
  me: {
    owner_id: '9001',
    metrics: metrics(52, 21, 11, 2, 24, 3312, 2, 3, '2026-10-13T00:03:00Z', '2026-10-13T05:01:00Z'),
    rank: { outbound: 3, total_duration: 2, over_90s: 2, appointments: 1, over_30s: 2, over_60s: 2, over_360s: 3, inbound: 2, avg_duration: 4 },
  },
  team_avg: {
    outbound: 47.5, over_30s: 17.2, over_60s: 12.6, over_90s: 8.4, over_360s: 1.1, rate_30s: 0.36, rate_60s: 0.27, rate_90s: 0.18, rate_360s: 0.02,
    total_duration_ms: 2_640_000, avg_duration_ms: 118_000, appointments: 1.1, inbound: 1.8,
  },
  population_size: 8,
  population: 'team',
  team_names: ['架空BPOチーム'],
  appointments_tracked: true,
  month_view: false,
  message: null,
};

export const FIXTURE_ALL: AllCallersResponse = {
  loading: false,
  as_of: FIXTURE_AS_OF,
  day: '2026-10-13',
  stale: false,
  teams: ['架空BPOチーム', '架空営業チーム'],
  callers: [
    { owner_id: '9001', name: '架空 一郎', teams: ['架空BPOチーム'], metrics: metrics(52, 21, 11, 2, 24, 3312, 2, 3, '2026-10-13T00:03:00Z', '2026-10-13T05:01:00Z') },
    { owner_id: '9002', name: 'サンプル 二郎', teams: ['架空BPOチーム'], metrics: metrics(61, 19, 8, 1, 22, 2520, 1, 0, '2026-10-13T00:10:00Z', '2026-10-13T05:04:00Z') },
    { owner_id: '9003', name: '架空 花子', teams: ['架空BPOチーム'], metrics: metrics(38, 20, 12, 3, 20, 4140, 3, 5, '2026-10-13T00:20:00Z', '2026-10-13T04:58:00Z') },
    { owner_id: '9004', name: 'ダミー 三郎', teams: ['架空営業チーム'], metrics: metrics(24, 9, 4, 0, 11, 1180, 0, 2, '2026-10-13T01:00:00Z', '2026-10-13T04:40:00Z') },
    { owner_id: '9005', name: '架空 四郎', teams: [], metrics: { ...BASE, inbound: 4, appointments: 1 } },
  ],
};

/** 新規営業の人から見た今日の成績 (month_view = true。画面は今月の帯を主にする) */
export const FIXTURE_TODAY_SALES: TodayMetricsResponse = {
  ...FIXTURE_TODAY,
  team_names: ['新規営業'],
  appointments_tracked: false,
  month_view: true,
};

export const FIXTURE_MONTH: MonthMetricsResponse = {
  loading: false,
  as_of: FIXTURE_AS_OF,
  month: '2026-10',
  stale: false,
  me: {
    owner_id: '9001',
    metrics: { outbound: 812, today_outbound: 52, talked: 301, total_duration_ms: 36_900_000, avg_duration_ms: 122_591, appointments: 9 },
    rank: { outbound: 3, total_duration: 2, appointments: 1, avg_duration: 2 },
  },
  team_avg: { outbound: 760.4, total_duration_ms: 34_200_000, avg_duration_ms: 118_000, appointments: 6.2 },
  population_size: 14,
  message: null,
};

const monthRow = (id: string, name: string, outbound: number, today: number, talked: number, totalSec: number, apo: number): MonthAllResponse['callers'][number] => ({
  owner_id: id, name, teams: ['新規営業'],
  metrics: { outbound, today_outbound: today, talked, total_duration_ms: totalSec * 1000, avg_duration_ms: talked > 0 ? Math.round((totalSec * 1000) / talked) : null, appointments: apo },
});

export const FIXTURE_MONTH_ALL: MonthAllResponse = {
  loading: false,
  as_of: FIXTURE_AS_OF,
  month: '2026-10',
  stale: false,
  callers: [
    monthRow('9011', '架空 太郎', 903, 61, 340, 41_800, 7),
    monthRow('9012', 'サンプル 花子', 851, 48, 322, 39_500, 11),
    monthRow('9001', '架空 一郎', 812, 52, 301, 36_900, 9),
    monthRow('9013', 'ダミー 次郎', 640, 0, 233, 27_100, 4),
    monthRow('9014', '架空 三郎', 55, 0, 0, 0, 1),
  ],
};
