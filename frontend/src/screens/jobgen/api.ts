// /api/jobgen/* への POST (JSON)。
//
// TODO(platform): platform-team の `client.post` が提供されたら postJson をそれに置き換える。
// それまでは画面内にこの 1 関数だけを置く (wave-d 共通指示)。
//
// 旧 static/jobgen.html の `postJSON` と同じ失敗判定:
// - HTTP 2xx 以外        → message = body.message || body.error || 'HTTP <status>'
// - 200 で status:'error' → message = body.message || body.error || 'サーバエラー'
// 違い: 未ログイン (303 → /login の HTML) は AuthRequiredError にする (旧は null を返して
// 呼び出し側で TypeError になっていた)。Gemini 生成は 1 分を超えることがあるので
// タイムアウトは掛けない (旧と同じ)。
import {
  ApiDataError,
  ApiError,
  ApiHttpError,
  ApiNetworkError,
  type ApiResult,
  AuthRequiredError,
} from '../../api/client';
import type { AbRequest } from '../../generated/AbRequest';
import type { AbResponse } from '../../generated/AbResponse';
import type { AnalyzeRequest } from '../../generated/AnalyzeRequest';
import type { AnalyzeResponse } from '../../generated/AnalyzeResponse';
import type { CopyRequest } from '../../generated/CopyRequest';
import type { CopyResponse } from '../../generated/CopyResponse';
import type { ExtractRequest } from '../../generated/ExtractRequest';
import type { ExtractResponse } from '../../generated/ExtractResponse';
import type { HrhackerRequest } from '../../generated/HrhackerRequest';
import type { HrhackerResponse } from '../../generated/HrhackerResponse';
import type { ImagePromptsRequest } from '../../generated/ImagePromptsRequest';
import type { ImagePromptsResponse } from '../../generated/ImagePromptsResponse';
import type { ImagesRequest } from '../../generated/ImagesRequest';
import type { ImagesResponse } from '../../generated/ImagesResponse';
import type { MobileRequest } from '../../generated/MobileRequest';
import type { MobileResponse } from '../../generated/MobileResponse';
import type { NormalizeRequest } from '../../generated/NormalizeRequest';
import type { NormalizeResponse } from '../../generated/NormalizeResponse';
import type { PersonasRequest } from '../../generated/PersonasRequest';
import type { PersonasResponse } from '../../generated/PersonasResponse';

/** 画面が呼ぶ API と要求/応答の対応。キーがパス。 */
export interface JobgenEndpoints {
  '/api/jobgen/normalize': { req: NormalizeRequest; res: NormalizeResponse };
  '/api/jobgen/extract': { req: ExtractRequest; res: ExtractResponse };
  '/api/jobgen/analyze': { req: AnalyzeRequest; res: AnalyzeResponse };
  '/api/jobgen/personas': { req: PersonasRequest; res: PersonasResponse };
  '/api/jobgen/copy': { req: CopyRequest; res: CopyResponse };
  '/api/jobgen/images': { req: ImagesRequest; res: ImagesResponse };
  '/api/jobgen/image_prompts': { req: ImagePromptsRequest; res: ImagePromptsResponse };
  '/api/jobgen/mobile': { req: MobileRequest; res: MobileResponse };
  '/api/jobgen/hrhacker': { req: HrhackerRequest; res: HrhackerResponse };
  '/api/jobgen/ab': { req: AbRequest; res: AbResponse };
}

export type JobgenPath = keyof JobgenEndpoints;

/** パイプラインが使う送信関数の形 (テストではモックに差し替える)。 */
export type PostFn = <P extends JobgenPath>(
  path: P,
  body: JobgenEndpoints[P]['req'],
) => Promise<ApiResult<JobgenEndpoints[P]['res']>>;

function messageFromBody(body: unknown): string | null {
  if (typeof body !== 'object' || body === null) return null;
  const rec = body as Record<string, unknown>;
  for (const k of ['message', 'error']) {
    const v = rec[k];
    if (typeof v === 'string' && v !== '') return v;
    if (v !== undefined && v !== null && v !== false && v !== '') return JSON.stringify(v);
  }
  return null;
}

function isErrorBody(body: unknown): boolean {
  if (typeof body !== 'object' || body === null) return false;
  const rec = body as Record<string, unknown>;
  const err = rec.error;
  if (err !== undefined && err !== null && err !== false && err !== '') return true;
  return rec.status === 'error';
}

function tryParseJson(text: string): { ok: true; value: unknown } | { ok: false } {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch {
    return { ok: false };
  }
}

function redirectedToLogin(res: Response): boolean {
  if (!res.redirected || res.url === '') return false;
  try {
    return new URL(res.url).pathname === '/login';
  } catch {
    return false;
  }
}

/** 同一オリジンの JSON POST。例外は投げず `ApiResult` で返す。 */
export async function postJson<T>(path: string, body: unknown): Promise<ApiResult<T>> {
  if (!path.startsWith('/') || path.startsWith('//')) {
    return { ok: false, error: new ApiError(`path must be same-origin absolute: ${path}`) };
  }
  let res: Response;
  try {
    res = await fetch(path, {
      method: 'POST',
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
        // CSRF: React からの POST はこのヘッダーを必須にする (全チーム共通の決まり)。
        'X-Requested-With': 'fetch',
      },
      body: JSON.stringify(body),
    });
  } catch (e) {
    return { ok: false, error: new ApiNetworkError(e instanceof Error ? e.message : String(e)) };
  }
  if (redirectedToLogin(res)) {
    return { ok: false, error: new AuthRequiredError('ログインが必要です（セッション切れ）') };
  }
  const text = await res.text().catch(() => '');
  const json = tryParseJson(text);
  const parsed: unknown = json.ok ? json.value : null;
  const isJson = json.ok;
  if (!res.ok) {
    // 旧: (d && (d.message||d.error)) || txt || ('HTTP '+status)。JSON でない本文は旧では
    // 読めなかった (json() 失敗後の text() は空) ので、ここでも 'HTTP <status>' に揃える。
    const msg = messageFromBody(parsed) ?? `HTTP ${String(res.status)}`;
    const err = new ApiHttpError(res.status);
    err.message = msg;
    return { ok: false, error: err };
  }
  if (!isJson) {
    return {
      ok: false,
      error: new AuthRequiredError(
        `ログインが必要です（JSON でない応答: ${res.headers.get('content-type') ?? 'none'}）`,
      ),
    };
  }
  if (isErrorBody(parsed)) {
    return { ok: false, error: new ApiDataError(messageFromBody(parsed) ?? 'サーバエラー', parsed) };
  }
  return { ok: true, data: parsed as T };
}

/** 型付きの送信関数 (本番用)。 */
export const postJobgen: PostFn = (path, body) => postJson(path, body);
