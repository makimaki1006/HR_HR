// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError, AuthRequiredError, ApiNetworkError } from '../../api/client';
import { DEFAULT_FILTERS } from './queueModel';
import type { QueueFilters } from './queueModel';
import { makeItem, makeResponse, deferredFetcher } from './queueTestUtil';
import { SCOPE_MISMATCH_MESSAGE, fixtureFetch, useCallQueue } from './useCallQueue';

afterEach(() => { cleanup(); });

const f = (patch: Partial<QueueFilters>): QueueFilters => ({ ...DEFAULT_FILTERS, ...patch });
const flush = () => act(async () => { await new Promise(r => setTimeout(r, 0)); });
const http = (status: number, body?: unknown) => ({ ok: false as const, error: new ApiHttpError(status, body) });

describe('useCallQueue: stale responses', () => {
  it('a late response for the old condition never replaces the new condition (and its signal was aborted)', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = f({ q: 'A' }); const B = f({ q: 'B' });
    const { result, rerender } = renderHook(({ fl }: { fl: QueueFilters }) => useCallQueue(fl, 'live', fetcher), { initialProps: { fl: A } });
    rerender({ fl: B });
    expect(calls).toHaveLength(2);
    expect(calls[0]?.signal.aborted).toBe(true);
    expect(calls[1]?.signal.aborted).toBe(false);
    // B が先に返り、その後で古い A が遅れて返る
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(B, [makeItem('b1')]) }); await Promise.resolve(); });
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('a1')]) }); await Promise.resolve(); });
    await flush();
    expect(result.current.state.phase).toBe('ready');
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['b1']);
  });

  it('a late response of the old condition arriving before the new one is still ignored', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = f({ sort: 'next_call_asc' }); const B = f({ sort: 'last_call_desc' });
    const { result, rerender } = renderHook(({ fl }: { fl: QueueFilters }) => useCallQueue(fl, 'live', fetcher), { initialProps: { fl: A } });
    rerender({ fl: B });
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('a1')]) }); await Promise.resolve(); });
    expect(result.current.state.phase).toBe('loading');
    expect(result.current.state.items).toEqual([]);
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(B, [makeItem('b1')]) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['b1']);
  });

  it('A -> B -> A: the first A answering late does not show as the new A', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = f({ q: 'A' }); const B = f({ q: 'B' });
    const { result, rerender } = renderHook(({ fl }: { fl: QueueFilters }) => useCallQueue(fl, 'live', fetcher), { initialProps: { fl: A } });
    rerender({ fl: B });
    rerender({ fl: f({ q: 'A' }) });
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('STALE')]) }); await Promise.resolve(); });
    expect(result.current.state.phase).toBe('loading');
    await act(async () => { calls[2]?.resolve({ ok: true, data: makeResponse(A, [makeItem('fresh')]) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['fresh']);
  });

  it('a response whose scope does not match the current condition is not shown', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = f({ q: 'A' });
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(f({ q: 'ほかの条件' }), [makeItem('x')]) }); await Promise.resolve(); });
    expect(result.current.state.phase).toBe('error');
    expect(result.current.state.items).toEqual([]);
    expect(result.current.state.message).toBe(SCOPE_MISMATCH_MESSAGE);
  });
});

