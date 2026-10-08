import { describe, expect, it } from 'vitest';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import {
  DEFAULT_SELECTED, LEGACY_DEFAULT_SELECTED, LEGACY_PROPS_STORAGE_KEY, MAX_SELECTED_PER_OBJECT, PROPS_STORAGE_KEY, cardAvailableNames, cardSections,
  catalogIndex, loadSelected, migrateLegacySelected, orderedSelection, parseSelected, sanitizeSelected, selectedQuery, toggleSelected, viewValue,
} from './propertyModel';
import { HUBSPOT_CARDS, HUBSPOT_CARDS_CHECKED_AT, cardPropertyNames } from './hubspotCards';
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
  it('the default is the items of the HubSpot cards リスト情報 + BPOアポ情報 (deal properties, card order, no duplicates); no contact / company items', () => {
    expect(DEFAULT_SELECTED.deals).toEqual(cardPropertyNames());
    expect(DEFAULT_SELECTED.deals).toHaveLength(63);
    expect(DEFAULT_SELECTED.deals.slice(0, 4)).toEqual(['bpo_32', 'risuto_kadennbi', 'risuto_saikadennbi', 'saikadenn_zikan']);
    expect(DEFAULT_SELECTED.deals.slice(-3)).toEqual(['tjikanjiku', 'ckyougoujoukyou', 'ahkessaifuro']);
    expect(DEFAULT_SELECTED.deals.length).toBeLessThanOrEqual(MAX_SELECTED_PER_OBJECT);
    expect(DEFAULT_SELECTED.contacts).toEqual([]);
    expect(DEFAULT_SELECTED.companies).toEqual([]);
    expect(PROPS_STORAGE_KEY).toMatch(/\.v2$/);
  });

  it('a v1 choice that was still the old default becomes the new default; a customised v1 choice is kept; the v1 key is removed once saved', () => {
    const v1 = (sel: object) => JSON.stringify({ v: 1, ...sel });
    expect(migrateLegacySelected(v1(LEGACY_DEFAULT_SELECTED))).toEqual(DEFAULT_SELECTED);
    const custom = { deals: ['bpo_50'], contacts: ['lastname'], companies: [] };
    expect(migrateLegacySelected(v1(custom))).toEqual(custom);
    expect(migrateLegacySelected(null)).toBeNull();
    expect(migrateLegacySelected('{')).toBeNull();
    const data: Record<string, string> = { [LEGACY_PROPS_STORAGE_KEY]: v1(LEGACY_DEFAULT_SELECTED) };
    const st = { getItem: (k: string) => data[k] ?? null, setItem: (k: string, v: string) => { data[k] = v; }, removeItem: (k: string) => { Reflect.deleteProperty(data, k); } } as unknown as Storage;
    expect(loadSelected(st)).toEqual(DEFAULT_SELECTED);
    expect(Object.keys(data)).toEqual([PROPS_STORAGE_KEY]);
    // 2 回目は v2 から読む (同じ結果)
    expect(loadSelected(st)).toEqual(DEFAULT_SELECTED);
  });

  it('broken or suspicious stored data falls back to the default', () => {
    for (const raw of [null, '{', '[]', JSON.stringify({ v: 1, deals: [], contacts: [], companies: [] }), JSON.stringify({ v: 2, deals: ['a b'], contacts: [], companies: [] }),
      JSON.stringify({ v: 2, deals: [1], contacts: [], companies: [] }), JSON.stringify({ v: 2, deals: [], contacts: [] }),
      JSON.stringify({ v: 2, deals: Array.from({ length: 101 }, (_, i) => `p${String(i)}`), contacts: [], companies: [] })]) {
      expect(parseSelected(raw), String(raw)).toEqual(DEFAULT_SELECTED);
    }
    expect(parseSelected(JSON.stringify({ v: 2, deals: ['bpo_10', 'bpo_10'], contacts: [], companies: ['name'] }))).toEqual({ deals: ['bpo_10'], contacts: [], companies: ['name'] });
  });

  it('the query sends only non-empty lists, comma separated', () => {
    expect(selectedQuery({ deals: ['bpo_10', 'bpo_32'], contacts: [], companies: ['website'] })).toBe('?deal_props=bpo_10%2Cbpo_32&company_props=website');
    expect(selectedQuery({ deals: [], contacts: [], companies: [] })).toBe('');
  });

  it('names missing from the catalog are dropped (same object back when nothing changes); display follows the HubSpot order', () => {
    const idx = catalogIndex(FIXTURE_CATALOG);
    expect(sanitizeSelected(DEFAULT_SELECTED, idx)).toBe(DEFAULT_SELECTED);
    const stale = { ...LEGACY_DEFAULT_SELECTED, deals: ['bpo_32', 'deleted_prop', 'bpo_10'] };
    expect(sanitizeSelected(stale, idx).deals).toEqual(['bpo_32', 'bpo_10']);
    expect(orderedSelection('deals', stale, idx).map(e => e.prop.name)).toEqual(['bpo_10', 'bpo_32']);
    expect(orderedSelection('deals', stale, idx).map(e => e.group)).toEqual(['Deal information', 'Deal information']);
  });

  it('toggling respects the per-object limit', () => {
    const full = { deals: Array.from({ length: MAX_SELECTED_PER_OBJECT }, (_, i) => `p${String(i)}`), contacts: [], companies: [] };
    expect(toggleSelected(full, 'deals', ['extra'], true)).toBe(full);
    expect(toggleSelected(full, 'deals', ['p0'], false).deals).toHaveLength(MAX_SELECTED_PER_OBJECT - 1);
    expect(toggleSelected(LEGACY_DEFAULT_SELECTED, 'contacts', ['email', 'phone'], true).contacts).toEqual(['lastname', 'firstname', 'phone', 'email']);
  });
});

