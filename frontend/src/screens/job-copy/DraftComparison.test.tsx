// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DraftReview } from './DraftComparison';
import { draftVersion, listingRecord } from './hubspotListings';
import { draftFixture, versionsFixture } from './draftFixtures.test-helper';
import { compareCopy, markInlineChanges } from './diff';
import { extractSalary } from './salaryExtract';
import { JobCopyBody } from './JobCopyBody';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
describe('保存された求人の案', () => {
  it('published version stays current, separate drafts retain exact salary and colored changed characters', () => {
    const data = versionsFixture(); const first = draftFixture(); first.draft_id = '20000000-0000-4000-8000-000000000002'; first.created_at = '2026-10-10T00:00:00Z'; first.review_status = 'rejected'; data.drafts?.unshift(first);
    const job = listingRecord(data); expect(job.versions.map(v => v.kind)).toEqual(['published', 'ai_draft', 'ai_draft']); expect(job.latestDraftId).toBe(draftFixture().draft_id); expect(job.versions[1]?.draft?.review_status).toBe('rejected');
    const current = job.versions[0]; const draft = job.versions[2]; expect(draft).toBeDefined();
    expect(extractSalary(draft?.body ?? '')).toMatchObject({ kind: '月給', min: 270000, max: 300000 });
    const marked = markInlineChanges(compareCopy(current?.body ?? null, draft?.body ?? null).lines); expect(marked.some(line => line.segments?.some(segment => segment.changed))).toBe(true);
    render(<JobCopyBody body={draft?.body ?? ''} sections={draft?.bodySections} />);
    expect(screen.getByLabelText('求人票').textContent).toContain('月給 270,000円〜300,000円'); expect(screen.getByLabelText('求人票').textContent).not.toMatch(/hidden-|基本給与|給与形態|求人id/);
  });
  it('元データの原文を加工せず開け、旧い案の原文は未取得と表示する', () => {
    const job = listingRecord(versionsFixture());
    const source = '  給与 20,000円\r\n保険：加入\n（月4回まで）　';
    render(<DraftReview job={job} draft={{ ...draftFixture(), source_text: source }} onSaved={vi.fn()} />);
    const details = screen.getByText('元データを見る').closest('details');
    expect(details?.querySelector('pre')?.textContent).toBe(source);
    expect(screen.queryByText('元データの事実と今の版の確認')).toBeNull();
    cleanup(); render(<DraftReview job={job} draft={draftFixture()} onSaved={vi.fn()} />);
    expect(screen.getByText('未取得')).toBeTruthy();
  });
  it('changing review sends explicit patch and applies returned status without changing published body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ status: 'saved', draft: { ...draftFixture(), review_status: 'adopted' }, revision: 'b'.repeat(64) }), { status:200, headers:{'Content-Type':'application/json'} })); vi.stubGlobal('fetch', fetchMock);
    const saved = vi.fn(); const job = listingRecord(versionsFixture()); render(<DraftReview job={job} draft={draftFixture()} onSaved={saved} />);
    expect(fetchMock).not.toHaveBeenCalled(); fireEvent.change(screen.getByRole('combobox', {name:'案の確認状態'}), { target:{ value:'adopted' } }); fireEvent.click(screen.getByRole('button',{name:'確認状態を保存'}));
    await waitFor(() => { expect(saved).toHaveBeenCalledWith(expect.objectContaining({review_status:'adopted'}), 'b'.repeat(64)); });
    const [url, init] = fetchMock.mock.calls[0] as [string,RequestInit]; expect(url).toBe('/api/job-copy/listings/30/draft'); expect(init.method).toBe('PATCH'); expect(JSON.parse(typeof init.body === 'string' ? init.body : '{}')).toMatchObject({ base_revision:'a'.repeat(64), draft_id:draftFixture().draft_id, status:'adopted' }); expect(job.versions[0]?.body).toContain('250,000円');
  });
  it('past and unauthorized drafts cannot change status', () => {
    const job = listingRecord(versionsFixture()); render(<DraftReview job={{...job,latestDraftId:'other'}} draft={draftFixture()} onSaved={vi.fn()} />); expect(screen.queryByRole('button',{name:'確認状態を保存'})).toBeNull(); expect(screen.getByText(/過去の案です/)).toBeTruthy();
    cleanup(); render(<DraftReview job={{...job,canWriteDrafts:false}} draft={draftFixture()} onSaved={vi.fn()} />); expect(screen.queryByLabelText('案の確認状態', { selector:'select' })).toBeNull();
  });
  it('draft status labels do not change the 84 field text', () => { const draft = draftFixture(); const before = draftVersion(draft,0); const after = draftVersion({...draft,review_status:'adopted'},0); expect(after.label).toBe('採用の案 1'); expect(after.body).toBe(before.body); });
});
