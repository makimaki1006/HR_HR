// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CompanyListings } from './CompanyListings';
import { updateJobCopyQuery } from './navigation';
import type { CompanyDeal } from './companyDeals';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('./dealSummaryLoad', async original => ({ ...await original<typeof import('./dealSummaryLoad')>(), loadCompanyDatas: () => Promise.resolve({ status: 'ready', datas: [], notes: [] }) }));
afterEach(() => { cleanup(); vi.resetAllMocks(); window.history.replaceState(null, '', '/app/job-copy'); });
const props = { companyId: '100', billingByJob: {}, onOpenDeal: vi.fn(), onOpenListing: vi.fn() };
it('求人ゼロの案件を除き、21案件は20+1に分けて索引を読む', async () => {
  const deals = Array.from({ length: 21 }, (_, i) => ({ id: String(i + 1), listings_total: 1 })) as CompanyDeal[];
  deals.push({ id: '99', listings_total: 0 } as CompanyDeal);
  api.mockResolvedValue({ ok: true, data: { status: 'ready', listings: [], next_offset: null } });
  render(<CompanyListings {...props} deals={deals} />);
  await screen.findByRole('heading', { name: '掲載中 0件' });
  expect(api.mock.calls.map(call => new URL(call[0] as string, 'http://localhost').searchParams.get('deal'))).toEqual([
    Array.from({ length: 20 }, (_, i) => String(i + 1)).join(','), '21',
  ]);
  expect(screen.getByText('過去（掲載終了） 0件')).toBeTruthy();
});
it('求人がない顧客は索引を呼ばず、空の区分を表示する', async () => {
  render(<CompanyListings {...props} deals={[]} />);
  await screen.findByRole('heading', { name: '掲載中 0件' });
  expect(api).not.toHaveBeenCalled();
});
it('索引が準備中なら求人0件と誤表示しない', async () => {
  api.mockResolvedValue({ ok: true, data: { status: 'preparing', listings: [], next_offset: null } });
  render(<CompanyListings {...props} deals={[{ id: '1' } as CompanyDeal]} />);
  await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('求人を読み込めませんでした'); });
  expect(screen.queryByRole('heading', { name: '掲載中 0件' })).toBeNull();
});

it('コンサル担当から取引担当・全員への切り替えを掲載中と過去の求人取得にも引き継ぐ', async () => {
  window.history.replaceState(null, '', '/app/job-copy?owner_field=consultant&owner=11');
  api.mockImplementation((url: string) => {
    const params = new URL(url, 'http://localhost').searchParams;
    const person = params.get('owner') ?? 'me';
    return Promise.resolve({ ok: true, data: { status: 'ready', next_offset: null, listings: [
      { id: '1', title: `掲載中・${person}`, publication_status: '掲載中', deal_ids: ['10'], media: 'hrh' },
      { id: '2', title: `過去・${person}`, publication_status: '掲載終了', deal_ids: ['10'], media: 'hrh' },
    ] } });
  });
  render(<CompanyListings {...props} deals={[{ id: '10', name: '合成案件' } as CompanyDeal]} />);
  await screen.findByRole('button', { name: '掲載中・11' });
  expect(screen.getByRole('button', { name: '過去・11', hidden: true })).toBeTruthy();
  expect(api.mock.calls[0]?.[0]).toContain('owner_field=consultant&owner=11');
  act(() => { updateJobCopyQuery({ owner_field: 'hubspot_owner_id', owner: '22' }, 'replace'); });
  await screen.findByRole('button', { name: '掲載中・22' });
  expect(screen.queryByRole('button', { name: '掲載中・11' })).toBeNull();
  expect(screen.getByRole('button', { name: '過去・22', hidden: true })).toBeTruthy();
  expect(api.mock.calls.at(-1)?.[0]).toContain('owner_field=hubspot_owner_id&owner=22');
  act(() => { updateJobCopyQuery({ owner: 'all' }, 'replace'); });
  await screen.findByRole('button', { name: '掲載中・all' });
  expect(screen.getByRole('button', { name: '過去・all', hidden: true })).toBeTruthy();
});

it('掲載中の求人だけをホームと同じ分類で読み、分類を先に並べ、並べ替えを選べる', async () => {
  api.mockImplementation((url: string) => {
    if (url.startsWith('/api/job-copy/listing-flags')) return Promise.resolve({ ok: true, data: { status: 'ready', flags: { '1': ['more'], '2': ['to_zero', 'costly'] }, cards: [
      { key: 'to_zero', title: '応募がゼロになった求人', rule: '掲載中で、前の4週に応募が1件以上あり、直近4週は0件になった求人。' },
      { key: 'costly', title: '課金が高いまま応募が少ない求人', rule: '課金の基準' },
    ] } });
    return Promise.resolve({ ok: true, data: { status: 'ready', next_offset: null, listings: [
      { id: '1', title: '合成求人・新しい', publication_status: '掲載中', deal_ids: ['10'], media: 'hrh', summary: { body_changed_at: '2026-10-20T00:00:00Z' } },
      { id: '2', title: '合成求人・古い', publication_status: '掲載中', deal_ids: ['10'], media: 'hrh', summary: { body_changed_at: '2026-10-01T00:00:00Z' } },
      { id: '3', title: '合成求人・終了', publication_status: '掲載終了', deal_ids: ['10'], media: 'hrh' },
    ] } });
  });
  render(<CompanyListings {...props} deals={[{ id: '10', name: '合成案件' } as CompanyDeal]} />);
  await screen.findByText('応募がゼロになった 1件');
  const flagsUrl = new URL(api.mock.calls.find(call => (call[0] as string).startsWith('/api/job-copy/listing-flags'))?.[0] as string, 'http://localhost');
  expect(flagsUrl.searchParams.get('ids')).toBe('1,2');
  expect(flagsUrl.searchParams.get('owner')).toBe('me');
  expect(screen.getByText('課金が高いまま応募が少ない 1件')).toBeTruthy();
  const liveTable = screen.getByRole('table', { name: '掲載中の求人' });
  const titles = () => [...liveTable.querySelectorAll('tbody th > button.jh-link')].map(button => button.textContent);
  expect(titles()).toEqual(['合成求人・古い', '合成求人・新しい']);
  const zeroChip = [...liveTable.querySelectorAll('.jh-change-chip')].find(chip => chip.textContent === '応募がゼロになった');
  expect(zeroChip?.getAttribute('title')).toBe('掲載中で、前の4週に応募が1件以上あり、直近4週は0件になった求人。');
  fireEvent.change(screen.getByRole('combobox', { name: '並べ替え' }), { target: { value: 'changed' } });
  expect(titles()).toEqual(['合成求人・新しい', '合成求人・古い']);
});

it('分類が読めなくても求人の表は出し、読めなかったことを伝える', async () => {
  api.mockImplementation((url: string) => Promise.resolve(url.startsWith('/api/job-copy/listing-flags')
    ? { ok: false, status: 503 }
    : { ok: true, data: { status: 'ready', next_offset: null, listings: [{ id: '1', title: '合成求人', publication_status: '掲載中', deal_ids: ['10'], media: 'hrh' }] } }));
  render(<CompanyListings {...props} deals={[{ id: '10', name: '合成案件' } as CompanyDeal]} />);
  await waitFor(() => { expect(screen.getByRole('alert').textContent).toBe('分類を読み込めませんでした。求人の表はそのまま使えます。'); });
  expect(screen.getByRole('button', { name: '合成求人' })).toBeTruthy();
});
