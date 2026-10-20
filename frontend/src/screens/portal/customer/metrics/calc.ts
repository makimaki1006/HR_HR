// 数字の画面の集計。表示中のシートの行と、選考状況 → 段階の対応から作る。
import type { ApplicantRow, ColumnConfig, FunnelStage, SheetConfig } from '../../types';
import { kindOf } from '../../columns';

/** 試作の「今日」。 */
export const TODAY = { y: 2026, m: 10, d: 20 };

/** 面接以降に進んだ人を数えるための順位。不採用・辞退は進み具合が分からないので順位に入れない。 */
const RANK: Record<FunnelStage, number> = {
  applied: 0,
  screening: 1,
  interview_set: 2,
  interviewed: 3,
  offer: 4,
  hired: 5,
  rejected: -1,
  declined: -1,
};

export const STAGE_LABEL: Record<FunnelStage, string> = {
  applied: '応募',
  screening: '選考中',
  interview_set: '面接設定',
  interviewed: '面接実施',
  offer: '内定',
  hired: '採用',
  rejected: '不採用',
  declined: '辞退',
};
export const STAGE_ORDER: FunnelStage[] = ['applied', 'screening', 'interview_set', 'interviewed', 'offer', 'hired', 'rejected', 'declined'];

export interface Tally {
  total: number;
  /** 段階ごとに「そこまで進んだ人」の累計。 */
  reached: { screening: number; interviewSet: number; interviewed: number; offer: number; hired: number };
  rejected: number;
  declined: number;
}

export function tally(rows: ApplicantRow[], sheet: SheetConfig): Tally {
  const t: Tally = { total: rows.length, reached: { screening: 0, interviewSet: 0, interviewed: 0, offer: 0, hired: 0 }, rejected: 0, declined: 0 };
  const col = sheet.statusColumn;
  for (const r of rows) {
    const status = col ? (r.values[col] ?? '') : '';
    const stage = sheet.stageMap?.[status];
    if (stage === 'rejected') t.rejected += 1;
    if (stage === 'declined') t.declined += 1;
    const rank = stage ? RANK[stage] : 0;
    if (rank >= 1) t.reached.screening += 1;
    if (rank >= 2) t.reached.interviewSet += 1;
    if (rank >= 3) t.reached.interviewed += 1;
    if (rank >= 4) t.reached.offer += 1;
    if (rank >= 5) t.reached.hired += 1;
  }
  return t;
}

export interface Rate {
  num: number;
  den: number;
  /** 0 から 1。分母が 0 のときは null。 */
  value: number | null;
}
const rate = (num: number, den: number): Rate => ({ num, den, value: den === 0 ? null : num / den });

export interface Kpis {
  count: number;
  interview: Rate;
  held: Rate;
  offer: Rate;
  hire: Rate;
  decline: Rate;
}

export function kpis(t: Tally): Kpis {
  return {
    count: t.total,
    interview: rate(t.reached.interviewSet, t.total),
    held: rate(t.reached.interviewed, t.reached.interviewSet),
    offer: rate(t.reached.offer, t.total),
    hire: rate(t.reached.hired, t.total),
    decline: rate(t.declined, t.total),
  };
}

export function pct(v: number | null): string {
  if (v === null) return '—';
  return String(Math.round(v * 1000) / 10) + '%';
}

// ---- 期間 ----

export type Period = 'this' | 'last' | 'last3' | 'all' | 'custom';
/** 日付を通し番号にしたもの（両端を含む）。 */
export interface Range { from: number; to: number }

export const dayNum = (y: number, m: number, d: number): number => Math.floor(Date.UTC(y, m - 1, d) / 86400000);
const todayNum = dayNum(TODAY.y, TODAY.m, TODAY.d);

export function parseDay(s: string | undefined): number | null {
  if (!s) return null;
  const m = /^(\d{4})[/-](\d{1,2})[/-](\d{1,2})/.exec(s);
  if (!m) return null;
  return dayNum(Number(m[1]), Number(m[2]), Number(m[3]));
}

export function rangesOf(period: Period, customFrom: string, customTo: string): { cur: Range | null; prev: Range | null; prevLabel: string } {
  const { y, m, d } = TODAY;
  if (period === 'this') {
    const from = dayNum(y, m, 1);
    const pFrom = dayNum(y, m - 1, 1);
    return { cur: { from, to: todayNum }, prev: { from: pFrom, to: pFrom + (todayNum - from) }, prevLabel: '先月の同じ日まで' };
  }
  if (period === 'last') {
    return { cur: { from: dayNum(y, m - 1, 1), to: dayNum(y, m, 1) - 1 }, prev: { from: dayNum(y, m - 2, 1), to: dayNum(y, m - 1, 1) - 1 }, prevLabel: '先々月' };
  }
  if (period === 'last3') {
    const from = dayNum(y, m - 3, d);
    return { cur: { from, to: todayNum }, prev: { from: dayNum(y, m - 6, d), to: from - 1 }, prevLabel: 'その前の 3 か月' };
  }
  if (period === 'custom') {
    const f = parseDay(customFrom);
    const t = parseDay(customTo);
    if (f === null || t === null || t < f) return { cur: null, prev: null, prevLabel: '' };
    const len = t - f + 1;
    return { cur: { from: f, to: t }, prev: { from: f - len, to: f - 1 }, prevLabel: '直前の同じ長さ' };
  }
  return { cur: null, prev: null, prevLabel: '' };
}

