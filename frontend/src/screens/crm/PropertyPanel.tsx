import { memo, useEffect, useId, useMemo, useRef, useState } from 'react';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { PropLink } from './CenterTabs';
import { HUBSPOT_CARDS } from './hubspotCards';
import {
  CATALOG_OBJECTS, DEFAULT_SELECTED, MAX_SELECTED_PER_OBJECT, OBJECT_LABELS, OBJECT_VALUE_LABELS, cardAvailableNames, cardSections, orderedSelection,
  sameSelected, selectedValue, toggleSelected, viewValue,
} from './propertyModel';
import type { CardSection, CatalogEntry, CatalogObject, SelectedProps } from './propertyModel';
import type { CatalogState } from './usePropertyCatalog';
import type { CrmPropertyGroup } from '../../generated/CrmPropertyGroup';
import { EditableValue } from './WriteWidgets';
import { WRITES_OFF_NOTE } from './useCrmWrite';
import type { PanelWrite } from './writeBindings';
import './property-panel.css';

/**
 * 「プロパティ」パネル: 選んだ項目を HubSpot の表示名で並べる (読み取りだけ)。
 * 案件の項目は、HubSpot の取引レコードの左サイドバーと同じカード (「リスト情報」は開いて、「BPOアポ情報」は閉じて) に分けて出す
 * (hubspotCards.ts)。どのカードにも無い項目は「案件(そのほかの項目)」に出す。
 * 「表示する項目を選ぶ」で、HubSpot のカードまとめて・グループまとめて・1 項目ずつ・名前で探して選ぶ。
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

function ValueList({ obj, entries, data, ownerNames, id, write }: {
  obj: CatalogObject; entries: CatalogEntry[]; data: WorkspaceResponse; ownerNames: ReadonlyMap<string, string>; id?: string | undefined;
  write?: PanelWrite | null | undefined;
}) {
  return <dl className="pp-list" id={id}>{entries.map(e => {
    const raw = selectedValue(data.selected, obj, e.prop.name);
    const def = write?.def(obj, e) ?? null;
    const st = write?.status(obj, e.prop.name);
    const failedReq = st?.phase === 'error' ? st.req : null;
    return <div key={e.prop.name}>
      <dt>{e.prop.label}</dt>
      <dd>{def !== null && write ? <EditableValue def={def} raw={raw} display={<Value entry={e} raw={raw} ownerNames={ownerNames} />}
        status={write.status(obj, e.prop.name)}
        onSave={(base, value) => { write.save(def, base, value); }}
        onRetry={failedReq !== null ? (() => { write.retry(failedReq); }) : undefined} />
        : <Value entry={e} raw={raw} ownerNames={ownerNames} />}</dd>
    </div>;
  })}</dl>;
}

/** HubSpot のカード 1 枚 (見出しのボタンで開け閉め) */
function CardValues({ section, data, ownerNames, open, onToggle, write }: {
  section: CardSection; data: WorkspaceResponse; ownerNames: ReadonlyMap<string, string>; open: boolean; onToggle: () => void; write?: PanelWrite | null | undefined;
}) {
  const listId = `pp-card-${section.card.id}`;
  const { entries, unavailable } = section;
  return <section className="pp-section pp-card" aria-label={section.card.title} data-testid={`pp-card-${section.card.id}`}>
    <h4 className="pp-card-head"><button type="button" className="pp-card-toggle" aria-expanded={open} aria-controls={listId} onClick={onToggle}>
      <span aria-hidden="true">{open ? '▾' : '▸'}</span> {section.card.title}<small>({String(entries.length)})</small></button></h4>
    {/* 閉じている間も枠は置いておく (aria-controls の先が消えないように)。中身は開いたときだけ描く */}
    <div id={listId} hidden={!open}>{open && <>
      {entries.length > 0 && <ValueList obj="deals" entries={entries} data={data} ownerNames={ownerNames} write={write} />}
      {unavailable > 0 && <p className="crm-muted pp-note">ほかに {String(unavailable)} 項目は HubSpot の項目の一覧に無いため表示できません(非表示・削除・機微情報の項目)。</p>}
    </>}</div>
  </section>;
}

