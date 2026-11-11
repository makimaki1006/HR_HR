import { expect, it } from 'vitest';
import { companyListingPeriod, companyListingsModel, perApplicationLabel, sortListingItems } from './companyListingsModel';
import { yen } from './homeModel';
import type { HubSpotListing } from './hubspotListings';
import type { CompanyDeal } from './companyDeals';
import type { DealData } from './dealSummaryLoad';
const row = (id: string, status: string | null, changed: string | null): HubSpotListing => ({ id, title: `求人${id}`, media: 'hrh', media_job_id: id, account_id: null, prefecture: null, municipality: null, category: null, publication_status: status, application_count: null, last_csv_detected_at: null, deal_ids: ['10', '20'], summary: { body_changed_at: changed, occupation: null, location: null, salary_kind: null, salary_min: null, salary_max: null, hours: null, holidays: null } });
const deals = [{ id: '10', name: '合成案件A' }, { id: '20', name: '合成案件B' }] as CompanyDeal[];
it('公開中を上、終了・掲載前・不明を下に分け、変更日時降順・求人名順で重複なく数える', () => {
  const rows = [row('2', '公開中', '2026-10-10'), row('3', '掲載終了', '2026-10-12'), row('1', '掲載中', '2026-10-10'), row('4', '公開開始前', null), row('5', null, null), row('6', '公開中', '2026-10-15')];
  const model = companyListingsModel([...rows, row('2', '公開中', '2026-10-10')], [], deals, {});
  expect(model.live.map(item => item.row.id)).toEqual(['6', '1', '2']);
  expect(model.past.map(item => item.row.id)).toEqual(['3', '4', '5']);
  expect([model.live.length, model.past.length]).toEqual([3, 3]);
  expect(model.live[0]?.deals.map(deal => deal.id)).toEqual(['10', '20']);
  expect(model.live[0]?.numbers).toBeNull();
});
it('直近4週の応募7件と課金28000円を案件間で重複加算せず、応募日不明を0にしない', () => {
  const rows = [row('1', '公開中', null), row('2', '掲載終了', null)];
  const data: DealData = { id: '10', name: '合成案件A', rows, capped: false, applications: new Map([['1', { '2026-10-13': 7, '2026-09-01': 100 }], ['2', null]]), shared: 0, overlap: {}, fetched: new Map([['1', '2026-10-15']]), bodyDates: { '1': '2026-10-11T00:00:00Z' }, perf: new Map(), company: null };
  const billing = { 'hubspot-history-1': [{ source: 'hrhacker' as const, start: '2026-09-18', end: '2026-10-15', amountYen: 28000, taxIncluded: null, media: 'HRハッカー', mediaJobId: '1' }] };
  const model = companyListingsModel(rows, [data, { ...data, id: '20' }], deals, billing);
  expect(model.asOf).toBe('2026-10-15');
  expect(model.live[0]?.numbers).toMatchObject({ recent: 7, billing: 28000 });
  expect(model.live[0]?.changedAt).toBe('2026-10-11T00:00:00Z');
  expect(model.past[0]?.numbers?.recent).toBeNull();
  expect(model.live).toHaveLength(1);
});

it('索引の開始日と終了予定日を表示し、掲載中は予定日に関係なく今日までにする', () => {
  const live = { ...row('1', '公開中', null), listing_started_on: '2026-09-02', listing_planned_end_on: '2027-10-31' };
  expect(companyListingPeriod(live)).toBe('2026/09/02 〜 今日');
  expect(companyListingPeriod({ ...live, publication_status: '掲載終了', listing_planned_end_on: '2026-09-30' })).toBe('2026/09/02 〜 2026/09/30（終了予定）');
  expect(companyListingPeriod({ ...live, publication_status: '掲載終了', listing_ended_on: '2026-10-15' })).toBe('2026/09/02 〜 2026/10/15');
  expect(companyListingPeriod({ ...live, listing_ended_on: '2026-10-15' })).toBe('2026/09/02 〜 今日');
  expect(companyListingPeriod({ ...live, listing_started_on: null })).toBe('開始日不明 〜 今日');
  expect(companyListingPeriod(row('2', '掲載終了', null))).toBe('開始日不明 〜 終了日不明');
});

