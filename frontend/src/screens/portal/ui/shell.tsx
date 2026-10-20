// ログイン後の画面の外枠（v2）。左のメニュー、表示中のシートの切り替え、シートとのつながりの状態。
// 社内の人が見ているときは、顧客を切り替えられる（「顧客の画面を見る」）。
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { actions, currentCustomer, currentSheet, sheetsOf, useData, viewerName } from '../data';
import { ROUTES, usePreviewNav } from '../preview';
import { cx, formatTime } from './kit';
import { StaffBand } from '../admin/customers/StaffBand';
import { IconChart, IconCheck, IconChevronDown, IconColumns, IconHistory, IconLogout, IconSearch, IconSheet, IconUser, IconUsers } from './icons';

export type Section = 'applicants' | 'metrics' | 'history' | 'columns' | 'account';

const NAV: { key: Section; label: string; route: string; icon: (p: { size?: number }) => ReactNode }[] = [
  { key: 'applicants', label: '応募者', route: ROUTES.list, icon: IconUsers },
  { key: 'metrics', label: '数字', route: ROUTES.metrics, icon: IconChart },
  { key: 'history', label: '記録', route: ROUTES.history, icon: IconHistory },
  { key: 'columns', label: '項目の設定', route: ROUTES.columns, icon: IconColumns },
  { key: 'account', label: 'アカウント', route: ROUTES.account, icon: IconUser },
];

/** シートとのつながりの状態。見出しの横の 1 か所にだけ出す。 */
export type SyncInfo =
  | { state: 'stale'; /** 最後に読めた時刻（ISO）。 */ at: string; onReload: () => void }
  | { state: 'unavailable' };

function SyncStatus({ sync }: { sync?: SyncInfo | undefined }) {
  const d = useData();
  const readAt = d.readAt[d.currentSheetId];
  const sheet = currentSheet(d);
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => { tick((n) => n + 1); }, 30_000);
    return () => { clearInterval(t); };
  }, []);
  const s: SyncInfo | undefined = sync ?? (sheet && sheet.connection !== 'ok' ? { state: 'unavailable' } : undefined);
  if (s?.state === 'unavailable') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-danger">
        <span className="h-1.5 w-1.5 rounded-full bg-pt-danger" />シートを読み込めません
      </span>
    );
  }
  if (s?.state === 'stale') {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-muted" title="いまは書き換えられません">
        <span className="h-1.5 w-1.5 rounded-full bg-pt-warn" />
        <span><span className="pt-num">{formatTime(s.at)}</span> 時点の内容・書き換え不可</span>
        <button type="button" onClick={s.onReload} className="font-medium text-pt-primary hover:underline">読み直す</button>
      </span>
    );
  }
  if (!readAt) return null;
  return (
    <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-faint" title="画面を開いている間は、30 秒おきにシートを確かめます">
      <span className="h-1.5 w-1.5 rounded-full bg-pt-line-strong" />
      <span className="pt-num">{formatTime(readAt)}</span> 時点
    </span>
  );
}

