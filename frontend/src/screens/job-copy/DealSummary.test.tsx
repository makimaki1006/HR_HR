// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealSummary } from './DealSummary';
import type { HubSpotListing } from './hubspotListings';
import { addDays, todayJst } from './timelineModel';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: ({ testId }: { testId: string }) => <div data-testid={testId} /> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const row = (id: string, title: string, status: string): HubSpotListing => ({ id, media: 'hrh', media_job_id: `m${id}`, account_id: null, title, prefecture: null, municipality: null, category: null, publication_status: status, last_csv_detected_at: null, application_count: null });

it('案件の合計: 20/9/10/0 → 39件、前の4週31件、課金6万円 → 1件あたり1,538円', async () => {
  const day = addDays(todayJst(), -5); const before = addDays(todayJst(), -40);
  const rows = [row('1', '求人A', '公開'), row('2', '求人B', '公開'), row('3', '求人C', '公開'), row('4', '求人D', '公開終了')];
  api.mockImplementation((url: string) => {
    if (url.includes('deal-applications')) return Promise.resolve({ ok: true, data: { shared_applications: 0, listings: [
      { listing_id: '1', by_date: { [day]: 20, [before]: 10 } }, { listing_id: '2', by_date: { [day]: 9, [before]: 11 } },
      { listing_id: '3', by_date: { [day]: 10, [before]: 10 } }, { listing_id: '4', by_date: {} }] } });
    if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {} } });
    return Promise.resolve({ ok: true, data: { status: 'ready', total: 4, listings: rows, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
  });
  const start = addDays(todayJst(), -27); const end = todayJst();
  const csv = (amountYen: number) => [{ source: 'csv' as const, start, end, amountYen }];
  render(<DealSummary deal={{ id: '9', name: '案件', prefecture: '', title: '', media: '' }} billingByJob={{ 'hubspot-history-1': csv(30000), 'hubspot-history-2': csv(15000), 'hubspot-history-3': csv(15000), 'hubspot-history-4': [{ source: 'csv' as const, start: '2026-01-01', end: '2026-01-31', amountYen: 5000 }] }} onOpenListing={vi.fn()} />);
  const band = await screen.findByRole('group', { name: '案件の合計' });
  await waitFor(() => { expect(band.textContent).toContain('39件'); });
  expect(band.textContent).toContain('前の4週 31件');
  expect(band.textContent).toContain('1,538円');
  expect(band.textContent).toContain('掲載中 3 ・ 終了 1');
  expect(screen.queryByText('市場の職種')).toBeNull();
});
