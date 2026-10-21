// 操作の記録の 1 行の組み立て。操作の記録の表とアカウント・顧客の「最近の記録」で同じ言い方にそろえる。
import { getData } from '../../data';
import type { DataState } from '../../data';
import { STAFF_NAME } from '../../fixtures';
import type { ActivityEntry, ActivityKind } from '../../types';
import { columnLabel } from '../../activityText';

const STAFF_KINDS: ActivityKind[] = ['invite', 'disable', 'device_removed', 'account_unlock', 'password_reset', 'customer_change', 'sheet_change_admin', 'column_change'];

export function personOf(email: string): string {
  const a = getData().accounts.find((x) => x.email === email);
  return a && a.displayName !== '' ? a.displayName : email;
}

/** 誰が。名前が記録に無い社内の操作は、操作した社内の人の名前にする。 */
export function actorOf(e: ActivityEntry): string {
  if (e.actorName !== undefined && e.actorName !== '') return e.actorName;
  if (STAFF_KINDS.includes(e.kind)) return STAFF_NAME;
  return e.accountEmail !== '' ? personOf(e.accountEmail) : '社内';
}

export function applicantOf(d: DataState, e: ActivityEntry): string {
  if (e.sheetId === undefined || e.rowNumber === undefined) return '';
  const sheet = d.sheets.find((s) => s.id === e.sheetId);
  const row = d.rows[e.sheetId]?.find((r) => r.rowNumber === e.rowNumber);
  return row?.values[sheet?.titleColumn ?? ''] ?? '';
}

const splitList = (s: string): string[] => s.split(/[、,]/).map((x) => x.trim()).filter((x) => x !== '');

/** 変わった内容を 1 行の文字にする。 */
export function changeText(e: ActivityEntry): string {
  const hasDiff = e.before !== undefined || e.after !== undefined;
  const before = e.before ?? '';
  const after = e.after ?? '';
  if (e.kind === 'save_conflict') return `入れようとした値「${after !== '' ? after : '（空）'}」は、シートで先に変更されていたため保存しなかった`;
  if (!hasDiff) return e.detail;
  if (e.kind === 'column_change' && (before.includes('、') || after.includes('、'))) {
    const b = splitList(before);
    const a = splitList(after);
    const added = a.filter((x) => !b.includes(x));
    const removed = b.filter((x) => !a.includes(x));
    const parts = [added.length > 0 ? `追加: ${added.join('、')}` : '', removed.length > 0 ? `削除: ${removed.join('、')}` : ''].filter((x) => x !== '');
    return parts.length > 0 ? parts.join(' ／ ') : '並び順を変更';
  }
  if (before !== '' && after.startsWith(before) && after.length > before.length) return `追記: ${after.slice(before.length).trim()}`;
  return `${before !== '' ? before : '（空）'} → ${after !== '' ? after : '（空）'}`;
}

/** 「最近の記録」用の 1 文。応募者がいれば「名前 ／ 項目 ／ 変更」の形。 */
export function entrySentence(d: DataState, e: ActivityEntry): string {
  const app = applicantOf(d, e);
  const parts = [app, columnLabel(d.sheets, e.sheetId, e.column), changeText(e)].filter((x) => x !== '');
  return app !== '' || e.column !== undefined ? parts.join(' ／ ') : changeText(e);
}
