import { expect, it } from 'vitest';
import { companyListingsModel } from './companyListingsModel';
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
