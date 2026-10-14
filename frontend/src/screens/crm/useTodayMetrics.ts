import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiAbortedError, ApiHttpError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { AllCallersResponse } from '../../generated/AllCallersResponse';
import type { MonthAllResponse } from '../../generated/MonthAllResponse';
import type { MonthMetricsResponse } from '../../generated/MonthMetricsResponse';
import type { TodayMetricsResponse } from '../../generated/TodayMetricsResponse';
import { FIXTURE_ALL, FIXTURE_MONTH, FIXTURE_MONTH_ALL, FIXTURE_TODAY, FIXTURE_TODAY_SALES } from './metricsModel';

/** 読み直す間隔。サーバも 2 分に 1 回までしか HubSpot を読み直さない */
export const METRICS_REFRESH_MS = 120_000;
/** サーバが「集計中」(loading: true) と答えたときの取り直しの間隔 */
export const METRICS_WARMING_MS = 12_000;

export type MetricsFetch<T> = (signal: AbortSignal) => Promise<ApiResult<T>>;

export const liveTodayFetch: MetricsFetch<TodayMetricsResponse> = signal =>
  apiGet<TodayMetricsResponse>('/api/crm/metrics/today', { signal, timeoutMs: 30_000 });
export const liveAllFetch: MetricsFetch<AllCallersResponse> = signal =>
  apiGet<AllCallersResponse>('/api/crm/metrics/today/all', { signal, timeoutMs: 30_000 });

export const liveMonthFetch: MetricsFetch<MonthMetricsResponse> = signal =>
  apiGet<MonthMetricsResponse>('/api/crm/metrics/month', { signal, timeoutMs: 30_000 });
export const liveMonthAllFetch: MetricsFetch<MonthAllResponse> = signal =>
  apiGet<MonthAllResponse>('/api/crm/metrics/month/all', { signal, timeoutMs: 30_000 });

const fixtureOf = <T,>(data: T): MetricsFetch<T> => async signal => {
  await Promise.resolve();
  return signal.aborted ? { ok: false, error: new ApiAbortedError('aborted') } : { ok: true, data };
};
export const fixtureTodayFetch = fixtureOf(FIXTURE_TODAY);
export const fixtureAllFetch = fixtureOf(FIXTURE_ALL);
export const fixtureSalesTodayFetch = fixtureOf(FIXTURE_TODAY_SALES);
export const fixtureMonthFetch = fixtureOf(FIXTURE_MONTH);
export const fixtureMonthAllFetch = fixtureOf(FIXTURE_MONTH_ALL);

/** fixture のとき、URL の `metrics=sales` なら新規営業の人の見え方 (今月の帯) にする */
export function pickFixtureToday(): MetricsFetch<TodayMetricsResponse> {
  const sales = typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('metrics') === 'sales';
  return sales ? fixtureSalesTodayFetch : fixtureTodayFetch;
}

export type MetricsState<T> =
  | { phase: 'loading' }
  | { phase: 'warming' }
  | { phase: 'ready'; data: T; refreshFailed: boolean }
  | { phase: 'error'; message: string };

function failure(error: unknown): string {
  if (error instanceof ApiHttpError && error.status === 403) return 'この数字を見る権限がありません。';
  if (error instanceof ApiHttpError && error.status === 409) return 'HubSpot の担当者と結び付けられませんでした。';
  return '今日の成績を読み込めませんでした。';
}

/**
 * 今日の成績を取り、2 分ごとに読み直す。タブが隠れている間は読み直さず、見えたときに
 * 前回から 2 分以上たっていればすぐ読み直す。読み直しに失敗しても前回の数字は残す。
 */
export function useMetrics<T>(fetcher: MetricsFetch<T>, intervalMs: number = METRICS_REFRESH_MS, warmingMs: number = METRICS_WARMING_MS) {
  const [state, setState] = useState<MetricsState<T>>({ phase: 'loading' });
  const [nonce, setNonce] = useState(0);
  const lastRef = useRef(0);

  useEffect(() => {
    let ctl: AbortController | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let stopped = false;
    const clear = () => { if (timer !== null) { clearTimeout(timer); timer = null; } };
    const hidden = () => typeof document !== 'undefined' && document.visibilityState === 'hidden';
    let delay = intervalMs;
    const schedule = () => {
      clear();
      if (stopped || hidden()) return;
      timer = setTimeout(load, delay);
    };
    function load() {
      ctl?.abort();
      const c = new AbortController();
      ctl = c;
      lastRef.current = Date.now();
      void fetcher(c.signal).then(r => {
        if (stopped || c.signal.aborted) return;
        delay = intervalMs;
        if (r.ok && (r.data as { loading?: boolean }).loading === true) {
          // 今日の最初の読み込みが背景で進んでいる。エラーにせず、短い間隔で取り直す
          delay = warmingMs;
          setState({ phase: 'warming' });
        } else if (r.ok) setState({ phase: 'ready', data: r.data, refreshFailed: false });
        else if (!(r.error instanceof ApiAbortedError)) {
          const message = failure(r.error);
          setState(prev => (prev.phase === 'ready' ? { ...prev, refreshFailed: true } : { phase: 'error', message }));
        }
        schedule();
      }).catch(() => { if (!stopped) schedule(); });
    }
    const onVisibility = () => {
      if (hidden()) { clear(); return; }
      if (Date.now() - lastRef.current >= delay) load(); else schedule();
    };
    load();
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      stopped = true;
      clear();
      ctl?.abort();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [fetcher, intervalMs, warmingMs, nonce]);

  const reload = useCallback(() => { setState({ phase: 'loading' }); setNonce(n => n + 1); }, []);
  return { state, reload };
}
