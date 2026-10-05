import { describe, expect, it } from 'vitest';
import type { JobCopyRecord } from './data';
import { actualApplicationCount, applicationCountLabel, orderJobs } from './jobList';
import { parseRealMoc } from './realMoc';

function job(id: string, total?: number): JobCopyRecord {
  return { id, title: id, company: 'Synthetic company', media: 'HRハッカー', mediaJobId: id, location: '', versions: [], dataSource: 'hubspot',
    ...(total === undefined ? {} : { overallApplications: { total, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {} } }) };
}

describe('actual job list applicant aggregates', () => {
  it('distinguishes unacquired counts from an acquired zero and excludes demo totals', () => {
    expect(actualApplicationCount(job('missing'))).toBeNull();
    expect(applicationCountLabel(job('missing'))).toBe('応募未取得');
    expect(actualApplicationCount(job('zero', 0))).toBe(0);
    expect(applicationCountLabel(job('zero', 0))).toBe('応募0件');
    const demo = job('demo', 42);
    delete demo.dataSource;
    expect(applicationCountLabel(demo)).toBe('応募未取得');
    for (const total of [-1, NaN, 1.5, Infinity]) expect(actualApplicationCount(job('invalid', total))).toBeNull();
  });
  it('sorts rich actual counts ahead of zero and unknown while retaining equal-count source order', () => {
    const input = [job('missing-a'), job('zero', 0), job('rich-a', 247), job('missing-b'), job('rich-b', 247), job('lower', 12)];
    expect(orderJobs(input, 'applications').map(row => row.id)).toEqual(['rich-a', 'rich-b', 'lower', 'zero', 'missing-a', 'missing-b']);
    expect(input.map(row => row.id)).toEqual(['missing-a', 'zero', 'rich-a', 'missing-b', 'rich-b', 'lower']);
    expect(orderJobs(input, 'source').map(row => row.id)).toEqual(input.map(row => row.id));
    expect(orderJobs(input.filter(row => row.id === 'lower' || row.id === 'missing-b'), 'applications').map(row => row.id)).toEqual(['lower', 'missing-b']);
  });
  it('uses the mapped overall total even when all actual applicants have unknown version attribution', () => {
    const currentId = 'capture-synthetic-2026-10-05T00:00:00Z';
    const payload = { schemaVersion: 1, capturedAt: '2026-10-05T00:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-05T00:00:00Z', jobs: [{ id: 'synthetic', hubspotListingId: '30', title: 'Synthetic job', company: 'Synthetic company', media: 'HRハッカー', mediaJobId: 'TEST', location: '', body: 'Synthetic body', images: [] }] }, results: [{ listing_id: '30', summary: { total: 247, missing_date: 7, by_date: { '2026-09-01': 240 }, dimensions: { gender: { 男性: 110, 女性: 130, 不明: 7 } } }, dated_comparison: { total: 247, unknown: 247, by_version: { [currentId]: { count: 0, dimensions: { gender: { denominator: 0, categories: [] } } } }, daily_representatives: {} } }] };
    const mapped = parseRealMoc(JSON.stringify(payload))[0];
    if (!mapped) throw new Error('Missing synthetic mapped job.');
    expect(actualApplicationCount(mapped)).toBe(247);
    expect(applicationCountLabel(mapped)).toBe('応募247件');
    expect(mapped.attributionUnknown).toBe(247);
    expect(mapped.versions[0]?.applications).toEqual({ confirmed: 0, estimated: 0, unknown: 0 });
  });
});
