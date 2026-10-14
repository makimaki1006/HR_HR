import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { indeedWeeklyPerformance } from './hubspotListings';
import { parseHrhPerformance } from './hrhPerformanceModel';
import { billingEntries, dummyBilling, realBilling } from './timelineModel';

const collection = (jobId: string) => ({
  schema_version: 1, source: 'hrhacker', job_id: jobId, captured_at: '2026-10-14T00:00:00Z',
  rows: [
    { period_start: '2026-09-28', period_end: '2026-10-04', impressions: 100, clicks: 5, cost_yen: 12000, applications: 2 },
    { period_start: '2026-10-05', period_end: '2026-10-11', impressions: 80, clicks: 4, cost_yen: 9500, applications: 0 },
  ],
});
const base = jobs.find(job => job.id === 'demo-job-001');
if (!base) throw new Error('demo missing');
const real = (patch: Partial<JobCopyRecord>): JobCopyRecord => ({ ...base, id: 'hubspot-history-1', dataSource: 'hubspot', hrhPerformance: undefined, ...patch });

describe('AirWork 週ごとの実績', () => {
  const aw = (jobId: string) => ({ ...collection(jobId), source: 'airwork' });
  it('数字だけの求人番号が一致するときだけ使い、HRハッカーの形や別の番号は無視する', () => {
    expect(indeedWeeklyPerformance(aw('98765'), 'airwork', '98765')?.rows[0]?.cost_yen).toBe(12000);
    expect(indeedWeeklyPerformance(aw('98765'), 'airwork', '11111')).toBeUndefined();
    expect(indeedWeeklyPerformance(aw('AW-1'), 'airwork', 'AW-1')).toBeUndefined();
    expect(indeedWeeklyPerformance(collection('1234567'), 'airwork', '1234567')).toBeUndefined();
    expect(indeedWeeklyPerformance(aw('98765'), 'hrh', '98765')).toBeUndefined();
    expect(indeedWeeklyPerformance('{broken', 'airwork', '98765')).toBeUndefined();
  });
  it('課金の行は AirWork の実績として出て、実績のない AirWork に仮の金額は出ない', () => {
    const perf = parseHrhPerformance(aw('98765'), '98765', 'airwork');
    const entries = billingEntries(real({ mediaJobId: '98765', media: 'AirWork', hrhPerformance: perf }), undefined, { asOf: '2026-10-14' });
    expect(realBilling(entries).map(entry => [entry.media, entry.amountYen])).toEqual([['AirWork', 12000], ['AirWork', 9500]]);
    expect(dummyBilling(entries)).toHaveLength(0);
    expect(billingEntries(real({ media: 'AirWork' }), undefined, { asOf: '2026-10-14' })).toHaveLength(0);
  });
});

describe('Indeed 週ごとの実績', () => {
  it('accepts 7 and 8 digit job numbers that match the record', () => {
    expect(parseHrhPerformance(collection('1234567'), '1234567').rows).toHaveLength(2);
    expect(parseHrhPerformance(collection('12345678'), '12345678').rows[1]?.cost_yen).toBe(9500);
    expect(() => parseHrhPerformance(collection('123456'), '123456')).toThrow();
    expect(() => parseHrhPerformance(collection('1234567'), '7654321')).toThrow();
  });
  it('is used for HRH only, and a broken value is treated as absent', () => {
    expect(indeedWeeklyPerformance(collection('1234567'), 'hrh', '1234567')?.rows[0]?.cost_yen).toBe(12000);
    expect(indeedWeeklyPerformance(collection('1234567'), 'airwork', '1234567')).toBeUndefined();
    expect(indeedWeeklyPerformance(collection('1234567'), 'hrh', '9999999')).toBeUndefined();
    expect(indeedWeeklyPerformance({ nope: 1 }, 'hrh', '1234567')).toBeUndefined();
    expect(indeedWeeklyPerformance(null, 'hrh', '1234567')).toBeUndefined();
  });
  it('feeds real billing and shows no made-up amount when real data exists', () => {
    const perf = parseHrhPerformance(collection('1234567'), '1234567');
    const entries = billingEntries(real({ mediaJobId: '1234567', media: 'HRハッカー', hrhPerformance: perf }), undefined, { asOf: '2026-10-14' });
    expect(realBilling(entries).map(entry => [entry.start, entry.amountYen])).toEqual([['2026-09-28', 12000], ['2026-10-05', 9500]]);
    expect(dummyBilling(entries)).toHaveLength(0);
  });
  it('keeps the made-up amount for an HRH job with nothing real, and none for AirWork', () => {
    const hrh = billingEntries(real({ media: 'HRハッカー' }), undefined, { asOf: '2026-10-14' });
    expect(dummyBilling(hrh).length).toBeGreaterThan(0);
    expect(billingEntries(real({ media: 'AirWork' }), undefined, { asOf: '2026-10-14' })).toHaveLength(0);
  });
});
