import { useState } from 'react';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { DatePicker } from '../../components/DatePicker';
import type { CatalogState } from './usePropertyCatalog';
import { MAX_PROPERTY_CONDITIONS, operatorsFor } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import { isValidDate } from './queueModel';
import './queue-property-filter.css';

export function QueuePropertyFilter({ catalog, count, onAdd, onReload }: {
  catalog: CatalogState; count: number; onAdd: (c: PropertyCondition) => void; onReload: () => void;
}) {
  const [query, setQuery] = useState('');
  const [property, setProperty] = useState<CrmCatalogProperty | null>(null);
  const [operator, setOperator] = useState('');
  const [values, setValues] = useState<string[]>([]);
  const props = catalog.phase === 'ready' ? [...catalog.index.deals.values()].map(e => e.prop).filter(p => operatorsFor(p).length > 0) : [];
  const options = props.filter(p => p.label.toLowerCase().includes(query.toLowerCase()));
  const pick = (p: CrmCatalogProperty | null) => { setProperty(p); setOperator(p ? operatorsFor(p)[0]?.value ?? '' : ''); setValues(p?.property_type === 'bool' ? ['true'] : []); };
  const empty = ['HAS_PROPERTY', 'NOT_HAS_PROPERTY'].includes(operator);
  const value = values[0] ?? '';
  const valid = property !== null && (empty || (property.property_type === 'enumeration' ? values.length > 0 && values.length <= 20
    : ['date', 'datetime'].includes(property.property_type) ? isValidDate(value)
    : property.property_type === 'number' ? value.trim() !== '' && Number.isFinite(Number(value))
    : value.trim() !== '' && Array.from(value).length <= 200));
  return <fieldset className="cq-property-filter"><legend>条件を追加</legend>
    {catalog.phase === 'loading' && <p role="status">項目の一覧を読み込み中…</p>}
    {catalog.phase === 'error' && <p role="alert">{catalog.message} <button type="button" onClick={onReload}>項目を読み直す</button></p>}
    {count >= MAX_PROPERTY_CONDITIONS ? <p>追加条件は 3 つまでです。条件を外すと追加できます。</p> : catalog.phase === 'ready' && <>
      <label>項目を検索<input type="search" value={query} onChange={e => { setQuery(e.target.value); }} placeholder="取引の項目名" /></label>
      <label>項目<select value={property?.name ?? ''} onChange={e => { pick(props.find(p => p.name === e.target.value) ?? null); }}>
        <option value="">項目を選択</option>{options.map(p => <option key={p.name} value={p.name}>{p.label}</option>)}</select></label>
      {property && <><label>条件<select value={operator} onChange={e => { setOperator(e.target.value); setValues(property.property_type === 'bool' ? ['true'] : []); }}>
        {operatorsFor(property).map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select></label>
      {!empty && (property.property_type === 'enumeration' ? <fieldset className="cq-property-options"><legend>値(複数選択)</legend>
        {property.options.map(o => <label key={o.value}><input type="checkbox" checked={values.includes(o.value)}
          onChange={e => { setValues(prev => e.target.checked ? [...prev, o.value] : prev.filter(v => v !== o.value)); }} />{o.label}</label>)}
        {property.options.length === 0 && <p>この項目に選べる値はありません。</p>}</fieldset>
        : property.property_type === 'bool' ? <label>値<select value={value} onChange={e => { setValues([e.target.value]); }}><option value="true">はい</option><option value="false">いいえ</option></select></label>
        : ['date', 'datetime'].includes(property.property_type) ? <label>値<DatePicker value={value} onChange={v => { setValues([v]); }} /></label>
        : <label>値<input type={property.property_type === 'number' ? 'number' : 'text'} value={value} maxLength={200} step="any"
          onChange={e => { setValues([e.target.value]); }} /></label>)}
      <button type="button" className="cq-btn" disabled={!valid} onClick={() => { onAdd({ property: property.name, operator, values: empty ? [] : values }); pick(null); setQuery(''); }}>この条件を追加</button></>}
      <small>最大 3 つ。追加したすべての条件に合う架電先を表示します。</small>
    </>}
  </fieldset>;
}
