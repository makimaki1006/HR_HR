import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import { apiPatch } from '../../api/client';
import { classifyPatch, liveWriteApi } from './crmWrite';
import type { WriteApi } from './crmWrite';
import type { PatchRequest, PatchResponse } from './writeTypes';
import { useCrmWrite } from './useCrmWrite';
import { PropInput, SaveStatus } from './WriteWidgets';
import { fieldKey } from './writeModel';
import { useCurrentUser } from './useCurrentUser';
import type { UserFetch } from './useCurrentUser';
import { APPLICANT_DRAFT_KEY, APPLICANT_MEMO, APPLICANT_WRITES_OFF, applicantChanges, applicantField, applicantInput } from './applicantResultModel';
import { parseStore } from './callResultModel';
import type { ResultDraft } from './callResultModel';

const fixtureUser: UserFetch = () => Promise.resolve({ ok: true, data: { user_email: 'fixture@example.invalid' } });
const quickLabels = ['朝連絡', '昼連絡', '夕連絡', '約束あり連絡', '選考書類待ち', '足切り猶予期間', '一次対応実施済'];
const empty = (): ResultDraft => ({ stageId: '', values: {} });

export function ApplicantEditor({ detail, fixture, onNext, contactTarget, hearingTarget }: { detail: ApplicantDetail | null; fixture: boolean; onNext: (id: string) => void; contactTarget?: HTMLElement | null; hearingTarget?: HTMLElement | null }) {
  const detailMap = useRef(new Map<string, ApplicantDetail>());
  useEffect(() => { if (detail) detailMap.current.set(detail.row.id, detail); }, [detail]);
  const user = useCurrentUser(fixture ? fixtureUser : undefined);
  const [notice, setNotice] = useState<{ id: string; name: string; label: string } | null>(null);
  const api = useMemo<WriteApi>(() => ({
    editSchema: id => {
      const d = detailMap.current.get(id);
      return Promise.resolve({ ok: true, data: { deal_id: id, pipeline_id: d?.write_pipeline?.id ?? null, stage_id: d?.row.stage_id ?? null, editable: [], stages: [], pipelines: [], writes_enabled: d?.writes_enabled ?? false } });
    },
    operation: id => liveWriteApi.operation(id),
    patchDeal: async (id, body) => {
      const d = detailMap.current.get(id);
      const set = { ...body.set };
      const stage = set.hs_pipeline_stage;
      delete set.hs_pipeline_stage;
      const request: PatchRequest = { ...body, set, base: { ...body.base, hs_pipeline: d?.values.hs_pipeline ?? null }, ...(stage ? { stage: { pipeline_id: d?.write_pipeline?.id ?? '', stage_id: stage } } : {}) };
      if (fixture) return { kind: 'saved', response: { status: 'saved', values: body.set, objects_values: {}, fetched_at: new Date().toISOString() } };
      const result = classifyPatch(await apiPatch<PatchResponse>(`/api/crm/applicants/${encodeURIComponent(id)}`, request, { timeoutMs: 35_000 }));
      if (result.kind === 'conflict') return { ...result, body: { ...result.body, object: 'deal' } };
      return result;
    },
  }), [fixture]);
  const write = useCrmWrite({ dealId: detail?.row.id ?? null, api, onSaved: () => undefined });
  const status = notice ? write.statuses[fieldKey(notice.id, 'deal', notice.name)] : undefined;
  return <section className={`crm-card applicant-editor${detail?.writes_enabled ? ' applicant-editor-editing' : ''}`} aria-label="応募の記録">
    {notice && <p>{notice.label}<SaveStatus status={status} /></p>}
    {detail && (detail.writes_enabled && write.writesEnabled !== false && detail.write_pipeline
      ? user.phase === 'loading' ? <p role="status">下書きを確認中…</p> : <ApplicantForm key={`${fixture ? 'fixture' : 'live'}:${detail.row.id}:${user.phase === 'ready' ? user.email : ''}`} contactTarget={contactTarget} hearingTarget={hearingTarget} detail={detail} user={user.phase === 'ready' ? user.email : null} fixture={fixture}
        save={async draft => {
          const changes = applicantChanges(detail, draft.values, draft.stageId);
          if (!changes.length) { onNext(detail.row.id); return true; }
          const first = changes[0];
          if (first) setNotice({ id: detail.row.id, name: first.name, label: detail.row.name });
          const result = await write.save({ dealId: detail.row.id, changes });
          if (result.kind === 'saved' || result.kind === 'queued') { onNext(detail.row.id); return true; }
          return false;
        }} />
      : <p>{APPLICANT_WRITES_OFF}</p>)}
    {write.conflict && <div role="alert" className="wr-error"><p>HubSpot 側で先に変更されています。保存せず、最新の内容を確認してください。</p>
      <dl>{Object.entries(write.conflict.current).map(([name, value]) => <div key={name}><dt>{detail?.editable.find(f => f.name === name)?.label ?? (name === 'hs_pipeline_stage' ? 'ステージ' : 'パイプライン')}</dt><dd>{name === 'hs_pipeline_stage' ? detail?.write_pipeline?.stages.find(s => s.id === value)?.label ?? '変更済み' : name === 'hs_pipeline' ? '変更済み' : detail?.editable.find(f => f.name === name)?.options.find(o => o.value === value)?.label ?? value ?? '未入力'}</dd></div>)}</dl>
      <button onClick={() => { write.resolveConflict('theirs'); window.location.reload(); }}>最新の内容を読み直す</button></div>}
  </section>;
}
function ApplicantForm({ detail, user, fixture, save, contactTarget, hearingTarget }: { detail: ApplicantDetail; user: string | null; fixture: boolean; contactTarget?: HTMLElement | null | undefined; hearingTarget?: HTMLElement | null | undefined; save: (draft: ResultDraft) => Promise<boolean> }) {
  const key = `${fixture ? 'fixture' : 'live'}:${detail.row.id}`;
  const [draft, setDraft] = useState<ResultDraft>(() => {
    try { return user ? parseStore(sessionStorage.getItem(APPLICANT_DRAFT_KEY), user).drafts[key] ?? empty() : empty(); } catch { return empty(); }
  });
  const [saving, setSaving] = useState(false);
  const busy = useRef(false);
  const formRef = useRef<HTMLFormElement>(null);
  useEffect(() => { formRef.current?.querySelector<HTMLButtonElement>('.applicant-quick-stages button')?.focus({ preventScroll: true }); }, []);
  const [error, setError] = useState('');
  function store(next: ResultDraft | null) {
    if (!user) return;
    try {
      const state = parseStore(sessionStorage.getItem(APPLICANT_DRAFT_KEY), user);
      if (next) state.drafts[key] = next; else state.drafts = Object.fromEntries(Object.entries(state.drafts).filter(([k]) => k !== key));
      sessionStorage.setItem(APPLICANT_DRAFT_KEY, JSON.stringify({ ...state, user }));
    } catch { /* sessionStorage が使えないときも編集は続ける */ }
  }
  function change(next: ResultDraft) { setDraft(next); store(next); }
  async function submit() {
    if (busy.current) return;
    busy.current = true; setSaving(true); setError('');
    try { if (await save(draft)) { store(null); } }
    catch { setError('入力した日時と通信状態を確認してください。保存を完了できませんでした。'); }
    finally { busy.current = false; setSaving(false); }
  }
  return <form ref={formRef} onSubmit={e => { e.preventDefault(); void submit(); }} onKeyDown={e => { if (e.key === 'Enter' && !e.ctrlKey && e.target instanceof HTMLInputElement && !e.nativeEvent.isComposing) { e.preventDefault(); return; } if (e.key === 'Enter' && e.ctrlKey && !e.nativeEvent.isComposing) { e.preventDefault(); void submit(); } }}>
    <fieldset disabled={saving} className="applicant-result-header">
      <legend>結果を記録</legend>
      <div className="applicant-quick-stages">{detail.write_pipeline?.stages.filter(s => quickLabels.includes(s.label)).map(s => <button type="button" key={s.id} aria-pressed={draft.stageId === s.id} onClick={() => { change({ ...draft, stageId: s.id }); }}>{s.label}</button>)}</div>
      <label>ステージ<select aria-label="ステージ" value={draft.stageId || detail.row.stage_id} onChange={e => { change({ ...draft, stageId: e.target.value }); }}>{detail.write_pipeline?.stages.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}</select></label>
    </fieldset>
    <fieldset disabled={saving} className="applicant-result-fields">
      {['結果', 'ヒアリング', '連絡先'].map(section => {
        const target = section === 'ヒアリング' ? hearingTarget : section === '連絡先' ? contactTarget : null;
        const fields = <details key={section} open={section === '結果'}><summary>{section === '結果' ? '対応結果' : `${section}の編集`}</summary>
        {detail.editable.filter(f => f.section === section).map(f => {
          const def = applicantField(f); if (!def) return null;
          const isMemo = f.name === APPLICANT_MEMO;
          const value = draft.values[f.name] ?? (isMemo ? '' : applicantInput(f, detail.values[f.name]));
          const onChange = (v: string) => { change({ ...draft, values: { ...draft.values, [f.name]: v } }); };
          return <div className="applicant-edit-field" key={f.name}><span>{f.label}</span>
            {isMemo && detail.values[f.name] && <p className="applicant-existing-memo">{detail.values[f.name]}</p>}
            {f.property_type === 'datetime' ? <input type="datetime-local" aria-label={f.label} value={value} onChange={e => { onChange(e.target.value); }} />
              : <PropInput def={isMemo ? { ...def, kind: 'textarea' } : def} value={value} onChange={onChange} />}</div>;
        })}
      </details>;
        return target ? createPortal(<fieldset disabled={saving}>{fields}</fieldset>, target, section) : fields;
      })}
    </fieldset>
    <div className="applicant-save-actions"><button type="submit" disabled={saving}>{saving ? '保存中…' : '記録して次へ'}</button><small>Ctrl+Enter</small></div>
    {error && <p className="wr-error" role="alert">{error}</p>}
  </form>;
}
