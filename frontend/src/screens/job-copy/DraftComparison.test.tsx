// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DraftFacts, DraftReview, factDifferences } from './DraftComparison';
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
  it('salary and location differences show concrete facts, equal salaries ignore formatting, missing stays missing', () => {
    const job = listingRecord(versionsFixture()); const draft = draftFixture();
    expect(factDifferences(draft, job.versions[0], job.location).map(item => item.key)).toEqual(['salary','work_location']);
    render(<DraftFacts draft={draft} current={job.versions[0]} location={job.location} />);
    const table = screen.getByRole('table'); expect(table.textContent).toContain('月給 250,000円〜280,000円'); expect(table.textContent).toContain('大分県別府市'); expect(table.textContent).toContain('大分県大分市'); expect(table.textContent).not.toContain('土日休み');
    const formatted = { ...draft, facts: { salary: { value: '月給25万円〜28万円', evidence_quote: '月給25万円〜28万円', status: 'verified' } } };
    expect(factDifferences(formatted, job.versions[0], job.location)).toEqual([]);
    expect(factDifferences(formatted, undefined, '勤務地不明')).toMatchObject([{ current: '未取得', missing: true }]);
    formatted.facts.salary.status = 'rejected'; expect(factDifferences(formatted, undefined, '勤務地不明')).toEqual([]);
  });
  it.each(['本文', '表示用項目'])('%sの休日・休暇と休日を事実と比較する', representation => {
    const draft = { ...draftFixture(), facts: { holidays: { value: '土日休み', evidence_quote: '休日は土日休み。', status: 'verified' } } };
    const base = listingRecord(versionsFixture()).versions[0];
    if (!base) throw new Error('比較する今の版がありません');
    for (const heading of ['休日・休暇', '休日']) {
      const current = { ...base, body: `${heading}：土日休み`, bodySections: representation === '表示用項目' ? [{ heading, text: '土日休み' }] : [] };
      expect(factDifferences(draft, current, '勤務地不明')).toEqual([]);
      const different = { ...current, body: `${heading}：日曜休み`, bodySections: representation === '表示用項目' ? [{ heading, text: '日曜休み' }] : [] };
      expect(factDifferences(draft, different, '勤務地不明')).toMatchObject([{ key: 'holidays', current: '日曜休み', fact: '土日休み', missing: false }]);
      render(<DraftFacts draft={draft} current={different} location="勤務地不明" />);
      expect(screen.getByRole('table').textContent).toContain('日曜休み');
      expect(screen.getByRole('table').textContent).toContain('土日休み');
      expect(screen.getByRole('table').textContent).not.toContain('未取得');
      cleanup();
    }
  });
  it.each(['84項目', '本文', '表示用項目'])('%sの福利厚生・待遇と手当を事実と比較する', representation => {
    const value = '通勤手当（上限20,000円／月）';
    const other = '通勤手当（上限10,000円／月）';
    const draft = { ...draftFixture(), facts: { allowances: { value, evidence_quote: value, status: 'verified' } } };
    const base = listingRecord(versionsFixture()).versions[0];
    if (!base) throw new Error('比較する今の版がありません');
    for (const heading of ['福利厚生・待遇', '手当']) {
      const currentFor = (text: string) => representation === '84項目'
        ? draftVersion({ ...draftFixture(), row: { ...draftFixture().row, '自由項目2のタイトル': heading, '自由項目2の内容': text } }, 0)
        : { ...base, body: `${heading}：${text}`, bodySections: representation === '表示用項目' ? [{ heading, text }] : [] };
      expect(factDifferences(draft, currentFor(value), '勤務地不明')).toEqual([]);
      const different = currentFor(other);
      expect(factDifferences(draft, different, '勤務地不明')).toMatchObject([{ key: 'allowances', current: other, fact: value, missing: false }]);
      render(<DraftFacts draft={draft} current={different} location="勤務地不明" />);
      expect(screen.getByRole('table').textContent).toContain(other);
      expect(screen.getByRole('table').textContent).toContain(value);
      expect(screen.getByRole('table').textContent).not.toContain('未取得');
      cleanup();
    }
    expect(factDifferences(draft, { ...base, body: '', bodySections: [] }, '勤務地不明')).toMatchObject([{ current: '未取得', missing: true }]);
  });
  it.each(['84項目', '本文', '表示用項目'])('%sの福利厚生欄から保険と手当を別々に比較する', representation => {
    const insurance = '雇用保険 労災保険 健康保険 厚生年金';
    const allowance = '夜勤手当3,500円/回';
    const draft = { ...draftFixture(), facts: {
      insurance: { value: insurance, evidence_quote: insurance, status: 'verified' },
      allowances: { value: allowance, evidence_quote: allowance, status: 'verified' },
    } };
    const base = listingRecord(versionsFixture()).versions[0];
    if (!base) throw new Error('比較する今の版がありません');
    const currentFor = (text: string) => representation === '84項目'
      ? draftVersion({ ...draftFixture(), row: { ...draftFixture().row, '自由項目2のタイトル': '福利厚生・待遇', '自由項目2の内容': text } }, 0)
      : { ...base, body: `福利厚生・待遇：${text}\n\n勤務時間：8時〜17時`, bodySections: representation === '表示用項目' ? [{ heading: '福利厚生・待遇', text }] : [] };
    for (const text of [`${insurance}\n${allowance}`, `${allowance}\r\n${insurance}\r\n制服貸与`]) {
      expect(factDifferences(draft, currentFor(text), '勤務地不明')).toEqual([]);
      render(<DraftFacts draft={draft} current={currentFor(text)} location="勤務地不明" />);
      expect(screen.queryByRole('table')).toBeNull();
      expect(screen.getByText(/確認できた事実の違いはありません/)).toBeTruthy();
      cleanup();
    }
    const different = currentFor('雇用保険 労災保険\n夜勤手当3,000円/回');
    expect(factDifferences(draft, different, '勤務地不明')).toMatchObject([
      { key: 'insurance', current: '雇用保険 労災保険', missing: false },
      { key: 'allowances', current: '夜勤手当3,000円/回', missing: false },
    ]);
    expect(factDifferences(draft, currentFor(insurance), '勤務地不明')).toMatchObject([{ key: 'allowances', current: '未取得', missing: true }]);
    expect(factDifferences(draft, currentFor(allowance), '勤務地不明')).toMatchObject([{ key: 'insurance', current: '未取得', missing: true }]);
    const prefix = currentFor(`${insurance}\n夜勤手当3,500円/回（月4回まで）`);
    expect(factDifferences(draft, prefix, '勤務地不明')).toMatchObject([{ key: 'allowances', current: '夜勤手当3,500円/回（月4回まで）', missing: false }]);
    expect(factDifferences(draft, currentFor(`${insurance}\n夜勤手当3,5000円/回`), '勤務地不明')).toHaveLength(1);
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
