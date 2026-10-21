// 選択式の項目の選び方。検索欄つき、↑↓ で動かし Enter／クリックで確定。
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { cx } from '../../ui/kit';
import { IconCheck, IconSearch } from '../../ui/icons';

export function ChoicePopover({ options, value, onSelect, onClose, dotFor, align = 'left' }: {
  options: string[];
  value: string;
  onSelect: (v: string) => void;
  onClose: () => void;
  dotFor?: (v: string) => string;
  align?: 'left' | 'right';
}) {
  const [q, setQ] = useState('');
  const list = useMemo(() => options.filter((o) => (o === '' ? q === '' : o.includes(q))), [options, q]);
  const [cursor, setCursor] = useState(() => Math.max(0, options.indexOf(value)));
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const showSearch = options.length > 6;

  useEffect(() => {
    if (showSearch) input.current?.focus();
    else ref.current?.focus();
  }, [showSearch]);

  useEffect(() => {
    const h = (e: MouseEvent) => {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) onClose();
    };
    document.addEventListener('mousedown', h);
    return () => { document.removeEventListener('mousedown', h); };
  }, [onClose]);

  const cur = Math.min(cursor, Math.max(0, list.length - 1));
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); setCursor(Math.min(list.length - 1, cur + 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setCursor(Math.max(0, cur - 1)); }
    else if (e.key === 'Enter') { e.preventDefault(); const v = list[cur]; if (v !== undefined) onSelect(v); }
  };

  return (
    <div
      ref={ref}
      tabIndex={-1}
      role="listbox"
      onKeyDown={onKey}
      className={cx('absolute top-full z-30 mt-1 w-60 max-w-[calc(100vw-2rem)] rounded-md border border-pt-line-strong bg-pt-surface p-1 shadow-pt-pop outline-none', align === 'right' ? 'right-0' : 'left-0')}
    >
      {showSearch && (
        <div className="relative mb-1">
          <span className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-pt-faint"><IconSearch size={13} /></span>
          <input ref={input} value={q} onChange={(e) => { setQ(e.target.value); setCursor(0); }} placeholder="さがす" aria-label="選択肢をさがす" className="h-9 w-full rounded border border-pt-line bg-pt-bg pr-2 pl-7 text-base outline-none focus:border-pt-select sm:h-8 sm:text-[13px]" />
        </div>
      )}
      <ul className="max-h-60 overflow-y-auto">
        {list.length === 0 && <li className="px-2 py-2 text-[13px] text-pt-faint">該当なし</li>}
        {list.map((o, i): ReactNode => (
          <li key={o === '' ? '(empty)' : o}>
            <button
              type="button"
              role="option"
              aria-selected={o === value}
              onMouseEnter={() => { setCursor(i); }}
              onClick={() => { onSelect(o); }}
              className={cx('flex h-[44px] w-full items-center gap-2 rounded px-2 text-left text-[13px] sm:h-8', i === cur && 'bg-pt-sunken')}
            >
              {dotFor && <span className={cx('h-1.5 w-1.5 shrink-0 rounded-full', dotFor(o))} />}
              <span className={cx('min-w-0 flex-1 truncate', o === '' && 'text-pt-faint')}>{o === '' ? '（空）' : o}</span>
              {o === value && <span className="text-pt-primary"><IconCheck size={14} /></span>}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
