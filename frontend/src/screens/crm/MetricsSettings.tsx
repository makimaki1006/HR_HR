import { useState } from 'react';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import type { CustomMetricDef } from '../../generated/CustomMetricDef';
import type { MetricKind } from '../../generated/MetricKind';
import type { MetricPrefs } from '../../generated/MetricPrefs';
import { BUILTIN_KEYS, BUILTIN_LABELS, MONTH_KEYS, isBuiltinKey } from './metricsModel';
import { DEFAULT_OWNER_PROPERTY, DEFAULT_PREFS, KIND_LABELS, MAX_CUSTOM, MAX_CUSTOM_CONDITIONS, MAX_NAME_CHARS, newCustomId } from './metricPrefs';
import type { SaveResult } from './metricPrefs';
import { QueuePropertyFilter } from './QueuePropertyConditions';
import { QueuePropertyPicker } from './QueuePropertyPicker';
import { conditionLabel } from './queuePropertyFilter';
import type { CatalogState } from './usePropertyCatalog';
import { NO_VALUE_CHOICES, valueLabel } from './queueValueChoices';
import type { ValueChoiceSource } from './queueValueChoices';

export interface MetricsSettingsProps {
  prefs: MetricPrefs;
  /** false: 設定を読み書きできない (案内を 1 行出し、保存させない) */
  available: boolean;
  note: string | null;
  /** 新規営業の今月の帯を見ている (今月で数えていない項目に注記) */
  monthView: boolean;
  catalog: CatalogState;
  onReloadCatalog: () => void;
  choices?: ValueChoiceSource | undefined;
  onSave: (prefs: MetricPrefs) => Promise<SaveResult>;
  onClose: () => void;
}

const DAY_ONLY_NOTE = '今月の帯には出ません';

function itemName(key: string, prefs: MetricPrefs): string {
  if (isBuiltinKey(key)) return BUILTIN_LABELS[key];
  return prefs.custom.find(c => c.id === key)?.name ?? '';
}

