// jobgen postJson goes through the shared client (apiPost): CSRF header, 401, error body.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiDataError, ApiHttpError, AuthRequiredError } from '../../api/client';
import { postJson } from './api';

type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;
const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

let fetchMock: FetchMock;
beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>();
  vi.stubGlobal('fetch', fetchMock);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('jobgen postJson via the shared client', () => {
  it('HTTP 401 (JSON) is AuthRequiredError, not ApiHttpError', async () => {
    fetchMock.mockResolvedValueOnce(json({ error: 'auth_required', login_url: '/login' }, 401));
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(AuthRequiredError);
    expect(r.error.message).toBe('ログインの有効期限が切れました。もう一度ログインしてください');
  });

  it('a non-2xx JSON body is kept in ApiHttpError.body and its message is shown', async () => {
    fetchMock.mockResolvedValueOnce(json({ message: 'Gemini 429', retry_after: 7 }, 502));
    const r = await postJson('/api/jobgen/analyze', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiHttpError);
    expect((r.error as ApiHttpError).status).toBe(502);
    expect((r.error as ApiHttpError).body).toEqual({ message: 'Gemini 429', retry_after: 7 });
    expect(r.error.message).toBe('Gemini 429');
  });

  it('200 with {error: "..."} keeps the body message (message wins over error), body in ApiDataError.body', async () => {
    fetchMock.mockResolvedValueOnce(json({ error: 'code_x', message: '本文のメッセージ' }));
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiDataError);
    expect(r.error.message).toBe('本文のメッセージ');
    expect((r.error as ApiDataError).body).toEqual({ error: 'code_x', message: '本文のメッセージ' });
  });

  it('200 with status:"error" (no error key) is still an ApiDataError', async () => {
    fetchMock.mockResolvedValueOnce(json({ status: 'error', message: 'source_text が必要です' }));
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiDataError);
    expect(r.error.message).toBe('source_text が必要です');
  });

  it('a 200 text/html login form (no redirect flag) is AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('<form action="/login" method="post"></form>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      }),
    );
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(AuthRequiredError);
    expect(r.error.message).toBe('ログインの有効期限が切れました。もう一度ログインしてください');
  });

  it('has no 15 s default timeout (Gemini calls can exceed a minute)', async () => {
    vi.useFakeTimers();
    let resolve!: (r: Response) => void;
    let signal: AbortSignal | undefined;
    // Like the real fetch: rejects as soon as its signal is aborted.
    fetchMock.mockImplementationOnce(
      (_url, init) =>
        new Promise<Response>((res, rej) => {
          resolve = res;
          signal = init?.signal ?? undefined;
          signal?.addEventListener('abort', () => {
            rej(new DOMException('aborted', 'AbortError'));
          });
        }),
    );
    const p = postJson<{ status: string }>('/api/jobgen/ab', {});
    await vi.advanceTimersByTimeAsync(120_000);
    expect(signal?.aborted).toBe(false);
    resolve(json({ status: 'ok' }));
    const r = await p;
    expect(r).toEqual({ ok: true, data: { status: 'ok' } });
  });
});
