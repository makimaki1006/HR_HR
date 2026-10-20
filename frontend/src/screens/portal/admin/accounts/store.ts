// 管理画面「アカウントと記録」の試作用の状態。4 つの画面で共有する（画面を移っても残る）。
import { useSyncExternalStore } from 'react';
import { LOGIN_URL, NOW, accounts as seedAccounts, activity as seedActivity, customers, devices as seedDevices } from '../../fixtures';
import type { ActivityKind, PortalAccount, TrustedDevice } from '../../types';

/** 共通の種類に、社内の操作で使う 2 つを足したもの。 */
export type AdminKind = ActivityKind | 'unlock' | 'reset';

export interface AdminEntry {
  id: string;
  at: string;
  /** 操作した人（社内の担当者名、または顧客のアカウントのメール）。 */
  actor: string;
  /** 社内の人の操作なら true。 */
  internal: boolean;
  /** 操作の対象のアカウントのメール。 */
  target: string;
  customerId: string;
  kind: AdminKind;
  detail: string;
}

export interface AccountsState {
  accounts: PortalAccount[];
  devices: TrustedDevice[];
  activity: AdminEntry[];
  selectedId: string;
  /** 操作の記録を開くときの、アカウントでの絞り込み（空はすべて）。 */
  activityAccountId: string;
}

export const STAFF = '社内担当 藤田';
export { LOGIN_URL };

const DAY = 86400000;
const nowMs = () => new Date(NOW).getTime();
export const addDays = (iso: string, days: number): string => new Date(new Date(iso).getTime() + days * DAY).toISOString();
export const daysSince = (iso: string): number => Math.floor((nowMs() - new Date(iso).getTime()) / DAY);
export const isInviteExpired = (a: PortalAccount): boolean =>
  a.status === 'invited' && a.inviteExpiresAt !== null && new Date(a.inviteExpiresAt).getTime() < nowMs();

const extraAccounts: PortalAccount[] = [
  {
    id: 'a6',
    customerId: 'c3',
    email: 'dr-ono@hikari-dental.example',
    displayName: '院長 小野',
    status: 'active',
    extraCodeEmail: null,
    lastLoginAt: '2026-07-28T09:00:00+09:00',
    invitedAt: '2026-06-10T10:00:00+09:00',
    inviteExpiresAt: null,
  },
  {
    id: 'a7',
    customerId: 'c3',
    email: 'keiri@hikari-dental.example',
    displayName: '経理 川上',
    status: 'invited',
    extraCodeEmail: null,
    lastLoginAt: null,
    invitedAt: '2026-10-08T10:00:00+09:00',
    inviteExpiresAt: '2026-10-15T10:00:00+09:00',
  },
];

const INTERNAL_KINDS: AdminKind[] = ['invite', 'disable', 'device_removed', 'unlock', 'reset'];
const seedEntries: AdminEntry[] = seedActivity.map((e) => {
  const internal = INTERNAL_KINDS.includes(e.kind);
  return {
    id: e.id,
    at: e.at,
    actor: internal ? STAFF : e.accountEmail,
    internal,
    target: e.accountEmail,
    customerId: e.customerId,
    kind: e.kind,
    detail: e.detail,
  };
});

let state: AccountsState = {
  accounts: [...seedAccounts, ...extraAccounts],
  devices: [...seedDevices],
  activity: seedEntries,
  selectedId: 'a1',
  activityAccountId: '',
};
const listeners = new Set<() => void>();
const set = (next: AccountsState) => {
  state = next;
  listeners.forEach((l) => { l(); });
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};

export function useAccountsState(): AccountsState {
  return useSyncExternalStore(subscribe, () => state);
}

export const customerName = (id: string): string => customers.find((c) => c.id === id)?.name ?? '';

let seq = 100;
/** 社内の人が行った操作の記録。 */
const staffEntry = (a: PortalAccount, kind: AdminKind, detail: string): AdminEntry => ({
  id: 'n' + String(seq),
  at: new Date(nowMs() + seq++ * 1000).toISOString(),
  actor: STAFF,
  internal: true,
  target: a.email,
  customerId: a.customerId,
  kind,
  detail,
});

