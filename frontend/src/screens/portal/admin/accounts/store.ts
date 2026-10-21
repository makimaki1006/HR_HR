// アカウント管理の状態。アカウントと操作の記録は data.ts にあり、ここは覚えている端末と画面の受け渡しだけを持つ。
import { useSyncExternalStore } from 'react';
import { LOGIN_URL, NOW, devices as seedDevices } from '../../fixtures';
import { actions as data, getData, useData } from '../../data';
import type { PortalAccount, TrustedDevice } from '../../types';
import { adminLog } from '../customers/state';

export { LOGIN_URL };

interface Local {
  devices: TrustedDevice[];
  selectedId: string;
  /** 操作の記録を開くときの、アカウントでの絞り込み（空はすべて）。 */
  activityAccountId: string;
  /** 操作の記録を開くときの、顧客での絞り込み（空はすべて）。 */
  activityCustomerId: string;
}

export interface AccountsState extends Local {
  accounts: PortalAccount[];
}

const DAY = 86400000;
const nowMs = (): number => new Date(NOW).getTime();
export const addDays = (iso: string, days: number): string => new Date(new Date(iso).getTime() + days * DAY).toISOString();
export const daysSince = (iso: string): number => Math.floor((nowMs() - new Date(iso).getTime()) / DAY);
export const isInviteExpired = (a: PortalAccount): boolean =>
  a.status === 'invited' && a.inviteExpiresAt !== null && new Date(a.inviteExpiresAt).getTime() < nowMs();

// ひかり歯科のアカウントを足しておく（歯科の画面の確認用）。
const extraAccounts: PortalAccount[] = [
  { id: 'a6', customerId: 'c3', email: 'dr-ono@hikari-dental.example', displayName: '院長 小野', status: 'active', extraCodeEmail: null, lastLoginAt: '2026-07-28T09:00:00+09:00', invitedAt: '2026-06-10T10:00:00+09:00', inviteExpiresAt: null },
  { id: 'a7', customerId: 'c3', email: 'keiri@hikari-dental.example', displayName: '経理 川上', status: 'invited', extraCodeEmail: null, lastLoginAt: null, invitedAt: '2026-10-08T10:00:00+09:00', inviteExpiresAt: '2026-10-15T10:00:00+09:00' },
];
{
  const have = new Set(getData().accounts.map((a) => a.id));
  const missing = extraAccounts.filter((a) => !have.has(a.id));
  if (missing.length > 0) data.updateAccounts([...getData().accounts, ...missing]);
}

let local: Local = { devices: [...seedDevices], selectedId: 'a1', activityAccountId: '', activityCustomerId: '' };
const listeners = new Set<() => void>();
const set = (next: Partial<Local>): void => {
  local = { ...local, ...next };
  listeners.forEach((l) => { l(); });
};
const subscribe = (l: () => void): (() => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};

export function useAccountsState(): AccountsState {
  const l = useSyncExternalStore(subscribe, () => local);
  const d = useData();
  return { ...l, accounts: d.accounts };
}

export const customerName = (id: string): string => getData().customers.find((c) => c.id === id)?.name ?? '';

const token = (): string => Math.random().toString(36).slice(2, 10) + Math.random().toString(36).slice(2, 10);
const accountsNow = (): PortalAccount[] => getData().accounts;

export function selectAccount(id: string): void {
  set({ selectedId: id });
}

export function setActivityAccount(id: string): void {
  set({ activityAccountId: id });
}

export function setActivityCustomer(id: string): void {
  set({ activityCustomerId: id });
}

export function findByEmail(email: string): PortalAccount | undefined {
  const key = email.trim().toLowerCase();
  return accountsNow().find((a) => a.email.toLowerCase() === key);
}

export interface Issued { link: string; expiresAt: string }

/** 社内の人の操作の記録（対象のアカウントのメールを付ける）。 */
function staffLog(a: PortalAccount, kind: 'invite' | 'disable' | 'device_removed' | 'account_unlock' | 'password_reset', detail: string): void {
  adminLog(a.customerId, kind, `${a.displayName !== '' ? a.displayName : a.email}：${detail}`, { accountEmail: a.email });
}

export function invite(customerId: string, email: string, displayName: string): Issued & { account: PortalAccount } {
  const invitedAt = new Date(nowMs()).toISOString();
  const expiresAt = addDays(invitedAt, 7);
  const account: PortalAccount = {
    id: 'n' + String(Date.now()),
    customerId,
    email,
    displayName,
    status: 'invited',
    extraCodeEmail: null,
    lastLoginAt: null,
    invitedAt,
    inviteExpiresAt: expiresAt,
  };
  data.updateAccounts([...accountsNow(), account]);
  staffLog(account, 'invite', '招待リンクを発行（1 週間有効）');
  return { account, link: LOGIN_URL.replace('/login', '/invite/') + token(), expiresAt };
}

/** 招待中の人に、招待リンクを出し直す。前のリンクは使えなくなる。 */
export function reissueInvite(id: string): Issued | null {
  const a = accountsNow().find((x) => x.id === id);
  if (!a) return null;
  const expiresAt = addDays(new Date(nowMs()).toISOString(), 7);
  data.updateAccounts(accountsNow().map((x) => (x.id === id ? { ...x, inviteExpiresAt: expiresAt } : x)));
  staffLog(a, 'invite', '招待リンクを出し直し（前のリンクは無効、1 週間有効）');
  return { link: LOGIN_URL.replace('/login', '/invite/') + token(), expiresAt };
}

export function issueResetLink(id: string): Issued | null {
  const a = accountsNow().find((x) => x.id === id);
  if (!a) return null;
  const expiresAt = addDays(new Date(nowMs()).toISOString(), 7);
  staffLog(a, 'password_reset', 'パスワード再設定のリンクを発行（1 週間有効）');
  return { link: LOGIN_URL.replace('/login', '/reset/') + token(), expiresAt };
}

export function unlock(id: string): void {
  const a = accountsNow().find((x) => x.id === id);
  if (!a) return;
  data.updateAccounts(accountsNow().map((x) => (x.id === id ? { ...x, status: 'active' } : x)));
  staffLog(a, 'account_unlock', 'ロックを解除');
}

export function setDisabled(id: string, disabled: boolean): void {
  const a = accountsNow().find((x) => x.id === id);
  if (!a) return;
  data.updateAccounts(accountsNow().map((x) => (x.id === id ? { ...x, status: disabled ? 'disabled' : 'active' } : x)));
  staffLog(a, 'disable', disabled ? 'アカウントを停止' : 'アカウントを再開');
}

export function removeDevices(id: string, deviceId?: string): void {
  const a = accountsNow().find((x) => x.id === id);
  if (!a) return;
  const target = local.devices.filter((d) => d.accountId === id && (!deviceId || d.id === deviceId));
  if (target.length === 0) return;
  const ids = new Set(target.map((d) => d.id));
  set({ devices: local.devices.filter((d) => !ids.has(d.id)) });
  staffLog(a, 'device_removed', deviceId ? '覚えている端末を 1 台解除' : '覚えている端末をすべて解除');
}
