import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import type { MarketRow } from './marketChartModel';
import type { BillingPeriod } from './billingTypes';
import {
  addDays, applicationBuckets, applicationsOutsidePeriods, billingEntries, billingEntriesByJob, billingOverlaps, buildPeriods, changeKinds, countApplications, daysBetween,
  formatMonth, formatPerDay, formatYen, jstDate, lastMarketMonth, marketChange, marketDataUntil, marketLane, nextMonth, periodRows, positionOf, publishedVersions, timelineRange, versionChanges,
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
    expect(rows[0]?.billing).toEqual({ connected: true, yen: 30000, prorated: false, missingAmount: false, entries: 1, overlapping: false });
    expect(rows[1]?.billing).toEqual({ connected: true, yen: 28125, prorated: true, missingAmount: false, entries: 1, overlapping: false });
    expect(rows[2]?.billing).toEqual({ connected: true, yen: 16875, prorated: true, missingAmount: true, entries: 2, overlapping: false });
  });
  it('does not add up billing periods that overlap each other', () => {
    const rows = periodRows(driver, { asOf: '2026-10-05', billing: [
      { source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 30000, sourceRow: 2 },
      { source: 'csv', start: '2026-09-10', end: '2026-09-20', amountYen: 20000, sourceRow: 3 },
    ] });
    expect(rows[0]?.billing).toMatchObject({ connected: true, yen: null, overlapping: true, entries: 2 });
    expect(billingOverlaps([{ source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 1 }, { source: 'csv', start: '2026-09-15', end: '2026-09-30', amountYen: 1 }])).toBe(false);
    expect(billingOverlaps([{ source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 1 }, { source: 'csv', start: '2026-09-14', end: '2026-09-30', amountYen: 1 }])).toBe(true);
  });
  it('turns billing CSV periods into timeline rows per job and replaces the HRハッカー row for the same days', () => {
    const periods: BillingPeriod[] = [
      { jobId: 'demo-job-001', media: 'HRハッカー', mediaJobId: 'DEMO-HRH-001', periodStart: '2026-09-01', periodEnd: '2026-09-14', amountYen: 33000, taxBasis: '税込', planName: 'スタンダード', impressions: null, clicks: null, mediaApplications: null, source: 'csv', sourceRow: 2, overlapsSourceRows: [] },
      { jobId: 'demo-job-002', media: 'Airワーク', mediaJobId: 'DEMO-AIR-002', periodStart: '2026-09-05', periodEnd: '2026-09-30', amountYen: null, taxBasis: '不明', planName: null, impressions: null, clicks: null, mediaApplications: null, source: 'csv', sourceRow: 3, overlapsSourceRows: [] },
      { jobId: 'demo-job-001', media: 'HRハッカー', mediaJobId: 'DEMO-HRH-001', periodStart: '2026-09-15', periodEnd: '2026-09-30', amountYen: 1, taxBasis: '不明', planName: null, impressions: null, clicks: null, mediaApplications: null, source: 'hrh_performance', sourceRow: null, overlapsSourceRows: [] },
    ];
    const byJob = billingEntriesByJob(periods);
    expect(Object.keys(byJob).sort()).toEqual(['demo-job-001', 'demo-job-002']);
    expect(byJob['demo-job-001']).toEqual([{ source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 33000, taxIncluded: true, media: 'HRハッカー', mediaJobId: 'DEMO-HRH-001', plan: 'スタンダード', impressions: null, clicks: null, mediaApplications: null, sourceRow: 2 }]);
    expect(byJob['demo-job-002']?.[0]).toMatchObject({ amountYen: null, taxIncluded: null });
    // demo-001 has HRハッカー実績 30000 / 45000 / 12000. The CSV row for 09-01〜09-14 replaces the first one.
    const merged = billingEntries(demo, byJob['demo-job-001']);
    expect(merged.map(entry => [entry.source, entry.start, entry.amountYen])).toEqual([
      ['csv', '2026-09-01', 33000], ['hrhacker', '2026-09-15', 45000], ['hrhacker', '2026-10-01', 12000],
    ]);
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
    expect(marketChange(rows, '2026-07-20', '2026-08-10')).toEqual({ ok: true, value: { fromMonth: '2026-07', toMonth: '2026-08', fromJobs: 100, toJobs: 110, changePct: 10, noDataFrom: null } });
    expect(marketChange(rows, '2026-07-20', '2026-08-10')).toMatchObject({ value: { noDataFrom: null } });
    // Runs into September (no data): the comparison stops at August and says September has none.
    expect(marketChange(rows, '2026-08-20', '2026-09-10')).toEqual({ ok: false, reason: 'same_month', month: '2026-08', jobs: 110, noDataFrom: '2026-09' });
    // The whole period is after the last month with data: say up to which month there is data.
    expect(marketChange(rows, '2026-09-01', '2026-09-10')).toEqual({ ok: false, reason: 'after_data', noDataFrom: '2026-09', lastDataMonth: '2026-08' });
    expect(marketChange(rows, '2026-09-05', '2026-10-08')).toEqual({ ok: false, reason: 'after_data', noDataFrom: '2026-09', lastDataMonth: '2026-08' });
    expect(marketChange(rows, '2026-08-01', '2026-08-10')).toEqual({ ok: false, reason: 'same_month', month: '2026-08', jobs: 110, noDataFrom: null });
    expect(marketChange(null, '2026-08-01', '2026-08-10')).toEqual({ ok: false, reason: 'not_selected' });
  });
});

