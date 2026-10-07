import { useEffect, useId, useRef } from 'react';
import type { CallState } from './smartEmbed';
import type { MocPropertyDefinition } from './mocProperties';
import type { DefinitionsState } from './useResultDefinitions';
import { clock } from './workspaceModel';
import {
  CALL_RESULTS, FALLBACK_LABELS, FIELD_PROPERTY, TEXT_LIMITS, activeFields, draftSummary, selectableOptions, validateResultDraft, withOutcome,
} from './callResultModel';
import type { CallResult, DraftErrors, DraftField, ResultDraft } from './callResultModel';
import './result-form.css';

const OUTCOMES = Object.entries(CALL_RESULTS) as [CallResult, string][];
const ENDED_LABELS: Record<string, string> = { missed: '応答なし', rejected: '拒否' };

/** 中央の列の下端に固定する、架電結果の入力欄 (下書き。HubSpot には送らない) */
export function CallResultForm({
  dealId, draft, onChange, defsState, onReloadDefs, recorded, onRecord, onClear, collapsed, onCollapsedChange, endedCall, focusCallId, onCallHandled, today, notice,
}: {
  dealId: string;
  draft: ResultDraft;
  onChange: (d: ResultDraft) => void;
  defsState: DefinitionsState;
  onReloadDefs: () => void;
  recorded: boolean;
  onRecord: () => void;
  onClear: () => void;
  collapsed: boolean;
  onCollapsedChange: (collapsed: boolean) => void;
  /** この案件から発信した通話が終わったとき */
  endedCall: CallState | null;
  /** まだフォーカスを移していない終了済みの通話の ID (済んでいれば null) */
  focusCallId: string | null;
  onCallHandled: (callId: string) => void;
  /** JST の今日 YYYY-MM-DD */
  today: string;
  notice?: string | undefined;
}) {
  const ready = defsState.phase === 'ready';
  const defs: Record<string, MocPropertyDefinition> = ready ? defsState.defs : {};
  const errors: DraftErrors = ready ? validateResultDraft(draft, defs, today) : {};
  const valid = ready && Object.keys(errors).length === 0;
  const active = activeFields(draft);
  const outcomeRef = useRef<HTMLDivElement | null>(null);
  const focusOutcome = useRef(false);
  const uid = useId();

  // 通話が終わったら、入力欄を開いて結果のボタンにフォーカスする。1 つの通話につき 1 回だけ
  // (案件を行き来したときにフォーカスを奪わないよう、済んだことは onCallHandled で画面側に返す)
  useEffect(() => {
    if (focusCallId === null) return;
    focusOutcome.current = true;
    onCollapsedChange(false);
  }, [focusCallId, onCollapsedChange]);
  useEffect(() => {
    if (!focusOutcome.current || collapsed || !ready || focusCallId === null) return;
    focusOutcome.current = false;
    const btns = outcomeRef.current?.querySelectorAll<HTMLButtonElement>('button');
    const target = Array.from(btns ?? []).find(b => b.getAttribute('aria-pressed') === 'true') ?? btns?.[0];
    target?.focus();
    onCallHandled(focusCallId);
  });

  // Ctrl+Enter / Cmd+Enter で「記録して次へ」 (記録できるときだけ)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Enter' || !(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      if (valid) onRecord();
    };
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('keydown', onKey); };
  }, [valid, onRecord]);

  const set = (f: DraftField, v: string) => { onChange({ ...draft, [f]: v }); };
  const label = (f: DraftField) => defs[FIELD_PROPERTY[f]]?.label ?? FALLBACK_LABELS[FIELD_PROPERTY[f]] ?? FIELD_PROPERTY[f];
  const err = (f: DraftField) => errors[f] && <small className="rf-err" id={`${uid}-${f}-err`} role="alert">{errors[f]}</small>;
  const described = (f: DraftField) => (errors[f] ? `${uid}-${f}-err` : undefined);

  function choice({ f, required = false }: { f: DraftField; required?: boolean }) {
    const opts = selectableOptions(defs[FIELD_PROPERTY[f]]);
    const value = draft[f];
    const unknown = value !== '' && !opts.some(o => o.value === value);
    if (opts.length <= 4) {
      return <fieldset className="rf-field" aria-describedby={described(f)}>
        <legend>{label(f)}{required && <span className="rf-req">必須</span>}</legend>
        <div className="rf-chips" role="radiogroup" aria-label={label(f)}>
          {opts.map(o => <label key={o.value} className="rf-chip"><input type="radio" name={`${uid}-${f}`} value={o.value}
            checked={value === o.value} onChange={() => { set(f, o.value); }} />{o.label}</label>)}
          {unknown && <label className="rf-chip"><input type="radio" name={`${uid}-${f}`} checked readOnly />(現在の値) {value}</label>}
          {value !== '' && !required && <button type="button" className="rf-clear" onClick={() => { set(f, ''); }}>選択を外す</button>}
        </div>
        {err(f)}
      </fieldset>;
    }
    return <label className="rf-field">
      <span className="rf-label">{label(f)}{required && <span className="rf-req">必須</span>}</span>
      <select value={value} aria-invalid={!!errors[f]} aria-describedby={described(f)} onChange={e => { set(f, e.target.value); }}>
        <option value="">選択してください</option>
        {unknown && <option value={value}>(現在の値) {value}</option>}
        {opts.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
      {err(f)}
    </label>;
  }

  function dateInput({ f, required = false }: { f: DraftField; required?: boolean }) {
    return <label className="rf-field">
      <span className="rf-label">{label(f)}{required && <span className="rf-req">必須</span>}</span>
      <input type="date" value={draft[f]} min={today} aria-invalid={!!errors[f]} aria-describedby={described(f)}
        onChange={e => { set(f, e.target.value); }} />
      {err(f)}
    </label>;
  }

  function textInput({ f, required = false, multiline = false }: { f: DraftField; required?: boolean; multiline?: boolean }) {
    const max = TEXT_LIMITS[f];
    const common = { value: draft[f], 'aria-invalid': !!errors[f], 'aria-describedby': described(f) };
    return <label className={`rf-field${multiline ? ' rf-wide' : ''}`}>
      <span className="rf-label">{label(f)}{required && <span className="rf-req">必須</span>}
        {max !== undefined && <small className="rf-count">{draft[f].length}/{max}</small>}</span>
      {multiline ? <textarea rows={3} {...common} onChange={e => { set(f, e.target.value); }} />
        : <input type="text" {...common} onChange={e => { set(f, e.target.value); }} />}
      {err(f)}
    </label>;
  }

  const needNext = draft.outcome === 'callback' || (active.has('nextAction') && draft.nextAction === '再架電');
  const status = recorded ? '記録済み(未送信)' : '下書き(HubSpot 未送信)';
  const ended = endedCall !== null;
  const talk = endedCall?.talkSeconds ?? null;

  return <form className={`rf${collapsed ? ' is-collapsed' : ''}`} aria-label="架電結果の入力" data-deal-id={dealId}
    onSubmit={e => { e.preventDefault(); if (valid) onRecord(); }}>
    <div className="rf-head">
      <button type="button" className="rf-toggle" aria-expanded={!collapsed} onClick={() => { onCollapsedChange(!collapsed); }}>
        <span aria-hidden="true">{collapsed ? '▲' : '▼'}</span> 架電結果</button>
      {ended && <span className="rf-call" data-testid="ended-call">通話終了{talk !== null ? ` 通話時間 ${clock(talk)}` : endedCall.result ? `(${ENDED_LABELS[endedCall.result] ?? 'つながらず'})` : ''}</span>}
      {collapsed && ready && <span className="rf-summary" data-testid="draft-summary">{draftSummary(draft, defs)}</span>}
      <span className={`rf-status${recorded ? ' is-recorded' : ''}`} role="status">{status}</span>
    </div>

    {!collapsed && <div className="rf-body">
      {defsState.phase === 'loading' && <p role="status" className="rf-muted">HubSpot から選択肢の定義を読み込み中…</p>}
      {defsState.phase === 'error' && <div className="cq-notice cq-error rf-defs-error" role="alert">
        <strong>入力欄を表示できません</strong><p>{defsState.message}</p>
        <p>架空の選択肢で代用はしません。再試行するか、HubSpot で直接入力してください。</p>
        <button type="button" onClick={onReloadDefs}>再試行</button></div>}

      {ready && <>
        <div className="rf-outcomes" role="group" aria-label="今回の結果" ref={outcomeRef} aria-describedby={errors.outcome ? `${uid}-outcome-err` : undefined}>
          {OUTCOMES.map(([id, text]) => <button key={id} type="button" className="rf-outcome" data-outcome={id}
            aria-pressed={draft.outcome === id} onClick={() => { onChange(withOutcome(draft, id)); }}>{text}</button>)}
        </div>
        {errors.outcome && <small className="rf-hint" id={`${uid}-outcome-err`}>{errors.outcome}</small>}

        {draft.outcome !== '' && <div className="rf-grid">
          {active.has('spokeTo') && choice({ f: 'spokeTo' })}
          {active.has('interest') && choice({ f: 'interest' })}
          {active.has('unreachable') && choice({ f: 'unreachable', required: draft.outcome === 'wrong_number' })}
          {active.has('unreachableOther') && textInput({ f: 'unreachableOther', required: true })}
          {active.has('stopReason') && textInput({ f: 'stopReason', required: true })}
          {active.has('blockReason') && <details className="rf-more rf-wide"><summary>ブロック理由(任意)</summary>{choice({ f: 'blockReason' })}</details>}
          {active.has('apptDate') && <div className="rf-row rf-wide">
            {dateInput({ f: 'apptDate', required: true })}{choice({ f: 'apptTime', required: true })}{choice({ f: 'apptMethod', required: true })}</div>}
          {active.has('nextAction') && choice({ f: 'nextAction' })}
          {active.has('nextCallDate') && <div className="rf-row">
            {dateInput({ f: 'nextCallDate', required: needNext })}{choice({ f: 'nextCallTime', required: needNext })}</div>}
        </div>}
        {textInput({ f: 'memo', multiline: true })}
        <small className="rf-muted">タスクメモは HubSpot に送るときは上書きになります(送信は未実装)。</small>
      </>}

      <div className="rf-actions">
        {notice && <span className="rf-notice" role="status">{notice}</span>}
        <button type="button" className="cq-btn cq-btn-quiet" onClick={onClear}>下書きを消す</button>
        <button type="submit" className="rf-record" disabled={!valid} title="Ctrl+Enter / ⌘+Enter">
          記録して次へ <kbd>Ctrl+Enter</kbd></button>
      </div>
      <small className="rf-muted">「記録して次へ」はこのブラウザの中で記録済みの印を付けるだけです。HubSpot には保存されません。</small>
    </div>}
  </form>;
}
