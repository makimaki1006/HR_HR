import type { WorkReady } from './OwnerTodo';
export type { DealBoardRow as DealRow } from '../../generated/DealBoardRow';
import type { DealBoardRow as DealRow } from '../../generated/DealBoardRow';
export type DealBoardResponse = { status: 'preparing' | 'failed' } | {
  status: 'ready'; rows: DealRow[]; work: WorkReady; sheet_as_of: string | null; refreshing: boolean; refresh_failed: boolean; applications_status: 'ready' | 'preparing' | 'unavailable';
};
export const dealSorts = [
  ['priority', '印の多い順 → 金額の多い順'], ['name', '案件名'], ['company', '顧客名'], ['reasons', '要対応の印'],
  ['consultant', '担当'], ['stage', 'ステージ'], ['months', '何ヶ月目'], ['days_left', '満了まで'],
  ['last_contact', '最後の接触'], ['last_mtg', '最後のMTG'], ['nps', '定期NPS'], ['applications', '応募'],
  ['interviews', '面接'], ['hires', '採用'], ['achievement', '採用目標に対して'], ['cpa', '採用単価'], ['amount', '金額'],
] as const;
export type DealSort = typeof dealSorts[number][0];
export function sortDeals(rows: DealRow[], sort: DealSort, descending: boolean): DealRow[] {
  const val = (row: DealRow) => sort === 'priority' || sort === 'reasons' ? row.reasons.length : row[sort];
  return [...rows].sort((a, b) => {
    const left = val(a); const right = val(b);
    if (left === null && right !== null) return 1;
    if (right === null && left !== null) return -1;
    const delta = typeof left === 'string' && typeof right === 'string' ? left.localeCompare(right, 'ja') : Number(left) - Number(right);
    return (sort === 'priority' ? -delta || (b.amount ?? -1) - (a.amount ?? -1) : descending ? -delta : delta) || a.id.localeCompare(b.id);
  });
}

/** 印の件数は、印による絞り込みの前の案件を1件ずつ数える。 */
export function countDealReasons(rows: readonly DealRow[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const row of rows) for (const kind of new Set(row.reasons.map(reason => reason.kind))) counts[kind] = (counts[kind] ?? 0) + 1;
  return counts;
}

export const launchGroups = [['warming', '立ち上がり中'], ['on_track', '目安どおり'], ['below', '目安より少ない'], ['unavailable', '未取得']] as const;
export function launchLabel(row: DealRow, preparing: boolean): string {
  if (row.launch.status === 'unavailable') return preparing ? '集計中' : '未取得';
  return (launchGroups.find(([key]) => key === row.launch.status)?.[1] ?? '未取得') + (row.launch.status === 'below' ? `（あと${String(row.launch.remaining)}件）` : '');
}
export function sortLaunch(rows: DealRow[]): DealRow[] {
  return [...rows].sort((a, b) => Number(b.launch.status === 'below') - Number(a.launch.status === 'below') || (b.launch.elapsed ?? -Infinity) - (a.launch.elapsed ?? -Infinity) || a.id.localeCompare(b.id));
}
