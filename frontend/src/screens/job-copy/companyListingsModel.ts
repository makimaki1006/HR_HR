import type { CompanyDeal } from './companyDeals';
import { dealInputs } from './dealSummaryLoad';
import type { DealData } from './dealSummaryLoad';
import { companyRollup, dealCutoff, listingNumbers } from './dealSummaryModel';
import type { ListingNumbers } from './dealSummaryModel';
import type { Billing } from './DealSummary';
import { publicationLabel } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import { formatDateJst } from './format';
import { todayJst } from './timelineModel';

/** 求人は顧客内で1回。公開状況は kyuujin_status から正規化済みの索引の値を使う。 */
export function companyListingsModel(rows: readonly HubSpotListing[], datas: readonly DealData[], deals: readonly CompanyDeal[], billing: Billing, flags: ListingFlags = {}, sort: ListingSort = 'changed') {
  const inputs = companyRollup(datas.map(data => ({ id: data.id, name: data.name, inputs: dealInputs(data, billing), overlap: data.overlap }))).inputs;
  const asOf = dealCutoff(inputs) ?? todayJst();
  const items = [...new Map(rows.map(row => [row.id, row])).values()].map(row => {
    const input = inputs.find(input => input.id === row.id);
    const changedAt = row.summary?.body_changed_at ?? datas.map(data => data.bodyDates[row.id]).find(Boolean) ?? null;
    return { row, changedAt, live: publicationLabel(row.publication_status) === '掲載中', numbers: input ? listingNumbers(input, asOf) : null, flags: shownFlags(flags[row.id]),
      deals: deals.filter(deal => row.deal_ids?.includes(deal.id) === true || datas.some(data => data.id === deal.id && data.rows.some(item => item.id === row.id))) };
  });
  const sorted = sortListingItems(items, sort);
  return { live: sorted.filter(item => item.live), past: sorted.filter(item => !item.live), asOf };
}

/** ホーム（listing_home.rs の classify）が求人ごとに付けた分類のキー。求人 ID → キーの並び。 */
export type ListingFlags = Readonly<Record<string, readonly string[]>>;
export interface ListingFlagCard { key: string; title: string; rule: string }
export type ListingFlagsResponse = { status: 'preparing' | 'failed' } | { status: 'ready'; flags: ListingFlags; cards: ListingFlagCard[] };

/** 表に出す分類と、その短い名前。並びはホームのカードの順。対象外・判定できないは出さない。 */
export const FLAG_LABELS: Readonly<Record<string, string>> = {
  zero_stay: '応募がゼロのまま', to_zero: '応募がゼロになった', less: '応募が減った', more: '応募が増えた', flat: '応募はほぼ同じ',
  costly: '課金が高いまま応募が少ない', stale: 'データが止まっている',
};
const shownFlags = (keys: readonly string[] | undefined) => Object.keys(FLAG_LABELS).filter(key => keys?.includes(key) === true);

/** 「分類」の並べ替えで先に出す分類と、その順。ほかの分類（増えた・ほぼ同じ）は、この後に変更日の新しい順。 */
export const ATTENTION_ORDER = ['to_zero', 'zero_stay', 'costly', 'less', 'stale'] as const;

export type ListingSort = 'attention' | 'cpa' | 'apps' | 'billing' | 'changed';
export const LISTING_SORTS: readonly { key: ListingSort; label: string }[] = [
  { key: 'attention', label: '応募ゼロ・課金が高い求人を先に' },
  { key: 'cpa', label: '応募1件あたりが高い順' },
  { key: 'apps', label: '直近4週の応募が多い順' },
  { key: 'billing', label: '今の課金（4週）が多い順' },
  { key: 'changed', label: '最後に変えた日が新しい順' },
];

interface SortItem { row: HubSpotListing; changedAt: string | null; numbers: ListingNumbers | null; flags: readonly string[] }
const NO_APPLICATION_RANK = 1e13;
/** 大きいほど先。値がない行は向きによらず最後。応募1件あたりは、課金があり応募が0件の求人をいちばん高いものとして先に置く。 */
function sortValue(item: SortItem, sort: ListingSort): number | null {
  const n = item.numbers;
  switch (sort) {
    case 'attention': { const rank = ATTENTION_ORDER.findIndex(key => item.flags.includes(key)); return rank < 0 ? null : -rank; }
    case 'cpa': return n?.perApplication ?? (n?.billing && n.recent === 0 ? NO_APPLICATION_RANK + n.billing : null);
    case 'apps': return n?.recent ?? null;
    case 'billing': return n?.billing ?? null;
    case 'changed': return null;
  }
}
/** 同じ値の中は、最後に本文を変えた日が新しい順 → 求人名 → ID。 */
export function sortListingItems<T extends SortItem>(items: readonly T[], sort: ListingSort): T[] {
  const byChange = (a: T, b: T) => (b.changedAt ?? '').localeCompare(a.changedAt ?? '') || (a.row.title ?? '').localeCompare(b.row.title ?? '', 'ja') || a.row.id.localeCompare(b.row.id);
  return [...items].sort((a, b) => {
    const left = sortValue(a, sort); const right = sortValue(b, sort);
    if (left !== right) {
      if (left === null) return 1;
      if (right === null) return -1;
      return right - left;
    }
    return byChange(a, b);
  });
}

/** 応募1件あたり。課金があって応募が0件なら「応募0件」と書き、0円と区別する。 */
export function perApplicationLabel(numbers: { recent: number | null; billing: number | null; perApplication: number | null } | null, yen: (value: number | null) => string): string {
  if (numbers?.perApplication != null) return yen(numbers.perApplication);
  return numbers?.billing && numbers.recent === 0 ? '応募0件' : '—';
}

/** 掲載終了日を優先。終了予定日時は予定と明記し、実際の終了日と区別する。 */
export function companyListingPeriod(row: HubSpotListing): string {
  const start = formatDateJst(row.listing_started_on, '開始日不明');
  if (publicationLabel(row.publication_status) === '掲載中') return `${start} 〜 今日`;
  const ended = formatDateJst(row.listing_ended_on);
  if (ended) return `${start} 〜 ${ended}`;
  const end = formatDateJst(row.listing_planned_end_on);
  return `${start} 〜 ${end ? `${end}（終了予定）` : '終了日不明'}`;
}
