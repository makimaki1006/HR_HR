// 顧客とシートの管理画面で、data.ts に無いものだけを持つ（選んでいる顧客・シートの URL・画面の受け渡し）。
// 顧客・シート・アカウント・記録は data.ts を読み書きする。
import { useSyncExternalStore } from 'react';
import { applicantRows } from '../../columns';
import { STAFF_NAME } from '../../fixtures';
import { actions as data, getData, log, useData } from '../../data';
import type { ActivityKind, ApplicantRow, CustomerStage, SheetConfig } from '../../types';

export const SERVICE_ACCOUNT_EMAIL = 'portal-sheets@hrhr-portal.iam.gserviceaccount.com';

interface State {
  customerId: string;
  sheetId: string;
  /** アカウントの画面を開くときに絞る顧客（受け渡し用）。 */
  accountsFilterCustomerId: string | null;
  /** 登録済みシートの URL（同じシートとタブの二重登録を知らせるため）。 */
  sheetUrls: Record<string, string>;
  /** スプレッドシートのファイル名（架空）。 */
  sheetFiles: Record<string, string>;
  /** 招待の画面を開くときに選んでおく顧客（受け渡し用）。 */
  inviteCustomerId: string | null;
}

let state: State = {
  customerId: 'c1',
  sheetId: 's1',
  accountsFilterCustomerId: null,
  inviteCustomerId: null,
  sheetFiles: {
    s1: 'サンプル介護サービス様_応募者管理',
    s2: 'サンプル介護サービス様_応募者管理',
    s3: 'みどり運輸 採用管理',
    s4: 'ひかり歯科 応募管理',
  },
  sheetUrls: {
    s1: 'https://docs.google.com/spreadsheets/d/sample-kaigo-2026/edit',
    s2: 'https://docs.google.com/spreadsheets/d/sample-kaigo-2026/edit',
    s3: 'https://docs.google.com/spreadsheets/d/sample-midori-2026/edit',
    s4: 'https://docs.google.com/spreadsheets/d/sample-hikari-2026/edit',
  },
};
const listeners = new Set<() => void>();
const set = (next: Partial<State>): void => {
  state = { ...state, ...next };
  listeners.forEach((l) => { l(); });
};
const subscribe = (l: () => void): (() => void) => {
  listeners.add(l);
  return () => { listeners.delete(l); };
};

export function useAdminState(): State & { sheets: SheetConfig[] } {
  const s = useSyncExternalStore(subscribe, () => state);
  // sheets は旧い store.tsx（v1 の名残）が読むための互換。
  return { ...s, sheets: useData().sheets };
}

/** 社内の人が管理画面で行った操作を記録に残す。 */
export function adminLog(customerId: string, kind: ActivityKind, detail: string, extra: { sheetId?: string; column?: string; before?: string; after?: string; accountEmail?: string } = {}): void {
  log({ kind, detail, customerId, actorName: STAFF_NAME, source: 'admin', accountEmail: '', ...extra });
}

