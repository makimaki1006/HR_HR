// @vitest-environment happy-dom
import { act, cleanup, render, screen } from '@testing-library/react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { HubSpotUsageResponse } from '../../generated/HubSpotUsageResponse';
import {
  durationText,
  hitRateText,
  HUBSPOT_USAGE_PATH,
  HubSpotUsagePanel,
  HubSpotUsageView,
  remainingText,
} from './HubSpotUsagePanel';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/client')>()),
  apiGet: api,
}));
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.resetAllMocks();
});

/** Same shape as the Rust HubSpotUsageResponse (src/handlers/admin/hubspot_usage.rs) */
const sample: HubSpotUsageResponse = {
  configured: true,
  generated_at: '2026-10-08T03:00:00Z',
  limits: {
    per_second: 8,
    per_10s: 80,
    search_interval_ms: 1000,
    interactive_max_wait_ms: 5000,
    background_max_wait_ms: 60000,
  },
  rate_limit: {
    observed_at: '2026-10-08T02:59:58Z',
    per_10s_max: 190,
    per_10s_remaining: 170,
    per_second_max: 19,
    per_second_remaining: 15,
    daily_max: 625000,
    daily_remaining: 612345,
  },
  counters: { calls: 1234, search_calls: 56, rate_limited: 2, coalesced: 78, busy_rejected: 3 },
  calls_by_group: [
    { key: 'batch_read', label: 'まとめ読み', count: 900 },
    { key: 'search', label: '検索 (Search)', count: 56 },
  ],
  caches: [{ key: 'call_queue_page', label: '架電キューの一覧 (30 秒)', hits: 30, misses: 10 }],
  queue: {
    waiting: 4,
    waiting_search: 1,
    paused_ms: null,
    wait_p50_ms: 120,
    wait_p95_ms: 2500,
    wait_samples: 321,
    wait_window_secs: 300,
  },
};

describe('HubSpotUsageView', () => {
  it('shows the rate-limit headers, counters, queue and caches in plain Japanese', () => {
    const html = renderToStaticMarkup(<HubSpotUsageView data={sample} />);
    expect(html).toContain('HubSpot の利用状況');
    expect(html).toContain('残り 170 / 上限 190');
    expect(html).toContain('残り 15 / 上限 19');
    expect(html).toContain('残り 612,345 / 上限 625,000');
    expect(html).toContain('1,234 回');
    expect(html).toContain('混雑で断った');
    expect(html).toContain('120 ミリ秒 / 2.5 秒');
    expect(html).toContain('直近 5 分・321 件');
    expect(html).toContain('架電キューの一覧 (30 秒)');
    expect(html).toContain('75%');
    expect(html).toContain('1 秒あたり 8 回・10 秒あたり 80 回');
    expect(html).not.toContain('鍵が設定されていません');
  });
  it('formats missing values as not yet observed', () => {
    expect(remainingText(null, null)).toBe('未取得');
    expect(remainingText(3, null)).toBe('残り 3 / 上限 ?');
    expect(durationText(null)).toBe('記録なし');
    expect(durationText(999)).toBe('999 ミリ秒');
    expect(durationText(60000)).toBe('60 秒');
    expect(hitRateText(0, 0)).toBe('-');
    expect(hitRateText(1, 2)).toBe('33%');
  });
});

describe('HubSpotUsagePanel', () => {
  it('reads the endpoint at once and again every 15 seconds; a failed refresh keeps the last values', async () => {
    vi.useFakeTimers();
    api.mockResolvedValue({ ok: true, data: sample });
    render(<HubSpotUsagePanel />);
    await act(async () => {
      await Promise.resolve();
    });
    expect(api).toHaveBeenCalledTimes(1);
    expect(api.mock.calls[0]?.[0]).toBe(HUBSPOT_USAGE_PATH);
    expect(screen.getByTestId('c-calls').textContent).toBe('1,234 回');

    api.mockResolvedValue({ ok: true, data: { ...sample, counters: { ...sample.counters, calls: 2000 } } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_000);
    });
    expect(api).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('c-calls').textContent).toBe('2,000 回');

    api.mockResolvedValue({ ok: false, error: Object.assign(new Error('HTTP 502'), { status: 502 }) });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_000);
    });
    expect(api).toHaveBeenCalledTimes(3);
    expect(screen.getByTestId('hubspot-refresh-error').textContent).toContain('前回の値のまま');
    expect(screen.getByTestId('c-calls').textContent).toBe('2,000 回');
  });
});
