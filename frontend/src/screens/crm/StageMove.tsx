import { useEffect, useId, useRef, useState } from 'react';
import { PropInput, SaveStatus } from './WriteWidgets';
import { APPOINTER_FIELDS, draftToValue, initialDraft, ruleFieldNames, sameValue } from './writeModel';
import type { FieldChange, FieldDef } from './writeModel';
import type { StageMoveCtx } from './writeBindings';

interface Target { pipeline_id: string; stage_id: string; label: string }

/** 項目の見出しがまとめて決まらないとき (項目の一覧が読めていない・どのグループにも無い) の見出し */
const OTHER_SECTION = 'その他の項目';

interface Section { title: string; defs: FieldDef[] }

/** 項目を HubSpot のプロパティのグループごとに分ける (並びは初めて出た順。ステージの規則の並びを保つ) */
export function sectionsOf(defs: readonly FieldDef[], groupOf: (name: string) => string | null): Section[] {
  const out: Section[] = [];
  for (const d of defs) {
    const title = groupOf(d.name) ?? OTHER_SECTION;
    const found = out.find(x => x.title === title);
    if (found) found.defs.push(d); else out.push({ title, defs: [d] });
  }
  return out;
}

function MoveModal({ ctx, target, onClose }: { ctx: StageMoveCtx; target: Target; onClose: () => void }) {
  const titleId = useId();
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const rule = ctx.ruleOf(target.stage_id);
  const defs = ruleFieldNames(rule).map(n => ctx.defOf(n)).filter((d): d is FieldDef => d !== null);
  const required = new Set(rule?.required ?? []);
  const hasFields = defs.length > 0;
  const [current, setCurrent] = useState<Record<string, string | null> | null>(hasFields ? null : {});
  const [draft, setDraft] = useState<Record<string, string>>({});
  // 利用者が手を入れた項目 (「アポ獲得者」の初期値を、空にした後でもう一度入れ直さない)
  const [touched, setTouched] = useState<ReadonlySet<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [message, setMessage] = useState('');
  const [sending, setSending] = useState(false);
  const defsKey = defs.map(d => d.name).join(',');

  useEffect(() => {
    if (!hasFields) return;
    let live = true;
    void ctx.currentValues(defs).then(v => {
      if (!live) return;
      setCurrent(v);
      setDraft(Object.fromEntries(defs.map(d => [d.name, initialDraft(d.kind, v[d.name])])));
    }, () => { if (live) { setCurrent({}); setMessage('いまの値を読み込めませんでした。空欄から入力してください。'); setDraft(Object.fromEntries(defs.map(d => [d.name, '']))); } });
    return () => { live = false; };
  // defs は defsKey で表している
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defsKey, hasFields]);

  const loading = current === null;
  /** 画面に見せる値。アポ獲得者・アポ取得者が空で、手を入れていなければ、操作している本人を入れておく */
  const valueOf = (d: FieldDef): string => {
    const v = draft[d.name] ?? '';
    return v === '' && d.kind === 'owner' && APPOINTER_FIELDS.includes(d.name) && !touched.has(d.name) && ctx.meOwnerId !== null ? ctx.meOwnerId : v;
  };
  const isMissing = (d: FieldDef): boolean => required.has(d.name) && draftToValue(d.kind, valueOf(d)) === null;
  const missing = defs.filter(isMissing);
  const canMove = !loading && missing.length === 0 && !sending;
  const sections = sectionsOf(defs, n => ctx.groupOf(n));
  const wide = defs.length > 4;

  /** 最初の未入力の必須項目へ移る (入力欄にフォーカスして見える位置へ) */
  function jumpToMissing() {
    const first = missing[0];
    if (!first) return;
    const row = bodyRef.current?.querySelector<HTMLElement>(`[data-field="${first.name}"]`);
    if (!row) return;
    row.scrollIntoView({ block: 'center' });
    row.querySelector<HTMLElement>('input, textarea, select, button')?.focus();
  }

  async function submit() {
    if (current === null) return;
    const changes: FieldChange[] = defs.flatMap(d => {
      const base = current[d.name] ?? null;
      const value = draftToValue(d.kind, valueOf(d));
      return sameValue(value, draftToValue(d.kind, initialDraft(d.kind, base))) ? [] : [{ object: d.object, name: d.name, base, value }];
    });
    setSending(true); setErrors({}); setMessage('');
    const out = await ctx.move(target, changes);
    setSending(false);
    if (out.kind === 'invalid') {
      setErrors(out.errors);
      const onScreen = out.missing.filter(n => defs.some(d => d.name === n));
      const offScreen = out.missing.filter(n => !defs.some(d => d.name === n));
      const labels = onScreen.map(n => defs.find(d => d.name === n)?.label).filter((x): x is string => x !== undefined);
      const parts: string[] = [];
      if (labels.length > 0) parts.push(`次の項目を入力してください: ${labels.join('、')}`);
      // この画面に出せない項目は、内部名ではなく表示名で知らせる
      if (offScreen.length > 0) parts.push(`HubSpot 側で必須の項目のうち、この画面で入力できないものがあります(${offScreen.map(n => ctx.labelOf(n) ?? 'ほかの項目').join('、')})`);
      setMessage(parts.length > 0 ? parts.join('。') : '入力を確認してください。');
      return;
    }
    if (out.kind === 'error') setMessage(`保存できていません: ${out.message}`);
    else onClose();
  }

  const field = (d: FieldDef) => {
    const miss = !loading && isMissing(d);
    return <li key={d.name} data-field={d.name} className={`wr-field${miss ? ' is-missing' : ''}${d.kind === 'textarea' || d.kind === 'multi' ? ' is-full' : ''}`}>
      <label><span className="wr-label-text">{d.label}{required.has(d.name) && <span className="wr-required"> (必須)</span>}</span>
        <PropInput def={d} value={valueOf(d)} onChange={v => { setTouched(p => new Set(p).add(d.name)); setDraft(p => ({ ...p, [d.name]: v })); }} /></label>
      {errors[d.name] !== undefined && <span className="wr-status wr-error" role="alert">{errors[d.name]}</span>}
    </li>;
  };

  return <div className="wr-overlay">
    <div className={`wr-dialog wr-dialog-stage${wide ? ' is-wide' : ''}`} role="dialog" aria-modal="true" aria-labelledby={titleId} data-testid="stage-modal">
      <div className="wr-dialog-head">
        <h3 id={titleId}>{hasFields ? 'このステージに移すには次の項目が必要です' : 'ステージを移します'}</h3>
        <p className="wr-target">移し先: <strong>{target.label}</strong></p>
        {hasFields && !loading && required.size > 0 && (missing.length > 0
          ? <button type="button" className="wr-remaining" data-testid="stage-remaining" onClick={jumpToMissing}>必須 あと {missing.length} 項目<span className="wr-remaining-go"> 最初の未入力へ移る</span></button>
          : <p className="wr-remaining is-done" role="status" data-testid="stage-remaining">必須項目はすべて入力済みです</p>)}
      </div>
      <div className="wr-dialog-body" ref={bodyRef}>
        {hasFields ? (loading ? <p role="status">いまの値を読み込み中…</p>
          : sections.map(sec => <section key={sec.title} className="wr-section">
            {sections.length > 1 && <h4 className="wr-section-title">{sec.title}</h4>}
            <ul className="wr-fields">{sec.defs.map(field)}</ul>
          </section>))
          : <p>ステージを「{target.label}」に移します。よろしいですか。</p>}
        {message !== '' && <p className="wr-status wr-error" role="alert">{message}</p>}
      </div>
      <div className="wr-dialog-actions">
        <button type="button" onClick={onClose} disabled={sending}>やめる</button>
        <button type="button" className="wr-primary" disabled={loading || sending} aria-disabled={!canMove}
          title={missing.length > 0 ? '必須の項目が未入力です。押すと最初の未入力へ移ります' : undefined}
          onClick={() => { if (missing.length > 0) jumpToMissing(); else void submit(); }}>移す</button>
      </div>
    </div>
  </div>;
}

