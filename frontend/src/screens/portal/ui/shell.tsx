// ログイン後の画面の外枠（v2）。左のメニュー、表示中のシートの切り替え、シートとのつながりの状態。
// 社内の人が見ているときは、顧客を切り替えられる（「顧客の画面を見る」）。
import { useEffect, useState, type ReactNode } from 'react';
import { actions, currentCustomer, currentSheet, sheetsOf, useData, viewerName } from '../data';
import { ROUTES, usePreviewNav } from '../preview';
import { cx, formatTime } from './kit';
import { IconChart, IconColumns, IconHistory, IconLogout, IconSheet, IconUser, IconUsers } from './icons';

export type Section = 'applicants' | 'metrics' | 'history' | 'columns' | 'account';

const NAV: { key: Section; label: string; route: string; icon: (p: { size?: number }) => ReactNode }[] = [
  { key: 'applicants', label: '応募者', route: ROUTES.list, icon: IconUsers },
  { key: 'metrics', label: '数字', route: ROUTES.metrics, icon: IconChart },
  { key: 'history', label: '記録', route: ROUTES.history, icon: IconHistory },
  { key: 'columns', label: '項目の設定', route: ROUTES.columns, icon: IconColumns },
  { key: 'account', label: 'アカウント', route: ROUTES.account, icon: IconUser },
];

/** シートを最後に確かめた時刻。開いている間は 30 秒おきに確かめる想定（試作は表示だけ）。 */
function SyncStatus() {
  const d = useData();
  const readAt = d.readAt[d.currentSheetId];
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => { tick((n) => n + 1); }, 30_000);
    return () => { clearInterval(t); };
  }, []);
  if (!readAt) return null;
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-pt-muted" title="画面を開いている間は、30 秒おきにシートを確かめます">
      <span className="h-1.5 w-1.5 rounded-full bg-pt-ok" />
      <span className="pt-num">{formatTime(readAt)}</span> 時点のシート
    </span>
  );
}

function SheetSwitcher() {
  const d = useData();
  const list = sheetsOf(d, d.currentCustomerId);
  if (list.length <= 1) {
    const only = list[0];
    return only ? <span className="inline-flex items-center gap-1.5 text-[13px] text-pt-muted"><IconSheet size={14} />{only.tabName}</span> : null;
  }
  return (
    <div className="flex min-w-0 items-center gap-1 overflow-x-auto" role="tablist" aria-label="シート">
      {list.map((s) => (
        <button
          key={s.id}
          type="button"
          role="tab"
          aria-selected={s.id === d.currentSheetId}
          onClick={() => { actions.selectSheet(s.id); }}
          className={cx('inline-flex h-10 items-center gap-1.5 rounded-md px-2.5 text-[13px] whitespace-nowrap sm:h-8', s.id === d.currentSheetId ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card ring-1 ring-pt-line' : 'text-pt-muted hover:bg-pt-sunken')}
        >
          <IconSheet size={14} />{s.tabName}
        </button>
      ))}
    </div>
  );
}

function CustomerSwitcher() {
  const d = useData();
  if (d.viewer.kind !== 'staff') {
    return <p className="truncate text-[15px] font-semibold">{currentCustomer(d)?.name}</p>;
  }
  return (
    <label className="block">
      <span className="mb-1 block text-[11px] font-medium tracking-wide text-pt-faint">表示中の顧客（社内）</span>
      <select
        value={d.currentCustomerId}
        onChange={(e) => { actions.selectCustomer(e.target.value); }}
        className="h-9 w-full rounded-md border border-pt-line-strong bg-pt-surface px-2 text-[13px] font-medium"
      >
        {d.customers.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
      </select>
    </label>
  );
}

export function AppShell({ section, title, actions: pageActions, children, flush = false }: {
  section: Section;
  title: string;
  /** 見出しの右に並べるボタン。 */
  actions?: ReactNode;
  children: ReactNode;
  /** true なら本文の余白を付けない（表を端まで広げる）。 */
  flush?: boolean;
}) {
  const nav = usePreviewNav();
  const d = useData();
  const sheet = currentSheet(d);
  const isStaff = d.viewer.kind === 'staff';
  const items = NAV.filter((n) => (n.key === 'columns' ? isStaff || sheet?.customerCanEditColumns === true : n.key === 'account' ? !isStaff : true));
  return (
    <div className="flex min-h-dvh bg-pt-bg">
      <aside className="sticky top-0 hidden h-dvh w-56 shrink-0 flex-col border-r border-pt-line bg-pt-sunken/60 lg:flex">
        <div className="px-4 pt-4 pb-3">
          <p className="mb-3 text-[11px] font-semibold tracking-[0.12em] text-pt-faint">応募者の管理</p>
          <CustomerSwitcher />
        </div>
        <nav className="flex flex-col gap-0.5 px-2">
          {items.map((n) => (
            <button
              key={n.key}
              type="button"
              onClick={() => { nav.go(n.route); }}
              aria-current={n.key === section ? 'page' : undefined}
              className={cx('flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[13px]', n.key === section ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card ring-1 ring-pt-line' : 'text-pt-muted hover:bg-pt-surface/70 hover:text-pt-ink')}
            >
              {n.icon({ size: 16 })}{n.label}
            </button>
          ))}
        </nav>
        <div className="mt-auto border-t border-pt-line px-4 py-3">
          <p className="truncate text-[13px] font-medium">{viewerName(d)}</p>
          {isStaff ? (
            <button type="button" onClick={() => { nav.go(ROUTES.adminCustomers); }} className="mt-1 text-xs text-pt-primary hover:underline">社内の管理画面へ</button>
          ) : (
            <button type="button" onClick={() => { nav.go(ROUTES.loggedOut); }} className="mt-1 inline-flex items-center gap-1 text-xs text-pt-muted hover:text-pt-ink"><IconLogout size={13} />ログアウト</button>
          )}
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col pb-16 lg:pb-0">
        <header className="sticky top-0 z-20 border-b border-pt-line bg-pt-bg/95 backdrop-blur">
          <div className="flex items-center gap-2 px-4 pt-3 lg:hidden">
            <div className="min-w-0 flex-1"><CustomerSwitcher /></div>
            <SyncStatus />
          </div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5 lg:px-6">
            <h1 className="text-[17px] font-semibold">{title}</h1>
            <div className="min-w-0 max-w-full flex-1 sm:flex-none"><SheetSwitcher /></div>
            <div className="hidden lg:block"><SyncStatus /></div>
            {pageActions && <div className="ml-auto flex items-center gap-2">{pageActions}</div>}
          </div>
        </header>
        <main className={cx('min-w-0 flex-1', !flush && 'px-4 py-4 lg:px-6')}>{children}</main>
      </div>

      <nav className="fixed inset-x-0 bottom-0 z-30 flex border-t border-pt-line bg-pt-surface lg:hidden">
        {items.map((n) => (
          <button
            key={n.key}
            type="button"
            onClick={() => { nav.go(n.route); }}
            className={cx('flex h-14 flex-1 flex-col items-center justify-center gap-0.5 text-[11px]', n.key === section ? 'font-semibold text-pt-primary' : 'text-pt-muted')}
          >
            {n.icon({ size: 20 })}{n.label}
          </button>
        ))}
      </nav>
    </div>
  );
}
