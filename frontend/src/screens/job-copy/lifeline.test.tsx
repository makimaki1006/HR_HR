// @vitest-environment happy-dom
import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { CopyVersion, JobCopyRecord } from './data';
import { LifelineChart, ListingOverview, ListingReasons } from './ListingOverview';
import { buildLifeline, changeMarkerLabel, COLUMN_PX, weekIndex } from './lifelineModel';
import { applicationEvents, applicantText, buildListingEvents, statusTimeline } from './listingEventsModel';
import type { BillingEntry } from './timelineModel';

afterEach(() => { cleanup(); });

const version = (id: string, from: string, salary: string, image = ''): CopyVersion => ({
  id, label: id, observedAt: `${from}T09:00:00+09:00`, publishedFrom: `${from}T09:00:00+09:00`, certainty: 'confirmed', kind: 'published', source: 'test',
  body: `仕事内容\n配送します。\n給与：${salary}\n休日：土日祝`, applications: null, note: '', ...(image ? { images: [{ id: image, url: `/x/${image}.png`, caption: image }] } : {}),
});
// 6/15(月)〜 の週ごとの実績。6/29 の週は課金0円。
const perfRows = [
  { period_start: '2026-06-15', period_end: '2026-06-21', impressions: 800, clicks: 30, cost_yen: 5000, applications: 1 },
  { period_start: '2026-06-22', period_end: '2026-06-28', impressions: 900, clicks: 35, cost_yen: 7500, applications: 0 },
  { period_start: '2026-06-29', period_end: '2026-07-05', impressions: 0, clicks: 0, cost_yen: 0, applications: 0 },
  { period_start: '2026-07-06', period_end: '2026-07-12', impressions: 700, clicks: 20, cost_yen: 7500, applications: 2 },
];
const billing: BillingEntry[] = perfRows.map(row => ({ source: 'hrhacker', start: row.period_start, end: row.period_end, amountYen: row.cost_yen, media: 'HRハッカー', impressions: row.impressions, clicks: row.clicks, mediaApplications: row.applications }));
const job: JobCopyRecord = {
  id: 'j', title: '配送', company: '架空運輸', media: 'HRハッカー', mediaJobId: '1234567', location: '大分県', dataSource: 'hubspot',
  hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: '1234567', captured_at: '2026-07-13T09:00:00+09:00', rows: perfRows },
  overallApplications: { total: 3, missingDate: 0, fetchedAt: '2026-07-13T09:00:00+09:00', distributions: {}, byDate: { '2026-06-16': 1, '2026-07-07': 2 } },
  applicationRows: [
    { key: 'a1', date: '2026-06-16', gender: '女性', age: '30代', prefecture: '大分県', municipality: '大分市' },
    { key: 'a2', date: '2026-07-07', gender: '男性', age: '40代', prefecture: null, municipality: null },
    { key: 'a3', date: '2026-07-07', gender: null, age: null, prefecture: null, municipality: null },
  ],
  statusHistory: [
    { at: '2026-06-15T00:00:00Z', value: '公開' }, { at: '2026-07-01T03:00:00Z', value: '非公開' }, { at: '2026-07-09T03:00:00Z', value: '公開' },
  ],
  versions: [version('v1', '2026-07-02', '時給 1,100 円', 'img0'), version('v2', '2026-07-08', '時給 1,100 円', 'img1')],
};

describe('出来事の種類', () => {
  it('公開状況の履歴: 掲載中が始まり、終了、復活。最初の掲載中は掲載開始の日', () => {
    const timeline = statusTimeline(job);
    expect(timeline.startOn).toBe('2026-06-15');
    expect(timeline.events.map(event => [event.date, event.kind, event.title])).toEqual([
      ['2026-07-01', 'end', '掲載を終了'], ['2026-07-09', 'revive', '掲載が復活（終了から公開中に戻った）'],
    ]);
  });
  it('応募は1件ずつ。日付と属性が出る。属性が無い応募はそう書く', () => {
    expect(applicationEvents(job).map(event => [event.date, event.summary])).toEqual([
      ['2026-06-16', '女性 ・ 30代 ・ 大分県大分市'], ['2026-07-07', '男性 ・ 40代'], ['2026-07-07', '応募者の属性は記録されていません'],
    ]);
    expect(applicantText({ gender: null, age: null, prefecture: null, municipality: null })).toBe('応募者の属性は記録されていません');
  });
  it('全部の出来事を古い順に並べる。画像だけの変更は「画像を変更」', () => {
    const events = buildListingEvents(job, billing);
    expect(events.map(event => [event.date, event.kind])).toEqual([
      ['2026-06-15', 'start'], ['2026-06-15', 'billing'], ['2026-06-16', 'application'], ['2026-06-22', 'billing'], ['2026-06-29', 'billing'],
      ['2026-07-01', 'end'], ['2026-07-02', 'content'], ['2026-07-06', 'billing'], ['2026-07-07', 'application'], ['2026-07-07', 'application'], ['2026-07-08', 'image'], ['2026-07-09', 'revive'],
    ]);
    expect(events.find(event => event.kind === 'image')?.title).toBe('画像を変更');
    expect(events.find(event => event.date === '2026-07-02')?.title).toBe('HubSpot に本文を記録しはじめた');
  });
});

