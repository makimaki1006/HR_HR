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

const describeFailure = (what: string, e: unknown): string =>
  `${what}: ${e instanceof Error ? e.message : String(e)}`;

/**
 * Filter state for the shell. Startup: a URL query wins and is pushed to the session via
 * set_*; without one, GET /api/filters/current is the source. Later changes are applied
 * optimistically (state + URL via history.replaceState) and sent through ONE promise queue, so
 * set_* requests always reach the server in call order (set_prefecture resets the municipality
 * server-side; a municipality POST must not overtake it). When a request fails the state and
 * the URL go back to the last server-confirmed filters, queued follow-ups that depended on the
 * failed change are dropped, and `error` is set.
 * When `enabled` is false nothing is fetched and `ready` is true immediately.
 */
export function useFilterState(enabled: boolean): FiltersContextValue {
  const [filters, setFilters] = useState<FiltersCurrent>(EMPTY_FILTERS);
  const [ready, setReady] = useState(!enabled);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const filtersRef = useRef(filters);
  /** Last filters the server acknowledged (what a revert goes back to). */
  const confirmedRef = useRef(filters);
  const queueRef = useRef<Promise<void>>(Promise.resolve());
  const pendingRef = useRef(0);
  /** Bumped on every failure; operations queued before it are dropped. */
  const epochRef = useRef(0);

  const show = useCallback((f: FiltersCurrent, url: boolean): void => {
    filtersRef.current = f;
    setFilters(f);
    if (url) writeQueryFilters(f);
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
      if (cur.ok) {
        base = cur.data;
      } else if (cur.error instanceof AuthRequiredError) {
        redirectToLogin();
        return;
      } else {
        // Do not pretend the filters are empty: stay not-ready and say why.
        setError(describeFailure('絞り込み条件を取得できませんでした', cur.error));
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
          if (isAborted()) return;
          if (e instanceof ShellAuthError) {
            redirectToLogin();
            return;
          }
          // The session did not take the URL's filters: show what the server holds now.
          const again = await apiGet<FiltersCurrent>('/api/filters/current', {
            signal: controller.signal,
          });
          if (isAborted()) return;
          next = again.ok ? again.data : base;
          setError(describeFailure('URL の絞り込み条件を保存できませんでした', e));
        }
        if (isAborted()) return;
      }
      confirmedRef.current = next;
      show(next, hasQueryFilters(q));
      setReady(true);
    })();
    return () => {
      controller.abort();
    };
  }, [enabled, show]);

  const apply = useCallback(
    (
      next: FiltersCurrent,
      run: () => Promise<void>,
      confirm: (c: FiltersCurrent) => FiltersCurrent,
    ): Promise<void> => {
      show(next, true);
      setError(null);
      const epoch = epochRef.current;
      pendingRef.current += 1;
      setSyncing(true);
      const task = async (): Promise<void> => {
        if (epochRef.current !== epoch) return; // an earlier change failed; this one depended on it
        try {
          await run();
          confirmedRef.current = confirm(confirmedRef.current);
        } catch (e) {
          if (e instanceof ShellAuthError) {
            redirectToLogin();
            return;
          }
          epochRef.current += 1;
          show(confirmedRef.current, true);
          setError(describeFailure('絞り込みを保存できませんでした', e));
        }
      };
      const result = queueRef.current.then(task).finally(() => {
        pendingRef.current -= 1;
        if (pendingRef.current === 0) setSyncing(false);
      });
      queueRef.current = result;
      return result;
    },
    [show],
  );

  const setPrefecture = useCallback(
    (prefecture: string) =>
      apply(
        { ...filtersRef.current, prefecture, municipality: '' },
        () => postSetFilter('prefecture', { prefecture }),
        (c) => ({ ...c, prefecture, municipality: '' }),
      ),
    [apply],
  );
  const setMunicipality = useCallback(
    (municipality: string) =>
      apply(
        { ...filtersRef.current, municipality },
        () => postSetFilter('municipality', { municipality }),
        (c) => ({ ...c, municipality }),
      ),
    [apply],
  );
  const setIndustry = useCallback(
    (jobTypes: string[], industryRaws: string[]) =>
      apply(
        { ...filtersRef.current, job_types: jobTypes, industry_raws: industryRaws },
        () =>
          postSetFilter('industry_filter', {
            job_types: joinList(jobTypes),
            industry_raws: joinList(industryRaws),
          }),
        (c) => ({ ...c, job_types: jobTypes, industry_raws: industryRaws }),
      ),
    [apply],
  );

  return useMemo(
    () => ({ filters, ready, syncing, error, setPrefecture, setMunicipality, setIndustry }),
    [filters, ready, syncing, error, setPrefecture, setMunicipality, setIndustry],
  );
}
