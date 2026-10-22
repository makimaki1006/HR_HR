// ログイン後の画面の外枠（v2）。上の濃い青の帯、左のメニュー、表示中のシートの切り替え、シートとのつながりの状態。
// 社内の人が見ているときは、顧客を切り替えられる（「顧客の画面を見る」）。
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { LIVE_SHEET_ID, actions, currentCustomer, currentSheet, retryLive, sheetsOf, useData, viewerName } from '../data';
import { ROUTES, usePreviewNav } from '../preview';
import { cx, formatTime } from './kit';
import { StaffBand } from '../admin/customers/StaffBand';
import { canSee } from '../features';
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
  if (d.live.status === 'offline' && d.currentSheetId === LIVE_SHEET_ID) {
    return (
      <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-danger" title={d.live.message}>
        <span className="h-1.5 w-1.5 rounded-full bg-pt-danger" />本物のシートにつながりません
        <button type="button" onClick={() => { retryLive(); }} className="font-medium text-pt-primary hover:underline">読み直す</button>
      </span>
    );
  }
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
    // 1 枚だけのときは PC だけ名前を出す（スマホは選ぶものが無いので出さない）。
    return only ? <span className="hidden items-center gap-1.5 text-[13px] text-pt-muted lg:inline-flex"><IconSheet size={14} className="text-pt-faint" />{only.tabName}</span> : null;
  }
  return (
    <>
      {/* スマホ: 1 行の選択。 */}
      <label className="relative flex h-[36px] min-w-0 items-center lg:hidden">
        <span className="sr-only">シート</span>
        <IconSheet size={14} className="pointer-events-none absolute left-2.5 text-pt-faint" />
        <select
          value={d.currentSheetId}
          onChange={(e) => { actions.selectSheet(e.target.value); }}
          className="h-[36px] w-full min-w-0 appearance-none truncate rounded-[6px] border border-pt-line-strong bg-pt-surface pr-8 pl-8 text-[13px] text-pt-ink"
        >
          {list.map((s) => <option key={s.id} value={s.id}>{s.tabName}</option>)}
        </select>
        <IconChevronDown size={14} className="pointer-events-none absolute right-2.5 text-pt-faint" />
      </label>
      {/* PC: タブ。 */}
      <div className="hidden min-w-0 items-center gap-0.5 overflow-x-auto rounded-md bg-pt-sunken p-0.5 lg:flex" role="tablist" aria-label="シート">
        {list.map((s) => (
          <button
            key={s.id}
            type="button"
            role="tab"
            aria-selected={s.id === d.currentSheetId}
            onClick={() => { actions.selectSheet(s.id); }}
            className={cx('inline-flex h-7 items-center gap-1.5 rounded px-2.5 text-[13px] whitespace-nowrap', s.id === d.currentSheetId ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card' : 'text-pt-muted hover:text-pt-ink')}
          >
            <IconSheet size={13} className={s.id === d.currentSheetId ? 'text-pt-muted' : 'text-pt-faint'} />{s.tabName}
          </button>
        ))}
      </div>
    </>
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
    return <p className="truncate text-[14px] font-medium text-white" title={cur?.name}>{cur?.name}</p>;
  }
  const list = d.customers.filter((c) => q.trim() === '' || c.name.includes(q.trim()));
  return (
    <div ref={ref} className="relative min-w-0">
      <button
        type="button"
        onClick={() => { setOpen(!open); setQ(''); }}
        aria-expanded={open}
        title={cur?.name}
        aria-label={`表示中の顧客: ${cur?.name ?? ''}`}
        className="flex h-8 max-w-full items-center gap-1.5 rounded-md px-2 text-left text-[14px] font-medium text-white hover:bg-white/15"
      >
        <span className="min-w-0 truncate">{cur?.name}</span>
        <IconChevronDown size={14} className="shrink-0 text-white/75" />
      </button>
      {open && (
        <div className="absolute top-full left-0 z-50 mt-1 w-80 max-w-[calc(100vw-2rem)] rounded-lg border border-pt-line bg-pt-surface py-1 shadow-pt-pop">
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
                  {c.disabled && <span className="shrink-0 text-[12px] text-pt-faint lg:text-[12px]">停止中</span>}
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

/** 上の濃い青の帯。画面の名前、表示中の顧客、見ている人。 */
function BrandBar() {
  const nav = usePreviewNav();
  const d = useData();
  const isStaff = d.viewer.kind === 'staff';
  return (
    <div className="sticky top-[var(--staff,0px)] z-40 flex h-12 items-center gap-3 bg-pt-brand px-4 text-white">
      <span className="flex shrink-0 items-center gap-2">
        <span className="flex size-7 items-center justify-center rounded-md bg-white/15"><IconSheet size={15} /></span>
        <span className="hidden text-[14px] font-semibold sm:inline">応募者の管理</span>
      </span>
      <span className="hidden h-5 w-px shrink-0 bg-white/30 sm:block" aria-hidden="true" />
      <div className="min-w-0 flex-1"><CustomerSwitcher /></div>
      <div className="hidden shrink-0 items-center gap-3 lg:flex">
        <span className="max-w-48 truncate text-[13px] text-white/85">{viewerName(d)}</span>
        {isStaff ? (
          <button type="button" onClick={() => { nav.go(ROUTES.adminCustomers); }} className="h-8 rounded-md px-2 text-[13px] font-medium text-white hover:bg-white/15">社内の管理画面へ</button>
        ) : (
          <button type="button" onClick={() => { nav.go(ROUTES.loggedOut); }} className="inline-flex h-8 items-center gap-1 rounded-md px-2 text-[13px] text-white/90 hover:bg-white/15 hover:text-white"><IconLogout size={14} />ログアウト</button>
        )}
      </div>
    </div>
  );
}

export function AppShell({ section, title, actions: pageActions, children, flush = false, sync, mobileHeader = 'default' }: {
  section: Section;
  title: string;
  /** 見出しの右に並べるボタン。 */
  actions?: ReactNode;
  children: ReactNode;
  /** true なら本文の余白を付けない（表を端まで広げる）。 */
  flush?: boolean;
  /** つながりの状態の表示を上書きする（少し前の内容・読めないとき）。 */
  sync?: SyncInfo | undefined;
  /** スマホの見出しの行の余白。'compact' は一覧のように上を詰めたい画面で使う。 */
  mobileHeader?: 'default' | 'compact';
}) {
  const nav = usePreviewNav();
  const d = useData();
  const sheet = currentSheet(d);
  const isStaff = d.viewer.kind === 'staff';
  /** シートに関係しない画面（アカウント）では、シートの切り替えと時刻を出さない。 */
  const noSheet = section === 'account';
  const items = NAV.filter((n) => {
    if (n.key === 'columns') return isStaff || sheet?.customerCanEditColumns === true;
    if (n.key === 'account') return !isStaff;
    if (n.key === 'metrics' || n.key === 'history') return canSee(d, n.key);
    return true;
  });
  /** 顧客の担当者のスマホの下のナビには「項目の設定」を出さない（アカウントの画面から入る）。社内の人はアカウントの画面が無いので残す。 */
  const bottomItems = isStaff ? items : items.filter((n) => n.key !== 'columns');
  return (
    <div style={{ '--staff': isStaff ? '36px' : '0px', '--band': isStaff ? '84px' : '48px' } as CSSProperties}>
    <StaffBand />
    <BrandBar />
    <div className="flex min-h-dvh bg-pt-bg">
      <aside className="sticky top-[var(--band,0px)] z-40 hidden h-[calc(100dvh-var(--band,0px))] w-56 shrink-0 flex-col border-r border-pt-line bg-pt-surface lg:flex">
        <nav className="flex flex-col gap-0.5 px-2 pt-3">
          {items.map((n) => (
            <button
              key={n.key}
              type="button"
              onClick={() => { nav.go(n.route); }}
              aria-current={n.key === section ? 'page' : undefined}
              className={cx('flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[13px]', n.key === section ? 'bg-pt-primary-soft font-semibold text-pt-primary' : 'text-pt-muted hover:bg-pt-sunken hover:text-pt-ink')}
            >
              {n.icon({ size: 16 })}{n.label}
            </button>
          ))}
        </nav>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col pb-16 lg:pb-0">
        <header className="sticky top-[var(--band,0px)] z-20 border-b border-pt-line bg-pt-bg/95 backdrop-blur">
          <div className={cx('flex flex-wrap items-center gap-x-4 gap-y-2 px-4 lg:px-6 lg:py-2.5', mobileHeader === 'compact' ? 'py-2' : 'py-2.5')}>
            <h1 className="text-[17px] font-semibold">{title}</h1>
            {!noSheet && (
              <>
                <span className="hidden h-4 w-px bg-pt-line-strong lg:block" aria-hidden="true" />
                <div className="hidden lg:block"><SheetSwitcher /></div>
                <div className="hidden lg:block"><SyncStatus sync={sync} /></div>
              </>
            )}
            {(pageActions !== undefined || !noSheet) && (
              <div className="ml-auto flex items-center gap-2">
                {/* スマホでは、つながりの状態を見出しの行の右端に出す。 */}
                {!noSheet && <div className="lg:hidden"><SyncStatus sync={sync} /></div>}
                {pageActions}
              </div>
            )}
            {/* スマホ: シートが 2 枚以上のときだけ、見出しの下に 1 行の選択。 */}
            {!noSheet && sheetsOf(d, d.currentCustomerId).length > 1 && <div className="w-full lg:hidden"><SheetSwitcher /></div>}
          </div>
        </header>
        <main className={cx('min-w-0 flex-1', !flush && 'px-4 py-4 lg:px-6')}>{children}</main>
      </div>

      <nav className="fixed inset-x-0 bottom-0 z-30 flex border-t border-pt-line bg-pt-surface lg:hidden">
        {bottomItems.map((n) => (
          <button
            key={n.key}
            type="button"
            onClick={() => { nav.go(n.route); }}
            aria-current={n.key === section ? 'page' : undefined}
            className={cx('flex h-14 flex-1 flex-col items-center justify-center gap-0.5 text-[12px]', n.key === section ? 'font-semibold text-pt-primary' : 'text-pt-muted')}
          >
            {n.icon({ size: 20 })}{n.label}
          </button>
        ))}
      </nav>
    </div>
    </div>
  );
}
