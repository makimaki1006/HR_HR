// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CompanyListings } from './CompanyListings';
import type { CompanyDeal } from './companyDeals';
import type { DealData } from './dealSummaryLoad';

const api = vi.hoisted(() => vi.fn());
const load = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('./dealSummaryLoad', async original => ({ ...await original<typeof import('./dealSummaryLoad')>(), loadCompanyDatas: load }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const props = { companyId: '100', billingByJob: {}, onOpenDeal: vi.fn(), onOpenListing: vi.fn() };
const row = { id: '7', title: '合成求人', media: 'hrh', publication_status: '掲載中', deal_ids: ['10'] };

it('応募・課金が読めなかったら「読み込み中」で止まらず、押すと読み直して数字を出す', async () => {
  api.mockImplementation((url: string) => Promise.resolve(url.startsWith('/api/job-copy/listing-flags')
    ? { ok: true, data: { status: 'ready', flags: { '7': ['more'] }, cards: [] } }
    : { ok: true, data: { status: 'ready', listings: [row], next_offset: null } }));
  load.mockResolvedValueOnce({ status: 'error', message: '' });
  const data: DealData = {
    id: '10', name: '合成案件', rows: [row] as DealData['rows'], capped: false,
    applications: new Map([['7', { '2026-10-30': 4 }]]), shared: 0, overlap: {}, fetched: new Map([['7', '2026-11-01']]),
    bodyDates: {}, perf: new Map(), company: null,
  };
  load.mockResolvedValueOnce({ status: 'ready', datas: [data], notes: [] });
  render(<CompanyListings {...props} deals={[{ id: '10', name: '合成案件' } as CompanyDeal]} />);
  await screen.findByRole('button', { name: '合成求人' });
  await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('応募・課金を読み込めませんでした'); });
  expect(screen.queryByText('応募・課金を読み込んでいます…')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'もう一度読み込む' }));
  await waitFor(() => { expect(screen.getByText('0 → 4 件')).toBeTruthy(); });
  expect(load).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole('alert')).toBeNull();
});
