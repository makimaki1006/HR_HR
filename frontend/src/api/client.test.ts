import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiAbortedError,
  ApiDataError,
  ApiHttpError,
  ApiInvalidResponseError,
  ApiTimeoutError,
  AuthRequiredError,
  DEFAULT_TIMEOUT_MS,
  NO_TIMEOUT,
  apiGet,
  apiPost,
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

  it('case 2b: a text/html 200 with the login form (no redirect flag) is AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('<html><form method="post" action="/login"></form></html>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      }),
    );

    const result = await apiGet('/api/recruitment_diag/difficulty');

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
  });

  it('case 2c: other non-JSON 2xx (html without a login form, 204) is ApiInvalidResponseError', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('<html><p>hello</p></html>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      }),
    );
    const html = await apiGet('/api/x');
    expect(html.ok).toBe(false);
    if (!html.ok) {
      expect(html.error).toBeInstanceOf(ApiInvalidResponseError);
      expect(html.error).not.toBeInstanceOf(AuthRequiredError);
    }

    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));
    const empty = await apiGet('/api/x');
    expect(empty.ok).toBe(false);
    if (!empty.ok) expect(empty.error).toBeInstanceOf(ApiInvalidResponseError);
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

describe('apiGet Accept header', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('defaults to application/json and honours the accept option (apiPost never does)', async () => {
    const f = vi.fn<typeof fetch>(() =>
      Promise.resolve(
        new Response('{"a":1}', { status: 200, headers: { 'content-type': 'application/json' } }),
      ),
    );
    vi.stubGlobal('fetch', f);
    await apiGet('/api/x');
    await apiGet('/api/x', { accept: 'text/html' });
    await apiPost('/api/x', {}, { accept: 'text/html' });
    const accepts = f.mock.calls.map((c) => (c[1]?.headers as Record<string, string>).Accept);
    expect(accepts).toEqual(['application/json', 'text/html', 'application/json']);
  });
});

describe('a response that arrives after abort / timeout is not ok', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('caller abort, then the (signal-ignoring) fetch answers: ApiAbortedError', async () => {
    let answer!: (r: Response) => void;
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(
        () =>
          new Promise<Response>((r) => {
            answer = r;
          }),
      ),
    );
    const c = new AbortController();
    const p = apiGet<{ v: number }>('/api/x', { signal: c.signal });
    c.abort();
    answer(
      new Response('{"v":1}', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    const r = await p;
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiAbortedError);
  });

  it('timeout, then the (signal-ignoring) fetch answers: ApiTimeoutError', async () => {
    vi.useFakeTimers();
    let answer!: (r: Response) => void;
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(
        () =>
          new Promise<Response>((r) => {
            answer = r;
          }),
      ),
    );
    const p = apiGet<{ v: number }>('/api/x', { timeoutMs: 1000 });
    await vi.advanceTimersByTimeAsync(1001);
    answer(
      new Response('{"v":1}', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    const r = await p;
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiTimeoutError);
  });
});

describe('timeoutMs: NO_TIMEOUT (待ち時間の上限なし)', () => {
  it('30 日待っても時間切れにならず、呼び出し側の中断は効く', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementationOnce(
      (_input, init) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () => {
            reject(new DOMException('The operation was aborted.', 'AbortError'));
          });
        }),
    );
    const controller = new AbortController();
    let settled: Awaited<ReturnType<typeof apiGet>> | undefined;
    const pending = apiGet('/api/sales-kpi/data', {
      timeoutMs: NO_TIMEOUT,
      signal: controller.signal,
    }).then((r) => (settled = r));

    // setTimeout に Infinity を渡すと即時に発火する (2^31-1 ms 超は 1 ms 扱い)。その事故が無いこと。
    await vi.advanceTimersByTimeAsync(30 * 24 * 60 * 60 * 1000);
    expect(settled).toBeUndefined();

    controller.abort();
    await pending;
    expect(settled?.ok).toBe(false);
    if (settled === undefined || settled.ok) return;
    expect(settled.error).toBeInstanceOf(ApiAbortedError);
  });

  it('遅れて返った応答はそのまま成功になる', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          setTimeout(() => {
            resolve(jsonResponse({ a: 1 }));
          }, 10 * 60 * 1000);
        }),
    );
    const pending = apiGet<{ a: number }>('/api/sales-kpi/data', { timeoutMs: NO_TIMEOUT });
    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);
    const r = await pending;
    expect(r).toEqual({ ok: true, data: { a: 1 } });
  });
});
