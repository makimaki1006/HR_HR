// 一覧の並べ替え・絞り込み・書き出し。表とスマホのカードで同じ決まりを使う。
import { TODAY_NUM, dateSearchText, dayNum, formatDateTimeJa, formatDayJa, kindOf, labelOf, parseLooseDay, parseTimeOfDay, sortByDate } from '../../columns';
import type { ApplicantRow, ColumnConfig, FunnelStage, ListFilter, ListSort, SheetConfig } from '../../types';
import { portalNow, portalYear } from '../../clock';

export type ColumnFilter = ListFilter;
export type SortSpec = ListSort;

/** 検索のための寄せ: 半角カナ・全角英数を寄せ（NFKC）、カタカナをひらがなにし、空白とハイフンを除いて小文字にする。 */
export const norm = (s: string): string =>
  s
    .normalize('NFKC')
    .replace(/[\u30a1-\u30f6]/g, (ch) => String.fromCharCode(ch.charCodeAt(0) - 0x60))
    .replace(/[-\s\u2212\u2010-\u2015]/g, '')
    .toLowerCase();

/** 検索。日付の列は、原文でも「10/23」「2026/10/23」でも当たる。refHeader は応募日の列（年の無い値の年を当てる）。 */
export function matchesQuery(row: ApplicantRow, cols: ColumnConfig[], q: string, refHeader?: string): boolean {
  const n = norm(q);
  if (n === '') return true;
  const ref = refHeader ? parseLooseDay(row.values[refHeader]) : null;
  return cols.some((c) => {
    const v = row.values[c.header] ?? '';
    const k = kindOf(c);
    return norm(k === 'date' || k === 'datetime' ? dateSearchText(v, ref) : v).includes(n);
  });
}

export function matchesFilters(row: ApplicantRow, filters: Record<string, ColumnFilter>): boolean {
  return Object.entries(filters).every(([h, f]) => {
    const v = row.values[h] ?? '';
    if (f.type === 'in') return f.values.includes(v);
    return norm(v).includes(norm(f.text));
  });
}

// ---- シートの基準の年 ----

const baseCache = new WeakMap<ApplicantRow[], { year: number; value: number | null }>();

/**
 * シートの基準の年の 1 月 1 日の通し番号。年付きの日付の値（「2025/10/19」「2025年10月19日」）で一番多い年。
 * 今年なら null（年の無い値は今まで通り今年として読む）。過去の年度のシートで「10月19日」を今年と取り違えないために使う。
 */
export function sheetBaseRef(sheet: Pick<SheetConfig, 'id' | 'columns'>, rows: ApplicantRow[] = []): number | null {
  const hit = baseCache.get(rows);
  if (hit?.year === portalYear()) return hit.value;
  const count = new Map<number, number>();
  const dateCols = sheet.columns.filter((c) => { const k = kindOf(c); return k === 'date' || k === 'datetime'; });
  for (const r of rows) {
    for (const c of dateCols) {
      const m = /^(\d{4})\s*[年/.-]/.exec((r.values[c.header] ?? '').trim());
      if (m) { const y = Number(m[1]); count.set(y, (count.get(y) ?? 0) + 1); }
    }
  }
  let best: number | null = null;
  let bestN = 0;
  for (const [y, n] of count) if (n > bestN) { best = y; bestN = n; }
  const out = best === null || best === portalYear() ? null : dayNum(best, 1, 1);
  baseCache.set(rows, { year: portalYear(), value: out });
  return out;
}

/** 行の応募日の通し番号（年の無い値の年を当てる基準）。応募日の列が無い・読めなければ null。 */
export function rowRefDay(sheet: Pick<SheetConfig, 'id' | 'columns' | 'sortColumn'>, row: ApplicantRow, rows: ApplicantRow[] = [row]): number | null {
  if (!sheet.sortColumn) return null;
  return parseLooseDay(row.values[sheet.sortColumn], sheetBaseRef(sheet, rows)) ?? sheetBaseRef(sheet, rows);
}

