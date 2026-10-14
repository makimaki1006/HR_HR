import { createContext, useContext, useId, useMemo, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { ownerLabel, visibleOwners } from './ownerModel';
import type { OwnersState } from './useOwners';

/** 担当者を選ぶ入力欄 (PropInput の 'owner') が使う担当者の一覧と、操作している本人の ID */
export interface OwnerChoices {
  state: OwnersState;
  /** 操作している本人の owner ID (引けなければ null) */
  meId: string | null;
  reload: () => void;
}

const NONE: OwnerChoices = { state: { phase: 'idle' }, meId: null, reload: () => undefined };
const Ctx = createContext<OwnerChoices>(NONE);

export const OwnerChoicesProvider = Ctx.Provider;
export const useOwnerChoices = (): OwnerChoices => useContext(Ctx);

/**
 * 担当者を名前で探して選ぶ入力欄。値は owner ID、表示は名前。
 * 一覧が取れなければ ID の直接入力に戻す (読み込み中は待たせる)。退職者は、いま選ばれている人だけ出す
 */
export function OwnerSelect({ value, onChange, id, label, autoFocus, onKeyDown, startOpen = false }: {
  value: string; onChange: (id: string) => void; id?: string | undefined; label: string; autoFocus?: boolean | undefined;
  onKeyDown?: ((e: KeyboardEvent) => void) | undefined; startOpen?: boolean | undefined;
}) {
  const { state, reload } = useOwnerChoices();
  const listId = useId();
  const [open, setOpen] = useState(startOpen);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const list = useMemo(() => (state.phase === 'ready' ? state.owners : []), [state]);
  const picked = list.find(o => o.id === value);
  const shown = useMemo(() => visibleOwners(list, query, false, value), [list, query, value]);

  if (state.phase === 'loading' || state.phase === 'idle') {
    return <span className="wr-owner-note" role="status">担当者の一覧を読み込み中…</span>;
  }
  if (state.phase === 'error') {
    return <span className="wr-owner-fallback">
      <input id={id} aria-label={label} inputMode="numeric" placeholder="担当者の ID (数字)" value={value} autoFocus={autoFocus}
        onKeyDown={onKeyDown} onChange={e => { onChange(e.target.value.replace(/\D/g, '').slice(0, 20)); }} />
      <span className="wr-status wr-error" role="alert">担当者の一覧を取得できませんでした。{state.message}</span>
      <button type="button" className="wr-link" onClick={reload}>一覧を取り直す</button>
    </span>;
  }

  const rows = [{ id: '', text: '(未入力)' }, ...shown.map(o => ({ id: o.id, text: ownerLabel(o) }))];
  const shownText = open ? query : (picked ? picked.name : (value === '' ? '' : `ID ${value}(一覧にありません)`));
  function choose(i: number) {
    const r = rows[i];
    if (!r) return;
    onChange(r.id);
    setOpen(false); setQuery(''); setActive(0);
  }
  function key(e: KeyboardEvent) {
    if (open && e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setOpen(false); setQuery(''); return; }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (!open) { setOpen(true); return; }
      setActive(a => Math.min(rows.length - 1, Math.max(0, a + (e.key === 'ArrowDown' ? 1 : -1))));
      return;
    }
    if (open && e.key === 'Enter' && !e.nativeEvent.isComposing) { e.preventDefault(); choose(active); return; }
    onKeyDown?.(e);
  }
  return <span className="wr-owner"
    onBlur={e => { if (!(e.relatedTarget instanceof Node && e.currentTarget.contains(e.relatedTarget))) { setOpen(false); setQuery(''); } }}>
    <input id={id} type="text" role="combobox" aria-label={label} aria-expanded={open} aria-controls={listId} aria-autocomplete="list"
      autoComplete="off" autoFocus={autoFocus} placeholder={open ? '名前・メールで絞り込み' : '(未入力) 名前で探す'}
      value={shownText} onFocus={() => { setOpen(true); }} onClick={() => { setOpen(true); }}
      onChange={e => { setQuery(e.target.value); setOpen(true); setActive(1); }} onKeyDown={key} />
    {open && <ul id={listId} role="listbox" aria-label={`${label}の候補`} className="wr-owner-list">
      {rows.map((r, i) => <li key={r.id === '' ? 'none' : r.id} role="option" aria-selected={r.id === value}
        className={`wr-owner-opt${i === active ? ' is-active' : ''}${r.id === value ? ' is-picked' : ''}`}
        onMouseDown={e => { e.preventDefault(); choose(i); }} onMouseEnter={() => { setActive(i); }}>{r.text}</li>)}
      {shown.length === 0 && <li className="wr-owner-empty" role="presentation">該当する担当者がいません</li>}
      {state.truncated && <li className="wr-owner-empty" role="presentation">担当者が多いため、一覧の一部しか取得できていません</li>}
    </ul>}
  </span>;
}