describe('the last market month comes from the data, not from the code', () => {
  it('moves forward when the monthly refresh adds 2026-09 and 2026-10', () => {
    const later = [row('2026-07', 100), row('2026-08', 110), row('2026-09', 121), row('2026-10', 133)];
    expect(lastMarketMonth(later)).toBe('2026-10');
    expect(marketDataUntil(lastMarketMonth(later) ?? '')).toBe('市場データは2026年10月まで（毎月更新）');
    const lane = marketLane(later, { start: '2026-07-15', end: '2026-10-05' });
    expect(lane.lastDataMonth).toBe('2026-10');
    expect(lane.noDataFrom).toBeNull();
    expect(lane.points.map(point => point.jobs)).toEqual([100, 110, 121, 133]);
    // A period in September is compared with real September data (not shown as no-data).
    expect(marketChange(later, '2026-08-20', '2026-09-30')).toEqual({ ok: true, value: { fromMonth: '2026-08', toMonth: '2026-09', fromJobs: 110, toJobs: 121, changePct: 10, noDataFrom: null } });
    expect(marketChange(later, '2026-11-01', '2026-11-10')).toEqual({ ok: false, reason: 'after_data', noDataFrom: '2026-11', lastDataMonth: '2026-10' });
  });
  it('writes the month without a leading zero and ignores months with no value', () => {
    expect(marketDataUntil('2026-08')).toBe('市場データは2026年8月まで（毎月更新）');
    expect(lastMarketMonth([row('2026-07', 100), { month: '2026-08', jobs: null, viewers: null, employers: null, viewersPerJob: null }])).toBe('2026-07');
    expect(lastMarketMonth([])).toBeNull();
  });
});

describe('market data that ends before the period does', () => {
  const rows = [row('2026-07', 220), row('2026-08', 230)];
  it('keeps the months with data when a period runs past 2026-08 (220 → 230, +4.5%, no data from 2026-09)', () => {
    const result = marketChange(rows, '2026-07-01', '2026-10-04');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.value).toMatchObject({ fromMonth: '2026-07', toMonth: '2026-08', fromJobs: 220, toJobs: 230, noDataFrom: '2026-09' });
    expect(result.value.changePct).toBeCloseTo(4.545, 2);
  });
  it('has no "data ends at" month when the choice has no market rows at all', () => {
    const lane = marketLane([], { start: '2026-09-01', end: '2026-10-05' });
    expect(lane).toEqual({ points: [{ month: '2026-09', jobs: null, viewers: null }, { month: '2026-10', jobs: null, viewers: null }], lastDataMonth: null, noDataFrom: null });
  });
});

describe('applications that were never fetched', () => {
  it('leaves the period counts empty instead of 0 when application dates were never fetched', () => {
    const job: JobCopyRecord = { ...driver };
    delete job.overallApplications;
    const rows = periodRows(job, { asOf: '2026-10-05' });
    expect(rows.map(item => [item.applications, item.perDay])).toEqual([[null, null], [null, null], [null, null]]);
    expect(applicationsOutsidePeriods(job, rows)).toBe(0);
  });
});

