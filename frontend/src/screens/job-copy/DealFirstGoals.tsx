import { useCallback, useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import { useCrmWrite, WRITES_OFF_NOTE } from '../crm/useCrmWrite';
import { fieldKey } from '../crm/writeModel';
import type { PropValues } from '../crm/writeTypes';
import { contractWriteApi } from './dealContractApi';
import { SaveStatus } from './DealContract';
import { DELIVERY_PIPELINES } from './dealContractModel';
import {
  DEFAULT_BY_WHEN, GOAL_PROP_NAMES, KIND_LABEL, KIND_NOTE, confirmedData, emptyGoal, goalLine, goalProblems, goalWarnings,
  goalsRequest, readGoals, sourceLabel,
} from './firstGoalsModel';
import type { Goal, GoalData, GoalKind } from './firstGoalsModel';
import './deal-contract.css';
import './first-goals.css';

const KINDS: GoalKind[] = ['ideal', 'minimum', 'unclassified'];

function GoalList({ goals }: { goals: Goal[] }) {
  return <div className="fg-groups">{KINDS.map(kind => {
    const rows = goals.filter(goal => goal.kind === kind);
    if (kind === 'unclassified' && rows.length === 0) return null;
    return <div className={`fg-group fg-${kind}`} key={kind}>
      <h3>{KIND_LABEL[kind]}<small>{KIND_NOTE[kind]}</small></h3>
      {rows.length ? <ul>{rows.map((goal, index) => <li key={`${kind}-${String(index)}`}>{goalLine(goal)}</li>)}</ul> : <p>未入力</p>}
    </div>;
  })}</div>;
}

function DraftNote({ data }: { data: GoalData }) {
  const meeting = data.meeting;
  return <div className="fg-draft" role="note">
    <strong>MTGから読み取った案です。確かめて、直してから「確定して保存」を押してください。</strong>
    {meeting && <p>{[meeting.start_jst, meeting.topic].filter(Boolean).join(' ')}{meeting.sources?.length ? `（読んだもの: ${meeting.sources.map(sourceLabel).join('・')}）` : ''}</p>}
    {meeting?.notes?.length ? <ul>{meeting.notes.map(note => <li key={note}>{note}</li>)}</ul> : null}
  </div>;
}

/** 初回MTGで決めた 2 つの目標 (理想・前進ライン) を表示・入力・訂正する。保存先は HubSpot の取引。 */
export function DealFirstGoals({ dealId, onSaved }: { dealId: string; onSaved?: (() => void) | undefined }) {
  const [state, setState] = useState<{ values: PropValues; fetchedAt: string; pipeline: string; missing: boolean } | null>(null);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const [editing, setEditing] = useState(false);
  const [rows, setRows] = useState<Goal[]>([]);
  const [base, setBase] = useState<PropValues>({});
  const done = useCallback(() => { onSaved?.(); setRevision(value => value + 1); }, [onSaved]);
  const write = useCrmWrite({ dealId, api: contractWriteApi, onSaved: done });
  useEffect(() => {
    const ctl = new AbortController();
    void (async () => {
      const catalog = await apiGet<CrmPropertyCatalogResponse>('/api/crm/property-catalog', { signal: ctl.signal, timeoutMs: 35_000 });
      if (!catalog.ok || !Array.isArray(catalog.data.objects)) { if (!ctl.signal.aborted) setError('初回MTGの目標を取得できませんでした。再読み込みしてください。'); return; }
      const known = new Set(catalog.data.objects.find(object => object.object_type === 'deals')?.groups.flatMap(group => group.properties.map(prop => prop.name)) ?? []);
      const props = GOAL_PROP_NAMES.filter(name => known.has(name));
      if (props.length < GOAL_PROP_NAMES.length) { if (!ctl.signal.aborted) setState({ values: {}, fetchedAt: '', pipeline: '', missing: true }); return; }
      const query = new URLSearchParams({ deal_props: props.join(','), fresh: '1' });
      const detail = await apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?${query}`, { signal: ctl.signal, timeoutMs: 35_000 });
      if (ctl.signal.aborted) return;
      if (detail.ok && !detail.data.partial.some(part => part.part === 'selected_properties')) {
        setState({ values: { ...detail.data.selected.deal }, fetchedAt: detail.data.fetched_at, pipeline: detail.data.deal.pipeline_id ?? '', missing: false });
        setError('');
      } else setError('初回MTGの目標を取得できませんでした。再読み込みしてください。');
    })();
    return () => { ctl.abort(); };
  }, [dealId, revision]);

  const values: PropValues = { ...state?.values };
  for (const name of GOAL_PROP_NAMES) {
    const overlay = write.overlay[fieldKey(dealId, 'deal', name)];
    if (overlay && (overlay.pending || !overlay.fetchedAt || !state || state.fetchedAt <= overlay.fetchedAt)) values[name] = overlay.value;
  }
  const data = readGoals(values);
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const editable = schema && Array.isArray(schema.editable) ? schema.editable : [];
  const writable = GOAL_PROP_NAMES.every(name => editable.some(item => item.object === 'deal' && item.name === name));
  const targeted = state !== null && DELIVERY_PIPELINES.includes(state.pipeline);
  const canEdit = targeted && write.writesEnabled === true && writable && !error;
  const locked = Object.values(write.statuses).some(status => status.phase === 'saving' || status.phase === 'queued') || write.conflict !== null;
  const statuses = GOAL_PROP_NAMES.flatMap(name => { const status = write.statuses[fieldKey(dealId, 'deal', name)]; return status ? [status] : []; });

  const startEdit = () => {
    setBase(values);
    setRows(data?.goals.length ? data.goals.map(goal => ({ ...goal })) : [emptyGoal('ideal'), emptyGoal('minimum')]);
    setEditing(true);
  };
  const setRow = (index: number, patch: Partial<Goal>) => { setRows(prev => prev.map((row, i) => (i === index ? { ...row, ...patch } : row))); };
  const problems = goalProblems(rows);
  const warnings = goalWarnings(rows);
  const save = () => {
    const next = confirmedData(rows, readGoals(base), new Date());
    const request = { ...goalsRequest(dealId, base, next), fetchedAt: state?.fetchedAt };
    if (!request.changes.length) { setEditing(false); return; }
    void write.save(request).then(outcome => { if (outcome.kind === 'saved' || outcome.kind === 'queued') setEditing(false); });
  };

  return <section className="fg-root" aria-label="初回MTGの目標">
    <header><h2>初回MTGの目標</h2>
      {canEdit && !editing && <button className="jc-button" disabled={locked} onClick={startEdit}>{data ? data.status === 'draft' ? '確かめて確定' : '訂正' : '入力'}</button>}
    </header>
    {!state && !error && <p role="status">初回MTGの目標を読み込んでいます…</p>}
    {error && <p role="alert">{error} <button className="jc-button" onClick={() => { setRevision(value => value + 1); }}>再読み込み</button></p>}
    {state?.missing && <p>HubSpot に初回MTGの目標の項目がまだありません。</p>}
    {state && !state.missing && <>
      {!write.conflict && <SaveStatus statuses={statuses} />}
      {write.conflict && <div role="alert" className="dc-conflict"><h3>ほかの更新があります。現在の目標を確認してください。</h3><p>今回の変更は保存されていません。</p>
        <button className="jc-button" onClick={() => { write.resolveConflict('theirs'); setEditing(false); }}>現在の目標を使う</button>
        <button className="jc-button" onClick={() => { write.resolveConflict('mine'); setEditing(false); }}>確認して入力した目標で保存</button></div>}
      {write.writesEnabled === false ? <p>{WRITES_OFF_NOTE}</p> : write.schema.phase === 'error' ? <p>編集できるか確認できないため、表示のみです。</p>
        : !targeted ? <p>この取引は納品管理ではないため、表示のみです。</p> : schema && !writable ? <p>HubSpot でこの項目の編集が許可されていないため、表示のみです。</p> : null}
      {!editing && data?.status === 'draft' && <DraftNote data={data} />}
      {!editing && (data ? <GoalList goals={data.goals} /> : <p>まだ入力されていません。初回MTGで決めた理想と前進ラインを入力してください。</p>)}
      {editing && <div className="fg-edit">
        {data?.status === 'draft' && <DraftNote data={data} />}
        <p className="fg-help">1 行に 1 つ。何をするか（職種・応募/面接/採用など）は必ず書いてください。いつまでが空なら「{DEFAULT_BY_WHEN}」で保存します。</p>
        <div className="fg-rows">
          {rows.map((row, index) => <div className="fg-row" key={String(index)}>
            <label>区分<select disabled={locked} value={row.kind} onChange={event => { setRow(index, { kind: event.target.value as GoalKind }); }}>
              {KINDS.filter(kind => kind !== 'unclassified' || row.kind === 'unclassified').map(kind => <option key={kind} value={kind}>{KIND_LABEL[kind]}</option>)}
            </select></label>
            <label className="fg-what">何をするか<input disabled={locked} value={row.what} placeholder="例: 生コンドライバーを採用する" onChange={event => { setRow(index, { what: event.target.value }); }} /></label>
            <label>数<input disabled={locked} value={row.target} placeholder="例: 2名" onChange={event => { setRow(index, { target: event.target.value }); }} /></label>
            <label>いつまで<input disabled={locked} value={row.by_when} placeholder={DEFAULT_BY_WHEN} onChange={event => { setRow(index, { by_when: event.target.value }); }} /></label>
            <button className="jc-button" disabled={locked} aria-label={`${String(index + 1)}行目を消す`} onClick={() => { setRows(prev => prev.filter((_, i) => i !== index)); }}>消す</button>
          </div>)}
        </div>
        <div className="fg-add">
          <button className="jc-button" disabled={locked} onClick={() => { setRows(prev => [...prev, emptyGoal('ideal')]); }}>理想を足す</button>
          <button className="jc-button" disabled={locked} onClick={() => { setRows(prev => [...prev, emptyGoal('minimum')]); }}>前進ラインを足す</button>
        </div>
        {problems.length > 0 && <ul className="fg-problems" role="alert">{problems.map(problem => <li key={problem}>{problem}</li>)}</ul>}
        {problems.length === 0 && warnings.length > 0 && <ul className="fg-warnings">{warnings.map(warning => <li key={warning}>{warning}</li>)}</ul>}
        <div className="dc-actions">
          <button className="jc-button" disabled={locked || problems.length > 0} onClick={save}>確定して保存</button>
          <button className="jc-button" disabled={locked} onClick={() => { setEditing(false); }}>キャンセル</button>
        </div>
      </div>}
    </>}
  </section>;
}
