// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { AbComparison } from './AbComparison';
import { comparisonPeriod, contentDifferences, localCandidates, periodPerformance, publishedVariants, variantCount } from './abComparisonModel';
import type { JobCopyRecord } from './data';

const row = { period_start: '2026-09-01', period_end: '2026-09-28', impressions: 1000, clicks: 50, cost_yen: 10000, applications: 5 };
const job = (id: string, accountId = 'fictional-account'): JobCopyRecord => ({ id, accountId, mediaJobId: '12345678', company: '架空物流', title: `募集${id}`, location: '大分県大分市', media: 'HRハッカー', listingStartedOn: '2026-09-01',
  versions: [{ id: `${id}-v1`, label: '掲載した本文', observedAt: '2026-09-01T00:00:00Z', kind: 'published', certainty: 'estimated', source: '合成媒体CSV', body: `本文${id}`, note: '合成例', applications: { confirmed: 1, estimated: 2, unknown: 8 }, images: [] }],
  overallApplications: { total: 20, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: { gender: { total: 20, categories: [{ category: '男性', count: 20, percentage: 100 }] } } },
  hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: '12345678', captured_at: '2026-10-05T00:00:00Z', rows: [row] },
});
const performance = (value: JobCopyRecord) => { if (!value.hrhPerformance) throw new Error('Missing test performance'); return value.hrhPerformance; };
const version = (value: JobCopyRecord) => { const first = value.versions[0]; if (!first) throw new Error('Missing test body'); return first; };
const parent = (value: HTMLElement) => { const result = value.parentElement; if (!result) throw new Error('Missing test parent'); return result; };
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
describe('求人比較', () => {
  it('keeps acquired totals separate from body attribution and excludes drafts', () => {
    const a = job('a');
    expect(variantCount(a, a.versions[0], 'record')).toBe(20);
    expect(variantCount(a, a.versions[0], 'version')).toBe(3);
    expect(publishedVariants({ ...a, versions: a.versions.map(v => ({ ...v, kind: 'ai_draft' })) })).toEqual([]);
  });
  it('aligns inclusive 28-day windows; never substitutes an observation date for a missing start', () => {
    expect(comparisonPeriod(job('a'), 'start', '2026-10-16')).toEqual({ start: '2026-09-01', end: '2026-09-28' });
    expect(comparisonPeriod({ ...job('b'), listingStartedOn: '2026-09-08' }, 'start', '2026-10-16')).toEqual({ start: '2026-09-08', end: '2026-10-05' });
    expect(comparisonPeriod({ ...job('a'), listingStartedOn: null }, 'start', '2026-10-16')).toBeNull();
    expect(comparisonPeriod({ ...job('a'), listingStartedOn: '2026-02-30' }, 'start', '2026-10-16')).toBeNull();
    expect(comparisonPeriod(job('a'), 'recent', '2026-10-16')).toEqual({ start: '2026-09-19', end: '2026-10-16' });
  });
  it('includes boundary weeks without prorating and returns null for gaps or missing metric values', () => {
    const a = job('a'); const period = { start: '2026-09-03', end: '2026-09-30' };
    const full = { ...a, hrhPerformance: { ...performance(a), rows: [{ ...row, period_end: '2026-09-07' }, { ...row, period_start: '2026-09-08', period_end: '2026-09-30', cost_yen: 20000, applications: 10 }] } };
    expect(periodPerformance(full, period)).toMatchObject({ covered: true, total: { impressions: 2000, clicks: 100, applications: 15, cost_yen: 30000 }, ratios: { ctr: 5, cpa: 2000 }, boundaryWeeks: [{ start: '2026-09-01', end: '2026-09-07' }] });
    const gap = { ...full, hrhPerformance: { ...full.hrhPerformance, rows: full.hrhPerformance.rows.map((r, i) => i ? { ...r, period_start: '2026-09-09' } : r) } };
    expect(periodPerformance(gap, period)).toMatchObject({ covered: false, total: { impressions: null, applications: null, cost_yen: null } });
    expect(periodPerformance({ ...a, hrhPerformance: undefined }, period).total.applications).toBeNull();
    expect(periodPerformance({ ...full, hrhPerformance: { ...full.hrhPerformance, rows: full.hrhPerformance.rows.map(r => ({ ...r, cost_yen: null, applications: 0 })) } }, period)).toMatchObject({ total: { cost_yen: null, applications: 0 }, ratios: { cpa: null } });
  });
  it('orders all local candidates before pagination; uses account rather than employer name', () => {
    const a = { ...job('a'), listingDealIds: ['deal-1'] };
    const pastSame = { ...job('z'), listingDealIds: ['deal-1'], listingPublicationStatus: '掲載終了' };
    const live = { ...job('y'), listingPublicationStatus: '掲載中' };
    const past = { ...job('b'), listingPublicationStatus: '掲載終了' };
    const outsider = job('x', 'another-fictional-account');
    const filters = { occupation: '', prefecture: '', media: '', q: '' };
    expect(localCandidates(a, [past, live, a, outsider, pastSame], 'same', filters, '2026-09-28').map(j => j.id)).toEqual(['z', 'y', 'b']);
    const popular = { ...job('popular', 'other'), hrhPerformance: { ...performance(outsider), rows: [{ ...row, applications: 15 }] } };
    expect(localCandidates(a, [a, outsider, popular, past], 'other', filters, '2026-09-28').map(j => j.id)).toEqual(['popular', 'x']);
    expect(localCandidates(a, [outsider], 'other', { ...filters, prefecture: '東京都' }, '2026-09-28')).toEqual([]);
  });
  it('marks only acquired fields that differ from the current job, including images', () => {
    const a = job('a'); const b = { ...job('b'), title: a.title, versions: [{ ...version(job('b')), images: [{ id: 'image-1', url: 'https://example.invalid/fictional.png', caption: '架空画像' }] }] };
    const rows = contentDifferences([a, b]);
    expect(rows.find(r => r.key === 'title')).toMatchObject({ different: false, differences: [false, false] });
    expect(rows.find(r => r.key === 'body')).toMatchObject({ different: true, differences: [false, true], values: ['本文a', '本文b'] });
    expect(rows.find(r => r.key === 'images')).toMatchObject({ different: true, differences: [false, true] });
  });
  it('renders three columns, automatically fills metrics, removes opponents and caps selection at four', () => {
    const a = job('a'); const records = [a, ...['b', 'c', 'd', 'e', 'f'].map(id => job(id))];
    render(<AbComparison job={a} records={records} />);
    fireEvent.click(screen.getByRole('button', { name: '募集bを比較に追加' }));
    fireEvent.click(screen.getByRole('button', { name: '募集cを比較に追加' }));
    const table = within(screen.getByRole('region', { name: '成果の横並び比較' }));
    expect(table.getAllByRole('columnheader').map(el => el.textContent)).toEqual(['項目', '今の求人募集a', '募集b募集bを外す', '募集c募集cを外す']);
    expect(within(parent(table.getByRole('rowheader', { name: '応募数' }))).getAllByRole('cell').map(el => el.textContent)).toEqual(['5件', '5件', '5件']);
    fireEvent.click(screen.getByRole('button', { name: '募集dを比較に追加' })); fireEvent.click(screen.getByRole('button', { name: '募集eを比較に追加' }));
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '募集fを比較に追加' }).disabled).toBe(true);
    fireEvent.click(table.getByRole('button', { name: '募集bを外す' }));
    expect(table.getAllByRole('columnheader')).toHaveLength(5);
    expect(screen.queryByLabelText('比較グループ名')).toBeNull();
    expect(screen.queryByRole('checkbox')).toBeNull();
    expect(screen.getByText('応募者構成（性別・年代・地域）').parentElement?.hasAttribute('open')).toBe(false);
  });
  it('shows missing start explicitly and changes every column to the same recent period', () => {
    const a = { ...job('a'), listingStartedOn: null };
    render(<AbComparison job={a} records={[a, job('b')]} />);
    expect(screen.getByText('掲載開始日が分かりません')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '募集bを比較に追加' }));
    fireEvent.change(screen.getByLabelText('比べる期間'), { target: { value: 'recent' } });
    expect(screen.queryByText('掲載開始日が分かりません')).toBeNull();
    const table = within(screen.getByRole('region', { name: '成果の横並び比較' }));
    const values = within(parent(table.getByRole('rowheader', { name: '比べる期間' }))).getAllByRole('cell').map(el => el.textContent);
    expect(values[0]).toBe(values[1]);
    expect(within(parent(table.getByRole('rowheader', { name: '費用' }))).getAllByRole('cell').map(el => el.textContent)).toEqual(['未取得', '未取得']);
  });
  it('keeps application reasons available in collapsed columns and excludes reasons for leaving a job', () => {
    const a: JobCopyRecord = { ...job('a'), applicantReasons: { available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-10-08T00:00:00Z', totalApplicants: 2, totalSourceValues: 2,
      sourceCounts: { oubodouki: { missing: 1, blank: 0, nonblank: 1 }, genshokumaeshokukaranotenshokuriyuu: { missing: 1, blank: 0, nonblank: 1 } }, missing: 2, blank: 0, truncated: false, selections: [], optionLabels: 'read', items: [
        { id: 'a'.repeat(64), applicant: '1'.repeat(64), text: '研修があるので応募しました', sourceProperty: 'oubodouki', applicationDate: '2026-09-01', collectedAt: null, versionId: null },
        { id: 'b'.repeat(64), applicant: '2'.repeat(64), text: '前職は通勤が長いため', sourceProperty: 'genshokumaeshokukaranotenshokuriyuu', applicationDate: '2026-09-02', collectedAt: null, versionId: null },
      ] } };
    render(<AbComparison job={a} records={[a, job('b'), job('c')]} />);
    fireEvent.click(screen.getByRole('button', { name: '募集bを比較に追加' }));
    fireEvent.click(screen.getByRole('button', { name: '募集cを比較に追加' }));
    const reasons = parent(screen.getByText('応募理由（内部閲覧）', { exact: true }));
    expect(reasons.hasAttribute('open')).toBe(false);
    expect(reasons.querySelectorAll('.ab-reasons > section')).toHaveLength(3);
    expect(reasons.textContent).toContain('表示対象：1記述');
    expect(reasons.textContent).toContain('研修があるので応募しました');
    expect(reasons.textContent).not.toContain('前職は通勤が長いため');
  });

});
