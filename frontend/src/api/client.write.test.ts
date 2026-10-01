import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiAbortedError,
  ApiDataError,
  ApiHttpError,
  ApiInvalidResponseError,
  ApiNetworkError,
  ApiTimeoutError,
  AuthRequiredError,
  apiGet,
  apiPost,
  apiPostForm,
  apiUpload,
  pollJob,
  type UploadProgress,
} from './client';

type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function textResponse(body: string, status = 200): Response {
  return new Response(body, { status, headers: { 'content-type': 'text/html; charset=utf-8' } });
}

function loginPageAfterRedirect(): Response {
  const res = textResponse('<!DOCTYPE html><html><body>ログイン</body></html>');
  Object.defineProperty(res, 'redirected', { value: true });
  Object.defineProperty(res, 'url', { value: 'https://hr-hw.onrender.com/login' });
  return res;
}

/** fetch that never answers on its own; rejects with the abort reason like a real fetch. */
function hangingFetch(_url: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  return new Promise((_resolve, reject) => {
    init?.signal?.addEventListener('abort', () => {
      reject(init.signal?.reason as Error);
    });
  });
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

describe('apiPost', () => {
  it('sends a JSON body with Accept / Content-Type / X-Requested-With and returns parsed JSON', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: 7, status: 'queued' }));

    const result = await apiPost<{ id: number; status: string }>('/api/survey/analyze', {
      session_id: 'abc',
      n: 3,
    });

    expect(result).toEqual({ ok: true, data: { id: 7, status: 'queued' } });
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe('/api/survey/analyze');
    expect(init?.method).toBe('POST');
    expect(init?.credentials).toBe('same-origin');
    expect(init?.body).toBe('{"session_id":"abc","n":3}');
    expect(init?.headers).toEqual({
      Accept: 'application/json',
      'Content-Type': 'application/json',
      'X-Requested-With': 'fetch',
    });
  });

  it('maps 401 {"error":"auth_required"} to AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ error: 'auth_required', login_url: '/login' }, 401),
    );
    const result = await apiPost('/api/x', {});
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (HTTP 401)');
  });

  it('maps a redirect to /login to AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(loginPageAfterRedirect());
    const result = await apiPost('/api/x', {});
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (redirected to /login)');
  });

  it('treats a plain non-JSON 200 as ApiInvalidResponseError and a login form as AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(textResponse('OK'));
    const plain = await apiPost('/api/x', {});
    expect(plain.ok).toBe(false);
    if (!plain.ok) expect(plain.error).toBeInstanceOf(ApiInvalidResponseError);

    fetchMock.mockResolvedValueOnce(
      textResponse('<form action="/login" method="post"><input name="email"></form>'),
    );
    const login = await apiPost('/api/x', {});
    expect(login.ok).toBe(false);
    if (!login.ok) expect(login.error).toBeInstanceOf(AuthRequiredError);
  });

  it('keeps the JSON body of a 503 in ApiHttpError.body (error_kind)', async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ error_kind: 'hubspot_rate_limited', message: 'slow down' }, 503),
    );
    const result = await apiPost('/api/crm/x', {});
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiHttpError);
    const err = result.error as ApiHttpError;
    expect(err.status).toBe(503);
    expect(err.body).toEqual({ error_kind: 'hubspot_rate_limited', message: 'slow down' });
  });

  it('leaves ApiHttpError.body undefined when the error body is not JSON', async () => {
    fetchMock.mockResolvedValueOnce(textResponse('<html>Bad Gateway</html>', 502));
    const result = await apiPost('/api/x', {});
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect((result.error as ApiHttpError).status).toBe(502);
    expect((result.error as ApiHttpError).body).toBeUndefined();
  });

  it('returns ApiDataError for 200 {"error"} and ApiHttpError for 500', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: 'bad input' }));
    const dataErr = await apiPost('/api/x', {});
    expect(dataErr.ok).toBe(false);
    if (!dataErr.ok) expect(dataErr.error).toBeInstanceOf(ApiDataError);

    fetchMock.mockResolvedValueOnce(jsonResponse({}, 500));
    const httpErr = await apiPost('/api/x', {});
    expect(httpErr.ok).toBe(false);
    if (!httpErr.ok) {
      expect(httpErr.error).toBeInstanceOf(ApiHttpError);
      expect((httpErr.error as ApiHttpError).status).toBe(500);
    }
  });

  it('times out with ApiTimeoutError after timeoutMs', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementation(hangingFetch);
    const pending = apiPost('/api/x', {}, { timeoutMs: 500 });
    await vi.advanceTimersByTimeAsync(500);
    const result = await pending;
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiTimeoutError);
    expect(result.error.message).toBe('request timed out after 500 ms');
  });

  it('returns ApiAbortedError when the caller aborts', async () => {
    fetchMock.mockImplementation(hangingFetch);
    const controller = new AbortController();
    const pending = apiPost('/api/x', {}, { signal: controller.signal });
    controller.abort();
    const result = await pending;
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(ApiAbortedError);
  });

  it('rejects non same-origin paths without calling fetch', async () => {
    const result = await apiPost('//evil.example/x', {});
    expect(result.ok).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe('apiGet headers', () => {
  it('adds X-Requested-With: fetch', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ a: 1 }));
    await apiGet('/api/x');
    const [, init] = fetchMock.mock.calls[0] ?? [];
    expect(init?.headers).toEqual({ Accept: 'application/json', 'X-Requested-With': 'fetch' });
  });

  it('maps 401 to AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: 'auth_required', login_url: '/login' }, 401));
    const result = await apiGet('/api/x');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(AuthRequiredError);
  });
});

