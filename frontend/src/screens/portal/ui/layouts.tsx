// 画面の外枠。ログイン前の画面と社内の管理画面。ログイン後の顧客の画面は shell.tsx の AppShell。
import type { ReactNode } from 'react';
import { IconSheet } from './icons';
import { cx } from './kit';

/** ログイン前（ログイン・招待・確認コード）。中央に細いカード。 */
export function AuthLayout({ children, hideFooter = false, company }: { children: ReactNode; hideFooter?: boolean; /** 招待のリンクから会社が分かる画面だけ渡す。ログイン前は渡さない。 */ company?: string }) {
  return (
    <div className="flex min-h-dvh flex-col items-center bg-pt-bg px-4 py-10 sm:py-20">
      <div className="mb-6 flex flex-col items-center gap-1">
        <span className="flex size-8 items-center justify-center rounded-md bg-pt-brand text-white"><IconSheet size={16} /></span>
        <p className="text-[13px] font-semibold text-pt-ink">応募者の管理</p>
        {company && <p className="text-xs text-pt-faint">{company}</p>}
      </div>
      <div className="w-full max-w-[380px] rounded-lg border border-pt-line bg-pt-surface p-6 shadow-pt-card sm:p-7">{children}</div>
      {!hideFooter && <p className="mt-5 text-center text-xs text-pt-faint">お困りのときは、弊社の担当者にご連絡ください。</p>}
    </div>
  );
}

/** 社内の管理画面。上に明るい細い帯（顧客の画面と同じ面）、その下に見出しと本文。 */
export type AdminSection = 'customers' | 'accounts' | 'activity' | 'mail';

const ADMIN_NAV: { key: AdminSection; label: string }[] = [
  { key: 'customers', label: '顧客とシート' },
  { key: 'accounts', label: 'アカウント' },
  { key: 'activity', label: '操作の記録' },
  { key: 'mail', label: 'メールの送信元' },
];

export function AdminLayout({ title, badge, current, onNavigate, actions, back, children }: {
  title: ReactNode;
  /** 見出しのすぐ右に置く状態。 */
  badge?: ReactNode;
  current: AdminSection;
  onNavigate: (to: AdminSection) => void;
  /** 見出しの右に並べるボタン。 */
  actions?: ReactNode;
  /** 見出しの上の「戻る」。 */
  back?: { label: string; onClick: () => void };
  children: ReactNode;
}) {
  return (
    <div className="min-h-dvh bg-pt-bg">
      <header className="border-b border-pt-line bg-pt-surface">
        <div className="mx-auto flex h-11 max-w-[1280px] items-center gap-5 px-4">
          <span className="inline-flex items-center gap-2 text-[13px] font-semibold text-pt-ink">
            <span className="flex size-6 items-center justify-center rounded bg-pt-brand text-white"><IconSheet size={13} /></span>
            応募者画面の管理
            <span className="rounded bg-pt-sunken px-1.5 py-0.5 text-[12px] font-medium text-pt-muted">社内</span>
          </span>
          <nav className="flex h-full items-stretch gap-0.5">
            {ADMIN_NAV.map((n) => (
              <button
                key={n.key}
                type="button"
                onClick={() => { onNavigate(n.key); }}
                aria-current={n.key === current ? 'page' : undefined}
                className={cx('relative px-3 text-[13px] whitespace-nowrap', n.key === current ? 'font-medium text-pt-ink after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:bg-pt-brand' : 'text-pt-muted hover:text-pt-ink')}
              >
                {n.label}
              </button>
            ))}
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-[1280px] px-4 py-5">
        {back && (
          <button type="button" onClick={back.onClick} className="mb-2 inline-flex items-center gap-1 text-xs text-pt-muted hover:text-pt-ink">
            <span aria-hidden="true">←</span>{back.label}
          </button>
        )}
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <h1 className="text-[17px] font-semibold">{title}</h1>
          {badge}
          {actions && <div className="ml-auto flex flex-wrap items-center gap-2">{actions}</div>}
        </div>
        {children}
      </main>
    </div>
  );
}
