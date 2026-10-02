// /api/jobgen/* への POST (JSON)。
//
// 共通 client の apiPost 経由 (CSRF ヘッダー X-Requested-With: fetch・401 → AuthRequiredError・
// エラー本文 ApiHttpError.body)。
//
// 旧 static/jobgen.html の `postJSON` と同じ失敗判定:
// - HTTP 2xx 以外        → message = body.message || body.error || 'HTTP <status>'
// - 200 で status:'error' → message = body.message || body.error || 'サーバエラー'
// 違い: 未ログイン (303 → /login の HTML / HTTP 401) は AuthRequiredError にする (旧は null を返して
// 呼び出し側で TypeError になっていた)。Gemini 生成は 1 分を超えることがあるので
// タイムアウトは実質掛けない (30 分、旧は無制限)。
import { ApiDataError, ApiHttpError, AuthRequiredError, type ApiResult, apiPost } from '../../api/client';
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

/** Gemini 生成は 1 分を超えることがある。旧は無制限。client の既定 15 秒を避けるため長めに取る。 */
export const JOBGEN_TIMEOUT_MS = 30 * 60 * 1000;

/** 未ログイン (401 / ログイン画面へのリダイレクト) のとき利用者に出す文言。 */
export const AUTH_REQUIRED_MESSAGE = 'ログインの有効期限が切れました。もう一度ログインしてください';

/** 同一オリジンの JSON POST (共通 client の apiPost 経由)。例外は投げず `ApiResult` で返す。 */
export async function postJson<T>(path: string, body: unknown): Promise<ApiResult<T>> {
  const r = await apiPost<T>(path, body, { timeoutMs: JOBGEN_TIMEOUT_MS });
  if (!r.ok) {
    const e = r.error;
    if (e instanceof AuthRequiredError) {
      // client の文言は英語 (login required ...)。利用者に出る文言は日本語にする。
      return { ok: false, error: new AuthRequiredError(AUTH_REQUIRED_MESSAGE) };
    }
    if (e instanceof ApiHttpError) {
      // 旧: (d && (d.message||d.error)) || ('HTTP '+status)。JSON でない本文は 'HTTP <status>'。
      const err = new ApiHttpError(e.status, e.body);
      err.message = messageFromBody(e.body) ?? `HTTP ${String(e.status)}`;
      return { ok: false, error: err };
    }
    if (e instanceof ApiDataError) {
      // client は error キーの文字列をそのままメッセージにするが、旧は message を優先する。
      return { ok: false, error: new ApiDataError(messageFromBody(e.body) ?? e.message, e.body) };
    }
    return r;
  }
  if (isErrorBody(r.data)) {
    // 200 で status:'error' (error キー無し)。client は成功扱いにするのでここで拾う。
    return {
      ok: false,
      error: new ApiDataError(messageFromBody(r.data) ?? 'サーバエラー', r.data),
    };
  }
  return r;
}

/** 型付きの送信関数 (本番用)。 */
export const postJobgen: PostFn = (path, body) => postJson(path, body);
