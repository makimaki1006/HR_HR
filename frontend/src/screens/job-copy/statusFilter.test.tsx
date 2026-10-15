// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DealSummary } from './DealSummary';
import { HubSpotListingsPanel } from './HubSpotListingsPanel';
import type { HubSpotListing } from './hubspotListings';
import { countStatuses, countsFromServer, matchesStatus, parseStatus, statusOf, withStatus } from './statusFilter';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: ({ testId }: { testId: string }) => <div data-testid={testId} /> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); });
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const row = (id: string, title: string, status: string | null): HubSpotListing => ({ id, media: 'hrh', media_job_id: `m${id}`, account_id: null, title, prefecture: null, municipality: null, category: null, publication_status: status, last_csv_detected_at: null, application_count: null });

describe('掲載状況の絞り込みの部品', () => {
  it('URL の status は既定が掲載中で、不正な値は掲載中に戻る', () => {
    expect(parseStatus('')).toBe('live');
    expect(parseStatus('?status=ended')).toBe('ended');
    expect(parseStatus('?status=all&company=1')).toBe('all');
    expect(parseStatus('?status=xyz')).toBe('live');
    expect(withStatus('?company=1', 'ended')).toBe('?company=1&status=ended');
    expect(withStatus('?company=1&status=ended', 'live')).toBe('?company=1');
    expect(withStatus('', 'all')).toBe('?status=all');
  });
  it('公開中は掲載中、公開終了は掲載終了、公開前は掲載前、空は状態不明として数える', () => {
    expect(['公開中', '公開', '掲載中', '公開終了', '非公開', '公開前', '公開開始前', null, '', '不明'].map(statusOf)).toEqual(['live', 'live', 'live', 'ended', 'ended', 'before', 'before', 'unknown', 'unknown', 'unknown']);
    const rows = [row('1', 'a', '公開中'), row('2', 'b', '公開中'), row('3', 'c', '公開終了'), row('4', 'd', '公開前'), row('5', 'e', null)];
    expect(countStatuses(rows)).toEqual({ live: 2, ended: 1, before: 1, unknown: 1, all: 5 });
    expect(rows.filter(r => matchesStatus(r.publication_status, 'live')).map(r => r.id)).toEqual(['1', '2']);
    expect(rows.filter(r => matchesStatus(r.publication_status, 'all')).length).toBe(5);
    expect(countsFromServer({ live: 3, ended: 37, before: 1, unknown: 2 })).toEqual({ live: 3, ended: 37, before: 1, unknown: 2, all: 43 });
    expect(countsFromServer(undefined)).toBeNull();
  });
});

describe('案件の求人の表', () => {
  function mockDeal() {
    const rows = [row('1', '求人A', '公開中'), row('2', '求人B', '公開中'), row('3', '求人C', '公開中'), row('4', '求人D', '公開終了'), row('5', '求人E', '公開終了'), row('6', '求人F', null)];
    api.mockImplementation((url: string) => {
      if (url.includes('deal-applications')) return Promise.resolve({ ok: true, data: { shared_applications: 0, listings: rows.map(r => ({ listing_id: r.id, by_date: {} })) } });
      if (url.includes('listing-body-dates')) return Promise.resolve({ ok: true, data: { dates: {} } });
      return Promise.resolve({ ok: true, data: { status: 'ready', total: 6, listings: rows, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z' } });
    });
  }
  const titles = () => within(screen.getByRole('region', { name: 'この案件の求人' })).getAllByRole('button').map(b => b.textContent).filter(t => /^求人[A-F]$/.test(t));
  it('既定は掲載中だけを表に出し、合計は絞り込みの影響を受けない。切り替えは URL に残る', async () => {
    mockDeal();
    render(<DealSummary deal={{ id: '9', name: '案件', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
    const band = await screen.findByRole('group', { name: '案件の合計' });
    await waitFor(() => { expect(titles()).toEqual(['求人A', '求人B', '求人C']); });
    expect(band.textContent).toContain('掲載中 4 ・ 終了 2');
    const filter = screen.getByRole('group', { name: '掲載状況で絞り込む' });
    expect(within(filter).getByRole('button', { name: /掲載中\s*3/ }).getAttribute('aria-pressed')).toBe('true');
    expect(within(filter).getByRole('button', { name: /掲載終了\s*2/ })).toBeTruthy();
    expect(within(filter).getByRole('button', { name: /状態不明\s*1/ })).toBeTruthy();
    expect(within(filter).getByRole('button', { name: /すべて\s*6/ })).toBeTruthy();
    expect(screen.getByText(/絞り込みの影響を受けません/)).toBeTruthy();
    expect(window.location.search).toBe('');

    fireEvent.click(within(filter).getByRole('button', { name: /掲載終了\s*2/ }));
    expect(titles()).toEqual(['求人D', '求人E']);
    expect(window.location.search).toBe('?status=ended');
    expect(band.textContent).toContain('掲載中 4 ・ 終了 2');

    fireEvent.click(within(filter).getByRole('button', { name: /すべて\s*6/ }));
    expect(titles()).toEqual(['求人A', '求人B', '求人C', '求人D', '求人E', '求人F']);
    expect(window.location.search).toBe('?status=all');

    fireEvent.click(within(filter).getByRole('button', { name: /掲載中\s*3/ }));
    expect(window.location.search).toBe('');
  });
  it('URL に status=ended があれば最初から掲載終了だけを出す', async () => {
    mockDeal();
    window.history.replaceState(null, '', '/app/job-copy?status=ended');
    render(<DealSummary deal={{ id: '9', name: '案件', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} />);
    await screen.findByRole('group', { name: '案件の合計' });
    await waitFor(() => { expect(titles()).toEqual(['求人D', '求人E']); });
  });
});

describe('求人を探す（左の一覧）', () => {
  it('既定は掲載中で検索し、サーバの件数を出して、すべてでは公開状況を付けずに検索する', async () => {
    const urls: string[] = [];
    api.mockImplementation((url: string) => {
      if (url.startsWith('/api/job-copy/listings?')) urls.push(url);
      return Promise.resolve({ ok: true, data: { status: 'ready', total: 3, listings: [row('1', '求人A', '公開中')], titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-12T00:00:00Z', publication_counts: { live: 3100, ended: 37800, before: 4, unknown: 0 } } });
    });
    render(<HubSpotListingsPanel onOpen={vi.fn()} />);
    const filter = await screen.findByRole('group', { name: '掲載状況で絞り込む' });
    await waitFor(() => { expect(within(filter).getByRole('button', { name: /掲載中\s*3,100/ })).toBeTruthy(); });
    expect(within(filter).getByRole('button', { name: /掲載終了\s*37,800/ })).toBeTruthy();
    expect(within(filter).getByRole('button', { name: /すべて\s*40,904/ })).toBeTruthy();
    expect(within(filter).getByRole('button', { name: /掲載前\s*4/ })).toBeTruthy();
    expect(within(filter).queryByRole('button', { name: /状態不明/ })).toBeNull();
    expect(urls.at(-1)).toContain('publication=%E6%8E%B2%E8%BC%89%E4%B8%AD');
    fireEvent.click(within(filter).getByRole('button', { name: /掲載終了/ }));
    await waitFor(() => { expect(urls.at(-1)).toContain('publication=%E6%8E%B2%E8%BC%89%E7%B5%82%E4%BA%86'); });
    expect(window.location.search).toBe('?status=ended');
    fireEvent.click(within(filter).getByRole('button', { name: /すべて/ }));
    await waitFor(() => { expect(urls.at(-1)).not.toContain('publication='); });
    expect(window.location.search).toBe('?status=all');
  });
});
