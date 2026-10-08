/**
 * Where the market data (/api/job-copy/market) comes from, with one cache per screen.
 *
 * The list of occupations / prefectures and the months of one (occupation, prefecture) do not
 * change while the screen is open (the Indeed data is refreshed monthly), so the timeline and the
 * 市場 tabs share one cache: switching between jobs or tabs does not fetch the same data again.
 * A failed request is never cached, so 「再取得」 always asks the server again.
 */
import { createContext, useContext, useMemo } from 'react';
import { apiGet } from '../../api/client';
import { demoMarketData } from './data';
import type { MarketData } from './marketChartModel';

export type MarketResult = { ok: true; data: MarketData } | { ok: false };
/** title and prefecture empty: the list only (series null). */
export type MarketFetch = (title: string, prefecture: string) => Promise<MarketResult>;
export type MarketCache = Map<string, Promise<MarketResult>>;

const apiMarket: MarketFetch = async (title, prefecture) => {
  const params = title && prefecture ? `?${new URLSearchParams({ title, prefecture }).toString()}` : '';
  const result = await apiGet<MarketData>(`/api/job-copy/market${params}`);
  return result.ok ? { ok: true, data: result.data } : { ok: false };
};
const demoMarket: MarketFetch = (title, prefecture) => Promise.resolve({ ok: true, data: demoMarketData(title, prefecture) });

/** A fetcher for the mode. With a cache, a request already made (or running) is reused. */
export function marketFetcher(mode: 'api' | 'demo', cache?: MarketCache): MarketFetch {
  const fetchMarket = mode === 'demo' ? demoMarket : apiMarket;
  if (!cache) return fetchMarket;
  return (title, prefecture) => {
    const key = JSON.stringify([mode, title, prefecture]);
    const cached = cache.get(key);
    if (cached) return cached;
    const request = fetchMarket(title, prefecture).then(result => {
      if (!result.ok) cache.delete(key);
      return result;
    }, (error: unknown) => {
      cache.delete(key);
      throw error;
    });
    cache.set(key, request);
    return request;
  };
}

/** Provided by JobCopyScreen; without it (a component rendered alone) nothing is cached. */
export const MarketCacheContext = createContext<MarketCache | null>(null);

export function useMarketFetch(mode: 'api' | 'demo'): MarketFetch {
  const cache = useContext(MarketCacheContext);
  return useMemo(() => marketFetcher(mode, cache ?? undefined), [mode, cache]);
}
