// @vitest-environment happy-dom
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { loadApplicationDates } from './listingApplicationDates';
import { listingRecord } from './hubspotListings';
import { SelectedReasonTables } from './SelectedReasonTables';
import { ApplicantReasons } from './ApplicantReasons';
import { ChangeReasons } from './ChangeReasons';
import { ApplicantComposition } from './ApplicantComposition';
import { JobTimeline } from './JobTimeline';
import { MOTIVE, PRIORITIES, selectedCounts, versionApplications } from './selectedReasonsModel';
import { applicationsOutsideVersions, buildPeriods, periodRows } from './timelineModel';
import type { ApplicationRow, JobCopyRecord, SelectionOptions } from './data';

vi.mock('./savedNotes', () => ({ persistableListingId: () => null, useSavedNote: () => ({ status: 'off', value: null }) }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

function at<T>(rows: readonly T[], index: number): T {
  const value = rows[index];
  if (value === undefined) throw new Error(`Missing row ${String(index)}`);
  return value;
}
const options = Array.from({ length: 13 }, (_, index) => ({ value: `choice-${String(index)}`, label: `合成の選択肢${String(index + 1)}`, display_order: index }));
const definitions: SelectionOptions = Object.fromEntries([MOTIVE, ...PRIORITIES].map(property => [property, options]));
const row = (key: string, date: string | null, selections: Record<string, string[]> = {}): ApplicationRow => ({ key, date, selections, gender: '女性', age: '38', prefecture: '大分県', municipality: '大分市' });
export function selectedJob(): JobCopyRecord {
  // listingRecord を通し、時刻つきの HubSpot 履歴で検証する。
  const job = listingRecord({ history_counts: {}, history_may_be_incomplete: false, listing: { id: '10', media: 'hrh', media_job_id: 'synthetic', account_id: null, title: null, prefecture: null, municipality: null, category: null, publication_status: null, last_csv_detected_at: null, application_count: null }, versions: [
    { written_at: '2026-09-18T02:00:00Z', body: '仕事内容：配送', image_urls: null },
    { written_at: '2026-09-20T14:30:00Z', body: '仕事内容：検品', image_urls: null },
    { written_at: '2026-09-20T15:30:00Z', body: '仕事内容：梱包', image_urls: null },
  ] });
  return { ...job, overallApplications: { total: 6, missingDate: 1, fetchedAt: '2026-09-23T00:00:00Z', distributions: {}, selectionOptions: definitions,
    byDate: { '2026-09-17': 1, '2026-09-18': 1, '2026-09-19': 1, '2026-09-20': 1, '2026-09-21': 1 } }, applicationRows: [
    row('old', '2026-09-17'), row('a', '2026-09-18', { [MOTIVE]: ['choice-0', 'choice-1', 'choice-0'], [PRIORITIES[0]]: ['choice-1'] }),
    row('b', '2026-09-19'), row('c', '2026-09-20', { [MOTIVE]: ['choice-1'], [PRIORITIES[1]]: ['choice-0'] }),
    row('d', '2026-09-21', { [PRIORITIES[2]]: ['choice-12'] }), row('e', null, { [MOTIVE]: ['choice-0'] }),
  ] };
}

it('counts only selected values once per application and retains separate denominators', () => {
  const job = selectedJob();
  const rows = job.applicationRows ?? [];
  const counts = selectedCounts([...rows, at(rows, 1)], MOTIVE);
  expect(counts.answered).toBe(3);
  expect([...counts.counts]).toEqual([['choice-0', 2], ['choice-1', 2]]);
  render(<SelectedReasonTables job={job} />);
  const tables = screen.getAllByRole('table');
  expect(tables).toHaveLength(2);
  expect(within(at(tables, 0)).getAllByRole('columnheader').map(cell => cell.textContent)).toEqual(['選択肢', '件数']);
  expect(within(at(tables, 0)).getAllByRole('cell').map(cell => cell.textContent)).toEqual(['2件', '2件', ...Array<string>(11).fill('0件')]);
  expect(screen.getByText('応募動機を聞けた応募 3件／この求人の全応募 6件')).toBeTruthy();
  expect(screen.getByText('①を聞けた応募 1件・②1件・③1件／この求人の全応募 6件')).toBeTruthy();
  expect(within(at(tables, 1)).getAllByRole('columnheader').map(cell => cell.textContent)).toEqual(['選択肢', '①', '②', '③']);
  for (const table of tables) {
    expect(table.textContent).not.toContain('聞けた応募');
    expect(within(table).getAllByRole('rowheader').map(cell => cell.textContent)).toEqual(options.map(option => option.label));
    expect(within(table).getAllByRole('row')).toHaveLength(14);
  }
  expect(within(at(tables, 1)).getAllByRole('row').slice(1).map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent))).toEqual([
    ['0件', '1件', '0件'],
    ['1件', '0件', '0件'],
    ...Array.from({ length: 10 }, () => ['0件', '0件', '0件']),
    ['0件', '0件', '1件'],
  ]);
});

it('assigns JST change days to the new body and keeps undated and earlier applications outside', () => {
  const job = selectedJob();
  const ids = job.versions.map(version => version.id);
  expect(buildPeriods(job, '2026-09-23').map(period => [period.start, period.end, period.basis])).toEqual([
    ['2026-09-18', '2026-09-20', 'captured'], ['2026-09-20', '2026-09-21', 'captured'], ['2026-09-21', null, 'captured'],
  ]);
  expect(ids.map(id => versionApplications(job, id).map(application => application.key))).toEqual([['a', 'b'], ['c'], ['d']]);
  const periods = periodRows(job, { asOf: '2026-09-23', dummyBilling: false });
  expect(periods.map(period => period.applications)).toEqual([2, 1, 1]);
  expect(applicationsOutsideVersions(job, periods)).toBe(2);
  expect(ids.map(id => selectedCounts(versionApplications(job, id), MOTIVE).answered)).toEqual([1, 1, 0]);
});

