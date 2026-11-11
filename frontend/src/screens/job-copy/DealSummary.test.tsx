// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealSummary } from './DealSummary';
import type { HubSpotListing } from './hubspotListings';
import { addDays, todayJst } from './timelineModel';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
const charts = vi.hoisted(() => new Map<string, unknown>());
vi.mock('../../components/EChart', () => ({ EChart: ({ testId, option }: { testId: string; option: unknown }) => { charts.set(testId, option); return <div data-testid={testId} />; } }));
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

it('右の欄に職種の指定状況・地域・一括指定へのリンク、表の「未指定」に印、出来事は番号付きで押すと求人を開く', async () => {
  // 本文の日付の読み取りは同じ求人の組を15秒まとめるので、上のテストと違う番号にする。
  const rows: HubSpotListing[] = [
    { ...row('11', '求人A', '公開'), prefecture: '東京都', municipality: '新宿区', indeed_title_state: 'set', indeed_title: '介護職' },
    { ...row('12', '求人B', '公開'), prefecture: '東京都', municipality: '渋谷区', indeed_title_state: 'unset' },
    { ...row('13', '求人C', '公開'), prefecture: '東京都', municipality: '新宿区', indeed_title_state: 'none' },
    { ...row('14', '求人D', '公開終了'), prefecture: '神奈川県', municipality: '横浜市', indeed_title_state: 'unset' },
  ];
  const day = addDays(todayJst(), -10);
  api.mockImplementation((url: string) => {
    if (url.includes('deal-applications')) return Promise.resolve({ ok: true, data: { shared_applications: 0, listings: rows.map(item => ({ listing_id: item.id, by_date: { [day]: 1 } })) } });
    if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {}, changes: {
      '12': { body: [`${addDays(todayJst(), -30)}T00:00:00Z`, `${day}T00:00:00Z`], images: [], status: {}, started_on: null },
    } } });
    return Promise.resolve({ ok: true, data: { status: 'ready', total: 4, listings: rows, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
  });
  const open = vi.fn();
  render(<DealSummary deal={{ id: '9', name: '案件', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={open} />);
  const rail = await screen.findByRole('complementary', { name: '数字の前提' });
  await waitFor(() => { expect(rail.textContent).toContain('4件中2件が指定済み'); });
  expect(rail.textContent).toContain('うち「市場と比べない」1件');
  expect(rail.textContent).toContain('東京都 新宿区・渋谷区、神奈川県 横浜市');
  // 未指定の 2 件には掲載を終えた求人D も入る（表は既定で掲載中だけ）
  const link = screen.getByRole('link', { name: '未指定の 2 件（うち掲載終了 1 件）を指定する' });
  expect(link.getAttribute('href')).toBe('/app/job-copy?view=occupations&deal=9');
  // 表は既定で掲載中だけ。掲載中で未指定の求人B に印が付く。
  const unset = document.querySelectorAll('.ds-table .ds-unset');
  expect(unset).toHaveLength(1);
  expect(unset[0]?.closest('tr')?.textContent).toContain('求人B');
  // 求人B の本文の変更 2 回（最後の 1 回だけでなく全部）
  const latest = screen.getByRole('button', { name: '2. 求人B：本文を変えました（この求人の画面を開く）' });
  expect(screen.getByRole('button', { name: '1. 求人B：本文を変えました（この求人の画面を開く）' })).toBeTruthy();
  latest.click();
  expect(open).toHaveBeenCalledTimes(1);
  expect((open.mock.calls[0]?.[0] as HubSpotListing).id).toBe('12');
  // グラフの番号の線は、応募の系列とは別の系列に付く（求人の系列を隠しても消えない）
  const option = charts.get('ds-applications') as { series: { name: string; markLine?: { data: { label: { formatter: string } }[] } }[] };
  const marked = option.series.filter(item => item.markLine);
  expect(marked.map(item => item.name)).toEqual(['出来事の番号']);
  expect(marked[0]?.markLine?.data.map(item => item.label.formatter).sort()).toEqual(['1', '2']);
  expect(screen.queryByText('出来事は先頭30件の求人の分です。')).toBeNull();
});

it('求人が31件以上の案件では、出来事は先頭30件の求人の分だと断る', async () => {
  const rows = Array.from({ length: 31 }, (_, i) => row(String(200 + i), `求人${String(i)}`, '公開'));
  api.mockImplementation((url: string) => {
    if (url.includes('deal-applications')) return Promise.resolve({ ok: true, data: { shared_applications: 0, listings: rows.slice(0, 30).map(item => ({ listing_id: item.id, by_date: {} })) } });
    if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {}, changes: Object.fromEntries(rows.map(item => [item.id, { body: ['2026-10-01T00:00:00Z'], images: [], status: {}, started_on: null }])) } });
    return Promise.resolve({ ok: true, data: { status: 'ready', total: 31, listings: rows, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
  });
  render(<DealSummary deal={{ id: '8', name: '案件', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
  expect(await screen.findByText('出来事は先頭30件の求人の分です。')).toBeTruthy();
  // 31件目（求人30）の本文の変更は出さない。出来事は30件
  expect(screen.getByRole('button', { name: 'すべて表示（30件）' })).toBeTruthy();
});
