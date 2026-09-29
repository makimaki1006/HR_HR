import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AuthRequiredError, apiGet } from '../api/client';
import {
  EMPTY_FILTERS,
  hasQueryFilters,
  readQueryFilters,
  writeQueryFilters,
  type FiltersContextValue,
} from './filters';
import { postSetFilter, ShellAuthError } from './filterApi';
import { redirectToLogin } from './navigation';
import type { FiltersCurrent } from './types';

const joinList = (l: string[]): string => l.join(',');

async function pushSession(f: FiltersCurrent, q: ReturnType<typeof readQueryFilters>): Promise<void> {
  // set_prefecture resets the municipality on the server, so it must come first.
  if (q.pref !== null) await postSetFilter('prefecture', { prefecture: f.prefecture });
  if (q.muni !== null) await postSetFilter('municipality', { municipality: f.municipality });
  if (q.ind !== null || q.jt !== null) {
    await postSetFilter('industry_filter', {
      job_types: joinList(f.job_types),
      industry_raws: joinList(f.industry_raws),
    });
  }
}

/**
 * Filter state for the shell. Startup: a URL query wins and is pushed to the session via
 * set_*; without one, GET /api/filters/current is the source. Later changes call set_*
 * and mirror the values into the URL with history.replaceState.
 * When `enabled` is false nothing is fetched and `ready` is true immediately.
 */
export function useFilterState(enabled: boolean): FiltersContextValue {
  const [filters, setFilters] = useState<FiltersCurrent>(EMPTY_FILTERS);
  const [ready, setReady] = useState(!enabled);
  const filtersRef = useRef(filters);
  useEffect(() => {
    filtersRef.current = filters;
  }, [filters]);

  const fail = useCallback((e: unknown): void => {
    if (e instanceof ShellAuthError) redirectToLogin();
  }, []);

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    const isAborted = (): boolean => controller.signal.aborted;
    const q = readQueryFilters(window.location.search);
    void (async () => {
      let base: FiltersCurrent = EMPTY_FILTERS;
      const cur = await apiGet<FiltersCurrent>('/api/filters/current', {
        signal: controller.signal,
      });
      if (isAborted()) return;
      if (cur.ok) base = cur.data;
      else if (cur.error instanceof AuthRequiredError) {
        redirectToLogin();
        return;
      }
      let next = base;
      if (hasQueryFilters(q)) {
        next = {
          // A new prefecture without ?muni= clears the municipality (same as set_prefecture).
          prefecture: q.pref ?? base.prefecture,
          municipality: q.muni ?? (q.pref !== null ? '' : base.municipality),
          job_types: q.jt ?? base.job_types,
          industry_raws: q.ind ?? base.industry_raws,
        };
        try {
          await pushSession(next, q);
        } catch (e) {
          fail(e);
        }
        if (isAborted()) return;
        writeQueryFilters(next);
      }
      setFilters(next);
      setReady(true);
    })();
    return () => {
      controller.abort();
    };
  }, [enabled, fail]);

  const apply = useCallback(
    async (next: FiltersCurrent, run: () => Promise<void>): Promise<void> => {
      setFilters(next);
      writeQueryFilters(next);
      try {
        await run();
      } catch (e) {
        fail(e);
      }
    },
    [fail],
  );

  const setPrefecture = useCallback(
    (prefecture: string) =>
      apply({ ...filtersRef.current, prefecture, municipality: '' }, () =>
        postSetFilter('prefecture', { prefecture }),
      ),
    [apply],
  );
  const setMunicipality = useCallback(
    (municipality: string) =>
      apply({ ...filtersRef.current, municipality }, () =>
        postSetFilter('municipality', { municipality }),
      ),
    [apply],
  );
  const setIndustry = useCallback(
    (jobTypes: string[], industryRaws: string[]) =>
      apply({ ...filtersRef.current, job_types: jobTypes, industry_raws: industryRaws }, () =>
        postSetFilter('industry_filter', {
          job_types: joinList(jobTypes),
          industry_raws: joinList(industryRaws),
        }),
      ),
    [apply],
  );

  return useMemo(
    () => ({ filters, ready, setPrefecture, setMunicipality, setIndustry }),
    [filters, ready, setPrefecture, setMunicipality, setIndustry],
  );
}
