import type { CrmRecord } from './model';

export const CALL_RESULTS = {
  connected: '担当者と会話', no_answer: '不在・応答なし', callback: '再架電の約束',
  appointment: 'アポイント獲得', wrong_number: '番号違い', do_not_call: '架電停止の希望',
} as const;
export type CallResult = keyof typeof CALL_RESULTS;
export interface CallDraft { result: CallResult | ''; memo: string; nextCallAt: string }
export interface DemoCallEntry extends CallDraft { duration: number }
export const emptyDraft = (): CallDraft => ({ result: '', memo: '', nextCallAt: '' });

export function validateDraft(draft: CallDraft): string | null {
  if (draft.result === '') return '今回の結果を選んでください。';
  if (draft.result === 'callback' && !draft.nextCallAt) return '再架電の日時を入力してください。';
  if (draft.nextCallAt && !Number.isFinite(Date.parse(draft.nextCallAt))) return '次回日時を確認してください。';
  return null;
}

export function nextContact(contacts: CrmRecord[], currentId: string, completed: string[], skipped: string[]): CrmRecord | undefined {
  const current = contacts.findIndex(c => c.id === currentId);
  const ordered = [...contacts.slice(current + 1), ...contacts.slice(0, current)];
  return ordered.find(c => !completed.includes(c.id) && !skipped.includes(c.id));
}

export function durationLabel(seconds: number): string {
  return `${Math.floor(seconds / 60).toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
}
