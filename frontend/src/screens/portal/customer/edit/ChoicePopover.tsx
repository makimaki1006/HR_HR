// 選択式の項目の選び方。検索欄つき、↑↓ で動かし Enter／クリックで確定。
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { cx } from '../../ui/kit';
import { IconCheck, IconSearch } from '../../ui/icons';

/**
 * 選択式の項目の選び方。variant="popover" は PC の吹き出し、"sheet" はスマホの下からのシートの中身
 * （行の高さ 52px・文字 15px・幅いっぱい）。suggested は先頭に「次によく使う」として出す。
 */
export function ChoicePopover({ options, value, onSelect, onClose, dotFor, align = 'left', variant = 'popover', suggested }: {
  options: string[];
  value: string;
  onSelect: (v: string) => void;
  onClose: () => void;
  dotFor?: (v: string) => string;
  align?: 'left' | 'right';
  variant?: 'popover' | 'sheet';
  suggested?: string[];
}) {
  const isSheet = variant === 'sheet';
  const [q, setQ] = useState('');
  const list = useMemo(() => options.filter((o) => (o === '' ? q === '' : o.includes(q))), [options, q]);
  const [cursor, setCursor] = useState(() => Math.max(0, options.indexOf(value)));
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  // スマホのシートは一覧が見えることを優先し、選択肢が多いときだけ検索欄を出す（自動で焦点は当てない。キーボードが開いて一覧が隠れるため）。
  const showSearch = isSheet ? options.length > 12 : options.length > 6;

  useEffect(() => {
    if (showSearch && !isSheet) input.current?.focus();
    else ref.current?.focus();
  }, [showSearch, isSheet]);

  useEffect(() => {
    if (isSheet) return undefined;
    const h = (e: MouseEvent) => {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) onClose();
    };
    document.addEventListener('mousedown', h);
    return () => { document.removeEventListener('mousedown', h); };
  }, [onClose, isSheet]);

  const cur = Math.min(cursor, Math.max(0, list.length - 1));
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); setCursor(Math.min(list.length - 1, cur + 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setCursor(Math.max(0, cur - 1)); }
    else if (e.key === 'Enter') { e.preventDefault(); const v = list[cur]; if (v !== undefined) onSelect(v); }
  };

  const hint = q === '' && suggested !== undefined && suggested.length > 0 ? suggested : [];
  const row = (o: string, i: number, keyPrefix: string): ReactNode => (
    <li key={`${keyPrefix}${o === '' ? '(empty)' : o}`}>
      <button
        type="button"
        role="option"
        aria-selected={o === value}
        onMouseEnter={() => { setCursor(i); }}
        onClick={() => { onSelect(o); }}
        className={cx('flex w-full items-center gap-2 rounded px-2 text-left', isSheet ? 'h-[52px] px-4 text-[15px]' : 'h-[44px] text-[13px] sm:h-8', !isSheet && i === cur && keyPrefix === '' && 'bg-pt-sunken', isSheet && 'active:bg-pt-sunken')}
      >
        {dotFor && <span className={cx('shrink-0 rounded-full', isSheet ? 'h-2 w-2' : 'h-1.5 w-1.5', dotFor(o))} />}
        <span className={cx('min-w-0 flex-1 truncate', o === '' && 'text-pt-faint')}>{o === '' ? '未入力' : o}</span>
        {o === value && <span className="text-pt-primary"><IconCheck size={isSheet ? 18 : 14} /></span>}
      </button>
    </li>
  );

  return (
    <div
      ref={ref}
      tabIndex={-1}
      role="listbox"
      onKeyDown={onKey}
      className={isSheet ? 'w-full px-1 outline-none' : cx('absolute top-full z-30 mt-1 w-60 max-w-[calc(100vw-2rem)] rounded-md border border-pt-line-strong bg-pt-surface p-1 shadow-pt-pop outline-none', align === 'right' ? 'right-0' : 'left-0')}
    >
      {showSearch && (
        <div className={cx('relative mb-1', isSheet && 'px-3')}>
          <span className={cx('pointer-events-none absolute top-1/2 -translate-y-1/2 text-pt-faint', isSheet ? 'left-6' : 'left-2')}><IconSearch size={13} /></span>
          <input ref={input} value={q} onChange={(e) => { setQ(e.target.value); setCursor(0); }} placeholder="さがす" aria-label="選択肢をさがす" className={cx('w-full rounded border border-pt-line bg-pt-bg pr-2 text-base outline-none focus:border-pt-select', isSheet ? 'h-[44px] pl-9' : 'h-9 pl-7 sm:h-8 sm:text-[13px]')} />
        </div>
      )}
      {hint.length > 0 && (
        <div className="mb-1">
          <p className={cx('pt-1 pb-0.5 font-semibold text-pt-faint', isSheet ? 'px-4 text-[13px]' : 'px-2 text-[12px]')}>次によく使う</p>
          <ul>{hint.map((o) => row(o, -1, 'next-'))}</ul>
          <p className={cx('mt-1 border-t border-pt-line pt-2 pb-0.5 font-semibold text-pt-faint', isSheet ? 'px-4 text-[13px]' : 'px-2 text-[12px]')}>すべて</p>
        </div>
      )}
      <ul className={isSheet ? '' : 'max-h-[340px] overflow-y-auto'}>
        {list.length === 0 && <li className="px-2 py-2 text-[13px] text-pt-faint">該当なし</li>}
        {list.map((o, i) => row(o, i, ''))}
      </ul>
    </div>
  );
}
