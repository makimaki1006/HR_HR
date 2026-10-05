import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ApiAbortedError,
  ApiHttpError,
  ApiNetworkError,
  ApiTimeoutError,
  AuthRequiredError,
} from '../../api/client';
import {
  PDF_TIMEOUT_MS,
  REPORT_TIMEOUT_MS,
  describeFailure,
  downloadPdf,
  filenameFromDisposition,
  isCompletePdf,
} from './api';

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const GOOD_PDF = enc('%PDF-1.7\n1 0 obj\n<<>>\nendobj\n%%EOF\n');

const pdfResponse = (
  body: Uint8Array | string,
  init: { status?: number; type?: string; disposition?: string } = {},
): Response => {
  const headers: Record<string, string> = {
    'content-type': init.type ?? 'application/pdf',
  };
  if (init.disposition !== undefined) headers['content-disposition'] = init.disposition;
  return new Response(body as BodyInit, { status: init.status ?? 200, headers });
};
const jsonResponse = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

const stubFetch = (fn: typeof fetch): ReturnType<typeof vi.fn<typeof fetch>> => {
  const mock = vi.fn<typeof fetch>(fn);
  vi.stubGlobal('fetch', mock);
  return mock;
};

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('タイムアウトの定数', () => {
  it('PDF は 180 秒、レポート作成は Google 待ちを含めて 120 秒以上', () => {
    expect(PDF_TIMEOUT_MS).toBe(180_000);
    expect(REPORT_TIMEOUT_MS).toBeGreaterThanOrEqual(120_000);
  });
});

describe('isCompletePdf (旧画面と同じ検査: 先頭 %PDF- と末尾 32 バイトの %%EOF)', () => {
  it('正常', () => {
    expect(isCompletePdf(GOOD_PDF)).toBe(true);
  });
  it('先頭が違う', () => {
    expect(isCompletePdf(enc('<html>%%EOF'))).toBe(false);
  });
  it('末尾が切れている (途中切断)', () => {
    expect(isCompletePdf(enc('%PDF-1.7\n' + 'x'.repeat(500)))).toBe(false);
  });
  it('%%EOF が末尾 32 バイトより前にあるだけなら不完全', () => {
    expect(isCompletePdf(enc('%PDF-1.7\n%%EOF\n' + 'y'.repeat(100)))).toBe(false);
  });
  it('空・極小', () => {
    expect(isCompletePdf(new Uint8Array(0))).toBe(false);
    expect(isCompletePdf(enc('%PDF'))).toBe(false);
  });
});

describe('filenameFromDisposition', () => {
  it('filename* (RFC 5987) を優先して日本語を復元する', () => {
    const h = `attachment; filename="competitor-report-2026-10-04.pdf"; filename*=UTF-8''${encodeURIComponent('競合調査_大阪府_施設長_2026-10-04.pdf')}`;
    expect(filenameFromDisposition(h)).toBe('競合調査_大阪府_施設長_2026-10-04.pdf');
  });
  it('filename* が壊れていれば filename= に落とす', () => {
    const h = `attachment; filename="competitor-report-2026-10-04.pdf"; filename*=UTF-8''%E7%ZZ`;
    expect(filenameFromDisposition(h)).toBe('competitor-report-2026-10-04.pdf');
  });
  it('どちらも無ければ既定名', () => {
    expect(filenameFromDisposition(null)).toBe('competitor-report.pdf');
    expect(filenameFromDisposition('attachment')).toBe('competitor-report.pdf');
  });
  it('パス区切り・制御文字を落とす (保存先の乗っ取り防止)', () => {
    const h = `attachment; filename*=UTF-8''${encodeURIComponent('../..\\evil\u0000.pdf')}`;
    const name = filenameFromDisposition(h);
    expect(name.includes('/')).toBe(false);
    expect(name.includes(String.fromCharCode(92))).toBe(false);
    expect(Array.from(name).some((c) => c.charCodeAt(0) < 32)).toBe(false);
    expect(name.endsWith('.pdf')).toBe(true);
  });
});