export function inRange(rows: ApplicantRow[], dateCol: string | undefined, range: Range | null): ApplicantRow[] {
  if (!range || !dateCol) return rows;
  return rows.filter((r) => {
    const n = parseDay(r.values[dateCol]);
    return n !== null && n >= range.from && n <= range.to;
  });
}

// ---- 内訳 ----

export function breakdownColumns(sheet: SheetConfig, rows: ApplicantRow[]): ColumnConfig[] {
  const skip = new Set([sheet.titleColumn, sheet.statusColumn, sheet.sortColumn]);
  const list = sheet.columns.filter((c) => {
    if (!c.visible || c.multiline || skip.has(c.header)) return false;
    const k = kindOf(c);
    if (k !== 'text' && k !== 'choice') return false;
    const kinds = new Set(rows.map((r) => r.values[c.header] ?? '').filter((v) => v !== ''));
    return kinds.size >= 2 && kinds.size <= 12;
  });
  const prefer = (c: ColumnConfig): number => (/媒体|職種/.test(c.header) ? 0 : 1);
  return list.sort((a, b) => prefer(a) - prefer(b));
}

export interface BreakdownRow {
  value: string;
  count: number;
  share: number;
  interview: Rate;
  hire: Rate;
}

export function breakdown(rows: ApplicantRow[], sheet: SheetConfig, header: string): BreakdownRow[] {
  const groups = new Map<string, ApplicantRow[]>();
  for (const r of rows) {
    const v = (r.values[header] ?? '') === '' ? '（未入力）' : (r.values[header] ?? '');
    groups.set(v, [...(groups.get(v) ?? []), r]);
  }
  return [...groups.entries()]
    .map(([value, list]) => {
      const k = kpis(tally(list, sheet));
      return { value, count: list.length, share: rows.length === 0 ? 0 : list.length / rows.length, interview: k.interview, hire: k.hire };
    })
    .sort((a, b) => b.count - a.count);
}

// ---- 月ごと ----

export interface MonthBar { label: string; applied: number; hired: number; from: number; to: number }

export function monthly(rows: ApplicantRow[], sheet: SheetConfig): MonthBar[] {
  const out: MonthBar[] = [];
  for (let i = 5; i >= 0; i -= 1) {
    const dt = new Date(Date.UTC(TODAY.y, TODAY.m - 1 - i, 1));
    const y = dt.getUTCFullYear();
    const m = dt.getUTCMonth() + 1;
    const range = { from: dayNum(y, m, 1), to: dayNum(y, m + 1, 1) - 1 };
    const list = inRange(rows, sheet.sortColumn, range);
    out.push({ label: String(m) + '月', applied: list.length, hired: tally(list, sheet).reached.hired, from: range.from, to: range.to });
  }
  return out;
}

/** 段階の対応に無い選択肢（シートの選択肢と、実際に入っている値）。 */
export function unmappedOptions(sheet: SheetConfig, rows: ApplicantRow[]): string[] {
  const col = sheet.columns.find((c) => c.header === sheet.statusColumn);
  const map = sheet.stageMap ?? {};
  const seen = new Set<string>([...(col?.options ?? []), ...rows.map((r) => (sheet.statusColumn ? (r.values[sheet.statusColumn] ?? '') : ''))]);
  return [...seen].filter((v) => v !== '' && map[v] === undefined);
}

/** 率の分母が少なく、傾向として読めない。 */
export const SMALL_DEN = 5;
export const isSmall = (r: Rate): boolean => r.den > 0 && r.den < SMALL_DEN;

/** 選択肢がその段階として 1 つも決まっていない段階。 */
export function missingStages(sheet: SheetConfig): Set<FunnelStage> {
  const used = new Set(Object.values(sheet.stageMap ?? {}));
  const out = new Set<FunnelStage>();
  for (const s of ['screening', 'interview_set', 'interviewed', 'offer', 'hired'] as const) if (!used.has(s)) out.add(s);
  return out;
}

/** シート内で一番古い応募日。 */
export function earliestDay(rows: ApplicantRow[], dateCol: string | undefined): number | null {
  if (!dateCol) return null;
  let min: number | null = null;
  for (const r of rows) {
    const n = parseDay(r.values[dateCol]);
    if (n !== null && (min === null || n < min)) min = n;
  }
  return min;
}