/** 帯の右端「⚙ 項目」で開くパネル。出す・出さない・並べ替え・自分の項目の追加/修正/削除。保存ボタンで 1 回だけ書く */
export function MetricsSettings({ prefs, available, note, monthView, catalog, onReloadCatalog, choices = NO_VALUE_CHOICES, onSave, onClose }: MetricsSettingsProps) {
  const [draft, setDraft] = useState<MetricPrefs>(prefs);
  const [editing, setEditing] = useState<CustomMetricDef | null>(null);
  const [status, setStatus] = useState<{ kind: 'idle' | 'saving' | 'saved' } | { kind: 'error'; message: string }>({ kind: 'idle' });
  const allKeys = [...BUILTIN_KEYS, ...draft.custom.map(c => c.id)];
  const hidden = allKeys.filter(k => !draft.items.includes(k));
  const change = (next: MetricPrefs) => { setDraft(next); setStatus({ kind: 'idle' }); };
  const toggle = (key: string, on: boolean) => { change({ ...draft, items: on ? [...draft.items, key] : draft.items.filter(k => k !== key) }); };
  const move = (i: number, d: -1 | 1) => {
    const j = i + d;
    if (j < 0 || j >= draft.items.length) return;
    const items = [...draft.items];
    const a = items[i];
    const b = items[j];
    if (a === undefined || b === undefined) return;
    items[i] = b; items[j] = a;
    change({ ...draft, items });
  };
  const remove = (id: string) => { change({ items: draft.items.filter(k => k !== id), custom: draft.custom.filter(c => c.id !== id) }); };
  const putCustom = (d: CustomMetricDef) => {
    const exists = draft.custom.some(c => c.id === d.id);
    change({
      custom: exists ? draft.custom.map(c => (c.id === d.id ? d : c)) : [...draft.custom, d],
      items: draft.items.includes(d.id) ? draft.items : [...draft.items, d.id],
    });
    setEditing(null);
  };
  const save = async () => {
    setStatus({ kind: 'saving' });
    const r = await onSave(draft);
    setStatus(r.ok ? { kind: 'saved' } : { kind: 'error', message: r.message });
  };
  const row = (key: string, shown: boolean, i: number) => {
    const custom = draft.custom.find(c => c.id === key);
    const name = itemName(key, draft);
    const dayOnly = monthView && isBuiltinKey(key) && !MONTH_KEYS.includes(key);
    return <li key={key} className="tm-set-row" data-testid={`metric-setting-${key}`}>
      <label className="tm-set-name"><input type="checkbox" checked={shown} aria-label={`${name}を帯に出す`} onChange={e => { toggle(key, e.target.checked); }} />
        <span>{name}</span>
        {custom && <small className="tm-set-kind">自分の項目・{KIND_LABELS[custom.kind]}</small>}
        {dayOnly && <small className="tm-set-kind">{DAY_ONLY_NOTE}</small>}</label>
      <span className="tm-set-actions">
        {shown && <><button type="button" className="tm-set-btn" aria-label={`${name}を上へ`} disabled={i === 0} onClick={() => { move(i, -1); }}>↑</button>
          <button type="button" className="tm-set-btn" aria-label={`${name}を下へ`} disabled={i === draft.items.length - 1} onClick={() => { move(i, 1); }}>↓</button></>}
        {custom && <><button type="button" className="tm-set-btn" onClick={() => { setEditing(custom); }}>直す</button>
          <button type="button" className="tm-set-btn" aria-label={`${name}を消す`} onClick={() => { remove(custom.id); }}>消す</button></>}
      </span>
    </li>;
  };
  return <div className="tm-settings" role="dialog" aria-label="帯に出す項目" data-testid="metrics-settings">
    <div className="tm-set-head"><strong>帯に出す項目</strong><button type="button" className="tm-set-btn" onClick={onClose}>閉じる</button></div>
    {!available && <p className="tm-set-note" role="status">{note ?? 'いまは項目の設定を読み書きできません。'}</p>}
    <ul className="tm-set-list" aria-label="出す項目">{draft.items.map((k, i) => row(k, true, i))}</ul>
    {hidden.length > 0 && <><p className="tm-set-sub">出さない項目</p><ul className="tm-set-list" aria-label="出さない項目">{hidden.map(k => row(k, false, -1))}</ul></>}
    {editing === null
      ? <div className="tm-set-add"><button type="button" className="tm-set-btn" disabled={draft.custom.length >= MAX_CUSTOM || !available}
        onClick={() => { setEditing({ id: newCustomId(), name: '', kind: 'count', conditions: [], owner_property: DEFAULT_OWNER_PROPERTY }); }}>＋ 自分の項目を足す</button>
        <small>あと {MAX_CUSTOM - draft.custom.length} 個。条件に合う取引の件数・数値の項目の合計・平均を出せます。</small></div>
      : <CustomEditor key={editing.id} def={editing} catalog={catalog} onReloadCatalog={onReloadCatalog} choices={choices} onDone={putCustom} onCancel={() => { setEditing(null); }} />}
    <div className="tm-set-foot">
      <button type="button" className="tm-set-btn tm-set-main" disabled={!available || status.kind === 'saving' || editing !== null} onClick={() => { void save(); }}>保存</button>
      <button type="button" className="tm-set-btn" disabled={!available} onClick={() => { change(DEFAULT_PREFS); }}>標準に戻す</button>
      {status.kind === 'saving' && <span role="status">保存しています…</span>}
      {status.kind === 'saved' && <span role="status" className="tm-set-ok">保存しました</span>}
      {status.kind === 'error' && <span role="alert" className="tm-set-err">{status.message}</span>}
      {editing !== null && <small>項目の編集を終えてから保存してください。</small>}
    </div>
  </div>;
}

