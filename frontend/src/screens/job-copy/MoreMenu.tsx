import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

/** 「⋯ その他」から呼ぶ、求人の作業画面（JobCopyMain）の操作。作業画面が開いていないときは null。 */
export interface MoreActions { importOpen: boolean; toggleImport: () => void; fixedShown: boolean; toggleFixed: () => void }
type Pending = 'import' | 'fixed';

const RegisterMoreActions = createContext<(actions: MoreActions | null) => void>(() => undefined);
export const MoreActionsProvider = RegisterMoreActions.Provider;
// 別の画面でデータ取込を押したとき、今日の画面へ移ってから開く（移った先の作業画面が受け取って消す）。
let pending: Pending | null = null;
export function peekPendingMoreAction(): Pending | null { return pending; }
export function clearPendingMoreAction() { pending = null; }

/** 作業画面が「⋯ その他」に操作を登録する。 */
export function useRegisterMoreActions(actions: MoreActions) {
  const register = useContext(RegisterMoreActions);
  const latest = useRef(actions);
  useLayoutEffect(() => { latest.current = actions; });
  const { importOpen, fixedShown } = actions;
  useEffect(() => {
    register({ importOpen, fixedShown, toggleImport: () => { latest.current.toggleImport(); }, toggleFixed: () => { latest.current.toggleFixed(); } });
  }, [register, importOpen, fixedShown]);
  useEffect(() => () => { register(null); }, [register]);
}

/** 上の帯の右端の「⋯ その他」。毎日は使わない作業（データ取込・固定一覧）をここに畳む。 */
export function MoreMenu({ actions, onGoHome }: { actions: MoreActions | null; onGoHome: (fixed: boolean) => void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => { if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { setOpen(false); trigger.current?.focus(); } };
    document.addEventListener('pointerdown', outside); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', outside); document.removeEventListener('keydown', escape); };
  }, [open]);
  const run = (kind: Pending) => {
    setOpen(false);
    if (actions) { if (kind === 'import') actions.toggleImport(); else actions.toggleFixed(); return; }
    // 作業画面の無い画面では、今日の画面へ移ってから開く（固定一覧は URL で、データ取込は印で渡す）。
    pending = kind === 'import' ? 'import' : null; onGoHome(kind === 'fixed');
  };
  return <div className="cm-more" ref={root}>
    <button type="button" ref={trigger} className="cm-more-toggle" aria-expanded={open} aria-controls={menuId} onClick={() => { setOpen(value => !value); }}><span aria-hidden="true">⋯ </span>その他</button>
    {open && <div className="cm-more-menu" id={menuId} role="group" aria-label="その他の操作">
      <button type="button" className="cm-more-item" aria-label="データを取り込む（作業用）" aria-expanded={actions?.importOpen ?? false} onClick={() => { run('import'); }}>データを取り込む（作業用）<small>課金のCSVや媒体の本文を読み込みます</small></button>
      <button type="button" className="cm-more-item" aria-label={actions?.fixedShown ? 'HubSpot の求人に戻す' : '固定一覧を表示'} aria-pressed={actions?.fixedShown ?? false} onClick={() => { run('fixed'); }}>{actions?.fixedShown ? 'HubSpot の求人に戻す' : '固定一覧を表示'}<small>{actions?.fixedShown ? '左の一覧を HubSpot の求人に戻します' : '取り込んだ一部の求人だけを左に並べます'}</small></button>
    </div>}
  </div>;
}
