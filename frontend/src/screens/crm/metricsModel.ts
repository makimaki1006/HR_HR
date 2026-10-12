import type { AllCallersResponse } from '../../generated/AllCallersResponse';
import type { CallerMetrics } from '../../generated/CallerMetrics';
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

export interface StripItem {
  key: 'outbound' | 'over_90s' | 'duration' | 'appointments';
  label: string;
  value: string;
  /** 補足 (90 秒以上の割合など) */
  sub: string | null;
  avg: string;
  rank: number | null;
}

/** 帯に出す 4 項目 */
export function stripItems(res: TodayMetricsResponse): StripItem[] | null {
  if (res.me === null) return null;
  const m = res.me.metrics;
  const a = res.team_avg;
  const r = res.me.rank;
  return [
    { key: 'outbound', label: '架電数', value: String(m.outbound), sub: m.inbound > 0 ? `着信 ${String(m.inbound)}` : null, avg: formatAvg(a?.outbound), rank: r.outbound },
    { key: 'over_90s', label: '90秒以上', value: String(m.over_90s), sub: m.rate_90s === null ? null : formatRate(m.rate_90s), avg: formatAvg(a?.over_90s), rank: r.over_90s },
    { key: 'duration', label: '通話時間合計', value: formatDuration(m.total_duration_ms), sub: m.avg_duration_ms === null ? null : `平均 ${formatDuration(m.avg_duration_ms)}`, avg: a ? formatDuration(Math.round(a.total_duration_ms)) : '-', rank: r.total_duration },
    { key: 'appointments', label: 'アポ', value: String(m.appointments), sub: null, avg: formatAvg(a?.appointments), rank: r.appointments },
  ];
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

// --- 架空サンプル --------------------------------------------------------

const BASE: CallerMetrics = {
  outbound: 0, inbound: 0, over_30s: 0, over_90s: 0, over_360s: 0, rate_30s: null, rate_90s: null, rate_360s: null,
  talked: 0, total_duration_ms: 0, avg_duration_ms: null, appointments: 0, first_call_at: null, last_call_at: null,
};

function metrics(outbound: number, o30: number, o90: number, o360: number, talked: number, totalSec: number, apo: number, inbound: number, first: string, last: string): CallerMetrics {
  return {
    ...BASE, outbound, inbound, over_30s: o30, over_90s: o90, over_360s: o360, talked,
    rate_30s: o30 / outbound, rate_90s: o90 / outbound, rate_360s: o360 / outbound,
    total_duration_ms: totalSec * 1000, avg_duration_ms: Math.round((totalSec * 1000) / talked), appointments: apo,
    first_call_at: first, last_call_at: last,
  };
}

export const FIXTURE_AS_OF = '2026-10-13T05:05:00Z';

export const FIXTURE_TODAY: TodayMetricsResponse = {
  as_of: FIXTURE_AS_OF,
  day: '2026-10-13',
  stale: false,
  me: {
    owner_id: '9001',
    metrics: metrics(52, 21, 11, 2, 24, 3312, 2, 3, '2026-10-13T00:03:00Z', '2026-10-13T05:01:00Z'),
    rank: { outbound: 3, total_duration: 2, over_90s: 2, appointments: 1 },
  },
  team_avg: {
    outbound: 47.5, over_30s: 17.2, over_90s: 8.4, over_360s: 1.1, rate_30s: 0.36, rate_90s: 0.18, rate_360s: 0.02,
    total_duration_ms: 2_640_000, avg_duration_ms: 118_000, appointments: 1.1,
  },
  population_size: 8,
  population: 'team',
  team_names: ['架空BPOチーム'],
  message: null,
};

export const FIXTURE_ALL: AllCallersResponse = {
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
