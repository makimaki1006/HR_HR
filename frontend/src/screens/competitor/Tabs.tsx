import { useRef, type KeyboardEvent, type ReactNode } from 'react';

export interface TabItem {
  /** `tab-{id}` / `panel-{id}` の ID になる (旧画面と同じ。PDF の収まり検査もこの ID を前提にしている)。 */
  id: string;
  label: string;
  content: ReactNode;
}

interface Props {
  label: string;
  items: readonly TabItem[];
  selected: string;
  onSelect: (id: string) => void;
}

/**
 * WAI-ARIA tablist。旧 static/js/competitor-tabs.js と同じ操作:
 * クリック、←→ (端で回り込む)、Home、End。非選択のパネルは DOM に残して hidden にする。
 */
export function Tabs({ label, items, selected, onSelect }: Props) {
  const refs = useRef(new Map<string, HTMLButtonElement>());

  const move = (index: number): void => {
    const target = items[index];
    if (!target) return;
    onSelect(target.id);
    refs.current.get(target.id)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number): void => {
    let next: number | undefined;
    if (event.key === 'ArrowRight') next = (index + 1) % items.length;
    else if (event.key === 'ArrowLeft') next = (index + items.length - 1) % items.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = items.length - 1;
    if (next === undefined) return;
    event.preventDefault();
    move(next);
  };

  return (
    <div className="cmp-tabs">
      <div className="cmp-tablist" role="tablist" aria-label={label}>
        {items.map((item, index) => {
          const active = item.id === selected;
          return (
            <button
              key={item.id}
              id={`tab-${item.id}`}
              type="button"
              role="tab"
              aria-selected={active}
              aria-controls={`panel-${item.id}`}
              tabIndex={active ? 0 : -1}
              className="cmp-tab"
              ref={(el) => {
                if (el) refs.current.set(item.id, el);
                else refs.current.delete(item.id);
              }}
              onClick={() => {
                onSelect(item.id);
              }}
              onKeyDown={(e) => {
                onKeyDown(e, index);
              }}
            >
              {item.label}
            </button>
          );
        })}
      </div>
      {items.map((item) => (
        <div
          key={item.id}
          id={`panel-${item.id}`}
          role="tabpanel"
          aria-labelledby={`tab-${item.id}`}
          tabIndex={0}
          hidden={item.id !== selected}
          className="cmp-panel"
        >
          {item.content}
        </div>
      ))}
    </div>
  );
}