export const actions = {
  selectCustomer(id: string): void { set({ customerId: id }); },
  selectSheet(id: string): void { set({ sheetId: id }); },
  setAccountsFilter(id: string | null): void { set({ accountsFilterCustomerId: id }); },
  setInviteCustomer(id: string | null): void { set({ inviteCustomerId: id }); },

  addCustomer(name: string): string {
    const id = 'c' + String(Date.now());
    data.updateCustomers([...getData().customers, { id, name, stage: 'sheet_only', disabled: false, lastUsedAt: null }]);
    adminLog(id, 'customer_change', `顧客「${name}」を追加`);
    set({ customerId: id });
    return id;
  },
  renameCustomer(id: string, name: string): void {
    const before = getData().customers.find((c) => c.id === id)?.name ?? '';
    data.updateCustomers(getData().customers.map((c) => (c.id === id ? { ...c, name } : c)));
    adminLog(id, 'customer_change', '顧客名を変更', { column: '顧客名', before, after: name });
  },
  setStage(id: string, stage: CustomerStage): void {
    const before = getData().customers.find((c) => c.id === id)?.stage;
    data.updateCustomers(getData().customers.map((c) => (c.id === id ? { ...c, stage } : c)));
    adminLog(id, 'customer_change', '使い方の段階を変更', { column: '使い方の段階', before: before ? STAGE_LABEL[before] : '', after: STAGE_LABEL[stage] });
  },
  setDisabled(id: string, disabled: boolean): void {
    data.updateCustomers(getData().customers.map((c) => (c.id === id ? { ...c, disabled } : c)));
    adminLog(id, 'customer_change', disabled ? '顧客を停止' : '顧客を再開', { column: '状態', before: disabled ? '利用中' : '停止中', after: disabled ? '停止中' : '利用中' });
  },

  /** 顧客が項目・プルダウンを変えてよいか。 */
  setCustomerCanEdit(sheet: SheetConfig, on: boolean): void {
    data.updateSheet(sheet.id, { customerCanEditColumns: on });
    adminLog(sheet.customerId, 'sheet_change_admin', `「${sheet.tabName}」の項目の変更を${on ? '顧客に許可' : '社内だけに変更'}`, { sheetId: sheet.id, column: '顧客も項目を変更', before: on ? '社内のみ' : '許可', after: on ? '許可' : '社内のみ' });
  },
  setConnection(sheet: SheetConfig, connection: SheetConfig['connection']): void {
    data.updateSheet(sheet.id, { connection });
  },
  saveTabName(sheet: SheetConfig, tabName: string): void {
    data.updateSheet(sheet.id, { tabName });
    adminLog(sheet.customerId, 'sheet_change_admin', 'シートのタブの名前を変更', { sheetId: sheet.id, column: 'タブの名前', before: sheet.tabName, after: tabName });
  },

  /**
   * シートを登録する。data.ts にはシートの追加・削除が無いので、配列を直接書き換えてから
   * updateSheet で通知する（試作だけの手当て。data.ts に addSheet / removeSheet があれば置き換える）。
   */
  addSheet(sheet: SheetConfig, url: string, fileName: string, rows: ApplicantRow[]): void {
    const d = getData();
    d.sheets.push(sheet);
    d.rows[sheet.id] = applicantRows(sheet, rows);
    d.readAt[sheet.id] = new Date().toISOString();
    data.updateSheet(sheet.id, {});
    adminLog(sheet.customerId, 'sheet_change_admin', `シート「${sheet.tabName}」を追加`, { sheetId: sheet.id });
    set({ sheetId: sheet.id, sheetUrls: { ...state.sheetUrls, [sheet.id]: url }, sheetFiles: { ...state.sheetFiles, [sheet.id]: fileName } });
  },
  removeSheet(sheet: SheetConfig): void {
    const d = getData();
    const i = d.sheets.findIndex((x) => x.id === sheet.id);
    if (i >= 0) d.sheets.splice(i, 1);
    data.updateSheet(sheet.id, {});
    adminLog(sheet.customerId, 'sheet_change_admin', `シート「${sheet.tabName}」を外した`);
    if (d.currentSheetId === sheet.id) data.selectCustomer(d.currentCustomerId);
  },
};

/** 社内の人として、この顧客のこのシートを画面で開く準備をする。 */
export function enterAs(customerId: string, sheetId?: string): void {
  data.setViewer({ kind: 'staff', name: STAFF_NAME });
  data.selectCustomer(customerId);
  if (sheetId) data.selectSheet(sheetId);
}

export const STAGE_LABEL: Record<CustomerStage, string> = {
  sheet_only: 'シートだけ',
  both: '両方',
  portal_only: '画面だけ',
};
export const STAGE_TONE: Record<CustomerStage, 'neutral' | 'info' | 'ok'> = {
  sheet_only: 'neutral',
  both: 'info',
  portal_only: 'ok',
};
export const STAGE_MEANING: Record<CustomerStage, string> = {
  sheet_only: '顧客は今までどおりシートを使う。画面は使えない。',
  both: '顧客はシートと画面のどちらにも書ける。シートの変更は 1 分以内に画面へ出る。',
  portal_only: '顧客は画面だけを使う。シートは社内だけで使い、顧客への共有は外す。',
};

export const CONNECTION_LABEL: Record<SheetConfig['connection'], string> = {
  ok: 'つながっている',
  not_shared: '共有されていない',
  not_found: 'シートが見つからない',
  header_changed: '見出しが変わっている',
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
  return sheets.some(isReady) ? null : '先にシートを追加して、項目を設定してください。';
}

/** 一覧に出す注意。段階とシートの状態が合わないとき。 */
export function stageWarning(stage: CustomerStage, sheets: SheetConfig[]): string | null {
  if (stage !== 'sheet_only' && !sheets.some(isReady)) return '画面に出せるシートがない';
  return null;
}
