import { useEffect, useRef, useState, type ReactNode } from 'react';
import { AdminLayout, type AdminSection } from '../../ui/layouts';
import { ROUTES, usePreviewNav } from '../../preview';
import { cx } from '../../ui/kit';
import { IconMore } from '../../ui/icons';

/** 社内の管理画面の外枠。上のメニューの移動先をここで決める。 */
export function AdminPage({ title, titleBadge, current, actions, back, children }: {
  title: ReactNode;
  /** 見出しのすぐ右に置く状態。 */
  titleBadge?: ReactNode;
  current: AdminSection;
  actions?: ReactNode;
  back?: { label: string; to: string };
  children: ReactNode;
}) {
  const { go } = usePreviewNav();
  const route = (to: AdminSection): string => (to === 'customers' ? ROUTES.adminCustomers : to === 'accounts' ? ROUTES.adminAccounts : 'admin-activity');
  return (
    <AdminLayout
      title={title}
      {...(titleBadge ? { badge: titleBadge } : {})}
      current={current}
      {...(actions ? { actions } : {})}
      {...(back ? { back: { label: back.label, onClick: () => { go(back.to); } } } : {})}
      onNavigate={(to) => { go(route(to)); }}
    >
      {children}
    </AdminLayout>
  );
}

/** 状態を示す小さな点と文字。 */
export function StatusDot({ tone, children }: { tone: 'ok' | 'warn' | 'danger' | 'neutral'; children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-[13px]">
      <span className={cx('h-1.5 w-1.5 shrink-0 rounded-full', tone === 'ok' && 'bg-pt-ok', tone === 'warn' && 'bg-pt-warn', tone === 'danger' && 'bg-pt-danger', tone === 'neutral' && 'bg-pt-faint')} />
      {children}
    </span>
  );
}

/** 表の外枠。細い線と薄い見出し行。 */
export function Table({ children, minWidth = 900 }: { children: ReactNode; minWidth?: number }) {
  return (
    <div className="overflow-x-auto rounded-lg border border-pt-line bg-pt-surface">
      <table className="w-full text-left text-[13px] [&_tbody_tr:last-child_td]:border-b-0" style={{ minWidth }}>{children}</table>
    </div>
  );
}
export const TH = 'h-8 border-b border-pt-line bg-pt-sunken px-3 text-xs font-medium text-pt-muted whitespace-nowrap';
export const TD = 'h-10 border-b border-pt-line px-3 align-middle';

/** 入り切りの小さなスイッチ。 */
export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => { onChange(!on); }}
      className={cx('relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors', on ? 'bg-pt-primary' : 'bg-pt-line-strong')}
    >
      <span className={cx('inline-block h-4 w-4 rounded-full bg-white shadow-pt-card transition-transform', on ? 'translate-x-[18px]' : 'translate-x-0.5')} />
    </button>
  );
}

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

/** 「…」で開く小さなメニュー。めったに使わない操作を入れる。 */
export function MoreMenu({ items, label = 'そのほかの操作' }: { items: MenuItem[]; label?: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return undefined;
    const close = (e: MouseEvent): void => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', close);
    return () => { document.removeEventListener('mousedown', close); };
  }, [open]);
  return (
    <span ref={ref} className="relative inline-block" onClick={(e) => { e.stopPropagation(); }}>
      <button
        type="button"
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => { setOpen(!open); }}
        className="inline-flex h-7 w-7 items-center justify-center rounded-md text-pt-muted hover:bg-pt-sunken hover:text-pt-ink"
      >
        <IconMore size={16} />
      </button>
      {open && (
        <ul role="menu" className="absolute top-full right-0 z-30 mt-1 min-w-40 rounded-md border border-pt-line bg-pt-surface py-1 shadow-pt-pop">
          {items.map((it) => (
            <li key={it.label} role="none">
              <button
                type="button"
                role="menuitem"
                disabled={it.disabled}
                onClick={() => { setOpen(false); it.onSelect(); }}
                className={cx('block w-full px-3 py-1.5 text-left text-[13px] whitespace-nowrap hover:bg-pt-sunken disabled:cursor-not-allowed disabled:opacity-45', it.danger === true ? 'text-pt-danger' : 'text-pt-ink')}
              >
                {it.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
