import retrievedDefinitions from '../../../../docs/research/hubspot-read-foundation/account-property-definitions.json?raw';
import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { MocPropertyInput, ContactProperties } from './ContactProperties';
import { MOC_DEAL_PROPERTIES, MOC_PROPERTIES_RETRIEVED_AT, propertyDraftKey, togglePropertyValue } from './mocProperties';
import { SAMPLE_RECORDS } from './fixtures';

describe('HubSpot definition backed MOC inputs', () => {
  it('matches the retrieved labels, types and internal option values without substituting UI labels', () => {
    const source = JSON.parse(retrievedDefinitions) as {
      retrieved_at: string; objects: { deals: { properties: { name: string; label: string; type: string; fieldType: string; options: { label: string; value: string; hidden: boolean }[] }[] } };
    };
    expect(MOC_PROPERTIES_RETRIEVED_AT).toBe(source.retrieved_at);
    for (const definition of Object.values(MOC_DEAL_PROPERTIES)) {
      const original = source.objects.deals.properties.find(p => p.name === definition.name);
      expect(original).toBeDefined();
      expect({ label: definition.label, type: definition.type, fieldType: definition.fieldType, options: definition.options })
        .toEqual({ label: original?.label, type: original?.type, fieldType: original?.fieldType, options: original?.options.map(({ label, value, hidden }) => ({ label, value, hidden })) });
    }
    expect(MOC_DEAL_PROPERTIES.bpo_10?.options[1]).toMatchObject({ label: '現在使われておりません', value: '使われておりません' });
    expect(MOC_DEAL_PROPERTIES.bpo_14?.options).toHaveLength(45);
  });

  it('keeps unknown stored enum values while rendering real choices', () => {
    const html = renderToStaticMarkup(<MocPropertyInput recordName="テスト" definitionName="bpo_42" value="旧選択肢" onChange={() => undefined} />);
    expect(html).toContain('value="旧選択肢" selected=""');
    expect(html).toContain('高（前向き）');
    expect(togglePropertyValue('未定義値;応募がない', '定着しない', true)).toBe('未定義値;応募がない;定着しない');
    expect(togglePropertyValue('未定義値;応募がない', '応募がない', false)).toBe('未定義値');
  });

  it('isolates Deal drafts and keeps business protected fields read only', () => {
    const record = SAMPLE_RECORDS.find(r => r.objectType === 'contacts');
    if (!record) throw new Error('contact fixture missing');
    const key = propertyDraftKey(record, 'bpo_24');
    expect(key).toBe(`sample-moc-deal-${record.id}:bpo_24`);
    expect(key).not.toBe(propertyDraftKey({ id: 'another-contact' }, 'bpo_24'));
    const html = renderToStaticMarkup(<ContactProperties record={record} values={{ [key]: 'ドライバー', [propertyDraftKey(record, 'bpo_18')]: '参照データ' }} onChange={() => undefined} />);
    expect(html).toContain('value="ドライバー"');
    expect(html).toContain('<output');
    expect(html).toContain('参照データ');
    expect(MOC_DEAL_PROPERTIES.bpo_18?.editable).toBe(false);
    expect(MOC_DEAL_PROPERTIES.bpo_19?.editable).toBe(false);
    expect(MOC_DEAL_PROPERTIES.bpo_32?.editable).toBe(false);
    expect(html).toContain('type="date"');
  });
});
