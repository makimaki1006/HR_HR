import { memo, useEffect, useId, useMemo, useRef, useState } from 'react';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { PropLink } from './CenterTabs';
import {
  CATALOG_OBJECTS, DEFAULT_SELECTED, MAX_SELECTED_PER_OBJECT, OBJECT_LABELS, OBJECT_VALUE_LABELS, orderedSelection, sameSelected, selectedValue,
  toggleSelected, viewValue,
} from './propertyModel';
import type { CatalogEntry, CatalogObject, SelectedProps } from './propertyModel';
import type { CatalogState } from './usePropertyCatalog';
import type { CrmPropertyGroup } from '../../generated/CrmPropertyGroup';
import './property-panel.css';

/**
 * 「プロパティ」パネル: 選んだ項目を HubSpot の表示名で並べる (読み取りだけ)。
 * 「表示する項目を選ぶ」で、HubSpot のグループごとに項目を選ぶ (グループまとめて・1 項目ずつ・名前で探す)。
 * 選んだ項目はこのブラウザに残り、適用すると今の案件を読み直す (同じ読み取りに項目を足すだけ)
 */

function Value({ entry, raw, ownerNames }: { entry: CatalogEntry; raw: string | null | undefined; ownerNames: ReadonlyMap<string, string> }) {
  // 応答に値が無い (項目の一覧を読めず、選んだ項目を読まなかった等)
  if (raw === undefined) return <span className="pp-empty">取得できませんでした</span>;
  const v = viewValue(entry.prop, raw, ownerNames);
  if (v.kind === 'empty') return <span className="pp-empty">未入力</span>;
  if (v.kind === 'links') {
    return <span className="pp-links">{v.urls.map(u => <PropLink key={u} url={u} label={entry.prop.label}>{u}</PropLink>)}</span>;
  }
  return <span className={v.multiline ? 'pp-multiline' : undefined}>{v.text}</span>;
}

function ObjectValues({ obj, entries, data, ownerNames }: {
  obj: CatalogObject; entries: CatalogEntry[]; data: WorkspaceResponse; ownerNames: ReadonlyMap<string, string>;
}) {
  if (entries.length === 0) return null;
  const missing = obj === 'contacts' ? data.contacts.length === 0 : obj === 'companies' ? data.companies.length === 0 : false;
  return <section className="pp-section" aria-label={OBJECT_VALUE_LABELS[obj]}>
    <h4>{OBJECT_VALUE_LABELS[obj]}</h4>
    {missing ? <p className="crm-muted pp-note">{OBJECT_LABELS[obj]}の情報を取得できませんでした(HubSpot に紐づいていない、または取得に失敗)。</p>
      : <dl className="pp-list">{entries.map(e => <div key={e.prop.name}>
        <dt>{e.prop.label}</dt>
        <dd><Value entry={e} raw={selectedValue(data.selected, obj, e.prop.name)} ownerNames={ownerNames} /></dd>
      </div>)}</dl>}
  </section>;
}

/** グループのまとめて選ぶ欄 (一部だけ選んでいるときは途中の表示) */
function GroupCheckbox({ group, obj, draft, onChange }: {
  group: CrmPropertyGroup; obj: CatalogObject; draft: SelectedProps; onChange: (next: SelectedProps) => void;
}) {
  const ref = useRef<HTMLInputElement | null>(null);
  const names = group.properties.map(p => p.name);
  const chosen = names.filter(n => draft[obj].includes(n)).length;
  const all = chosen === names.length;
  const some = chosen > 0 && !all;
  const tooMany = !all && draft[obj].length + (names.length - chosen) > MAX_SELECTED_PER_OBJECT;
  useEffect(() => { if (ref.current) ref.current.indeterminate = some; }, [some]);
  return <label className="pp-group-all" title={tooMany ? `項目が多いため、まとめては選べません(1 つの種類で ${String(MAX_SELECTED_PER_OBJECT)} 項目まで)。1 つずつ選んでください` : undefined}>
    <input ref={ref} type="checkbox" checked={all} disabled={tooMany} aria-label={`「${group.label}」の項目をまとめて選ぶ`}
      onChange={e => { onChange(toggleSelected(draft, obj, names, e.target.checked)); }} />
    {/* 多すぎてまとめて選べないときは、その理由を見える文字で出す */}
    {tooMany ? '多いため 1 つずつ選んでください' : 'まとめて選ぶ'}
  </label>;
}