describe('useCallQueue: load more', () => {
  it('appends the next page for the same condition, dedupes by deal_id, and clears the cursor at the end', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = DEFAULT_FILTERS;
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('1'), makeItem('2')], { next_cursor: 'cur-1' }) }); await Promise.resolve(); });
    expect(result.current.state.nextCursor).toBe('cur-1');
    act(() => { result.current.loadMore(); });
    expect(calls[1]?.cursor).toBe('cur-1');
    expect(calls[1]?.filters).toEqual(A);
    // 2 ページ目に 1 ページ目と同じ deal 2 が再び出る (ページ跨ぎの更新)
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(A, [makeItem('2'), makeItem('3')]) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['1', '2', '3']);
    expect(result.current.state.nextCursor).toBeNull();
  });

  it('pressing load more twice sends one request for the cursor (no duplicate append)', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = DEFAULT_FILTERS;
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('1')], { next_cursor: 'cur-1' }) }); await Promise.resolve(); });
    act(() => { result.current.loadMore(); result.current.loadMore(); });
    expect(calls).toHaveLength(2);
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(A, [makeItem('2')], { next_cursor: 'cur-2' }) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['1', '2']);
    // 返ったあとの再呼び出しは新しい cursor を使う
    act(() => { result.current.loadMore(); });
    expect(calls[2]?.cursor).toBe('cur-2');
  });

  it('a load-more answer that arrives after the condition changed is not appended to the new list', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = f({ q: 'A' }); const B = f({ q: 'B' });
    const { result, rerender } = renderHook(({ fl }: { fl: QueueFilters }) => useCallQueue(fl, 'live', fetcher), { initialProps: { fl: A } });
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('a1')], { next_cursor: 'cur-A' }) }); await Promise.resolve(); });
    act(() => { result.current.loadMore(); });
    rerender({ fl: B });
    expect(calls[1]?.signal.aborted).toBe(true);
    await act(async () => { calls[2]?.resolve({ ok: true, data: makeResponse(B, [makeItem('b1')]) }); await Promise.resolve(); });
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(A, [makeItem('a2')]) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['b1']);
  });

  it('a load-more answer with a mismatching scope is not appended and shows a message', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = DEFAULT_FILTERS;
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('1')], { next_cursor: 'c' }) }); await Promise.resolve(); });
    act(() => { result.current.loadMore(); });
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(f({ sort: 'last_call_asc' }), [makeItem('9')]) }); await Promise.resolve(); });
    expect(result.current.state.items.map(i => i.deal_id)).toEqual(['1']);
    expect(result.current.state.moreError?.message).toBe(SCOPE_MISMATCH_MESSAGE);
    expect(result.current.state.nextCursor).toBe('c');
  });

  it('cursor_mismatch while loading more keeps the list and reports the kind; reload starts over', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = DEFAULT_FILTERS;
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('1')], { next_cursor: 'c' }) }); await Promise.resolve(); });
    act(() => { result.current.loadMore(); });
    await act(async () => { calls[1]?.resolve(http(400, { error_kind: 'cursor_mismatch' })); await Promise.resolve(); });
    expect(result.current.state.items).toHaveLength(1);
    expect(result.current.state.moreError?.kind).toBe('cursor_mismatch');
    act(() => { result.current.reload(); });
    expect(calls[2]?.cursor).toBeNull();
    expect(result.current.state.items).toEqual([]);
  });
});

