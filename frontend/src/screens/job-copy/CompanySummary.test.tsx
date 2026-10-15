// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { CompanySummary } from './CompanySummary';
import { DealSummary } from './DealSummary';
import type { HubSpotListing } from './hubspotListings';
import { addDays, todayJst } from './timelineModel';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: ({ testId }: { testId: string }) => <div data-testid={testId} /> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const row = (id: string, media: 'hrh' | 'airwork', deals: string[]): HubSpotListing => ({ id, media, media_job_id: `100000${id}`, account_id: null, title: `求人${id}`, prefecture: null, municipality: null, category: null, publication_status: '公開', last_csv_detected_at: null, application_count: null, deal_ids: deals });
const listings: Record<string, HubSpotListing[]> = { d1: [row('1', 'hrh', ['d1', 'd2']), row('2', 'airwork', ['d1'])], d2: [row('1', 'hrh', ['d1', 'd2']), row('3', 'hrh', ['d2'])] };
const fetchedAt = new Date().toISOString();
const perf = (id: string, weekly: number) => {
  const monday = (offset: number) => addDays(todayJst(), offset);
  return { schema_version: 1, source: 'hrhacker', job_id: `100000${id}`, captured_at: fetchedAt, rows: [0, 1, 2, 3, 4].map(i => ({ period_start: monday(-7 * (i + 1) - 1), period_end: monday(-7 * i - 2), impressions: 10, clicks: 1, cost_yen: weekly, applications: 1 })).reverse() };
};
const day = addDays(todayJst(), -3);
function mockApi() {
  api.mockImplementation((url: string) => {
    if (url.includes('company-deals')) return Promise.resolve({ ok: true, data: { company: { id: 'c', name: '会社' }, deals: [{ id: 'd1', name: '取引1', current: false }, { id: 'd2', name: '取引2', current: false }] } });
    if (url.includes('deal-applications')) {
      const deal = /deal=(d\d)/.exec(url)?.[1] ?? '';
      return Promise.resolve({ ok: true, data: { shared_applications: 0, company: { id: 'c', name: '会社' }, listings: (listings[deal] ?? []).map(l => ({ listing_id: l.id, by_date: { [day]: l.id === '1' ? 10 : l.id === '2' ? 90 : 4 }, fetched_at: fetchedAt, indeed_weekly_perf: l.media === 'hrh' ? perf(l.id, 7000) : null })) } });
    }
    if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {} } });
    const deal = new URL(url, 'http://x').searchParams.get('deal') ?? '';
    return Promise.resolve({ ok: true, data: { status: 'ready', total: 2, listings: listings[deal] ?? [], titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
  });
}

it('事業所: 求人は 1 回だけ数える (10 + 90 + 4 = 104件)、取引ごとの見出しが出る、AirWork は課金データなし', async () => {
  mockApi();
  render(<CompanySummary company={{ id: 'c', name: '会社' }} billingByJob={{}} onOpenListing={vi.fn()} />);
  const band = await screen.findByRole('group', { name: '事業所の合計' });
  await waitFor(() => { expect(band.textContent).toContain('104件'); });
  expect(band.textContent).toContain('求人 3 件 ・ 取引 2 件');
  expect(band.textContent).toContain('課金データのない求人 1件（AirWork）は含みません');
  // 顧客の全求人は 1 か所の表に、同じ求人は 1 回だけ出る（求人1 は取引1・取引2 の両方に付いている）。
  expect(screen.getAllByRole('button', { name: /^求人\d$/ }).map(b => b.textContent)).toEqual(['求人1', '求人2', '求人3']);
  expect(screen.getByText('取引：取引1、取引2')).toBeTruthy();
  expect(screen.getByText('取引：取引1')).toBeTruthy();
  expect(screen.getByText('取引：取引2')).toBeTruthy();
  expect(screen.getAllByText('課金データなし').length).toBeGreaterThan(0);
});

it('取引: HubSpot の週ごとの実績から課金が出て、事業所へのリンクがある', async () => {
  mockApi();
  const open = vi.fn();
  render(<DealSummary deal={{ id: 'd1', name: '取引1', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} onOpenCompany={open} />);
  const band = await screen.findByRole('group', { name: '案件の合計' });
  await waitFor(() => { expect(band.textContent).toContain('100件'); });
  // 課金のある求人(1)だけの応募 10 件で割る。課金は 1 日 1,000 円。
  expect(band.textContent).toMatch(/\d万\d,\d{3}円|\d,\d{3}円/);
  expect(band.textContent).not.toContain('課金が分からない');
  screen.getByRole('button', { name: '会社の合計を見る' }).click();
  expect(open).toHaveBeenCalledWith({ id: 'c', name: '会社' });
});