function PipelineModal({ ctx, onNext, onClose }: { ctx: StageMoveCtx; onNext: (t: Target) => void; onClose: () => void }) {
  const titleId = useId();
  const [pid, setPid] = useState(ctx.pipelines[0]?.id ?? '');
  const stages = ctx.pipelines.find(p => p.id === pid)?.stages ?? [];
  const [sid, setSid] = useState('');
  const stage = stages.find(s => s.id === sid);
  return <div className="wr-overlay">
    <div className="wr-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} data-testid="pipeline-modal">
      <h3 id={titleId}>別のパイプラインへ移す</h3>
      <label>パイプライン
        <select value={pid} onChange={e => { setPid(e.target.value); setSid(''); }}>
          {ctx.pipelines.map(p => <option key={p.id} value={p.id}>{p.label}</option>)}</select></label>
      <label>ステージ
        <select value={sid} onChange={e => { setSid(e.target.value); }}>
          <option value="">(選んでください)</option>
          {stages.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select></label>
      <div className="wr-dialog-actions">
        <button type="button" onClick={onClose}>やめる</button>
        <button type="button" className="wr-primary" disabled={stage === undefined} onClick={() => { if (stage) onNext({ pipeline_id: pid, stage_id: stage.id, label: stage.label }); }}>次へ</button>
      </div>
    </div>
  </div>;
}

/** 案件の概要のステージ: 編集できるときはプルダウン、できないときは今までどおりの表示 */
export function StageMover({ ctx, fallback }: { ctx: StageMoveCtx | null | undefined; fallback: React.ReactNode }) {
  const [flow, setFlow] = useState<{ step: 'pipeline' } | { step: 'move'; target: Target } | null>(null);
  if (ctx?.writesEnabled !== true || ctx.stages.length === 0) return <>{fallback}</>;
  const known = ctx.stages.some(s => s.id === ctx.stageId);
  function pick(c: StageMoveCtx, stageId: string) {
    const s = c.stages.find(x => x.id === stageId);
    if (!s || c.pipelineId === null || stageId === c.stageId) return;
    setFlow({ step: 'move', target: { pipeline_id: c.pipelineId, stage_id: s.id, label: s.label } });
  }
  return <span className="wr-stage">
    <select className="wr-stage-select cq-stage" aria-label="ステージを変更" value={ctx.stageId ?? ''} disabled={ctx.status?.phase === 'saving'}
      onChange={e => { pick(ctx, e.target.value); }}>
      {!known && <option value={ctx.stageId ?? ''}>(現在のステージ)</option>}
      {ctx.stages.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
    </select>
    {ctx.pipelines.length > 1 && <button type="button" className="wr-link" onClick={() => { setFlow({ step: 'pipeline' }); }}>別のパイプラインへ移す</button>}
    <SaveStatus status={ctx.status} onRetry={ctx.status?.phase === 'error' && ctx.status.req ? (() => { if (ctx.status?.phase === 'error' && ctx.status.req) ctx.retry(ctx.status.req); }) : undefined} />
    {flow?.step === 'pipeline' && <PipelineModal ctx={ctx} onNext={t => { setFlow({ step: 'move', target: t }); }} onClose={() => { setFlow(null); }} />}
    {flow?.step === 'move' && <MoveModal ctx={ctx} target={flow.target} onClose={() => { setFlow(null); }} />}
  </span>;
}