describe('useCallQueue: states', () => {
  it('empty result is ready with no items and no cursor', async () => {
    const { calls, fetcher } = deferredFetcher();
    const { result } = renderHook(() => useCallQueue(DEFAULT_FILTERS, 'live', fetcher));
    expect(result.current.state.phase).toBe('loading');
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(DEFAULT_FILTERS, []) }); await Promise.resolve(); });
    expect(result.current.state.phase).toBe('ready');
    expect(result.current.state.items).toEqual([]);
    expect(result.current.state.nextCursor).toBeNull();
  });

  it('accumulates partial counts over pages', async () => {
    const { calls, fetcher } = deferredFetcher();
    const A = DEFAULT_FILTERS;
    const part = (mc: number, np: number, failed: string[]) => ({
      missing_contacts: mc, missing_companies: 0, failed, excluded: { no_phone: np, stop_reason: 0, out_of_scope: 0 },
    });
    const { result } = renderHook(() => useCallQueue(A, 'live', fetcher));
    await act(async () => { calls[0]?.resolve({ ok: true, data: makeResponse(A, [makeItem('1')], { next_cursor: 'c', partial: part(1, 2, ['contacts']) }) }); await Promise.resolve(); });
    act(() => { result.current.loadMore(); });
    await act(async () => { calls[1]?.resolve({ ok: true, data: makeResponse(A, [makeItem('2')], { partial: part(2, 1, ['contacts', 'associations']) }) }); await Promise.resolve(); });
    expect(result.current.state.partial?.missing_contacts).toBe(3);
    expect(result.current.state.partial?.excluded.no_phone).toBe(3);
    expect(result.current.state.partial?.failed).toEqual(['contacts', 'associations']);
  });

  it('maps failures: 401 / auth redirect -> unauthorized, 403 kinds, 503 kind -> error; never falls back to fixture items', async () => {
    const cases: [Parameters<ReturnType<typeof deferredFetcher>['calls'][number]['resolve']>[0], string, string][] = [
      [{ ok: false, error: new AuthRequiredError('x') }, 'unauthorized', 'ログイン'],
      [http(401), 'unauthorized', 'ログイン'],
      [http(409, { error_kind: 'owner_not_resolved' }), 'error', '所有者を選んでください'],
      [http(403, { error_kind: 'forbidden' }), 'unauthorized', '権限'],
      [http(503, { error_kind: 'hubspot_rate_limited' }), 'error', '上限'],
      [http(502, { error_kind: 'hubspot_timeout' }), 'error', '時間内'],
      [http(504, { error_kind: 'crm_timeout' }), 'error', '時間がかかりすぎ'],
      [{ ok: false, error: new ApiNetworkError('offline') }, 'error', 'ネットワーク'],
    ];
    for (const [res, phase, text] of cases) {
      const { calls, fetcher } = deferredFetcher();
      const { result, unmount } = renderHook(() => useCallQueue(DEFAULT_FILTERS, 'live', fetcher));
      await act(async () => { calls[0]?.resolve(res); await Promise.resolve(); });
      expect(result.current.state.phase, text).toBe(phase);
      expect(result.current.state.message, text).toContain(text);
      expect(result.current.state.items).toEqual([]);
      unmount();
    }
  });

  it('live mode with the default fetcher shows the error, not fixture rows, when the request fails', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.reject(new TypeError('offline'))));
    try {
      const { result } = renderHook(() => useCallQueue(DEFAULT_FILTERS, 'live'));
      await waitFor(() => { expect(result.current.state.phase).toBe('error'); });
      expect(result.current.state.items).toEqual([]);
    } finally { vi.unstubAllGlobals(); }
  });

  it('invalid conditions are reported and nothing is fetched', async () => {
    const { calls, fetcher } = deferredFetcher();
    const { result } = renderHook(() => useCallQueue(f({ nextFrom: '2026-10-10', nextTo: '2026-10-01' }), 'live', fetcher));
    await flush();
    expect(result.current.state.phase).toBe('invalid');
    expect(result.current.state.invalid).toHaveLength(1);
    expect(calls).toHaveLength(0);
  });

  it('fixture mode answers with fictional rows only and does not call fetch', async () => {
    const spy = vi.fn<typeof fetch>();
    vi.stubGlobal('fetch', spy);
    try {
      const { result } = renderHook(() => useCallQueue(DEFAULT_FILTERS, 'fixture'));
      await waitFor(() => { expect(result.current.state.phase).toBe('ready'); });
      expect(result.current.state.items.length).toBeGreaterThan(0);
      expect(spy).not.toHaveBeenCalled();
      expect(result.current.state.items.every(i => i.deep_links.deal.startsWith('https://example.invalid/'))).toBe(true);
    } finally { vi.unstubAllGlobals(); }
  });

  it('the fixture fetcher honours abort', async () => {
    const ctl = new AbortController();
    const p = fixtureFetch(DEFAULT_FILTERS, null, ctl.signal);
    ctl.abort();
    expect((await p).ok).toBe(false);
  });
});