describe('ライフラインの行', () => {
  const events = buildListingEvents(job, billing);
  const lifeline = buildLifeline(job, events, billing, new Date('2026-07-13T12:00:00+09:00'));
  it('6/15 の週から今日（7/13）の週まで、1週1列で5列', () => {
    expect(lifeline?.weeks.map(week => week.start)).toEqual(['2026-06-15', '2026-06-22', '2026-06-29', '2026-07-06', '2026-07-13']);
    expect(lifeline?.startIndex).toBe(0);
    expect(lifeline?.todayIndex).toBe(4);
    expect(weekIndex(lifeline ?? { weeks: [] }, '2026-07-07')).toBe(3);
  });
  it('課金・閲覧数・クリック数・応募数。0円・0件も0として持つ', () => {
    expect(lifeline?.weeks.map(week => [week.billing, week.impressions, week.clicks, week.mediaApplications, week.hubspotApplications])).toEqual([
      [5000, 800, 30, 1, 1], [7500, 900, 35, 0, 0], [0, 0, 0, 0, 0], [7500, 700, 20, 2, 2], [null, null, null, null, 0],
    ]);
  });
  it('終了のまま（復活なし）なら、終了の週まで。今日の線は出さない', () => {
    const ended = { ...job, statusHistory: [{ at: '2026-06-15T00:00:00Z', value: '公開' }, { at: '2026-07-01T03:00:00Z', value: '非公開' }] };
    const result = buildLifeline(ended, buildListingEvents(ended, billing), billing, new Date('2026-07-13T12:00:00+09:00'));
    expect(result?.weeks).toHaveLength(3);
    expect(result?.endIndex).toBe(2);
    expect(result?.todayIndex).toBeNull();
  });
});

describe('画面', () => {
  const events = buildListingEvents(job, billing);
  function Harness({ record }: { record: JobCopyRecord }) {
    const [selected, setSelected] = useState<string | null>(null);
    return <ListingOverview job={record} events={events} billing={billing} market={{ state: 'unset' }} selectedId={selected} onSelect={setSelected} onCompare={() => undefined} onOpenDrawer={() => undefined} />;
  }
  const view = (record: JobCopyRecord = job) => render(<Harness record={record} />);
  it('5つの行名があり、HubSpot の応募と Indeed の集計は別の行', () => {
    view();
    const lifeline = screen.getByTestId('lifeline');
    for (const label of ['課金（週）', '閲覧数（表示回数）', 'クリック数', '応募数（HubSpot に記録された応募日）', '応募数（Indeedの集計）']) expect(lifeline.textContent).toContain(label);
    expect(screen.getByRole('row', { name: '課金（週）' }).textContent).toContain('¥0');
    expect(COLUMN_PX).toBe(56);
  });
  it('応募の出来事を開くと、その人の応募理由が出る', () => {
    const reasons = { ...job, applicantReasons: { available: true, basis: 'b', fetchedAt: '2026-07-13T00:00:00Z', totalApplicants: 3, totalSourceValues: 0, sourceCounts: {}, missing: 0, blank: 0, truncated: false, selections: [], optionLabels: 'read' as const,
      items: [{ id: 'r1', applicant: 'a1', text: '家から近いので応募しました', sourceProperty: 'ouboriyuu_hiaringu', applicationDate: '2026-06-16', collectedAt: null, versionId: null }] } };
    view(reasons);
    fireEvent.click(screen.getByRole('button', { name: /応 6\/16/u }));
    expect(screen.getByText('家から近いので応募しました')).toBeTruthy();
    expect(screen.getByTestId('lifeline-selected').style.left).toBe('0px');
  });
  it('応募理由が無い求人は「応募理由の記録はまだありません」', () => {
    render(<ListingReasons job={{ ...job, applicantReasons: undefined }} />);
    expect(screen.getByRole('status').textContent).toBe('応募理由の記録はまだありません。');
  });
  it('AirWork で実績が無ければ「課金データなし」', () => {
    const aw = { ...job, media: 'AirWork', hrhPerformance: undefined };
    view(aw);
    expect(screen.getAllByText('課金データなし').length).toBeGreaterThan(0);
  });
});

