import type { NavItem, NavResponse } from './types';

interface NavBarProps {
  nav: NavResponse;
  screen: string;
  /** Explicitly opened group id; the group holding the active item is open unless toggled. */
  openGroup: string | null;
  onToggleGroup: (groupId: string) => void;
}

export const isActive = (item: NavItem, screen: string): boolean =>
  item.kind === 'app' && item.href === `/app/${screen}`;

function NavLink({ item, screen, small }: { item: NavItem; screen: string; small?: boolean }) {
  const active = isActive(item, screen);
  return (
    <a
      href={item.href}
      title={item.title ?? undefined}
      aria-current={active ? 'page' : undefined}
      data-nav-id={item.id}
      className={`hr-tab${active ? ' hr-tab-active' : ''}${small ? ' hr-tab-sub' : ''}`}
    >
      {item.label}
    </a>
  );
}

type TopEntry = { type: 'item'; item: NavItem } | { type: 'group'; id: string };

export function NavBar({ nav, screen, openGroup, onToggleGroup }: NavBarProps) {
  const visibleItems = nav.items.filter((i) => !i.hidden);
  const groupLabel = new Map(nav.groups.map((g) => [g.id, g.label]));
  const groupItems = new Map<string, NavItem[]>();
  // Top row: ungrouped items in order; a group is one button placed at its first item.
  const top: TopEntry[] = [];
  for (const item of visibleItems) {
    if (item.group === null) {
      top.push({ type: 'item', item });
      continue;
    }
    const list = groupItems.get(item.group);
    if (list) list.push(item);
    else {
      groupItems.set(item.group, [item]);
      top.push({ type: 'group', id: item.group });
    }
  }
  const activeGroup = visibleItems.find((i) => i.group !== null && isActive(i, screen))?.group ?? null;
  const opened = openGroup ?? activeGroup;
  const subItems = opened === null ? [] : (groupItems.get(opened) ?? []);

  return (
    <>
      <nav className="hr-nav" aria-label="ダッシュボードナビ">
        {top.map((entry) => {
          if (entry.type === 'item') {
            return <NavLink key={entry.item.id} item={entry.item} screen={screen} />;
          }
          return (
            <button
              key={`group:${entry.id}`}
              type="button"
              aria-expanded={opened === entry.id}
              aria-controls="hr-subnav"
              data-group-id={entry.id}
              className={`hr-tab${entry.id === activeGroup ? ' hr-tab-active' : ''}`}
              onClick={() => {
                onToggleGroup(entry.id);
              }}
            >
              {groupLabel.get(entry.id) ?? entry.id} ▾
            </button>
          );
        })}
      </nav>
      {opened !== null ? (
        <nav
          id="hr-subnav"
          className="hr-nav hr-subnav"
          aria-label={groupLabel.get(opened) ?? opened}
        >
          {subItems.map((item) => (
            <NavLink key={item.id} item={item} screen={screen} small />
          ))}
        </nav>
      ) : null}
    </>
  );
}
