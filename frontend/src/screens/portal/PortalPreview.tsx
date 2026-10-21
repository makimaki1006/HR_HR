// 試作の入口。上の細い帯で画面と試作の操作を切り替える（実装では無くなる帯）。
import { useEffect, useMemo, useState } from 'react';
import { PreviewNavContext, ROUTES, type PreviewScreen } from './preview';
import { screens as auth } from './customer/auth';
import { screens as list } from './customer/list';
import { screens as edit } from './customer/edit';
import { screens as account } from './customer/account';
import { screens as metrics } from './customer/metrics';
import { screens as history } from './customer/history';
import { screens as columnsScreens } from './customer/columns';
import { IS_SERVER_TEST, LIVE_SHEET_ID, actions, retryLive, startLive, useData } from './data';
import { STAFF_NAME } from './fixtures';
import { screens as adminCustomers } from './admin/customers';
import { screens as adminAccounts } from './admin/accounts';

const ALL: PreviewScreen[] = [...auth, ...list, ...edit, ...metrics, ...history, ...columnsScreens, ...account, ...adminCustomers, ...adminAccounts];

const readHash = () => decodeURIComponent(window.location.hash.replace(/^#/, ''));

function PreviewBar({ currentId, onPick }: { currentId: string; onPick: (id: string) => void }) {
  const data = useData();
  const [open, setOpen] = useState(false);
  const groups = useMemo(() => {
    const m = new Map<string, PreviewScreen[]>();
    for (const s of ALL) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.entries()];
  }, []);
  const sheetId = data.currentSheetId;
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
        <select
          value={data.viewer.kind === 'staff' ? 'staff' : data.viewer.accountId}
          onChange={(e) => { actions.setViewer(e.target.value === 'staff' ? { kind: 'staff', name: STAFF_NAME } : { kind: 'customer', accountId: e.target.value }); }}
          className="rounded bg-[#3a3a3a] px-2 py-1 text-white"
          aria-label="見ている人"
        >
          <option value="a1">顧客の担当者として（サンプル介護サービス）</option>
          <option value="a5">顧客の担当者として（ひかり歯科）</option>
          <option value="staff">社内の人として</option>
        </select>
        <button type="button" onClick={() => { setOpen((v) => !v); }} className="rounded bg-[#3a3a3a] px-2 py-1">{open ? '操作を閉じる' : '試作の操作'}</button>
        <LiveBadge />
      </div>
      {open && (
        <div className="flex flex-wrap gap-2 px-3 pb-2">
          <button type="button" className="rounded bg-[#3a3a3a] px-2 py-1" onClick={() => { actions.simulateSheetEdit(sheetId); }}>社内がシートで 1 行を書き換える</button>
          <button type="button" className="rounded bg-[#3a3a3a] px-2 py-1" onClick={() => { actions.simulateNewApplicant(sheetId); }}>シートに新しい応募者が入る</button>
          <label className="flex items-center gap-1">次の保存:
            <select value={data.nextSave} onChange={(e) => { const v = e.target.value as 'ok' | 'conflict' | 'failed'; actions.setNextSave(v); }} className="rounded bg-[#3a3a3a] px-1 py-0.5">
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

/** 本物のシート（テスト）の表示。?live=1 のときは状態、それ以外は切り替えのリンク。 */
function LiveBadge() {
  const d = useData();
  const url = new URL(window.location.href);
  const on = d.live.status !== 'off';
  // /app/portal-test は既定で本物のシート（?live=0 で試作だけ）、手元の試作は ?live=1 のときだけ本物。
  if (IS_SERVER_TEST) { if (on) url.searchParams.set('live', '0'); else url.searchParams.delete('live'); }
  else if (on) url.searchParams.delete('live'); else url.searchParams.set('live', '1');
  const sheetName = d.sheets.find((s) => s.id === LIVE_SHEET_ID)?.tabName ?? '';
  return (
    <span className="ml-auto flex items-center gap-2">
      {d.live.status === 'loading' && <span className="text-sky-300">本物のシート（テスト）を読んでいます…</span>}
      {d.live.status === 'ok' && <span className="text-emerald-300" title={'「' + sheetName + '」だけが本物。ほかのシートは試作のサンプル'}>● 本物のシート（テスト）: 「{sheetName}」だけ本物、ほかのシートは試作</span>}
      {d.live.status === 'offline' && (
        <span className="text-red-300">
          ● {d.live.message ?? '本物のシートにつながりません。'}
          <button type="button" onClick={() => { retryLive(); }} className="ml-1 rounded bg-[#3a3a3a] px-1.5 py-0.5 text-white">読み直す</button>
        </span>
      )}
      <a href={url.toString()} className="rounded bg-[#3a3a3a] px-2 py-1 text-white no-underline">{on ? '試作のサンプルに戻す' : '本物のシート（テスト）で開く'}</a>
    </span>
  );
}

export function PortalPreview() {
  useEffect(() => { startLive(); }, []);
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
  // 社内の管理画面を開いたら、見ている人を社内の人にする。
  const isAdmin = screen?.group === '社内の管理';
  const viewerKind = useData().viewer.kind;
  useEffect(() => {
    if (isAdmin && viewerKind !== 'staff') actions.setViewer({ kind: 'staff', name: STAFF_NAME });
  }, [isAdmin, viewerKind]);
  return (
    <PreviewNavContext.Provider value={{ go }}>
      {!hideBar && <PreviewBar currentId={screen?.id ?? ''} onPick={go} />}
      {screen ? <div key={screen.id}>{screen.render()}</div> : <p className="p-6">画面がまだありません。</p>}
    </PreviewNavContext.Provider>
  );
}
