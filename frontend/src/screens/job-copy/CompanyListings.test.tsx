// @vitest-environment happy-dom
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
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
