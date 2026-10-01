import { useEffect, useState } from 'react';
import { useFilters } from './filters';
import {
  fetchMunicipalities,
  fetchPrefectures,
  ShellAuthError,
  type SelectOption,
} from './filterApi';
import { redirectToLogin } from './navigation';

/**
 * Loads <option> data; re-runs only when `key` (or `enabled`) changes. While `enabled` is false
 * nothing is requested and the previous options stay.
 */
function useOptions(
  load: (signal: AbortSignal) => Promise<SelectOption[]>,
  key: string,
  enabled = true,
) {
  const [options, setOptions] = useState<SelectOption[]>([]);
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    load(controller.signal).then(
      (o) => {
        if (!controller.signal.aborted) setOptions(o);
      },
      (e: unknown) => {
        if (e instanceof ShellAuthError) redirectToLogin();
        else if (!controller.signal.aborted) setOptions([]);
      },
    );
    return () => {
      controller.abort();
    };
    // `load` is a fresh closure every render; `key` identifies the request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled]);
  return options;
}

export function HeaderFilters() {
  const { filters, ready, syncing, error, setPrefecture, setMunicipality } = useFilters();
  const prefs = useOptions((s) => fetchPrefectures(s), 'prefectures');
  // Same order as the legacy JS: the municipality list is fetched only after set_prefecture finished.
  const munis = useOptions(
    (s) => fetchMunicipalities(filters.prefecture, s),
    `munis:${filters.prefecture}`,
    ready && !syncing,
  );
  const industryCount = filters.industry_raws.length + filters.job_types.length;

  return (
    <div className="hr-filter-group" role="group" aria-label="絞り込み">
      <span className="hr-filter-title">絞り込み</span>
      <div className="hr-filter-field">
        <label htmlFor="hr-pref-select" className="hr-filter-label">
          📍 都道府県
        </label>
        <select
          id="hr-pref-select"
          className={`hr-filter-control${filters.prefecture !== '' ? ' is-active' : ''}`}
          value={filters.prefecture}
          disabled={!ready}
          onChange={(e) => {
            void setPrefecture(e.target.value);
          }}
        >
          <option value="">全国</option>
          {prefs.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
      <div className="hr-filter-field">
        <label htmlFor="hr-muni-select" className="hr-filter-label">
          市区町村
        </label>
        <select
          id="hr-muni-select"
          className={`hr-filter-control${filters.municipality !== '' ? ' is-active' : ''}`}
          value={filters.municipality}
          disabled={!ready || syncing || filters.prefecture === ''}
          onChange={(e) => {
            void setMunicipality(e.target.value);
          }}
        >
          <option value="">すべて</option>
          {munis.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
      {syncing ? (
        <span className="hr-filter-status" role="status">
          保存中...
        </span>
      ) : null}
      {error === null ? null : (
        <span className="hr-filter-error" role="alert">
          {error}
        </span>
      )}
      <div className="hr-filter-field">
        <span className="hr-filter-label">🏭 産業</span>
        <span
          className={`hr-filter-control${industryCount > 0 ? ' is-active' : ''}`}
          data-testid="industry-summary"
        >
          {industryCount > 0 ? `${String(industryCount)} 件選択中` : '全産業'}
        </span>
      </div>
    </div>
  );
}
