import { describe, expect, it } from 'vitest';
import { billingEvents, companyRollup, dealTotals, perfBillingEntries, shortYen, weeklySeries } from './dealSummaryModel';
import { addDays } from './timelineModel';
import type { DealListingInput } from './dealSummaryModel';

const ASOF = '2026-10-12';
// 直近4週 = 2026-09-15〜2026-10-12。週ごとの実績は 9/14〜10/11 の 4 週で、窓の中は 27 日分 (10/12 は実績なし)。
const week = (start: string, end: string, cost: number | null) => ({ period_start: start, period_end: end, cost_yen: cost });
const hrh = (id: string, recent: number, weeklyCost: number, extra: Partial<DealListingInput> = {}): DealListingInput => ({
  id, title: `求人${id}`, media: 'hrh', ended: false, fetchedDate: ASOF, byDate: recent ? { '2026-10-01': recent } : {},
  billing: perfBillingEntries({ rows: [week('2026-09-14', '2026-09-20', weeklyCost), week('2026-09-21', '2026-09-27', weeklyCost), week('2026-09-28', '2026-10-04', weeklyCost), week('2026-10-05', '2026-10-11', weeklyCost)] }, id),
  ...extra,
});
const air = (id: string, recent: number): DealListingInput => ({ id, title: `求人${id}`, media: 'airwork', ended: false, fetchedDate: ASOF, byDate: { '2026-10-01': recent }, billing: [], billingUnavailable: true });

describe('週ごとの実績から作る課金と、AirWork の扱い', () => {
  it('HRハッカー 2 件: 週 7,000 円 (1日 1,000 円) と週 3,500 円 (1日 500 円) を 27 日分', () => {
    const totals = dealTotals([hrh('a', 10, 7000), hrh('b', 5, 3500)], ASOF);
    expect(totals.billing).toBe(27000 + 13500);
    expect(totals.recent).toBe(15);
    expect(totals.perApplication).toBe(Math.round(40500 / 15));
  });

  it('AirWork は課金の合計・1件あたりに入れない (応募は数える)', () => {
    const totals = dealTotals([hrh('a', 10, 7000), air('w', 90)], ASOF);
    expect(totals.recent).toBe(100);
    expect(totals.billing).toBe(27000);
    expect(totals.excludedBilling).toBe(1);
    expect(totals.unknownBilling).toBe(0);
    // 課金のある求人の応募 10 件だけで割る: 27,000 ÷ 10 = 2,700 (100 件で割ると 270 円になってしまう)
    expect(totals.perApplication).toBe(2700);
  });

  it('AirWork だけの取引は課金の合計を出さない', () => {
    const totals = dealTotals([air('w', 3)], ASOF);
    expect(totals.billing).toBeNull();
    expect(totals.perApplication).toBeNull();
    expect(weeklySeries([air('w', 3)], ASOF).billing).toBeNull();
  });

  it('課金が分からない HRハッカー求人があれば、合計は出さない (0 円にしない)', () => {
    const none = hrh('n', 4, 0, { billing: [] });
    expect(dealTotals([hrh('a', 10, 7000), none], ASOF).billing).toBeNull();
  });

  it('週ごとの課金の帯に AirWork を足さない', () => {
    const series = weeklySeries([hrh('a', 1, 7000), air('w', 1)], ASOF);
    const index = series.weeks.indexOf('2026-09-21');
    expect(series.billing?.[index]).toBe(7000);
  });
});

describe('AirWork の週ごとの実績', () => {
  const awPerf = (id: string, weeklyCost: number) => perfBillingEntries({ source: 'airwork', rows: [week('2026-09-14', '2026-09-20', weeklyCost), week('2026-09-21', '2026-09-27', weeklyCost), week('2026-09-28', '2026-10-04', weeklyCost), week('2026-10-05', '2026-10-11', weeklyCost)] }, id);
  const airBilled = (id: string, recent: number, weeklyCost: number): DealListingInput => ({ ...air(id, recent), billing: awPerf(id, weeklyCost), billingUnavailable: false });
  it('実績のある AirWork は合計と 1件あたりに入る (週 3,500 円 = 1日 500 円 × 27 日)', () => {
    const totals = dealTotals([airBilled('w', 5, 3500)], ASOF);
    expect(totals.billing).toBe(13500);
    expect(totals.excludedBilling).toBe(0);
    expect(totals.perApplication).toBe(2700);
    expect(awPerf('w', 3500)[0]?.media).toBe('AirWork');
  });
  it('混ざった取引: 実績のない AirWork だけが外れる', () => {
    const totals = dealTotals([hrh('a', 10, 7000), airBilled('w', 5, 3500), air('x', 50)], ASOF);
    expect(totals.billing).toBe(27000 + 13500);
    expect(totals.excludedBilling).toBe(1);
    expect(totals.perApplication).toBe(Math.round(40500 / 15));
  });
});

