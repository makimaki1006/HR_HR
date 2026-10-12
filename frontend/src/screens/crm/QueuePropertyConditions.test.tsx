// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';
import { DEFAULT_FILTERS, filtersKey, parseFilters, queueApiPath, screenSearch } from './queueModel';
import { conditionLabel, operatorsFor, parseConditions } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import { fixtureQueuePage } from './queueFixture';

afterEach(() => { cleanup(); });
const state = { phase: 'ready' as const, catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) };
function Harness() {
  const [conditions, setConditions] = useState<PropertyCondition[]>([]);
  return <><QueuePropertyFilter catalog={state} count={conditions.length} onAdd={c => { setConditions(prev => [...prev, c]); }} onReload={() => undefined} />
    <output>{JSON.stringify(conditions)}</output></>;
}
describe('取引の追加条件', () => {
  it('URL・API・キャッシュの比較キーに具体値を残す', () => {
    const conditions = [{ property: 'amount', operator: 'GTE', values: ['8000'] }, { property: 'bpo_20', operator: 'NOT_HAS_PROPERTY', values: [] }];
    const f = { ...DEFAULT_FILTERS, conditions };
    expect(parseFilters(screenSearch(f, 'fixture'))).toEqual(f);
    expect(new URL(queueApiPath(f, null), 'https://local.invalid').searchParams.get('conditions')).toBe(JSON.stringify(conditions));
    expect(filtersKey(f)).not.toBe(filtersKey({ ...f, conditions: [{ property:'amount', operator:'GTE', values: ['9000'] }] }));
    expect(fixtureQueuePage(DEFAULT_FILTERS, null).total).toBe(10);
    expect(fixtureQueuePage({ ...f, conditions: conditions.slice(0,1) }, null).total).toBe(3);
    expect(fixtureQueuePage(f, null).items.map(i => i.deal_id)).toEqual(['f-10']);
  });
  it('検索して数値条件を追加する。表示に演算子の内部名を出さない', () => {
    render(<Harness />);
    fireEvent.change(screen.getByLabelText('項目を検索'), { target: { value: '金額' } });
    expect(screen.getAllByRole('option')).toHaveLength(1);
    fireEvent.click(screen.getByRole('option',{name:'金額'}));
    fireEvent.change(screen.getByLabelText('条件'), { target: { value: 'GTE' } });
    fireEvent.change(screen.getByLabelText('値'), { target: { value: '8000' } });
    fireEvent.click(screen.getByRole('button', { name: 'この条件を追加' }));
    expect(screen.getByRole('status').textContent).toBe('[{"property":"amount","operator":"GTE","values":["8000"]}]');
    expect(conditionLabel({ property:'amount',operator:'GTE',values:['8000'] },state.index.deals.get('amount')?.prop)).toBe('金額 以上 8,000');
  });
  it('型ごとの演算子と真偽・日付・複数選択を用意する', () => {
    const base = state.index.deals.get('amount')?.prop;
    if (!base) throw new Error('金額の定義が必要');
    for (const [kind, expected] of Object.entries({ enumeration:['IN','NOT_IN','NOT_HAS_PROPERTY','HAS_PROPERTY'], string:['CONTAINS_TOKEN','NOT_CONTAINS_TOKEN','EQ','NEQ','NOT_HAS_PROPERTY','HAS_PROPERTY'], number:['EQ','NEQ','GTE','LTE','BETWEEN','NOT_HAS_PROPERTY','HAS_PROPERTY'], date:['GTE','LTE','BETWEEN','TODAY','BEFORE_TODAY','LAST_N_DAYS','NEXT_N_DAYS','NOT_HAS_PROPERTY','HAS_PROPERTY'], bool:['IS_TRUE','IS_FALSE','NOT_HAS_PROPERTY'] })) {
      expect(operatorsFor({ ...base, property_type:kind }).map(o => o.value)).toEqual(expected);
    }
    expect(operatorsFor({ ...base,property_type:'date' }).map(o => o.label)).toEqual(['以降','以前','範囲','今日','今日より前','過去 N 日以内','今後 N 日以内','空','空でない']);
    expect(conditionLabel({property:'amount',operator:'EQ',values:['false']},{...base,property_type:'bool'})).toBe('金額: いいえ');
  });
  it('6 件で追加を止め、壊れた URL と長すぎる値を復元しない', () => {
    const c = {property:'amount',operator:'EQ',values:['1']};
    render(<QueuePropertyFilter catalog={state} count={6} onAdd={() => { throw new Error('追加不可'); }} onReload={() => undefined} />);
    expect(screen.getByText('このまとまりは 6 条件までです。条件を外すと追加できます。')).not.toBeNull();
    expect(parseConditions(JSON.stringify([c,c,c,c,c,c,c]))).toEqual([]);
    expect(parseConditions(JSON.stringify([{...c,values:['a'.repeat(201)]}]))).toEqual([]);
    expect(parseConditions('{broken')).toEqual([]);
  });
});
