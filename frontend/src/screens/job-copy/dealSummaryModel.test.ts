import { describe, expect, it } from 'vitest';
import { dealCutoff, dealEvents, dealTotals, listingNumbers, mediaCutoffs, shareOfDeal, weeklySeries, weekStart } from './dealSummaryModel';
import type { DealListingInput } from './dealSummaryModel';
import type { BillingEntry } from './timelineModel';

const ASOF = '2026-10-12';
// 直近4週 = 2026-09-15〜2026-10-12、前の4週 = 2026-08-18〜2026-09-14
const bill = (start: string, end: string, amountYen: number | null): BillingEntry => ({ source: 'csv', start, end, amountYen });
const listing = (id: string, recent: number, previous: number, billing: BillingEntry[] = [], ended = false): DealListingInput => ({
  id, title: `求人${id}`, media: 'hrh', ended, billing, fetchedDate: ASOF,
  byDate: { ...(recent ? { '2026-10-01': recent } : {}), ...(previous ? { '2026-09-01': previous } : {}) },
});

const at = (items: readonly DealListingInput[], i: number): DealListingInput => { const v = items[i]; if (!v) throw new Error('fixture'); return v; };
describe('dealSummaryModel', () => {
  const listings = [
    listing('a', 20, 10, [bill('2026-09-15', '2026-10-12', 28000)]),
    listing('b', 9, 11, [bill('2026-09-15', '2026-10-12', 14000)]),
    listing('c', 10, 10, [bill('2026-09-15', '2026-10-12', 18000)]),
    listing('d', 0, 0, [bill('2026-01-01', '2026-01-31', 5000)], true),
  ];

  it('合計: 20/9/10/0 → 39件、前は31件、課金6万円 → 1件あたり1,538円', () => {
    const totals = dealTotals(listings, ASOF);
    expect(totals.recent).toBe(39);
    expect(totals.previous).toBe(31);
    expect(totals.billing).toBe(60000);
    expect(totals.perApplication).toBe(1538);
    expect([totals.listings, totals.live, totals.ended]).toEqual([4, 3, 1]);
    expect(totals.unknownBilling).toBe(0);
  });

  it('課金の期間が直近4週より長いときは日割り (56日で56,000円 → 28日で28,000円)', () => {
    const n = listingNumbers(listing('a', 10, 0, [bill('2026-08-18', '2026-10-12', 56000)]), ASOF);
    expect(n.billing).toBe(28000);
    expect(n.perApplication).toBe(2800);
  });

  it('応募日が未取得の求人は 0 にせず、合計に入れない', () => {
    const unknown: DealListingInput = { ...listing('x', 5, 0), byDate: null };
    const totals = dealTotals([listing('a', 4, 2), unknown], ASOF);
    expect(totals.recent).toBe(4);
    expect(totals.unknownApplications).toBe(1);
    expect(dealTotals([unknown], ASOF).recent).toBeNull();
  });

  it('課金が重なる求人・金額なしの求人は課金を null にして1件あたりも出さない', () => {
    const overlap = listing('o', 10, 0, [bill('2026-09-15', '2026-10-01', 1000), bill('2026-10-01', '2026-10-12', 1000)]);
    expect(listingNumbers(overlap, ASOF).billing).toBeNull();
    const blank = listing('b', 10, 0, [bill('2026-09-15', '2026-10-12', null)]);
    const totals = dealTotals([blank], ASOF);
    expect(totals.billing).toBeNull();
    expect(totals.perApplication).toBeNull();
    expect(totals.unknownBilling).toBe(1);
  });
  it('1件あたりは、全部の求人の課金と応募が分かるときだけ出す', () => {
    const noBilling = listing('n', 100, 0);
    const totals = dealTotals([listing('a', 10, 0, [bill('2026-09-15', '2026-10-12', 20000)]), noBilling], ASOF);
    expect(totals.perApplication).toBeNull();
    expect(totals.billing).toBeNull();
    expect(totals.recent).toBe(110);
    expect(dealTotals([listing('a', 10, 0, [bill('2026-09-15', '2026-10-12', 20000)])], ASOF).perApplication).toBe(2000);
  });

  it('割合: 20/39 = 51%、合計0なら null', () => {
    expect(shareOfDeal(20, 39)).toBe(51);
    expect(shareOfDeal(0, 0)).toBeNull();
    expect(shareOfDeal(null, 39)).toBeNull();
  });

  it('週ごと: 月曜はじまり、12週、課金は日割りの合計', () => {
    expect(weekStart('2026-10-12')).toBe('2026-10-12');
    expect(weekStart('2026-10-11')).toBe('2026-10-05');
    const series = weeklySeries(listings, ASOF);
    expect(series.weeks).toHaveLength(12);
    expect(series.weeks.at(-1)).toBe('2026-10-12');
    // 2026-10-01 (木) は 09-28 の週
    const index = series.weeks.indexOf('2026-09-28');
    expect(series.perListing[0]?.counts?.[index]).toBe(20);
    expect(series.total[index]).toBe(39);
    // 09-28〜10-04 の7日: (28000+14000+18000)/28*7 = 15000
    expect(series.billing?.[index]).toBe(15000);
  });

  it('出来事: 新しい順で、前後4週の案件の応募を数で出す。4週たっていなければ後は null', () => {
    const events = dealEvents(listings, [{ listingId: 'a', bodyChangedAt: '2026-10-05' }, { listingId: 'b', bodyChangedAt: '2026-09-02' }], ASOF, id => `求人${id}`, v => `${String(v)}円`);
    const dates = events.map(e => e.date);
    expect([...dates].sort().reverse()).toEqual(dates);
    const late = events.find(e => e.date === '2026-10-05' && e.kind === 'body');
    const early = events.find(e => e.date === '2026-09-02' && e.kind === 'body');
    // 前4週 08-05〜09-01: 前の応募は 09-01 のぶんは含まない(09-02 より前) → 09-01 の 10+11+10 = 31 は前に入る
    // 前4週 08-05〜09-01 は、応募データの最初の日 (09-01) より前にかかる → 数えない
    expect(early?.before).toBeNull();
    expect(early?.beforeIncomplete).toBe(true);
    // 後4週 09-02〜09-29: 10-01 は外 → 0
    expect(early?.after).toBe(0);
    expect(late?.beforeIncomplete).toBe(false);
    expect(late?.after).toBeNull();
    expect(late?.before).toBe(39);
  });

  it('基準日: 求人の取得日のいちばん早い日。取得日が違えば直近4週はその日まで', () => {
    const early = { ...listing('a', 0, 0), fetchedDate: '2026-10-10' };
    const late = { ...listing('b', 0, 0), fetchedDate: '2026-10-12' };
    expect(dealCutoff([early, late])).toBe('2026-10-10');
    expect(dealCutoff([{ ...early, byDate: null }, late])).toBe('2026-10-12');
    expect(dealCutoff([{ ...early, byDate: null }])).toBeNull();
    // 10-11 の応募は基準日 10-10 の外 (直近4週に入らない)
    const withLate = { ...listing('c', 0, 0), byDate: { '2026-10-10': 2, '2026-10-11': 5 } };
    expect(listingNumbers(withLate, '2026-10-10').recent).toBe(2);
  });

  it('重なり: 複数の求人に結び付いた応募は案件の合計で1回だけ (日ごとに引く)', () => {
    const overlap = { '2026-10-01': 3, '2026-08-20': 1 };
    const totals = dealTotals(listings, ASOF, overlap);
    expect(totals.recent).toBe(36); // 39 − 3
    expect(totals.previous).toBe(30); // 31 − 1 (08-20 は前の4週)
    expect(totals.perApplication).toBe(1667); // 60,000 ÷ 36
    const series = weeklySeries(listings, ASOF, overlap);
    expect(series.total[series.weeks.indexOf('2026-09-28')]).toBe(36);
    // 求人ごとの行は引かない
    expect(listingNumbers(at(listings, 0), ASOF).recent).toBe(20);
    const event = dealEvents(listings, [{ listingId: 'a', bodyChangedAt: '2026-10-05' }], ASOF, id => id, v => String(v), overlap).find(e => e.kind === 'body');
    expect(event?.before).toBe(36);
  });

  it('課金の不明: 終了した求人の課金が分からなければ合計は null。窓の外にしか課金が無い求人は 0円', () => {
    const noBilling = [...listings.slice(0, 3), listing('d', 0, 0, [], true)];
    const flagged = dealTotals(noBilling, ASOF);
    expect(flagged.unknownBilling).toBe(1);
    expect(flagged.billing).toBeNull();
    expect(flagged.perApplication).toBeNull();
    expect(weeklySeries(noBilling, ASOF).billing).toBeNull();
    expect(listingNumbers(at(listings, 3), ASOF).billing).toBe(0);
    expect(dealTotals(listings, ASOF).unknownBilling).toBe(0);
  });

  it('媒体ごとの最終日', () => {
    const cutoffs = mediaCutoffs([...listings, { ...listing('w', 1, 0, [bill('2026-09-01', '2026-09-30', 1)]), media: 'airwork' }]);
    expect(cutoffs).toEqual([
      { media: 'hrh', lastApplication: '2026-10-01', lastBilling: '2026-10-12' },
      { media: 'airwork', lastApplication: '2026-10-01', lastBilling: '2026-09-30' },
    ]);
  });
});
