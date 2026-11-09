// @vitest-environment happy-dom
/**
 * The 応募理由 lane, the per-period reason table, the 多い応募理由 column of the cross-job overview,
 * and the category breakdown on the 応募理由 tab, with the fictional demo reasons (demoReasons.ts):
 * 9 applications with a reason out of 28; 1 has no application date.
 */
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { MOTIVE } from './selectedReasonsModel';
import { selectedRow, selectionOptions, withSelectedReasons } from './selectedReasons.testFixtures';
import { CAUSAL_PATTERN, JARGON_PATTERN } from './format';
import { ApplicantReasons } from './ApplicantReasons';

vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); });

function demo(): JobCopyRecord {
  const job = jobs.find(item => item.id === 'demo-job-001');
  if (!job) throw new Error('Missing demo job');
  return job;
}
describe('多い応募理由 in the cross-job overview', () => {
  it('names the top two reasons with counts and n, and 未取得 for a job without reasons', () => {
    const other = jobs.find(item => item.id !== 'demo-job-001');
    if (!other) throw new Error('Missing second demo job');
    render(<JobOverview records={[demo(), other]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    const table = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getByRole('table');
    const header = [...table.querySelectorAll('thead th')].map(cell => cell.textContent);
    expect(header.at(-1)).toBe('多い応募理由');
    const values = within(table).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[row.querySelectorAll('td').length - 1]?.textContent);
    // 職種興味 is also 2件: the cut tie is named, never dropped silently.
    expect(values).toEqual(['給与 2件（選択1件・推定1件）・勤務地 2件（選択1件・推定1件）・ほかに同じ件数の分類1つ／n=9', '未取得']);
  });

  it('marks an old stored file: texts counted as 記述, choices 未取得, and no 記録なし without texts', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const legacy = { ...reasons, selections: null, items: [], sourceCounts: Object.fromEntries(Object.entries(reasons.sourceCounts).filter(([property]) => ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'].includes(property))) };
    render(<JobOverview records={[{ ...job, applicantReasons: legacy }]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    const table = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getByRole('table');
    expect(table.querySelector('td.jo-reasons')?.textContent).toBe('文の記録なし（分類の選択は未取得）');
    cleanup();
    render(<JobTimeline job={{ ...job, applicantReasons: legacy }} marketMode="demo" />);
    expect(within(screen.getByRole('group', { name: '応募理由' })).getByText('選択式の応募理由は未取得です。')).toBeTruthy();
  });

  it('says when the texts were cut, in the overview and under the timeline lane', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const cut = { ...reasons, truncated: true };
    render(<JobOverview records={[{ ...job, applicantReasons: cut }]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    expect(screen.getByRole('region', { name: '求人の横断比較の表' }).querySelector('td.jo-reasons')?.textContent).toBe('給与 2件（選択1件・推定1件）・勤務地 2件（選択1件・推定1件）・ほかに同じ件数の分類1つ／n=9（記述の一部だけで集計）');
    cleanup();
    render(<JobTimeline job={{ ...job, applicantReasons: cut }} marketMode="demo" />);
    expect(screen.getByRole('group', { name: '応募理由' }).textContent).toContain('選択式の応募理由は未取得です。');
  });
});


describe('listing detail with legacy reasons', () => {
  it('shows selected reasons as unread rather than inferring from old categories and text', () => {
    render(<ApplicantReasons job={demo()} />);
    const section = screen.getByRole('region', { name: '応募理由の集計' });
    expect(section.textContent).toContain('選択式の応募理由は未取得です。');
    expect(section.textContent).not.toContain('推定');
    expect(section.querySelectorAll('table')).toHaveLength(0);
  });
  it('keeps the text read limit separate from selected counts', () => {
    const job = demo();
    if (!job.applicantReasons) throw new Error('Missing demo reasons');
    render(<ApplicantReasons job={{ ...job, applicantReasons: { ...job.applicantReasons, truncated: true } }} />);
    expect(screen.getByText('文章は読み込み上限のため一部を表示しています。')).toBeTruthy();
    expect(screen.queryByText(/キーワードで推定/u)).toBeNull();
  });
  it('does not mix transfer reasons into the application-reason text', () => {
    const job = demo();
    if (!job.applicantReasons) throw new Error('Missing demo reasons');
    const transfer = job.applicantReasons.items.filter(item => item.sourceProperty === 'genshokumaeshokukaranotenshokuriyuu');
    expect(transfer).toHaveLength(1);
    render(<ApplicantReasons job={{ ...job, applicantReasons: { ...job.applicantReasons, items: transfer } }} />);
    expect(screen.getByText('応募理由の文章は記録されていません。')).toBeTruthy();
    expect(document.body.textContent).not.toContain(transfer[0]?.text);
  });
});

// origin/main の期間・操作・注記の検証を、選択式の値で継続する。
const selectedDemo = () => withSelectedReasons(demo());
const periodTables = () => within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getAllByRole('table');
const motiveCounts = () => periodTables().filter((_, index) => index % 2 === 0).map(table => table.closest('section')?.querySelector('.ar-selection-denominator')?.textContent);

describe('応募理由 lane and the per-period table', () => {
  it('shows each item denominator and all selected values, and focuses its period when pressed', () => {
    const job = selectedDemo();
    job.applicationRows = job.applicationRows?.map(row => row.date && row.date < '2026-09-15' && row.selections?.[MOTIVE]?.length ? { ...row, selections: { ...row.selections, [MOTIVE]: ['option-0', 'option-1'] } } : row);
    render(<JobTimeline job={job} marketMode="demo" />);
    expect(motiveCounts()).toEqual([
      '応募動機を聞けた応募 3件／この求人の全応募 28件',
      '応募動機を聞けた応募 5件／この求人の全応募 28件',
      '応募動機を聞けた応募 0件／この求人の全応募 28件',
    ]);
    const lane = screen.getByRole('group', { name: '応募理由' });
    const chips = within(lane).getAllByRole('button');
    expect(chips.map(chip => chip.textContent)).toEqual(['選択式の記録 3件', '選択式の記録 5件']);
    expect(chips[0]?.getAttribute('title')).toContain('応募動機: 合成項目1 3件・合成項目2 3件');
    expect(chips[0]?.getAttribute('aria-label')).toContain('優先順位③: 合成項目4 3件');
    const second = chips[1];
    if (!second) throw new Error('Missing second chip');
    fireEvent.click(second);
    expect(document.activeElement?.tagName).toBe('SECTION');
    expect(document.activeElement?.querySelector('h4')?.textContent).toBe('給与・勤務条件変更');
    expect(screen.getByText('枠を押すと、下の「期間ごとの応募理由」の表で項目ごとの数を確認できます。')).toBeTruthy();
    expect(screen.getByText('応募日も作成日も分からない応募理由 1件 は段と表に入れていません')).toBeTruthy();
    const section = screen.getByRole('region', { name: '期間ごとの応募理由' });
    expect(section.textContent).not.toMatch(CAUSAL_PATTERN);
    expect(section.textContent).not.toMatch(JARGON_PATTERN);
    expect(section.textContent).not.toMatch(/oubodouki|kinmujouken/u);
    expect(section.textContent).not.toContain('推定');
    expect(section.textContent).toContain('この数だけでは分かりません');
  });

  it('keeps unread selected reasons distinct from zero in both the lane and period tables', () => {
    render(<JobTimeline job={demo()} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect(lane.textContent).toContain('選択式の応募理由は未取得です。');
    expect(lane.querySelectorAll('button')).toHaveLength(0);
    const table = screen.getByRole('region', { name: '期間ごとの応募理由の数値' });
    expect(within(table).queryAllByRole('table')).toHaveLength(0);
    expect(table.textContent).not.toContain('0件');
  });
});

describe('reasons between record dates and on the as-of day', () => {
  const version = (id: string, observedAt: string, body: string): CopyVersion => ({ id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '', images: [] });
  const record = (dates: string[]): JobCopyRecord => ({ ...demo(), id: 'synthetic',
    versions: [version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円'), version('a2', '2026-08-12T00:00:00Z', '給与：月給230,000円'), version('b1', '2026-08-20T00:00:00Z', '給与：月給250,000円'), version('b2', '2026-08-31T00:00:00Z', '給与：月給250,000円')],
    overallApplications: { total: dates.length, missingDate: 0, fetchedAt: '2026-09-02T00:00:00Z', distributions: {}, byDate: Object.fromEntries(dates.map(date => [date, 1])), selectionOptions },
    applicationRows: dates.map((date, index) => selectedRow(String(index), date)),
  });
  it('counts days between records in the earlier body and the change day in the later body', () => {
    render(<JobTimeline job={record(['2026-08-15', '2026-08-18', '2026-08-20'])} marketMode="demo" showDummyBilling={false} />);
    expect(motiveCounts()).toEqual([
      '応募動機を聞けた応募 0件／この求人の全応募 3件',
      '応募動機を聞けた応募 2件／この求人の全応募 3件',
      '応募動機を聞けた応募 1件／この求人の全応募 3件',
      '応募動機を聞けた応募 0件／この求人の全応募 3件',
    ]);
    expect(screen.getByRole('group', { name: '応募理由' }).querySelectorAll('.jt-reason-zone')).toHaveLength(0);
  });
  it('includes the as-of day in the last period', () => {
    render(<JobTimeline job={record(['2026-09-02'])} marketMode="demo" showDummyBilling={false} />);
    expect(motiveCounts().at(-1)).toBe('応募動機を聞けた応募 1件／この求人の全応募 1件');
    expect(screen.queryByText(/表のどの期間にも入らない日付の応募理由/u)).toBeNull();
  });
  it('retains a separate reason table and lane marker for a known publication gap', () => {
    const job = record(['2026-08-15']);
    job.versions = [{ ...version('first', '2026-08-01', '本文A'), publishedFrom: '2026-08-01', publishedUntil: '2026-08-10' }, { ...version('second', '2026-08-20', '本文B'), publishedFrom: '2026-08-20' }];
    render(<JobTimeline job={job} marketMode="demo" showDummyBilling={false} />);
    expect(motiveCounts()).toEqual([
      '応募動機を聞けた応募 0件／この求人の全応募 1件',
      '応募動機を聞けた応募 1件／この求人の全応募 1件',
      '応募動機を聞けた応募 0件／この求人の全応募 1件',
    ]);
    expect(screen.getByRole('group', { name: '応募理由' }).querySelectorAll('.jt-reason-zone')).toHaveLength(1);
    expect(screen.getByText('この期間の応募は、どの本文への応募か分からないため別に数えています。')).toBeTruthy();
  });
});

describe('applications linked to more than one job', () => {
  const multiJob = (): JobCopyRecord => {
    const job = selectedDemo();
    if (!job.applicantReasons || !job.overallApplications) throw new Error('Missing fixture');
    return { ...job, applicantReasons: { ...job.applicantReasons, multiListingApplicants: ['1'.repeat(64), '2'.repeat(64)] }, overallApplications: { ...job.overallApplications, multiListing: { byDate: { '2026-09-02': 1, '2026-09-04': 1 }, missingDate: 0 } } };
  };
  it('leaves shared reasons out of the period tables and lane, retaining the overview coverage', () => {
    render(<JobTimeline job={multiJob()} marketMode="demo" />);
    expect(motiveCounts()[0]).toBe('応募動機を聞けた応募 1件／この求人の全応募 28件');
    expect(within(screen.getByRole('group', { name: '応募理由' })).getAllByRole('button').map(chip => chip.textContent)).toEqual(['選択式の記録 1件', '選択式の記録 5件']);
    expect(screen.getByText('複数の求人に関連する応募の応募理由 2件 は、期間比較表と同じく段と表に入れていません')).toBeTruthy();
    expect(screen.getByRole('region', { name: '期間ごとの応募理由' }).textContent).toContain('複数の求人に関連する応募 2件 の応募理由は、期間比較表と同じく入れていません。');
    cleanup();
    render(<JobOverview records={[multiJob()]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    expect(screen.getByRole('region', { name: '求人の横断比較の表' }).querySelector('td.jo-reasons')?.textContent).toBe('職種興味 2件（選択1件・推定1件）・給与 1件（推定1件）・ほかに同じ件数の分類3つ／n=7（ほかの求人にも関連する応募2件を除く）');
  });
  it('shows missing shared-link information only when it was not read', () => {
    const job = selectedDemo();
    const { rerender } = render(<JobTimeline job={job} marketMode="demo" />);
    expect(screen.getByRole('region', { name: '期間ごとの応募理由' }).textContent).toContain('応募理由の記録では複数の求人に関連する応募を見分けられないため');
    if (!job.applicantReasons) throw new Error('Missing reasons');
    rerender(<JobTimeline job={{ ...job, applicantReasons: { ...job.applicantReasons, multiListingApplicants: [] } }} marketMode="demo" />);
    expect(screen.getByRole('region', { name: '期間ごとの応募理由' }).textContent).not.toContain('見分けられない');
  });
});

describe('a lane with reasons but no box', () => {
  it('explains why when every selected reason is undated', () => {
    const job = selectedDemo();
    job.applicationRows = job.applicationRows?.map(row => ({ ...row, date: null }));
    render(<JobTimeline job={job} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect(lane.querySelectorAll('.jt-reason')).toHaveLength(0);
    expect(lane.textContent).toContain('表の期間に入る応募理由はありません（選択式の記録 9件 は、日付が分からないか表の期間の外です）');
    expect(screen.getByText('応募日も作成日も分からない応募理由 9件 は段と表に入れていません')).toBeTruthy();
  });
  it('explains why when a reason predates the first body record', () => {
    const job = selectedDemo();
    job.applicationRows = [selectedRow('earlier', '2026-08-30')];
    render(<JobTimeline job={job} marketMode="demo" />);
    expect(screen.getByRole('group', { name: '応募理由' }).querySelectorAll('button')).toHaveLength(0);
    expect(screen.getByText('表のどの期間にも入らない日付の応募理由 1件 は段と表に入れていません')).toBeTruthy();
  });
  it('keeps the reason section and an explanation when no periods were read', () => {
    render(<JobTimeline job={{ ...selectedDemo(), versions: [] }} marketMode="demo" />);
    expect(screen.getByRole('group', { name: '応募理由' }).textContent).toContain('掲載期間が取得できていないため、期間ごとに分けられません（選択式の記録 9件 は「応募理由」で確認できます）');
    expect(screen.getByRole('region', { name: '期間ごとの応募理由' }).textContent).toContain('掲載期間が取得できていないため、期間ごとの応募理由は出せません。');
  });
});

it('shows zero selected records when definitions were read and all selected fields are blank', () => {
  const job = selectedDemo();
  job.applicationRows = job.applicationRows?.map(row => ({ ...row, selections: {} }));
  render(<JobTimeline job={job} marketMode="demo" />);
  expect(screen.getByRole('group', { name: '応募理由' }).textContent).toContain('選択式の応募理由の記録はありません');
  expect(motiveCounts()).toEqual(Array<string>(3).fill('応募動機を聞けた応募 0件／この求人の全応募 28件'));
  expect(screen.getByRole('group', { name: '応募理由' }).querySelectorAll('button')).toHaveLength(0);
});
