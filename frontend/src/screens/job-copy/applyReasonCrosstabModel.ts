/**
 * 応募理由 × 地域 × 職種の集計 (純関数)。理由はヒアリングで選んだ選択式の値をそのまま数える。
 * 応募動機 (oubodouki_1、複数選択) と勤務条件の優先順位①〜③ (kinmujoukennoyuusenjuni_1〜3) は別の表。
 * 文章は件数に使わず、詳細として添えるだけ。1 件の応募が複数の列に入ることがある。
 */
import { MIN_SHARE_N } from './reasonCategories';

export type OccupationStatus = 'grouped' | 'missing' | 'unmatched' | 'conflict';
export interface ApplyReasonArea { prefecture: string | null; municipality: string | null }
export interface ApplyReasonRow {
  date: string;
  applicant_area: ApplyReasonArea;
  job_area: ApplyReasonArea | null;
  occupation: string | null;
  occupation_status: OccupationStatus;
  selections: { source_property: string; label: string }[];
  texts: { source_property: string; text: string }[];
}
export interface ApplyReasonPeriod { months: number; since: string; total: number; with_reason: number }
export interface ApplyReasonReport { today: string; options: string[]; periods: ApplyReasonPeriod[]; occupation_groups: string[]; rows: ApplyReasonRow[] }
export interface ApplyReasonResponse {
  status: 'ready' | 'preparing' | 'failed';
  built_at?: string | null; refreshing?: boolean; stale?: boolean; refresh_failed?: boolean;
  report?: ApplyReasonReport;
}

export type AreaKind = 'applicant' | 'job';
export type ReasonKind = 'motive' | 'priority';
export const REASON_KINDS: { kind: ReasonKind; label: string }[] = [
  { kind: 'motive', label: '応募動機' },
  { kind: 'priority', label: '勤務条件の優先順位' },
];
export const MOTIVE_PROPERTY = 'oubodouki_1';
export const PRIORITY_PROPERTIES = ['kinmujoukennoyuusenjuni_1', 'kinmujoukennoyuusenjuni_2', 'kinmujoukennoyuusenjuni_3'];
export const SOURCE_LABELS: Record<string, string> = {
  oubodouki_1: '応募動機',
  kinmujoukennoyuusenjuni_1: '優先順位①',
  kinmujoukennoyuusenjuni_2: '優先順位②',
  kinmujoukennoyuusenjuni_3: '優先順位③',
  oubodouki: '応募動機の詳細',
  kinmujoukennoyuusenjuni: '勤務条件の詳細',
  honnin_kibou_kinyuuran: '本人希望記入欄',
};
export const UNKNOWN_MUNICIPALITY = '市区町村不明';
export const UNCATEGORIZED_OCCUPATION = '未分類';

const propertiesOf = (kind: ReasonKind) => kind === 'motive' ? [MOTIVE_PROPERTY] : PRIORITY_PROPERTIES;

/** その応募がこの表で選んだ値（重複なし）。空ならこの表には入らない。 */
export function labelsOf(row: ApplyReasonRow, kind: ReasonKind): string[] {
  const props = propertiesOf(kind);
  return [...new Set(row.selections.filter(s => props.includes(s.source_property)).map(s => s.label))];
}

/** 列 = HubSpot の選択肢の並び。選択肢に無い値（名前が変わった等）があれば後ろに足す。 */
export function columnsOf(report: ApplyReasonReport, kind: ReasonKind): string[] {
  const columns = [...report.options];
  for (const row of report.rows) for (const label of labelsOf(row, kind)) if (!columns.includes(label)) columns.push(label);
  return columns;
}

export interface Cells { counts: Record<string, number>; withReason: number }
export interface DetailText { key: string; labels: string; items: { source: string; text: string }[] }
export interface CrosstabRow extends Cells { key: string; label: string; children: CrosstabRow[]; details: DetailText[] }
function addTo(cells: Cells, labels: string[]) {
  cells.withReason += 1;
  for (const l of labels) cells.counts[l] = (cells.counts[l] ?? 0) + 1;
}
function detailOf(row: ApplyReasonRow, labels: string[], index: number): DetailText {
  const items = [
    ...row.selections.map(s => ({ source: SOURCE_LABELS[s.source_property] ?? '選択', text: s.label })),
    ...row.texts.map(t => ({ source: SOURCE_LABELS[t.source_property] ?? '記入欄', text: t.text })),
  ];
  return { key: `${row.date}-${String(index)}`, labels: labels.join('・'), items };
}

