import { useState } from 'react';
import type { CatalogState } from './usePropertyCatalog';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { conditionLabel, MAX_CONDITION_GROUPS, parsePropertySort, sortable } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import type { QueueSort } from './queueModel';

export function QueueGroupEditor({ catalog, groups, onChange, onReload }: {
  catalog: CatalogState; groups: PropertyCondition[][]; onChange: (gs: PropertyCondition[][]) => void; onReload: () => void;
}) {
  const [target, setTarget] = useState(0);
  const [draft, setDraft] = useState(false);
  const at = draft ? groups.length : Math.min(target, Math.max(0, groups.length - 1));
  return <div className="cq-group-editor">
    <div className="cq-group-toolbar"><label>追加先<select aria-label="追加先" value={at} onChange={e => { const n = Number(e.target.value); setTarget(n); setDraft(n === groups.length && groups.length > 0); }}>
      {Array.from({ length: Math.max(groups.length, 1) }, (_, i) => <option key={i} value={i}>まとまり {i + 1}</option>)}
      {groups.length > 0 && draft && <option value={groups.length}>新しいまとまり</option>}
    </select></label>
    <button type="button" className="cq-btn" disabled={groups.length === 0 || groups.length >= MAX_CONDITION_GROUPS}
      onClick={() => { setTarget(groups.length); setDraft(true); }}>またはのまとまりを追加</button>
    <small>まとまりの中はすべての条件、まとまり同士はいずれかに合う架電先を表示します。</small></div>
    <QueuePropertyFilter key={at} catalog={catalog} count={groups[at]?.length ?? 0} onReload={onReload} onAdd={c => {
      const next = groups.map(g => [...g]);
      if (next[at]) next[at].push(c); else next.push([c]);
      onChange(next); setTarget(at); setDraft(false);
    }} />
  </div>;
}
export function QueueGroupSummary({ catalog, groups, onChange }: {
  catalog: CatalogState; groups: PropertyCondition[][]; onChange: (gs: PropertyCondition[][]) => void;
}) {
  if (groups.length < 2) return null;
  return <div className="cq-condition-groups" aria-label="条件のまとまり">{groups.map((cs, i) => <div key={i}>
    {i > 0 && <p className="cq-or-label">または</p>}
    <section className="cq-condition-group" aria-label={`まとまり ${String(i + 1)}`}><strong>まとまり {i + 1}</strong>
      <button type="button" className="cq-btn cq-btn-quiet" onClick={() => { onChange(groups.filter((_, n) => n !== i)); }}>このまとまりを外す</button>
      <ul className="cq-chips">{cs.map((c, j) => { const label = conditionLabel(c, catalog.phase === 'ready' ? catalog.index.deals.get(c.property)?.prop : undefined);
        return <li key={j} className="cq-chip"><span>{label}</span><button type="button" aria-label={`まとまり ${String(i + 1)} の「${label}」を外す`}
          onClick={() => { onChange(groups.map((g, n) => n === i ? g.filter((_, k) => k !== j) : g).filter(g => g.length > 0)); }}>×</button></li>; })}</ul>
    </section></div>)}</div>;
}
export function PropertySortPicker({ catalog, sort, onChange }: { catalog: CatalogState; sort: QueueSort; onChange: (s: QueueSort) => void }) {
  const [query, setQuery] = useState('');
  const chosen = parsePropertySort(sort);
  const [property, setProperty] = useState(chosen?.property ?? '');
  const [direction, setDirection] = useState<'asc' | 'desc'>(chosen?.direction ?? 'asc');
  if (catalog.phase !== 'ready') return null;
  const ps = [...catalog.index.deals.values()].map(e => e.prop).filter(sortable);
  return <fieldset className="cq-property-filter"><legend>項目で並べ替え</legend>
    <label>並べ替える項目を検索<input aria-label="並べ替える項目を検索" type="search" value={query} onChange={e => { setQuery(e.target.value); }} /></label>
    <label>並べ替える項目<select aria-label="並べ替える項目" value={property} onChange={e => { setProperty(e.target.value); }}><option value="">項目を選択</option>
      {ps.filter(p => p.label.toLowerCase().includes(query.toLowerCase())).map(p => <option key={p.name} value={p.name}>{p.label}</option>)}</select></label>
    <label>順序<select aria-label="順序" value={direction} onChange={e => { setDirection(e.target.value === 'desc' ? 'desc' : 'asc'); }}><option value="asc">昇順</option><option value="desc">降順</option></select></label>
    <button type="button" className="cq-btn" disabled={!ps.some(p => p.name === property)} onClick={() => { onChange(`property:${property}:${direction}`); }}>この順に並べ替え</button>
  </fieldset>;
}
