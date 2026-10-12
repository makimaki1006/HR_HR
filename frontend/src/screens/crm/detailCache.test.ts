import { describe, expect, it } from 'vitest';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { DETAIL_CACHE_MAX, DETAIL_CACHE_TTL_MS, DetailCache, detailCacheFor, dropDealEverywhere } from './detailCache';

const data = (id: string): WorkspaceResponse => ({ deal: { id } } as unknown as WorkspaceResponse);

describe('DetailCache', () => {
  it('returns the stored detail for 60 s and then forgets it', () => {
    const t = { v: 1_000 };
    const c = new DetailCache(() => t.v);
    c.set('a', '1', data('1'));
    t.v += DETAIL_CACHE_TTL_MS;
    expect(c.get('a')?.deal.id).toBe('1');
    t.v += 1;
    expect(c.get('a')).toBeNull();
    expect(c.size).toBe(0);
  });

  it('keeps at most 20 entries and drops the least recently used first', () => {
    const c = new DetailCache(() => 0);
    for (let i = 0; i < DETAIL_CACHE_MAX; i += 1) c.set(`k${String(i)}`, String(i), data(String(i)));
    c.get('k0');
    c.set('new', '99', data('99'));
    expect(c.size).toBe(20);
    expect(c.get('k0')).not.toBeNull();
    expect(c.get('k1')).toBeNull();
  });

  it('dropDeal removes every key of that deal only', () => {
    const c = new DetailCache(() => 0);
    c.set(DetailCache.key('1', 'live', ''), '1', data('1'));
    c.set(DetailCache.key('1', 'live', '?deal_props=a'), '1', data('1'));
    c.set(DetailCache.key('2', 'live', ''), '2', data('2'));
    c.dropDeal('1');
    expect(c.size).toBe(1);
    expect(c.get(DetailCache.key('2', 'live', ''))).not.toBeNull();
  });

  it('dropDealEverywhere clears the deal from every fetcher cache (a write drops the screen cache too)', () => {
    const f1 = () => 1; const f2 = () => 2;
    detailCacheFor(f1).set('k', '7', data('7'));
    detailCacheFor(f2).set('k', '7', data('7'));
    dropDealEverywhere('7');
    expect(detailCacheFor(f1).get('k')).toBeNull();
    expect(detailCacheFor(f2).get('k')).toBeNull();
  });
});
