// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchPrefectures, ShellAuthError } from './filterApi';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('fetchPrefectures (option HTML fragment via the shared client)', () => {
  it('HTTP 401 is ShellAuthError (so the shell redirects to login)', async () => {
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
    await expect(fetchPrefectures()).rejects.toBeInstanceOf(ShellAuthError);
  });

  it('sends X-Requested-With: fetch and parses the options', async () => {
    const f = vi.fn<typeof fetch>(() =>
      Promise.resolve(
        new Response('<option value="東京都" data-citycode="13">東京都</option>', {
          status: 200,
          headers: { 'content-type': 'text/html; charset=utf-8' },
        }),
      ),
    );
    vi.stubGlobal('fetch', f);
    const opts = await fetchPrefectures();
    expect(opts).toEqual([{ value: '東京都', label: '東京都', citycode: '13' }]);
    const init = f.mock.calls[0]?.[1];
    expect((init?.headers as Record<string, string>)['X-Requested-With']).toBe('fetch');
  });
});
