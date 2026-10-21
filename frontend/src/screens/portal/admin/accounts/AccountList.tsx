import { useEffect, useState } from 'react';
import { useData } from '../../data';
import { actions, useAdminState } from '../customers/state';
import { usePreviewNav } from '../../preview';
import { Button, Select, formatShortDateTime } from '../../ui/kit';
import { IconPlus } from '../../ui/icons';
import type { PortalAccount } from '../../types';
import { AdminPage, TD, TH, Table } from '../customers/shared';
import { customerName, daysSince, isInviteExpired, selectAccount, useAccountsState } from './store';
import { AccountStatusBadge } from './shared';

const DORMANT_DAYS = 60;

const rank = (a: PortalAccount): number => (a.status === 'locked' ? 0 : a.status === 'invited' ? 1 : a.status === 'active' ? 2 : 3);

export function AccountList() {
  const nav = usePreviewNav();
  const d = useData();
  const { accounts, devices } = useAccountsState();
  const admin = useAdminState();
  const [customerId, setCustomerId] = useState(admin.accountsFilterCustomerId ?? '');
  useEffect(() => { if (admin.accountsFilterCustomerId !== null) actions.setAccountsFilter(null); }, [admin.accountsFilterCustomerId]);
  const order = (id: string): number => {
    const i = d.customers.findIndex((c) => c.id === id);
    return i < 0 ? 999 : i;
  };
  const shown = accounts
    .filter((a) => !customerId || a.customerId === customerId)
    .sort((a, b) => order(a.customerId) - order(b.customerId) || rank(a) - rank(b) || new Date(b.lastLoginAt ?? b.invitedAt).getTime() - new Date(a.lastLoginAt ?? a.invitedAt).getTime());
  const dormant = accounts.filter((a) => a.status === 'active' && a.lastLoginAt && daysSince(a.lastLoginAt) >= DORMANT_DAYS);

  const open = (id: string): void => {
    selectAccount(id);
    nav.go('admin-account-detail');
  };

  return (
    <AdminPage title="アカウント" current="accounts" actions={<Button variant="primary" size="sm" onClick={() => { nav.go('admin-invite'); }}><IconPlus size={14} />招待する</Button>}>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <Select aria-label="顧客" value={customerId} onChange={(e) => { setCustomerId(e.target.value); }} className="!w-64 sm:!h-8">
          <option value="">すべての顧客</option>
          {d.customers.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        </Select>
        <span className="pt-num text-xs text-pt-muted">{String(shown.length)} 件</span>
      </div>
      <Table minWidth={900}>
        <thead>
          <tr>
            <th className={TH}>メールアドレス</th>
            <th className={TH}>表示名</th>
            <th className={TH}>顧客</th>
            <th className={TH}>状態</th>
            <th className={TH}>最後のログイン</th>
            <th className={TH + ' text-right'}>覚えている端末</th>
          </tr>
        </thead>
        <tbody>
          {shown.map((a) => (
            <tr key={a.id} onClick={() => { open(a.id); }} className="cursor-pointer hover:bg-pt-bg">
              <td className={TD + ' font-medium'}>{a.email}</td>
              <td className={TD}>{a.displayName || <span className="text-pt-faint">（未設定）</span>}</td>
              <td className={TD + ' text-pt-muted'}>{customerName(a.customerId)}</td>
              <td className={TD}>
                <AccountStatusBadge account={a} />
                {a.status === 'invited' && a.inviteExpiresAt && (
                  <span className="pt-num ml-2 text-xs text-pt-muted">{formatShortDateTime(a.inviteExpiresAt)} {isInviteExpired(a) ? 'に期限切れ' : 'まで'}</span>
                )}
              </td>
              <td className={TD + ' pt-num text-pt-muted'}>{a.lastLoginAt ? formatShortDateTime(a.lastLoginAt) : <span className="text-pt-faint">まだない</span>}</td>
              <td className={TD + ' pt-num text-right'}>{String(devices.filter((x) => x.accountId === a.id).length)} 台</td>
            </tr>
          ))}
          {shown.length === 0 && <tr><td colSpan={6} className="px-3 py-10 text-center text-pt-muted">この顧客のアカウントはまだない</td></tr>}
        </tbody>
      </Table>
      {dormant.length > 0 && (
        <p className="mt-3 text-xs text-pt-muted">
          {String(DORMANT_DAYS)} 日以上ログインしていない：
          {dormant.map((a, i) => (
            <span key={a.id}>
              {i > 0 && '、'}
              <button type="button" onClick={() => { open(a.id); }} className="ml-1 text-pt-primary hover:underline">{a.displayName || a.email}</button>
              <span className="pt-num text-pt-faint">（{String(daysSince(a.lastLoginAt ?? ''))} 日前）</span>
            </span>
          ))}
        </p>
      )}
    </AdminPage>
  );
}
