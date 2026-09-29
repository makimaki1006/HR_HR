// Calls into the existing (legacy) filter endpoints.
//
// - POST /api/set_prefecture | set_municipality | set_industry_filter answer Html("OK") and store
//   the value in the server session (tower-sessions). Bodies are application/x-www-form-urlencoded.
//   CSRF: the Rust middleware accepts `X-Requested-With: fetch` in place of a token.
// - GET /api/prefectures and /api/municipalities_cascade answer <option> HTML fragments, not JSON.
//
// postSetFilter is the single place to swap for apiPostForm from src/api/client.ts at integration.

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

function redirectedToLogin(res: Response): boolean {
  if (!res.redirected || res.url === '') return false;
  try {
    return new URL(res.url).pathname === LOGIN_PATH;
  } catch {
    return false;
  }
}

/**
 * POST /api/set_{name} with a form body. `fields` are the form fields
 * (e.g. { prefecture: '東京都' }). Throws on non-2xx / login redirect / network failure.
 */
export async function postSetFilter(
  name: SetFilterName,
  fields: Record<string, string>,
): Promise<void> {
  const res = await fetch(`/api/set_${name}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
      'X-Requested-With': 'fetch',
    },
    body: new URLSearchParams(fields).toString(),
  });
  if (redirectedToLogin(res)) throw new ShellAuthError('login required');
  if (!res.ok) throw new Error(`HTTP ${String(res.status)}`);
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
  const res = await fetch(path, {
    method: 'GET',
    credentials: 'same-origin',
    headers: { Accept: 'text/html' },
    ...(signal ? { signal } : {}),
  });
  if (redirectedToLogin(res)) throw new ShellAuthError('login required');
  if (!res.ok) throw new Error(`HTTP ${String(res.status)}`);
  return parseOptions(await res.text());
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
