// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealComparison, ParallelJobs } from './DealComparison';
import { HubSpotListingsPanel } from './HubSpotListingsPanel';
import { compareJobs, salaryAmount } from './dealComparisonModel';
import type { HubSpotListing } from './hubspotListings';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const row: HubSpotListing = { id: '1', media: 'hrh', media_job_id: 'internal-hrh', account_id: null, title: '配送の仕事', prefecture: '沖縄県', municipality: '那覇市', category: 'ドライバー', publication_status: '公開', last_csv_detected_at: null, application_count: 12,
  summary: { occupation: '配送ドライバー', location: '沖縄県那覇市', salary_kind: '月給', salary_min: '250000', salary_max: '300000', hours: '8:00〜17:00', holidays: '土日休み', body_changed_at: '2026-10-10T00:00:00Z' } };
const page = { status: 'ready', total: 101, listings: [row, { ...row, id: '2', title: '配送の仕事・別媒体', media: 'airwork', publication_status: '01' }], titles: ['ドライバー'], deals: [{ id: '901', name: '合成配送の契約', count: 101 }], offset: 0, next_offset: 50, index_built_at: '2026-10-10T00:00:00Z', no_deal_count: 3 };
const body = (salary: string) => `求人id：raw-id-secret\n案件名：配送の仕事\n仕事内容：日用品を届けます\n給与形態：月給\n基本給与 最小：${salary}\n基本給与 最大：300000\n勤務時間：8:00〜17:00`;
it('formats exact salaries and never infers prose conditions', () => {
  expect(salaryAmount('月給', '250000')).toBe('月給 250,000円');
  expect(salaryAmount('時給', '1,200')).toBe('時給 1,200円');
  expect(salaryAmount('月給', null)).toBe('未取得');
  expect(salaryAmount(null, '250000')).toBe('給与形態不明 250,000円');
  const result = compareJobs([{ id: 'hrh', title: 'A', media: 'HRハッカー', body: body('250000') }, { id: 'aw', title: 'B', media: 'AirWork', body: body('280000') }]);
  expect(result.fullDifferent).toBe(true);
  expect(result.rows.filter(row => row.different).map(row => row.key)).toEqual(['基本給与 最小']);
  expect(result.rows.map(row => row.key)).not.toContain('求人id');
  expect(compareJobs([{ id: '1', title: 'A', media: 'AirWork', body: '月給25万円で土日休みです' }, { id: '2', title: 'B', media: 'HRハッカー', body: null }]).rows).toEqual([]);
});
it.each(['250000 ', ' 250000', '　２５０，０００　', '250,000\t'])('formats salary %j consistently with numeric sorting', value => {
  expect(salaryAmount('月給', value)).toBe('月給 250,000円');
});
it('highlights differing fields and collapses identical ones without identifiers', () => {
  const { container } = render(<ParallelJobs jobs={[{ id: '1', title: 'A', media: 'HRハッカー', body: body('250000') }, { id: '2', title: 'B', media: 'AirWork', body: body('280000') }]} />);
  expect(container.querySelectorAll('.jc-comparison-different')).toHaveLength(1);
  expect(container.querySelector('.jc-comparison-different')?.textContent).toContain('月給 250,000円');
  expect(container.querySelector('.jc-comparison-different')?.textContent).toContain('月給 280,000円');
  expect(container.querySelector('details')?.open).toBe(false);
  expect(container.textContent).not.toContain('raw-id-secret');
  expect(container.textContent).not.toContain('基本給与 最小');
  fireEvent.click(screen.getByText('同じ項目（5項目）'));
});
it.each([' ', '　', '\t', '\n', '\r\n'])('preserves trailing whitespace %j in full-body and field comparison', suffix => {
  const jobs = [{ id: '1', title: 'A', media: 'HRハッカー', body: '仕事内容：配送' }, { id: '2', title: 'B', media: 'AirWork', body: `仕事内容：配送${suffix}` }];
  const result = compareJobs(jobs);
  expect(result.fullDifferent).toBe(true);
  expect(result.rows).toEqual([{ key: '仕事内容', label: '仕事内容', values: ['配送', `配送${suffix}`], different: true }]);
  const { container } = render(<ParallelJobs jobs={jobs} />);
  expect(screen.getByText('違う項目（1項目）')).toBeTruthy();
  expect(screen.queryByText(/取得した本文は同じ/)).toBeNull();
  expect(container.querySelectorAll('.jc-comparison-different')).toHaveLength(1);
});
it('does not discard a whitespace-only field value', () => {
  expect(compareJobs([{ id: '1', title: 'A', media: 'HRハッカー', body: '仕事内容：' }, { id: '2', title: 'B', media: 'AirWork', body: '仕事内容： ' }]).rows).toEqual([{ key: '仕事内容', label: '仕事内容', values: ['', ' '], different: true }]);
});
it('searches cached deal candidates, combines existing filters and opens the table', async () => {
  api.mockResolvedValue({ ok: true, data: page }); const show = vi.fn(); render(<HubSpotListingsPanel onOpen={vi.fn()} onShowDeal={show} />);
  await screen.findByRole('button', { name: '配送の仕事の本文を見る' });
  fireEvent.click(screen.getByText('絞り込み・並び順'));
  fireEvent.change(screen.getByLabelText('取引名を検索'), { target: { value: '合成配送' } });
  await waitFor(() => { expect(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0])).toContain('deal_search='); });
  expect(screen.getByRole('option', { name: '合成配送の契約（101件）' })).toBeTruthy();
  fireEvent.change(screen.getByLabelText('取引', { exact: true }), { target: { value: '901' } });
  fireEvent.change(screen.getByLabelText('都道府県'), { target: { value: '沖縄県' } });
  fireEvent.change(screen.getByLabelText('職種の分類'), { target: { value: 'ドライバー' } });
  await waitFor(() => { const query = new URL(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0]), 'http://test').searchParams; expect(query.get('deal')).toBe('901'); expect(query.get('prefecture')).toBe('沖縄県'); expect(query.get('title')).toBe('ドライバー'); });
  fireEvent.click(screen.getByRole('button', { name: 'この取引の求人' }));
  expect(show).toHaveBeenCalledWith({ id: '901', name: '合成配送の契約', prefecture: '沖縄県', title: 'ドライバー', media: '' });
  expect(screen.queryByText('01', { exact: true })).toBeNull();
  expect(screen.getByText('不明', { selector: '.jc-publication' })).toBeTruthy();
  fireEvent.change(screen.getByLabelText('取引', { exact: true }), { target: { value: 'none' } });
  await waitFor(() => { expect(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0])).toContain('deal=none'); });
});
it('sorts on the server, pages 101 jobs and compares distinct media jobs', async () => {
  api.mockImplementation((url: string) => Promise.resolve({ ok: true, data: url.includes('listing-comparison') ? { jobs: [{ listing: row, body: body('250000') }, { listing: { ...row, id: '2', media: 'airwork' }, body: body('280000') }] } : url.includes('/versions') ? { listing: row, versions: [], history_counts: {}, history_may_be_incomplete: false } : page }));
  const open = vi.fn(); render(<DealComparison deal={{ id: '901', name: '合成配送', prefecture: '沖縄県', title: '', media: '' }} onOpen={open} onClose={vi.fn()} />);
  const table = await screen.findByRole('table', { name: '取引の求人一覧表' });
  expect(within(table).getAllByText('月給 250,000円〜300,000円')).toHaveLength(2);
  expect(within(table).getAllByText('12件')).toHaveLength(2);
  fireEvent.click(within(table).getByRole('button', { name: '給与' }));
  await waitFor(() => { expect(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0])).toContain('sort=salary_min'); });
  fireEvent.click(screen.getByRole('button', { name: '次の50件' }));
  await waitFor(() => { expect(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0])).toContain('offset=50'); });
  fireEvent.click(within(table).getByLabelText('配送の仕事を比較に選ぶ'));
  expect(screen.getByRole('button', { name: '横に並べて比べる' }).hasAttribute('disabled')).toBe(true);
  fireEvent.click(within(table).getByLabelText('配送の仕事・別媒体を比較に選ぶ'));
  fireEvent.click(screen.getByRole('button', { name: '横に並べて比べる' }));
  await screen.findByRole('region', { name: '選んだ求人の横並び比較' });
  expect(String(api.mock.calls.find(call => String(call[0]).includes('listing-comparison'))?.[0])).toContain('ids=1,2');
  fireEvent.click(within(table).getByRole('button', { name: '配送の仕事' }));
  await waitFor(() => { expect(open).toHaveBeenCalledOnce(); });
});
it('limits selection to five jobs and keeps unknown counts separate from zero', async () => {
  api.mockResolvedValue({ ok: true, data: { ...page, listings: Array.from({ length: 6 }, (_, index) => ({ ...row, id: String(index + 1), title: `比較求人${String(index + 1)}`, application_count: index === 0 ? 0 : null })) } });
  render(<DealComparison deal={{ id: '901', name: '合成配送', prefecture: '', title: '', media: '' }} onOpen={vi.fn()} onClose={vi.fn()} />);
  const table = await screen.findByRole('table', { name: '取引の求人一覧表' });
  expect(within(table).getByText('0件')).toBeTruthy();
  expect(within(table).getAllByText('未取得')).toHaveLength(5);
  for (let index = 1; index <= 5; index++) fireEvent.click(within(table).getByLabelText(`比較求人${String(index)}を比較に選ぶ`));
  expect(within(table).getByLabelText<HTMLInputElement>('比較求人6を比較に選ぶ').disabled).toBe(true);
  fireEvent.click(within(table).getByLabelText('比較求人1を比較に選ぶ'));
  expect(within(table).getByLabelText<HTMLInputElement>('比較求人6を比較に選ぶ').disabled).toBe(false);
});
it('renders real-shaped AirWork fields beside HRハッカー with bounded cells and expanded highlights', () => {
  const description = '梱包済みの商品を車へ積み込みます。\n'.repeat(12);
  const { container } = render(<ParallelJobs jobs={[
    { id: '1', title: '架空の配送スタッフ', media: 'HRハッカー', body: `仕事内容：${description}\n給与形態：月給\n基本給与 最小：213000\n基本給与 最大：249000` },
    { id: '2', title: '架空の運搬スタッフ', media: 'AirWork', body: '配送先へ荷物を届けます。', fields: { 仕事内容: '配送先へ荷物を届けます。', 給与形態: '月給', 勤務地: '東京都江東区', 勤務時間: '8:00〜17:00' } },
  ]} />);
  expect(container.textContent).toContain('配送先へ荷物を届けます。');
  expect(container.textContent).toContain('東京都江東区');
  expect(container.textContent).toContain('8:00〜17:00');
  expect(container.textContent).toContain('月給 213,000円');
  const locationRow = [...container.querySelectorAll('tr')].find(tr => tr.querySelector('th')?.textContent === '勤務地');
  expect(locationRow?.textContent).toContain('未取得');
  expect(locationRow?.querySelector('mark')).toBeNull();
  const button = screen.getByRole('button', { name: '全文を見る' });
  expect(button.getAttribute('aria-expanded')).toBe('false');
  expect(container.querySelector('.jc-comparison-preview mark')).toBeTruthy();
  fireEvent.click(button);
  expect(container.querySelector('.jc-comparison-full')?.textContent).toBe(description);
  expect(container.querySelector('.jc-comparison-full mark')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '閉じる' }));
  expect(container.querySelector('.jc-comparison-full')).toBeNull();
});
it('reads only displayed rows for dates, preserves unknowns and drops a previous page response', async () => {
  let oldResolve: ((value: unknown) => void) | undefined;
  const first = { ...page, listings: [{ ...row, summary: { ...row.summary, body_changed_at: null } }] };
  const second = { ...page, listings: [{ ...row, id: '51', title: '次のページの求人', summary: { ...row.summary, body_changed_at: null } }] };
  api.mockImplementation((url: string) => {
    if (url.includes('listing-body-dates?ids=1')) return new Promise(resolve => { oldResolve = resolve; });
    if (url.includes('listing-body-dates?ids=51')) return Promise.resolve({ ok: true, data: { dates: { '51': '2026-10-09T00:00:00Z' } } });
    return Promise.resolve({ ok: true, data: url.includes('offset=50') ? second : first });
  });
  render(<DealComparison deal={{ id: '901', name: '架空の契約', prefecture: '', title: '', media: '' }} onOpen={vi.fn()} onClose={vi.fn()} />);
  const table = await screen.findByRole('table', { name: '取引の求人一覧表' });
  await waitFor(() => { expect(oldResolve).toBeTruthy(); });
  expect(table.querySelector('[data-column="body_changed_at"]')?.textContent).toBe('本文の最終変更日');
  expect(table.querySelector('tbody [data-column="body_changed_at"]')?.textContent).toBe('未取得');
  fireEvent.click(screen.getByRole('button', { name: '次の50件' }));
  await waitFor(() => { expect(table.querySelector('tbody [data-column="body_changed_at"]')?.textContent).toContain('2026/10/09'); });
  oldResolve?.({ ok: true, data: { dates: { '1': '2026-01-01T00:00:00Z' } } });
  await waitFor(() => { expect(table.textContent).toContain('次のページの求人'); });
  expect(table.textContent).not.toContain('2026/01/01');
  expect(api.mock.calls.filter(call => String(call[0]).includes('listing-body-dates')).map(call => String(call[0]).split('ids=')[1])).toEqual(['1', '1', '51']);
});

it.each([['検品', '梱包'], ['検'.repeat(600), '梱'.repeat(600)]])('colours whole field replacements in the deal comparison for %s', (before, after) => {
  const { container } = render(<ParallelJobs jobs={[
    { id: '1', title: '検品の求人', media: 'HRハッカー', body: `仕事内容：${before}` },
    { id: '2', title: '梱包の求人', media: 'AirWork', body: `仕事内容：${after}` },
  ]} />);
  const marks = container.querySelectorAll('mark');
  expect(marks).toHaveLength(2);
  expect(marks[0]?.textContent).toBe(before);
  expect(marks[1]?.textContent).toBe(after);
});
