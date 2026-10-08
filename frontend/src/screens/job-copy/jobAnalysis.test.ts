import { describe, expect, it } from 'vitest';
import { parseHrhPerformance, performanceRatios, comparePerformance } from './hrhPerformanceModel';
import { parseJointDemographics, reverseSearch } from './reverseSearchModel';
import type { JobCopyRecord } from './data';

const row = { period_start: '2026-09-01', period_end: '2026-09-10', impressions: 1000, clicks: 50, cost_yen: 10000, applications: 5 };
const payload = () => ({ schema_version: 1, source: 'hrhacker', job_id: '01234567', captured_at: '2026-10-05T01:00:00Z', rows: [{ ...row }] });
describe('HR Hacker period metrics', () => {
  it('preserves eight digit ID and computes rates with actual denominators', () => {
    const parsed = parseHrhPerformance(payload(), '01234567');
    expect(parsed.job_id).toBe('01234567');
    const first = parsed.rows[0];
    if (!first) throw new Error('Missing synthetic metric row');
    expect(performanceRatios(first)).toEqual({ ctr: 5, cpc: 200, cpa: 2000 });
    expect(comparePerformance(row, { ...row, period_start: '2026-09-11', period_end: '2026-09-15', clicks: 50, impressions: 500 })).toEqual({ ctrDeltaPp: 5, clicksPerDayDelta: 5 });
  });
  it('does not invent missing denominators or treat missing cost as free', () => {
    expect(performanceRatios({ ...row, impressions: null, clicks: 0, cost_yen: null, applications: 0 })).toEqual({ ctr: null, cpc: null, cpa: null });
    expect(performanceRatios({ ...row, clicks: 0, cost_yen: 0 })).toEqual({ ctr: 0, cpc: null, cpa: 0 });
  });
  it('rejects numeric/wrong job IDs, ambiguous overlapping periods and impossible counts', () => {
    expect(() => parseHrhPerformance(payload(), '1234567')).toThrow();
    expect(() => parseHrhPerformance({ ...payload(), job_id: 1234567 }, '01234567')).toThrow();
    for (const bad of [{ ...row, clicks: 1001 }, { ...row, cost_yen: -1 }, { ...row, period_start: '2026-02-30' }, { ...row, applications: 1.5 }, { ...row, secret: 'extra' }]) expect(() => parseHrhPerformance({ ...payload(), rows: [bad] }, '01234567')).toThrow();
    expect(() => parseHrhPerformance({ ...payload(), rows: [row, { ...row, period_start: '2026-09-10' }] }, '01234567')).toThrow();
  });
});
describe('joint demographic reverse search', () => {
  const joint = { total: 8, cells: [
    { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分市', count: 3 },
    { gender: '女性', age: '20代', prefecture: '大分県', municipality: '大分市', count: 4 },
    { gender: '男性', age: '30代', prefecture: '福岡県', municipality: '福岡市', count: 1 },
  ] };
  const job = { id: 'synthetic', jointDemographics: joint } as JobCopyRecord;
  it('answers intersecting conditions from actual cells rather than marginal totals', () => {
    // 地域は都道府県 + 市区町村に丸め、3件未満の地域（福岡県の1件）は「その他」にまとめる
    expect(parseJointDemographics(joint, 8)).toEqual({ total: 8, cells: [
      { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分県大分市', count: 3 },
      { gender: '女性', age: '20代', prefecture: '大分県', municipality: '大分県大分市', count: 4 },
      { gender: '男性', age: '30代', prefecture: 'その他', municipality: 'その他', count: 1 },
    ] });
    const query = { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分県大分市', minimum: 1 };
    expect(reverseSearch([job, { ...job, id: 'missing', jointDemographics: undefined }], query).map(result => [result.count, result.denominator, result.percentage])).toEqual([[3, 8, 37.5]]);
    expect(reverseSearch([job], { ...query, minimum: 4 })).toEqual([]);
    expect(reverseSearch([job], { ...query, municipality: '大分県別府市' })).toEqual([]);
  });
  it('rejects total drift, duplicate cells and leaked additional fields', () => {
    expect(() => parseJointDemographics(joint, 9)).toThrow();
    expect(() => parseJointDemographics({ ...joint, cells: [...joint.cells, joint.cells[0]] }, 8)).toThrow();
    expect(() => parseJointDemographics({ total: 1, cells: [{ ...joint.cells[0], email: 'synthetic@example.test' }] }, 1)).toThrow();
  });
});