it('前4週・直近4週・応募1件あたりは案件の画面と同じ計算（listingNumbers）で出す', () => {
  const rows = [row('1', '公開中', null)];
  // asOf 10/15: 直近4週 = 09/18〜10/15、前の4週 = 08/21〜09/17。
  const data: DealData = { id: '10', name: '合成案件A', rows, capped: false, applications: new Map([['1', { '2026-10-13': 7, '2026-09-10': 3, '2026-08-01': 50 }]]), shared: 0, overlap: {}, fetched: new Map([['1', '2026-10-15']]), bodyDates: {}, perf: new Map(), company: null };
  const billing = { 'hubspot-history-1': [{ source: 'hrhacker' as const, start: '2026-09-18', end: '2026-10-15', amountYen: 28000, taxIncluded: null, media: 'HRハッカー', mediaJobId: '1' }] };
  const numbers = companyListingsModel(rows, [data], deals, billing).live[0]?.numbers ?? null;
  expect(numbers).toMatchObject({ previous: 3, recent: 7, billing: 28000, perApplication: 4000 });
  expect(perApplicationLabel(numbers, yen)).toBe('¥4,000');
  expect(perApplicationLabel({ recent: 0, billing: 12000, perApplication: null }, yen)).toBe('応募0件');
  expect(perApplicationLabel({ recent: 0, billing: 0, perApplication: null }, yen)).toBe('—');
  expect(perApplicationLabel(null, yen)).toBe('—');
});

it('ホームの分類を求人に付け、対象外・判定できないは出さず、ホームのカードの順に並べる', () => {
  const rows = [row('1', '公開中', null), row('2', '公開中', null), row('3', '掲載終了', null)];
  const model = companyListingsModel(rows, [], deals, {}, { '1': ['costly', 'to_zero'], '2': ['unknown'], '3': ['out_of_scope'] });
  expect(model.live.map(item => [item.row.id, item.flags])).toEqual([['1', ['to_zero', 'costly']], ['2', []]]);
  expect(model.past[0]?.flags).toEqual([]);
});

const item = (id: string, changedAt: string | null, flags: string[], recent: number | null, billing: number | null) => ({
  row: row(id, '公開中', changedAt), changedAt, flags,
  numbers: { id, recent, previous: 0, billing, perApplication: billing !== null && recent ? Math.round(billing / recent) : null },
});
const ids = (items: readonly { row: { id: string } }[]) => items.map(i => i.row.id);
const items = [
  item('A', '2026-10-20', [], 10, 10000), // 1件あたり 1,000円
  item('B', '2026-10-01', ['to_zero', 'costly'], 0, 30000), // 応募0件・課金3万
  item('C', '2026-10-05', ['costly'], 2, 20000), // 1件あたり 10,000円
  item('D', '2026-10-10', ['more'], 8, null), // 課金不明
  item('E', '2026-10-02', ['zero_stay'], 0, 15000), // 応募0件・課金1.5万
  item('F', null, ['stale', 'less'], null, null), // 応募・課金とも不明
];
it('「応募ゼロ・課金が高い求人を先に」は ゼロになった → ゼロのまま → 課金が高い → 減った・止まっている、残りは変更日の新しい順', () => {
  expect(ids(sortListingItems(items, 'attention'))).toEqual(['B', 'E', 'C', 'F', 'A', 'D']);
});
it('応募1件あたりは応募0件で課金のある求人を最も高いとして先に（課金の多い順）、不明は最後', () => {
  expect(ids(sortListingItems(items, 'cpa'))).toEqual(['B', 'E', 'C', 'A', 'D', 'F']);
});
it('応募が多い順・課金が多い順・最後に変えた日が新しい順。値がない求人は最後', () => {
  expect(ids(sortListingItems(items, 'apps'))).toEqual(['A', 'D', 'C', 'E', 'B', 'F']); // 同じ0件は変更日の新しい E が先
  expect(ids(sortListingItems(items, 'billing'))).toEqual(['B', 'C', 'E', 'A', 'D', 'F']);
  expect(ids(sortListingItems(items, 'changed'))).toEqual(['A', 'D', 'C', 'E', 'B', 'F']);
});
