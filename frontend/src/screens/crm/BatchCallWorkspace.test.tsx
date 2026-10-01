import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { BatchCallWorkspace } from './BatchCallWorkspace';
import { SAMPLE_RECORDS } from './fixtures';
import { nextContact } from './callModel';
import { BATCH_CONTACTS, emptyBatchFilters, filterBatchContacts, emptyBatchDraft, validateBatchDraft } from './batchModel';
import { ContactProperties } from './ContactProperties';

describe('batch calling workspace', () => {
  it('uses shared company drafts without changing source records or contact data', () => {
    const first = BATCH_CONTACTS.find(record => record.id === 'sample-contact-1');
    const third = BATCH_CONTACTS.find(record => record.id === 'sample-contact-3');
    if (!first || !third) throw new Error('Missing fictional contacts');
    const values = { 'sample-company-1:city': '臼杵市', 'sample-moc-deal-sample-contact-1:bpo_49': '3人', 'sample-contact-1:jobtitle': '採用責任者' };
    const one = renderToStaticMarkup(<ContactProperties record={first} values={values} onChange={() => undefined} />);
    const three = renderToStaticMarkup(<ContactProperties record={third} values={values} onChange={() => undefined} />);
    expect(one).toContain('value="臼杵市"');
    expect(three).toContain('value="臼杵市"');
    expect(one).toContain('value="3人"');
    expect(three).not.toContain('value="3人"');
    expect(one).toContain('value="採用責任者"');
    expect(three).toContain('value="拠点責任者"');
    expect(SAMPLE_RECORDS.find(record => record.id === 'sample-company-1')?.properties.find(p => p.name === 'city')?.value).toBe('大分市');
  });
  it('validates actual time options and callback dates instead of free text dates', () => {
    const callback = { ...emptyBatchDraft(), result: 'callback' as const };
    expect(validateBatchDraft(callback)).toBe('次回架電の日付と時間を入力してください。');
    expect(validateBatchDraft({ ...callback, nextCallDate: '2026-10-02', nextCallTime: '8:15' })).toBeNull();
    expect(validateBatchDraft({ ...callback, nextCallDate: '2026-10-02', nextCallTime: '8:17' })).toBe('選択肢を確認してください。');
    expect(validateBatchDraft({ ...callback, nextCallDate: '2026-02-30', nextCallTime: '8:15' })).toBe('次回架電の日付を確認してください。');
    expect(validateBatchDraft({ ...callback, nextCallDate: '2026-10-02', nextCallTime: '8:15', interest: '情報収集中' })).toBe('選択肢を確認してください。');
  });
  it('combines filters and searches associated company names', () => {
    const filters = { ...emptyBatchFilters(), query: 'サンプル物流', owner: '佐々木 葵', phone: 'yes', state: 'pending' };
    const found = filterBatchContacts(BATCH_CONTACTS, filters, ['sample-contact-1'], ['sample-contact-3']);
    expect(found.map(record => record.id)).toEqual(['sample-batch-4', 'sample-batch-6', 'sample-batch-10', 'sample-batch-12']);
    expect(filterBatchContacts(BATCH_CONTACTS, { ...emptyBatchFilters(), query: '  森 大輔  ' }, [], []).map(r => r.id)).toEqual(['sample-contact-2']);
    expect(filterBatchContacts(BATCH_CONTACTS, { ...emptyBatchFilters(), state: 'done' }, ['sample-contact-1'], []).map(r => r.id)).toEqual(['sample-contact-1']);
    expect(filterBatchContacts(BATCH_CONTACTS, { ...emptyBatchFilters(), query: '存在しない会社' }, [], [])).toEqual([]);
    expect(filterBatchContacts(BATCH_CONTACTS, { ...emptyBatchFilters(), query: '高橋', phone: 'yes' }, [], [], { 'sample-contact-1:phone': '' })).toEqual([]);
    expect(filterBatchContacts(BATCH_CONTACTS, { ...emptyBatchFilters(), query: '森', phone: 'yes' }, [], [], { 'sample-contact-2:phone': '発信不可サンプル' }).map(r => r.id)).toEqual(['sample-contact-2']);
  });
  it('shows all customers with separate result and memo controls', () => {
    const html = renderToStaticMarkup(<BatchCallWorkspace />);
    for (const record of SAMPLE_RECORDS.filter(record => record.objectType === 'contacts')) {
      expect(html).toContain(record.name);
      expect(html).toContain(`id="batch-result-${record.id}"`);
      expect(html).toContain(`id="batch-memo-${record.id}"`);
    }
    expect(html).toContain('電話番号なし');
    expect(html).toContain('発信・記録はデモです');
    expect(html).not.toContain('href="tel:');
  });

  it('advances past missing numbers and stops after eligible rows are completed', () => {
    const callable = SAMPLE_RECORDS.filter(record => record.objectType === 'contacts'
      && record.properties.find(property => property.name === 'phone')?.value);
    expect(nextContact(callable, 'sample-contact-1', ['sample-contact-1'], [])?.id).toBe('sample-contact-3');
    expect(nextContact(callable, 'sample-contact-3', ['sample-contact-1', 'sample-contact-3'], [])).toBeUndefined();
    expect(nextContact(callable, 'sample-contact-1', [], ['sample-contact-3'])).toBeUndefined();
  });
});
