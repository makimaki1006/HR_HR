// @vitest-environment happy-dom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { redirectToLogin } from '../../shell/navigation';
import { useApiGet } from './useApiGet';

vi.mock('../../shell/navigation', () => ({ redirectToLogin: vi.fn() }));

const json = (body: unknown): Response =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

afterEach(() => {
  vi.mocked(redirectToLogin).mockClear();
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

  it('a 401 redirects to the login page exactly once', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(() =>
        Promise.resolve(
          new Response('{"error":"auth_required"}', {
            status: 401,
            headers: { 'content-type': 'application/json' },
          }),
        ),
      ),
    );
    renderHook(() => useApiGet<{ v: string }>('/api/a'));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(redirectToLogin).toHaveBeenCalledTimes(1);
  });

  it('a non-auth error (500) does not redirect', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(() => Promise.resolve(new Response('x', { status: 500 }))),
    );
    const { result } = renderHook(() => useApiGet<{ v: string }>('/api/a'));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(redirectToLogin).not.toHaveBeenCalled();
    expect(result.current.status).toBe('error');
  });
});
