import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import { invalidateListingUrl, loadCompany, loadVersions, MAX_PREFETCH, peekVersions, prefetchNeighbors } from './listingDetailCache';
import type { HubSpotVersions } from './hubspotListings';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: api }));

interface Pending { path: string; signal: AbortSignal | undefined; resolve: (result: ApiResult<unknown>) => void }
let pending: Pending[] = [];
const detail = (id: string) => ({ listing: { id }, versions: [], history_counts: {}, history_may_be_incomplete: false }) as unknown as HubSpotVersions;
const calls = (suffix: string) => api.mock.calls.filter(call => String(call[0]).includes(suffix)).map(call => String(call[0]));
const idOf = (path: string) => /listings\/(\d+)\//.exec(path)?.[1] ?? '';
/** Every request waits until the test answers it. */
function hold() {
  api.mockImplementation((path: string, options?: { signal?: AbortSignal }) => new Promise(resolve => {
    pending.push({ path, signal: options?.signal, resolve });
  }));
}
const answer = (item: Pending, result?: ApiResult<unknown>) => { item.resolve(result ?? { ok: true, data: path2data(item.path) }); };
const path2data = (path: string) => path.endsWith('/company') ? { listing_id: idOf(path), company_name: '合成運輸株式会社' } : detail(idOf(path));
const waiting = (): Pending => {
  const item = pending[0];
  if (!item) throw new Error('no request is waiting');
  return item;
};
const tick = async () => { for (let i = 0; i < 5; i += 1) await Promise.resolve(); };

beforeEach(() => { pending = []; });
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks(); });

describe('listing detail reuse', () => {
  it('reads a listing once and serves the reopened listing from memory for 60 seconds', async () => {
    vi.useFakeTimers(); vi.setSystemTime(new Date('2026-10-13T00:00:00Z'));
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: detail(idOf(path)) }));
    const first = await loadVersions('10');
    expect(first.ok && first.data.listing.id).toBe('10');
    expect(calls('/versions')).toEqual(['/api/job-copy/listings/10/versions']);
    const again = await loadVersions('10');
    expect(again).toEqual(first);
    expect(api).toHaveBeenCalledTimes(1);
    vi.setSystemTime(new Date('2026-10-13T00:00:59Z'));
    expect(peekVersions('10')).not.toBeNull();
    vi.setSystemTime(new Date('2026-10-13T00:01:01Z'));
    expect(peekVersions('10')).toBeNull();
    await loadVersions('10');
    expect(api).toHaveBeenCalledTimes(2);
  });
  it('joins simultaneous reads of one listing and does not keep failures', async () => {
    hold();
    const [a, b] = [loadVersions('10'), loadVersions('10')];
    await tick();
    expect(pending).toHaveLength(1);
    answer(waiting(), { ok: false, error: new ApiError('busy') });
    expect((await a).ok).toBe(false);
    expect((await b).ok).toBe(false);
    expect(peekVersions('10')).toBeNull();
    pending = [];
    const retry = loadVersions('10');
    await tick();
    expect(pending).toHaveLength(1);
    answer(waiting());
    expect((await retry).ok).toBe(true);
  });
  it('lets a cancelled caller go at once and stops a read nobody waits for', async () => {
    hold();
    const controller = new AbortController();
    const result = loadVersions('10', { signal: controller.signal });
    await tick();
    controller.abort();
    expect((await result).ok).toBe(false);
    expect(waiting().signal?.aborted).toBe(true);
  });
  it('forgets a listing after a draft is saved to it, and ignores a read that began before the save', async () => {
    hold();
    const first = loadVersions('10');
    await tick();
    answer(waiting());
    await first;
    expect(peekVersions('10')).not.toBeNull();
    invalidateListingUrl('/api/job-copy/listings/10/manual-draft');
    expect(peekVersions('10')).toBeNull();
    pending = [];
    const stale = loadVersions('10');
    await tick();
    invalidateListingUrl('/api/job-copy/listings/10/draft');
    answer(waiting());
    await stale;
    expect(peekVersions('10'), 'a read that began before the save is not kept').toBeNull();
    // Other listings are untouched.
    pending = [];
    const other = loadVersions('11');
    await tick();
    answer(waiting());
    await other;
    invalidateListingUrl('/api/job-copy/listings/10/draft');
    expect(peekVersions('11')).not.toBeNull();
  });
});

describe('background reading of the neighbours', () => {
  const ids = ['1', '2', '3', '4', '5', '6', '7'];
  it('reads only the next and previous listing, never more than the limit at once', async () => {
    hold();
    prefetchNeighbors(ids, '4');
    await tick();
    expect(MAX_PREFETCH).toBeLessThanOrEqual(2);
    expect(pending.map(item => idOf(item.path)).sort()).toEqual(['3', '5']);
    expect(pending.length).toBeLessThanOrEqual(MAX_PREFETCH);
    pending.forEach(item => { answer(item); });
    await tick();
    expect(peekVersions('5')).not.toBeNull();
    expect(peekVersions('3')).not.toBeNull();
    expect(api).toHaveBeenCalledTimes(2);
    // Warmed neighbours are served at once.
    await loadVersions('5');
    expect(api).toHaveBeenCalledTimes(2);
  });
  it('waits for the listing being opened, and drops the old plan when the user moves on', async () => {
    hold();
    const opening = loadVersions('4');
    await tick();
    prefetchNeighbors(ids, '4');
    await tick();
    expect(pending.map(item => idOf(item.path))).toEqual(['4']);
    answer(waiting());
    await opening;
    await tick();
    const warming = pending.slice(1);
    expect(warming.map(item => idOf(item.path)).sort()).toEqual(['3', '5']);
    // The user moves on to 6 while 3 and 5 are still being read: 3 is no longer wanted.
    prefetchNeighbors(ids, '6');
    await tick();
    expect(warming.find(item => idOf(item.path) === '3')?.signal?.aborted).toBe(true);
    expect(warming.find(item => idOf(item.path) === '5')?.signal?.aborted).toBe(false);
    const running = pending.filter(item => !item.signal?.aborted && !warming.includes(item) && idOf(item.path) !== '4');
    expect(running.map(item => idOf(item.path))).toEqual(['7']);
    expect(pending.filter(item => !item.signal?.aborted).length - 1).toBeLessThanOrEqual(MAX_PREFETCH);
  });
  it('does nothing for an unknown listing or the ends of the list', async () => {
    hold();
    prefetchNeighbors(ids, '99');
    prefetchNeighbors(ids, '1');
    await tick();
    expect(pending.map(item => idOf(item.path))).toEqual(['2']);
  });
});

describe('customer name', () => {
  it('is read separately, kept, and a failure leaves it blank without being remembered', async () => {
    api.mockResolvedValueOnce({ ok: false, error: new Error('offline') });
    expect(await loadCompany('10')).toBeUndefined();
    api.mockResolvedValueOnce({ ok: true, data: { listing_id: '10', company_name: '合成運輸株式会社' } });
    expect(await loadCompany('10')).toBe('合成運輸株式会社');
    expect(await loadCompany('10')).toBe('合成運輸株式会社');
    expect(calls('/company')).toEqual(['/api/job-copy/listings/10/company', '/api/job-copy/listings/10/company']);
    expect(api).toHaveBeenCalledTimes(2);
    api.mockResolvedValueOnce({ ok: true, data: { listing_id: '11', company_name: null } });
    expect(await loadCompany('11')).toBeNull();
    expect(await loadCompany('11')).toBeNull();
    expect(api).toHaveBeenCalledTimes(3);
  });
});
