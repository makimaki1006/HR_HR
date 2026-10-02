// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchMunicipalities, fetchPrefectures, ShellAuthError } from './filterApi';

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

  it('asks for Accept: text/html (option fragments, not JSON)', async () => {
    const f = vi.fn<typeof fetch>(() =>
      Promise.resolve(
        new Response('<option value="大阪府">大阪府</option>', {
          status: 200,
          headers: { 'content-type': 'text/html' },
        }),
      ),
    );
    vi.stubGlobal('fetch', f);
    await fetchPrefectures();
    await fetchMunicipalities('東京都');
    const accepts = f.mock.calls.map((c) => (c[1]?.headers as Record<string, string>).Accept);
    expect(accepts).toEqual(['text/html', 'text/html']);
  });

  it('a 303 -> /login redirect (HTML login page) is ShellAuthError', async () => {
    const res = new Response('<form action="/login" method="post"></form>', {
      status: 200,
      headers: { 'content-type': 'text/html' },
    });
    Object.defineProperty(res, 'redirected', { value: true });
    Object.defineProperty(res, 'url', { value: 'http://localhost/login' });
    vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(res)));
    await expect(fetchPrefectures()).rejects.toBeInstanceOf(ShellAuthError);
  });
});
