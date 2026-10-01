import type { CrmRecord } from './model';
import type { CallResult } from './callModel';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { MocPropertyDefinition } from './mocProperties';
import { SAMPLE_RECORDS } from './fixtures';

export interface BatchDraft { result: CallResult | ''; memo: string; spokeTo: string; interest: string; nextAction: string; nextCallDate: string; nextCallTime: string }
export const emptyBatchDraft = (): BatchDraft => ({ result: '', memo: '', spokeTo: '', interest: '', nextAction: '', nextCallDate: '', nextCallTime: '' });

export function validateBatchDraft(draft: BatchDraft, definitions: Record<string, MocPropertyDefinition> = MOC_DEAL_PROPERTIES): string | null {
  if (!draft.result) return '今回の結果を選んでください。';
  for (const [name, value] of [['bpo_40', draft.spokeTo], ['bpo_42', draft.interest], ['bpo_45', draft.nextAction], ['bpo_14', draft.nextCallTime]]) {
    if (value && !definitions[name ?? '']?.options.some(option => option.value === value)) return '選択肢を確認してください。';
  }
  const requiresNext = draft.result === 'callback' || draft.nextAction === '再架電';
  if ((requiresNext || draft.nextCallDate || draft.nextCallTime) && (!draft.nextCallDate || !draft.nextCallTime)) return '次回架電の日付と時間を入力してください。';
  if (draft.nextCallDate && (!/^\d{4}-\d{2}-\d{2}$/.test(draft.nextCallDate) || !Number.isFinite(Date.parse(draft.nextCallDate))
    || new Date(draft.nextCallDate).toISOString().slice(0, 10) !== draft.nextCallDate)) return '次回架電の日付を確認してください。';
  if (draft.result === 'do_not_call' && (draft.nextCallDate || draft.nextCallTime || draft.nextAction)) return '架電停止時は次回予定と次アクションを解除してください。';
  return null;
}
const original = SAMPLE_RECORDS.filter(record => record.objectType === 'contacts');
const template = original[0];
if (!template) throw new Error('Batch preview requires a fictional contact fixture.');
// Additional fictional contacts are batch-preview-only; never clone business history or emails.
export const BATCH_CONTACTS: CrmRecord[] = [...original, ...[
  '田中 花', '佐藤 翔', '伊藤 結衣', '中村 健太', '渡辺 愛', '山口 直樹', '松本 彩', '井上 誠', '石井 恵',
].map((name, index): CrmRecord => ({
  ...template, id: `sample-batch-${String(index + 4)}`, name, subtitle: '架空の担当者',
  owner: index % 2 === 0 ? '佐々木 葵' : '山本 健', status: '未コンタクト',
  properties: [{ name: 'phone', label: '電話番号', value: index === 4 ? null : 'サンプル（発信不可）' },
    { name: 'jobtitle', label: '役職', value: '採用担当' }],
  activities: [], associations: [{ objectType: 'companies', id: index % 2 === 0 ? 'sample-company-1' : 'sample-company-2', label: '主たる会社' }],
}))];
export interface BatchFilters { query: string; owner: string; state: string; phone: string }
export const emptyBatchFilters = (): BatchFilters => ({ query: '', owner: '', state: '', phone: '' });
export function filterBatchContacts(records: CrmRecord[], filters: BatchFilters, completed: string[], skipped: string[], propertyDrafts: Record<string, string> = {}): CrmRecord[] {
  const query = filters.query.trim().normalize('NFKC').toLocaleLowerCase('ja');
  return records.filter(record => {
    const company = SAMPLE_RECORDS.find(r => r.id === record.associations.find(a => a.objectType === 'companies')?.id)?.name ?? '';
    const hasPhone = !!(propertyDrafts[`${record.id}:phone`] ?? record.properties.find(p => p.name === 'phone')?.value)?.trim();
    const state = completed.includes(record.id) ? 'done' : skipped.includes(record.id) ? 'skipped' : 'pending';
    return (!query || `${record.name} ${company} ${record.subtitle}`.normalize('NFKC').toLocaleLowerCase('ja').includes(query))
      && (!filters.owner || filters.owner === record.owner)
      && (!filters.state || filters.state === state)
      && (!filters.phone || (filters.phone === 'yes' ? hasPhone : !hasPhone));
  });
}
