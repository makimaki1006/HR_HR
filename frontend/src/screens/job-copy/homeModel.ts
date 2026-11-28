import type { Work } from './OwnerTodo';
import type { IndexProgress } from './ListingPreparationNotice';
export type CardKey = 'zero_stay' | 'to_zero' | 'less' | 'more' | 'flat' | 'costly' | 'stale';
export interface HomeRowData { id: string; title: string | null; company: string | null; media: 'hrh' | 'airwork'; recent_apps: number | null; prev_apps: number | null; recent_cost: number | null; latest_week_end: string | null }
export interface HomeCard { key: CardKey; title: string; rule: string; count: number; listings: HomeRowData[] }
export interface LatestDeal { name: string | null; stage_label: string | null; active: boolean; created_at: string | null }
export interface HomeCustomer { company_id: string; name: string | null; deals: number; active_deals: number; latest_deal: LatestDeal | null; live_listings: number; total_listings: number; recent_apps: number; prev_apps: number; recent_cost: number; cost_per_app: number | null }
export interface HomeData { cards: HomeCard[]; customers: HomeCustomer[]; live_without_company: number; median_cost_per_app: number | null; media_weeks?: MediaWeek[] }
/** 媒体ごとの実績の時点。最新の週が同じ求人が最も多い週と、それより新しい・古い・実績なしの件数（掲載中の求人）。 */
export interface MediaWeek { label: string; week_end: string | null; listings: number; newer: number; older: number; no_data: number; total: number }
/** 例「Indeed：最新の週が 10/11 の求人が最も多い（98件）・それより新しい 12件・実績なし 10件」。 */
export function mediaWeekText(source: MediaWeek, format: (day: string) => string): string {
  // HRハッカーの求人は Indeed に出ている。表の媒体名（HRハッカー）と同じものだと分かるように書く。
  const week = { ...source, label: source.label === 'Indeed' ? 'HRハッカー（Indeed）' : source.label };
  if (week.total === 0) return `${week.label}：掲載中の求人なし`;
  if (week.week_end === null) return `${week.label}：実績なし`;
  const rest = [week.newer > 0 && `それより新しい ${String(week.newer)}件`, week.older > 0 && `それより古い ${String(week.older)}件`, week.no_data > 0 && `実績なし ${String(week.no_data)}件`].filter(Boolean);
  return [`${week.label}：最新の週が ${format(week.week_end)} の求人が最も多い（${String(week.listings)}件）`, ...rest].join('・');
}
export type HomeResponse =
  | { status: 'preparing'; progress?: IndexProgress }
  | { status: 'ready'; work?: Work; index_built_at: string; today: string; companies_ok: boolean; rules: { trend_min_diff: number; trend_min_percent: number; cost_floor_yen: number; cost_per_app_times: number; stale_days: number; list_cap: number; delivery_pipelines?: { id: string; label: string }[] }; home: HomeData };

/** 'default' はサーバが返した順（契約中の案件がある顧客を先に、最新の案件の作成日が新しい順）。 */
export type CustomerSort = 'default' | 'name' | 'latest' | 'deals' | 'live' | 'apps' | 'cost' | 'cpa';
export const customerName = (customer: HomeCustomer) => customer.name ?? '顧客名なし';
const value = (customer: HomeCustomer, key: CustomerSort): number | string | null => {
  switch (key) {
    case 'default': return 0;
    case 'name': return customerName(customer);
    case 'latest': return customer.latest_deal?.created_at ?? null;
    case 'deals': return customer.active_deals;
    case 'live': return customer.live_listings;
    case 'apps': return customer.recent_apps;
    case 'cost': return customer.recent_cost;
    case 'cpa': return customer.cost_per_app;
  }
};
/** 並べ替え。値がない（応募 0 件で 1 件あたりが出ない）行は、向きによらず最後に置く。 */
export function sortCustomers(customers: readonly HomeCustomer[], key: CustomerSort, descending: boolean): HomeCustomer[] {
  if (key === 'default') return [...customers];
  return [...customers].sort((a, b) => {
    const left = value(a, key); const right = value(b, key);
    if (left === null && right === null) return 0;
    if (left === null) return 1;
    if (right === null) return -1;
    const order = typeof left === 'string' && typeof right === 'string' ? left.localeCompare(right, 'ja') : Number(left) - Number(right);
    return descending ? -order : order;
  });
}
/** 顧客名で探す。onlyActive のときは、契約中の案件（完了のステージでない納品管理の案件）がある顧客だけ。 */
export function filterCustomers(customers: readonly HomeCustomer[], query: string, onlyActive = false): HomeCustomer[] {
  const text = query.trim().toLocaleLowerCase('ja-JP');
  return customers.filter(customer => (!onlyActive || customer.active_deals > 0) && (!text || customerName(customer).toLocaleLowerCase('ja-JP').includes(text)));
}
export const dealCountLabel = (customer: HomeCustomer) => `${customer.active_deals.toLocaleString('ja-JP')} ／ 全 ${customer.deals.toLocaleString('ja-JP')}`;
export const listingCountLabel = (customer: HomeCustomer) => `${customer.live_listings.toLocaleString('ja-JP')} ／ 全 ${customer.total_listings.toLocaleString('ja-JP')}`;
export const yen = (amount: number | null) => amount === null ? '—' : `¥${amount.toLocaleString('ja-JP')}`;
/** 「前の 4 週 → 直近 4 週」。 */
export const appsChange = (prev: number | null, recent: number | null) => prev === null || recent === null ? '—' : `${prev.toLocaleString('ja-JP')} → ${recent.toLocaleString('ja-JP')} 件`;
export const mediaName = (media: HomeRowData['media']) => media === 'hrh' ? 'HRハッカー（Indeed）' : 'AirWork';
