// 試作の画面の登録と、画面どうしの移動。各チームは自分のフォルダの index.ts で screens を出す。
import { createContext, useContext, type ReactNode } from 'react';

/** 画面の移動先として、ほかのチームが使ってよい名前。 */
export const ROUTES = {
  login: 'auth-login',
  loggedOut: 'auth-logged-out',
  list: 'list',
  metrics: 'metrics',
  history: 'history',
  columns: 'columns',
  account: 'account',
  adminCustomers: 'admin-customers',
  adminAccounts: 'admin-accounts',
} as const;

export interface PreviewScreen {
  /** URL の # に入る名前。チームの中で一意（例: auth-code）。 */
  id: string;
  group: '顧客（ログイン前）' | '顧客（ログイン後）' | '社内の管理';
  /** 切り替えの一覧に出す名前（例: 「確認コード（新しい端末）」）。 */
  title: string;
  render: () => ReactNode;
}

export interface PreviewNav {
  go: (id: string) => void;
}

export const PreviewNavContext = createContext<PreviewNav>({ go: () => undefined });

export function usePreviewNav(): PreviewNav {
  return useContext(PreviewNavContext);
}
