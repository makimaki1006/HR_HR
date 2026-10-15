import { describe, expect, it } from 'vitest';
import type { CopyVersion, JobCopyRecord } from './data';
import { noApplicationDatesText } from './detailBandModel';
import { buildListingEvents, earliestStart, eventCounts, resultText } from './listingEventsModel';
import { buildChart } from './listingChartModel';
import type { BillingEntry } from './timelineModel';

const version = (id: string, from: string, salary: string): CopyVersion => ({
  id, label: id, observedAt: `${from}T09:00:00+09:00`, publishedFrom: `${from}T09:00:00+09:00`, certainty: 'confirmed', kind: 'published', source: 'test',
  body: `仕事内容\n決まったルートで配送します。\n給与：${salary}\n休日：土日祝`, applications: null, note: '',
});
const perf = (start: string, end: string, cost: number, impressions: number): BillingEntry => ({ source: 'hrhacker', start, end, amountYen: cost, media: 'HRハッカー', impressions });
// Indeed の週ごとの実績: 6/15 の週は ¥20,000、6/22 の週は ¥30,000（+50%）、以降は同額。
const weeks = [perf('2026-06-15', '2026-06-21', 5000, 800), perf('2026-06-22', '2026-06-28', 7500, 900), perf('2026-06-29', '2026-07-05', 7500, 900)];
const base: JobCopyRecord = {
  id: 'j', title: '配送ドライバー', company: '架空運輸', media: 'HRハッカー', mediaJobId: 'M1', location: '大分県大分市', dataSource: 'hubspot',
  versions: [version('v1', '2026-10-09', '時給 1,100 円'), version('v2', '2026-10-20', '時給 1,200 円')],
};

describe('掲載開始のきめ方', () => {
  it('HubSpot の本文の履歴が 10/09 からでも、6/15 から課金があれば掲載開始は 6/15。最初の本文は「記録しはじめた」', () => {
    const events = buildListingEvents(base, weeks);
    expect(events.map(event => [event.date, event.kind, event.title])).toEqual([
      ['2026-06-15', 'start', '掲載を開始'],
      ['2026-06-15', 'billing', '課金を開始（¥20,000／4週）'],
      ['2026-06-22', 'billing', '課金を増額（¥20,000 → ¥30,000／4週）'],
      ['2026-10-09', 'content', 'HubSpot に本文を記録しはじめた'],
      ['2026-10-20', 'content', '給与を変更（時給 1,100 円 → 時給 1,200 円）'],
    ]);
    expect(events[0]?.summary).toBe('Indeed の週ごとの実績の最初の週から分かる、掲載を始めた日です。');
    expect(events[3]?.versionId).toBe('v1');
    // 課金の出来事は掲載開始より前に来ない。
    expect(events.filter(event => event.kind === 'billing').every(event => event.date >= (events[0]?.date ?? ''))).toBe(true);
  });
  it('公開開始日時がもっと前（6/01）なら、そちらが掲載開始', () => {
    const events = buildListingEvents({ ...base, listingStartedOn: '2026-06-01' }, weeks);
    expect(events[0]).toMatchObject({ date: '2026-06-01', kind: 'start' });
    expect(events[0]?.summary).toBe('HubSpot の公開開始日時から分かる、掲載を始めた日です。');
    expect(earliestStart({ listingStartedOn: '2026-06-01' }, weeks)).toEqual({ date: '2026-06-01', basis: 'HubSpot の公開開始日時' });
  });
  it('公開開始日時のほうが後（10/20）なら、いちばん早い 6/15 を使う', () => {
    expect(buildListingEvents({ ...base, listingStartedOn: '2026-10-20' }, weeks)[0]).toMatchObject({ date: '2026-06-15', kind: 'start' });
  });
  it('表示も課金も0の週は開始にしない', () => {
    expect(earliestStart({}, [perf('2026-06-08', '2026-06-14', 0, 0), ...weeks])?.date).toBe('2026-06-15');
  });
  it('ほかに根拠が無ければ、従来どおり最初の本文の日が掲載開始', () => {
    const events = buildListingEvents(base, []);
    expect(events.map(event => [event.date, event.kind, event.title])).toEqual([['2026-10-09', 'start', '掲載を開始'], ['2026-10-20', 'content', '給与を変更（時給 1,100 円 → 時給 1,200 円）']]);
  });
  it('最新の出来事（帯の表示）は一覧の最後と同じ', () => {
    expect(buildListingEvents(base, weeks).at(-1)?.title).toBe('給与を変更（時給 1,100 円 → 時給 1,200 円）');
  });
  it('本文の履歴が無くても、課金から掲載開始だけは出る', () => {
    expect(buildListingEvents({ ...base, versions: [] }, weeks).map(event => event.kind)).toEqual(['start', 'billing', 'billing']);
  });
});

describe('応募が0件の求人', () => {
  const zero: JobCopyRecord = { ...base, overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-10-25T09:00:00+09:00', distributions: {}, byDate: {} } };
  const events = buildListingEvents(zero, weeks);
  it('週ごとの棒は 6/15 の週から取得日の週まで全部 0 件で、最大は 0', () => {
    const chart = buildChart(zero, events, weeks, '2026-10-25', new Date('2026-10-25T12:00:00+09:00'));
    expect(chart).not.toBeNull();
    expect(chart?.bars[0]).toMatchObject({ start: '2026-06-15', count: 0 });
    expect(chart?.bars.every(bar => bar.count === 0)).toBe(true);
    expect(chart?.bars).toHaveLength(19);
  });
  it('出来事の前後4週は「分かりません」ではなく 0 → 0 件', () => {
    const change = events.at(-1);
    expect(change).toBeDefined();
    if (!change) return;
    const counts = eventCounts(zero, change, '2026-10-25', '2026-06-15');
    expect(counts).toMatchObject({ before: 0, after: 0 });
    expect(resultText(change, counts)).toBe('0 → 0 件');
  });
});

describe('応募の日別データが無いときの文', () => {
  it('読み込み中・失敗・記録元なしを分ける', () => {
    expect(noApplicationDatesText({ applicationDates: 'loading' })).toBe('応募を読み込み中です。');
    expect(noApplicationDatesText({ applicationDates: 'failed' })).toBe('応募を取得できませんでした。時間を置いて、もう一度開いてください。');
    expect(noApplicationDatesText({})).toBe('この求人の応募はまだ記録されていません。');
  });
});
