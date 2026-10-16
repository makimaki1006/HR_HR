// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CompanyListings } from './CompanyListings';
import type { CompanyDeal } from './companyDeals';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('./dealSummaryLoad', async original => ({ ...await original<typeof import('./dealSummaryLoad')>(), loadCompanyDatas: () => Promise.resolve({ status: 'ready', datas: [], notes: [] }) }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
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
