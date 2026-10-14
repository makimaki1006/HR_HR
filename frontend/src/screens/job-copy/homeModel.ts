export type CardKey = 'more' | 'same' | 'less' | 'costly' | 'stale' | 'occupation';
export interface HomeRowData { id: string; title: string | null; company: string | null; media: 'hrh' | 'airwork'; recent_apps: number | null; prev_apps: number | null; recent_cost: number | null; latest_week_end: string | null }
export interface HomeCard { key: CardKey; title: string; rule: string; count: number; listings: HomeRowData[] }
export interface HomeCustomer { company_id: string; name: string | null; deals: number; live_listings: number; recent_apps: number; prev_apps: number; recent_cost: number; cost_per_app: number | null }
export interface HomeData { cards: HomeCard[]; customers: HomeCustomer[]; live_without_company: number; median_cost_per_app: number | null }
export type HomeResponse =
  | { status: 'preparing' }
  | { status: 'ready'; index_built_at: string; today: string; companies_ok: boolean; rules: { trend_min_diff: number; cost_floor_yen: number; cost_per_app_times: number; stale_days: number; list_cap: number }; home: HomeData };

export type CustomerSort = 'name' | 'deals' | 'live' | 'apps' | 'cost' | 'cpa';
export const customerName = (customer: HomeCustomer) => customer.name ?? '顧客名未取得';
const value = (customer: HomeCustomer, key: CustomerSort): number | string | null => {
  switch (key) {
    case 'name': return customerName(customer);
    case 'deals': return customer.deals;
    case 'live': return customer.live_listings;
    case 'apps': return customer.recent_apps;
    case 'cost': return customer.recent_cost;
    case 'cpa': return customer.cost_per_app;
  }
};
/** 並べ替え。値がない（応募 0 件で 1 件あたりが出ない）行は、向きによらず最後に置く。 */
export function sortCustomers(customers: readonly HomeCustomer[], key: CustomerSort, descending: boolean): HomeCustomer[] {
  return [...customers].sort((a, b) => {
    const left = value(a, key); const right = value(b, key);
    if (left === null && right === null) return 0;
    if (left === null) return 1;
    if (right === null) return -1;
    const order = typeof left === 'string' && typeof right === 'string' ? left.localeCompare(right, 'ja') : Number(left) - Number(right);
    return descending ? -order : order;
  });
}
export function filterCustomers(customers: readonly HomeCustomer[], query: string): HomeCustomer[] {
  const text = query.trim().toLocaleLowerCase('ja-JP');
  return text ? customers.filter(customer => customerName(customer).toLocaleLowerCase('ja-JP').includes(text)) : [...customers];
}
export const yen = (amount: number | null) => amount === null ? '—' : `¥${amount.toLocaleString('ja-JP')}`;
/** 「前の 4 週 → 直近 4 週」。 */
export const appsChange = (prev: number | null, recent: number | null) => prev === null || recent === null ? '—' : `${String(prev)} → ${String(recent)} 件`;
export const mediaName = (media: HomeRowData['media']) => media === 'hrh' ? 'HRハッカー' : 'AirWork';
