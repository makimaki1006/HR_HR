// @vitest-environment happy-dom
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { CopyVersion, JobCopyRecord } from './data';
import { reasonSourceLabels } from './applicantReasonsModel';
import { ApplicantReasons } from './ApplicantReasons';
import { SelectedReasonTables } from './SelectedReasonTables';
import { JobTimeline } from './JobTimeline';
import { loadApplicationDates } from './listingApplicationDates';
import { changeReasonBlocks } from './changeReasonsModel';
import { versionApplications, MOTIVE, PRIORITIES } from './selectedReasonsModel';
import { buildPeriods, periodRows, unpublishedRanges } from './timelineModel';
import { selectedRow, selectionOptions } from './selectedReasons.testFixtures';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
function required<T>(value: T | null | undefined): T {
  if (value == null) throw new Error('Missing fixture');
  return value;
}
const key = (digit: string) => digit.repeat(64);
const version = (id: string, date: string, body: string): CopyVersion => ({ id, label: id === 'old' ? '前の本文' : '新しい本文', observedAt: date, body, kind: 'published', certainty: 'unknown', source: '架空の本文', applications: null, note: '', images: [] });
function baseJob(): JobCopyRecord {
  return { id: 'review-fixture', dataSource: 'hubspot', title: '架空の募集', company: '架空の事業所', media: 'AirWork', mediaJobId: 'synthetic', location: '', indeedTitleState: 'none',
    versions: [version('old', '2026-10-01T03:00:00Z', '仕事内容：配送'), version('new', '2026-10-02T03:00:00Z', '仕事内容：検品')] };
}
async function readJob(shared: string[] | null) {
  const rows = [selectedRow(key('1'), '2026-10-01'), selectedRow(key('2'), '2026-10-02'), selectedRow(key('3'), null)];
  const sources = Object.keys(reasonSourceLabels);
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ listing_id: '30', total: 3, missing_date: 1, by_date: { '2026-10-01': 1, '2026-10-02': 1 }, fetched_at: '2026-10-05T00:00:00Z', applications: rows, selection_options: selectionOptions,
    applicant_reasons: { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: '2026-10-05T00:00:00Z', total_applicants: 3, total_source_values: 3 * sources.length, source_counts: Object.fromEntries(sources.map(source => [source, { missing: 3, blank: 0, nonblank: 0 }])), missing: 3 * sources.length, blank: 0, truncated: false, items: [], selections: [], ...(shared === null ? {} : { multi_listing_applicants: shared }) },
  }), { headers: { 'Content-Type': 'application/json' } })));
  return loadApplicationDates('30');
}

it('excludes shared dated and undated applications consistently across version reasons, changes and the timeline', async () => {
  const loaded = required(await readJob([key('2'), key('3')]));
  expect(loaded.overall.multiListing).toEqual({ byDate: { '2026-10-02': 1 }, missingDate: 1 });
  const job = { ...baseJob(), overallApplications: loaded.overall, applicationRows: loaded.rows, applicantReasons: loaded.reasons };
  expect(versionApplications(job, 'old').map(row => row.key)).toEqual([key('1')]);
  expect(versionApplications(job, 'new')).toEqual([]);
  expect(periodRows(job, { asOf: '2026-10-05', dummyBilling: false }).map(row => row.applications)).toEqual([1, 0]);
  const block = changeReasonBlocks(job, 4, '2026-10-05')[0];
  expect([block?.before.applications, block?.after.applications, block?.after.reasons?.n]).toEqual([1, 0, 0]);
  const { unmount } = render(<ApplicantReasons job={job} before={job.versions[0]} after={job.versions[1]} />);
  const compared = required(screen.getByRole('heading', { name: '比較先: 新しい本文' }).parentElement);
  expect(within(compared).getByText('応募 0件')).toBeTruthy();
  expect(within(compared).getByText('応募動機を聞けた応募 0件／この求人の全応募 3件')).toBeTruthy();
  expect(within(within(compared).getByRole('table', { name: '応募動機の選択件数' })).getAllByRole('cell').map(cell => cell.textContent)).toEqual(Array<string>(13).fill('0件'));
  expect(screen.getByText('ほかの求人にも関連する応募 2件は、本文ごとの応募数・理由から除いています。')).toBeTruthy();
  unmount();
  render(<JobTimeline job={job} marketMode="demo" showDummyBilling={false} />);
  const table = screen.getByRole('region', { name: '期間比較表の数値' });
  expect(within(table).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[1]?.textContent)).toEqual(['1件', '0件']);
  expect(screen.getByText('複数の求人に関連する応募 2件 は期間比較表に入れていません')).toBeTruthy();
});