it('puts all applications on a day with two writes into the last body of that day', () => {
  const job = selectedJob();
  at(job.versions, 2).observedAt = '2026-09-20T14:45:00Z';
  expect(job.versions.map(version => versionApplications(job, version.id).map(application => application.key))).toEqual([['a', 'b'], [], ['c', 'd']]);
});

it('does not count old category selections or infer reasons from the attached text', () => {
  const job = selectedJob();
  job.applicantReasons = { available: true, basis: 'recorded_applicant_reason', fetchedAt: null, totalApplicants: 6, totalSourceValues: 1, sourceCounts: {}, missing: 0, blank: 0, truncated: false, optionLabels: 'read',
    selections: [{ applicant: 'b', sourceProperty: 'ouboriyuukategori_hiaringu', value: 'salary', label: '給与', applicationDate: '2026-09-19' }],
    items: [{ id: 'text', applicant: 'b', sourceProperty: 'oubodouki', applicationDate: '2026-09-19', versionId: null, collectedAt: null, text: '給与を希望。連絡先 090-1234-5678' }] };
  const { container } = render(<ApplicantReasons job={job} />);
  expect(screen.getByText('選択式の記録は 2026/09/18 の応募からです')).toBeTruthy();
  expect(screen.getByText('内容を見る（応募ごとの文章・社内確認用）').closest('details')?.open).toBe(false);
  expect(container.textContent).not.toContain('090-1234-5678');
  expect(container.textContent).not.toContain('推定');
  expect(within(at(screen.getAllByRole('table'), 0)).getAllByRole('cell').map(cell => cell.textContent).slice(0, 2)).toEqual(['2件', '2件']);
});

it('compares selected reasons around changes and shows all versions on the timeline', () => {
  const job = selectedJob();
  const { unmount } = render(<ChangeReasons job={job} onOpenFactors={() => undefined} />);
  const blocks = screen.getAllByTestId('change-reason-block');
  expect(blocks).toHaveLength(2);
  expect(within(at(blocks, 0)).getAllByRole('table')).toHaveLength(4);
  expect(within(at(blocks, 0)).getByRole('group', { name: '変更前 1日' }).textContent).toContain('応募 1件');
  expect(within(at(blocks, 0)).getByRole('group', { name: '変更後 3日' }).textContent).toContain('応募 1件');
  unmount();
  render(<JobTimeline job={{ ...job, indeedTitleState: 'none' }} marketMode="demo" />);
  const reasons = screen.getByRole('region', { name: '期間ごとの応募理由' });
  expect(within(reasons).getAllByRole('table')).toHaveLength(6);
  expect(reasons.textContent).not.toContain('推定');
  expect(within(screen.getByRole('region', { name: '期間比較表' })).getByRole('table').textContent).toContain('2件');
});

it('loads selected values and full attribute distributions from the same applicant response', async () => {
  const job = selectedJob();
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ listing_id: '10', total: 6, missing_date: 1, created_date_count: 1, fetched_at: job.overallApplications?.fetchedAt, by_date: job.overallApplications?.byDate, applications: job.applicationRows, selection_options: definitions }), { headers: { 'Content-Type': 'application/json' } })));
  const loaded = await loadApplicationDates('10');
  if (!loaded) throw new Error('Missing loaded applications');
  expect(loaded.rows[1]?.selections?.[MOTIVE]).toEqual(['choice-0', 'choice-1']);
  expect(loaded.overall.selectionOptions?.[MOTIVE]?.map(option => option.label)).toEqual(options.map(option => option.label));
  expect(loaded.overall.createdDateCount).toBe(1);
  for (const dimension of ['gender', 'age', 'prefecture', 'municipality'] as const) expect(loaded.overall.distributions[dimension]?.total).toBe(6);
  render(<ApplicantComposition job={{ ...job, overallApplications: loaded.overall }} includeReasons={false} />);
  expect(screen.getByLabelText('求人全体の性別').textContent).toContain('女性6件 (100.0%)');
  expect(screen.getByLabelText('求人全体の年代').textContent).toContain('30代6件 (100.0%)');
  expect(screen.getByLabelText('求人全体の都道府県').textContent).toContain('大分県6件 (100.0%)');
  expect(screen.getByLabelText('求人全体の市区町村').textContent).toContain('大分市6件 (100.0%)');
});

it('retains the existing small-area protection when counting the full applicant attributes', async () => {
  const job = selectedJob();
  const rows = job.applicationRows ?? [];
  at(rows, 0).gender = '男性';
  at(rows, 0).age = '48';
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ listing_id: '10', total: 6, missing_date: 1, fetched_at: job.overallApplications?.fetchedAt, by_date: job.overallApplications?.byDate, applications: rows }), { headers: { 'Content-Type': 'application/json' } })));
  const loaded = await loadApplicationDates('10');
  expect(loaded?.overall.distributions.municipality?.categories.map(row => [row.category, row.count])).toEqual([['その他', 1], ['大分県大分市', 5]]);
  expect(loaded?.overall.distributions.prefecture?.categories.map(row => [row.category, row.count])).toEqual([['その他', 1], ['大分県', 5]]);
});
