import { useEffect, useRef, useState } from 'react';
import type { CatalogState } from './usePropertyCatalog';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { QueuePropertyPicker } from './QueuePropertyPicker';
import { conditionLabel, MAX_CONDITION_GROUPS, parsePropertySort, sortable } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
import type { QueueFilters, QueueSort } from './queueModel';
import { remainingConditions } from './queueConditionBudget';
import { NO_VALUE_CHOICES, valueLabel } from './queueValueChoices';
import type { ValueChoiceSource } from './queueValueChoices';

export function QueueGroupEditor({ catalog, groups, filters, choices = NO_VALUE_CHOICES, onChange, onReload, focusAdd = false, onFocusAddDone }: {
  catalog: CatalogState; groups: PropertyCondition[][]; filters: QueueFilters; choices?: ValueChoiceSource; onChange: (gs: PropertyCondition[][]) => void; onReload: () => void;
  /** バーの「条件を追加」から来た: 最後のまとまりの項目検索欄を開いてフォーカスする */
  focusAdd?: boolean; onFocusAddDone?: () => void;
}) {
  const [editing, setEditing] = useState<number | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const lastIdx = Math.max(0, groups.length - 1);
  const canAdd = remainingConditions(filters, groups, lastIdx) > 0;
  // 「条件を追加」から来たときは、最後のまとまりの検索欄を開く (上限に達していれば何も開かない)
  if (focusAdd && canAdd && editing !== lastIdx) setEditing(lastIdx);
  useEffect(() => { if (focusAdd && !canAdd) onFocusAddDone?.(); }, [focusAdd, canAdd, onFocusAddDone]);
  // 項目の検索欄ができたら、フォーカスを置く
  useEffect(() => {
    if (!focusAdd || editing === null) return;
    const input = rootRef.current?.querySelector<HTMLInputElement>('.cq-property-filter input[role="combobox"]');
    if (input) { input.focus(); onFocusAddDone?.(); }
  });
  const [draft, setDraft] = useState(false);
  const shown = groups.length ? [...groups, ...(draft ? [[]] : [])] : [[]];
  return <div className="cq-group-editor" ref={rootRef}><small>まとまりの中はすべての条件、まとまり同士はいずれかに合う架電先を表示します。</small>
    {shown.map((cs, i) => { const remaining = remainingConditions(filters, groups, i); return <div key={i}>
      {i > 0 && <p className="cq-or-label">または</p>}
      <section className="cq-condition-group" aria-label={`まとまり ${String(i + 1)}`}><div className="cq-group-heading"><strong>まとまり {i + 1}</strong>
        {groups[i] && <button type="button" className="cq-btn cq-btn-quiet" onClick={() => { onChange(groups.filter((_, n) => n !== i)); setEditing(null); setDraft(false); }}>このまとまりを外す</button>}</div>
        <ul className="cq-chips">{cs.map((c, j) => { const prop = catalog.phase === 'ready' ? catalog.index.deals.get(c.property)?.prop : undefined; const label = conditionLabel(c, prop, v => (prop ? valueLabel(prop, v, choices) : undefined));
          return <li key={j} className="cq-chip"><span>{label}</span><button type="button" aria-label={`まとまり ${String(i + 1)} の「${label}」を外す`}
            onClick={() => { onChange(groups.map((g, n) => n === i ? g.filter((_, k) => k !== j) : g).filter(g => g.length > 0)); setEditing(null); setDraft(false); }}>×</button></li>; })}</ul>
        <div className="cq-group-actions"><button type="button" className="cq-btn" disabled={remaining === 0} onClick={() => { setEditing(i); }}>＋ 条件を追加</button><small>条件はあと {remaining} 個です</small></div>
        {editing === i && remaining > 0 && <QueuePropertyFilter key={i} catalog={catalog} count={cs.length} choices={choices} remaining={remaining} onReload={onReload} onAdd={c => {
          const next = groups.map(g => [...g]); if (next[i]) next[i].push(c); else next.push([c]);
          onChange(next); setEditing(null); setDraft(false);
        }} />}
      </section></div>; })}
    <button type="button" className="cq-btn cq-add-or" disabled={draft || groups.length === 0 || groups.length >= MAX_CONDITION_GROUPS || remainingConditions(filters, groups, groups.length) === 0}
      onClick={() => { setDraft(true); setEditing(groups.length); }}>＋ 「または」の条件を追加</button>
  </div>;
}
export function PropertySortPicker({ catalog, sort, onChange }: { catalog: CatalogState; sort: QueueSort; onChange: (s: QueueSort) => void }) {
  const chosen = parsePropertySort(sort);
  const [property, setProperty] = useState(chosen?.property ?? '');
  const [direction, setDirection] = useState<'asc' | 'desc'>(chosen?.direction ?? 'asc');
  if (catalog.phase !== 'ready') return <p role="status">項目の一覧を読み込み中…</p>;
  const ps = [...catalog.index.deals.values()].map(e => e.prop).filter(sortable);
  return <div className="cq-sort-picker">
    <QueuePropertyPicker properties={ps} value={ps.find(p => p.name === property) ?? null} label="並べ替える項目を検索" onChange={p => { setProperty(p?.name ?? ''); }} />
    <label>順序<select aria-label="順序" value={direction} onChange={e => { setDirection(e.target.value === 'desc' ? 'desc' : 'asc'); }}><option value="asc">昇順</option><option value="desc">降順</option></select></label>
    <button type="button" className="cq-btn" disabled={!ps.some(p => p.name === property)} onClick={() => { onChange(`property:${property}:${direction}`); }}>この順に並べ替え</button>
  </div>;
}
