import { describe, expect, it } from 'vitest';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import {
  DEFAULT_SELECTED, MAX_SELECTED_PER_OBJECT, PROPS_STORAGE_KEY, catalogIndex, orderedSelection, parseSelected, sanitizeSelected, selectedQuery,
  toggleSelected, viewValue,
} from './propertyModel';
import { FIXTURE_CATALOG } from './usePropertyCatalog';

const prop = (over: Partial<CrmCatalogProperty>): CrmCatalogProperty => ({ name: 'x', label: 'X', property_type: 'string', field_type: 'text', options: [], ...over });
const owners = new Map([['9001', '架空 一郎']]);
const text = (p: CrmCatalogProperty, raw: string | null) => {
  const v = viewValue(p, raw, owners);
  return v.kind === 'text' ? v.text : v.kind;
};

describe('viewValue (how a property value is shown)', () => {
  it('enumeration values show the option label; multiple values (a;b) are joined; unknown values stay as they are', () => {
    const p = prop({ property_type: 'enumeration', field_type: 'checkbox', options: [{ label: '現在使われておりません', value: '使われておりません', hidden: false }, { label: '常時通話中', value: 'busy', hidden: false }] });
    expect(text(p, '使われておりません')).toBe('現在使われておりません');
    expect(text(p, 'busy;使われておりません')).toBe('常時通話中、現在使われておりません');
    expect(text(p, 'other')).toBe('other');
  });

  it('empty values are 「未入力」 (kind empty)', () => {
    expect(viewValue(prop({}), null, owners)).toEqual({ kind: 'empty' });
    expect(viewValue(prop({}), '   ', owners)).toEqual({ kind: 'empty' });
    expect(viewValue(prop({}), undefined, owners)).toEqual({ kind: 'empty' });
  });

  it('dates are YYYY/MM/DD; date-times are Japan time; booleans are はい / いいえ; numbers get separators', () => {
    expect(text(prop({ property_type: 'date' }), '2026-10-05')).toBe('2026/10/05');
    expect(text(prop({ property_type: 'date' }), '2026-10-05T00:00:00.000Z')).toBe('2026/10/05');
    expect(text(prop({ property_type: 'datetime' }), '2026-10-04T15:30:00Z')).toBe('2026/10/05 00:30');
    expect(text(prop({ property_type: 'datetime' }), '1791127800000')).toBe('2026/10/05 00:30');
    expect(text(prop({ property_type: 'bool' }), 'true')).toBe('はい');
    expect(text(prop({ property_type: 'bool' }), 'false')).toBe('いいえ');
    expect(text(prop({ property_type: 'number' }), '1200000')).toBe('1,200,000');
    expect(text(prop({ property_type: 'number' }), '12.5')).toBe('12.5');
  });

  it('the owner shows the name (or 担当あり), never the owner ID; phone numbers are hyphenated', () => {
    expect(text(prop({ name: 'hubspot_owner_id', property_type: 'enumeration' }), '9001')).toBe('架空 一郎');
    expect(text(prop({ name: 'hubspot_owner_id', property_type: 'enumeration' }), '123')).toBe('担当あり');
    expect(text(prop({ field_type: 'phonenumber' }), '+81312345678')).toBe('03-1234-5678');
  });

  it('values that are only http(s) URLs become links; text with a URL inside or javascript: stays text', () => {
    expect(viewValue(prop({}), 'https://a.example/x\nhttps://b.example/', owners)).toEqual({ kind: 'links', urls: ['https://a.example/x', 'https://b.example/'] });
    expect(viewValue(prop({}), '求人は https://a.example/ を参照', owners).kind).toBe('text');
    expect(viewValue(prop({}), 'javascript:alert(1)', owners).kind).toBe('text');
    expect(viewValue(prop({}), '一行目\n二行目', owners)).toEqual({ kind: 'text', text: '一行目\n二行目', multiline: true });
  });
});

describe('the chosen properties (stored per browser)', () => {
  it('the default is the call fields + URL_求人検索 + contact name / phone + company site', () => {
    expect(DEFAULT_SELECTED.deals).toEqual(['hubspot_owner_id', 'bpo_13', 'bpo_14', 'bpo_20', 'bpo_10', 'bpo_3', 'bpo_4', 'bpo_32']);
    expect(DEFAULT_SELECTED.contacts).toEqual(['lastname', 'firstname', 'phone']);
    expect(DEFAULT_SELECTED.companies).toEqual(['website']);
    expect(PROPS_STORAGE_KEY).toMatch(/\.v1$/);
  });

  it('broken or suspicious stored data falls back to the default', () => {
    for (const raw of [null, '{', '[]', JSON.stringify({ v: 2, deals: [], contacts: [], companies: [] }), JSON.stringify({ v: 1, deals: ['a b'], contacts: [], companies: [] }),
      JSON.stringify({ v: 1, deals: [1], contacts: [], companies: [] }), JSON.stringify({ v: 1, deals: [], contacts: [] }),
      JSON.stringify({ v: 1, deals: Array.from({ length: 101 }, (_, i) => `p${String(i)}`), contacts: [], companies: [] })]) {
      expect(parseSelected(raw), String(raw)).toEqual(DEFAULT_SELECTED);
    }
    expect(parseSelected(JSON.stringify({ v: 1, deals: ['bpo_10', 'bpo_10'], contacts: [], companies: ['name'] }))).toEqual({ deals: ['bpo_10'], contacts: [], companies: ['name'] });
  });

  it('the query sends only non-empty lists, comma separated', () => {
    expect(selectedQuery({ deals: ['bpo_10', 'bpo_32'], contacts: [], companies: ['website'] })).toBe('?deal_props=bpo_10%2Cbpo_32&company_props=website');
    expect(selectedQuery({ deals: [], contacts: [], companies: [] })).toBe('');
  });

  it('names missing from the catalog are dropped (same object back when nothing changes); display follows the HubSpot order', () => {
    const idx = catalogIndex(FIXTURE_CATALOG);
    expect(sanitizeSelected(DEFAULT_SELECTED, idx)).toBe(DEFAULT_SELECTED);
    const stale = { ...DEFAULT_SELECTED, deals: ['bpo_32', 'deleted_prop', 'bpo_10'] };
    expect(sanitizeSelected(stale, idx).deals).toEqual(['bpo_32', 'bpo_10']);
    expect(orderedSelection('deals', stale, idx).map(e => e.prop.name)).toEqual(['bpo_10', 'bpo_32']);
    expect(orderedSelection('deals', stale, idx).map(e => e.group)).toEqual(['Deal information', 'Deal information']);
  });

  it('toggling respects the per-object limit', () => {
    const full = { deals: Array.from({ length: MAX_SELECTED_PER_OBJECT }, (_, i) => `p${String(i)}`), contacts: [], companies: [] };
    expect(toggleSelected(full, 'deals', ['extra'], true)).toBe(full);
    expect(toggleSelected(full, 'deals', ['p0'], false).deals).toHaveLength(MAX_SELECTED_PER_OBJECT - 1);
    expect(toggleSelected(DEFAULT_SELECTED, 'contacts', ['email', 'phone'], true).contacts).toEqual(['lastname', 'firstname', 'phone', 'email']);
  });
});
