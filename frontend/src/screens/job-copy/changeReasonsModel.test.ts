import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { changeDateLabel, changeReasonBlocks, changeReasonSummary } from './changeReasonsModel';
import { asOfDate } from './timelineModel';

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};
const blocksOf = (job: JobCopyRecord, weeks: 2 | 4 | 8) => changeReasonBlocks(job, weeks, asOfDate(job));

const item = (n: number, applicant: string, date: string | null, text: string) => ({ id: String(n).padStart(64, '0'), applicant: applicant.repeat(64), text, sourceProperty: 'oubodouki', applicationDate: date, collectedAt: null, versionId: null });
function collection(items: ReturnType<typeof item>[]): ApplicantReasonCollection {
  return { available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-10-05T00:00:00Z', totalApplicants: 20, totalSourceValues: 20, sourceCounts: {}, missing: 0, blank: 0, truncated: false, items, selections: [], optionLabels: 'read' };
}
/**
 * One change on 9/10 (publication time, exact): HRハッカー demo versions 1 and 3 re-dated, the
 * first shown from 8/1. Applications: 8/5 ×2, 8/20, 9/1, 9/5, 9/12 ×2, 9/20, 10/1 ×3; counted on 10/5.
 */
function longJob(): JobCopyRecord {
  const base = demo('demo-job-001');
  const v1 = base.versions.find(version => version.id === 'demo-001-v1');
  const v3 = base.versions.find(version => version.id === 'demo-001-v3');
  if (!v1 || !v3) throw new Error('Missing versions');
  return {
    ...base,
    versions: [{ ...v1, observedAt: '2026-08-01T09:00:00+09:00', publishedFrom: '2026-08-01T09:00:00+09:00', publishedUntil: '2026-09-10T09:00:00+09:00' }, { ...v3, observedAt: '2026-09-10T09:00:00+09:00', publishedFrom: '2026-09-10T09:00:00+09:00' }],
    overallApplications: { total: 11, missingDate: 0, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-08-05': 2, '2026-08-20': 1, '2026-09-01': 1, '2026-09-05': 1, '2026-09-12': 2, '2026-09-20': 1, '2026-10-01': 3 } },
    applicantReasons: collection([
      item(1, 'a', '2026-08-20', '時給が高いので'), item(2, 'b', '2026-09-01', '家から近いため'), item(3, 'c', '2026-09-12', '時給と月給が良い'),
      item(4, 'd', '2026-09-12', '大手で安心'), item(5, 'e', '2026-10-01', '土日休みが良い'), item(6, 'f', null, '日付不明の記録'),
    ]),
  };
}

describe('changeReasonBlocks', () => {
  it('returns nothing when no change was found', () => {
    expect(blocksOf(demo('demo-job-008'), 4)).toEqual([]);
    const single = demo('demo-job-001');
    expect(blocksOf({ ...single, versions: single.versions.slice(0, 1) }, 4)).toEqual([]);
  });

  it('lays out one change with exact windows of 2, 4 and 8 weeks and different counts', () => {
    const job = longJob();
    const two = blocksOf(job, 2);
    expect(two).toHaveLength(1);
    const [only] = two;
    expect(only?.window).toEqual({ from: '2026-09-10', to: '2026-09-10', exact: true });
    expect(only?.before).toMatchObject({ start: '2026-08-27', end: '2026-09-10', days: 14, applications: 2, shortened: null });
    expect(only?.before.reasons?.n).toBe(1);
    expect(only?.after).toMatchObject({ start: '2026-09-10', end: '2026-09-24', days: 14, applications: 3, shortened: null });
    expect(only?.after.reasons?.n).toBe(2);

    const four = blocksOf(job, 4)[0];
    expect(four?.before).toMatchObject({ start: '2026-08-13', end: '2026-09-10', applications: 3, shortened: null });
    expect(four?.before.reasons?.n).toBe(2);
    // 10/8 would pass the day applications were counted (10/5): the window stops at 10/5 and says so.
    expect(four?.after).toMatchObject({ start: '2026-09-10', end: '2026-10-06', days: 26, applications: 6, shortened: { reason: 'today', date: '2026-10-05' } });
    expect(four?.after.reasons?.n).toBe(3);

    const eight = blocksOf(job, 8)[0];
    expect(eight?.before).toMatchObject({ start: '2026-08-01', end: '2026-09-10', days: 40, applications: 5, shortened: { reason: 'listing_start', date: '2026-08-01' } });
    expect(eight?.before.reasons?.n).toBe(2);
    expect(eight?.after.applications).toBe(6);
  });

  it('keeps the reason categories of a window and the masked source texts together', () => {
    const after = blocksOf(longJob(), 2)[0]?.after;
    expect(after?.reasons?.counts.map(row => [row.category, row.total])).toEqual([['給与', 1], ['勤務地', 0], ['職種興味', 0], ['会社規模', 1], ['その他', 0]]);
    expect(after?.texts.map(text => [text.applicationDate, text.text, text.sourceLabel])).toEqual([['2026-09-12', '時給と月給が良い', '応募動機（HubSpot）'], ['2026-09-12', '大手で安心', '応募動機（HubSpot）']]);
  });

  it('lists several changes newest first and clips each window at the adjacent change', () => {
    const blocks = blocksOf(demo('demo-job-001'), 4);
    expect(blocks.map(block => block.window.to)).toEqual(['2026-09-25', '2026-09-15']);
    const [latest, earlier] = blocks;
    expect(latest?.kinds).toEqual(['給与', '本文', '画像']);
    expect(latest?.before).toMatchObject({ start: '2026-09-15', end: '2026-09-25', days: 10, applications: 8, shortened: { reason: 'previous_change', date: '2026-09-15' } });
    expect(latest?.before.reasons?.n).toBe(5);
    expect(latest?.after).toMatchObject({ start: '2026-09-25', end: '2026-10-06', days: 11, applications: 3, shortened: { reason: 'today', date: '2026-10-05' } });
    expect(latest?.after.reasons?.n).toBe(0);
    expect(earlier?.before).toMatchObject({ start: '2026-09-01', end: '2026-09-15', applications: 7, shortened: { reason: 'listing_start', date: '2026-09-01' } });
    expect(earlier?.after).toMatchObject({ start: '2026-09-15', end: '2026-09-25', applications: 8, shortened: { reason: 'next_change', date: '2026-09-25' } });
    // Two weeks fits the first window exactly: nothing is cut.
    expect(blocksOf(demo('demo-job-001'), 2)[1]?.before.shortened).toBeNull();
    expect(latest?.summary).toContain('給与の行を変更');
  });

  it('counts only known days for a change seen between two acquisitions (AirWork)', () => {
    const job = demo('demo-job-004');
    const [block] = blocksOf(job, 4);
    expect(blocks004(block)).toEqual({ exact: false, from: '2026-09-10', to: '2026-09-28' });
    expect(changeDateLabel(block?.window ?? { from: '', to: '', exact: false })).toBe('9/10〜9/28 の間');
    // The first acquisition day is not counted; the days up to the change are unknown, so the
    // 9/15 application belongs to neither window.
    expect(block?.before).toMatchObject({ start: '2026-09-11', end: '2026-09-11', days: 0, applications: 0 });
    expect(block?.after).toMatchObject({ start: '2026-09-29', end: '2026-10-04', days: 5, applications: 2, shortened: { reason: 'today', date: '2026-10-03' } });
    expect(block?.after.reasons?.n).toBe(2);
    expect(block?.after.texts.map(text => text.applicationDate)).toEqual(['2026-09-29', '2026-10-02']);
    expect(block?.before.reasons?.n).toBe(0);
    expect(changeReasonSummary(job)).toEqual({ reasonsAvailable: true, undated: 1, multiListing: null });
  });

  it('keeps unknown apart from zero when applications or reasons were not read', () => {
    const { applicantReasons, overallApplications, ...rest } = demo('demo-job-001');
    expect([applicantReasons, overallApplications].every(Boolean)).toBe(true);
    const unread: JobCopyRecord = rest;
    const [block] = blocksOf(unread, 4);
    expect(block?.after.applications).toBeNull();
    expect(block?.after.reasons).toBeNull();
    expect(changeReasonSummary(unread).reasonsAvailable).toBe(false);
  });
});

function blocks004(block: { window: { exact: boolean; from: string; to: string } } | undefined) {
  return block ? { exact: block.window.exact, from: block.window.from, to: block.window.to } : null;
}
