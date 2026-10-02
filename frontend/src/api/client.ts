// Same-origin JSON API client for the Rust (Axum) backend.
//
// Contract with the current backend (see claudedocs plan §3 C-1 / C-3):
// - Auth is a session cookie managed by Rust. This client never reads or stores
//   cookies / tokens; it only sends them with `credentials: 'same-origin'`.
// - When the session is missing, /api/* requests with `Accept: application/json` and no
//   HX-Request get 401 `{"error":"auth_required","login_url":"/login"}` (src/auth/mod.rs);
//   everything else still gets 303 -> /login (HTML), which fetch follows to 200 text/html.
//   "redirected to /login", HTTP 401, and a text/html body containing the login form
//   (form[action="/login"]) all map to AuthRequiredError. Any other 2xx non-JSON body
//   (204, plain text, ...) is ApiInvalidResponseError, not an auth problem.
// - Some handlers answer HTTP 200 with `{"error": "..."}`; that becomes ApiDataError.
// - Every request sends `X-Requested-With: fetch`. The CSRF check (src/lib.rs check_csrf)
//   lets writes without Origin/Referer through only with this header or HX-Request.
// - 4xx/5xx answers become ApiHttpError; when the body is JSON it is kept in `error.body`
//   (e.g. {"error_kind": "hubspot_rate_limited", "message": "..."}).
// - POST /api/set_* style endpoints answer `Html("OK")`: use `expect: 'text'` for them.

export const DEFAULT_TIMEOUT_MS = 15_000;
/** Uploads (survey CSV up to 20 MB) get a longer default than plain requests. */
export const DEFAULT_UPLOAD_TIMEOUT_MS = 120_000;

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

/** Non-2xx HTTP status (other than the login redirect / 401). */
export class ApiHttpError extends ApiError {
  override name = 'ApiHttpError';
  readonly status: number;
  /** Parsed JSON body of the error response; undefined when absent or not valid JSON. */
  readonly body?: unknown;