function ObjectValues({ obj, entries, data, ownerNames, title, write }: {
  obj: CatalogObject; entries: CatalogEntry[]; data: WorkspaceResponse; ownerNames: ReadonlyMap<string, string>; title?: string | undefined; write?: PanelWrite | null | undefined;
}) {
  if (entries.length === 0) return null;
  const missing = obj === 'contacts' ? data.contacts.length === 0 : obj === 'companies' ? data.companies.length === 0 : false;
  const heading = title ?? OBJECT_VALUE_LABELS[obj];
  return <section className="pp-section" aria-label={heading}>
    <h4>{heading}</h4>
    {missing ? <p className="crm-muted pp-note">{OBJECT_LABELS[obj]}の情報を取得できませんでした(HubSpot に紐づいていない、または取得に失敗)。</p>
      : <ValueList obj={obj} entries={entries} data={data} ownerNames={ownerNames} write={write} />}
  </section>;
}

/** 項目をまとめて選ぶ欄 (一部だけ選んでいるときは途中の表示)。HubSpot のグループ・カードで使う */
function BulkCheckbox({ names, obj, draft, onChange, name, children }: {
  names: readonly string[]; obj: CatalogObject; draft: SelectedProps; onChange: (next: SelectedProps) => void;
  /** 読み上げ用の名前 */
  name: string; children: React.ReactNode;
}) {
  const ref = useRef<HTMLInputElement | null>(null);
  const chosen = names.filter(n => draft[obj].includes(n)).length;
  const all = names.length > 0 && chosen === names.length;
  const some = chosen > 0 && !all;
  const tooMany = !all && draft[obj].length + (names.length - chosen) > MAX_SELECTED_PER_OBJECT;
  useEffect(() => { if (ref.current) ref.current.indeterminate = some; }, [some]);
  return <label className="pp-group-all" title={tooMany ? `項目が多いため、まとめては選べません(1 つの種類で ${String(MAX_SELECTED_PER_OBJECT)} 項目まで)。1 つずつ選んでください` : undefined}>
    <input ref={ref} type="checkbox" checked={all} disabled={tooMany || names.length === 0} aria-label={name}
      onChange={e => { onChange(toggleSelected(draft, obj, names, e.target.checked)); }} />
    {/* 多すぎてまとめて選べないときは、その理由を見える文字で出す */}
    {tooMany ? '多いため 1 つずつ選んでください' : children}
  </label>;
}

/** グループのまとめて選ぶ欄 */
function GroupCheckbox({ group, obj, draft, onChange }: {
  group: CrmPropertyGroup; obj: CatalogObject; draft: SelectedProps; onChange: (next: SelectedProps) => void;
}) {
  return <BulkCheckbox names={group.properties.map(p => p.name)} obj={obj} draft={draft} onChange={onChange}
    name={`「${group.label}」の項目をまとめて選ぶ`}>まとめて選ぶ</BulkCheckbox>;
}

/** 「HubSpotのカードから選ぶ」: HubSpot の取引レコードの左サイドバーのカードの項目をまとめて選ぶ */
function CardPresets({ index, draft, onChange }: {
  index: Extract<CatalogState, { phase: 'ready' }>['index']; draft: SelectedProps; onChange: (next: SelectedProps) => void;
}) {
  return <fieldset className="pp-cards" data-testid="pp-card-presets">
    <legend>HubSpotのカードから選ぶ</legend>
    {HUBSPOT_CARDS.map(card => {
      const names = cardAvailableNames(card, index);
      return <BulkCheckbox key={card.id} names={names} obj="deals" draft={draft} onChange={onChange}
        name={`HubSpot のカード「${card.title}」の項目をまとめて選ぶ`}>
        {card.title}<small>({String(names.length)} 項目)</small></BulkCheckbox>;
    })}
  </fieldset>;
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
    <CardPresets index={catalog.index} draft={draft} onChange={setDraft} />
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
      <button type="button" className="pp-reset" disabled={sameSelected(draft, DEFAULT_SELECTED)} title="HubSpot のカード「リスト情報」「BPOアポ情報」の項目に戻します(適用するまで表示は変わりません)"
        onClick={() => { setDraft(DEFAULT_SELECTED); }}>既定に戻す</button>
    </div>
  </div>;
}

