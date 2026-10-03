// legacy_sales_kpi.js (旧画面 JS の逐語コピー) の型。テスト専用。
import type { Counts, StockList } from './types';

export const WD: readonly string[];
export function fmt(n: number | null | undefined): string;
export function pct(n: number | null | undefined): string;
export function md(s: string): string;
export function wd(s: string): string;
export function makeAgo(today: string): (s: string) => number;
export function makeSumIf(
  hidden: Set<string>,
): (byPerson: Record<string, Counts> | undefined, ok: (id: string) => boolean) => Counts;
export function avgLine(
  ab: { label: string; team: string | null; n: number } | null,
  total: number,
  unit?: string,
): string;
export function wowEl(now: number, prev: number | null, unit?: string, invert?: boolean): string;
export function growText(n: number | null | undefined): string;
export function growColor(n: number | null | undefined): string;
export interface LegacyStockRow {
  label: string;
  note?: string;
  sub?: boolean;
  key?: string;
  n: Counts;
  named: Counts;
}
export function makeLs(lists: readonly StockList[]): {
  escS: (v: unknown) => string;
  lsGet: (c: Counts | undefined, b: string) => number;
  lsAdd: (a: Counts, b: Counts | undefined) => Counts;
  lsPct: (n: number, d: number) => string;
  lsNames: (kind: string) => string[];
  lsRows: (l: StockList) => LegacyStockRow[];
};