  constructor(status: number, body?: unknown) {
    super(`HTTP ${String(status)}`);
    this.status = status;
    if (body !== undefined) this.body = body;
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

/** `json` (default): body must be JSON. `text`: a non-JSON 2xx body is returned as a string. */
export type ApiExpect = 'json' | 'text';

export interface ApiGetOptions {
  /** Defaults to DEFAULT_TIMEOUT_MS. */
  timeoutMs?: number;
  /** Caller-owned cancellation (e.g. from a React effect cleanup). */
  signal?: AbortSignal;
}

export interface ApiRequestOptions extends ApiGetOptions {
  /** Defaults to 'json'. */
  expect?: ApiExpect;
  /**
   * Accept header value, used by apiGet only (default 'application/json'). The legacy option
   * fragment endpoints (/api/prefectures, ...) expect 'text/html': with JSON they would answer an
   * unauthenticated request with 401 JSON instead of the 303 -> /login the old screen relied on.
   */
  accept?: string;
}

export interface UploadProgress {
  loaded: number;
  total: number;
  /** loaded / total, 0..1. */
  ratio: number;
}

export interface ApiUploadOptions extends ApiRequestOptions {
  /** Called on upload progress (only when the browser knows the total size). */
  onProgress?: (progress: UploadProgress) => void;
}

export interface PollJobOptions<T> {
  /** Delay between two polls, after a response that is not done yet. */
  intervalMs: number;
  /** Overall deadline for the whole polling loop. */
  timeoutMs: number;
  /** Receives each successful (2xx JSON) body; return true to stop polling. */
  isDone: (body: T) => boolean;
  signal?: AbortSignal;
}

const LOGIN_PATH = '/login';
const REQUESTED_WITH = { 'X-Requested-With': 'fetch' } as const;

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

function extractDataError(body: unknown): string | null {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) return null;
  if (!('error' in body)) return null;
  const value: unknown = body.error;
  if (value === null || value === undefined || value === false) return null;
  return typeof value === 'string' ? value : JSON.stringify(value);
}

// ---------------------------------------------------------------------------
// Shared internals
// ---------------------------------------------------------------------------

/** Timeout + caller-cancellation bundled into one AbortSignal. */
interface Deadline {
  signal: AbortSignal;
  /** Non-null once aborted: timeout and caller cancellation are told apart. */
  aborted: () => ApiResult<never> | null;
  dispose: () => void;
}

function startDeadline(timeoutMs: number, external: AbortSignal | undefined): Deadline {
  const controller = new AbortController();
  const timeoutError = new ApiTimeoutError(timeoutMs);
  const timer = setTimeout(() => {
    controller.abort(timeoutError);
  }, timeoutMs);
  const onExternalAbort = (): void => {
    controller.abort();
  };
  if (external) {
    if (external.aborted) controller.abort();
    else external.addEventListener('abort', onExternalAbort, { once: true });
  }
  return {
    signal: controller.signal,
    aborted: () => {
      if (!controller.signal.aborted) return null;
      return controller.signal.reason === timeoutError
        ? { ok: false, error: timeoutError }
        : { ok: false, error: new ApiAbortedError('request aborted') };
    },
    dispose: () => {
      clearTimeout(timer);
      external?.removeEventListener('abort', onExternalAbort);
    },
  };
}

/** Status-level failures shared by fetch and XHR: login redirect, 401, other non-2xx. */
function classifyStatus(status: number, redirectedToLoginPage: boolean): ApiError | null {
  if (redirectedToLoginPage) return new AuthRequiredError('login required (redirected to /login)');
  if (status === 401) return new AuthRequiredError('login required (HTTP 401)');
  if (status < 200 || status >= 300) return new ApiHttpError(status);
  return null;
}

function isJsonContentType(contentType: string): boolean {
  return /^application\/(?:[\w.+-]+\+)?json\b/i.test(contentType.trim());
}

const LOGIN_FORM_RE = /<form\b[^>]*\baction\s*=\s*["']?\/login["'\s>]/i;

/** A text/html body that carries the login form (the session expired without a redirect flag). */
function isLoginPage(contentType: string, text: string): boolean {
  return /^text\/html\b/i.test(contentType.trim()) && LOGIN_FORM_RE.test(text);
}

/** Adds the parsed JSON body (if any) to an ApiHttpError. Other errors pass through. */
function withErrorBody(error: ApiError, text: string): ApiError {
  if (!(error instanceof ApiHttpError)) return error;
  try {
    return new ApiHttpError(error.status, JSON.parse(text) as unknown);
  } catch {
    return error;
  }
}

/** Turns a 2xx body into a result. `contentType` is the raw header value ('' if none). */
function parseBody<T>(contentType: string, text: string, expect: ApiExpect): ApiResult<T> {
  if (isLoginPage(contentType, text)) {
    return { ok: false, error: new AuthRequiredError('login required (login form returned)') };
  }
  if (!isJsonContentType(contentType)) {
    if (expect === 'text') return { ok: true, data: text as T };
    return {
      ok: false,
      error: new ApiInvalidResponseError(
        `unexpected non-JSON response: ${contentType === '' ? 'none' : contentType}`,
      ),
    };
  }
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch (e) {
    return {
      ok: false,
      error: new ApiInvalidResponseError(e instanceof Error ? e.message : String(e)),
    };
  }
  const dataError = extractDataError(body);
  if (dataError !== null) return { ok: false, error: new ApiDataError(dataError, body) };
  return { ok: true, data: body as T };
}

interface FetchInit {
  method: 'GET' | 'POST';
  headers: Record<string, string>;
  body?: string;
}

async function sendFetch<T>(
  path: string,
  init: FetchInit,
  options: ApiRequestOptions,
): Promise<ApiResult<T>> {
  if (!isSameOriginPath(path)) {
    return { ok: false, error: new ApiError(`path must be same-origin absolute: ${path}`) };
  }
  const deadline = startDeadline(options.timeoutMs ?? DEFAULT_TIMEOUT_MS, options.signal);
  try {
    let res: Response;
    try {
      res = await fetch(path, {
        method: init.method,
        credentials: 'same-origin',
        headers: init.headers,
        ...(init.body === undefined ? {} : { body: init.body }),
        signal: deadline.signal,
      });
    } catch (e) {
      const aborted = deadline.aborted();
      if (aborted) return aborted;
      return { ok: false, error: new ApiNetworkError(e instanceof Error ? e.message : String(e)) };
    }

    const statusError = classifyStatus(res.status, redirectedToLogin(res));
    if (statusError instanceof ApiHttpError) {
      let errorText = '';
      try {
        errorText = await res.text();
      } catch {
        // Keep the plain status error when the body cannot be read.
      }
      return { ok: false, error: withErrorBody(statusError, errorText) };
    }
    if (statusError) return { ok: false, error: statusError };

    let text: string;
    try {
      text = await res.text();
    } catch (e) {
      const aborted = deadline.aborted();
      if (aborted) return aborted;
      return {
        ok: false,
        error: new ApiInvalidResponseError(e instanceof Error ? e.message : String(e)),
      };
    }
    // A response that arrives after the timeout / caller abort (fetch ignoring the signal) is not a success.
    const abortedBeforeParse = deadline.aborted();
    if (abortedBeforeParse) return abortedBeforeParse;
    return parseBody<T>(res.headers.get('content-type') ?? '', text, options.expect ?? 'json');
  } finally {
    deadline.dispose();
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * GET a JSON endpoint on the same origin. Never throws; all failures are
 * returned as `{ ok: false, error }` with a specific ApiError subclass.
 *
 * Pass `{ expect: 'text' }` for endpoints that answer an HTML fragment instead of JSON.
 *
 * `T` is not validated at runtime; it is trusted to match the Rust contract
 * (generated types arrive in Phase 0-4).
 */
export async function apiGet<T>(
  path: string,
  options: ApiRequestOptions = {},
): Promise<ApiResult<T>> {
  return sendFetch<T>(
    path,
    {
      method: 'GET',
      headers: { Accept: options.accept ?? 'application/json', ...REQUESTED_WITH },
    },
    options,
  );
}

/** POST a JSON body. Same result/error contract as apiGet. */
export async function apiPost<T>(
  path: string,
  body: unknown,
  options: ApiRequestOptions = {},
): Promise<ApiResult<T>> {
  return sendFetch<T>(
    path,
    {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        'Content-Type': 'application/json',
        ...REQUESTED_WITH,
      },
      body: JSON.stringify(body),
    },
    options,
  );
}

/**
 * POST application/x-www-form-urlencoded (existing `/api/set_*` handlers).
 * Those answer `Html("OK")`, so pass `{ expect: 'text' }` to accept it as success.
 */
export async function apiPostForm<T>(
  path: string,
  form: URLSearchParams | Record<string, string>,
  options: ApiRequestOptions = {},
): Promise<ApiResult<T>> {
  return sendFetch<T>(
    path,
    {
      method: 'POST',
      headers: {
        Accept: 'application/json',
        'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
        ...REQUESTED_WITH,
      },
      body: new URLSearchParams(form).toString(),
    },
    options,
  );
}

/**
 * POST multipart/form-data with upload progress. XMLHttpRequest is used because
 * fetch has no upload progress. The Content-Type (with boundary) is set by the browser.
 */
export function apiUpload<T>(
  path: string,
  form: FormData,
  options: ApiUploadOptions = {},
): Promise<ApiResult<T>> {
  if (!isSameOriginPath(path)) {
    return Promise.resolve({
      ok: false,
      error: new ApiError(`path must be same-origin absolute: ${path}`),
    });
  }
  const deadline = startDeadline(options.timeoutMs ?? DEFAULT_UPLOAD_TIMEOUT_MS, options.signal);
  const expect = options.expect ?? 'json';

  return new Promise<ApiResult<T>>((resolve) => {
    const xhr = new XMLHttpRequest();
    let settled = false;
    const finish = (result: ApiResult<T>): void => {
      if (settled) return;
      settled = true;
      deadline.signal.removeEventListener('abort', onAbort);
      deadline.dispose();
      resolve(result);
    };
    const abortedResult = (): ApiResult<T> =>
      deadline.aborted() ?? { ok: false, error: new ApiAbortedError('request aborted') };
    const onAbort = (): void => {
      xhr.abort();
      finish(abortedResult());
    };

    xhr.open('POST', path);
    xhr.setRequestHeader('Accept', 'application/json');
    xhr.setRequestHeader('X-Requested-With', REQUESTED_WITH['X-Requested-With']);
    xhr.responseType = 'text';

    const onProgress = options.onProgress;
    if (onProgress) {
      xhr.upload.onprogress = (ev: ProgressEvent): void => {
        if (!ev.lengthComputable || ev.total === 0) return;
        onProgress({ loaded: ev.loaded, total: ev.total, ratio: ev.loaded / ev.total });
      };
    }
    xhr.onerror = (): void => {
      finish({ ok: false, error: new ApiNetworkError('network error during upload') });
    };
    xhr.onabort = (): void => {
      finish(abortedResult());
    };
    xhr.onload = (): void => {
      let onLoginPage: boolean;
      try {
        onLoginPage = xhr.responseURL !== '' && new URL(xhr.responseURL).pathname === LOGIN_PATH;
      } catch {
        onLoginPage = false;
      }
      const statusError = classifyStatus(xhr.status, onLoginPage);
      if (statusError) {
        finish({ ok: false, error: withErrorBody(statusError, xhr.responseText) });
        return;
      }
      finish(parseBody<T>(xhr.getResponseHeader('content-type') ?? '', xhr.responseText, expect));
    };

    if (deadline.signal.aborted) {
      onAbort();
      return;
    }
    deadline.signal.addEventListener('abort', onAbort, { once: true });
    xhr.send(form);
  });
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const done = (): void => {
      clearTimeout(timer);
      signal.removeEventListener('abort', done);
      resolve();
    };
    const timer = setTimeout(done, ms);
    signal.addEventListener('abort', done, { once: true });
  });
}

/**
 * Poll a GET endpoint until `isDone(body)` is true (async jobs such as survey analysis).
 * The first request goes out immediately. Any request failure (including 401) ends the
 * loop with that error; exceeding `timeoutMs` overall gives ApiTimeoutError, the caller's
 * signal gives ApiAbortedError.
 */
export async function pollJob<T>(path: string, options: PollJobOptions<T>): Promise<ApiResult<T>> {
  const deadline = startDeadline(options.timeoutMs, options.signal);
  try {
    for (;;) {
      const res = await apiGet<T>(path, { signal: deadline.signal });
      const aborted = deadline.aborted();
      if (aborted) return aborted;
      if (!res.ok) return res;
      if (options.isDone(res.data)) return res;
      await sleep(options.intervalMs, deadline.signal);
      const abortedAfterSleep = deadline.aborted();
      if (abortedAfterSleep) return abortedAfterSleep;
    }
  } finally {
    deadline.dispose();
  }
}
