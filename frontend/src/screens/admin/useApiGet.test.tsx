// @vitest-environment happy-dom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useApiGet } from './useApiGet';

const json = (body: unknown): Response =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('useApiGet', () => {
  it('a late response of an earlier request for the same path (A -> B -> A) does not show as the new A', async () => {
    const pending: { path: string; resolve: (r: Response) => void }[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(
        (input) =>
          new Promise<Response>((resolve) => {
            pending.push({ path: typeof input === 'string' ? input : '?', resolve });
          }),
      ),
    );
    const { result, rerender } = renderHook(({ p }: { p: string }) => useApiGet<{ v: string }>(p), {
      initialProps: { p: '/api/a' },
    });
    rerender({ p: '/api/b' });
    rerender({ p: '/api/a' });
    expect(pending.map((x) => x.path)).toEqual(['/api/a', '/api/b', '/api/a']);
    // The first (aborted) request answers late, while the third is still in flight.
    await act(async () => {
      pending[0]?.resolve(json({ v: 'STALE' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(result.current).toEqual({ status: 'loading' });
    await act(async () => {
      pending[2]?.resolve(json({ v: 'FRESH' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(result.current).toEqual({ status: 'ok', data: { v: 'FRESH' } });
  });
});
