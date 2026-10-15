// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { QueueGroupEditor } from './QueueConditionGroups';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';
import { DEFAULT_FILTERS } from './queueModel';
import { FIXTURE_PIPELINES, FIXTURE_PIPELINE_ID, DEFAULT_PIPELINE_ID, withLabels } from './queuePipelines';
import { choicesFor, usableAsCondition, valueLabel } from './queueValueChoices';
import type { ValueChoiceSource } from './queueValueChoices';
import type { PropertyCondition } from './queuePropertyFilter';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';

afterEach(() => { cleanup(); });
const state = { phase: 'ready' as const, catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) };
const prop = (name: string): CrmCatalogProperty => {
  const p = state.index.deals.get(name)?.prop;
  if (!p) throw new Error(`${name} が架空の項目一覧に必要`);
  return p;
};
const owners = { phase: 'ready' as const, owners: [{ id: '9001', name: '架空 一郎', email: 'ichiro@example.invalid', archived: false, teams: [] }], truncated: false, meId: null };
const source = (ids: string[], allDeals = false): ValueChoiceSource => ({ pipelines: FIXTURE_PIPELINES, selectedPipelineIds: ids, allDeals, owners });

function Harness({ src }: { src: ValueChoiceSource }) {
  const [conditions, setConditions] = useState<PropertyCondition[]>([]);
  return <><QueuePropertyFilter catalog={state} count={conditions.length} choices={src} onAdd={c => { setConditions(prev => [...prev, c]); }} onReload={() => undefined} />
    <output>{JSON.stringify(conditions)}</output></>;
}

describe('取引ステージの条件', () => {
  it('選べる値は選んだパイプラインのステージで、パイプライン名が付く', () => {
    const one = choicesFor(prop('dealstage'), source([FIXTURE_PIPELINE_ID]));
    expect(one.status).toBe('ready');
    expect(one.choices).toEqual([
      { value: 'fx-new', label: '架空パイプライン(確認用) / 新規' },
      { value: 'fx-follow', label: '架空パイプライン(確認用) / 追客' },
      { value: 'fx-stop', label: '架空パイプライン(確認用) / 対象外' },
    ]);
    const both = choicesFor(prop('dealstage'), source([DEFAULT_PIPELINE_ID, FIXTURE_PIPELINE_ID])).choices;
    expect(both).toHaveLength(19 + 3);
    expect(both[0]).toEqual({ value: '1095387442', label: 'bpo_リクロジ / 未済' });
    expect(choicesFor(prop('dealstage'), source([])).choices).toEqual([]);
  });
  it('表に無いステージは「すべての取引」のときだけ選べる', () => {
    const defs = withLabels(FIXTURE_PIPELINES, {
      default_pipeline: DEFAULT_PIPELINE_ID, labels_available: true,
      pipelines: [{ id: FIXTURE_PIPELINE_ID, label: '架空パイプライン(確認用)', stages: [], unknown_stages: [{ id: 'fx-new-added', label: '追加ステージ' }] }],
    } as never);
    const src = { ...source([FIXTURE_PIPELINE_ID]), pipelines: defs };
    expect(choicesFor(prop('dealstage'), src).choices.map(c => c.value)).not.toContain('fx-new-added');
    expect(choicesFor(prop('dealstage'), { ...src, allDeals: true }).choices.map(c => c.value)).toContain('fx-new-added');
  });
  it('項目に取引ステージを選ぶと、値の一覧にステージが出て、条件に追加できる', () => {
    render(<Harness src={source([FIXTURE_PIPELINE_ID])} />);
    fireEvent.change(screen.getByLabelText('項目を検索'), { target: { value: '取引ステージ' } });
    fireEvent.click(screen.getByRole('option', { name: '取引ステージ' }));
    expect(screen.queryByText('この項目に選べる値はありません。')).toBeNull();
    fireEvent.click(screen.getByLabelText('架空パイプライン(確認用) / 追客'));
    fireEvent.click(screen.getByRole('button', { name: 'この条件を追加' }));
    expect(screen.getByRole('status').textContent).toBe('[{"property":"dealstage","operator":"IN","values":["fx-follow"]}]');
  });
  it('「どれにも一致しない」と「空」も選べる', () => {
    render(<Harness src={source([FIXTURE_PIPELINE_ID])} />);
    fireEvent.change(screen.getByLabelText('項目を検索'), { target: { value: '取引ステージ' } });
    fireEvent.click(screen.getByRole('option', { name: '取引ステージ' }));
    const ops = [...screen.getByLabelText('条件').querySelectorAll('option')].map(o => o.textContent);
    expect(ops).toEqual(['いずれかに一致', 'いずれにも一致しない', '空', '空でない']);
    fireEvent.change(screen.getByLabelText('条件'), { target: { value: 'NOT_IN' } });
    fireEvent.click(screen.getByLabelText('架空パイプライン(確認用) / 対象外'));
    fireEvent.click(screen.getByRole('button', { name: 'この条件を追加' }));
    expect(screen.getByRole('status').textContent).toBe('[{"property":"dealstage","operator":"NOT_IN","values":["fx-stop"]}]');
  });
  it('条件のチップにはステージ名を出し、ID を出さない', () => {
    const groups: PropertyCondition[][] = [[{ property: 'dealstage', operator: 'IN', values: ['fx-follow'] }]];
    render(<QueueGroupEditor catalog={state} groups={groups} filters={DEFAULT_FILTERS} choices={source([FIXTURE_PIPELINE_ID])} onChange={() => undefined} onReload={() => undefined} />);
    expect(screen.getByText('取引ステージ いずれかに一致 架空パイプライン(確認用) / 追客')).not.toBeNull();
    expect(valueLabel(prop('dealstage'), 'fx-follow', source([]))).toBe('架空パイプライン(確認用) / 追客');
  });
});

describe('定義の外に選択肢がある他の項目', () => {
  it('担当者を選ぶ項目は担当者の一覧から選べる', () => {
    const r = choicesFor(prop('hubspot_owner_id'), source([]));
    expect(r.choices).toEqual([{ value: '9001', label: '架空 一郎(ichiro@example.invalid)' }]);
    expect(choicesFor(prop('hubspot_owner_id'), { ...source([]), owners: { phase: 'loading' } }).status).toBe('loading');
    expect(choicesFor(prop('hubspot_owner_id'), { ...source([]), owners: { phase: 'error', message: 'x' } }).status).toBe('error');
  });
  it('パイプラインは条件の項目に出さない。選べる値を作れない選択式も出さない', () => {
    const pipeline: CrmCatalogProperty = { name: 'pipeline', label: 'パイプライン', property_type: 'enumeration', field_type: 'select', options: [], owner_ref: false };
    const dead: CrmCatalogProperty = { ...pipeline, name: 'other_ref', label: '外部参照' };
    const withOptions: CrmCatalogProperty = { ...dead, name: 'ok', options: [{ value: 'a', label: 'A', hidden: false }] };
    expect(usableAsCondition(pipeline)).toBe(false);
    expect(usableAsCondition(dead)).toBe(false);
    expect(usableAsCondition(withOptions)).toBe(true);
    expect(usableAsCondition(prop('dealstage'))).toBe(true);
    expect(usableAsCondition(prop('hubspot_owner_id'))).toBe(true);
  });
});
