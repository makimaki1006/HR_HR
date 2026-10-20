// 操作の記録の言い方。顧客の「記録」・詳細の「この応募者の記録」・社内の「操作の記録」で同じ言葉を使う。
import { labelOf } from './columns';
import type { ActivityEntry, ActivityKind, SheetConfig } from './types';

/** どこからの操作か。 */
export const SOURCE_LABEL = { portal: '画面', sheet: 'シート', admin: '項目の設定' } as const;

export function sourceLabel(e: ActivityEntry): string {
  const s = e.source ?? (e.kind === 'sheet_change' ? 'sheet' : e.kind === 'column_change' ? 'admin' : 'portal');
  if (s === 'admin' && e.kind !== 'column_change') return '社内の管理画面';
  return SOURCE_LABEL[s];
}

/** 種類の名前。 */
export const KIND_LABEL: Record<ActivityKind, string> = {
  save: '画面で変更',
  sheet_change: 'シートで変更',
  save_conflict: '保存しなかった',
  column_change: '項目の変更',
  option_replace: '選択肢の置き換え',
  login: 'ログイン',
  login_failed: 'ログイン失敗',
  code_sent: '確認コード',
  view: '閲覧',
  export: '書き出し',
  invite: '招待',
  disable: 'アカウントの停止',
  customer_change: '顧客の設定',
  sheet_change_admin: 'シートの設定',
  account_unlock: 'ロック解除',
  password_reset: '再設定のリンク',
  device_removed: '端末の削除',
};

/** 1 件の種類の名前。応募者に関係しない「保存」はアカウントの変更として出す。 */
export function kindLabel(e: ActivityEntry): string {
  if (e.kind === 'save' && e.sheetId === undefined) return 'アカウントの変更';
  return KIND_LABEL[e.kind];
}

export const KIND_DOT: Record<ActivityKind, string> = {
  save: 'bg-pt-ok',
  sheet_change: 'bg-pt-primary',
  save_conflict: 'bg-pt-warn',
  column_change: 'bg-pt-primary',
  option_replace: 'bg-pt-primary',
  login: 'bg-pt-faint',
  login_failed: 'bg-pt-danger',
  code_sent: 'bg-pt-faint',
  view: 'bg-pt-faint',
  export: 'bg-pt-faint',
  invite: 'bg-pt-faint',
  disable: 'bg-pt-danger',
  customer_change: 'bg-pt-primary',
  sheet_change_admin: 'bg-pt-primary',
  account_unlock: 'bg-pt-faint',
  password_reset: 'bg-pt-faint',
  device_removed: 'bg-pt-faint',
};

/** 応募者のシートに関係しない操作（ログイン・パスワード・端末など）。 */
export const isAccountEntry = (e: ActivityEntry): boolean => e.sheetId === undefined;

/** 項目の名前。シートの見出しではなく、画面での表示名にする。 */
export function columnLabel(sheets: SheetConfig[], sheetId: string | undefined, column: string | undefined): string {
  if (column === undefined) return '';
  const c = sheets.find((s) => s.id === sheetId)?.columns.find((x) => x.header === column);
  return c ? labelOf(c) : column;
}
