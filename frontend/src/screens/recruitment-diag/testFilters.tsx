import { useMemo, useState, type ReactNode } from 'react';
import { vi } from 'vitest';
import { EMPTY_FILTERS, FiltersContext, type FiltersContextValue } from '../../shell/filters';
import type { FiltersCurrent } from '../../shell/types';

export interface FiltersSpy {
  setPrefecture: ReturnType<typeof vi.fn<(p: string) => Promise<void>>>;
  setMunicipality: ReturnType<typeof vi.fn<(m: string) => Promise<void>>>;
  /** Pushes new filters from outside the screen (the header filter bar changing them). */
  current: () => FiltersCurrent;
}

/**
 * Stand-in for <AppShell filters>'s context: stateful like the real useFilterState (set_prefecture
 * clears the municipality), with spies so a test can see what the screen wrote back.
 */
export function makeFiltersProvider(
  initial: Partial<FiltersCurrent> = {},
  options: { syncing?: boolean } = {},
): {
  Provider: (props: { children: ReactNode }) => ReactNode;
  spy: FiltersSpy;
  /** Changes filters as the header bar would (not through the screen). */
  headerSet: (f: Partial<FiltersCurrent>) => void;
} {
  const spy: FiltersSpy = {
    setPrefecture: vi.fn<(p: string) => Promise<void>>(),
    setMunicipality: vi.fn<(m: string) => Promise<void>>(),
    current: () => ({ ...EMPTY_FILTERS, ...initial }),
  };
  let external: ((f: Partial<FiltersCurrent>) => void) | null = null;
  function Provider({ children }: { children: ReactNode }) {
    const [filters, setFilters] = useState<FiltersCurrent>({ ...EMPTY_FILTERS, ...initial });
    external = (f) => {
      setFilters((cur) => ({ ...cur, ...f }));
    };
    spy.current = () => filters;
    const value = useMemo<FiltersContextValue>(
      () => ({
        filters,
        ready: true,
        syncing: options.syncing ?? false,
        error: null,
        setPrefecture: (p) => {
          setFilters((cur) => ({ ...cur, prefecture: p, municipality: '' }));
          void spy.setPrefecture(p);
          return Promise.resolve();
        },
        setMunicipality: (m) => {
          setFilters((cur) => ({ ...cur, municipality: m }));
          void spy.setMunicipality(m);
          return Promise.resolve();
        },
        setIndustry: () => Promise.resolve(),
      }),
      [filters],
    );
    return <FiltersContext.Provider value={value}>{children}</FiltersContext.Provider>;
  }
  return {
    Provider,
    spy,
    headerSet: (f) => {
      external?.(f);
    },
  };
}