describe('HubSpot sidebar cards (hubspotCards.json, copied from the HubSpot deal record on 2026-10-08)', () => {
  it('has リスト情報 (open) then BPOアポ情報 (closed), with the exact internal names in HubSpot order', () => {
    expect(HUBSPOT_CARDS_CHECKED_AT).toBe('2026-10-08');
    expect(HUBSPOT_CARDS.map(c => [c.title, c.expanded, c.items.length])).toEqual([['リスト情報', true, 43], ['BPOアポ情報', false, 25]]);
    expect(HUBSPOT_CARDS[0]?.items.map(i => i.name)).toEqual([
      'bpo_32', 'risuto_kadennbi', 'risuto_saikadennbi', 'saikadenn_zikan', 'risuto_bikou', 'service1', 'bpo_43', 'syuuryou', 'kyotenkessaiari',
      'syokusyu_risuto', 'tanntousya', 'hurigana_', 'risuto_tanntouyakusyoku', 'ketteishamei', 'ketteishanoyakushoku', 'kessaishamei',
      'kessaishanoyakushoku', 'daihyou', 'honsya', 'rikulogi_2', 'risuto_adoresu', 'aposyutokubi', 'rikulogi_jigyounaiyou', 'rikulogi_syuugyoubasyo',
      'rikulogi_1', 'miapokayouinsentaku', 'miapokayouinjiyuukinyuu', 'risuto_ninnzuu', 'bosyuriyu', 'risuto_syusyusaki', 'gyoukai', 'etc_',
      'kigyouzentaininzuu', 'syugyoubasyo', 'sihonkin', 'head_office_address', 'recruit_media_observed_list', 'recruit_media_observed_job_categories',
      'recruit_media_observed_urls', 'recruit_media_first_observed_at', 'recruit_media_last_observed_at', 'recruit_media_posting_period_raw',
      'recruit_media_observation_status',
    ]);
    expect(HUBSPOT_CARDS[1]?.items.map(i => i.name)).toEqual([
      'bpo_appo_date', 'scheduled_business_meeting_date', 'negotiation_type', 'bpo_20251', 'bpo_24', 'risuto_ninnzuu', 'bpo_20252', 'bpo_2', 'bpo_1',
      'recruitment_issues', 'bpo_20253', 'remarks_after_calling', 'risuto_tanntouyakusyoku', 'tanntousya', 'hurigana_', 'charge_impression', 'bpo_29',
      'risuto_adoresu', 'bpo_apo_rikulogi', 'bpo_transactio_id', 'bpo_hsurl', 'nkadainofukasa', 'tjikanjiku', 'ckyougoujoukyou', 'ahkessaifuro',
    ]);
    // 表示名は HubSpot のカードのまま
    expect(HUBSPOT_CARDS[0]?.items[0]).toEqual({ label: 'URL_求人検索 ※編集不可', name: 'bpo_32' });
    expect(HUBSPOT_CARDS[1]?.items.at(-1)).toEqual({ label: 'A/H（決裁フロー）', name: 'ahkessaifuro' });
    for (const c of HUBSPOT_CARDS) for (const it of c.items) expect(it.name).toMatch(/^[a-z0-9_]{1,100}$/);
  });

  it('splits the chosen deal items by card (card order, both cards may show the same item); others keep the HubSpot order; missing ones are counted', () => {
    const idx = catalogIndex(FIXTURE_CATALOG);
    const sel = { deals: [...DEFAULT_SELECTED.deals, 'bpo_50', 'gone_from_hubspot'], contacts: [], companies: [] };
    const { sections, others } = cardSections(sel, idx);
    expect(sections.map(x => x.card.title)).toEqual(['リスト情報', 'BPOアポ情報']);
    expect(sections[0]?.entries.slice(0, 3).map(e => e.prop.label)).toEqual(['URL_求人検索 ※編集不可', '架電日', '再架電日']);
    expect(sections[0]?.entries).toHaveLength(43);
    expect(sections[1]?.entries.map(e => e.prop.name)).toContain('tanntousya');
    expect(sections[0]?.entries.map(e => e.prop.name)).toContain('tanntousya');
    expect(sections.map(x => x.unavailable)).toEqual([0, 0]);
    expect(others.map(e => e.prop.name)).toEqual(['bpo_50']);
    // 一覧に無い (HubSpot で隠された) カードの項目は数えるだけで出さない
    const without = catalogIndex({ ...FIXTURE_CATALOG, objects: FIXTURE_CATALOG.objects.map(o => (o.object_type !== 'deals' ? o
      : { ...o, groups: o.groups.map(g => ({ ...g, properties: g.properties.filter(p => p.name !== 'risuto_bikou') })) })) });
    expect(cardSections(DEFAULT_SELECTED, without).sections.map(x => [x.entries.length, x.unavailable])).toEqual([[42, 1], [25, 0]]);
    const listCard = HUBSPOT_CARDS[0];
    if (listCard === undefined) throw new Error('card');
    expect(cardAvailableNames(listCard, without)).toHaveLength(42);
    // 外した項目はどちらのカードにも出ない
    const fewer = toggleSelected(DEFAULT_SELECTED, 'deals', ['tanntousya'], false);
    expect(cardSections(fewer, idx).sections.map(x => x.entries.some(e => e.prop.name === 'tanntousya'))).toEqual([false, false]);
  });
});
