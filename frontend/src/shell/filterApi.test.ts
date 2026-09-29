// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../api/client';
import { ShellAuthError, postSetFilter } from './filterApi';

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubFetch(res: Response): ReturnType<typeof vi.fn> {
  const f = vi.fn(() => Promise.resolve(res));
  vi.stubGlobal('fetch', f);
  return f;
}

describe('postSetFilter (via apiPostForm)', () => {
  it('POSTs the same urlencoded body to /api/set_{name} with X-Requested-With: fetch', async () => {
    const f = stubFetch(new Response('OK', { status: 200, headers: { 'content-type': 'text/html' } }));
    await postSetFilter('prefecture', { prefecture: '東京都' });
    const [url, init] = f.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/set_prefecture');
    expect(init.method).toBe('POST');
    expect(init.body).toBe('prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD');
    expect((init.headers as Record<string, string>)['X-Requested-With']).toBe('fetch');
  });

  it('keeps multi-field bodies in field order', async () => {
    const f = stubFetch(new Response('OK', { status: 200, headers: { 'content-type': 'text/html' } }));
    await postSetFilter('industry_filter', { job_types: 'a,b', industry_raws: 'x' });
    const [, init] = f.mock.calls[0] as unknown as [string, RequestInit];
    expect(init.body).toBe('job_types=a%2Cb&industry_raws=x');
  });

  it('throws ShellAuthError on 401 and the ApiError on other statuses', async () => {
    stubFetch(new Response('', { status: 401 }));
    await expect(postSetFilter('prefecture', { prefecture: 'x' })).rejects.toBeInstanceOf(ShellAuthError);
    stubFetch(new Response('', { status: 500 }));
    await expect(postSetFilter('prefecture', { prefecture: 'x' })).rejects.toBeInstanceOf(ApiHttpError);
  });
});
