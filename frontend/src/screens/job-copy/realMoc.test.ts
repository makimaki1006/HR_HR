import { describe, expect, it } from 'vitest';
import { parseRealMoc } from './realMoc';

function first<T>(rows: T[]): T {
  const row = rows[0];
  if (row === undefined) throw new Error('Missing synthetic fixture');
  return row;
}

function fixture() {
  const id = 'capture-synthetic-real-2026-10-04T00:00:00.000Z';
  const empty = { denominator: 0, categories: [] };
  return { schemaVersion: 1, capturedAt: '2026-10-04T00:00:00.000Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-04T00:00:00.000Z', jobs: [{
    id: 'synthetic-real', hubspotListingId: '30', title: '架空集計求人', company: '架空取引先', media: 'HRハッカー', mediaJobId: 'TEST-REAL', location: '架空市', body: '架空の本文', images: [],
  }] }, results: [{ listing_id: '30', summary: { total: 11, missing_date: 0, by_date: { '2026-09-01': 11 }, dimensions: { gender: { 男性: 6, 女性: 4, 不明: 1 }, age: { '30代': 8, 不明: 3 }, prefecture: { 東京都: 9, 不明: 2 }, municipality: { 不明: 11 } } },
    dated_comparison: { total: 11, unknown: 11, basis: '架空の日付対応', by_version: { [id]: { count: 0, dimensions: { gender: empty, age: empty, prefecture: empty, municipality: empty } } } as Record<string, { count: number; dimensions: Record<string, typeof empty> }>, daily_representatives: {} },
  }] };
}

describe('real MOC aggregates', () => {
  it('preserves validated daily aggregates and rejects impossible dates before charting', () => {
    const input = fixture();
    expect(first(parseRealMoc(JSON.stringify(input))).overallApplications?.byDate).toEqual({ '2026-09-01': 11 });
    const summary = first(input.results).summary;
    Object.assign(summary, { by_date: { '2026-02-30': 11 } });
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
    Object.assign(summary, { by_date: { '2026-09': 11 } });
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
  });
  it('preserves optional recorded reasons without inferring version attribution from the application date', () => {
    const input = fixture();
    const reasons = { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null,
      fetched_at: input.capturedAt, total_applicants: 11, total_source_values: 33,
      source_counts: { oubodouki: { missing: 10, blank: 0, nonblank: 1 }, ouboriyuu_baitaikisai: { missing: 11, blank: 0, nonblank: 0 }, ouboriyuu_hiaringu: { missing: 11, blank: 0, nonblank: 0 } }, missing: 32, blank: 0, truncated: false,
      items: [{ id: 'a'.repeat(64), text: '合成例：研修の説明を確認しました。', source: 'hubspot', source_property: 'oubodouki', application_date: '2026-09-01', collected_at: null, version_id: null }] };
    const enriched = { ...input, results: [{ ...first(input.results), applicant_reasons: reasons }] };
    const [job] = parseRealMoc(JSON.stringify(enriched));
    expect(job?.applicantReasons?.items[0]?.versionId).toBeNull();
    expect(job?.applicantReasons?.items[0]?.collectedAt).toBeNull();
    expect(job?.overallApplications?.total).toBe(11);
    reasons.total_applicants = 10;
    expect(() => parseRealMoc(JSON.stringify(enriched))).toThrow();
    expect(first(parseRealMoc(JSON.stringify(input))).applicantReasons).toBeUndefined();
  });
  it('keeps all 11 unattributed applications in overall counts without allocating them to a version', () => {
    const [job] = parseRealMoc(JSON.stringify(fixture()));
    expect(job?.hubspotId).toBe('30');
    expect(job?.dataSource).toBe('hubspot');
    expect(job?.attributionUnknown).toBe(11);
    expect(job?.overallApplications?.total).toBe(11);
    expect(job?.overallApplications?.distributions.gender?.categories).toEqual([
      { category: '男性', count: 6, percentage: 6 / 11 * 100 }, { category: '女性', count: 4, percentage: 4 / 11 * 100 }, { category: '不明', count: 1, percentage: 1 / 11 * 100 },
    ]);
    expect(job?.versions[0]?.applications).toEqual({ confirmed: 0, estimated: 0, unknown: 0 });
    expect(job?.versions[0]?.distributions?.gender?.total).toBe(0);
  });

  it('matches results by HubSpot listing ID regardless of row order', () => {
    const input = fixture();
    input.capture_bundle.jobs.push({ ...first(input.capture_bundle.jobs), id: 'synthetic-other', hubspotListingId: '31' });
    input.results.unshift({ ...first(input.results), listing_id: '31', dated_comparison: { ...first(input.results).dated_comparison, by_version: {} } });
    const jobs = parseRealMoc(JSON.stringify(input));
    expect(jobs.map(job => job.hubspotId)).toEqual(['30', '31']);
    expect(jobs.map(job => job.overallApplications?.total)).toEqual([11, 11]);
  });

  it('rejects a same media ID paired to a different listing ID', () => {
    const input = fixture(); first(input.results).listing_id = '999';
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
  });

  it('rejects duplicate or missing result rows', () => {
    const input = fixture(); input.results.push(first(input.results));
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
    input.results = [];
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
  });

  it('rejects inconsistent aggregate counts and inconsistent version attribution totals', () => {
    const input = fixture(); first(input.results).summary.dimensions.gender.男性 = 7;
    expect(() => parseRealMoc(JSON.stringify(input))).toThrow();
    const second = fixture(); first(second.results).dated_comparison.unknown = 10;
    expect(() => parseRealMoc(JSON.stringify(second))).toThrow();
  });

  it('does not turn absent attribution data into assigned version counts', () => {
    const input = fixture();
    const value = { ...input, results: [{ ...input.results[0], dated_comparison: null }] };
    const [job] = parseRealMoc(JSON.stringify(value));
    expect(job?.attributionUnknown).toBe(11);
    expect(job?.versions[0]?.applications).toBeNull();
  });
});
