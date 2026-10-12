import { useEffect, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';

/** 絞り込みバーの右端の「…」メニュー (元の配置に戻す / 架空サンプルと実データの切り替え) */
export function BarMenu({ mode, inCall, onResetLayout, onSwitchMode }: {
  mode: 'live' | 'fixture'; inCall: boolean; onResetLayout: () => void; onSwitchMode: () => void;
}) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  const switchBlocked = mode === 'live' && inCall;

  useEffect(() => {
    if (!open) return;
    items.current[0]?.focus();
    function outside(e: MouseEvent) { if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false); }
    document.addEventListener('mousedown', outside);
    return () => { document.removeEventListener('mousedown', outside); };
  }, [open]);

  function close(focusToggle: boolean) { setOpen(false); if (focusToggle) toggle.current?.focus(); }
  function onMenuKey(e: KeyboardEvent<HTMLDivElement>) {
    const list = items.current.filter((b): b is HTMLButtonElement => b !== null && !b.disabled);
    const i = list.findIndex(b => b === document.activeElement);
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(true); }
    else if (e.key === 'Tab') setOpen(false);
    else if (e.key === 'ArrowDown') { e.preventDefault(); list[(i + 1) % list.length]?.focus(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); list[(i - 1 + list.length) % list.length]?.focus(); }
    else if (e.key === 'Home') { e.preventDefault(); list[0]?.focus(); }
    else if (e.key === 'End') { e.preventDefault(); list[list.length - 1]?.focus(); }
  }
  function onToggleKey(e: KeyboardEvent<HTMLButtonElement>) {
    if (e.key === 'ArrowDown') { e.preventDefault(); setOpen(true); }
  }

  return <span className="cq-more" ref={wrap}>
    <button type="button" ref={toggle} className="cq-btn cq-more-menu-toggle" aria-label="その他の操作" aria-haspopup="menu"
      aria-expanded={open} aria-controls="cq-bar-menu" onKeyDown={onToggleKey} onClick={() => { setOpen(o => !o); }}>…</button>
    {open && <div id="cq-bar-menu" className="cq-menu" role="menu" aria-label="その他の操作" onKeyDown={onMenuKey}>
      <button type="button" role="menuitem" ref={el => { items.current[0] = el; }}
        title="パネルの置き場所と列の幅を、最初の配置に戻します(表示する項目の選択はそのまま)"
        onClick={() => { onResetLayout(); close(true); }}>元の配置に戻す</button>
      <button type="button" role="menuitem" ref={el => { items.current[1] = el; }} disabled={switchBlocked}
        title={switchBlocked ? '通話中は切り替えられません(切り替えると電話の枠が閉じて通話が切れます)' : undefined}
        onClick={() => { onSwitchMode(); setOpen(false); }}>{mode === 'live' ? '架空サンプルを表示' : '実データに戻す'}</button>
    </div>}
  </span>;
}