describe('billing CSV rows that only partly cover an HRハッカー period', () => {
  it('keeps the HRハッカー period (30000円) and marks the overlap instead of dropping it', () => {
    const merged = billingEntries(demo, [{ source: 'csv', start: '2026-09-01', end: '2026-09-02', amountYen: 1000, media: 'HRハッカー' }]);
    expect(merged.map(entry => [entry.source, entry.start, entry.end, entry.amountYen])).toEqual([
      ['csv', '2026-09-01', '2026-09-02', 1000], ['hrhacker', '2026-09-01', '2026-09-14', 30000], ['hrhacker', '2026-09-15', '2026-09-30', 45000], ['hrhacker', '2026-10-01', '2026-10-05', 12000],
    ]);
    const rows = periodRows(demo, { asOf: '2026-10-05', billing: [{ source: 'csv', start: '2026-09-01', end: '2026-09-02', amountYen: 1000, media: 'HRハッカー' }] });
    expect(rows[0]?.billing).toMatchObject({ connected: true, yen: null, overlapping: true, entries: 2 });
    // A CSV row for exactly the same days still replaces the HRハッカー row.
    expect(billingEntries(demo, [{ source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 33000, media: 'HRハッカー' }]).map(entry => entry.amountYen)).toEqual([33000, 45000, 12000]);
  });
});

