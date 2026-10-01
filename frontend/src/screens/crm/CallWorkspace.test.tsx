import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CallWorkspace } from './CallWorkspace';
import { durationLabel, emptyDraft, nextContact, validateDraft } from './callModel';
import { SAMPLE_RECORDS } from './fixtures';

const contacts = SAMPLE_RECORDS.filter(record => record.objectType === 'contacts');

describe('call workflow', () => {
  it('keeps the initial screen focused on calling and previous handover', () => {
    const html = renderToStaticMarkup(<CallWorkspace />);
    expect(html).toContain('今回の架電先');
    expect(html).toContain('高橋 美咲');
    expect(html).toContain('採用人数と開始時期を確認する。');
    expect(html).toContain('次の人に伝えたいこと');
    expect(html).toContain('デモ記録して次へ');
    expect(html).toContain('実際の発信・HubSpot保存は行いません。');
    expect(html).not.toContain('href="tel:');
    expect(html).not.toContain('¥300,000');
    expect(html).not.toContain('CRMオブジェクト');
  });

  it('requires a result and requires a next date for promised callbacks', () => {
    expect(validateDraft(emptyDraft())).toBe('今回の結果を選んでください。');
    expect(validateDraft({ result: 'callback', memo: '', nextCallAt: '' })).toBe('再架電の日時を入力してください。');
    expect(validateDraft({ result: 'callback', memo: '午後に連絡', nextCallAt: '2026-10-02T14:00' })).toBeNull();
    expect(validateDraft({ result: 'no_answer', memo: '', nextCallAt: '' })).toBeNull();
    expect(validateDraft({ result: 'callback', memo: '', nextCallAt: 'bad date' })).toBe('次回日時を確認してください。');
  });

  it('moves forward, excludes completed/skipped contacts and ends instead of looping', () => {
    expect(nextContact(contacts, 'sample-contact-1', ['sample-contact-1'], [])?.id).toBe('sample-contact-2');
    expect(nextContact(contacts, 'sample-contact-2', ['sample-contact-1'], ['sample-contact-2'])?.id).toBe('sample-contact-3');
    expect(nextContact(contacts, 'sample-contact-3', ['sample-contact-1', 'sample-contact-3'], ['sample-contact-2'])).toBeUndefined();
    expect(nextContact(contacts, 'sample-contact-3', [], [])?.id).toBe('sample-contact-1');
  });

  it('formats simulated call duration consistently', () => {
    expect(durationLabel(0)).toBe('00:00');
    expect(durationLabel(65)).toBe('01:05');
  });
});
