import { useId, useRef, useState } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';
import { draftToValue, initialDraft, sameValue } from './writeModel';
import type { FieldDef } from './writeModel';
import type { FieldStatus } from './useCrmWrite';
import type { ConflictView } from './writeBindings';
import './write.css';

/** 値を入れる欄 (項目の種類ごと)。値は文字列 (日付は YYYY-MM-DD、真偽は true/false、複数選択は ; 区切り) */
export function PropInput({ def, value, onChange, autoFocus, onKeyDown, id }: {
  def: FieldDef; value: string; onChange: (v: string) => void; autoFocus?: boolean; onKeyDown?: (e: KeyboardEvent) => void; id?: string;
}) {
  const common = { id, 'aria-label': def.label, autoFocus, onKeyDown } as const;
  switch (def.kind) {
    case 'textarea':
      return <textarea {...common} rows={4} maxLength={def.maxLength} value={value} onChange={e => { onChange(e.target.value); }} />;
    case 'select':
      return <select {...common} value={value} onChange={e => { onChange(e.target.value); }}>
        <option value="">(未入力)</option>
        {def.options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>;
    case 'multi': {
      const chosen = value === '' ? [] : value.split(';');
      return <span role="group" aria-label={def.label} className="wr-multi">
        {def.options.map(o => <label key={o.value}><input type="checkbox" checked={chosen.includes(o.value)}
          onChange={e => { onChange(def.options.map(x => x.value).filter(v => (v === o.value ? e.target.checked : chosen.includes(v))).join(';')); }} />{o.label}</label>)}
      </span>;
    }
    case 'date':
      return <input {...common} type="date" value={value} onChange={e => { onChange(e.target.value); }} />;
    case 'number':
      return <input {...common} type="number" value={value} onChange={e => { onChange(e.target.value); }} />;
    case 'bool':
      return <input {...common} type="checkbox" checked={value === 'true'} onChange={e => { onChange(e.target.checked ? 'true' : 'false'); }} />;
    default:
      return <input {...common} type="text" maxLength={def.maxLength} value={value} onChange={e => { onChange(e.target.value); }} />;
  }
}

/** 保存の状態 (緑 = 保存済み / 黄 = 一時保存・HubSpot 反映待ち / 赤 = 保存できていません) */
export function SaveStatus({ status, onRetry }: { status: FieldStatus | undefined; onRetry?: (() => void) | undefined }) {
  if (status === undefined) return null;
  switch (status.phase) {
    case 'saving': return <span className="wr-status wr-saving" role="status">保存中…</span>;
    case 'saved': return <span className="wr-status wr-saved" role="status" data-state="saved">✓ 保存済み</span>;
    case 'queued': return <span className="wr-status wr-queued" role="status" data-state="queued">
      {status.slow ? 'まだ反映待ちです(HubSpot に一時保存しています。しばらくしても反映されないときは管理者に伝えてください)' : '反映待ち(HubSpot に一時保存しました。つながり次第、自動で反映します)'}</span>;
    case 'invalid': return <span className="wr-status wr-error" role="alert" data-state="error">入力を確認してください: {status.message}</span>;
    case 'error': return <span className="wr-status wr-error" role="alert" data-state="error">保存できていません: {status.message}
      {status.req !== null && onRetry && <button type="button" className="wr-retry" onClick={onRetry}>もう一度保存</button>}</span>;
  }
}

/** 値の表示 + 「編集」。編集中は入力欄と 保存 / やめる (Esc でもやめる) */
export function EditableValue({ def, raw, display, status, onSave, onRetry }: {
  def: FieldDef; raw: string | null | undefined; display: ReactNode; status: FieldStatus | undefined;
  onSave: (base: string | null, value: string | null) => void; onRetry?: (() => void) | undefined;
}) {
  const [session, setSession] = useState<{ base: string | null; draft: string } | null>(null);
  const editBtn = useRef<HTMLButtonElement | null>(null);
  const inputId = useId();
  const busy = status?.phase === 'saving';
  function finish() { setSession(null); window.setTimeout(() => { editBtn.current?.focus(); }, 0); }
  function commit() {
    if (session === null) return;
    const next = draftToValue(def.kind, session.draft);
    const before = draftToValue(def.kind, initialDraft(def.kind, session.base));
    finish();
    if (sameValue(next, before)) return;
    // 「見ていた値」は編集を始めたときのもの (編集中に表示が変わっても変えない)
    onSave(session.base, next);
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); finish(); return; }
    if (e.key === 'Enter' && def.kind !== 'textarea' && !e.nativeEvent.isComposing) { e.preventDefault(); commit(); }
    if (e.key === 'Enter' && def.kind === 'textarea' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); commit(); }
  }
  if (session !== null) {
    return <span className="wr-edit" data-testid={`edit-${def.name}`}>
      <PropInput def={def} id={inputId} value={session.draft} autoFocus onKeyDown={key} onChange={v => { setSession(s => (s === null ? s : { ...s, draft: v })); }} />
      <span className="wr-edit-actions">
        <button type="button" className="wr-save" onClick={commit}>保存</button>
        <button type="button" onClick={finish}>やめる</button>
      </span>
    </span>;
  }
  return <span className="wr-view">
    {display}
    <button type="button" ref={editBtn} className="wr-edit-btn" title="編集" disabled={busy} aria-label={`${def.label}を編集`}
      onClick={() => { setSession({ base: raw ?? null, draft: initialDraft(def.kind, raw) }); }} />
    <SaveStatus status={status} onRetry={onRetry} />
  </span>;
}

/** 衝突 (409): HubSpot の今の値と自分の値のどちらを使うか選ぶ */
export function ConflictDialog({ view }: { view: ConflictView }) {
  const titleId = useId();
  return <div className="wr-overlay">
    <div className="wr-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} data-testid="conflict-dialog">
      <h3 id={titleId}>HubSpot 側で先に変更されています</h3>
      <p>保存しようとした項目が、見ていたときから HubSpot で変わっていました。どちらの値を使いますか。</p>
      <table className="wr-conflict">
        <thead><tr><th>項目</th><th>HubSpot の今の値</th><th>あなたの値</th></tr></thead>
        <tbody>{view.rows.map(r => <tr key={r.name}><th scope="row">{r.label}</th><td>{r.theirs}</td><td>{r.mine}</td></tr>)}</tbody>
      </table>
      <div className="wr-dialog-actions">
        <button type="button" onClick={view.keepTheirs}>HubSpotの値を使う</button>
        <button type="button" className="wr-primary" onClick={view.keepMine}>自分の値で上書き</button>
      </div>
    </div>
  </div>;
}
