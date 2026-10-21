/**
 * 応募理由 × 地域 × 職種の集計 (純関数)。区分の辞書は reasonCategories.ts のものをそのまま使う。
 * 応募 1 件の区分 = 区分の選択 (未設定以外) があればそれ、無ければ 3 つの文をつないだキーワード判定、
 * どれにも当たらなければ「分類できない」。1 件の応募が複数の区分に入ることがある。
 */
import { REASON_CATEGORIES, MIN_SHARE_N, inferCategories, selectedCategory } from './reasonCategories';
import type { ReasonCategory } from './reasonCategories';

export type OccupationStatus = 'grouped' | 'missing' | 'unmatched' | 'conflict';
export interface ApplyReasonArea { prefecture: string | null; municipality: string | null }
export interface ApplyReasonRow {
  date: string;
  applicant_area: ApplyReasonArea;
  job_area: ApplyReasonArea;
  occupation: string | null;
  occupation_status: OccupationStatus;
  selections: { source_property: string; label: string }[];
  texts: { source_property: string; text: string }[];
}
export interface ApplyReasonPeriod { months: number; since: string; total: number; with_reason: number }
export interface ApplyReasonReport { today: string; periods: ApplyReasonPeriod[]; occupation_groups: string[]; rows: ApplyReasonRow[] }
export interface ApplyReasonResponse {
  status: 'ready' | 'preparing' | 'failed';
  built_at?: string | null; refreshing?: boolean; stale?: boolean; refresh_failed?: boolean;
  report?: ApplyReasonReport;
}

export type AreaKind = 'applicant' | 'job';
export const UNCLASSIFIED = '分類できない';
export const TABLE_COLUMNS = [...REASON_CATEGORIES, UNCLASSIFIED] as const;
export const SOURCE_LABELS: Record<string, string> = {
  oubodouki: '応募動機',
  kinmujoukennoyuusenjuni: '勤務条件の優先順位',
  honnin_kibou_kinyuuran: '本人希望記入欄',
};
export const UNKNOWN_MUNICIPALITY = '市区町村不明';
export const UNCATEGORIZED_OCCUPATION = '未分類';

export interface RowClass { categories: ReasonCategory[]; basis: 'selected' | 'estimated' | 'unclassified' }

export function classifyRow(row: ApplyReasonRow): RowClass {
  const chosen = new Set<ReasonCategory>();
  for (const s of row.selections) {
    const c = selectedCategory({ value: s.label, label: s.label });
    if (c && c !== 'unset') chosen.add(c);
  }
  if (chosen.size) return { categories: REASON_CATEGORIES.filter(c => chosen.has(c)), basis: 'selected' };
  const found = new Set(inferCategories(row.texts.map(t => t.text).join('\n')));
  const categories = REASON_CATEGORIES.filter(c => found.has(c));
  return categories.length ? { categories, basis: 'estimated' } : { categories: [], basis: 'unclassified' };
}

export interface Cells { counts: Record<string, number>; withReason: number }
export interface DetailText { key: string; categories: string; items: { source: string; text: string }[] }
export interface CrosstabRow extends Cells { key: string; label: string; children: CrosstabRow[]; details: DetailText[] }
const emptyCells = (): Cells => ({ counts: Object.fromEntries(TABLE_COLUMNS.map(c => [c, 0])), withReason: 0 });
function addTo(cells: Cells, cls: RowClass) {
  cells.withReason += 1;
  if (cls.categories.length) for (const c of cls.categories) cells.counts[c] = (cells.counts[c] ?? 0) + 1;
  else cells.counts[UNCLASSIFIED] = (cells.counts[UNCLASSIFIED] ?? 0) + 1;
}
function detailOf(row: ApplyReasonRow, cls: RowClass, index: number): DetailText {
  const items = [
    ...row.selections.map(s => ({ source: '区分の選択', text: s.label })),
    ...row.texts.map(t => ({ source: SOURCE_LABELS[t.source_property] ?? '記入欄', text: t.text })),
  ];
  return { key: `${row.date}-${String(index)}`, categories: cls.categories.length ? cls.categories.join('・') : UNCLASSIFIED, items };
}

