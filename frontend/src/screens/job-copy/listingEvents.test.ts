import { describe, expect, it } from 'vitest';
import type { CopyVersion, JobCopyRecord } from './data';
import { billingBands, buildListingEvents, eventChecks, eventCounts, eventWindows, resultText } from './listingEventsModel';
import { buildChart } from './listingChartModel';
import type { BillingEntry } from './timelineModel';

const version = (id: string, publishedFrom: string, salary: string, extra = ''): CopyVersion => ({
  id, label: id, observedAt: `${publishedFrom}T09:00:00+09:00`, publishedFrom: `${publishedFrom}T09:00:00+09:00`, certainty: 'confirmed', kind: 'published', source: 'test',
  body: `仕事内容\n決まったルートで配送します。${extra}\n給与：${salary}\n休日：土日祝`, applications: null, note: '',
});
const bill = (start: string, end: string, amountYen: number | null): BillingEntry => ({ source: 'csv', start, end, amountYen });

// 取得日 2026-10-09。応募は 8/05〜9/01 に 1 日 1 件、9/02〜10/09 に 1 日 2 件。
const byDate: Record<string, number> = {};
for (let day = new Date('2026-06-10T00:00:00Z'); day <= new Date('2026-10-09T00:00:00Z'); day = new Date(day.getTime() + 86_400_000)) {
  const date = day.toISOString().slice(0, 10);
  byDate[date] = date >= '2026-09-02' ? 2 : date >= '2026-08-05' ? 1 : 0;
}
const job: JobCopyRecord = {
  id: 'j', title: '配送ドライバー', company: '架空運輸', media: 'HRハッカー', mediaJobId: 'M1', location: '大分県大分市',
  overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-10-09T09:00:00+09:00', distributions: {}, byDate },
  versions: [
    version('v1', '2026-06-10', '時給 1,100 円'),
    version('v2', '2026-08-05', '時給 1,200 円'),
    version('v3', '2026-09-12', '時給 1,200 円', '1日平均40件を担当します。'),
  ],
};
// 61 日で 61,000 円 → 4 週 28,000 円。次の 61 日で 30,500 円 → 4 週 14,000 円。
const billing = [bill('2026-06-10', '2026-08-09', 61000), bill('2026-08-10', '2026-10-09', 30500)];

describe('出来事の取り出し', () => {
  const events = buildListingEvents(job, billing);
  it('古い順に番号を付ける。掲載開始・給与の変更・課金の減額・本文の変更', () => {
    expect(events.map(event => [event.n, event.date, event.kind])).toEqual([
      [1, '2026-06-10', 'start'], [2, '2026-06-10', 'billing'], [3, '2026-08-05', 'content'], [4, '2026-08-10', 'billing'], [5, '2026-09-12', 'content'],
    ]);
    expect(events[0]?.title).toBe('掲載を開始');
    expect(events[1]?.title).toBe('課金を開始（¥28,000／4週）');
    expect(events[2]?.title).toBe('給与を変更（時給 1,100 円 → 時給 1,200 円）');
    expect(events[3]?.title).toBe('課金を減額（¥28,000 → ¥14,000／4週）');
    expect(events[4]?.title).toBe('本文を変更');
  });
  it('同じ額が続く課金の期間は1つにまとめ、変わらない週は出来事にしない', () => {
    expect(billingBands(billing)).toEqual([{ start: '2026-06-10', end: '2026-08-09', per4: 28000 }, { start: '2026-08-10', end: '2026-10-09', per4: 14000 }]);
    expect(billingBands([bill('2026-08-01', '2026-08-28', 28000), bill('2026-08-29', '2026-09-25', 28000)])).toEqual([{ start: '2026-08-01', end: '2026-09-25', per4: 28000 }]);
  });
  it('課金の空白は停止と再開、20%未満の動きは出来事にしない', () => {
    const gap = buildListingEvents(job, [bill('2026-06-10', '2026-06-30', 21000), bill('2026-08-01', '2026-08-28', 28000), bill('2026-08-29', '2026-09-25', 30000)]).filter(event => event.kind === 'billing');
    expect(gap.map(event => [event.date, event.title])).toEqual([
      ['2026-06-10', '課金を開始（¥28,000／4週）'], ['2026-07-01', '課金を停止（¥28,000／4週 → なし）'], ['2026-08-01', '課金を再開（¥28,000／4週）'],
    ]);
  });
});

describe('前後4週の応募', () => {
  const events = buildListingEvents(job, billing);
  const at = (n: number) => events.find(event => event.n === n)!; // eslint-disable-line @typescript-eslint/no-non-null-assertion
  it('9/12 の本文の変更: 前の4週（8/15〜9/11）は 1×18 + 2×10 = 38、後の4週（9/12〜10/09）は 2×28 = 56', () => {
    expect(eventCounts(job, at(5), '2026-10-09', '2026-06-10')).toEqual({ before: 38, after: 56, beforeDays: 28, afterDays: 28 });
    expect(resultText(at(5), eventCounts(job, at(5), '2026-10-09', '2026-06-10'))).toBe('38 → 56 件');
  });
  it('取得日で切れる後の窓は日数を返す', () => {
    expect(eventCounts(job, at(5), '2026-09-20', '2026-06-10')).toMatchObject({ after: 18, afterDays: 9 });
  });
  it('掲載開始は前を持たず、最初の4週だけ', () => {
    const counts = eventCounts(job, at(1), '2026-10-09', '2026-06-10');
    expect(counts).toMatchObject({ before: null, after: 0 });
    expect(resultText(at(1), counts)).toBe('最初の4週 0 件');
  });
  it('応募の日別データが無い・取得日が不明なら件数は出さない', () => {
    expect(eventCounts(job, at(5), null, '2026-06-10')).toEqual({ before: null, after: null, beforeDays: 0, afterDays: 0 });
    expect(resultText(at(5), { before: null, after: null, beforeDays: 0, afterDays: 0 })).toBe('応募の件数が分かりません');
  });
  it('掲載から28日たたない出来事は、前の窓の日数が短い', () => {
    expect(eventCounts(job, { date: '2026-06-20', kind: 'content' }, '2026-10-09', '2026-06-10').beforeDays).toBe(10);
  });
  it('グラフの塗りは前28日と後28日（取得日で切る）', () => {
    expect(eventWindows(at(5), '2026-10-09')).toEqual({ before: { start: '2026-08-15', end: '2026-09-12' }, after: { start: '2026-09-12', end: '2026-10-09' } });
    expect(eventWindows(at(1), '2026-10-09').before).toBeNull();
  });
});

