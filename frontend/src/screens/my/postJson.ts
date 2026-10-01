// The one POST helper for the my screen (W8).
// TODO(platform-team): replace with `client.post` once the shared client grows a POST.
//
// Contract (same as apiGet in src/api/client.ts):
// - session cookie only (`credentials: 'same-origin'`), never a token
// - `X-Requested-With: fetch` is required by the Rust CSRF plan for React POSTs
// - redirect to /login or a non-JSON body => AuthRequiredError; non-2xx => ApiHttpError
import {
  ApiDataError,
  ApiError,
  ApiHttpError,
  ApiInvalidResponseError,
  ApiNetworkError,
  AuthRequiredError,
  type ApiResult,
} from '../../api/client';

export const FETCH_MARKER_HEADER = 'X-Requested-With';
export const FETCH_MARKER_VALUE = 'fetch';

function isJsonContentType(res: Response): boolean {
  const ct = res.headers.get('content-type') ?? '';
  return /^application\/(?:[\w.+-]+\+)?json\b/i.test(ct.trim());
}

function redirectedToLogin(res: Response): boolean {
  if (!res.redirected || res.url === '') return false;
  try {
    return new URL(res.url).pathname === '/login';
  } catch {
    return false;
  }
}

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
        Accept: 'application/json',
        'Content-Type': 'application/json',
        [FETCH_MARKER_HEADER]: FETCH_MARKER_VALUE,
      },
      body: JSON.stringify(body),
    });
  } catch (e) {
    return { ok: false, error: new ApiNetworkError(e instanceof Error ? e.message : String(e)) };
  }
  if (redirectedToLogin(res)) {
    return { ok: false, error: new AuthRequiredError('login required (redirected to /login)') };
  }
  if (!res.ok) return { ok: false, error: new ApiHttpError(res.status) };
  if (!isJsonContentType(res)) {
    return {
      ok: false,
      error: new AuthRequiredError(
        `login required (non-JSON response: ${res.headers.get('content-type') ?? 'none'})`,
      ),
    };
  }
  let parsed: unknown;
  try {
    parsed = await res.json();
  } catch (e) {
    return {
      ok: false,
      error: new ApiInvalidResponseError(e instanceof Error ? e.message : String(e)),
    };
  }
  if (typeof parsed === 'object' && parsed !== null && 'error' in parsed) {
    const err: unknown = parsed.error;
    if (typeof err === 'string' && err !== '') {
      return { ok: false, error: new ApiDataError(err, parsed) };
    }
  }
  return { ok: true, data: parsed as T };
}