const numeric = (v: string): number | null => (/^\d+(\.\d+)?$/.test(v) ? Number(v) : null);

// ---- 面接の日時 ----

/** 日付の文字（と別の列の時刻）を日本時間の時刻（ミリ秒）にする。日付が読めなければ null。時刻が無ければ 0 時。ref は年のない日付に年を補う応募日の通し番号。 */
export function parseDateTime(v: string, time?: string, ref?: number | null): number | null {
  const day = parseLooseDay(v, ref);
  if (day === null) return null;
  const minutes = parseTimeOfDay(time ?? /\d{1,2}:\d{2}/.exec(v)?.[0] ?? '') ?? 0;
  return day * 86_400_000 - 9 * 3_600_000 + minutes * 60_000;
}

/** 試作の「いま」からの日数ずれ。0 = 今日、1 = 明日。 */
export function dayOffset(v: string, ref?: number | null): number | null {
  const day = parseLooseDay(v, ref);
  return day === null ? null : day - TODAY_NUM;
}

/** 行の面接の日時（ミリ秒）。面接の列と、別の列の時刻、応募日からの年の補いを使う。 */
export function interviewAt(sheet: Pick<SheetConfig, 'id' | 'columns' | 'interviewColumn' | 'interviewTimeColumn' | 'sortColumn'>, row: ApplicantRow, rows: ApplicantRow[] = [row]): number | null {
  if (!sheet.interviewColumn) return null;
  const ref = rowRefDay(sheet, row, rows);
  return parseDateTime(row.values[sheet.interviewColumn] ?? '', sheet.interviewTimeColumn ? (row.values[sheet.interviewTimeColumn] ?? '') : undefined, ref);
}

export function sortRows(rows: ApplicantRow[], sort: SortSpec | null, fallbackHeader: string | undefined, timeHeader?: string): ApplicantRow[] {
  if (!sort) return sortByDate(rows, fallbackHeader);
  if (sort.mode === 'changed') {
    const rank = (r: ApplicantRow): number => (r.changedSinceLastView === null ? 1 : 0);
    return sortByDate(rows, fallbackHeader).sort((a, b) => rank(a) - rank(b));
  }
  if (sort.mode === 'near') {
    const now = portalNow();
    return [...rows].sort((a, b) => {
      const at = (r: ApplicantRow): number | null => parseDateTime(r.values[sort.column] ?? '', timeHeader ? (r.values[timeHeader] ?? '') : undefined, fallbackHeader ? parseLooseDay(r.values[fallbackHeader]) : null);
      const ta = at(a);
      const tb = at(b);
      if (ta === null && tb === null) return b.rowNumber - a.rowNumber;
      if (ta === null) return 1;
      if (tb === null) return -1;
      const fa = ta >= now;
      const fb = tb >= now;
      if (fa !== fb) return fa ? -1 : 1;
      return fa ? ta - tb : tb - ta;
    });
  }
  const sign = sort.dir === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const va = a.values[sort.column] ?? '';
    const vb = b.values[sort.column] ?? '';
    if (va === '' && vb !== '') return 1;
    if (vb === '' && va !== '') return -1;
    const ra = fallbackHeader ? parseLooseDay(a.values[fallbackHeader]) : null;
    const rb = fallbackHeader ? parseLooseDay(b.values[fallbackHeader]) : null;
    const da = parseLooseDay(va, ra);
    const db = parseLooseDay(vb, rb);
    if (da !== null && db !== null) {
      if (da !== db) return (da - db) * sign;
      return b.rowNumber - a.rowNumber;
    }
    const na = numeric(va);
    const nb = numeric(vb);
    if (na !== null && nb !== null && na !== nb) return (na - nb) * sign;
    if (va !== vb) return va.localeCompare(vb, 'ja') * sign;
    return b.rowNumber - a.rowNumber;
  });
}