export function PropertyPicker({ catalog, selection, onApply, onCancel }: {
  catalog: Extract<CatalogState, { phase: 'ready' }>; selection: SelectedProps;
  onApply: (next: SelectedProps) => void; onCancel: () => void;
}) {
  const [draft, setDraft] = useState<SelectedProps>(selection);
  const [obj, setObj] = useState<CatalogObject>('deals');
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  const searchId = useId();
  const q = query.trim().normalize('NFKC').toLowerCase();
  const shown = useMemo(() => (catalog.catalog.objects.find(o => o.object_type === obj)?.groups ?? []).map(g => ({
    group: g, props: q === '' ? g.properties : g.properties.filter(p => p.label.normalize('NFKC').toLowerCase().includes(q)),
  })).filter(x => x.props.length > 0), [catalog.catalog, obj, q]);
  const count = draft[obj].length;
  const atLimit = count >= MAX_SELECTED_PER_OBJECT;
  const changed = !sameSelected(draft, selection);
  function toggleGroup(key: string) {
    setOpen(prev => { const n = new Set(prev); if (n.has(key)) n.delete(key); else n.add(key); return n; });
  }
  return <div className="pp-picker" role="region" aria-label="表示する項目を選ぶ" data-testid="property-picker">
    <div className="pp-objects" role="group" aria-label="項目の種類">
      {CATALOG_OBJECTS.map(o => <button key={o} type="button" aria-pressed={o === obj} onClick={() => { setObj(o); }}>
        {OBJECT_LABELS[o]}<small>({draft[o].length})</small></button>)}
    </div>
    <label className="pp-search" htmlFor={searchId}>項目名で探す</label>
    <input id={searchId} type="search" className="pp-search-input" value={query} placeholder="例: 架電日" onChange={e => { setQuery(e.target.value); }} />
    <p className="pp-count" role="status">{OBJECT_LABELS[obj]}: {String(count)} 項目を選択中(最大 {String(MAX_SELECTED_PER_OBJECT)} 項目)</p>
    {shown.length === 0 && <p className="crm-muted pp-note">{q === '' ? '選べる項目がありません。' : '一致する項目がありません。'}</p>}
    <ul className="pp-groups">
      {shown.map(({ group, props }) => {
        const key = `${obj}:${group.name}`;
        const expanded = q !== '' || open.has(key);
        const chosen = group.properties.filter(p => draft[obj].includes(p.name)).length;
        const listId = `pp-g-${obj}-${group.name}`.replace(/[^A-Za-z0-9_-]/g, '_');
        return <li key={key} className="pp-group">
          <div className="pp-group-head">
            <button type="button" className="pp-group-toggle" aria-expanded={expanded} aria-controls={listId} disabled={q !== ''}
              onClick={() => { toggleGroup(key); }}>
              <span aria-hidden="true">{expanded ? '▾' : '▸'}</span> {group.label}
              <small>({chosen > 0 ? `${String(chosen)} / ` : ''}{String(group.properties.length)})</small></button>
            {q === '' && <GroupCheckbox group={group} obj={obj} draft={draft} onChange={setDraft} />}
          </div>
          {expanded && <ul id={listId} className="pp-props">
            {props.map(p => {
              const checked = draft[obj].includes(p.name);
              return <li key={p.name}><label>
                <input type="checkbox" checked={checked} disabled={!checked && atLimit}
                  onChange={e => { setDraft(d => toggleSelected(d, obj, [p.name], e.target.checked)); }} />
                {p.label}</label></li>;
            })}
          </ul>}
        </li>;
      })}
    </ul>
    <div className="pp-actions">
      <button type="button" className="pp-apply" disabled={!changed} onClick={() => { onApply(draft); }}>この項目で表示する</button>
      <button type="button" onClick={onCancel}>やめる</button>
      <button type="button" className="pp-reset" disabled={sameSelected(draft, DEFAULT_SELECTED)} onClick={() => { setDraft(DEFAULT_SELECTED); }}>既定の項目に戻す</button>
    </div>
  </div>;
}

function PropertyPanelImpl({ catalog, onReloadCatalog, selection, onApply, data, placeholder, ownerNames, hasSelection }: {
  catalog: CatalogState; onReloadCatalog: () => void;
  selection: SelectedProps; onApply: (next: SelectedProps) => void;
  /** 表示中の案件の詳細 (無ければ placeholder を出す) */
  data: WorkspaceResponse | null; placeholder: string;
  ownerNames: ReadonlyMap<string, string>;
  /** 架電先を選んでいるか (選ぶまでは項目の一覧も読まない) */
  hasSelection: boolean;
}) {
  const [picking, setPicking] = useState(false);
  const pickerId = useId();
  const ready = catalog.phase === 'ready' ? catalog : null;
  const entries = ready === null ? null : Object.fromEntries(CATALOG_OBJECTS.map(o => [o, orderedSelection(o, selection, ready.index)])) as Record<CatalogObject, CatalogEntry[]>;
  const nothing = entries !== null && CATALOG_OBJECTS.every(o => entries[o].length === 0);
  return <div className="pp dock-scroll" data-testid="property-panel">
    <div className="pp-head">
      <button type="button" className="pp-pick" aria-expanded={picking} aria-controls={pickerId} disabled={ready === null}
        onClick={() => { setPicking(p => !p); }}>表示する項目を選ぶ</button>
    </div>
    <div id={pickerId}>{picking && ready !== null && <PropertyPicker catalog={ready} selection={selection}
      onApply={next => { setPicking(false); onApply(next); }} onCancel={() => { setPicking(false); }} />}</div>
    {!hasSelection ? <p className="dock-placeholder">{placeholder}</p>
      : catalog.phase === 'loading' ? <p role="status" className="dock-placeholder">項目の一覧を読み込み中…</p>
        : catalog.phase === 'error' ? <div className="cq-notice cq-error" role="alert"><strong>項目を表示できません</strong><p>{catalog.message}</p>
          <button type="button" onClick={onReloadCatalog}>再試行</button></div>
          : data === null ? <p className="dock-placeholder">{placeholder}</p>
            : nothing ? <p className="crm-muted pp-note">表示する項目がありません。「表示する項目を選ぶ」から選んでください。</p>
              : entries !== null && CATALOG_OBJECTS.map(o => <ObjectValues key={o} obj={o} entries={entries[o]} data={data} ownerNames={ownerNames} />)}
  </div>;
}

/** 親 (架電画面) が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const PropertyPanel = memo(PropertyPanelImpl);