describe('day helpers', () => {
  it('rolls over months and years', () => {
    expect(nextMonth('2026-12')).toBe('2027-01');
    expect(nextMonth('2026-09')).toBe('2026-10');
    expect(addDays('2026-12-31', 1)).toBe('2027-01-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
    expect(daysBetween('2026-12-25', '2027-01-08')).toBe(14);
    expect(formatMonth('2026-08')).toBe('2026/08');
    expect(formatYen(45000)).toBe('4万5,000円');
    expect(formatYen(30000)).toBe('3万円');
  });
  it('counts the start day and leaves out the exclusive end day', () => {
    const byDate = { '2026-09-14': 1, '2026-09-15': 2, '2026-09-24': 4, '2026-09-25': 8 };
    expect(countApplications(byDate, '2026-09-15', '2026-09-25')).toBe(6);
    expect(countApplications(byDate, '2026-09-14', '2026-09-15')).toBe(1);
    expect(countApplications(undefined, '2026-09-01', '2026-10-01')).toBe(0);
  });
  it('places days on the inclusive range and clamps outside it', () => {
    const range = { start: '2026-09-01', end: '2026-09-10' };
    expect(positionOf('2026-09-01', range)).toBe(0);
    expect(positionOf('2026-09-06', range)).toBe(50);
    expect(positionOf('2026-09-11', range)).toBe(100);
    expect(positionOf('2026-08-01', range)).toBe(0);
    expect(positionOf('2026-12-01', range)).toBe(100);
  });
  it('lists only published versions with a readable date, oldest first', () => {
    const version = (id: string, from: string, kind: CopyVersion['kind'] = 'published'): CopyVersion => ({ id, label: id, observedAt: from, certainty: 'confirmed', kind, source: 'test', body: id, applications: null, note: '' });
    const job: JobCopyRecord = { ...driver, versions: [version('late', '2026-09-20T00:00:00+09:00'), version('draft', '2026-09-10T00:00:00+09:00', 'ai_draft'), version('bad', 'いつか'), version('early', '2026-09-01T00:00:00+09:00')] };
    expect(publishedVersions(job).map(item => item.id)).toEqual(['early', 'late']);
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

describe('period ends, gaps and short periods (review round 2)', () => {
  const version = (id: string, from: string, until?: string): CopyVersion => ({ id, label: id, observedAt: from, publishedFrom: from, ...(until ? { publishedUntil: until } : {}), certainty: 'confirmed', kind: 'published', source: 'test', body: id, applications: null, note: '' });
  const job = (versions: CopyVersion[], byDate: Record<string, number>, fetchedAt = '2026-09-30T09:00:00+09:00'): JobCopyRecord => ({ id: 'gap', title: 't', company: 'c', media: 'm', mediaJobId: 'x', location: '大分県', versions,
    overallApplications: { total: Object.values(byDate).reduce((sum, count) => sum + count, 0), missingDate: 0, fetchedAt, distributions: {}, byDate } });

  it('adds a row for the days with no confirmed publication between two versions (09-10 → 09-15: 5 days, 1 application, 0.20件/日)', () => {
    const record = job([version('v1', '2026-09-01T10:00:00+09:00', '2026-09-10T10:00:00+09:00'), version('v2', '2026-09-15T10:00:00+09:00')], { '2026-09-05': 2, '2026-09-12': 1, '2026-09-20': 3 });
    const rows = periodRows(record, { asOf: '2026-09-30' });
    expect(rows.map(item => [item.kind, item.label, item.start, item.end, item.days, item.applications])).toEqual([
      ['period', 'v1', '2026-09-01', '2026-09-10', 9, 2],
      ['gap', '掲載が確認できない期間', '2026-09-10', '2026-09-15', 5, 1],
      ['period', 'v2', '2026-09-15', null, 16, 3],
    ]);
    expect(rows[1]?.perDay).toBeCloseTo(0.2, 10);
    expect(formatPerDay(rows[1]?.perDay ?? null)).toBe('0.20件/日');
    expect(applicationsOutsidePeriods(record, rows)).toBe(0);
  });

  it('ends a period at publishedUntil when it is before the next start, and at the next start when it is after', () => {
    const early = buildPeriods(job([version('v1', '2026-09-01T10:00:00+09:00', '2026-09-10T10:00:00+09:00'), version('v2', '2026-09-15T10:00:00+09:00')], {}), '2026-09-30');
    expect(early.map(period => [period.start, period.end, period.days])).toEqual([['2026-09-01', '2026-09-10', 9], ['2026-09-15', null, 16]]);
    // publishedUntil 09-20 runs past the next start (09-15): clamped, so the periods do not overlap.
    const late = job([version('v1', '2026-09-01T10:00:00+09:00', '2026-09-20T10:00:00+09:00'), version('v2', '2026-09-15T10:00:00+09:00')], { '2026-09-16': 4 });
    const periods = buildPeriods(late, '2026-09-30');
    expect(periods.map(period => [period.start, period.end, period.days])).toEqual([['2026-09-01', '2026-09-15', 14], ['2026-09-15', null, 16]]);
    const rows = periodRows(late, { asOf: '2026-09-30' });
    // The 09-16 applications are counted once, in v2 only.
    expect(rows.map(item => [item.kind, item.applications])).toEqual([['period', 0], ['period', 4]]);
    expect(applicationsOutsidePeriods(late, rows)).toBe(0);
  });

  it('gives no per-day value for a 0-day period (two versions starting on the same JST day)', () => {
    const record = job([version('v1', '2026-09-10T09:00:00+09:00'), version('v2', '2026-09-10T18:00:00+09:00')], { '2026-09-10': 2 });
    const rows = periodRows(record, { asOf: '2026-09-12' });
    expect(rows.map(item => [item.start, item.days, item.applications, item.perDay])).toEqual([['2026-09-10', 0, 0, null], ['2026-09-10', 3, 2, 2 / 3]]);
    expect(formatPerDay(rows[0]?.perDay ?? null)).toBe('—');
  });

  it('leaves applications unknown (not 0件) for a period that starts after the counts were taken', () => {
    const base = jobs.find(item => item.id === 'demo-job-001');
    if (!base?.overallApplications) throw new Error('Missing demo-job-001');
    // Counts taken 2026-09-20, five days before v3 starts on 09-25.
    const record: JobCopyRecord = { ...base, overallApplications: { ...base.overallApplications, fetchedAt: '2026-09-20T09:00:00+09:00' } };
    const rows = periodRows(record, { asOf: '2026-09-20' });
    const last = rows.at(-1);
    expect([last?.start, last?.days, last?.applications, last?.perDay, last?.afterCounts]).toEqual(['2026-09-25', 0, null, null, true]);
    expect(rows.slice(0, -1).every(item => !item.afterCounts && item.applications !== null)).toBe(true);
  });
});

describe('demo data adds up', () => {
  it('has dated + undated applications equal to the total for every demo job (the rule realMoc enforces)', () => {
    for (const job of jobs) {
      const applications = job.overallApplications;
      if (!applications?.byDate) continue;
      const dated = Object.values(applications.byDate).reduce((sum, count) => sum + count, 0);
      expect({ id: job.id, sum: dated + applications.missingDate }).toEqual({ id: job.id, sum: applications.total });
    }
  });
});
