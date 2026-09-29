import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiAbortedError,
  ApiDataError,
  ApiHttpError,
  ApiTimeoutError,
  AuthRequiredError,
  DEFAULT_TIMEOUT_MS,
  apiGet,
} from './client';

type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** What fetch yields after following Rust's 303 -> /login for an unauthenticated request. */
function loginPageAfterRedirect(): Response {
  const res = new Response('<!DOCTYPE html><html><body>ログイン</body></html>', {
    status: 200,
    headers: { 'content-type': 'text/html; charset=utf-8' },
  });
  Object.defineProperty(res, 'redirected', { value: true });
  Object.defineProperty(res, 'url', { value: 'https://hr-hw.onrender.com/login' });
  return res;
}

let fetchMock: FetchMock;

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('apiGet', () => {
  it('case 1: returns the parsed JSON body for 200 application/json', async () => {
    const payload = { prefecture: '東京都', posting_count: 1234, ratio: 0.42 };
    fetchMock.mockResolvedValueOnce(jsonResponse(payload));

    const result = await apiGet<typeof payload>('/api/recruitment_diag/difficulty?prefecture=東京都');

    expect(result).toEqual({ ok: true, data: { prefecture: '東京都', posting_count: 1234, ratio: 0.42 } });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe('/api/recruitment_diag/difficulty?prefecture=東京都');
    expect(init?.credentials).toBe('same-origin');
    expect(init?.method).toBe('GET');
    expect(init?.headers).toEqual({ Accept: 'application/json', 'X-Requested-With': 'fetch' });
  });

  it('case 2: maps a redirect to /login into AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(loginPageAfterRedirect());

    const result = await apiGet('/api/recruitment_diag/difficulty');

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (redirected to /login)');
  });

  it('case 2b: maps a non-JSON 200 body (no redirect flag) into AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('<html></html>', { status: 200, headers: { 'content-type': 'text/html' } }),
    );

    const result = await apiGet('/api/recruitment_diag/difficulty');

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (non-JSON response: text/html)');
  });

  it('case 3: maps HTTP 200 {"error": ...} into ApiDataError with the exact message', async () => {
    const body = { error: 'hellowork.db 未接続', notes: { hw_scope: 'HW 掲載求人のみ' } };
    fetchMock.mockResolvedValueOnce(jsonResponse(body));

    const result = await apiGet('/api/recruitment_diag/talent_pool');

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiDataError);
    expect(result.error.message).toBe('hellowork.db 未接続');
    expect((result.error as ApiDataError).body).toEqual(body);
  });

  it('case 4: returns ApiTimeoutError after 15s, not before', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementationOnce(
      (_input, init) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('The operation was aborted.', 'AbortError'));
          });
        }),
    );

    let settled: Awaited<ReturnType<typeof apiGet>> | undefined;
    const pending = apiGet('/api/recruitment_diag/inflow').then((r) => (settled = r));

    await vi.advanceTimersByTimeAsync(DEFAULT_TIMEOUT_MS - 1);
    expect(settled).toBeUndefined();

    await vi.advanceTimersByTimeAsync(1);
    await pending;

    expect(DEFAULT_TIMEOUT_MS).toBe(15_000);
    expect(settled?.ok).toBe(false);
    if (settled === undefined || settled.ok) return;
    expect(settled.error).toBeInstanceOf(ApiTimeoutError);
    expect(settled.error.message).toBe('request timed out after 15000 ms');
  });

  it('distinguishes caller abort from timeout', async () => {
    fetchMock.mockImplementationOnce(
      (_input, init) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('The operation was aborted.', 'AbortError'));
          });
        }),
    );
    const caller = new AbortController();
    const pending = apiGet('/api/recruitment_diag/inflow', { signal: caller.signal });
    caller.abort();

    const result = await pending;
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiAbortedError);
  });

  it('returns ApiHttpError with the status for 500', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ message: 'boom' }, 500));

    const result = await apiGet('/api/recruitment_diag/inflow');

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiHttpError);
    expect((result.error as ApiHttpError).status).toBe(500);
  });

  it('refuses cross-origin targets without calling fetch', async () => {
    const result = await apiGet('https://example.com/api/x');
    const result2 = await apiGet('//example.com/api/x');

    expect(result.ok).toBe(false);
    expect(result2.ok).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
