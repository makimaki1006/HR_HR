import { useEffect, useId, useRef, useState } from 'react';
import type { CallState } from './smartEmbed';
import { clock } from './workspaceModel';
import { PropInput } from './WriteWidgets';
import { MoveModal } from './StageMove';
import type { Target } from './StageMove';
import { ruleFieldNames } from './writeModel';
import type { FieldChange, FieldDef } from './writeModel';
import type { SaveOutcome } from './useCrmWrite';
import type { StageMoveCtx } from './writeBindings';
import { RESULT_GROUPS, appendMemo, isEmptyDraft, memoDatePrefix, previousDateLabel, quickStagesOf, resultChanges, shownValue } from './callResultModel';
import type { ResultDraft, ResultFieldSpec, ResultTeam } from './callResultModel';
import './result-form.css';

const ENDED_LABELS: Record<string, string> = { missed: '応答なし', rejected: '拒否' };
export const COLLAPSED_NOTICE = '入力欄を開きました。内容を確かめてから、もう一度「記録して次へ」を押してください。';
/** 書き込みが OFF (試験運用の対象外) のとき */
export const RESULT_WRITES_OFF = 'この案件は、まだ HubSpot に保存できないため、ここでは入力できません(試験運用中)。';
/** 編集できる項目の一覧を読めなかった (権限が無い・通信の失敗) とき */
export const RESULT_SCHEMA_FAILED = 'HubSpot に保存できる項目を読み込めなかったため、ここでは入力できません。続くときは管理者に連絡してください。';
export const MEMO_EDIT_HINT = '書いた内容は、今の内容の末尾に日付を付けて足します。全文を直すときは、左のプロパティ欄の編集を使ってください。';

/** 保存した結果 (none = 変えた項目もステージも無いので送っていない) */
export type ResultSaved = 'saved' | 'queued' | 'none';

/** 文字を入力中の要素か (自動でフォーカスを移さない) */
function isTyping(el: Element | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  return el instanceof HTMLInputElement && !['button', 'submit', 'reset', 'checkbox', 'radio'].includes(el.type);
}

/** 読み取りの表示用の値 (選択肢はラベル、日付は YYYY/MM/DD) */
function readValue(def: FieldDef | null, raw: string | null | undefined): string {
  if (raw === null || raw === undefined || raw === '') return '(未入力)';
  if (def?.kind === 'date' && /^\d{4}-\d{2}-\d{2}/.test(raw)) return raw.slice(0, 10).replace(/-/g, '/');
  if (def?.kind === 'bool') return raw === 'true' ? 'はい' : 'いいえ';
  return def?.options.find(o => o.value === raw)?.label ?? raw;
}

interface ModalState { target: Target; changes: FieldChange[]; presets: Record<string, string> }

/**
 * 「架電結果の入力」パネル。HubSpot に実在する取引プロパティ (組ごと) とステージを、1 回の保存で送る。
 * 入力途中の内容は下書きとしてこのタブに残す (HubSpot には「記録して次へ」で送る)
 */
