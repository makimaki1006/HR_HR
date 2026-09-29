import { useState, type ReactNode } from 'react';
import { FiltersContext } from './filters';
import { HeaderFilters } from './HeaderFilters';
import { NavBar } from './NavBar';
import { useFilterState } from './useFilterState';
import { useNav } from './useNav';

export interface AppShellProps {
  /** Screen name; the active nav item is the kind==='app' item whose href is '/app/' + screen. */
  screen: string;
  /** Show the header filters (prefecture / municipality) and gate children on the initial sync. */
  filters?: boolean;
  children: ReactNode;
}

export function AppShell({ screen, filters = false, children }: AppShellProps) {
  const nav = useNav();
  const filterState = useFilterState(filters);
  const [openGroup, setOpenGroup] = useState<string | null>(null);

  return (
    <FiltersContext.Provider value={filterState}>
      <div className="min-h-screen bg-navy-900 text-slate-100">
        <header className="hr-header">
          <div className="hr-header-left">
            <h1 className="text-lg font-bold text-white">求人ダッシュボード</h1>
            {filters ? <HeaderFilters /> : null}
          </div>
          <div className="hr-header-right" data-testid="header-links">
            {nav ? (
              <>
                <span className="text-slate-400 text-sm" data-testid="user-email">
                  ログイン: {nav.user_email}
                </span>
                {nav.header_links.map((l) => (
                  <a
                    key={l.id}
                    href={l.href}
                    title={l.title ?? undefined}
                    className="text-slate-400 hover:text-white text-sm transition"
                  >
                    {l.label}
                  </a>
                ))}
              </>
            ) : null}
          </div>
        </header>
        {nav ? (
          <NavBar
            nav={nav}
            screen={screen}
            openGroup={openGroup}
            onToggleGroup={(id) => {
              setOpenGroup((cur) => (cur === id ? null : id));
            }}
          />
        ) : null}
        <main id="content" className="p-6">
          {filterState.ready ? children : <p className="text-slate-400">読み込み中...</p>}
        </main>
      </div>
    </FiltersContext.Provider>
  );
}