describe('apiPostForm', () => {
  it('url-encodes a Record and accepts the text body "OK" with expect: text', async () => {
    fetchMock.mockResolvedValueOnce(textResponse('OK'));

    const result = await apiPostForm<string>(
      '/api/set_prefecture',
      { prefecture: '東京都', municipality: 'a&b' },
      { expect: 'text' },
    );

    expect(result).toEqual({ ok: true, data: 'OK' });
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe('/api/set_prefecture');
    expect(init?.method).toBe('POST');
    expect(init?.body).toBe('prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD&municipality=a%26b');
    expect(init?.headers).toEqual({
      Accept: 'application/json',
      'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
      'X-Requested-With': 'fetch',
    });
  });

  it('accepts URLSearchParams', async () => {
    fetchMock.mockResolvedValueOnce(textResponse('OK'));
    await apiPostForm('/api/set_industry', new URLSearchParams([['industry', 'x y']]), {
      expect: 'text',
    });
    const [, init] = fetchMock.mock.calls[0] ?? [];
    expect(init?.body).toBe('industry=x+y');
  });

  it('with expect: text a redirect to /login is still AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(loginPageAfterRedirect());
    const result = await apiPostForm('/api/set_prefecture', { prefecture: 'x' }, { expect: 'text' });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (redirected to /login)');
  });

  it('with expect: text a 401 is AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: 'auth_required' }, 401));
    const result = await apiPostForm('/api/set_prefecture', {}, { expect: 'text' });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(AuthRequiredError);
  });

  it('without expect: text the "OK" body is an invalid response (not a login page)', async () => {
    fetchMock.mockResolvedValueOnce(textResponse('OK'));
    const result = await apiPostForm('/api/set_prefecture', {});
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(ApiInvalidResponseError);
  });
});

/** Minimal XMLHttpRequest stand-in that the test drives by hand. */
class FakeXhr {
  static last: FakeXhr | null = null;
  method = '';
  url = '';
  headers: Record<string, string> = {};
  sent: unknown = null;
  aborted = false;
  responseType = '';
  status = 0;
  responseText = '';
  responseURL = '';
  responseHeaders: Record<string, string> = {};
  upload: { onprogress: ((ev: ProgressEvent) => void) | null } = { onprogress: null };
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onabort: (() => void) | null = null;

  constructor() {
    FakeXhr.last = this;
  }
  open(method: string, url: string): void {
    this.method = method;
    this.url = url;
  }
  setRequestHeader(name: string, value: string): void {
    this.headers[name] = value;
  }
  getResponseHeader(name: string): string | null {
    return this.responseHeaders[name.toLowerCase()] ?? null;
  }
  send(body: unknown): void {
    this.sent = body;
  }
  abort(): void {
    this.aborted = true;
    this.onabort?.();
  }
  respond(status: number, body: string, contentType: string, url = ''): void {
    this.status = status;
    this.responseText = body;
    this.responseURL = url;
    this.responseHeaders = { 'content-type': contentType };
    this.onload?.();
  }
}

function lastXhr(): FakeXhr {
  if (!FakeXhr.last) throw new Error('no XHR created');
  return FakeXhr.last;
}

