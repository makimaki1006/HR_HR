import type { WorkReady } from './OwnerTodo';
export type { DealBoardRow as DealRow } from '../../generated/DealBoardRow';
import type { DealBoardRow as DealRow } from '../../generated/DealBoardRow';
export type DealBoardResponse = { status: 'preparing' | 'failed' } | {
  status: 'ready'; hubspot_portal_id: string; rows: DealRow[]; work: WorkReady; sheet_as_of: string | null; refreshing: boolean; facts_built_at?: string | null; facts_stale?: boolean; refresh_failed: boolean; applications_status: 'ready' | 'preparing' | 'unavailable';
};
export const dealSorts = [
  ['priority', '印の多い順 → 金額の多い順'], ['name', '案件名'], ['company', '顧客名'], ['reasons', '要対応の印'],
  ['consultant', '担当'], ['stage', 'ステージ'], ['months', '何ヶ月目'], ['days_left', '満了まで'],
  ['last_contact', '最後の接触'], ['last_mtg', '最後のMTG'], ['nps', '定期NPS'], ['applications', '応募'],
  ['recent_applications', '応募（直近4週）'], ['interviews', '面接'], ['hires', '採用'], ['achievement', '採用目標に対して'], ['cpa', '採用単価'], ['amount', '金額'],
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

/** 応募ゼロを調べる期間。all は全期間（従来の「応募が1件も無い取引」）。 */
export type ZeroPeriod = 'all' | 'contract' | 'weeks' | 'dates';
export const zeroPeriods: readonly (readonly [ZeroPeriod, string])[] = [['all', '全期間'], ['contract', '初回契約の期間中'], ['weeks', '直近の週数'], ['dates', '日付を選ぶ']];
export const DEFAULT_PERIOD_WEEKS = 4;
const isDate = (value: string | null): value is string => !!value && /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(Date.parse(value));
/** 日本時間の今日（YYYY-MM-DD）。 */
export function jstToday(now = Date.now()): string { return new Date(now + 9 * 3600_000).toISOString().slice(0, 10); }
export function parseWeeks(value: string | null): number { return /^[1-9]\d{0,2}$/.test(value ?? '') ? Number(value) : DEFAULT_PERIOD_WEEKS; }
/** 直近 N 週は、今日を含む 7×N 日間。日付を選ぶときは両方そろって前後が正しいときだけ範囲になる。 */
export function periodRange(mode: ZeroPeriod, weeks: number, from: string | null, to: string | null, today: string): { from: string; to: string } | null {
  if (mode === 'weeks') return { from: new Date(Date.parse(`${today}T00:00:00Z`) - (weeks * 7 - 1) * 86_400_000).toISOString().slice(0, 10), to: today };
  if (mode === 'dates' && isDate(from) && isDate(to) && from <= to) return { from, to };
  return null;
}
const attached = (row: DealRow) => (row.listing_count ?? 0) >= 1;
/** 応募が1件も無い取引か。期間を指定したときは、その期間の応募日が0件の取引（求人票が付いていない取引は出さない）。 */
export function isZeroApplicant(row: DealRow, mode: ZeroPeriod): boolean {
  if (mode === 'all') return row.zero_applicants;
  if (!row.active || !attached(row)) return false;
  return mode === 'contract' ? row.first_contract && row.contract_applications === 0 : row.period_applications === 0;
}
/** 期間の応募数を数えられない取引（求人票はあるが応募日が分からない応募がある）。 */
export function periodUnknown(row: DealRow, mode: ZeroPeriod): boolean {
  if (mode === 'all' || !row.active || !attached(row)) return false;
  return mode === 'contract' ? row.first_contract && row.contract_applications == null : row.period_applications == null;
}
/** 面接の件数が 0 の取引。空（数字なし）は 0 に混ぜない。 */
export const hasNoInterview = (row: DealRow) => row.interviews === 0;
export const interviewMissing = (row: DealRow) => row.interviews === null;