function PropertyPanelImpl({ catalog, onReloadCatalog, selection, onApply, data, placeholder, ownerNames, hasSelection, write }: {
  catalog: CatalogState; onReloadCatalog: () => void;
  selection: SelectedProps; onApply: (next: SelectedProps) => void;
  /** 表示中の案件の詳細 (無ければ placeholder を出す) */
  data: WorkspaceResponse | null; placeholder: string;
  ownerNames: ReadonlyMap<string, string>;
  /** 架電先を選んでいるか (選ぶまでは項目の一覧も読まない) */
  hasSelection: boolean;
  /** 項目の書き換え (無ければ読み取りだけ) */
  write?: PanelWrite | null | undefined;
}) {
  const [picking, setPicking] = useState(false);
  const pickerId = useId();
  // カードの開け閉め (最初は HubSpot と同じく「リスト情報」だけ開く)
  const [openCards, setOpenCards] = useState<Readonly<Record<string, boolean>>>(() => Object.fromEntries(HUBSPOT_CARDS.map(c => [c.id, c.expanded])));
  const ready = catalog.phase === 'ready' ? catalog : null;
  const cards = useMemo(() => (ready === null ? null : cardSections(selection, ready.index)), [ready, selection]);
  const entries = ready === null ? null : { deals: cards?.others ?? [], contacts: orderedSelection('contacts', selection, ready.index), companies: orderedSelection('companies', selection, ready.index) };
  const shownCards = cards?.sections.filter(c => c.entries.length > 0 || c.unavailable > 0) ?? [];
  const nothing = entries !== null && shownCards.length === 0 && CATALOG_OBJECTS.every(o => entries[o].length === 0);
  return <div className="pp dock-scroll" data-testid="property-panel">
    <div className="pp-head">
      <button type="button" className="pp-pick" aria-expanded={picking} aria-controls={pickerId} disabled={ready === null}
        onClick={() => { setPicking(p => !p); }}>表示する項目を選ぶ</button>
    </div>
    <div id={pickerId}>{picking && ready !== null && <PropertyPicker catalog={ready} selection={selection}
      onApply={next => { setPicking(false); onApply(next); }} onCancel={() => { setPicking(false); }} />}</div>
    {write?.writesEnabled === false && hasSelection && <p className="crm-muted pp-note" data-testid="writes-off-note">{WRITES_OFF_NOTE}</p>}
    {!hasSelection ? <p className="dock-placeholder">{placeholder}</p>
      : catalog.phase === 'loading' ? <p role="status" className="dock-placeholder">項目の一覧を読み込み中…</p>
        : catalog.phase === 'error' ? <div className="cq-notice cq-error" role="alert"><strong>項目を表示できません</strong><p>{catalog.message}</p>
          <button type="button" onClick={onReloadCatalog}>再試行</button></div>
          : data === null ? <p className="dock-placeholder">{placeholder}</p>
            : nothing ? <p className="crm-muted pp-note">表示する項目がありません。「表示する項目を選ぶ」から選んでください。</p>
              : entries !== null && <>
                {shownCards.map(c => <CardValues key={c.card.id} section={c} data={data} ownerNames={ownerNames} write={write} open={openCards[c.card.id] === true}
                  onToggle={() => { setOpenCards(o => ({ ...o, [c.card.id]: o[c.card.id] !== true })); }} />)}
                {CATALOG_OBJECTS.map(o => <ObjectValues key={o} obj={o} entries={entries[o]} data={data} ownerNames={ownerNames} write={write}
                  title={o === 'deals' && shownCards.length > 0 ? '案件(そのほかの項目)' : undefined} />)}
              </>}
  </div>;
}

/** 親 (架電画面) が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const PropertyPanel = memo(PropertyPanelImpl);
