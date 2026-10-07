import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import type { MarketRow } from './marketChartModel';
import {
  applicationBuckets, applicationsOutsidePeriods, buildPeriods, changeKinds, formatPerDay, jstDate, marketChange, marketLane,
  periodRows, timelineRange, versionChanges,
} from './timelineModel';

const demo = jobs.find(job => job.id === 'demo-job-001');
if (!demo) throw new Error('Missing demo-job-001');
const driver: JobCopyRecord = { ...demo, hrhPerformance: undefined, overallApplications: { total: 9, missingDate: 4, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-10': 2, '2026-09-16': 3 } } };
const row = (month: string, jobs: number | null): MarketRow => ({ month, jobs, viewers: jobs === null ? null : jobs * 3, employers: null, viewersPerJob: null });

describe('periods on the JST day axis', () => {
  it('splits demo-001 into 14 days, 10 days and a running period', () => {
    const periods = buildPeriods(driver, '2026-10-05');
    expect(periods.map(period => [period.versionId, period.start, period.end, period.days, period.ongoing])).toEqual([
      ['demo-001-v1', '2026-09-01', '2026-09-15', 14, false],
      ['demo-001-v2', '2026-09-15', '2026-09-25', 10, false],
      ['demo-001-v3', '2026-09-25', null, 11, true],
    ]);
    expect(periods.every(period => period.basis === 'published' && period.certainty === 'confirmed')).toBe(true);
  });
  it('counts applications per period and per day (2 → 0.14件/日, 3 → 0.30件/日)', () => {
    const rows = periodRows(driver, { asOf: '2026-10-05' });
    expect(rows.map(item => [item.applications, item.days])).toEqual([[2, 14], [3, 10], [0, 11]]);
    expect(formatPerDay(rows[0]?.perDay ?? null)).toBe('0.14件/日');
    expect(formatPerDay(rows[1]?.perDay ?? null)).toBe('0.30件/日');
    expect(rows[0]?.perDay).toBeCloseTo(2 / 14, 10);
  });
  it('keeps applications without a date out of the bars', () => {
    const buckets = applicationBuckets(driver.overallApplications?.byDate, 'day');
    expect(buckets.reduce((sum, bucket) => sum + bucket.count, 0)).toBe(5);
    expect(driver.overallApplications?.missingDate).toBe(4);
    expect(applicationBuckets(driver.overallApplications?.byDate, 'week')).toEqual([
      { start: '2026-09-07', end: '2026-09-14', count: 2 }, { start: '2026-09-14', end: '2026-09-21', count: 3 },
    ]);
    expect(applicationBuckets({ '2026-09-30': 1, '2026-10-01': 2 }, 'month')).toEqual([
      { start: '2026-09-01', end: '2026-10-01', count: 1 }, { start: '2026-10-01', end: '2026-11-01', count: 2 },
    ]);
  });
  it('splits periods by the Japan date even when the switch time is given in UTC or at 10:00 JST', () => {
    expect(jstDate('2026-09-15T10:00:00+09:00')).toBe('2026-09-15');
    expect(jstDate('2026-09-14T16:00:00Z')).toBe('2026-09-15');
    expect(jstDate('2026-09-14T23:59:00+09:00')).toBe('2026-09-14');
    const version = (id: string, from: string): CopyVersion => ({ id, label: id, observedAt: from, publishedFrom: from, certainty: 'confirmed', kind: 'published', source: 'test', body: id, applications: null, note: '' });
    const job: JobCopyRecord = { id: 'utc', title: 't', company: 'c', media: 'm', mediaJobId: 'x', location: '大分県', versions: [version('a', '2026-09-01T10:00:00+09:00'), version('b', '2026-09-14T16:00:00Z')],
      overallApplications: { total: 2, missingDate: 0, fetchedAt: '2026-09-20T00:00:00Z', distributions: {}, byDate: { '2026-09-14': 1, '2026-09-15': 1 } } };
    const rows = periodRows(job, { asOf: '2026-09-20' });
    expect(rows.map(item => [item.start, item.days, item.applications])).toEqual([['2026-09-01', 14, 1], ['2026-09-15', 6, 1]]);
  });
  it('uses capture days for real data without publication times and marks them as such', () => {
    const job: JobCopyRecord = { ...driver, versions: driver.versions.map((version): CopyVersion => { const copy = { ...version }; delete copy.publishedFrom; delete copy.publishedUntil; return copy; }) };
    const periods = buildPeriods(job, '2026-10-05');
    expect(periods.map(period => [period.start, period.basis])).toEqual([['2026-09-01', 'captured'], ['2026-09-15', 'captured'], ['2026-09-25', 'captured']]);
  });
});

describe('billing in the period table', () => {
  it('shows not connected (not zero) without billing data', () => {
    expect(periodRows(driver, { asOf: '2026-10-05' })[0]?.billing).toEqual({ connected: false });
  });
  it('spreads a billing period over version periods by days and flags it', () => {
    const rows = periodRows(driver, { asOf: '2026-10-05', billing: [
      { source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 30000 },
      { source: 'csv', start: '2026-09-15', end: '2026-09-30', amountYen: 45000 },
      { source: 'csv', start: '2026-10-01', end: '2026-10-05', amountYen: null },
    ] });
    expect(rows[0]?.billing).toEqual({ connected: true, yen: 30000, prorated: false, missingAmount: false, entries: 1 });
    expect(rows[1]?.billing).toEqual({ connected: true, yen: 28125, prorated: true, missingAmount: false, entries: 1 });
    expect(rows[2]?.billing).toEqual({ connected: true, yen: 16875, prorated: true, missingAmount: true, entries: 2 });
  });
  it('reads HRハッカー cost_yen from the snapshot', () => {
    const rows = periodRows(demo, { asOf: '2026-10-05' });
    expect(rows[0]?.billing).toMatchObject({ connected: true, yen: 30000 });
  });
});

describe('market lane', () => {
  const rows = [row('2026-07', 100), row('2026-08', 110)];
  it('draws no value after the data ends and reports the no-data months', () => {
    const lane = marketLane(rows, { start: '2026-07-15', end: '2026-10-05' });
    expect(lane.points).toEqual([
      { month: '2026-07', jobs: 100, viewers: 300 }, { month: '2026-08', jobs: 110, viewers: 330 },
      { month: '2026-09', jobs: null, viewers: null }, { month: '2026-10', jobs: null, viewers: null },
    ]);
    expect(lane.lastDataMonth).toBe('2026-08');
    expect(lane.noDataFrom).toBe('2026-09');
    expect(marketLane(rows, { start: '2026-07-01', end: '2026-08-20' }).noDataFrom).toBeNull();
  });
  it('compares the market job count between the first and last month of a period', () => {
    expect(marketChange(rows, '2026-07-20', '2026-08-10')).toEqual({ ok: true, value: { fromMonth: '2026-07', toMonth: '2026-08', fromJobs: 100, toJobs: 110, changePct: 10 } });
    expect(marketChange(rows, '2026-08-20', '2026-09-10')).toEqual({ ok: false, reason: 'no_data' });
    expect(marketChange(rows, '2026-09-01', '2026-09-10')).toMatchObject({ ok: false, reason: 'no_data' });
    expect(marketChange(rows, '2026-08-01', '2026-08-10')).toEqual({ ok: false, reason: 'same_month', month: '2026-08', jobs: 110 });
    expect(marketChange(null, '2026-08-01', '2026-08-10')).toEqual({ ok: false, reason: 'not_selected' });
  });
});

describe('version changes', () => {
  it('finds salary, body and image changes on demo-001', () => {
    const changes = versionChanges(demo);
    expect(changes.map(change => [change.versionId, change.salary?.min, change.salaryChanged, change.imageChange])).toEqual([
      ['demo-001-v1', 250000, null, 'initial'], ['demo-001-v2', 270000, true, 'changed'], ['demo-001-v3', 250000, true, 'changed'],
    ]);
    expect(changes[1]?.bodyAdded).toBeGreaterThan(0);
    expect(changes[1]?.bodyRemoved).toBe(changes[1]?.bodyAdded);
    const second = changes[1];
    if (!second) throw new Error('Missing second version');
    expect(changeKinds(second)).toEqual(['給与', '本文', '画像']);
    expect(changes.every(change => change.versionId !== 'demo-001-draft')).toBe(true);
  });
  it('counts dated applications outside every period separately and covers them in the range', () => {
    const job: JobCopyRecord = { ...driver, overallApplications: { total: 3, missingDate: 0, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-08-20': 1, '2026-09-10': 2 } } };
    expect(applicationsOutsidePeriods(job, periodRows(job, { asOf: '2026-10-05' }))).toBe(1);
    expect(timelineRange(job, '2026-10-05', [])).toEqual({ start: '2026-08-20', end: '2026-10-05' });
  });
});
