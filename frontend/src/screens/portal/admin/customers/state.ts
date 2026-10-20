// 顧客とシートの管理画面の試作用の状態。4 つの画面で共有する（サーバーの代わり）。
import { useSyncExternalStore } from 'react';
import { accounts, customers as initialCustomers, sheets as initialSheets } from '../../fixtures';
import type { ColumnConfig, Customer, CustomerStage, PortalAccount, SheetConfig } from '../../types';

export const SERVICE_ACCOUNT_EMAIL = 'portal-sheets@hrhr-portal.iam.gserviceaccount.com';

interface State {
  customers: Customer[];
  sheets: SheetConfig[];
  accounts: PortalAccount[];
  customerId: string;
  sheetId: string;
  /** アカウントの画面を開くときに絞る顧客（受け渡し用）。 */
  accountsFilterCustomerId: string | null;
  /** 登録済みシートの URL（同じシートとタブの二重登録を知らせるため）。 */
  sheetUrls: Record<string, string>;
}

let state: State = {
  customers: initialCustomers,
  sheets: initialSheets.map((x) => (x.id === 's2' ? { ...x, connection: 'header_changed' as const } : x)),
  accounts,
  customerId: 'c1',
  sheetId: 's1',
  accountsFilterCustomerId: null,
  sheetUrls: {
    s1: 'https://docs.google.com/spreadsheets/d/sample-kaigo-2026/edit',
    s2: 'https://docs.google.com/spreadsheets/d/sample-kaigo-2026/edit',
  },
};
const listeners = new Set<() => void>();
const set = (next: Partial<State>) => {
  state = { ...state, ...next };
  listeners.forEach((l) => { l(); });
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};

export function useAdminState(): State {
  return useSyncExternalStore(subscribe, () => state);
}

export const actions = {
  selectCustomer(id: string) { set({ customerId: id }); },
  selectSheet(id: string) { set({ sheetId: id }); },
  addCustomer(name: string): string {
    const id = 'c' + String(Date.now());
    set({ customers: [...state.customers, { id, name, stage: 'sheet_only', disabled: false, lastUsedAt: null }], customerId: id });
    return id;
  },
  updateCustomer(id: string, patch: Partial<Customer>) {
    set({ customers: state.customers.map((c) => (c.id === id ? { ...c, ...patch } : c)) });
  },
  setStage(id: string, stage: CustomerStage) { actions.updateCustomer(id, { stage }); },
  setAccountsFilter(id: string | null) { set({ accountsFilterCustomerId: id }); },
  removeSheet(id: string) { set({ sheets: state.sheets.filter((x) => x.id !== id) }); },
  updateSheet(id: string, patch: Partial<SheetConfig>) {
    set({ sheets: state.sheets.map((x) => (x.id === id ? { ...x, ...patch } : x)) });
  },
  addSheet(sheet: SheetConfig, url: string) {
    set({ sheets: [...state.sheets, sheet], sheetId: sheet.id, sheetUrls: { ...state.sheetUrls, [sheet.id]: url } });
  },
  saveColumns(sheetId: string, columns: ColumnConfig[]) {
    set({ sheets: state.sheets.map((s) => (s.id === sheetId ? { ...s, columns } : s)) });
  },
};

export const STAGE_LABEL: Record<CustomerStage, string> = {
  sheet_only: 'シートだけ',
  both: 'シートと画面の両方',
  portal_only: '画面だけ',
};
export const STAGE_TONE: Record<CustomerStage, 'neutral' | 'info' | 'ok'> = {
  sheet_only: 'neutral',
  both: 'info',
  portal_only: 'ok',
};
export const STAGE_MEANING: Record<CustomerStage, string> = {
  sheet_only: '顧客は今までどおりシートを使います。顧客向けの画面は使えません。',
  both: '顧客はシートと画面のどちらにも書けます。シートへの書き込みは 1 分以内に画面へ出ます。',
  portal_only: '顧客は画面だけを使います。シートは社内だけで使い、顧客への共有は外します。',
};

export const CONNECTION_LABEL: Record<SheetConfig['connection'], string> = {
  ok: 'つながっています',
  not_shared: '共有されていません',
  not_found: 'シートが見つかりません',
  header_changed: '見出しが変わっています',
};
export const CONNECTION_TONE: Record<SheetConfig['connection'], 'ok' | 'warn' | 'danger'> = {
  ok: 'ok',
  not_shared: 'warn',
  not_found: 'danger',
  header_changed: 'warn',
};

/** 画面に使えるシートか。つながっていて、顧客に見せる列がある。 */
export function isReady(sh: SheetConfig): boolean {
  return sh.connection === 'ok' && sh.columns.some((c) => c.visible);
}

/** 「両方」「画面だけ」に進めない理由。進めるなら null。 */
export function stageBlockReason(sheets: SheetConfig[]): string | null {
  return sheets.some(isReady) ? null : '先にシートを追加して、列を設定してください。';
}

/** 一覧に出す注意。段階とシートの状態が合わないとき。 */
export function stageWarning(stage: CustomerStage, sheets: SheetConfig[]): string | null {
  if (stage !== 'sheet_only' && !sheets.some(isReady)) return '画面に出せるシートがありません';
  return null;
}
