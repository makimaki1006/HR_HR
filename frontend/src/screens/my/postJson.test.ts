// my postJson goes through the shared client (apiPost): 401 and the error body.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError, AuthRequiredError } from '../../api/client';
import { postJson } from './postJson';

type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;
const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

let fetchMock: FetchMock;
beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>();
  vi.stubGlobal('fetch', fetchMock);
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('my postJson via the shared client', () => {
  it('HTTP 401 is AuthRequiredError, not ApiHttpError', async () => {
    fetchMock.mockResolvedValueOnce(json({ error: 'auth_required', login_url: '/login' }, 401));
    const r = await postJson('/api/my/profile', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(AuthRequiredError);
  });

  it('403 / 409 JSON bodies are kept in ApiHttpError.body, message stays "HTTP <status>"', async () => {
    fetchMock.mockResolvedValueOnce(json({ message: '権限がありません' }, 403));
    const r1 = await postJson('/api/my/profile', {});
    expect(r1.ok).toBe(false);
    if (r1.ok) return;
    expect(r1.error).toBeInstanceOf(ApiHttpError);
    expect((r1.error as ApiHttpError).status).toBe(403);
    expect((r1.error as ApiHttpError).body).toEqual({ message: '権限がありません' });
    expect(r1.error.message).toBe('HTTP 403');

    fetchMock.mockResolvedValueOnce(json({ code: 'conflict' }, 409));
    const r2 = await postJson('/api/my/profile', {});
    expect(r2.ok).toBe(false);
    if (r2.ok) return;
    expect((r2.error as ApiHttpError).body).toEqual({ code: 'conflict' });
  });

  it('a 200 text/html login form (no redirect flag) is AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('<form action="/login" method="post"></form>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      }),
    );
    const r = await postJson('/api/my/profile', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(AuthRequiredError);
  });
});
