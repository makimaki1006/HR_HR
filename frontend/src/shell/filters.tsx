import { createContext, useContext } from 'react';
import type { FiltersCurrent } from './types';

export const EMPTY_FILTERS: FiltersCurrent = {
  prefecture: '',
  municipality: '',
  job_types: [],
  industry_raws: [],
};

export interface FiltersContextValue {
  filters: FiltersCurrent;
  /** True once the initial sync (URL query -> session, or session -> state) finished. */
  ready: boolean;
  setPrefecture: (prefecture: string) => Promise<void>;
  setMunicipality: (municipality: string) => Promise<void>;
  setIndustry: (jobTypes: string[], industryRaws: string[]) => Promise<void>;
}

export const FiltersContext = createContext<FiltersContextValue | null>(null);

/** Current header filters. Must be called under <AppShell>. */
export function useFilters(): FiltersContextValue {
  const ctx = useContext(FiltersContext);
  if (!ctx) throw new Error('useFilters must be used inside <AppShell>');
  return ctx;
}

const splitList = (v: string | null): string[] =>
  v === null ? [] : v.split(',').map((s) => s.trim()).filter((s) => s !== '');

export interface QueryFilters {
  pref: string | null;
  muni: string | null;
  ind: string[] | null;
  jt: string[] | null;
}

/** ?pref=&muni=&ind=a,b&jt=x,y ("ind" = industry_raws, "jt" = job_types). null = key absent. */
export function readQueryFilters(search: string): QueryFilters {
  const p = new URLSearchParams(search);
  return {
    pref: p.get('pref'),
    muni: p.get('muni'),
    ind: p.has('ind') ? splitList(p.get('ind')) : null,
    jt: p.has('jt') ? splitList(p.get('jt')) : null,
  };
}

export function hasQueryFilters(q: QueryFilters): boolean {
  return q.pref !== null || q.muni !== null || q.ind !== null || q.jt !== null;
}

/** Write the filters into the address bar (history.replaceState); empty values are omitted. */
export function writeQueryFilters(f: FiltersCurrent): void {
  const url = new URL(window.location.href);
  const set = (key: string, value: string): void => {
    if (value === '') url.searchParams.delete(key);
    else url.searchParams.set(key, value);
  };
  set('pref', f.prefecture);
  set('muni', f.municipality);
  set('ind', f.industry_raws.join(','));
  set('jt', f.job_types.join(','));
  window.history.replaceState(window.history.state, '', url);
}