it('distinguishes unread shared links from a known zero and rejects keys without an application row', async () => {
  const unknown = required(await readJob(null));
  expect(unknown.overall.multiListing).toBeUndefined();
  const zero = required(await readJob([]));
  expect(zero.overall.multiListing).toEqual({ byDate: {}, missingDate: 0 });
  const { rerender } = render(<ApplicantReasons job={{ ...baseJob(), overallApplications: unknown.overall, applicationRows: unknown.rows, applicantReasons: unknown.reasons }} />);
  expect(screen.getByText('ほかの求人にも関連する応募を見分ける情報は未取得です。')).toBeTruthy();
  rerender(<ApplicantReasons job={{ ...baseJob(), overallApplications: zero.overall, applicationRows: zero.rows, applicantReasons: zero.reasons }} />);
  expect(screen.queryByText('ほかの求人にも関連する応募を見分ける情報は未取得です。')).toBeNull();
  expect(await readJob([key('4')])).toBeNull();
});

it('honors an inclusive publication end before a later captured version and shows the gap separately', () => {
  const job: JobCopyRecord = { ...baseJob(), versions: [{ ...version('old', '2026-10-01', '仕事内容：配送'), publishedFrom: '2026-10-01', publishedUntil: '2026-10-05' }, version('new', '2026-10-20T03:00:00Z', '仕事内容：検品')],
    applicationRows: ['2026-10-05', '2026-10-06', '2026-10-10', '2026-10-20'].map(day => selectedRow(day, day)),
    overallApplications: { total: 4, missingDate: 0, fetchedAt: '2026-10-25T00:00:00Z', distributions: {}, selectionOptions, byDate: { '2026-10-05': 1, '2026-10-06': 1, '2026-10-10': 1, '2026-10-20': 1 } } };
  const periods = buildPeriods(job, '2026-10-25');
  expect(periods.map(period => [period.start, period.end])).toEqual([['2026-10-01', '2026-10-06'], ['2026-10-20', null]]);
  expect(unpublishedRanges(periods, '2026-10-25')).toEqual([{ start: '2026-10-06', end: '2026-10-20' }]);
  expect(periodRows(job, { asOf: '2026-10-25', dummyBilling: false }).map(row => [row.kind, row.start, row.lastDay, row.applications])).toEqual([
    ['period', '2026-10-01', '2026-10-05', 1], ['gap', '2026-10-06', '2026-10-19', 2], ['period', '2026-10-20', '2026-10-25', 1],
  ]);
  expect(versionApplications(job, 'old').map(row => row.date)).toEqual(['2026-10-05']);
  expect(versionApplications(job, 'new').map(row => row.date)).toEqual(['2026-10-20']);
  const noEnd = { ...job, versions: job.versions.map(version => { const copy = { ...version }; delete copy.publishedUntil; return copy; }) };
  expect(periodRows(noEnd, { asOf: '2026-10-25', dummyBilling: false }).map(row => [row.kind, row.applications])).toEqual([['period', 3], ['period', 1]]);
  render(<JobTimeline job={job} marketMode="demo" showDummyBilling={false} />);
  expect(screen.getByRole('group', { name: '掲載期間' }).querySelector('.jt-zone')?.getAttribute('title')).toBe('2026/10/06〜2026/10/19（掲載が確認できない期間）');
});

it('uses a known end even without a publication start or a later version', () => {
  const job = { ...baseJob(), versions: [{ ...version('old', '2026-10-01', '仕事内容：配送'), publishedUntil: '2026-10-05' }] };
  const periods = buildPeriods(job, '2026-10-20');
  expect(periods[0]?.end).toBe('2026-10-06');
  expect(unpublishedRanges(periods, '2026-10-20')).toEqual([{ start: '2026-10-06', end: '2026-10-21' }]);
  expect(periodRows(job, { asOf: '2026-10-20', dummyBilling: false }).map(row => [row.kind, row.start, row.lastDay])).toEqual([['period', '2026-10-01', '2026-10-05'], ['gap', '2026-10-06', '2026-10-20']]);
});