describe('課金の出来事', () => {
  const wk = (start: string, cost: number | null) => ({ source: 'hrhacker' as const, start, end: addDays(start, 6), amountYen: cost });
  it('最後の週まで費用が続くなら、始まりだけで「止まった」は出ない。週ごとの上下は出来事にしない', () => {
    const events = billingEvents([wk('2026-09-14', 7000), wk('2026-09-21', 7100), wk('2026-09-28', 6900), wk('2026-10-05', 7200)]);
    expect(events).toEqual([{ date: '2026-09-14', kind: 'start', amountYen: 7000 }]);
  });
  it('途中で費用0が2週以上続くと、その最初の週に1件だけ出る (再開は始まりとして出る)', () => {
    const events = billingEvents([wk('2026-09-07', 5000), wk('2026-09-14', 0), wk('2026-09-21', 0), wk('2026-09-28', 0), wk('2026-10-05', 4000)]);
    expect(events).toEqual([
      { date: '2026-09-07', kind: 'start', amountYen: 5000 },
      { date: '2026-09-14', kind: 'stop', amountYen: 0 },
      { date: '2026-10-05', kind: 'start', amountYen: 4000 },
    ]);
  });
  it('費用0が1週だけなら止まったとはしない。費用0から始まる週は「始まり」にしない', () => {
    expect(billingEvents([wk('2026-09-07', 0), wk('2026-09-14', 3000), wk('2026-09-21', 0), wk('2026-09-28', 3000)])).toEqual([{ date: '2026-09-14', kind: 'start', amountYen: 3000 }]);
  });
});

describe('帯の短い金額', () => {
  it('9.4万・5,000円。0 と不明は空', () => {
    expect([shortYen(94286), shortYen(5000), shortYen(0), shortYen(Number.NaN)]).toEqual(['9.4万', '5000円', '', '']);
  });
});

describe('事業所の集計', () => {
  it('2 つの取引に結び付いた求人は、取引ごとには全部数え、事業所では 1 回だけ数える', () => {
    const shared = hrh('s', 10, 7000, { inOtherDeals: true });
    const d1 = { id: 'd1', name: '取引1', inputs: [shared, hrh('a', 5, 3500)], overlap: {} };
    const d2 = { id: 'd2', name: '取引2', inputs: [shared, hrh('b', 2, 1400)], overlap: {} };
    expect(dealTotals(d1.inputs, ASOF).recent).toBe(15);
    expect(dealTotals(d2.inputs, ASOF).recent).toBe(12);
    const company = companyRollup([d1, d2]);
    expect(company.inputs.map(input => input.id)).toEqual(['s', 'a', 'b']);
    const totals = dealTotals(company.inputs, ASOF, company.overlap);
    expect(totals.recent).toBe(17);
    expect(totals.listings).toBe(3);
    // 課金も 1 回だけ: 27,000 + 13,500 + 5,400
    expect(totals.billing).toBe(27000 + 13500 + 5400);
  });

  it('重なる応募は日ごとに取引のうち大きいほうを使う (二重に引かない)', () => {
    const rollup = companyRollup([
      { id: 'd1', name: '', inputs: [], overlap: { '2026-10-01': 2, '2026-10-02': 1 } },
      { id: 'd2', name: '', inputs: [], overlap: { '2026-10-01': 1 } },
    ]);
    expect(rollup.overlap).toEqual({ '2026-10-01': 2, '2026-10-02': 1 });
  });
});
