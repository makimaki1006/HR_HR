import { describe, expect, it } from 'vitest';
import type { CrmOwner } from '../../generated/CrmOwner';
import { FIXTURE_OWNERS, ownerLabel, ownerNameMap, visibleOwners } from './ownerModel';

const o = (id: string, name: string, email: string | null, archived = false): CrmOwner => ({ id, name, email, archived });
const LIST = [
  o('1', '架空 一郎', 'ichiro@example.invalid'),
  o('2', '架空 一郎', 'ichiro2@example.invalid'),
  o('3', 'サンプル 花子', null),
  o('4', 'ダミー 退職', 'gone@example.invalid', true),
];

describe('visibleOwners', () => {
  it('hides archived owners by default and shows them when asked', () => {
    expect(visibleOwners(LIST, '', false, '').map(x => x.id)).toEqual(['1', '2', '3']);
    expect(visibleOwners(LIST, '', true, '').map(x => x.id)).toEqual(['1', '2', '3', '4']);
  });
  it('searches name, email and id with every word (full-width and case insensitive)', () => {
    expect(visibleOwners(LIST, '一郎', false, '').map(x => x.id)).toEqual(['1', '2']);
    expect(visibleOwners(LIST, '架空 ICHIRO2', false, '').map(x => x.id)).toEqual(['2']);
    expect(visibleOwners(LIST, 'ＩＣＨＩＲＯ2', false, '').map(x => x.id)).toEqual(['2']);
    expect(visibleOwners(LIST, '3', false, '').map(x => x.id)).toEqual(['3']);
    expect(visibleOwners(LIST, '存在しない', false, '')).toEqual([]);
  });
  it('always keeps the selected owner, even when archived or not matching the search', () => {
    expect(visibleOwners(LIST, '花子', false, '4').map(x => x.id)).toEqual(['3', '4']);
  });
  it('handles zero owners', () => {
    expect(visibleOwners([], 'x', true, '9')).toEqual([]);
  });
});

describe('ownerLabel / ownerNameMap', () => {
  it('tells same-named people apart by email, falls back to ID, marks the retired', () => {
    expect(ownerLabel(o('1', '架空 一郎', 'ichiro@example.invalid'))).toBe('架空 一郎(ichiro@example.invalid)');
    expect(ownerLabel(o('2', '架空 一郎', 'ichiro2@example.invalid'))).not.toBe(ownerLabel(o('1', '架空 一郎', 'ichiro@example.invalid')));
    expect(ownerLabel(o('3', 'サンプル 花子', null))).toBe('サンプル 花子(ID 3)');
    expect(ownerLabel(o('4', 'ダミー 退職', null, true))).toContain('[退職者]');
    expect(ownerNameMap(LIST).get('3')).toBe('サンプル 花子');
    expect(ownerNameMap(LIST).get('999')).toBeUndefined();
  });
  it('fixture owners are fictional (example.invalid only)', () => {
    for (const f of FIXTURE_OWNERS) if (f.email) expect(f.email.endsWith('@example.invalid')).toBe(true);
  });
});