export interface Excluded {
  noArea: number;
  noOccupation: number;
  occupationMissing: number; occupationUnmatched: number; occupationConflict: number;
  unclassified: number;
}
export interface Crosstab {
  period: ApplyReasonPeriod | null;
  inPeriod: number;
  excluded: Excluded;
  area: CrosstabRow[];
  occupation: CrosstabRow[];
  hiddenShare: boolean;
}

const byCountDesc = (a: CrosstabRow, b: CrosstabRow) => b.withReason - a.withReason || a.label.localeCompare(b.label, 'ja');

export function share(count: number, base: number): number | null {
  return base >= MIN_SHARE_N ? count / base : null;
}
export function formatShare(count: number, base: number): string | null {
  const s = share(count, base);
  return s === null ? null : `${(s * 100).toFixed(1)}%`;
}
export const formatCount = (n: number) => n.toLocaleString('ja-JP');

export function buildCrosstab(report: ApplyReasonReport, opts: { months: number; area: AreaKind; occupation: string | null }): Crosstab {
  const period = report.periods.find(p => p.months === opts.months) ?? null;
  const since = period?.since ?? '';
  const rows = report.rows.filter(r => r.date >= since);
  const excluded: Excluded = { noArea: 0, noOccupation: 0, occupationMissing: 0, occupationUnmatched: 0, occupationConflict: 0, unclassified: 0 };
  const prefs = new Map<string, CrosstabRow>();
  const occs = new Map<string, CrosstabRow>();
  const mk = (key: string, label: string): CrosstabRow => ({ ...emptyCells(), key, label, children: [], details: [] });
  rows.forEach((row, index) => {
    const cls = classifyRow(row);
    if (cls.basis === 'unclassified') excluded.unclassified += 1;
    const a = opts.area === 'job' ? row.job_area : row.applicant_area;
    const hasArea = a.prefecture !== null;
    if (!hasArea) excluded.noArea += 1;
    if (row.occupation === null) {
      excluded.noOccupation += 1;
      if (row.occupation_status === 'unmatched') excluded.occupationUnmatched += 1;
      else if (row.occupation_status === 'conflict') excluded.occupationConflict += 1;
      else excluded.occupationMissing += 1;
    }
    // 表 2: 期間だけで集計。職種が無い応募は「未分類」の行に入れる。
    const occKey = row.occupation ?? UNCATEGORIZED_OCCUPATION;
    let o = occs.get(occKey); if (!o) { o = mk(occKey, occKey); occs.set(occKey, o); }
    addTo(o, cls);
    // 表 1: 地域が読める応募のうち、職種の絞り込みに合うもの。
    if (!hasArea) return;
    if (opts.occupation !== null && row.occupation !== opts.occupation) return;
    const pref = a.prefecture ?? '';
    let p = prefs.get(pref); if (!p) { p = mk(pref, pref); prefs.set(pref, p); }
    addTo(p, cls);
    const detail = detailOf(row, cls, index);
    p.details.push(detail);
    const muniLabel = a.municipality ?? UNKNOWN_MUNICIPALITY;
    let m = p.children.find(c => c.label === muniLabel);
    if (!m) { m = mk(`${pref}/${muniLabel}`, muniLabel); p.children.push(m); }
    addTo(m, cls);
    m.details.push(detail);
  });
  const area = [...prefs.values()].sort(byCountDesc);
  for (const p of area) p.children.sort(byCountDesc);
  const order = report.occupation_groups;
  const occupation = [...occs.values()].sort((x, y) => {
    const ix = x.label === UNCATEGORIZED_OCCUPATION ? 9999 : !order.includes(x.label) ? 9998 : order.indexOf(x.label);
    const iy = y.label === UNCATEGORIZED_OCCUPATION ? 9999 : !order.includes(y.label) ? 9998 : order.indexOf(y.label);
    return ix - iy;
  });
  // 区分名は表の見出しに合わせるため、全ての行に全ての列があることを保つ。
  const hiddenShare = [...area, ...area.flatMap(p => p.children), ...occupation].some(r => r.withReason < MIN_SHARE_N);
  return { period, inPeriod: rows.length, excluded, area, occupation, hiddenShare };
}
