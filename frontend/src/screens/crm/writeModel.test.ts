import { describe, expect, it } from 'vitest';
import rules from '../../../../src/crm/stage_rules.json';
import defs from '../../../../src/crm/stage_rule_property_defs.json';
import { datetimeToJstDate, draftToValue, initialDraft, jstDateToMillis, kindOf } from './writeModel';

describe('日時 (datetime) の項目', () => {
  it('type=datetime / fieldType=date は日付の入力欄になる (クラウドサイン送付期限)', () => {
    expect(kindOf('datetime', 'date', 0)).toBe('datetime');
  });

  it('入力した JST の日付は、その日の 0 時 (JST) のミリ秒で送る', () => {
    // 2026-10-20 00:00 JST = 2026-10-19 15:00 UTC
    expect(draftToValue('datetime', '2026-10-20')).toBe(String(Date.UTC(2026, 9, 19, 15, 0, 0)));
    expect(draftToValue('datetime', '')).toBeNull();
    expect(draftToValue('datetime', 'abc')).toBeNull();
    expect(jstDateToMillis('2026-10-20')).toBe(String(Date.UTC(2026, 9, 19, 15, 0, 0)));
  });

  it('HubSpot の日時 (ミリ秒・ISO) は JST の日付で見せる。0 時 JST は元の日に戻る', () => {
    const midnightJst = String(Date.UTC(2026, 9, 19, 15, 0, 0));
    expect(initialDraft('datetime', midnightJst)).toBe('2026-10-20');
    expect(initialDraft('datetime', '2026-10-19T15:00:00Z')).toBe('2026-10-20');
    // UTC では前日でも JST では翌日の朝
    expect(datetimeToJstDate('2026-10-19T20:00:00.000Z')).toBe('2026-10-20');
    expect(initialDraft('datetime', null)).toBe('');
    expect(initialDraft('datetime', 'not a date')).toBe('');
  });

  it('変えていない値は変更として扱わない (時刻つきの元の値でも同じ日なら同じ)', () => {
    const base = '2026-10-19T20:00:00.000Z';
    expect(draftToValue('datetime', initialDraft('datetime', base))).toBe(draftToValue('datetime', '2026-10-20'));
  });
});

describe('ステージの必須項目はすべて画面で入力できる', () => {
  interface Def { name: string; type: string; fieldType: string; options: unknown[]; referencedObjectType?: string }
  const byName = new Map((defs.deals as Def[]).map(d => [d.name, d]));
  const required = [...new Set(rules.rules.flatMap(r => r.props.filter(p => p.required).map(p => p.name)))];

  it('必須の項目が 40 個以上あり、すべて定義がある', () => {
    expect(required.length).toBeGreaterThan(40);
    expect(required).toContain('deadline_of_cloud_sign');
    expect(required.filter(n => !byName.has(n))).toEqual([]);
  });

  it('入力部品が作れない必須項目は無い', () => {
    const none = required.filter(n => {
      const d = byName.get(n);
      return d === undefined || kindOf(d.type, d.fieldType, d.options.length, d.referencedObjectType === 'OWNER') === null;
    });
    expect(none).toEqual([]);
  });
});
