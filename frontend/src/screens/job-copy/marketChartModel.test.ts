import { describe, expect, it } from 'vitest';
import { marketRows, monthlyApplications, trendOption } from './marketChartModel';
import type { JobCopyRecord } from './data';

describe('market chart evidence', () => {
  it('sorts months, preserves real zero, and leaves missing calendar periods and invalid values null', () => {
    expect(marketRows({ prefecture: '合成県', months: ['2026-03', '2026-01'], job_count: [0, 100], ctk_count: [300, null], employer_count: [-1, 20], seekers_per_posting: [null, 3] })).toEqual([
      { month: '2026-01', jobs: 100, viewers: null, employers: 20, viewersPerJob: 3 },
      { month: '2026-02', jobs: null, viewers: null, employers: null, viewersPerJob: null },
      { month: '2026-03', jobs: 0, viewers: 300, employers: null, viewersPerJob: null },
    ]);
  });
  it('uses actual application dates without assigning version-unknown applicants to observation days', () => {
    const job: JobCopyRecord = { id: 'synthetic', title: '合成求人', company: '合成取引先', media: '合成媒体', mediaJobId: 'synthetic', location: '合成県', versions: [], overallApplications: { byDate: { '2026-09-03': 2, '2026-08-30': 3, '2026-09-20': 4 }, missingDate: 1, total: 10, fetchedAt: '2026-10-05T00:00:00Z', distributions: {} }, attributionUnknown: 10 };
    expect(monthlyApplications(job)).toEqual([{ month: '2026-08', count: 3 }, { month: '2026-09', count: 6 }]);
    expect(monthlyApplications({} as JobCopyRecord)).toBeNull();
    if (!job.overallApplications) throw new Error('Missing synthetic totals');
    job.overallApplications.byDate = { '2026-01-01': 3, '2026-03-01': 6 };
    expect(monthlyApplications(job)).toEqual([{ month: '2026-01', count: 3 }, { month: '2026-02', count: null }, { month: '2026-03', count: 6 }]);
  });
  it('retains units, zero baseline and null gaps; sparse periods are bars, full history lines', () => {
    const chart = trendOption(['2026-01', '2026-02', '2026-03'], [100, null, 0], '求人数', '件') as { yAxis: { min: number; name: string }; series: { type: string; data: (number | null)[]; connectNulls: boolean }[] };
    expect(chart.yAxis).toMatchObject({ min: 0, name: '件' });
    expect(chart.series[0]).toMatchObject({ type: 'line', data: [100, null, 0], connectNulls: false });
    expect(trendOption(['2026-01'], [100], '求人数', '件').series).toMatchObject([{ type: 'bar', data: [100] }]);
  });
});
