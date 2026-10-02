// Calls into the existing (legacy) filter endpoints.
//
// - POST /api/set_prefecture | set_municipality | set_industry_filter answer Html("OK") and store
//   the value in the server session (tower-sessions). Bodies are application/x-www-form-urlencoded.
//   CSRF: the browser sends Origin; requests without Origin/Referer pass only with
//   `X-Requested-With: fetch` (sent by the client) or HX-Request (src/lib.rs check_csrf).
// - GET /api/prefectures and /api/municipalities_cascade answer <option> HTML fragments, not JSON.
//
// postSetFilter goes through apiPostForm (src/api/client.ts) with expect: 'text'.

import { AuthRequiredError, apiGet, apiPostForm } from '../api/client';

export type SetFilterName = 'prefecture' | 'municipality' | 'job_type' | 'industry_filter';

export const LOGIN_PATH = '/login';

/** Session lost while calling a legacy endpoint (redirected to /login). */
export class ShellAuthError extends Error {
  override name = 'ShellAuthError';
}

export interface SelectOption {
  value: string;
  label: string;
  citycode?: string;
}

/**
 * POST /api/set_{name} with a form body. `fields` are the form fields
 * (e.g. { prefecture: '東京都' }). Throws on non-2xx / login redirect / network failure.
 */
export async function postSetFilter(
  name: SetFilterName,
  fields: Record<string, string>,
): Promise<void> {
  const result = await apiPostForm<string>(`/api/set_${name}`, fields, { expect: 'text' });
  if (result.ok) return;
  if (result.error instanceof AuthRequiredError) throw new ShellAuthError('login required');
  throw result.error;
}

/** Parse `<option value="x" data-citycode="y">label</option>` fragments. */
export function parseOptions(html: string): SelectOption[] {
  const doc = new DOMParser().parseFromString(`<select>${html}</select>`, 'text/html');
  return Array.from(doc.querySelectorAll('option')).map((o) => {
    const citycode = o.getAttribute('data-citycode');
    const base: SelectOption = { value: o.value, label: o.textContent };
    return citycode === null ? base : { ...base, citycode };
  });
}

async function fetchOptionHtml(path: string, signal?: AbortSignal): Promise<SelectOption[]> {
  // The endpoints answer <option> HTML, hence expect: 'text'. apiGet maps a login redirect /
  // 401 to AuthRequiredError and sends X-Requested-With: fetch.
  const result = await apiGet<string>(path, { expect: 'text', ...(signal ? { signal } : {}) });
  if (result.ok) return parseOptions(result.data);
  if (result.error instanceof AuthRequiredError) throw new ShellAuthError('login required');
  throw result.error;
}

export function fetchPrefectures(signal?: AbortSignal): Promise<SelectOption[]> {
  return fetchOptionHtml('/api/prefectures', signal);
}

export function fetchMunicipalities(
  prefecture: string,
  signal?: AbortSignal,
): Promise<SelectOption[]> {
  if (prefecture === '') return Promise.resolve([]);
  return fetchOptionHtml(
    `/api/municipalities_cascade?prefecture=${encodeURIComponent(prefecture)}`,
    signal,
  );
}