describe('apiUpload', () => {
  beforeEach(() => {
    FakeXhr.last = null;
    vi.stubGlobal('XMLHttpRequest', FakeXhr);
  });

  function csvForm(): FormData {
    const form = new FormData();
    form.append('file', new Blob(['a,b\n1,2\n']), 'survey.csv');
    return form;
  }

  it('POSTs the FormData with headers, reports progress and resolves with JSON', async () => {
    const progress: UploadProgress[] = [];
    const form = csvForm();
    const pending = apiUpload<{ rows: number }>('/api/survey/upload', form, {
      onProgress: (p) => progress.push(p),
    });
    const xhr = lastXhr();
    expect(xhr.method).toBe('POST');
    expect(xhr.url).toBe('/api/survey/upload');
    expect(xhr.sent).toBe(form);
    expect(xhr.headers).toEqual({ Accept: 'application/json', 'X-Requested-With': 'fetch' });

    xhr.upload.onprogress?.({ lengthComputable: true, loaded: 25, total: 100 } as ProgressEvent);
    xhr.upload.onprogress?.({ lengthComputable: false, loaded: 50, total: 0 } as ProgressEvent);
    xhr.upload.onprogress?.({ lengthComputable: true, loaded: 100, total: 100 } as ProgressEvent);
    xhr.respond(200, '{"rows":42}', 'application/json');

    expect(await pending).toEqual({ ok: true, data: { rows: 42 } });
    expect(progress).toEqual([
      { loaded: 25, total: 100, ratio: 0.25 },
      { loaded: 100, total: 100, ratio: 1 },
    ]);
  });

  it('maps 401 to AuthRequiredError', async () => {
    const pending = apiUpload('/api/survey/upload', csvForm());
    lastXhr().respond(401, '{"error":"auth_required","login_url":"/login"}', 'application/json');
    const result = await pending;
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(AuthRequiredError);
  });

  it('maps a response URL of /login to AuthRequiredError', async () => {
    const pending = apiUpload('/api/survey/upload', csvForm());
    lastXhr().respond(200, '<html></html>', 'text/html', 'https://hr-hw.onrender.com/login');
    const result = await pending;
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(result.error.message).toBe('login required (redirected to /login)');
  });

  it('keeps the JSON body of a 503 upload error and treats a non-JSON 2xx as invalid', async () => {
    const p1 = apiUpload('/api/x', csvForm());
    lastXhr().respond(503, '{"error_kind":"hubspot_rate_limited"}', 'application/json');
    const r1 = await p1;
    expect(r1.ok).toBe(false);
    if (!r1.ok) expect((r1.error as ApiHttpError).body).toEqual({ error_kind: 'hubspot_rate_limited' });

    const p2 = apiUpload('/api/x', csvForm());
    lastXhr().respond(200, 'done', 'text/plain');
    const r2 = await p2;
    expect(r2.ok).toBe(false);
    if (!r2.ok) expect(r2.error).toBeInstanceOf(ApiInvalidResponseError);
  });

  it('maps 413 to ApiHttpError and a network failure to ApiNetworkError', async () => {
    const tooBig = apiUpload('/api/survey/upload', csvForm());
    lastXhr().respond(413, '', 'text/plain');
    const r1 = await tooBig;
    expect(r1.ok).toBe(false);
    if (!r1.ok) expect((r1.error as ApiHttpError).status).toBe(413);

    const broken = apiUpload('/api/survey/upload', csvForm());
    lastXhr().onerror?.();
    const r2 = await broken;
    expect(r2.ok).toBe(false);
    if (!r2.ok) expect(r2.error).toBeInstanceOf(ApiNetworkError);
  });

  it('accepts a text body with expect: text', async () => {
    const pending = apiUpload<string>('/api/x', csvForm(), { expect: 'text' });
    lastXhr().respond(200, 'OK', 'text/html');
    expect(await pending).toEqual({ ok: true, data: 'OK' });
  });

  it('times out with ApiTimeoutError and aborts the XHR', async () => {
    vi.useFakeTimers();
    const pending = apiUpload('/api/x', csvForm(), { timeoutMs: 1000 });
    await vi.advanceTimersByTimeAsync(1000);
    const result = await pending;
    expect(lastXhr().aborted).toBe(true);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(ApiTimeoutError);
  });

  it('returns ApiAbortedError when the caller aborts', async () => {
    const controller = new AbortController();
    const pending = apiUpload('/api/x', csvForm(), { signal: controller.signal });
    controller.abort();
    const result = await pending;
    expect(lastXhr().aborted).toBe(true);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(ApiAbortedError);
  });
});

describe('pollJob', () => {
  interface Job {
    status: 'running' | 'done';
    rows?: number;
  }

  it('polls at intervalMs until isDone is true', async () => {
    vi.useFakeTimers();
    fetchMock
      .mockResolvedValueOnce(jsonResponse({ status: 'running' }))
      .mockResolvedValueOnce(jsonResponse({ status: 'running' }))
      .mockResolvedValueOnce(jsonResponse({ status: 'done', rows: 9 }));

    const pending = pollJob<Job>('/api/job/1', {
      intervalMs: 1000,
      timeoutMs: 60_000,
      isDone: (b) => b.status === 'done',
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(999);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1000);

    expect(await pending).toEqual({ ok: true, data: { status: 'done', rows: 9 } });
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('stops with the request error (401) without further polling', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: 'auth_required' }, 401));
    const result = await pollJob<Job>('/api/job/1', {
      intervalMs: 10,
      timeoutMs: 1000,
      isDone: () => false,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(AuthRequiredError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('gives ApiTimeoutError when the overall timeout elapses', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementation(() => Promise.resolve(jsonResponse({ status: 'running' })));
    const pending = pollJob<Job>('/api/job/1', {
      intervalMs: 1000,
      timeoutMs: 2500,
      isDone: (b) => b.status === 'done',
    });
    await vi.advanceTimersByTimeAsync(2500);
    const result = await pending;
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBeInstanceOf(ApiTimeoutError);
    expect((result.error as ApiTimeoutError).timeoutMs).toBe(2500);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it('gives ApiAbortedError when the caller aborts during the wait', async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementation(() => Promise.resolve(jsonResponse({ status: 'running' })));
    const controller = new AbortController();
    const pending = pollJob<Job>('/api/job/1', {
      intervalMs: 5000,
      timeoutMs: 60_000,
      isDone: () => false,
      signal: controller.signal,
    });
    await vi.advanceTimersByTimeAsync(100);
    controller.abort();
    const result = await pending;
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeInstanceOf(ApiAbortedError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
