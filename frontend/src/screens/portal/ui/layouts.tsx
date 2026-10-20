// 画面の外枠。顧客の画面（ログイン後）・ログイン前の画面・社内の管理画面の 3 種類。
import type { ReactNode } from 'react';
import { cx } from './kit';

/** ログイン前（ログイン・招待・確認コード）。中央に 1 枚のカード。 */
export function AuthLayout({ children, hideFooter = false }: { children: ReactNode; hideFooter?: boolean }) {
  return (
    <div className="flex min-h-dvh flex-col items-center bg-pt-bg px-4 py-10 sm:py-16">
      <p className="mb-6 text-sm font-medium tracking-wide text-pt-muted">応募者の管理</p>
      <div className="w-full max-w-md rounded-lg border border-pt-line bg-pt-surface p-6 sm:p-8">{children}</div>
      {!hideFooter && <p className="mt-6 text-center text-xs text-pt-faint">ログインやパスワードでお困りのときは、弊社の担当者にご連絡ください。</p>}
    </div>
  );
}

/** ログイン後の顧客の画面。上に顧客名と自分のメニュー。 */
export function CustomerLayout({ customerName, accountLabel, current, onNavigate, children, wide = true }: {
  customerName: string;
  accountLabel: string;
  current: 'list' | 'account';
  onNavigate: (to: 'list' | 'account' | 'logout') => void;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className="min-h-dvh bg-pt-bg">
      <header className="sticky top-0 z-20 border-b border-pt-line bg-pt-surface">
        <div className="mx-auto flex h-14 max-w-[1400px] items-center gap-3 px-4">
          <button type="button" onClick={() => { onNavigate('list'); }} className="min-w-0 text-left">
            <span className="block truncate text-base font-semibold text-pt-ink">{customerName}</span>
            <span className="hidden text-xs text-pt-muted sm:block">応募者の管理</span>
          </button>
          <nav className="ml-auto flex shrink-0 items-center gap-1">
            <button type="button" onClick={() => { onNavigate('list'); }} className={cx('h-11 whitespace-nowrap rounded-md px-3 text-sm', current === 'list' ? 'bg-pt-primary-soft font-medium text-pt-primary' : 'text-pt-muted hover:bg-pt-bg')}>応募者</button>
            <button type="button" onClick={() => { onNavigate('account'); }} className={cx('h-11 whitespace-nowrap rounded-md px-3 text-sm', current === 'account' ? 'bg-pt-primary-soft font-medium text-pt-primary' : 'text-pt-muted hover:bg-pt-bg')}>
              <span className="hidden sm:inline">{accountLabel}</span>
              <span className="sm:hidden">設定</span>
            </button>
            <button type="button" onClick={() => { onNavigate('logout'); }} className="hidden h-11 whitespace-nowrap rounded-md px-3 text-sm text-pt-muted hover:bg-pt-bg sm:block">ログアウト</button>
          </nav>
        </div>
      </header>
      <main className={cx('mx-auto px-4 py-5', wide ? 'max-w-[1400px]' : 'max-w-2xl')}>{children}</main>
    </div>
  );
}

/** 社内の管理画面（試作）。実装では社内の管理画面 /app/admin の中に入る。 */
export function AdminLayout({ title, current, onNavigate, children }: {
  title: string;
  current: 'customers' | 'accounts';
  onNavigate: (to: 'customers' | 'accounts') => void;
  children: ReactNode;
}) {
  return (
    <div className="min-h-dvh bg-pt-bg">
      <header className="border-b border-pt-line bg-[#1f2a37] text-white">
        <div className="mx-auto flex h-12 max-w-[1400px] items-center gap-4 px-4">
          <span className="text-sm font-semibold">社内 ／ 顧客向け応募者画面の管理</span>
          <nav className="flex gap-1">
            <button type="button" onClick={() => { onNavigate('customers'); }} className={cx('rounded px-3 py-1.5 text-sm', current === 'customers' ? 'bg-white/15' : 'text-white/70 hover:bg-white/10')}>顧客とシート</button>
            <button type="button" onClick={() => { onNavigate('accounts'); }} className={cx('rounded px-3 py-1.5 text-sm', current === 'accounts' ? 'bg-white/15' : 'text-white/70 hover:bg-white/10')}>アカウントと記録</button>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-[1400px] px-4 py-5">
        <h1 className="mb-4 text-xl font-semibold">{title}</h1>
        {children}
      </main>
    </div>
  );
}
