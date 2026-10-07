import { describe, expect, it } from 'vitest';
import { overallFromLiveSummary } from './liveApplications';

describe('applications read live from HubSpot', () => {
  it('keeps the dates, the undated count and the shares HubSpot returned', () => {
    const overall = overallFromLiveSummary({ total: 5, missing_date: 1, by_date: { '2026-09-20': 1, '2026-10-01': 3 }, dimensions: { gender: { 男性: 3, 女性: 1, 不明: 1 } } }, '2026-10-05T00:00:00Z');
    expect(overall).toEqual({
      total: 5, missingDate: 1, fetchedAt: '2026-10-05T00:00:00Z', byDate: { '2026-09-20': 1, '2026-10-01': 3 },
      distributions: { gender: { total: 5, categories: [{ category: '男性', count: 3, percentage: 60 }, { category: '女性', count: 1, percentage: 20 }, { category: '不明', count: 1, percentage: 20 }] } },
    });
  });
  it('uses nothing when the counts do not add up or a date is not a calendar day', () => {
    expect(overallFromLiveSummary({ total: 5, missing_date: 0, by_date: { '2026-09-20': 1 }, dimensions: {} }, '2026-10-05T00:00:00Z')).toBeNull();
    expect(overallFromLiveSummary({ total: 1, missing_date: 0, by_date: { '2026-02-30': 1 }, dimensions: {} }, '2026-10-05T00:00:00Z')).toBeNull();
    expect(overallFromLiveSummary({ total: 1, missing_date: 0, by_date: { '2026-09-01': 1.5 }, dimensions: {} }, '2026-10-05T00:00:00Z')).toBeNull();
  });
  it('leaves out a dimension whose counts do not add up to the total', () => {
    const overall = overallFromLiveSummary({ total: 2, missing_date: 2, by_date: {}, dimensions: { age: { '30代': 1 } } }, '2026-10-05T00:00:00Z');
    expect(overall?.distributions).toEqual({});
    expect(overall?.byDate).toEqual({});
  });
});
