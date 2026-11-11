import { describe, expect, it } from 'vitest';
import { dealCutoff, dealEvents, dealTotals, eventSourceOf, eventWeekMarks, listingNumbers, markLabel, mediaCutoffs, occupationStatus, perfBillingEntries, regionText, shareOfDeal, weeklySeries, weekStart } from './dealSummaryModel';
import type { DealListingInput } from './dealSummaryModel';
import type { BillingEntry } from './timelineModel';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing, HubSpotVersions } from './hubspotListings';
import { buildListingEvents } from './listingEventsModel';

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

  it('出来事の種類: 本文の変更はすべて、画像の変更、掲載の開始・終了・再開。番号は古い順', () => {
    const source = eventSourceOf({ id: 'a', media: 'hrh' }, '2026-10-05', {
      // 2026-09-30T20:00Z は JST では 10-01
      body: ['2026-09-10T00:00:00+00:00', '2026-09-30T20:00:00+00:00', '2026-10-05T00:00:00+00:00'],
      images: ['2026-10-05T03:00:00+00:00', '2026-09-20T00:00:00+00:00'],
      status: { baitai_genjoukyou_hrhakkaa: [
        { timestamp: '2026-08-20T00:00:00Z', value: '公開中' },
        { timestamp: '2026-09-25T00:00:00Z', value: '公開終了' },
        { timestamp: '2026-09-28T00:00:00Z', value: '公開中' },
      ] },
      started_on: '2026-08-15',
    }, []);
    expect(source.bodyChanges).toEqual(['2026-09-10', '2026-10-01', '2026-10-05']);
    expect(source.imageChanges).toEqual(['2026-09-20', '2026-10-05']);
    // 公開状況の最初の記録 (08-20) より公開開始日時 (08-15) が早い
    expect(source.startOn).toBe('2026-08-15');
    expect(source.statusEvents).toEqual([{ date: '2026-09-25', kind: 'end' }, { date: '2026-09-28', kind: 'revive' }]);
    const events = dealEvents(listings, [source], ASOF, id => `求人${id}`, v => `${String(v)}円`);
    const own = events.filter(e => e.listingId === 'a').map(e => [e.n, e.date, e.text]);
    // 全体の番号: 01-01 求人d の課金 = 1、09-15 は求人a・b・c の課金 = 4〜6
    expect(own).toEqual([
      [11, '2026-10-05', '求人a：本文と画像を変えました'],
      [10, '2026-10-01', '求人a：本文を変えました'],
      [9, '2026-09-28', '求人a：掲載を再開しました'],
      [8, '2026-09-25', '求人a：掲載を終えました'],
      [7, '2026-09-20', '求人a：画像を変えました'],
      [4, '2026-09-15', '求人a：課金が始まりました（28000円）'],
      [3, '2026-09-10', '求人a：本文を変えました'],
      [2, '2026-08-15', '求人a：掲載を始めました'],
    ]);
    // 番号は 1 から古い順で、表示は新しい順
    expect(events.map(e => e.n)).toEqual([11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    expect(events.at(-1)).toMatchObject({ n: 1, date: '2026-01-01', listingId: 'd' });
  });

  it('出来事: 履歴が届かない古い応答では、本文を最後に変えた日だけ', () => {
    const events = dealEvents([], [eventSourceOf({ id: 'a', media: 'hrh' }, '2026-10-05', undefined, [])], ASOF, id => id, v => String(v));
    expect(events.map(e => [e.n, e.date, e.kind])).toEqual([[1, '2026-10-05', 'body']]);
  });

  it('グラフの番号: 週（月曜はじまり）ごとにまとめ、期間の外は出さない', () => {
    const weeks = ['2026-09-21', '2026-09-28', '2026-10-05'];
    const marks = eventWeekMarks([
      { n: 1, date: '2026-09-01', listingId: 'a' },
      { n: 2, date: '2026-09-29', listingId: 'a' },
      { n: 3, date: '2026-10-04', listingId: 'b' },
      { n: 4, date: '2026-10-07', listingId: 'b' },
    ], weeks);
    expect(marks).toEqual([
      { index: 1, numbers: [2, 3], listingIds: ['a', 'b'] },
      { index: 2, numbers: [4], listingIds: ['b'] },
    ]);
    expect(markLabel([2, 3])).toBe('2・3');
    expect(markLabel([3, 4, 5])).toBe('3・4・5');
    expect(markLabel([3, 4, 5, 6])).toBe('3ほか3');
  });

  it('出来事: 応募と課金を読んだ求人（only）の分だけ出す。番号は詰める', () => {
    const events = dealEvents(listings, [{ listingId: 'a', bodyChangedAt: '2026-10-05' }, { listingId: 'b', bodyChangedAt: '2026-10-06' }], ASOF, id => id, v => String(v), {}, new Set(['a']));
    expect(events.map(e => [e.n, e.listingId, e.kind])).toEqual([[2, 'a', 'body'], [1, 'a', 'billing_start']]);
  });

  describe('掲載の開始・終了は求人の画面（buildListingEvents）と同じ日', () => {
    const row: HubSpotListing = { id: '7', media: 'hrh', media_job_id: 'h7', account_id: null, title: '求人7', prefecture: null, municipality: null, category: null, publication_status: '公開', last_csv_detected_at: null, application_count: null };
    const job = (bodyFirst: string, extra: Partial<HubSpotVersions> = {}) => listingRecord({ listing: row, versions: [{ written_at: bodyFirst, body: '本文1', image_urls: null }, { written_at: '2026-10-01T00:00:00Z', body: '本文2', image_urls: null }], history_counts: {}, history_may_be_incomplete: false, ...extra });
    // 表示だけあって費用が 0 の週も「表示か課金のある最初の週」に入る
    const billing = perfBillingEntries({ source: 'hrhacker', rows: [
      { period_start: '2026-07-06', period_end: '2026-07-12', cost_yen: 0, impressions: 120 },
      { period_start: '2026-07-13', period_end: '2026-07-19', cost_yen: 5000, impressions: 300 },
    ] }, 'h7');
    const startOf = (events: readonly { kind: string; date: string }[]) => events.find(e => e.kind === 'start')?.date;

    it('表示のある最初の週が、本文を記録しはじめた日より早いとき: その週', () => {
      const listingStart = startOf(buildListingEvents(job('2026-08-01T00:00:00Z', { listing_started_on: '2026-08-10' }), billing));
      const deal = eventSourceOf(row, null, { body: ['2026-10-01T00:00:00Z'], body_first: '2026-08-01T00:00:00Z', images: [], status: {}, started_on: '2026-08-10' }, billing);
      expect(listingStart).toBe('2026-07-06');
      expect(deal.startOn).toBe(listingStart);
    });

    it('本文を記録しはじめた日のほうが早いとき: その日', () => {
      const listingStart = startOf(buildListingEvents(job('2026-06-01T00:00:00Z', { listing_started_on: '2026-08-10' }), billing));
      const deal = eventSourceOf(row, null, { body: ['2026-10-01T00:00:00Z'], body_first: '2026-06-01T00:00:00Z', images: [], status: {}, started_on: '2026-08-10' }, billing);
      expect(listingStart).toBe('2026-06-01');
      expect(deal.startOn).toBe(listingStart);
    });

    it('公開状況の履歴に終了が無いとき: 媒体で非公開を記録した日', () => {
      const record = { ...job('2026-06-01T00:00:00Z'), mediaPublication: { start: null, plannedEnd: null, status: 'private' as const, lastInCsv: null, privateRecordedOn: '2026-10-21' } };
      const listingEnd = buildListingEvents(record, billing).find(e => e.kind === 'end')?.date;
      const deal = eventSourceOf(row, null, { body: [], body_first: '2026-06-01T00:00:00Z', images: [], status: {}, started_on: null, private_recorded_on: '2026-10-21' }, billing);
      expect(listingEnd).toBe('2026-10-21');
      expect(deal.statusEvents).toEqual([{ date: listingEnd, kind: 'end' }]);
      // 履歴に終了があれば、その日だけ（非公開を記録した日を重ねない）
      const withHistory = eventSourceOf(row, null, { body: [], private_recorded_on: '2026-10-21', status: { baitai_genjoukyou_hrhakkaa: [{ timestamp: '2026-08-01T00:00:00Z', value: '公開' }, { timestamp: '2026-10-19T00:00:00Z', value: '非公開' }] } }, billing);
      expect(withHistory.statusEvents).toEqual([{ date: '2026-10-19', kind: 'end' }]);
    });
  });

  it('職種の指定状況と地域', () => {
    expect(occupationStatus([
      { id: '1', indeed_title_state: 'set', publication_status: '公開' }, { id: '2', indeed_title_state: 'none', publication_status: '公開' },
      { id: '3', indeed_title_state: 'unset', publication_status: '公開' }, { id: '4', indeed_title_state: 'unset', publication_status: '公開終了' }, { id: '4', indeed_title_state: 'unset', publication_status: '公開終了' },
    ])).toEqual({ total: 4, set: 1, none: 1, unset: 2, unsetEnded: 1 });
    expect(occupationStatus([{ id: '1', publication_status: null }])).toBeNull();
    expect(regionText([
      { id: '1', prefecture: '東京都', municipality: '新宿区' }, { id: '2', prefecture: '東京都', municipality: '渋谷区' },
      { id: '3', prefecture: '東京都', municipality: '新宿区' }, { id: '4', prefecture: '神奈川県', municipality: null },
    ])).toBe('東京都 新宿区・渋谷区、神奈川県');
    expect(regionText([{ id: '1', prefecture: null, municipality: null }])).toBeNull();
    const many = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((city, i) => ({ id: String(i), prefecture: '大阪府', municipality: `${city}市` }));
    expect(regionText(many)).toBe('大阪府 a市・b市・c市・d市・e市 ほか2か所');
  });
});