function CustomEditor({ def, catalog, onReloadCatalog, choices, onDone, onCancel }: {
  def: CustomMetricDef; catalog: CatalogState; onReloadCatalog: () => void; choices: ValueChoiceSource;
  onDone: (d: CustomMetricDef) => void; onCancel: () => void;
}) {
  const [d, setD] = useState<CustomMetricDef>(def);
  const [adding, setAdding] = useState(false);
  const props = catalog.phase === 'ready' ? [...catalog.index.deals.values()].map(e => e.prop) : [];
  const numbers = props.filter(p => p.property_type === 'number');
  const owners = props.filter(p => p.owner_ref);
  const valueProp = numbers.find(p => p.name === d.property) ?? null;
  const name = d.name.trim();
  const valid = name.length > 0 && Array.from(name).length <= MAX_NAME_CHARS && (d.kind === 'count' || valueProp !== null) && d.conditions.length <= MAX_CUSTOM_CONDITIONS;
  const setKind = (kind: MetricKind) => {
    const next: CustomMetricDef = { ...d, kind };
    if (kind === 'count') delete next.property;
    setD(next);
  };
  const chip = (i: number) => {
    const c = d.conditions[i];
    if (!c) return null;
    const p: CrmCatalogProperty | undefined = catalog.phase === 'ready' ? catalog.index.deals.get(c.property)?.prop : undefined;
    const label = conditionLabel(c, p, v => (p ? valueLabel(p, v, choices) : undefined));
    return <li key={i} className="cq-chip"><span>{label}</span><button type="button" aria-label={`「${label}」を外す`}
      onClick={() => { setD({ ...d, conditions: d.conditions.filter((_, k) => k !== i) }); }}>×</button></li>;
  };
  const remaining = MAX_CUSTOM_CONDITIONS - d.conditions.length;
  return <fieldset className="tm-set-editor"><legend>{def.name ? `「${def.name}」を直す` : '自分の項目を足す'}</legend>
    <label>名前（帯に出す短い名前）<input aria-label="名前" value={d.name} maxLength={MAX_NAME_CHARS} placeholder="例: 不在" onChange={e => { setD({ ...d, name: e.target.value }); }} /></label>
    <fieldset className="tm-set-kinds"><legend>数え方</legend>
      {(['count', 'sum', 'avg'] as const).map(k => <label key={k}><input type="radio" name={`kind-${def.id}`} checked={d.kind === k} onChange={() => { setKind(k); }} />
        {k === 'count' ? '件数（条件に合う取引の数）' : k === 'sum' ? '合計（数値の項目）' : '平均（数値の項目）'}</label>)}
    </fieldset>
    {d.kind !== 'count' && (catalog.phase === 'ready'
      ? <QueuePropertyPicker properties={numbers} value={valueProp} label="合計・平均にする数値の項目" onChange={p => { setD(p ? { ...d, property: p.name } : (() => { const n = { ...d }; delete n.property; return n; })()); }} />
      : <p role="status">項目の一覧を読み込み中…</p>)}
    <label>誰の取引か<select aria-label="誰の取引か" value={d.owner_property} onChange={e => { setD({ ...d, owner_property: e.target.value }); }}>
      {owners.length === 0 && <option value={DEFAULT_OWNER_PROPERTY}>自分が担当（取引の担当者）</option>}
      {owners.map(p => <option key={p.name} value={p.name}>{p.name === DEFAULT_OWNER_PROPERTY ? `自分が担当（${p.label}）` : `自分が${p.label}`}</option>)}
    </select></label>
    <div className="tm-set-conds"><span>条件（すべてに合う取引）</span>
      {d.conditions.length > 0 ? <ul className="cq-chips">{d.conditions.map((_, i) => chip(i))}</ul> : <small>条件なし（「誰の」に当てはまる取引すべて）</small>}
      {!adding && <button type="button" className="tm-set-btn" disabled={remaining <= 0} onClick={() => { setAdding(true); }}>＋ 条件を追加</button>}
      <small>条件はあと {remaining} 個です</small>
      {adding && <QueuePropertyFilter catalog={catalog} count={d.conditions.length} remaining={remaining} choices={choices} onReload={onReloadCatalog}
        hint={`すべての条件に合う取引を数えます。最大 ${String(MAX_CUSTOM_CONDITIONS)} 条件。今日・今月・過去 N 日も選べます。`}
        limitNote={`条件は ${String(MAX_CUSTOM_CONDITIONS)} 個までです。条件を外すと追加できます。`}
        onAdd={c => { setD({ ...d, conditions: [...d.conditions, c] }); setAdding(false); }} />}
    </div>
    <div className="tm-set-foot">
      <button type="button" className="tm-set-btn tm-set-main" disabled={!valid} onClick={() => { onDone({ ...d, name }); }}>この項目を決める</button>
      <button type="button" className="tm-set-btn" onClick={onCancel}>やめる</button>
    </div>
  </fieldset>;
}
