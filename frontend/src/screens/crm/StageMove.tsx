import { useEffect, useId, useState } from 'react';
import { PropInput, SaveStatus } from './WriteWidgets';
import { draftToValue, initialDraft, ruleFieldNames, sameValue } from './writeModel';
import type { FieldChange, FieldDef } from './writeModel';
import type { StageMoveCtx } from './writeBindings';

interface Target { pipeline_id: string; stage_id: string; label: string }

function MoveModal({ ctx, target, onClose }: { ctx: StageMoveCtx; target: Target; onClose: () => void }) {
  const titleId = useId();
  const rule = ctx.ruleOf(target.stage_id);
  const defs = ruleFieldNames(rule).map(n => ctx.defOf(n)).filter((d): d is FieldDef => d !== null);
  const required = new Set(rule?.required ?? []);
  const hasFields = defs.length > 0;
  const [current, setCurrent] = useState<Record<string, string | null> | null>(hasFields ? null : {});
  const [draft, setDraft] = useState<Record<string, string>>({});
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
  const missing = defs.filter(d => required.has(d.name) && draftToValue(d.kind, draft[d.name] ?? '') === null);
  const canMove = !loading && missing.length === 0 && !sending;

  async function submit() {
    if (current === null) return;
    const changes: FieldChange[] = defs.flatMap(d => {
      const base = current[d.name] ?? null;
      const value = draftToValue(d.kind, draft[d.name] ?? '');
      return sameValue(value, draftToValue(d.kind, initialDraft(d.kind, base))) ? [] : [{ object: d.object, name: d.name, base, value }];
    });
    setSending(true); setErrors({}); setMessage('');
    const out = await ctx.move(target, changes);
    setSending(false);
    if (out.kind === 'invalid') {
      setErrors(out.errors);
      const labels = out.missing.map(n => defs.find(d => d.name === n)?.label).filter((x): x is string => x !== undefined);
      setMessage(labels.length > 0 ? `次の項目を入力してください: ${labels.join('、')}` : '入力を確認してください。');
      return;
    }
    if (out.kind === 'error') setMessage(`保存できていません: ${out.message}`);
    else onClose();
  }

  return <div className="wr-overlay">
    <div className="wr-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} data-testid="stage-modal">
      <h3 id={titleId}>{hasFields ? 'このステージに移すには次の項目が必要です' : 'ステージを移します'}</h3>
      <p className="wr-target">移し先: <strong>{target.label}</strong></p>
      {hasFields ? (loading ? <p role="status">いまの値を読み込み中…</p>
        : <ul className="wr-fields">{defs.map(d => <li key={d.name}>
          <label>{d.label}{required.has(d.name) && <span className="wr-required"> (必須)</span>}
            <PropInput def={d} value={draft[d.name] ?? ''} onChange={v => { setDraft(p => ({ ...p, [d.name]: v })); }} /></label>
          {errors[d.name] !== undefined && <span className="wr-status wr-error" role="alert">{errors[d.name]}</span>}
        </li>)}</ul>)
        : <p>ステージを「{target.label}」に移します。よろしいですか。</p>}
      {message !== '' && <p className="wr-status wr-error" role="alert">{message}</p>}
      <div className="wr-dialog-actions">
        <button type="button" onClick={onClose} disabled={sending}>やめる</button>
        <button type="button" className="wr-primary" disabled={!canMove} onClick={() => { void submit(); }}>移す</button>
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
