// The one POST helper for the my screen (W8). A thin wrapper over the shared client's apiPost, so
// the CSRF header (`X-Requested-With: fetch`), 401 / login redirect => AuthRequiredError and the
// error body (ApiHttpError.body) behave exactly like every other React screen.
import { apiPost, type ApiResult } from '../../api/client';

export const FETCH_MARKER_HEADER = 'X-Requested-With';
export const FETCH_MARKER_VALUE = 'fetch';

/** The Rust side writes to the audit Turso (30 s client timeout), so wait longer than the 15 s default. */
export const MY_POST_TIMEOUT_MS = 35_000;

export function postJson<T>(path: string, body: unknown): Promise<ApiResult<T>> {
  return apiPost<T>(path, body, { timeoutMs: MY_POST_TIMEOUT_MS });
}
