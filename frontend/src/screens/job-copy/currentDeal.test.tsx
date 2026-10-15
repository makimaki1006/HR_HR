// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { DealSummary } from './DealSummary';
import { clearCompanyDealsCache } from './companyDeals';
import type { HubSpotListing } from './hubspotListings';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: ({ testId }: { testId: string }) => <div data-testid={testId} /> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?status=all'); clearCompanyDealsCache(); });
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const row = (id: string, deals: string[], status: string): HubSpotListing => ({ id, media: 'hrh', media_job_id: `m${id}`, account_id: null, title: `求人${id}`, prefecture: null, municipality: null, category: null, publication_status: status, last_csv_detected_at: null, application_count: null, deal_ids: deals });
// 今の取引 d1 には求人 1 と 2、過去の取引 d2（契約終了）には求人 2 と 3。求人 3 は過去の取引にだけ結び付いている。
const byDeal: Record<string, HubSpotListing[]> = { d1: [row('1', ['d1'], '公開中'), row('2', ['d1', 'd2'], '公開中')], d2: [row('2', ['d1', 'd2'], '公開中'), row('3', ['d2'], '公開終了')] };
const dealRow = (id: string, name: string, current: boolean, active: boolean) => ({ id, name, current, active, pipeline_id: '21596025', pipeline_label: 'リクロジ_納品管理', stage_label: active ? '契約中' : '契約終了', created_at: current ? '2026-09-01' : '2025-01-01', contract_start: null, contract_end: null, listings_total: 2, listings_live: 2, recent_apps: 0, recent_cost: null });
function mock(currentIsD1: boolean) {
  api.mockImplementation((url: string) => {
    if (url.includes('company-deals')) return Promise.resolve({ ok: true, data: { company: { id: 'c', name: '会社' }, deals: [dealRow('d1', '今の取引', currentIsD1, true), dealRow('d2', '過去の取引', !currentIsD1, false)] } });
    if (url.includes('deal-applications')) { const deal = /deal=(d\d)/.exec(url)?.[1] ?? ''; return Promise.resolve({ ok: true, data: { shared_applications: 0, company: { id: 'c', name: '会社' }, listings: (byDeal[deal] ?? []).map(l => ({ listing_id: l.id, by_date: {} })) } }); }
    if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {} } });
    const deal = new URL(url, 'http://x').searchParams.get('deal') ?? '';
    return Promise.resolve({ ok: true, data: { status: 'ready', total: 2, listings: byDeal[deal] ?? [], titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
  });
}
const titles = () => screen.queryAllByRole('button', { name: /^求人\d$/ }).map(b => b.textContent);

it('今の取引の画面には、過去の取引にだけ結び付いた求人も出て、同じ求人は 1 回だけ数える', async () => {
  mock(true);
  render(<DealSummary deal={{ id: 'd1', name: '今の取引', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
  await waitFor(() => { expect(titles()).toEqual(['求人1', '求人2', '求人3']); });
  const band = screen.getByRole('group', { name: '案件の合計' });
  // 求人 2 は d1・d2 の両方に付いているが 1 件。求人は 3 件（掲載中 2・終了 1）、取引は 2 件。
  expect(band.textContent).toContain('求人 3 件 ・ 取引 2 件');
  expect(band.textContent).toContain('掲載中 2 ・ 終了 1');
  expect(screen.getByText('この顧客の全部の求人')).toBeTruthy();
  expect(screen.getByText('取引：今の取引、過去の取引')).toBeTruthy();
  // 取引の軸（契約中か）は、求人の軸（掲載中か）とは別に出る。
  await waitFor(() => { expect(document.querySelector('.ds-contract')?.textContent).toContain('取引の状態：契約中（契約中） ・ この顧客の今の取引 ・ 作成 2026-09-01'); });
});

it('今の取引でない古い取引の画面は、その取引の求人だけを出す', async () => {
  mock(true);
  render(<DealSummary deal={{ id: 'd2', name: '過去の取引', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
  await waitFor(() => { expect(titles()).toEqual(['求人2', '求人3']); });
  await new Promise(resolve => setTimeout(resolve, 50));
  expect(titles()).toEqual(['求人2', '求人3']);
  expect(screen.queryByText('この顧客の全部の求人')).toBeNull();
  expect(document.querySelector('.ds-contract')?.textContent).toContain('取引の状態：契約終了（契約終了） ・ 過去の取引');
});

it('契約中の取引が無い顧客は、最新の取引（current）の画面が顧客の全求人を出す', async () => {
  mock(false); // d2 が current（契約中が無いので最新の取引）
  render(<DealSummary deal={{ id: 'd2', name: '過去の取引', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
  await waitFor(() => { expect(titles()).toEqual(['求人1', '求人2', '求人3']); });
});