const token = () => Math.random().toString(36).slice(2, 10) + Math.random().toString(36).slice(2, 10);

export function selectAccount(id: string): void {
  set({ ...state, selectedId: id });
}

export function setActivityAccount(id: string): void {
  set({ ...state, activityAccountId: id });
}

export function findByEmail(email: string): PortalAccount | undefined {
  const key = email.trim().toLowerCase();
  return state.accounts.find((a) => a.email.toLowerCase() === key);
}

export interface Issued { link: string; expiresAt: string }

export function invite(customerId: string, email: string, displayName: string): Issued & { account: PortalAccount } {
  const invitedAt = new Date(nowMs()).toISOString();
  const expiresAt = addDays(invitedAt, 7);
  const account: PortalAccount = {
    id: 'n' + String(seq++),
    customerId,
    email,
    displayName,
    status: 'invited',
    extraCodeEmail: null,
    lastLoginAt: null,
    invitedAt,
    inviteExpiresAt: expiresAt,
  };
  set({
    ...state,
    accounts: [...state.accounts, account],
    activity: [staffEntry(account, 'invite', '招待リンクを発行（1 週間有効）'), ...state.activity],
  });
  return { account, link: LOGIN_URL.replace('/login', '/invite/') + token(), expiresAt };
}

/** 招待中の人に、招待リンクを出し直す。前のリンクは使えなくなる。 */
export function reissueInvite(id: string): Issued | null {
  const a = state.accounts.find((x) => x.id === id);
  if (!a) return null;
  const expiresAt = addDays(new Date(nowMs()).toISOString(), 7);
  set({
    ...state,
    accounts: state.accounts.map((x) => (x.id === id ? { ...x, inviteExpiresAt: expiresAt } : x)),
    activity: [staffEntry(a, 'invite', '招待リンクを出し直し（前のリンクは無効、1 週間有効）'), ...state.activity],
  });
  return { link: LOGIN_URL.replace('/login', '/invite/') + token(), expiresAt };
}

export function issueResetLink(id: string): Issued | null {
  const a = state.accounts.find((x) => x.id === id);
  if (!a) return null;
  const expiresAt = addDays(new Date(nowMs()).toISOString(), 7);
  set({ ...state, activity: [staffEntry(a, 'reset', 'パスワード再設定のリンクを発行（1 週間有効）'), ...state.activity] });
  return { link: LOGIN_URL.replace('/login', '/reset/') + token(), expiresAt };
}

export function unlock(id: string): void {
  const a = state.accounts.find((x) => x.id === id);
  if (!a) return;
  set({
    ...state,
    accounts: state.accounts.map((x) => (x.id === id ? { ...x, status: 'active' } : x)),
    activity: [staffEntry(a, 'unlock', 'ロックを解除（自動で解除される前に、社内の人が解除）'), ...state.activity],
  });
}

export function setDisabled(id: string, disabled: boolean): void {
  const a = state.accounts.find((x) => x.id === id);
  if (!a) return;
  set({
    ...state,
    accounts: state.accounts.map((x) => (x.id === id ? { ...x, status: disabled ? 'disabled' : 'active' } : x)),
    activity: [staffEntry(a, 'disable', disabled ? 'アカウントを停止' : 'アカウントを再開'), ...state.activity],
  });
}

export function removeDevices(id: string, deviceId?: string): void {
  const a = state.accounts.find((x) => x.id === id);
  if (!a) return;
  const target = state.devices.filter((d) => d.accountId === id && (!deviceId || d.id === deviceId));
  if (target.length === 0) return;
  const ids = new Set(target.map((d) => d.id));
  set({
    ...state,
    devices: state.devices.filter((d) => !ids.has(d.id)),
    activity: [staffEntry(a, 'device_removed', deviceId ? '覚えている端末を 1 台解除' : '覚えている端末をすべて解除'), ...state.activity],
  });
}
