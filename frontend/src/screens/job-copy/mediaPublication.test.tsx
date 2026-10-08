// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render, screen, within } from '@testing-library/react';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { parseListingStatus, publicationLane, withMediaPublication, withPublicationFor } from './mediaPublication';
import type { MediaPublication } from './mediaPublication';

afterEach(() => { cleanup(); });

const range = { start: '2026-09-01', end: '2026-10-08' };
const base: MediaPublication = { start: '2026-09-10', plannedEnd: '2030-01-01', status: 'public', lastInCsv: '2026-10-07', privateRecordedOn: null };

describe('parseListingStatus', () => {
  it('keeps checked values, marks non-HRハッカー listings and drops malformed rows', () => {
    const parsed = parseListingStatus({ fetched_at: '2026-10-08T00:00:00Z', listings: {
      '11': { hrhacker: true, start: '2026-08-24', planned_end: '2027-07-06', status: 'public', last_in_csv: '2026-10-08', private_recorded_on: null },
      '12': { hrhacker: false },
      '13': { hrhacker: true, start: '2026-02-30', planned_end: null, status: 'public', last_in_csv: null, private_recorded_on: null },
      '14': { hrhacker: true, start: null, planned_end: null, status: 'paused', last_in_csv: null, private_recorded_on: null },
    } });
    expect(parsed?.get('11')).toEqual({ start: '2026-08-24', plannedEnd: '2027-07-06', status: 'public', lastInCsv: '2026-10-08', privateRecordedOn: null });
    expect(parsed?.get('12')).toBeNull();
    expect(parsed?.has('13')).toBe(false);
    expect(parsed?.has('14')).toBe(false);
    expect(parseListingStatus({ listings: [] })).toBeNull();
  });

  it('marks only HubSpot HRハッカー jobs, and marks them unavailable when the read failed or the row is missing', () => {
    const hrh = { ...jobs[0], id: 'a', dataSource: 'hubspot', hubspotId: '11', media: 'HRハッカー' } as JobCopyRecord;
    const missing = { ...hrh, id: 'b', hubspotId: '99' };
    const airwork = { ...hrh, id: 'c', hubspotId: '12', media: 'AirWork' };
    const demo = { ...jobs[0], id: 'd' } as JobCopyRecord;
    const listings = new Map([['11', base], ['12', null]]);
    const [a, b, c, d] = withMediaPublication([hrh, missing, airwork, demo], listings);
    expect(a?.mediaPublication).toEqual(base);
    expect(b?.mediaPublication).toBe('unavailable');
    expect(c?.mediaPublication).toBeUndefined();
    expect(d?.mediaPublication).toBeUndefined();
    // The read worked but HubSpot has no HRハッカー job ID for it: nothing to show, not a failure.
    expect(withMediaPublication([{ ...hrh, hubspotId: '12' }], listings)[0]?.mediaPublication).toBeUndefined();
    expect(withMediaPublication([hrh], null)[0]?.mediaPublication).toBe('unavailable');
  });
});

describe('withPublicationFor', () => {
  it('keeps edits made while the status was read and touches only the snapshot jobs', () => {
    const hrh = { ...jobs[0], id: 'a', dataSource: 'hubspot', hubspotId: '11', media: 'HRハッカー' } as JobCopyRecord;
    const other = { ...hrh, id: 'x', hubspotId: '11' };
    const edited = { ...hrh, title: '編集後' };
    const result = withPublicationFor([edited, other], [hrh], new Map([['11', base]]));
    expect(result[0]).toMatchObject({ title: '編集後', mediaPublication: base });
    expect(result[1]?.mediaPublication).toBeUndefined();
  });
});

describe('publicationLane', () => {
  it('public: the bar runs from the start to the last CSV day, and the set end date is only in the tooltip', () => {
    const lane = publicationLane(base, range);
    expect(lane.bar).toEqual({ start: '2026-09-10', endExclusive: '2026-10-08', status: 'public' });
    expect(lane.label).toBe('2026/10/07時点で公開・公開開始 2026/09/10');
    expect(lane.label).not.toContain('2030');
    expect(lane.title).toContain('媒体に設定された公開終了日: 2030/01/01（予定の日付で、実際に終わった日ではありません）');
  });

  it('a start before the range is clipped to the left edge and named', () => {
    const lane = publicationLane({ ...base, start: '2023-12-25' }, range);
    expect(lane.bar?.start).toBe('2026-09-01');
    expect(lane.startsBefore).toBe('2023-12-25');
    expect(lane.label).toBe('2026/10/07時点で公開・公開開始 2023/12/25（表示期間より前）');
  });

  it('private: ends where the stop was recorded, or draws no days when the stop day is unknown', () => {
    const known = publicationLane({ ...base, status: 'private', privateRecordedOn: '2026-10-01' }, range);
    expect(known.bar).toEqual({ start: '2026-09-10', endExclusive: '2026-10-01', status: 'private' });
    expect(known.label).toBe('非公開（2026/10/01までに終了）・公開開始 2026/09/10');
    const unknown = publicationLane({ ...base, status: 'private' }, range);
    expect(unknown.bar).toBeNull();
    expect(unknown.label).toBe('非公開（終了した日は不明）・公開開始 2026/09/10');
  });

  it('missing values say 未取得 or 不明, never a made-up period', () => {
    expect(publicationLane({ ...base, start: null }, range)).toMatchObject({ bar: null, label: '2026/10/07時点で公開・公開開始日は未取得' });
    expect(publicationLane({ ...base, status: null }, range)).toMatchObject({ bar: null, label: '公開状況は未取得・公開開始 2026/09/10' });
    expect(publicationLane({ ...base, status: 'unknown' }, range).label).toBe('公開状況は不明・公開開始 2026/09/10');
    expect(publicationLane('unavailable', range)).toMatchObject({ bar: null, label: '媒体の公開状況は取得できませんでした（時間をおいて開き直してください）' });
  });
});

describe('the 媒体の公開状況 lane on the timeline', () => {
  const [job] = jobs;
  if (!job) throw new Error('demo job missing');
  it('is shown only for a job that carries the publication', () => {
    render(<JobTimeline job={{ ...job, mediaPublication: base }} marketMode="demo" now={new Date('2026-10-08T00:00:00+09:00')} />);
    const lane = screen.getByRole('group', { name: '媒体の公開状況' });
    expect(within(lane).getByText('2026/10/07時点で公開・公開開始 2026/09/10')).toBeTruthy();
    // The set end date is in the help text (reachable without hovering), named as a plan.
    expect(document.body.textContent).toContain('媒体に設定された公開終了日: 2030/01/01（予定の日付で、実際に終わった日ではありません）');
    expect(within(screen.getByRole('group', { name: '凡例' })).getByText('公開開始〜最後に公開を確認した日（途中で止めたかは不明）')).toBeTruthy();
  });
  it('is absent for a job without it (demo data, AirWork)', () => {
    render(<JobTimeline job={job} marketMode="demo" now={new Date('2026-10-08T00:00:00+09:00')} />);
    expect(screen.queryByRole('group', { name: '媒体の公開状況' })).toBeNull();
  });
});
