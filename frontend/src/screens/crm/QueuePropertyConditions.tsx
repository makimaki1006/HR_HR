import { useState } from 'react';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { DatePicker } from '../../components/DatePicker';
import type { CatalogState } from './usePropertyCatalog';
import { MAX_PROPERTY_CONDITIONS, operatorsFor } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import { isValidDate } from './queueModel';
import './queue-property-filter.css';
import { QueuePropertyPicker } from './QueuePropertyPicker';

export function QueuePropertyFilter({ catalog, count, remaining = MAX_PROPERTY_CONDITIONS - count, onAdd, onReload }: {
  catalog: CatalogState; count: number; remaining?: number; onAdd: (c: PropertyCondition) => void; onReload: () => void;
}) {
  const [property, setProperty] = useState<CrmCatalogProperty | null>(null);
  const [operator, setOperator] = useState('');
  const [values, setValues] = useState<string[]>([]);
  const props = catalog.phase === 'ready' ? [...catalog.index.deals.values()].map(e => e.prop).filter(p => operatorsFor(p).length > 0) : [];
  const pick = (p: CrmCatalogProperty | null) => { setProperty(p); setOperator(p ? operatorsFor(p)[0]?.value ?? '' : ''); setValues(p?.property_type === 'bool' ? ['true'] : []); };
  const empty = ['HAS_PROPERTY', 'NOT_HAS_PROPERTY', 'TODAY', 'BEFORE_TODAY', 'IS_TRUE', 'IS_FALSE'].includes(operator);
  const relative = ['LAST_N_DAYS', 'NEXT_N_DAYS'].includes(operator);
  const range = operator === 'BETWEEN';
  const value = values[0] ?? '';
  const valid = property !== null && (empty || (relative ? /^\d+$/.test(value) && Number(value) >= 1 && Number(value) <= 3650
    : property.property_type === 'enumeration' ? values.length > 0 && values.length <= 20
    : ['date', 'datetime'].includes(property.property_type) ? isValidDate(value) && (!range || isValidDate(values[1] ?? '') && value <= (values[1] ?? ''))
    : property.property_type === 'number' ? value.trim() !== '' && Number.isFinite(Number(value)) && (!range || (values[1] ?? '').trim() !== '' && Number.isFinite(Number(values[1])) && Number(value) <= Number(values[1]))
    : value.trim() !== '' && Array.from(value).length <= 200));
  const setAt = (i: number, v: string) => { setValues(prev => { const next = [...prev]; next[i] = v; return next; }); };
  return <fieldset className="cq-property-filter"><legend>条件を追加</legend>
    {catalog.phase === 'loading' && <p role="status">項目の一覧を読み込み中…</p>}
    {catalog.phase === 'error' && <p role="alert">{catalog.message} <button type="button" onClick={onReload}>項目を読み直す</button></p>}
    {remaining <= 0 ? <p>このまとまりは 6 条件までです。条件を外すと追加できます。</p> : catalog.phase === 'ready' && <>
      <QueuePropertyPicker properties={props} value={property} onChange={pick} />
      {property && <><label>条件<select aria-label="条件" value={operator} onChange={e => { setOperator(e.target.value); setValues(property.property_type === 'bool' ? ['true'] : []); }}>
        {operatorsFor(property).map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select></label>
      {!empty && (relative ? <label>日数<input aria-label="日数" type="number" min="1" max="3650" step="1" value={value} onChange={e => { setValues([e.target.value]); }} /><small>今日を含む日数</small></label> : property.property_type === 'enumeration' ? <fieldset className="cq-property-options"><legend>値(複数選択)</legend>
        {property.options.map(o => <label key={o.value}><input type="checkbox" checked={values.includes(o.value)}
          onChange={e => { setValues(prev => e.target.checked ? [...prev, o.value] : prev.filter(v => v !== o.value)); }} />{o.label}</label>)}
        {property.options.length === 0 && <p>この項目に選べる値はありません。</p>}</fieldset>
        : property.property_type === 'bool' ? <label>値<select aria-label="値" value={value} onChange={e => { setValues([e.target.value]); }}><option value="true">はい</option><option value="false">いいえ</option></select></label>
        : ['date', 'datetime'].includes(property.property_type) ? <><label>{range ? '開始日' : '値'}<DatePicker aria-label={range ? '開始日' : '値'} value={value} onChange={v => { setAt(0, v); }} /></label>{range && <label>終了日<DatePicker aria-label="終了日" value={values[1] ?? ''} onChange={v => { setAt(1, v); }} /></label>}</>
        : <label>{range ? '下限' : '値'}<input aria-label={range ? '下限' : '値'} type={property.property_type === 'number' ? 'number' : 'text'} value={value} maxLength={200} step="any"
          onChange={e => { setAt(0, e.target.value); }} /></label>)}
      {!empty && range && property.property_type === 'number' && <label>上限<input aria-label="上限" type="number" step="any" value={values[1] ?? ''} onChange={e => { setAt(1, e.target.value); }} /></label>}
      <button type="button" className="cq-btn" disabled={!valid || remaining <= 0} onClick={() => { onAdd({ property: property.name, operator, values: empty ? [] : values }); pick(null); }}>この条件を追加</button></>}
      <small>このまとまりのすべての条件に合う架電先を表示します。最大 6 条件。</small>
    </>}
  </fieldset>;
}