describe('ほかに応募を動かしそうなことの確認', () => {
  const events = buildListingEvents(job, billing);
  const at = (n: number) => events.find(event => event.n === n)!; // eslint-disable-line @typescript-eslint/no-non-null-assertion
  const rows = [
    { month: '2026-08', jobs: 100, viewers: null, employers: null, viewersPerJob: null },
    { month: '2026-09', jobs: 110, viewers: null, employers: null, viewersPerJob: null },
    { month: '2026-10', jobs: 111, viewers: null, employers: null, viewersPerJob: null },
  ];
  it('給与の変更（8/05）: 5日後の課金の減額は要注意、市場は職種が未指定なので比べない', () => {
    const checks = eventChecks(at(3), events, billing, { state: 'unset' }, '2026-10-09');
    expect(checks.map(check => [check.key, check.markLabel])).toEqual([['billing', '要注意'], ['market', '未指定'], ['other', '小さい']]);
    expect(checks[0]?.text).toBe('「課金を減額」が5日後（8/10）にあります。');
    expect(checks[1]?.text).toBe('職種が未指定のため比べていません。');
  });
  it('本文の変更（9/12）: 前後4週に課金も別の変更も無ければ小さい。市場は月の求人数 +10% なら要注意', () => {
    const checks = eventChecks(at(5), events, billing, { state: 'set', rows }, '2026-10-09');
    expect(checks.map(check => check.markLabel)).toEqual(['小さい', '要注意', '小さい']);
    expect(checks[1]?.text).toBe('競合の求人数 +11%（100件 → 111件）。');
  });
  it('課金の出来事自身には課金の確認を出さず、前後4週の本文の変更を「ほかの変更」に出す', () => {
    const checks = eventChecks(at(4), events, billing, { state: 'none' }, '2026-10-09');
    expect(checks.map(check => check.key)).toEqual(['market', 'other']);
    expect(checks[0]?.text).toBe('市場と比べないと指定されています。');
    expect(checks[1]).toMatchObject({ markLabel: '要注意', text: '「給与を変更」が5日前（8/05）にあります。' });
  });
  it('市場のデータが出来事の月に届いていないときは、いつまであるかを言って比べない', () => {
    const old = [{ month: '2026-07', jobs: 100, viewers: null, employers: null, viewersPerJob: null }, { month: '2026-08', jobs: 110, viewers: null, employers: null, viewersPerJob: null }];
    const check = eventChecks(at(5), events, billing, { state: 'set', rows: old }, '2026-10-09')[1];
    expect(check).toMatchObject({ key: 'market', markLabel: '未確認', text: '市場のデータは2026年8月までで、この期間にまだ届いていないため、比べていません。' });
  });
  it('課金のデータが無ければ未確認', () => {
    expect(eventChecks(at(5), events, [], { state: 'unset' }, '2026-10-09')[0]).toMatchObject({ key: 'billing', markLabel: '未確認' });
  });
  it('判定語（増えた・減った・効いた）を使わない', () => {
    const text = events.flatMap(event => eventChecks(event, events, billing, { state: 'set', rows }, '2026-10-09').map(check => check.text)).join('');
    expect(text).not.toMatch(/増えた|減った|効いた|効果/u);
  });
});

describe('グラフの配置', () => {
  const events = buildListingEvents(job, billing);
  const chart = buildChart(job, events, billing, '2026-10-09', new Date('2026-10-12T00:00:00+09:00'))!; // eslint-disable-line @typescript-eslint/no-non-null-assertion
  it('軸は掲載開始の週の月曜から、取得日の3日後（今日）まで', () => {
    expect(chart.start).toBe('2026-06-08');
    expect(chart.end).toBe('2026-10-12');
    expect(chart.totalDays).toBe(127);
  });
  it('週ごとの棒は取得日の週まで。応募のない週も0件で並ぶ', () => {
    expect(chart.bars[0]).toMatchObject({ start: '2026-06-08', count: 0 });
    expect(chart.bars.at(-1)).toMatchObject({ start: '2026-10-05', count: 10 });
    expect(chart.bars).toHaveLength(18);
  });
  it('ピンは出来事の数だけ。同じ日は別の段に分ける', () => {
    expect(chart.pins).toHaveLength(5);
    expect(chart.pins.slice(0, 2).map(pin => pin.row)).toEqual([0, 1]);
  });
  it('点線は取得日の翌日の位置', () => {
    expect(chart.cutoffPct).toBeCloseTo(daysFromStart('2026-10-10') / 127 * 100, 5);
  });
  it('応募の日別データが無ければグラフを作らない', () => {
    const rest = { ...job }; delete rest.overallApplications;
    expect(buildChart(rest, events, billing, '2026-10-09')).toBeNull();
  });
});
function daysFromStart(date: string) { return (Date.parse(`${date}T00:00:00Z`) - Date.parse('2026-06-08T00:00:00Z')) / 86_400_000; }
