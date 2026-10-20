// 試作の入口。上の細い帯で画面と試作の操作を切り替える（実装では無くなる帯）。
import { useEffect, useMemo, useState } from 'react';
import { PortalStoreProvider, usePortalStore } from './store';
import { PreviewNavContext, ROUTES, type PreviewScreen } from './preview';
import { screens as auth } from './customer/auth';
import { screens as list } from './customer/list';
import { screens as edit } from './customer/edit';
import { screens as account } from './customer/account';
import { screens as adminCustomers } from './admin/customers';
import { screens as adminAccounts } from './admin/accounts';

const ALL: PreviewScreen[] = [...auth, ...list, ...edit, ...account, ...adminCustomers, ...adminAccounts];

const readHash = () => decodeURIComponent(window.location.hash.replace(/^#/, ''));

function PreviewBar({ currentId, onPick }: { currentId: string; onPick: (id: string) => void }) {
  const store = usePortalStore();
  const [open, setOpen] = useState(false);
  const groups = useMemo(() => {
    const m = new Map<string, PreviewScreen[]>();
    for (const s of ALL) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.entries()];
  }, []);
  const sheetId = store.mySheets[0]?.id ?? 's1';
  return (
    <div className="sticky top-0 z-50 border-b border-[#444] bg-[#2b2b2b] text-xs text-white" data-preview-bar>
      <div className="flex flex-wrap items-center gap-2 px-3 py-1.5">
        <span className="font-semibold text-amber-300">試作</span>
        <select value={currentId} onChange={(e) => { onPick(e.target.value); }} className="max-w-[60vw] rounded bg-[#3a3a3a] px-2 py-1 text-white">
          {groups.map(([g, items]) => (
            <optgroup key={g} label={g}>
              {items.map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
            </optgroup>
          ))}
        </select>
        <button type="button" onClick={() => { setOpen((v) => !v); }} className="rounded bg-[#3a3a3a] px-2 py-1">{open ? '操作を閉じる' : '試作の操作'}</button>
      </div>
      {open && (
        <div className="flex flex-wrap gap-2 px-3 pb-2">
          <button type="button" className="rounded bg-[#3a3a3a] px-2 py-1" onClick={() => { store.simulateSheetEdit(sheetId); }}>社内がシートで 1 行を書き換える</button>
          <button type="button" className="rounded bg-[#3a3a3a] px-2 py-1" onClick={() => { store.simulateNewApplicant(sheetId); }}>シートに新しい応募者が入る</button>
          <label className="flex items-center gap-1">次の保存:
            <select value={store.nextSave} onChange={(e) => { store.setNextSave(e.target.value as 'ok' | 'conflict' | 'failed'); }} className="rounded bg-[#3a3a3a] px-1 py-0.5">
              <option value="ok">成功する</option>
              <option value="conflict">シートで先に変更されている</option>
              <option value="failed">保存できない</option>
            </select>
          </label>
        </div>
      )}
    </div>
  );
}

export function PortalPreview() {
  const [id, setId] = useState(() => readHash() || ROUTES.login);
  useEffect(() => {
    const onHash = () => { setId(readHash() || ROUTES.login); };
    window.addEventListener('hashchange', onHash);
    return () => { window.removeEventListener('hashchange', onHash); };
  }, []);
  const go = (next: string) => {
    window.location.hash = next;
    window.scrollTo(0, 0);
  };
  const screen = ALL.find((s) => s.id === id) ?? ALL[0];
  const hideBar = new URLSearchParams(window.location.search).get('bar') === '0';
  return (
    <PortalStoreProvider>
      <PreviewNavContext.Provider value={{ go }}>
        {!hideBar && <PreviewBar currentId={screen?.id ?? ''} onPick={go} />}
        {screen ? <div key={screen.id}>{screen.render()}</div> : <p className="p-6">画面がまだありません。</p>}
      </PreviewNavContext.Provider>
    </PortalStoreProvider>
  );
}