function SheetSwitcher() {
  const d = useData();
  const list = sheetsOf(d, d.currentCustomerId);
  if (list.length <= 1) {
    const only = list[0];
    return only ? <span className="inline-flex items-center gap-1.5 text-[13px] text-pt-muted"><IconSheet size={14} className="text-pt-faint" />{only.tabName}</span> : null;
  }
  return (
    <div className="flex min-w-0 items-center gap-0.5 overflow-x-auto rounded-md bg-pt-sunken p-0.5" role="tablist" aria-label="シート">
      {list.map((s) => (
        <button
          key={s.id}
          type="button"
          role="tab"
          aria-selected={s.id === d.currentSheetId}
          onClick={() => { actions.selectSheet(s.id); }}
          className={cx('inline-flex h-10 items-center gap-1.5 rounded px-2.5 text-[13px] whitespace-nowrap sm:h-7', s.id === d.currentSheetId ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card' : 'text-pt-muted hover:text-pt-ink')}
        >
          <IconSheet size={13} className={s.id === d.currentSheetId ? 'text-pt-muted' : 'text-pt-faint'} />{s.tabName}
        </button>
      ))}
    </div>
  );
}

function CustomerSwitcher() {
  const d = useData();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return undefined;
    const h = (e: MouseEvent) => { if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) setOpen(false); };
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', h);
    document.addEventListener('keydown', k);
    return () => { document.removeEventListener('mousedown', h); document.removeEventListener('keydown', k); };
  }, [open]);
  const cur = currentCustomer(d);
  if (d.viewer.kind !== 'staff') {
    return <p className="truncate text-[15px] font-semibold" title={cur?.name}>{cur?.name}</p>;
  }
  const list = d.customers.filter((c) => q.trim() === '' || c.name.includes(q.trim()));
  return (
    <div ref={ref} className="relative">
      <span className="mb-1 block text-[11px] font-medium text-pt-faint">表示中の顧客</span>
      <button
        type="button"
        onClick={() => { setOpen(!open); setQ(''); }}
        aria-expanded={open}
        title={cur?.name}
        className="flex h-8 w-full items-center gap-1.5 rounded-md px-2 -ml-2 text-left text-[14px] font-semibold hover:bg-pt-surface"
      >
        <span className="min-w-0 flex-1 truncate">{cur?.name}</span>
        <IconChevronDown size={14} className="shrink-0 text-pt-faint" />
      </button>
      {open && (
        <div className="absolute top-full left-0 z-50 mt-1 w-72 rounded-lg border border-pt-line bg-pt-surface py-1 shadow-pt-pop">
          <div className="relative px-2 pb-1">
            <IconSearch size={13} className="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-pt-faint" />
            <input
              autoFocus
              value={q}
              onChange={(e) => { setQ(e.target.value); }}
              placeholder="顧客の名前で探す"
              className="h-8 w-full rounded-md border border-pt-line-strong pr-2 pl-7 text-[13px] outline-none focus:border-pt-select"
            />
          </div>
          <ul className="max-h-72 overflow-y-auto">
            {list.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  title={c.name}
                  onClick={() => { actions.selectCustomer(c.id); setOpen(false); }}
                  className={cx('flex h-8 w-full items-center gap-2 px-3 text-left text-[13px] hover:bg-pt-sunken', c.id === d.currentCustomerId && 'font-medium')}
                >
                  <span className="min-w-0 flex-1 truncate">{c.name}</span>
                  {c.disabled && <span className="shrink-0 text-[11px] text-pt-faint">停止中</span>}
                  {c.id === d.currentCustomerId && <IconCheck size={13} className="shrink-0 text-pt-select" />}
                </button>
              </li>
            ))}
            {list.length === 0 && <li className="px-3 py-2 text-[13px] text-pt-faint">見つかりません</li>}
          </ul>
        </div>
      )}
    </div>
  );
}

export function AppShell({ section, title, actions: pageActions, children, flush = false, sync }: {
  section: Section;
  title: string;
  /** 見出しの右に並べるボタン。 */
  actions?: ReactNode;
  children: ReactNode;
  /** true なら本文の余白を付けない（表を端まで広げる）。 */
  flush?: boolean;
  /** つながりの状態の表示を上書きする（少し前の内容・読めないとき）。 */
  sync?: SyncInfo | undefined;
}) {
  const nav = usePreviewNav();
  const d = useData();
  const sheet = currentSheet(d);
  const isStaff = d.viewer.kind === 'staff';
  /** シートに関係しない画面（アカウント）では、シートの切り替えと時刻を出さない。 */
  const noSheet = section === 'account';
  const items = NAV.filter((n) => (n.key === 'columns' ? isStaff || sheet?.customerCanEditColumns === true : n.key === 'account' ? !isStaff : true));
  return (
    <div style={isStaff ? { '--band': '2.25rem' } as CSSProperties : undefined}>
    <StaffBand />
    <div className="flex min-h-dvh bg-pt-bg">
      <aside className="sticky top-[var(--band,0px)] hidden h-[calc(100dvh-var(--band,0px))] w-56 shrink-0 flex-col border-r border-pt-line bg-pt-sunken/60 lg:flex">
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
        <header className="sticky top-[var(--band,0px)] z-20 border-b border-pt-line bg-pt-bg/95 backdrop-blur">
          <div className="flex items-center gap-2 px-4 pt-3 lg:hidden">
            <div className="min-w-0 flex-1"><CustomerSwitcher /></div>
            {!noSheet && <SyncStatus sync={sync} />}
          </div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-2.5 lg:px-6">
            <h1 className="text-[17px] font-semibold">{title}</h1>
            {!noSheet && (
              <>
                <span className="hidden h-4 w-px bg-pt-line-strong sm:block" aria-hidden="true" />
                <div className="min-w-0 max-w-full flex-1 sm:flex-none"><SheetSwitcher /></div>
                <div className="hidden lg:block"><SyncStatus sync={sync} /></div>
              </>
            )}
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
    </div>
  );
}