export interface Excluded {
  noArea: number;
  noOccupation: number;
  occupationMissing: number; occupationUnmatched: number; occupationConflict: number;
}
export interface Crosstab {
  period: ApplyReasonPeriod | null;
  columns: string[];
  /** この表に入る応募 (期間内でこの表の値がある応募) の件数。 */
  inTable: number;
  /** 選択式の記録がある最初の応募日。期間の起点より後なら、その日から数えていることを画面に出す。 */
  firstRecorded: string | null;
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

export function buildCrosstab(report: ApplyReasonReport, opts: { months: number; kind: ReasonKind; area: AreaKind; occupation: string | null }): Crosstab {
  const period = report.periods.find(p => p.months === opts.months) ?? null;
  const since = period?.since ?? '';
  const columns = columnsOf(report, opts.kind);
  const dates = report.rows.filter(r => labelsOf(r, opts.kind).length).map(r => r.date).sort();
  const firstRecorded = dates[0] ?? null;
  const rows = report.rows.filter(r => r.date >= since);
  const excluded: Excluded = { noArea: 0, noOccupation: 0, occupationMissing: 0, occupationUnmatched: 0, occupationConflict: 0 };
  const prefs = new Map<string, CrosstabRow>();
  const occs = new Map<string, CrosstabRow>();
  const mk = (key: string, label: string): CrosstabRow => ({ counts: Object.fromEntries(columns.map(c => [c, 0])), withReason: 0, key, label, children: [], details: [] });
  let inTable = 0;
  rows.forEach((row, index) => {
    const labels = labelsOf(row, opts.kind);
    if (!labels.length) return;
    inTable += 1;
    const a = opts.area === 'job' ? row.job_area : row.applicant_area;
    const hasArea = a !== null && a.prefecture !== null;
    if (!hasArea) excluded.noArea += 1;
    if (row.occupation === null) {
      excluded.noOccupation += 1;
      if (row.occupation_status === 'unmatched') excluded.occupationUnmatched += 1;
      else if (row.occupation_status === 'conflict') excluded.occupationConflict += 1;
      else excluded.occupationMissing += 1;
    }
    // 職種別: 期間だけで集計。職種が無い応募は「未分類」の行に入れる。
    const occKey = row.occupation ?? UNCATEGORIZED_OCCUPATION;
    let o = occs.get(occKey); if (!o) { o = mk(occKey, occKey); occs.set(occKey, o); }
    addTo(o, labels);
    // 地域別: 地域が読める応募のうち、職種の絞り込みに合うもの。
    if (!hasArea) return;
    if (opts.occupation !== null && row.occupation !== opts.occupation) return;
    const pref = a.prefecture ?? '';
    let p = prefs.get(pref); if (!p) { p = mk(pref, pref); prefs.set(pref, p); }
    addTo(p, labels);
    const detail = detailOf(row, labels, index);
    p.details.push(detail);
    const muniLabel = a.municipality ?? UNKNOWN_MUNICIPALITY;
    let m = p.children.find(c => c.label === muniLabel);
    if (!m) { m = mk(`${pref}/${muniLabel}`, muniLabel); p.children.push(m); }
    addTo(m, labels);
    m.details.push(detail);
  });
  const area = [...prefs.values()].sort(byCountDesc);
  for (const p of area) p.children.sort(byCountDesc);
  const order = report.occupation_groups;
  const rank = (label: string) => label === UNCATEGORIZED_OCCUPATION ? 9999 : !order.includes(label) ? 9998 : order.indexOf(label);
  const occupation = [...occs.values()].sort((x, y) => rank(x.label) - rank(y.label));
  const hiddenShare = [...area, ...area.flatMap(p => p.children), ...occupation].some(r => r.withReason < MIN_SHARE_N);
  return { period, columns, inTable, firstRecorded, excluded, area, occupation, hiddenShare };
}
