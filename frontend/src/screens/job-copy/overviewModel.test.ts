import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import { overviewRange, overviewRow, overviewRows, sortOverview } from './overviewModel';

const demo = (id: string) => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};

describe('cross-job overview', () => {
  it('matches a hand calculation of applications per day 14 days before and after the latest change', () => {
    const row = overviewRow(demo('demo-job-001'));
    expect(row.lastChange).toBe('2026-09-25');
    expect(row.kinds).toEqual(['給与', '本文', '画像']);
    // 変更前は直前の版（09-15〜09-24）の中だけ: 09-16 3, 09-18 2, 09-21 2, 09-24 1 = 8件 / 10日。
    // 09-13 の 1件は、さらに前の版の応募なので入れない。
    expect(row.before).toEqual({ days: 10, applications: 8, perDay: 8 / 10 });
    // 09-25〜10-05（取得日で打ち切り）: 09-27 1, 09-30 1, 10-02 1 = 3件 / 11日
    expect(row.after).toEqual({ days: 11, applications: 3, perDay: 3 / 11 });
    expect(row.changeDates).toEqual(['2026-09-15', '2026-09-25']);
    expect(row.billingYen).toBe(87000);
  });
  it('keeps not-connected billing and missing applications apart from zero', () => {
    const row = overviewRow(demo('demo-job-005'));
    expect(row.billingConnected).toBe(false);
    expect(row.billingYen).toBeNull();
    const empty = overviewRow(demo('demo-job-008'));
    expect(empty.applicationsAvailable).toBe(false);
    expect([empty.lastChange, empty.before, empty.after]).toEqual([null, null, null]);
    const injected = overviewRow(demo('demo-job-005'), { billing: [{ source: 'csv', start: '2026-09-12', end: '2026-09-30', amountYen: 18000 }] });
    expect([injected.billingConnected, injected.billingYen]).toEqual([true, 18000]);
  });
  it('sorts with missing values last and never adds a rank', () => {
    const rows = overviewRows(jobs);
    const sorted = sortOverview(rows, 'billing');
    expect(sorted.slice(0, 2).map(row => [row.jobId, row.billingYen])).toEqual([['demo-job-001', 87000], ['demo-job-003', 48000]]);
    expect(sorted.at(-1)?.billingYen).toBeNull();
    expect(sortOverview(rows, 'source').map(row => row.jobId)).toEqual(jobs.map(job => job.id));
    expect(Object.keys(rows[0] ?? {})).not.toContain('rank');
    expect(overviewRange(jobs, rows)).toEqual({ start: '2026-09-01', end: '2026-10-05' });
  });
  it('uses the full 14 days before a change when the previous version ran longer', () => {
    const job = demo('demo-job-001');
    const row = overviewRow({ ...job, versions: job.versions.filter(version => version.id !== 'demo-001-v2') });
    // v1 09-01〜09-24, v3 from 09-25: 09-11〜09-24 = 09-13 1, 09-16 3, 09-18 2, 09-21 2, 09-24 1 = 9件 / 14日
    expect(row.before).toEqual({ days: 14, applications: 9, perDay: 9 / 14 });
  });
});