/** 段階 → 点の色。色は段階のまとまり（応募・選考中／面接／内定・採用／終了）だけを示す。見分けは札の文字で行う。 */
export function stageDot(stage: FunnelStage | undefined): string {
  switch (stage) {
    case 'applied':
    case 'screening': return 'bg-pt-stage-early';
    case 'interview_set':
    case 'interviewed': return 'bg-pt-select';
    case 'offer':
    case 'hired': return 'bg-pt-ok';
    case 'rejected':
    case 'declined': return 'bg-pt-faint';
    default: return 'bg-pt-line-strong';
  }
}

export function statusDot(sheet: SheetConfig, value: string): string {
  return stageDot(sheet.stageMap?.[value]);
}

const csvCell = (v: string): string => (/[",\n\r]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);

/** 表示中の行と列を CSV にする（Excel で開けるよう BOM を付ける）。 */
export function toCsv(rows: ApplicantRow[], cols: ColumnConfig[]): string {
  const lines = [cols.map((c) => csvCell(labelOf(c))).join(',')];
  for (const r of rows) lines.push(cols.map((c) => csvCell(r.values[c.header] ?? '')).join(','));
  return '﻿' + lines.join('\r\n');
}

export function downloadText(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/csv;charset=utf-8' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => { URL.revokeObjectURL(url); }, 1000);
}

/** 文字の見た目の幅のめやす（13px）。 */
const textWidth = (v: string): number => {
  let w = 0;
  for (const ch of v) w += ch.charCodeAt(0) < 0x80 ? 8.2 : 13.5;
  return w;
};

export const TITLE_MIN = 168;

/**
 * 列ごとの幅（px）。見出しと中身の長いほうに合わせて詰める。
 * 書けない列が場所を取りすぎず、書ける列が 1 画面に収まるようにする。
 */
export function columnWidths(cols: ColumnConfig[], rows: ApplicantRow[], titleHeader: string, statusHeader: string | undefined, refFor?: (r: ApplicantRow) => number | null): Record<string, number> {
  const sample = rows.slice(0, 300);
  const out: Record<string, number> = {};
  for (const c of cols) {
    const head = textWidth(labelOf(c)) + 36 + (c.writable ? 0 : 15);
    if (c.multiline) { out[c.header] = 240; continue; }
    const kind = kindOf(c);
    const isDate = kind === 'date' || kind === 'datetime';
    // 日付は画面に出す文字（「10/23（金）」「2025/10/19（日）」）で測る。
    const shownText = (r: ApplicantRow): string => {
      const v = r.values[c.header] ?? '';
      if (!isDate || v === '') return v;
      const ref = refFor ? refFor(r) : null;
      const f = kind === 'date' ? formatDayJa(v, ref) : formatDateTimeJa(v, undefined, ref);
      return f ? f.text : v;
    };
    const values = [...sample.map(shownText), ...(c.options ?? [])];
    const longest = values.reduce((m, v) => Math.max(m, textWidth(v)), 0);
    if (c.header === titleHeader) { out[c.header] = Math.round(Math.min(220, Math.max(TITLE_MIN, longest + 84))); continue; }
    const pad = c.header === statusHeader ? 54 : isDate ? 34 : 26;
    const floor = kind === 'datetime' ? 140 : kind === 'date' ? 92 : 56;
    out[c.header] = Math.round(Math.min(220, Math.max(floor, head, longest + pad)));
  }
  return out;
}

/** 表の初期の並び: 氏名、書ける列（選考状況を先頭）、そのほかはシートの順。 */
export function orderColumns(cols: ColumnConfig[], titleHeader: string, statusHeader: string | undefined): ColumnConfig[] {
  const title = cols.filter((c) => c.header === titleHeader);
  const rest = cols.filter((c) => c.header !== titleHeader);
  const writable = rest.filter((c) => c.writable).sort((a, b) => Number(b.header === statusHeader) - Number(a.header === statusHeader));
  return [...title, ...writable, ...rest.filter((c) => !c.writable)];
}

/** キーを 1 つ除いた写し。 */
export function omit<T>(obj: Record<string, T>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(obj).filter(([k]) => k !== key));
}
