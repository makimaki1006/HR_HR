import { useEffect, useState } from 'react';
import { customers } from '../../fixtures';
import { actions, useAdminState } from '../customers/state';
import { usePreviewNav } from '../../preview';
import { Button, Card, Select, formatShortDateTime } from '../../ui/kit';
import type { PortalAccount } from '../../types';
import { customerName, daysSince, isInviteExpired, selectAccount, useAccountsState } from './store';
import { AccountStatusBadge, AccountsAdminLayout, SubNav } from './shared';

const DORMANT_DAYS = 60;

const rank = (a: PortalAccount): number => (a.status === 'locked' ? 0 : a.status === 'invited' ? 1 : a.status === 'active' ? 2 : 3);
const customerOrder = (id: string): number => {
  const i = customers.findIndex((c) => c.id === id);
  return i < 0 ? 999 : i;
};
const byListOrder = (a: PortalAccount, b: PortalAccount): number =>
  customerOrder(a.customerId) - customerOrder(b.customerId) ||
  rank(a) - rank(b) ||
  new Date(b.lastLoginAt ?? b.invitedAt).getTime() - new Date(a.lastLoginAt ?? a.invitedAt).getTime();

export function AccountList() {
  const nav = usePreviewNav();
  const { accounts, devices } = useAccountsState();
  const admin = useAdminState();
  const [customerId, setCustomerId] = useState(admin.accountsFilterCustomerId ?? '');
  useEffect(() => { if (admin.accountsFilterCustomerId !== null) actions.setAccountsFilter(null); }, [admin.accountsFilterCustomerId]);
  const shown = accounts.filter((a) => !customerId || a.customerId === customerId).sort(byListOrder);
  const dormant = accounts.filter((a) => a.status === 'active' && a.lastLoginAt && daysSince(a.lastLoginAt) >= DORMANT_DAYS);

  const open = (id: string) => {
    selectAccount(id);
    nav.go('admin-account-detail');
  };

  return (
    <AccountsAdminLayout title="アカウントと記録">
      <SubNav current="list" />
      {dormant.length > 0 && (
        <div className="mb-4 rounded-md border border-pt-line bg-pt-surface px-4 py-3 text-sm">
          <p className="text-pt-muted">{String(DORMANT_DAYS)} 日以上ログインしていないアカウント</p>
          <ul className="mt-1 flex flex-wrap gap-x-5 gap-y-1">
            {dormant.map((a) => (
              <li key={a.id}>
                <button type="button" onClick={() => { open(a.id); }} className="text-pt-primary hover:underline">
                  {a.displayName || a.email}
                </button>
                <span className="ml-1 text-pt-faint">（{customerName(a.customerId)}、{String(daysSince(a.lastLoginAt ?? ''))} 日前）</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 whitespace-nowrap text-sm text-pt-muted">
          顧客
          <Select value={customerId} onChange={(e) => { setCustomerId(e.target.value); }} className="!h-9 w-64 text-sm">
            <option value="">すべての顧客</option>
            {customers.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
        </label>
        <span className="text-sm text-pt-muted">{String(shown.length)} 件</span>
        <Button variant="primary" className="ml-auto" onClick={() => { nav.go('admin-invite'); }}>招待する</Button>
      </div>
      <Card className="overflow-x-auto">
        <table className="w-full min-w-[900px] text-left text-sm">
          <thead className="border-b border-pt-line bg-pt-bg text-xs text-pt-muted">
            <tr>
              <th className="px-4 py-2 font-medium">メールアドレス</th>
              <th className="px-4 py-2 font-medium">表示名</th>
              <th className="px-4 py-2 font-medium">顧客</th>
              <th className="px-4 py-2 font-medium">状態</th>
              <th className="px-4 py-2 font-medium">最後のログイン</th>
              <th className="px-4 py-2 text-right font-medium">覚えている端末</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((a) => (
              <tr key={a.id} onClick={() => { open(a.id); }} className="cursor-pointer border-b border-pt-line last:border-0 hover:bg-pt-bg">
                <td className="px-4 py-3 font-medium text-pt-primary">{a.email}</td>
                <td className="px-4 py-3">{a.displayName || <span className="text-pt-faint">（未設定）</span>}</td>
                <td className="px-4 py-3 text-pt-muted">{customerName(a.customerId)}</td>
                <td className="px-4 py-3">
                  <AccountStatusBadge account={a} />
                  {a.status === 'invited' && a.inviteExpiresAt && (
                    <span className="ml-2 text-xs text-pt-muted">
                      {formatShortDateTime(a.inviteExpiresAt)} {isInviteExpired(a) ? 'に期限が切れました' : 'まで'}
                    </span>
                  )}
                </td>
                <td className="px-4 py-3 text-pt-muted">{a.lastLoginAt ? formatShortDateTime(a.lastLoginAt) : 'まだありません'}</td>
                <td className="px-4 py-3 text-right tabular-nums">{String(devices.filter((d) => d.accountId === a.id).length)} 台</td>
              </tr>
            ))}
            {shown.length === 0 && (
              <tr><td colSpan={6} className="px-4 py-8 text-center text-pt-muted">この顧客のアカウントはまだありません。</td></tr>
            )}
          </tbody>
        </table>
      </Card>
    </AccountsAdminLayout>
  );
}
