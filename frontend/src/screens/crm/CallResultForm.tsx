import { useEffect, useId, useRef, useState } from 'react';
import type { CallState } from './smartEmbed';
import type { MocPropertyDefinition } from './mocProperties';
import type { DefinitionsState } from './useResultDefinitions';
import { clock } from './workspaceModel';
import {
  CALL_RESULTS, FALLBACK_LABELS, FIELD_PROPERTY, TEXT_LIMITS, activeFields, draftSummary, selectableOptions, validateResultDraft, withField, withOutcome,
} from './callResultModel';
import type { CallResult, DraftErrors, DraftField, ResultDraft } from './callResultModel';
import './result-form.css';

const OUTCOMES = Object.entries(CALL_RESULTS) as [CallResult, string][];
const ENDED_LABELS: Record<string, string> = { missed: '応答なし', rejected: '拒否' };
export const COLLAPSED_NOTICE = '入力欄を開きました。内容を確かめてから、もう一度「記録して次へ」を押してください。';

/** 文字を入力中の要素か (自動でフォーカスを移さない) */
function isTyping(el: Element | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  return el instanceof HTMLInputElement && !['button', 'submit', 'reset', 'checkbox', 'radio'].includes(el.type);
}

/** 中央の列の下端に固定する、架電結果の入力欄 (下書き。HubSpot には送らない) */
export function CallResultForm({
  dealId, draft, onChange, defsState, onReloadDefs, recorded, onRecord, onClear, collapsed, onCollapsedChange, endedCall, focusCallId, onCallHandled, today, notice,
  recordBlocked = false, persistFailed = false, autoFocusOutcome = false, onAnnounce,
}: {
  dealId: string;
  draft: ResultDraft;
  onChange: (d: ResultDraft) => void;
  defsState: DefinitionsState;
  onReloadDefs: () => void;
  recorded: boolean;
  /** 記録して次へ。記録できなかった (送信の直前に日付が変わって検証に落ちた等) ときは false */
  onRecord: () => boolean;
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
  /** 記録できない状態 (選んだ案件がいまの一覧に無い等)。理由は notice に出す */
  recordBlocked?: boolean;
  /** 下書きをこのタブに残せていない (sessionStorage に書けない)。閉じる・再読み込みで消える */
  persistFailed?: boolean;
  /** 開いたら (選択肢が読めたら) 結果のボタンへフォーカスする (記録して次へで次の案件に移ったとき) */
  autoFocusOutcome?: boolean;
  /** 画面全体の読み上げ欄に流す (この入力欄は案件ごとに作り直すので、入力欄の外に置く) */
  onAnnounce?: ((text: string) => void) | undefined;
}) {
  const ready = defsState.phase === 'ready';
  const defs: Record<string, MocPropertyDefinition> = ready ? defsState.defs : {};
  const errors: DraftErrors = ready ? validateResultDraft(draft, defs, today) : {};
  const valid = ready && Object.keys(errors).length === 0;
  const canRecord = valid && !recordBlocked;
  const active = activeFields(draft);
  const formRef = useRef<HTMLFormElement | null>(null);
  const outcomeRef = useRef<HTMLDivElement | null>(null);
  const focusOutcome = useRef(autoFocusOutcome);
  const uid = useId();
  // 検証の文言は、その欄を触ったか、記録を試みた後にだけ出す (選んだ直後に赤い文言を並べない)
  const [touched, setTouched] = useState<ReadonlySet<DraftField>>(() => new Set());
  const [attempted, setAttempted] = useState(false);
  const focusInvalid = useRef(false);
  // 入力欄の中だけで出す案内 (折りたたみ中に Ctrl+Enter を押した等)
  const [localNotice, setLocalNotice] = useState<string | null>(null);
  const outcomeButtons = () => Array.from(outcomeRef.current?.querySelectorAll<HTMLButtonElement>('button.rf-outcome') ?? []);
  const touch = (f: DraftField) => { setTouched(prev => (prev.has(f) ? prev : new Set(prev).add(f))); };

  // 通話が終わったら、入力欄を開いて結果のボタンにフォーカスする。1 つの通話につき 1 回だけ
  // (案件を行き来したときにフォーカスを奪わないよう、済んだことは onCallHandled で画面側に返す)
  useEffect(() => {
    if (focusCallId === null) return;
    focusOutcome.current = true;
    onCollapsedChange(false);
  }, [focusCallId, onCollapsedChange]);
  useEffect(() => {
    if (!focusOutcome.current || collapsed || !ready) return;
    focusOutcome.current = false;
    // 文字を入力中 (メモ等) ならフォーカスを奪わない。次の Space / Enter で結果を選んでしまわないように
    if (!isTyping(document.activeElement)) outcomeButtons().find(b => b.tabIndex === 0)?.focus();
    if (focusCallId !== null) onCallHandled(focusCallId);
  });

  // 記録を試みて足りない欄があったら、最初の欄へフォーカスする
  useEffect(() => {
    if (!focusInvalid.current) return;
    focusInvalid.current = false;
    const target = formRef.current?.querySelector<HTMLElement>('[aria-invalid="true"]')
      ?? (draft.outcome === '' ? outcomeRef.current?.querySelector<HTMLButtonElement>('button') : null);
    target?.focus();
  });

  function submit() {
    setLocalNotice(null);
    if (!ready || recordBlocked) {
      // 押しても何も起きないように見えないよう、記録できない理由を読み上げ直す
      if (notice) onAnnounce?.(notice);
      return;
    }
    if (valid && onRecord()) return;
    setAttempted(true);
    focusInvalid.current = true;
  }

  // Ctrl+Enter / Cmd+Enter で「記録して次へ」。入力欄の中で押したときだけ (検索欄など他の場所では何もしない)。
  // 折りたたみ中は記録せず (中身を見ないまま記録しない)、入力欄を開いて知らせる
  function onKeyDown(e: React.KeyboardEvent<HTMLFormElement>) {
    if (e.key !== 'Enter' || !(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    if (collapsed) {
      onCollapsedChange(false);
      onAnnounce?.(COLLAPSED_NOTICE);
      setLocalNotice(COLLAPSED_NOTICE);
      return;
    }
    submit();
  }

  // 結果の 6 つのボタンは Tab では 1 か所 (選んだもの、無ければ先頭)。矢印キーで移る
  function onOutcomeKey(e: React.KeyboardEvent<HTMLDivElement>) {
    const btns = outcomeButtons();
    const cur = btns.findIndex(b => b === document.activeElement);
    if (cur === -1) return;
    // 1 行に並んでいる数 (幅が狭いと 3 列 × 2 行)。上下は 1 行分動く
    const firstTop = btns[0]?.offsetTop ?? 0;
    const cols = btns.filter(b => b.offsetTop === firstTop).length;
    const rowStep = cols >= btns.length ? 1 : cols;
    const last = btns.length - 1;
    const next = e.key === 'ArrowRight' ? cur + 1 : e.key === 'ArrowLeft' ? cur - 1 : e.key === 'ArrowDown' ? cur + rowStep
      : e.key === 'ArrowUp' ? cur - rowStep : e.key === 'Home' ? 0 : e.key === 'End' ? last : null;
    if (next === null) return;
    e.preventDefault();
    btns[Math.min(last, Math.max(0, next))]?.focus();
  }

  const set = (f: DraftField, v: string) => { touch(f); onChange(withField(draft, f, v)); };
  const label = (f: DraftField) => defs[FIELD_PROPERTY[f]]?.label ?? FALLBACK_LABELS[FIELD_PROPERTY[f]] ?? '';
  const shown = (f: DraftField) => (attempted || touched.has(f) ? errors[f] : undefined);
  const err = (f: DraftField) => shown(f) && <small className="rf-err" id={`${uid}-${f}-err`}>{shown(f)}</small>;
  const described = (f: DraftField) => (shown(f) ? `${uid}-${f}-err` : undefined);
  const invalid = (f: DraftField) => !!shown(f);

  function choice({ f, required = false }: { f: DraftField; required?: boolean }) {
    const opts = selectableOptions(defs[FIELD_PROPERTY[f]]);
    const value = draft[f];
    const unknown = value !== '' && !opts.some(o => o.value === value);
    if (opts.length <= 4) {
      // まとまりの名前は fieldset の legend だけ (同じ名前のグループを入れ子にしない)。検証の文言は各選択肢にも結び付ける
      return <fieldset className="rf-field" aria-describedby={described(f)} onBlur={() => { touch(f); }}>
        <legend>{label(f)}{required && <span className="rf-req">必須</span>}</legend>
        <div className="rf-chips">
          {opts.map(o => <label key={o.value} className="rf-chip"><input type="radio" name={`${uid}-${f}`} value={o.value}
            aria-invalid={invalid(f)} aria-describedby={described(f)} checked={value === o.value} onChange={() => { set(f, o.value); }} />{o.label}</label>)}
          {unknown && <label className="rf-chip"><input type="radio" name={`${uid}-${f}`} checked readOnly />(現在の値) {value}</label>}
          {value !== '' && !required && <button type="button" className="rf-clear" onClick={e => {
            // このボタンは値が空になると消えるので、先に同じ欄の最初の選択肢へフォーカスを移す (body に落とさない)
            e.currentTarget.closest('fieldset')?.querySelector<HTMLInputElement>('input[type="radio"]')?.focus();
            set(f, '');
          }}>選択を外す</button>}
        </div>
        {err(f)}
      </fieldset>;
    }
    return <label className="rf-field">
      <span className="rf-label">{label(f)}{required && <span className="rf-req">必須</span>}</span>
      <select value={value} aria-invalid={invalid(f)} aria-describedby={described(f)} onBlur={() => { touch(f); }}
        onChange={e => { set(f, e.target.value); }}>
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
      <input type="date" value={draft[f]} min={today} aria-invalid={invalid(f)} aria-describedby={described(f)}
        onBlur={() => { touch(f); }} onChange={e => { set(f, e.target.value); }} />
      {err(f)}
    </label>;
  }

  function textInput({ f, required = false, multiline = false }: { f: DraftField; required?: boolean; multiline?: boolean }) {
    const max = TEXT_LIMITS[f];
    const common = { value: draft[f], 'aria-invalid': invalid(f), 'aria-describedby': described(f), onBlur: () => { touch(f); } };
    return <label className={`rf-field${multiline ? ' rf-wide' : ''}`}>
      <span className="rf-label">{label(f)}{required && <span className="rf-req">必須</span>}
        {max !== undefined && <small className="rf-count">{draft[f].length}/{max}</small>}</span>
      {multiline ? <textarea rows={3} {...common} onChange={e => { set(f, e.target.value); }} />
        : <input type="text" {...common} onChange={e => { set(f, e.target.value); }} />}
      {err(f)}
    </label>;
  }

  const needNext = draft.outcome === 'callback' || (active.has('nextAction') && draft.nextAction === '再架電');
  // 残せていないときは「記録済み」を保存できた状態のように見せない (headless-crm-design §12: どこにも保存できていない状態は赤で区別する)
  const status = persistFailed ? (recorded ? '記録済み(この画面を閉じると消えます)' : '下書き(この画面を閉じると消えます)')
    : recorded ? '記録済み(HubSpot 未送信)' : '下書き(HubSpot 未送信)';
  const ended = endedCall !== null;
  const talk = endedCall?.talkSeconds ?? null;

  const shownNotice = notice ?? localNotice;
  const noticeId = `${uid}-notice`;
  const bodyId = `${uid}-body`;

  // 1 行の入力欄で Enter を押したときのブラウザの自動送信では記録しない (記録は「記録して次へ」と Ctrl/⌘+Enter だけ)
  return <form ref={formRef} className={`rf${collapsed ? ' is-collapsed' : ''}`} aria-label="架電結果の入力" data-deal-id={dealId}
    onSubmit={e => { e.preventDefault(); }} onKeyDown={onKeyDown}>
    <div className="rf-head">
      <button type="button" className="rf-toggle" aria-expanded={!collapsed} aria-controls={bodyId}
        onClick={() => { setLocalNotice(null); onCollapsedChange(!collapsed); }}>
        <span aria-hidden="true">{collapsed ? '▲' : '▼'}</span> 架電結果</button>
      {ended && <span className="rf-call" data-testid="ended-call">通話終了{talk !== null ? ` 通話時間 ${clock(talk)}` : endedCall.result ? `(${ENDED_LABELS[endedCall.result] ?? 'つながらず'})` : ''}</span>}
      {collapsed && ready && <span className="rf-summary" data-testid="draft-summary">{draftSummary(draft, defs)}</span>}
      <span className={`rf-status${persistFailed ? ' is-unsaved' : recorded ? ' is-recorded' : ''}`} role="status">{status}</span>
    </div>

    {/* 折りたたみは hidden で隠す (aria-controls の先を残す) */}
    <div className="rf-body" id={bodyId} hidden={collapsed}>
      {persistFailed && <div className="cq-notice cq-error rf-unsaved" role="alert" data-testid="unsaved-alert">
        <strong>入力を残せていません</strong>
        <p>このブラウザの設定などで、入力をこの画面に残せません。この画面を閉じたり再読み込みしたりすると入力が消えます。HubSpot にも保存されていません。</p></div>}
      {/* 読み上げ欄は先に置いておき、中身だけ変える (中身と一緒に作ると読み上げられないことがある) */}
      <p role="status" className="rf-muted rf-live">{defsState.phase === 'loading' ? '選択肢を読み込み中…' : ''}</p>
      {defsState.phase === 'error' && <div className="cq-notice cq-error rf-defs-error" role="alert">
        <strong>入力欄を表示できません</strong><p>{defsState.message}</p>
        <p>入力欄が表示されるまで、この架電先の結果は記録できません。再試行しても表示されないときは管理者に連絡してください。</p>
        <button type="button" onClick={onReloadDefs}>再試行</button></div>}

      {ready && <>
        <div className="rf-outcomes" role="group" aria-label="今回の結果" ref={outcomeRef} onKeyDown={onOutcomeKey}
          aria-describedby={errors.outcome ? `${uid}-outcome-err` : undefined}>
          {OUTCOMES.map(([id, text], i) => <button key={id} type="button" className="rf-outcome" data-outcome={id}
            tabIndex={(draft.outcome === '' ? i === 0 : draft.outcome === id) ? 0 : -1}
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
        <small className="rf-muted">タスクメモはまだ HubSpot に送られません。送るようになると、HubSpot にある今の内容は書き換えられます。</small>
      </>}
    </div>

    {/* 記録ボタンと「HubSpot には保存されない」注記は、入力欄のスクロールの外 (常に見える下端) に置く */}
    <div className="rf-actions" hidden={collapsed}>
      <span className="rf-notice" role="status" id={noticeId}>{shownNotice ?? ''}</span>
      <small className="rf-unsent" id={`${uid}-unsent`}>この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません</small>
      <button type="button" className="cq-btn cq-btn-quiet" onClick={onClear}>下書きを消す</button>
      {/* type="button": 1 行の入力欄で Enter を押しても記録しない */}
      <button type="button" className="rf-record" onClick={submit} aria-disabled={!canRecord}
        aria-describedby={recordBlocked && notice ? `${noticeId} ${uid}-unsent` : `${uid}-unsent`}
        title="入力欄の中で Ctrl+Enter / ⌘+Enter でも記録できます">
        記録して次へ <kbd>Ctrl+Enter</kbd></button>
    </div>
  </form>;
}
