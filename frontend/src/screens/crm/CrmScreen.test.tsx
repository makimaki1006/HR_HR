import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CrmReferenceScreen as CrmScreen, RecordView } from './CrmScreen';
import { SAMPLE_RECORDS } from './fixtures';
import { associationGroups, filterActivities, findRecords, formatDate } from './model';
import type { CrmRecord } from './model';

function sample(id: string): CrmRecord {
  const record = SAMPLE_RECORDS.find(r => r.id === id);
  if (!record) throw new Error(`missing sample ${id}`);
  return record;
}
const contact = sample('sample-contact-1');

describe('CRM reference prototype', () => {
  it('makes sample data and unavailable write/call actions explicit', () => {
    const html = renderToStaticMarkup(<CrmScreen />);
    expect(html).toContain('表示内容はすべて架空です。');
    expect(html).toContain('HubSpotへの接続・保存・発信は行いません。');
    expect(html).toContain('高橋 美咲');
    expect(html).toContain('misaki.takahashi@example.com');
    expect(html).not.toContain('href="tel:');
    expect(html).not.toContain('https://app.hubspot.com/');
    expect(SAMPLE_RECORDS.every(r => r.deepLink === null)).toBe(true);
  });

  it('searches contact properties, normalizes whitespace/case and respects object type', () => {
    expect(findRecords(SAMPLE_RECORDS, 'contacts', ' MISAKI.TAKAHASHI@EXAMPLE.COM ').map(r => r.id))
      .toEqual(['sample-contact-1']);
    expect(findRecords(SAMPLE_RECORDS, 'contacts', 'サンプル物流').map(r => r.id))
      .toEqual(['sample-contact-1', 'sample-contact-3']);
    expect(findRecords(SAMPLE_RECORDS, 'companies', 'サンプル物流').map(r => r.id))
      .toEqual(['sample-company-1']);
    expect(findRecords(SAMPLE_RECORDS, 'contacts', '不存在')).toEqual([]);
  });

  it('combines activity filters and sorts by occurrence rather than input order', () => {
    expect(filterActivities(contact.activities, 'all', '', '', '').map(a => a.id))
      .toEqual(['sample-task-1', 'sample-note-1', 'sample-call-1', 'sample-email-1']);
    expect(filterActivities(contact.activities, 'call', '午後', '佐々木 葵', '2026-09-30T14:30:00+09:00').map(a => a.id))
      .toEqual(['sample-call-1']);
    expect(filterActivities(contact.activities, 'call', '', '山本 健', '')).toEqual([]);
    expect(filterActivities(contact.activities, 'call', '', '', '2026-09-30T14:31:00+09:00')).toEqual([]);
  });

  it('resolves company/contact/deal associations with primary company label', () => {
    const groups = associationGroups(contact, SAMPLE_RECORDS);
    expect(groups.find(g => g.objectType === 'companies')?.records[0]?.record?.name)
      .toBe('サンプル物流株式会社');
    expect(groups.find(g => g.objectType === 'companies')?.records[0]?.association.label).toBe('主たる会社');
    expect(groups.find(g => g.objectType === 'deals')?.records[0]?.record?.properties.find(p => p.name === 'amount')?.value)
      .toBe('¥300,000');
    expect(associationGroups(sample('sample-company-1'), SAMPLE_RECORDS)
      .find(g => g.objectType === 'contacts')?.records.map(r => r.record?.id))
      .toEqual(['sample-contact-1', 'sample-contact-3']);
  });

  it('shows missing phone and genuinely empty sample activity history', () => {
    const html = renderToStaticMarkup(<RecordView record={sample('sample-contact-2')}
      records={SAMPLE_RECORDS} onSelect={() => undefined} />);
    expect(html).toContain('未設定');
    expect(html).toContain('記録された活動はありません');
    expect(html).toContain('daisuke.mori@example.com');
  });

  it('renders activity body as text, including untrusted markup', () => {
    const activity = contact.activities[0];
    if (!activity) throw new Error('missing activity fixture');
    const html = renderToStaticMarkup(<RecordView record={{ ...contact, activities: [{
      ...activity, body: '<img src=x onerror=alert(1)>',
    }] }} records={SAMPLE_RECORDS} onSelect={() => undefined} />);
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
    expect(html).not.toContain('<img src=x');
  });

  it('uses Japan timezone consistently for dates', () => {
    expect(formatDate('2026-09-30T05:30:00Z')).toBe('2026/09/30 14:30');
  });
});