describe('downloadPdf', () => {
  it('report_id を JSON で POST し、PDF と filename* の名前を返す', async () => {
    const f = stubFetch(() =>
      Promise.resolve(
        pdfResponse(GOOD_PDF, {
          disposition: `attachment; filename="competitor-report-2026-10-04.pdf"; filename*=UTF-8''${encodeURIComponent('競合調査_大阪府_2026-10-04.pdf')}`,
        }),
      ),
    );
    const r = await downloadPdf('abc123');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.filename).toBe('競合調査_大阪府_2026-10-04.pdf');
    expect(r.blob.size).toBe(GOOD_PDF.length);
    expect(r.blob.type).toBe('application/pdf');
    const [url, init] = f.mock.calls[0] ?? [];
    expect(url).toBe('/api/competitor/pdf');
    expect(init?.method).toBe('POST');
    expect(init?.body).toBe(JSON.stringify({ report_id: 'abc123' }));
    const headers = init?.headers as Record<string, string>;
    expect(headers['X-Requested-With']).toBe('fetch');
    expect(headers['Content-Type']).toBe('application/json');
  });

  it('不完全な PDF (末尾 %%EOF が無い) は保存せず incomplete', async () => {
    stubFetch(() => Promise.resolve(pdfResponse(enc('%PDF-1.7\n' + 'x'.repeat(300)))));
    const r = await downloadPdf('abc123');
    expect(r).toMatchObject({ ok: false, kind: 'incomplete' });
  });

  it('200 でも content-type が PDF でなければ失敗 (HTML のログイン画面など)', async () => {
    stubFetch(() => Promise.resolve(pdfResponse('<html></html>', { type: 'text/html' })));
    const r = await downloadPdf('abc123');
    expect(r).toMatchObject({ ok: false, kind: 'incomplete' });
  });

  it('404 は「もう一度作成してください」(期限切れ)', async () => {
    stubFetch(() =>
      Promise.resolve(
        jsonResponse(404, { error: 'report_not_found', message: 'レポートが見つかりません。' }),
      ),
    );
    const r = await downloadPdf('abc123');
    expect(r).toMatchObject({ ok: false, kind: 'expired' });
    if (!r.ok) expect(r.message).toContain('もう一度作成してください');
  });

  it('401 は auth', async () => {
    stubFetch(() => Promise.resolve(jsonResponse(401, { error: 'auth_required' })));
    expect(await downloadPdf('abc123')).toMatchObject({ ok: false, kind: 'auth' });
  });

  it('429 は他の作成中 (in_progress)', async () => {
    stubFetch(() =>
      Promise.resolve(jsonResponse(429, { error: 'report_in_progress', message: '作成中' })),
    );
    expect(await downloadPdf('abc123')).toMatchObject({ ok: false, kind: 'in_progress' });
  });

  it.each([
    [503, 'pdf_busy', 'busy'],
    [504, 'pdf_timeout', 'timeout'],
    [503, 'pdf_failed', 'server'],
  ] as const)('%i %s は kind=%s', async (status, code, kind) => {
    stubFetch(() => Promise.resolve(jsonResponse(status, { error: code, message: '固定文' })));
    const r = await downloadPdf('abc123');
    expect(r).toMatchObject({ ok: false, kind });
  });

  it('本文が JSON でない 502 でも落ちない', async () => {
    stubFetch(() => Promise.resolve(new Response('Bad Gateway', { status: 502 })));
    const r = await downloadPdf('abc123');
    expect(r).toMatchObject({ ok: false, kind: 'server' });
  });

  it('180 秒で打ち切って timeout (クライアント側)', async () => {
    vi.useFakeTimers();
    stubFetch(
      (_url, init) =>
        new Promise<Response>((_res, rej) => {
          init?.signal?.addEventListener('abort', () => {
            rej(new DOMException('aborted', 'AbortError'));
          });
        }),
    );
    const p = downloadPdf('abc123');
    await vi.advanceTimersByTimeAsync(PDF_TIMEOUT_MS - 1);
    let settled = false;
    void p.then(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    const r = await p;
    expect(r).toMatchObject({ ok: false, kind: 'timeout' });
    if (!r.ok) expect(r.message).toContain('180');
  });

  it('呼び出し側の中断は aborted', async () => {
    stubFetch(
      (_url, init) =>
        new Promise<Response>((_res, rej) => {
          init?.signal?.addEventListener('abort', () => {
            rej(new DOMException('aborted', 'AbortError'));
          });
        }),
    );
    const c = new AbortController();
    const p = downloadPdf('abc123', c.signal);
    c.abort();
    expect(await p).toMatchObject({ ok: false, kind: 'aborted' });
  });

  it('通信失敗は network', async () => {
    stubFetch(() => Promise.reject(new TypeError('Failed to fetch')));
    expect(await downloadPdf('abc123')).toMatchObject({ ok: false, kind: 'network' });
  });
});

describe('describeFailure (レポート作成のエラー)', () => {
  const http = (status: number, body?: unknown): ApiHttpError => new ApiHttpError(status, body);

  it('401 は auth、入力を残してログインを促す文言', () => {
    const f = describeFailure(new AuthRequiredError('x'), 'report');
    expect(f.kind).toBe('auth');
    expect(f.message).toContain('ログイン');
    expect(f.message).toContain('入力');
  });

  it('429 report_in_progress は in_progress', () => {
    const f = describeFailure(
      http(429, { error: 'report_in_progress', message: '前のレポートを作成中です。' }),
      'report',
    );
    expect(f.kind).toBe('in_progress');
    expect(f.message).toContain('作成中');
  });

  it('本文が無い 429 でも in_progress', () => {
    expect(describeFailure(http(429), 'report').kind).toBe('in_progress');
  });

  it.each([
    ['csv_missing', 400],
    ['csv_unreadable', 400],
    ['csv_too_large', 413],
    ['csv_too_many_rows', 422],
    ['csv_parse_failed', 422],
    ['no_indeed_jobs', 422],
    ['field_too_long', 400],
    ['invalid_source_type', 400],
    ['invalid_wage_mode', 400],
    ['invalid_prefecture', 400],
  ] as const)('%s は kind=input でコードを保持し、固定文を出す', (code, status) => {
    const f = describeFailure(http(status, { error: code, message: 'サーバの固定文 ' + code }), 'report');
    expect(f.kind).toBe('input');
    expect(f.code).toBe(code);
    expect(f.message).toContain('サーバの固定文 ' + code);
  });

  it('コードは知っているがメッセージ欠落ならクライアントの固定文', () => {
    const f = describeFailure(http(422, { error: 'no_indeed_jobs' }), 'report');
    expect(f.message).toContain('Indeed');
  });

  it('知らないコード・本文なしの 500 は汎用文 (内部語を出さない)', () => {
    const f = describeFailure(http(500, { error: 'weird', message: 'panicked at src/x.rs' }), 'report');
    expect(f.kind).toBe('server');
    expect(f.message).not.toContain('panicked');
  });

  it('タイムアウト・中断・通信失敗', () => {
    expect(describeFailure(new ApiTimeoutError(150_000), 'report').kind).toBe('timeout');
    expect(describeFailure(new ApiAbortedError('x'), 'report').kind).toBe('aborted');
    expect(describeFailure(new ApiNetworkError('x'), 'report').kind).toBe('network');
  });
});