function required<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error('検証対象がありません');
  return value;
}

describe('ライフラインの変更印', () => {
  const changedJob: JobCopyRecord = {
    ...job,
    versions: [
      version('initial', '2026-06-15', '時給 1,100 円', 'one'),
      // 日曜の23:59 JSTと、月曜の00:00 JST（UTCではどちらも日曜）。
      { ...version('salary', '2026-06-21', '時給 1,200 円', 'one'), publishedFrom: '2026-06-21T14:59:00Z' },
      { ...version('image', '2026-06-22', '時給 1,200 円', 'two'), publishedFrom: '2026-06-21T15:00:00Z' },
      { ...version('body', '2026-06-23', '時給 1,300 円', 'two'), body: '仕事内容\n地域へ配送します。\n給与：時給 1,300 円\n休日：土日祝' },
    ],
  };
  const changes = buildListingEvents(changedJob, billing);
  const model = required(buildLifeline(changedJob, changes, billing, new Date('2026-07-13T00:00:00Z')));
  it('JSTの日曜は前の週、月曜00:00は次の週。同じ週の本文・給与と画像を2件にまとめる', () => {
    expect(model.weeks.slice(0, 3).map(week => [week.start, week.changes.map(event => [event.date, event.versionId])])).toEqual([
      ['2026-06-15', [['2026-06-21', 'salary']]],
      ['2026-06-22', [['2026-06-22', 'image'], ['2026-06-23', 'body']]],
      ['2026-06-29', []],
    ]);
    expect(changeMarkerLabel(required(model.weeks[0]).changes)).toBe('給与変更');
    expect(changeMarkerLabel(required(model.weeks[1]).changes)).toBe('変更×2');
    expect(changeMarkerLabel([required(required(model.weeks[1]).changes[0])])).toBe('画像変更');
    expect(changeMarkerLabel([required(required(model.weeks[1]).changes[1])])).toBe('本文・給与変更');
    expect(changeMarkerLabel([{ ...required(required(model.weeks[1]).changes[1]), title: '本文を変更' }])).toBe('本文変更');
  });
  it('連携開始の本文記録、課金、応募は変更印から除く', () => {
    const events = buildListingEvents(job, billing);
    const record = events.find(event => event.title === 'HubSpot に本文を記録しはじめた');
    expect(record?.date).toBe('2026-07-02');
    const result = required(buildLifeline(job, events, billing, new Date('2026-07-13T00:00:00Z')));
    expect(result.weeks.flatMap(week => week.changes.map(event => [event.date, event.kind]))).toEqual([['2026-07-08', 'image']]);
  });
  it('単独の印はその出来事、まとめた印は最新の出来事のIDをonSelectに渡す。全件の日付と内容がある', () => {
    const onSelect = vi.fn();
    render(<LifelineChart lifeline={model} job={changedJob} selectedWeek={null} onSelect={onSelect} />);
    const single = screen.getByRole('button', { name: /2026-06-21 給与を変更/u });
    expect(single.textContent).toBe('給与変更');
    expect(single.getAttribute('type')).toBe('button');
    fireEvent.click(single);
    expect(onSelect).toHaveBeenLastCalledWith(required(changes.find(event => event.versionId === 'salary')).id);
    const group = screen.getByRole('button', { name: /2026-06-22 画像を変更/u });
    expect(group.textContent).toBe('変更×2');
    expect(group.title).toContain('2026-06-23 給与と本文を変更');
    expect(group.getAttribute('aria-label')).toContain('本文を変更（追加');
    fireEvent.click(group);
    expect(onSelect).toHaveBeenLastCalledWith(required(changes.find(event => event.versionId === 'body')).id);
  });
});
