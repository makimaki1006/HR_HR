import { useState } from 'react';
import { AdminLayout } from '../../ui/layouts';
import { Badge, Button } from '../../ui/kit';
import { ROUTES, usePreviewNav } from '../../preview';
import type { AccountStatus, PortalAccount } from '../../types';
import { isInviteExpired, setActivityAccount, type AdminKind } from './store';
import type { ReactNode } from 'react';

export function AccountsAdminLayout({ title, children }: { title: string; children: ReactNode }) {
  const nav = usePreviewNav();
  return (
    <AdminLayout
      title={title}
      current="accounts"
      onNavigate={(to) => { nav.go(to === 'customers' ? ROUTES.adminCustomers : ROUTES.adminAccounts); }}
    >
      {children}
    </AdminLayout>
  );
}

export function SubNav({ current }: { current: 'list' | 'activity' }) {
  const nav = usePreviewNav();
  const tab = (active: boolean) => 'rounded-md px-3 py-1.5 text-sm ' + (active ? 'bg-pt-primary-soft font-medium text-pt-primary' : 'text-pt-muted hover:bg-pt-bg');
  return (
    <div className="mb-4 flex gap-1 border-b border-pt-line pb-2">
      <button type="button" className={tab(current === 'list')} onClick={() => { nav.go(ROUTES.adminAccounts); }}>アカウント</button>
      <button type="button" className={tab(current === 'activity')} onClick={() => { setActivityAccount(''); nav.go('admin-activity'); }}>操作の記録</button>
    </div>
  );
}

export const STATUS_LABEL: Record<AccountStatus, string> = { invited: '招待中', active: '利用中', locked: 'ロック中', disabled: '停止中' };

export function AccountStatusBadge({ account }: { account: PortalAccount }) {
  if (isInviteExpired(account)) return <Badge tone="warn">招待の期限切れ</Badge>;
  return <StatusBadge status={account.status} />;
}

export function StatusBadge({ status }: { status: AccountStatus }) {
  const tone = status === 'active' ? 'ok' : status === 'invited' ? 'info' : status === 'locked' ? 'warn' : 'neutral';
  return <Badge tone={tone}>{STATUS_LABEL[status]}</Badge>;
}

export const KIND_LABEL: Record<AdminKind, string> = {
  login: 'ログイン',
  login_failed: 'ログイン失敗',
  code_sent: '確認コード送信',
  view: '閲覧',
  save: '保存',
  save_conflict: '保存しなかった',
  invite: '招待',
  reset: 'パスワード再設定',
  unlock: 'ロック解除',
  disable: '停止・再開',
  device_removed: '端末の解除',
};

export function KindBadge({ kind }: { kind: AdminKind }) {
  const tone = kind === 'login_failed' ? 'danger' : kind === 'save_conflict' ? 'warn' : kind === 'save' ? 'ok' : kind === 'invite' || kind === 'reset' || kind === 'code_sent' ? 'info' : 'neutral';
  return <Badge tone={tone}>{KIND_LABEL[kind]}</Badge>;
}

export function CopyButton({ text, label = 'コピー' }: { text: string; label?: string }) {
  const [done, setDone] = useState(false);
  const copy = () => {
    try {
      void navigator.clipboard.writeText(text).catch(() => undefined);
    } catch {
      /* 試作では失敗しても表示だけ変える */
    }
    setDone(true);
    window.setTimeout(() => { setDone(false); }, 2000);
  };
  return <Button size="sm" onClick={copy}>{done ? 'コピーしました' : label}</Button>;
}

export function StaffMark() {
  return <span className="ml-1.5 rounded bg-pt-bg px-1.5 py-0.5 text-xs text-pt-muted">社内</span>;
}