it('appends removed selections after all current options, counting each repeated selection once in the correct priority', () => {
  const rows = [selectedRow('a', '2026-10-01', { [MOTIVE]: ['option-0', '旧手当', '旧手当'], [PRIORITIES[0]]: ['旧手当'], [PRIORITIES[1]]: ['旧手当'] }), selectedRow('b', '2026-10-02', { [MOTIVE]: ['旧通勤手当'], [PRIORITIES[1]]: ['廃止された条件'], [PRIORITIES[2]]: ['option-12'] })];
  const job = { ...baseJob(), applicationRows: rows, overallApplications: { total: 2, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, selectionOptions } };
  render(<SelectedReasonTables job={job} />);
  const motivation = screen.getByRole('table', { name: '応募動機の選択件数' });
  expect(within(motivation).getAllByRole('rowheader').map(cell => cell.textContent)).toEqual([...required(selectionOptions[MOTIVE]).map(option => option.label), '旧手当（いまの選択肢にない値）', '旧通勤手当（いまの選択肢にない値）']);
  expect(within(motivation).getAllByRole('cell').map(cell => cell.textContent)).toEqual(['1件', ...Array<string>(12).fill('0件'), '1件', '1件']);
  expect(screen.getByText('応募動機を聞けた応募 2件／この求人の全応募 2件')).toBeTruthy();
  expect(screen.getByText('①を聞けた応募 1件・②2件・③1件／この求人の全応募 2件')).toBeTruthy();
  const priority = screen.getByRole('table', { name: '勤務条件の優先順位の選択件数' });
  const old = required(within(priority).getByRole('rowheader', { name: /旧手当\s*（いまの選択肢にない値）/u }).closest('tr'));
  expect(within(old).getAllByRole('cell').map(cell => cell.textContent)).toEqual(['1件', '1件', '0件']);
  const removed = required(within(priority).getByRole('rowheader', { name: /廃止された条件\s*（いまの選択肢にない値）/u }).closest('tr'));
  expect(within(removed).getAllByRole('cell').map(cell => cell.textContent)).toEqual(['0件', '1件', '0件']);
});

it('does not count a value twice when it is current in one priority and removed in another', () => {
  const definitions = { ...selectionOptions, [PRIORITIES[1]]: required(selectionOptions[PRIORITIES[1]]).map((option, index) => index === 7 ? { ...option, value: '旧手当' } : option) };
  const job = { ...baseJob(), applicationRows: [selectedRow('a', '2026-10-01', { [PRIORITIES[0]]: ['旧手当'], [PRIORITIES[1]]: ['旧手当'] })], overallApplications: { total: 1, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, selectionOptions: definitions } };
  render(<SelectedReasonTables job={job} />);
  const priority = screen.getByRole('table', { name: '勤務条件の優先順位の選択件数' });
  const cells = (label: string) => within(required(within(priority).getByRole('rowheader', { name: new RegExp(label.replace('（', '\\s*（')) }).closest('tr'))).getAllByRole('cell').map(cell => cell.textContent);
  expect(cells('合成項目8')).toEqual(['0件', '1件', '0件']);
  expect(cells('旧手当（いまの選択肢にない値）')).toEqual(['1件', '0件', '0件']);
});

it('still shows removed values when the read property definition has no remaining options', () => {
  const job = { ...baseJob(), applicationRows: [selectedRow('a', '2026-10-01', { [MOTIVE]: ['旧手当'] })], overallApplications: { total: 1, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, selectionOptions: { ...selectionOptions, [MOTIVE]: [] } } };
  render(<SelectedReasonTables job={job} />);
  const motivation = screen.getByRole('table', { name: '応募動機の選択件数' });
  expect(within(motivation).getAllByRole('rowheader').map(cell => cell.textContent)).toEqual(['旧手当（いまの選択肢にない値）']);
  expect(within(motivation).getByRole('cell').textContent).toBe('1件');
});
