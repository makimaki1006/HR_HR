// Same-origin JSON API client for the Rust (Axum) backend.
//
// Contract with the current backend (see claudedocs plan §3 C-1 / C-3):
// - Auth is a session cookie managed by Rust. This client never reads or stores
//   cookies / tokens; it only sends them with `credentials: 'same-origin'`.
// - When the session is missing, Rust answers 303 -> /login (HTML), even for /api/*.
//   fetch follows the redirect and yields 200 text/html, so we map
//   "redirected to /login" and "non-JSON body" to AuthRequiredError.
// - Some handlers answer HTTP 200 with `{"error": "..."}`; that becomes ApiDataError.

export const DEFAULT_TIMEOUT_MS = 15_000;

export class ApiError extends Error {
  override name = 'ApiError';
}

/** Session missing or expired: the request ended up on the login page. */
export class AuthRequiredError extends ApiError {
  override name = 'AuthRequiredError';
}

/** HTTP 200 whose JSON body carries `{"error": "..."}`. */
export class ApiDataError extends ApiError {
  override name = 'ApiDataError';
  readonly body: unknown;

  constructor(message: string, body: unknown) {
    super(message);
    this.body = body;
  }
}

/** Non-2xx HTTP status (other than the login redirect). */
export class ApiHttpError extends ApiError {
  override name = 'ApiHttpError';
  readonly status: number;

  constructor(status: number) {
    super(`HTTP ${String(status)}`);
    this.status = status;
  }
}

/** Response declared JSON but could not be parsed. */
export class ApiInvalidResponseError extends ApiError {
  override name = 'ApiInvalidResponseError';
}

export class ApiTimeoutError extends ApiError {
  override name = 'ApiTimeoutError';
  readonly timeoutMs: number;

  constructor(timeoutMs: number) {
    super(`request timed out after ${String(timeoutMs)} ms`);
    this.timeoutMs = timeoutMs;
  }
}

/** Cancelled by the caller's AbortSignal (e.g. component unmount). */
export class ApiAbortedError extends ApiError {
  override name = 'ApiAbortedError';
}

/** fetch itself failed (offline, DNS, connection reset). */
export class ApiNetworkError extends ApiError {
  override name = 'ApiNetworkError';
}

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

export interface ApiGetOptions {
  /** Defaults to DEFAULT_TIMEOUT_MS. */
  timeoutMs?: number;
  /** Caller-owned cancellation (e.g. from a React effect cleanup). */
  signal?: AbortSignal;
}

const LOGIN_PATH = '/login';

function isSameOriginPath(path: string): boolean {
  // Only absolute paths on this origin; rejects "//host" and full URLs so that
  // the session cookie is never attached to a foreign origin by mistake.
  return path.startsWith('/') && !path.startsWith('//');
}

function redirectedToLogin(res: Response): boolean {
  if (!res.redirected || res.url === '') return false;
  try {
    return new URL(res.url).pathname === LOGIN_PATH;
  } catch {
    return false;
  }
}

function isJsonContentType(res: Response): boolean {
  const ct = res.headers.get('content-type') ?? '';
  return /^application\/(?:[\w.+-]+\+)?json\b/i.test(ct.trim());
}

function extractDataError(body: unknown): string | null {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return null;
  if (!('error' in body)) return null;
  const value: unknown = body.error;
  if (value === null || value === undefined || value === false) return null;
  return typeof value === 'string' ? value : JSON.stringify(value);
}

/**
 * GET a JSON endpoint on the same origin. Never throws; all failures are
 * returned as `{ ok: false, error }` with a specific ApiError subclass.
 *
 * `T` is not validated at runtime; it is trusted to match the Rust contract
 * (generated types arrive in Phase 0-4).
 */
export async function apiGet<T>(path: string, options: ApiGetOptions = {}): Promise<ApiResult<T>> {
  if (!isSameOriginPath(path)) {
    return { ok: false, error: new ApiError(`path must be same-origin absolute: ${path}`) };
  }

  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const controller = new AbortController();
  const timeoutError = new ApiTimeoutError(timeoutMs);
  const timer = setTimeout(() => {
    controller.abort(timeoutError);
  }, timeoutMs);
  // Abort reason tells timeout and caller cancellation apart.
  const abortedResult = (): ApiResult<never> | null => {
    if (!controller.signal.aborted) return null;
    return controller.signal.reason === timeoutError
      ? { ok: false, error: timeoutError }
      : { ok: false, error: new ApiAbortedError('request aborted') };
  };

  const external = options.signal;
  const onExternalAbort = (): void => {
    controller.abort();
  };
  if (external) {
    if (external.aborted) controller.abort();
    else external.addEventListener('abort', onExternalAbort, { once: true });
  }

  try {
    let res: Response;
    try {
      res = await fetch(path, {
        method: 'GET',
        credentials: 'same-origin',
        headers: { Accept: 'application/json' },
        signal: controller.signal,
      });
    } catch (e) {
      const aborted = abortedResult();
      if (aborted) return aborted;
      return { ok: false, error: new ApiNetworkError(e instanceof Error ? e.message : String(e)) };
    }

    if (redirectedToLogin(res)) {
      return { ok: false, error: new AuthRequiredError('login required (redirected to /login)') };
    }
    if (!res.ok) {
      return { ok: false, error: new ApiHttpError(res.status) };
    }
    if (!isJsonContentType(res)) {
      return {
        ok: false,
        error: new AuthRequiredError(
          `login required (non-JSON response: ${res.headers.get('content-type') ?? 'none'})`,
        ),
      };
    }

    let body: unknown;
    try {
      body = await res.json();
    } catch (e) {
      const aborted = abortedResult();
      if (aborted) return aborted;
      return {
        ok: false,
        error: new ApiInvalidResponseError(e instanceof Error ? e.message : String(e)),
      };
    }

    const dataError = extractDataError(body);
    if (dataError !== null) {
      return { ok: false, error: new ApiDataError(dataError, body) };
    }
    return { ok: true, data: body as T };
  } finally {
    clearTimeout(timer);
    external?.removeEventListener('abort', onExternalAbort);
  }
}
