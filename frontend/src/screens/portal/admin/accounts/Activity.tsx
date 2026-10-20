import { useState } from 'react';
import { customers } from '../../fixtures';
import { NOW } from '../../fixtures';
import { Card, Select, formatShortDateTime } from '../../ui/kit';
import type { AdminKind } from './store';
import { customerName, setActivityAccount, useAccountsState } from './store';
import { AccountsAdminLayout, KIND_LABEL, KindBadge, StaffMark, SubNav } from './shared';

const PERIODS: { id: string; label: string; days: number | null }[] = [
  { id: 'all', label: 'すべての期間', days: null },
  { id: '1', label: '今日', days: 1 },
  { id: '7', label: '直近 7 日', days: 7 },
  { id: '30', label: '直近 30 日', days: 30 },
];

export function Activity() {
  const { activity, accounts, activityAccountId } = useAccountsState();
  const [customerId, setCustomerId] = useState('');
  const [kind, setKind] = useState('');
  const [period, setPeriod] = useState('all');

  const nameOf = (email: string): string => {
    const n = accounts.find((x) => x.email === email)?.displayName ?? '';
    return n === '' ? email : n;
  };
  const accountEmail = accounts.find((x) => x.id === activityAccountId)?.email ?? '';
  const days = PERIODS.find((p) => p.id === period)?.days ?? null;
  const base = new Date(NOW);
  const from = days === null ? 0 : days === 1 ? new Date(base.getFullYear(), base.getMonth(), base.getDate()).getTime() : base.getTime() - days * 86400000;
  const shown = [...activity]
    .filter((e) => (!customerId || e.customerId === customerId) && (!accountEmail || e.target === accountEmail) && (!kind || e.kind === kind) && new Date(e.at).getTime() >= from)
    .sort((a, b) => new Date(b.at).getTime() - new Date(a.at).getTime());

  return (
    <AccountsAdminLayout title="アカウントと記録">
      <SubNav current="activity" />
      <div className="mb-3 flex flex-wrap items-end gap-x-4 gap-y-2 text-sm text-pt-muted">
        <label className="flex flex-col gap-1">
          顧客
          <Select
            value={customerId}
            onChange={(e) => {
              setCustomerId(e.target.value);
              setActivityAccount('');
            }}
            className="!h-9 !w-56 text-sm"
          >
            <option value="">すべての顧客</option>
            {customers.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
        </label>
        <label className="flex flex-col gap-1">
          アカウント
          <Select value={activityAccountId} onChange={(e) => { setActivityAccount(e.target.value); }} className="!h-9 !w-64 text-sm">
            <option value="">すべてのアカウント</option>
            {accounts.filter((x) => !customerId || x.customerId === customerId).map((x) => <option key={x.id} value={x.id}>{x.email}</option>)}
          </Select>
        </label>
        <label className="flex flex-col gap-1">
          種類
          <Select value={kind} onChange={(e) => { setKind(e.target.value); }} className="!h-9 !w-44 text-sm">
            <option value="">すべての種類</option>
            {(Object.keys(KIND_LABEL) as AdminKind[]).map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
          </Select>
        </label>
        <label className="flex flex-col gap-1">
          期間
          <Select value={period} onChange={(e) => { setPeriod(e.target.value); }} className="!h-9 !w-40 text-sm">
            {PERIODS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
          </Select>
        </label>
        <span className="ml-auto pb-2">{String(shown.length)} 件</span>
      </div>
      <Card className="overflow-x-auto">
        <table className="w-full min-w-[1000px] text-left text-sm">
          <thead className="border-b border-pt-line bg-pt-bg text-xs text-pt-muted">
            <tr>
              <th className="px-4 py-2 font-medium">日時</th>
              <th className="min-w-52 px-4 py-2 font-medium">誰が</th>
              <th className="px-4 py-2 font-medium">対象のアカウント</th>
              <th className="px-4 py-2 font-medium">顧客</th>
              <th className="px-4 py-2 font-medium">種類</th>
              <th className="px-4 py-2 font-medium">内容</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((e) => (
              <tr key={e.id} className="border-b border-pt-line align-top last:border-0">
                <td className="whitespace-nowrap px-4 py-3 text-pt-muted">{formatShortDateTime(e.at)}</td>
                <td className="min-w-52 px-4 py-3">
                  {e.internal ? <span>{e.actor}<StaffMark /></span> : (
                    <>
                      <span className="block font-medium">{nameOf(e.actor)}</span>
                      {nameOf(e.actor) !== e.actor && <span className="block break-all text-xs text-pt-muted">{e.actor}</span>}
                    </>
                  )}
                </td>
                <td className="px-4 py-3 text-pt-muted">{e.target === e.actor ? '本人' : e.target}</td>
                <td className="px-4 py-3 text-pt-muted">{customerName(e.customerId)}</td>
                <td className="px-4 py-3"><KindBadge kind={e.kind} /></td>
                <td className="px-4 py-3">{e.detail}</td>
              </tr>
            ))}
            {shown.length === 0 && <tr><td colSpan={6} className="px-4 py-8 text-center text-pt-muted">条件に合う記録はありません。</td></tr>}
          </tbody>
        </table>
      </Card>
    </AccountsAdminLayout>
  );
}
