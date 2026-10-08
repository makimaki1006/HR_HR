// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { AbComparison } from './AbComparison';
import { compareMetricPeriods, variantCount, publishedVariants } from './abComparisonModel';
import type { JobCopyRecord } from './data';

const metric = { period_start: '2026-09-01', period_end: '2026-09-10', impressions: 1000, clicks: 50, cost_yen: 10000, applications: 5 };
const job = (id: string, mediaJobId: string, count: number): JobCopyRecord => ({ id, mediaJobId, company: '合成企業', title: `募集${id}`, location: '大分県大分市', media: 'HRハッカー',
  versions: [{ id: `${id}-v1`, label: '掲載観測', observedAt: '2026-09-01T00:00:00Z', kind: 'published', certainty: 'estimated', source: '合成媒体CSV', body: `本文${id}`, note: '合成例', applications: { confirmed: 1, estimated: 2, unknown: 8 } }],
  attributionUnknown: 8, overallApplications: { total: count, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: { gender: { total: count, categories: [{ category: '男性', count, percentage: 100 }] } } },
  hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: mediaJobId, captured_at: '2026-10-05T00:00:00Z', rows: [metric] },
});
describe('cross-record A/B comparison', () => {
  it('keeps record totals separate from version attribution and excludes drafts', () => {
    const a = job('a', '01234567', 20);
    expect(variantCount(a, a.versions[0], 'record')).toBe(20);
    expect(variantCount(a, a.versions[0], 'version')).toBe(3);
    const missing = { ...a }; delete missing.overallApplications;
    expect(variantCount(missing, undefined, 'record')).toBeNull();
    const first = a.versions[0]; if (!first) throw new Error('Missing fixture version');
    const draft = { ...first, kind: 'ai_draft' as const };
    expect(variantCount(a, draft, 'version')).toBeNull();
    expect(publishedVariants({ ...a, versions: [draft] })).toEqual([]);
  });
  it('uses inclusive dates and independent media denominators without interpolation', () => {
    expect(compareMetricPeriods(metric, { ...metric, period_start: '2026-09-10', period_end: '2026-09-14', impressions: 500, clicks: 50, applications: 0 })).toEqual({ daysA: 10, daysB: 5, overlapDays: 1, ctrDeltaPp: 5, cvrA: 10, cvrB: 0 });
    expect(compareMetricPeriods(metric, { ...metric, period_start: '2026-10-01', period_end: '2026-10-02', clicks: null })).toMatchObject({ overlapDays: 0, ctrDeltaPp: null, cvrB: null });
  });
  it('compares different IDs and bodies with explicit scope and resets pairing confirmation', () => {
    const a = job('a', '01234567', 20); const b = job('b', '07654321', 10); const c = job('c', '09999999', 0);
    render(<AbComparison job={a} records={[a, b, c]} />);
    expect(within(screen.getByLabelText('Bとして比較する求人')).queryByText(/募集a/)).toBeNull();
    fireEvent.change(screen.getByLabelText('Bとして比較する求人'), { target: { value: 'b' } });
    expect(screen.getByLabelText('A求人の比較内容').textContent).toContain('01234567');
    expect(screen.getByLabelText('B求人の比較内容').textContent).toContain('07654321');
    expect(screen.getByLabelText('B求人の比較内容').textContent).toContain('本文b');
    fireEvent.change(screen.getByLabelText('応募の比較範囲'), { target: { value: 'version' } });
    expect(screen.getByLabelText('A求人の比較内容').textContent).toContain('選んだ版に結びついた応募）：3件');
    const confirmation = screen.getByRole('checkbox'); fireEvent.click(confirmation); expect((confirmation as HTMLInputElement).checked).toBe(true);
    fireEvent.change(screen.getByLabelText('Bとして比較する求人'), { target: { value: 'c' } });
    expect((confirmation as HTMLInputElement).checked).toBe(false);
    expect(screen.getByRole('status').textContent).toContain('組み合わせ未確認');
  });
});
