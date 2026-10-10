import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiDataError, ApiHttpError, AuthRequiredError } from '../../api/client';
import type { ExtractResponse } from '../../generated/ExtractResponse';
import fixtures from '../../generated/jobgen/fixtures.json';
import { JOBGEN_TIMEOUT_MS, postJson } from './api';

type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

let fetchMock: FetchMock;

beforeEach(() => {
  fetchMock = vi.fn<typeof fetch>();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('postJson (/api/jobgen/*)', () => {
  it.each([
    [429, '利用が混み合っています。少し待ってから再実行してください。'],
    [413, 'ファイルが大きすぎます。求人ごとにファイルを分けて取り込んでください。'],
  ])('失敗 %s は内部応答を見せず次の操作を案内する', async (status, message) => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ message: 'Gemini HTTP error: private-id-123' }, status));
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.message).toBe(message);
  });
  it('POST JSON + X-Requested-With: fetch + same-origin cookie で送り、応答 JSON をそのまま返す', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(fixtures.responses.extract));
    const body = { source_text: fixtures.source_text };

    const r = await postJson<ExtractResponse>('/api/jobgen/extract', body);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe('/api/jobgen/extract');
    expect(init?.method).toBe('POST');
    expect(init?.credentials).toBe('same-origin');
    expect(init?.headers).toEqual({
      'Content-Type': 'application/json',
      Accept: 'application/json',
      'X-Requested-With': 'fetch',
    });
    expect(init?.body).toBe(JSON.stringify(body));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.data.facts.salary?.value).toBe('月給192,000円〜195,000円');
    expect(r.data.facts.insurance?.status).toBe('rejected');
  });

  it('200 でも status:"error" なら ApiDataError (message は本物のハンドラの文言)', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(fixtures.responses.error));
    const r = await postJson('/api/jobgen/extract', { source_text: '' });
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(ApiDataError);
    expect(r.error.message).toBe('処理を完了できませんでした。入力した資料を確認し、もう一度お試しください。');
  });

  it('非 2xx: JSON に message があればそれ、無ければ "HTTP <status>" (旧 postJSON と同じ)', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ message: 'Gemini 429' }, 502));
    const r1 = await postJson('/api/jobgen/analyze', {});
    expect(r1.ok).toBe(false);
    if (r1.ok) return;
    expect(r1.error).toBeInstanceOf(ApiHttpError);
    expect(r1.error.message).toBe('処理を完了できませんでした。少し待ってから再実行してください。');
    expect((r1.error as ApiHttpError).status).toBe(502);

    fetchMock.mockResolvedValueOnce(
      new Response('Forbidden: CSRF', { status: 403, headers: { 'content-type': 'text/plain' } }),
    );
    const r2 = await postJson('/api/jobgen/analyze', {});
    expect(r2.ok).toBe(false);
    if (r2.ok) return;
    expect(r2.error.message).toBe('この操作を実行できません。再度ログインしてお試しください。');
  });

  it('未ログイン (303 → /login の HTML) は AuthRequiredError', async () => {
    const res = new Response('<html>ログイン</html>', {
      status: 200,
      headers: { 'content-type': 'text/html' },
    });
    Object.defineProperty(res, 'redirected', { value: true });
    Object.defineProperty(res, 'url', { value: 'http://localhost:9216/login' });
    fetchMock.mockResolvedValueOnce(res);
    const r = await postJson('/api/jobgen/extract', {});
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.error).toBeInstanceOf(AuthRequiredError);
  });

  // 共通 client 経由になり signal は付くが、client 既定の 15 秒では切らない
  // (Gemini 生成は 1 分を超える)。120 秒待っても完了できることは api.client.test.ts で確認。
  it('client の既定 15 秒より長い待ち時間 (JOBGEN_TIMEOUT_MS) を使う', () => {
    expect(JOBGEN_TIMEOUT_MS).toBeGreaterThan(5 * 60 * 1000);
  });
});