export function CallResultForm({
  dealId, team, ctx, current, draft, onChange, today, onSaved, onClear, collapsed, onCollapsedChange, endedCall, focusCallId, onCallHandled, notice,
  noticeTone = 'info', recordBlocked = false, persistFailed = false, autoFocus = false, onAnnounce,
}: {
  dealId: string;
  team: ResultTeam;
  /** 書き込みの窓口 (案件の詳細を読み込むまで null) */
  ctx: StageMoveCtx | null;
  /** HubSpot の今の値 (組の項目。読み込むまで null) */
  current: Readonly<Record<string, string | null>> | null;
  draft: ResultDraft;
  onChange: (d: ResultDraft) => void;
  /** JST の今日 YYYY-MM-DD */
  today: string;
  /** HubSpot に保存できた (緑・黄)、または送るものが無かった。画面は次の案件へ進む */
  onSaved: (kind: ResultSaved) => void;
  onClear: () => void;
  collapsed: boolean;
  onCollapsedChange: (collapsed: boolean) => void;
  /** この案件から発信した通話が終わったとき */
  endedCall: CallState | null;
  /** まだフォーカスを移していない終了済みの通話の ID (済んでいれば null) */
  focusCallId: string | null;
  onCallHandled: (callId: string) => void;
  notice?: string | undefined;
  noticeTone?: 'info' | 'saved' | 'queued' | undefined;
  /** 記録できない状態 (選んだ案件がいまの一覧に無い等)。理由は notice に出す */
  recordBlocked?: boolean;
  /** 下書きをこのタブに残せていない (sessionStorage に書けない) */
  persistFailed?: boolean;
  /** 開いたらステージのボタンへフォーカスする (記録して次へで次の案件に移ったとき) */
  autoFocus?: boolean;
  onAnnounce?: ((text: string) => void) | undefined;
}) {
  const group = RESULT_GROUPS[team];
  const uid = useId();
  const formRef = useRef<HTMLFormElement | null>(null);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const focusStage = useRef(autoFocus);
  const [localNotice, setLocalNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [sending, setSending] = useState(false);
  const [modal, setModal] = useState<ModalState | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const clearCancelRef = useRef<HTMLButtonElement | null>(null);
  const clearOpenRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { if (confirmClear) clearCancelRef.current?.focus(); }, [confirmClear]);
  // 消すものが無くなったら (保存して下書きが消えた等) 確認を閉じる
  if (confirmClear && isEmptyDraft(draft)) setConfirmClear(false);
  function cancelClear() {
    setConfirmClear(false);
    requestAnimationFrame(() => clearOpenRef.current?.focus());
  }

  const writable = ctx?.writesEnabled === true;
  const reason = ctx === null ? null : ctx.schemaFailed ? RESULT_SCHEMA_FAILED : ctx.writesEnabled === false ? RESULT_WRITES_OFF : null;
  const checking = ctx !== null && ctx.writesEnabled === null && !ctx.schemaFailed;
  const fields = group.fields.map(spec => ({ spec, def: ctx?.defOf(spec.name) ?? null, label: ctx?.defOf(spec.name)?.label ?? ctx?.labelOf(spec.name) ?? null }));
  const editable = fields.filter((f): f is { spec: ResultFieldSpec; def: FieldDef; label: string } => f.def !== null);
  const ready = writable && current !== null;
  const canRecord = ready && !recordBlocked && !sending;
  const stages = ctx?.stages ?? [];
  const quick = quickStagesOf(group, stages);
  const currentStage = stages.find(s => s.id === ctx?.stageId);
  const chosen = draft.stageId !== '' && draft.stageId !== ctx?.stageId ? stages.find(s => s.id === draft.stageId) ?? null : null;
  const chosenNeedsInput = chosen !== null && ruleFieldNames(ctx?.ruleOf(chosen.id)).length > 0;

  // 通話が終わったら、入力欄を開いてステージのボタンにフォーカスする。1 つの通話につき 1 回だけ
  useEffect(() => {
    if (focusCallId === null) return;
    focusStage.current = true;
    onCollapsedChange(false);
  }, [focusCallId, onCollapsedChange]);
  useEffect(() => {
    if (!focusStage.current || collapsed || !ready) return;
    focusStage.current = false;
    if (!isTyping(document.activeElement)) stageRef.current?.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus();
    if (focusCallId !== null) onCallHandled(focusCallId);
  });

  const setValue = (name: string, v: string) => { setFieldErrors(e => Object.fromEntries(Object.entries(e).filter(([k]) => k !== name))); onChange({ ...draft, values: { ...draft.values, [name]: v } }); };
  const setStage = (id: string) => { setError(null); onChange({ ...draft, stageId: id === ctx?.stageId ? '' : id }); };

  function handle(out: SaveOutcome) {
    switch (out.kind) {
      case 'saved': case 'queued':
        setError(null); setFieldErrors({});
        onSaved(out.kind);
        return;
      case 'conflict':
        setError('HubSpot 側で先に変更されていました。どちらの値を使うか選んでから、もう一度「記録して次へ」を押してください。');
        return;
      case 'invalid': {
        setFieldErrors(out.errors);
        const labelOf = (n: string) => fields.find(f => f.spec.name === n)?.label ?? ctx?.labelOf(n) ?? 'ほかの項目';
        setError(out.missing.length > 0 ? `次の項目を入力してください: ${out.missing.map(labelOf).join('、')}` : '入力を確認してください。何も保存されていません。');
        return;
      }
      case 'error':
        setError(`保存できていません: ${out.message}`);
        return;
    }
  }

  async function submit() {
    setLocalNotice(null);
    if (ctx === null || !ready || recordBlocked || sending) {
      const why = notice ?? reason ?? (checking || current === null ? '入力欄を準備しています。少し待ってから押してください。' : null);
      if (why) onAnnounce?.(why);
      return;
    }
    const changes = resultChanges(editable, draft, current, today);
    const target: Target | null = chosen !== null && ctx.pipelineId !== null ? { pipeline_id: ctx.pipelineId, stage_id: chosen.id, label: chosen.label } : null;
    if (target !== null && chosenNeedsInput) {
      // 必須の項目があるステージ: ステージを移す画面で確かめてから、入力欄の変更と一緒に送る
      const presets: Record<string, string> = {};
      for (const { spec, def } of editable) {
        if (spec.role === 'memo') {
          const next = appendMemo(current[spec.name], draft.values[spec.name] ?? '', def.kind, today);
          if (next !== null) presets[spec.name] = next;
        } else presets[spec.name] = shownValue(spec, def, draft, current, today);
      }
      setModal({ target, changes, presets });
      return;
    }
    if (target === null && changes.length === 0) { onSaved('none'); return; }
    setSending(true); setError(null);
    const out = target !== null ? await ctx.move(target, changes) : await ctx.saveFields(changes);
    setSending(false);
    handle(out);
  }

  // Ctrl+Enter / Cmd+Enter で「記録して次へ」。入力欄の中で押したときだけ
  function onKeyDown(e: React.KeyboardEvent<HTMLFormElement>) {
    if (e.key !== 'Enter' || !(e.ctrlKey || e.metaKey) || modal !== null) return;
    e.preventDefault();
    if (collapsed) {
      onCollapsedChange(false);
      onAnnounce?.(COLLAPSED_NOTICE);
      setLocalNotice(COLLAPSED_NOTICE);
      return;
    }
    void submit();
  }

  const ended = endedCall !== null;
  const talk = endedCall?.talkSeconds ?? null;
  const shownNotice = notice ?? localNotice;
  const bodyId = `${uid}-body`;
  const summary = [chosen ? `ステージ: ${chosen.label}` : null, Object.keys(draft.values).length > 0 ? '入力あり' : null].filter(Boolean).join(' · ') || '未入力';

  function fieldRow({ spec, def, label }: { spec: ResultFieldSpec; def: FieldDef | null; label: string | null }) {
    const name = label ?? '項目';
    const cur = current?.[spec.name] ?? null;
    const err = fieldErrors[spec.name];
    if (!ready || def === null) {
      return <div key={spec.name} className={`rf-field${spec.role === 'memo' ? ' rf-wide' : ''}`} data-field={spec.name}>
        <span className="rf-label">{name}</span><span className="rf-read">{current === null ? '…' : readValue(def, cur)}</span>
      </div>;
    }
    if (spec.role === 'memo') {
      const add = draft.values[spec.name] ?? '';
      const multiline = def.kind === 'textarea';
      return <div key={spec.name} className="rf-field rf-wide rf-memo" data-field={spec.name}>
        <span className="rf-label">{name}</span>
        <div className="rf-memo-now" data-testid="memo-current" aria-label={`${name}(今の内容)`} tabIndex={0}>{cur !== null && cur !== '' ? cur : '(まだ何もありません)'}</div>
        <label className="rf-memo-add"><span className="rf-sub">追記する({memoDatePrefix(today)} を付けて末尾に足します)</span>
          {multiline
            ? <textarea rows={2} aria-label={`${name}に追記`} value={add} aria-invalid={err !== undefined} onChange={e => { setValue(spec.name, e.target.value); }} />
            : <input type="text" aria-label={`${name}に追記`} value={add} aria-invalid={err !== undefined} onChange={e => { setValue(spec.name, e.target.value); }} />}
        </label>
        <small className="rf-hint">{MEMO_EDIT_HINT}</small>
        {err !== undefined && <small className="rf-err" role="alert">{err}</small>}
      </div>;
    }
    const prev = spec.role === 'callDate' ? previousDateLabel(cur) : null;
    return <label key={spec.name} className={`rf-field${spec.primary ? '' : ' rf-minor'}`} data-field={spec.name}>
      {prev !== null
        ? <span className="rf-label-row"><span className="rf-label">{name}</span><small className="rf-prev" data-testid="call-date-prev">前回: {prev}</small></span>
        : <span className="rf-label">{name}</span>}
      <PropInput def={def} value={shownValue(spec, def, draft, current, today)} onChange={v => { setValue(spec.name, v); }} />
      {err !== undefined && <small className="rf-err" role="alert">{err}</small>}
    </label>;
  }

  const primary = fields.filter(f => f.spec.primary && f.spec.role !== 'memo');
  const memo = fields.filter(f => f.spec.role === 'memo');
  const minor = fields.filter(f => !f.spec.primary);

  // 1 行の入力欄で Enter を押したときのブラウザの自動送信では記録しない (記録は「記録して次へ」と Ctrl/⌘+Enter だけ)
  return <form ref={formRef} className={`rf${collapsed ? ' is-collapsed' : ''}`} aria-label="架電結果の入力" data-deal-id={dealId} data-team={team}
    onSubmit={e => { e.preventDefault(); }} onKeyDown={onKeyDown}>
    <div className="rf-head">
      <button type="button" className="rf-toggle" aria-expanded={!collapsed} aria-controls={bodyId}
        onClick={() => { setLocalNotice(null); onCollapsedChange(!collapsed); }}>
        <span aria-hidden="true">{collapsed ? '▲' : '▼'}</span> 架電結果</button>
      {ended && <span className="rf-call" data-testid="ended-call">通話終了{talk !== null ? ` 通話時間 ${clock(talk)}` : endedCall.result ? `(${ENDED_LABELS[endedCall.result] ?? 'つながらず'})` : ''}</span>}
      {collapsed && <span className="rf-summary" data-testid="draft-summary">{summary}</span>}
      <span className="rf-group" data-testid="result-group" title="入力する項目の組(所属チーム、無ければ取引のパイプラインで決まります)">{group.label}の項目</span>
      {sending && <span className="rf-status" role="status">保存中…</span>}
    </div>

    <div className="rf-body" id={bodyId} hidden={collapsed}>
      {persistFailed && <div className="cq-notice cq-warn rf-unsaved" role="status" data-testid="unsaved-alert">
        <p>入力途中の内容をこの画面に残せません。保存する前に閉じたり再読み込みしたりすると、入力が消えます。</p></div>}
      {reason !== null && <p className="cq-notice cq-warn rf-readonly" role="status" data-testid="result-readonly">{reason}</p>}
      {(ctx === null || checking || (writable && current === null)) && <p role="status" className="rf-muted">入力欄を準備しています…</p>}

      <div className="rf-stage">
        <div className="rf-stage-head"><span className="rf-stage-title">結果(ステージ)</span>
          <span className="rf-sub">今: {currentStage?.label ?? '(不明)'}</span></div>
        {ready && stages.length > 0 ? <>
          {quick.length > 0 && <div className="rf-stage-buttons" role="group" aria-label="ステージ(よく使うもの)" ref={stageRef}>
            {quick.map(s => <button key={s.id} type="button" className={`rf-stage-btn${s.id === ctx.stageId ? ' is-current' : ''}`}
              aria-pressed={draft.stageId === s.id} disabled={s.id === ctx.stageId}
              title={s.id === ctx.stageId ? '今のステージです' : ruleFieldNames(ctx.ruleOf(s.id)).length > 0 ? '移すときに入力する項目があります' : undefined}
              onClick={() => { setStage(s.id); }}>{s.label}{ruleFieldNames(ctx.ruleOf(s.id)).length > 0 && <span className="rf-need" aria-hidden="true">要入力</span>}</button>)}
          </div>}
          <label className="rf-stage-other"><span className="rf-sub">ほかのステージ</span>
            <select aria-label="ステージを選ぶ" value={draft.stageId} onChange={e => { setStage(e.target.value); }}>
              <option value="">(変えない)</option>
              {stages.filter(s => s.id !== ctx.stageId).map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
            </select></label>
          {chosenNeedsInput && <small className="rf-hint" data-testid="stage-needs-input">「{chosen.label}」に移すには必要な項目があります。記録するときに確かめます。</small>}
        </> : null}
      </div>

      <div className="rf-grid">{primary.map(fieldRow)}</div>
      {memo.map(fieldRow)}
      {minor.length > 0 && <div className="rf-minor-row">{minor.map(fieldRow)}</div>}
    </div>

    {/* 記録ボタンは入力欄のスクロールの外 (常に見える下端) に置く */}
    <div className="rf-actions" hidden={collapsed}>
      {error !== null
        ? <span className="rf-notice is-error" role="alert" data-testid="result-error">{error}</span>
        : <span className={`rf-notice${noticeTone === 'saved' ? ' is-saved' : noticeTone === 'queued' ? ' is-queued' : ''}`} role="status" data-testid="result-notice">{shownNotice ?? ''}</span>}
      {/* 「入力を消す」は 1 回押しただけでは消さず、その場で確かめる (隣の「記録して次へ」と押し間違えてメモを失わないように) */}
      {confirmClear
        ? <span className="rf-confirm" role="group" aria-label="入力を消すか確認"
          onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); cancelClear(); } }}>
          <span>入力した内容を消します(HubSpot の値は変わりません)。</span>
          <button type="button" className="cq-btn rf-confirm-yes" onClick={() => { setConfirmClear(false); setError(null); setFieldErrors({}); onClear(); }}>消す</button>
          <button type="button" className="cq-btn" ref={clearCancelRef} onClick={cancelClear}>やめる</button>
        </span>
        : <button type="button" className="cq-btn cq-btn-quiet" ref={clearOpenRef} disabled={isEmptyDraft(draft)}
          title={isEmptyDraft(draft) ? '消す入力はありません' : undefined} onClick={() => { setConfirmClear(true); }}>入力を消す</button>}
      <button type="button" className="rf-record" onClick={() => { void submit(); }} aria-disabled={!canRecord}
        title="HubSpot に保存して次の架電先へ進みます(入力欄の中で Ctrl+Enter / ⌘+Enter でも押せます)">
        記録して次へ <kbd>Ctrl+Enter</kbd></button>
    </div>
    {modal !== null && ctx !== null && <MoveModal ctx={ctx} target={modal.target} extraChanges={modal.changes} presets={modal.presets}
      onClose={() => { setModal(null); }} onDone={out => { handle(out); }} />}
  </form>;
}
