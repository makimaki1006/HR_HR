// 一覧の並べ替え・絞り込み・書き出し。表とスマホのカードで同じ決まりを使う。
import { kindOf, labelOf, sortByDate } from '../../columns';
import { stageOf } from '../../data';
import type { ApplicantRow, ColumnConfig, FunnelStage, SheetConfig } from '../../types';

export type ColumnFilter = { type: 'in'; values: string[] } | { type: 'has'; text: string };

export interface SortSpec {
  column: string;
  dir: 'asc' | 'desc';
}

export const norm = (s: string): string => s.replace(/[-\s]/g, '').toLowerCase();

export function matchesQuery(row: ApplicantRow, cols: ColumnConfig[], q: string): boolean {
  const n = norm(q);
  if (n === '') return true;
  return cols.some((c) => norm(row.values[c.header] ?? '').includes(n));
}

export function matchesFilters(row: ApplicantRow, filters: Record<string, ColumnFilter>): boolean {
  return Object.entries(filters).every(([h, f]) => {
    const v = row.values[h] ?? '';
    if (f.type === 'in') return f.values.includes(v);
    return norm(v).includes(norm(f.text));
  });
}

const numeric = (v: string): number | null => (/^\d+(\.\d+)?$/.test(v) ? Number(v) : null);

export function sortRows(rows: ApplicantRow[], sort: SortSpec | null, fallbackHeader: string | undefined): ApplicantRow[] {
  if (!sort) return sortByDate(rows, fallbackHeader);
  const sign = sort.dir === 'asc' ? 1 : -1;
  return [...rows].sort((a, b) => {
    const va = a.values[sort.column] ?? '';
    const vb = b.values[sort.column] ?? '';
    if (va === '' && vb !== '') return 1;
    if (vb === '' && va !== '') return -1;
    const na = numeric(va);
    const nb = numeric(vb);
    if (na !== null && nb !== null && na !== nb) return (na - nb) * sign;
    if (va !== vb) return va.localeCompare(vb, 'ja') * sign;
    return b.rowNumber - a.rowNumber;
  });
}

/** 段階 → 点の色。段階ごとに 1 色（終わった 2 つだけ同じ系統の灰色）。 */
export function stageDot(stage: FunnelStage | undefined): string {
  switch (stage) {
    case 'applied': return 'bg-pt-warn';
    case 'screening': return 'bg-pt-primary';
    case 'interview_set': return 'bg-pt-select';
    case 'interviewed': return 'bg-pt-select/45';
    case 'offer': return 'bg-pt-ok/45';
    case 'hired': return 'bg-pt-ok';
    case 'rejected': return 'bg-pt-faint';
    case 'declined': return 'bg-pt-line-strong';
    default: return 'bg-pt-line';
  }
}

export function statusDot(sheet: SheetConfig, value: string): string {
  return stageDot(stageOf(sheet, value));
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
export function columnWidths(cols: ColumnConfig[], rows: ApplicantRow[], titleHeader: string, statusHeader: string | undefined): Record<string, number> {
  const sample = rows.slice(0, 300);
  const out: Record<string, number> = {};
  for (const c of cols) {
    const head = textWidth(labelOf(c)) + 44 + (c.writable ? 0 : 15);
    if (c.multiline) { out[c.header] = 240; continue; }
    const values = [...sample.map((r) => r.values[c.header] ?? ''), ...(c.options ?? [])];
    const longest = values.reduce((m, v) => Math.max(m, textWidth(v)), 0);
    if (c.header === titleHeader) { out[c.header] = Math.round(Math.min(220, Math.max(TITLE_MIN, longest + 84))); continue; }
    const pad = c.header === statusHeader ? 50 : 30;
    const kind = kindOf(c);
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
