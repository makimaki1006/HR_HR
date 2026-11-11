import { useCallback, useEffect, useRef, useState } from 'react';
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
  DEFAULT_BY_WHEN, GOAL_PROP_NAMES, KIND_LABEL, KIND_NOTE, confirmDraft, confirmedData, emptyGoal, goalLine, goalProblems, goalWarnings,
  goalNotes, goalsRequest, readGoals, sourceLabel, textEditedInHubSpot,
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
      {rows.length ? <ul>{rows.map((goal, index) => <li key={`${kind}-${String(index)}`}>{goalLine(goal)}{goalNotes(goal.notes).map((note, i) => <small className="fg-note" key={i}>{note}</small>)}</li>)}</ul> : <p>未入力</p>}
    </div>;
  })}</div>;
}

function DraftNote({ data }: { data: GoalData }) {
  const meeting = data.meeting;
  return <div className="fg-draft" role="note">
    <strong>MTGから読み取った案です。確かめて、そのままでよければ「この案で確定」、直すなら「確かめて確定」を押してください。</strong>
    {meeting && <p>{[meeting.start_jst, meeting.topic].filter(Boolean).join(' ')}{meeting.sources?.length ? `（読んだもの: ${meeting.sources.map(sourceLabel).join('・')}）` : ''}</p>}
    {meeting?.notes?.length ? <ul>{meeting.notes.map(note => <li key={note}>{note}</li>)}</ul> : null}
  </div>;
}

/** HubSpot の画面で直された値を取り込む間隔 (画面を開いたままのとき)。サーバの短いキャッシュ (60 秒) を足した分が最大の遅れ */
export const GOALS_REFRESH_MS = 5 * 60_000;

interface Loaded { values: PropValues; fetchedAt: string; pipeline: string; missing: boolean }

/** 初回MTGで決めた 2 つの目標 (マインド目標・コミット目標) を表示・入力・訂正する。保存先は HubSpot の取引。 */
export function DealFirstGoals({ dealId, onSaved, refreshMs = GOALS_REFRESH_MS }: { dealId: string; onSaved?: (() => void) | undefined; refreshMs?: number }) {
  const [state, setState] = useState<Loaded | null>(null);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const [editing, setEditing] = useState(false);
  const [opening, setOpening] = useState(false);
  const [rows, setRows] = useState<Goal[]>([]);
  const [base, setBase] = useState<PropValues>({});
  const done = useCallback(() => { onSaved?.(); setRevision(value => value + 1); }, [onSaved]);
  const write = useCrmWrite({ dealId, api: contractWriteApi, onSaved: done });
  const ctlRef = useRef<AbortController | null>(null);
  const propsRef = useRef<{ dealId: string; props: string[] } | null>(null);
  const editingRef = useRef(false);
  useEffect(() => { editingRef.current = editing || opening; }, [editing, opening]);

  const busyRef = useRef(false);
  const stateRef = useRef<Loaded | null>(null);
  useEffect(() => { stateRef.current = state; }, [state]);

  /** HubSpot の今の値を読む。`fresh` ならサーバのキャッシュを使わない。`quiet` なら失敗しても表示を変えない */
  const load = useCallback(async (fresh: boolean, quiet: boolean): Promise<Loaded | null> => {
    ctlRef.current?.abort();
    const ctl = new AbortController();
    ctlRef.current = ctl;
    busyRef.current = true;
    const fail = () => { if (!ctl.signal.aborted && !quiet) setError('初回MTGの目標を取得できませんでした。再読み込みしてください。'); return null; };
    try {
      let props = propsRef.current?.dealId === dealId ? propsRef.current.props : null;
      if (!props) {
        const catalog = await apiGet<CrmPropertyCatalogResponse>('/api/crm/property-catalog', { signal: ctl.signal, timeoutMs: 35_000 });
        if (!catalog.ok || !Array.isArray(catalog.data.objects)) return fail();
        const known = new Set(catalog.data.objects.find(object => object.object_type === 'deals')?.groups.flatMap(group => group.properties.map(prop => prop.name)) ?? []);
        props = GOAL_PROP_NAMES.filter(name => known.has(name));
        if (props.length < GOAL_PROP_NAMES.length) {
          if (ctl.signal.aborted) return null;
          const missing = { values: {}, fetchedAt: '', pipeline: '', missing: true };
          setState(missing);
          return missing;
        }
        propsRef.current = { dealId, props };
      }
      const query = new URLSearchParams({ deal_props: props.join(','), ...(fresh ? { fresh: '1' } : {}) });
      const detail = await apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?${query}`, { signal: ctl.signal, timeoutMs: 35_000 });
      if (ctl.signal.aborted) return null;
      if (!detail.ok || detail.data.partial.some(part => part.part === 'selected_properties')) return fail();
      const loaded = { values: { ...detail.data.selected.deal }, fetchedAt: detail.data.fetched_at, pipeline: detail.data.deal.pipeline_id ?? '', missing: false };
      setState(loaded);
      setError('');
      return loaded;
    } catch {
      return fail();
    } finally {
      if (ctlRef.current === ctl) busyRef.current = false;
    }
  }, [dealId]);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- 取引を開いたとき・保存の後に HubSpot から読み直す（取得後に状態を入れる）
    void load(true, false);
    return () => { ctlRef.current?.abort(); };
  }, [load, revision]);

  // HubSpot の画面で直された値を取り込む: 画面に戻ったときはすぐ読み直し、開いたままなら一定の間隔で読む。編集中は読まない
  useEffect(() => {
    const visible = () => document.visibilityState === 'visible';
    // 読み込み中は重ねて読まない (進行中の読み込みを止めない)。まだ一度も読めていなければ、失敗も表示する通常の読み込みにする
    const refresh = (fresh: boolean) => {
      if (!visible() || editingRef.current || busyRef.current) return;
      void load(fresh, stateRef.current !== null);
    };
    const onVisible = () => { refresh(true); };
    const timer = setInterval(() => { refresh(false); }, refreshMs);
    document.addEventListener('visibilitychange', onVisible);
    return () => { clearInterval(timer); document.removeEventListener('visibilitychange', onVisible); };
  }, [load, refreshMs]);

  const values: PropValues = { ...state?.values };
  for (const name of GOAL_PROP_NAMES) {
    const overlay = write.overlay[fieldKey(dealId, 'deal', name)];
    if (overlay && (overlay.pending || !overlay.fetchedAt || !state || state.fetchedAt <= overlay.fetchedAt)) values[name] = overlay.value;
  }
  const data = readGoals(values);
  const edited = textEditedInHubSpot(values);
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const editable = schema && Array.isArray(schema.editable) ? schema.editable : [];
  const writable = GOAL_PROP_NAMES.every(name => editable.some(item => item.object === 'deal' && item.name === name));
  const targeted = state !== null && DELIVERY_PIPELINES.includes(state.pipeline);
  const canEdit = targeted && write.writesEnabled === true && writable && !error;
  const locked = opening || Object.values(write.statuses).some(status => status.phase === 'saving' || status.phase === 'queued') || write.conflict !== null;
  const statuses = GOAL_PROP_NAMES.flatMap(name => { const status = write.statuses[fieldKey(dealId, 'deal', name)]; return status ? [status] : []; });
  const conflictGoals = write.conflict ? readGoals({ ...values, ...write.conflict.current }) : null;

  // 編集を始める前に HubSpot の今の値を読み直し、その値を「編集を始めたときの値」(base) にする
  const startEdit = () => {
    setOpening(true);
    void (async () => {
      try {
        const loaded = await load(true, false);
        if (!loaded || loaded.missing) return;
        const current = readGoals(loaded.values);
        setBase(loaded.values);
        setRows(current?.goals.length ? current.goals.map(goal => ({ ...goal })) : [emptyGoal('ideal'), emptyGoal('minimum')]);
        setEditing(true);
      } catch {
        setError('初回MTGの目標を取得できませんでした。再読み込みしてください。');
      } finally {
        setOpening(false);
      }
    })();
  };
  const setRow = (index: number, patch: Partial<Goal>) => { setRows(prev => prev.map((row, i) => (i === index ? { ...row, ...patch } : row))); };
  const problems = goalProblems(rows);
  const warnings = goalWarnings(rows);
  const send = (from: PropValues, next: GoalData) => {
    const request = { ...goalsRequest(dealId, from, next), fetchedAt: state?.fetchedAt };
    if (!request.changes.length) { setEditing(false); return; }
    void write.save(request).then(outcome => { if (outcome.kind === 'saved' || outcome.kind === 'queued') setEditing(false); });
  };
  const save = () => { send(base, confirmedData(rows, readGoals(base), new Date())); };
  // 案を直さずにそのまま確定する。画面に出している値を base にする (HubSpot で変わっていれば保存しない)
  const confirmAsIs = () => { if (data) send(values, confirmDraft(data, new Date())); };
  const draftReady = data?.status === 'draft' && goalProblems(data.goals).length === 0;

  return <section className="fg-root" aria-label="初回MTGの目標">
    <header><h2>初回MTGの目標</h2>
      {canEdit && !editing && draftReady && <button className="jc-button" disabled={locked} onClick={confirmAsIs}>この案で確定</button>}
      {canEdit && !editing && <button className="jc-button" disabled={locked} onClick={startEdit}>{data ? data.status === 'draft' ? '確かめて確定' : '訂正' : '入力'}</button>}
    </header>
    {opening && <p role="status">HubSpot の今の目標を読み込んでいます…</p>}
    {!state && !error && <p role="status">初回MTGの目標を読み込んでいます…</p>}
    {error && <p role="alert">{error} <button className="jc-button" disabled={opening} onClick={() => { setRevision(value => value + 1); }}>再読み込み</button></p>}
    {state?.missing && <p>HubSpot に初回MTGの目標の項目がまだありません。</p>}
    {state && !state.missing && <>
      {!write.conflict && <SaveStatus statuses={statuses} />}
      {write.conflict && <div role="alert" className="dc-conflict"><h3>編集を始めたあとに、HubSpot で目標が変わっています。</h3><p>今回の変更は保存されていません。HubSpot の今の目標は次のとおりです。</p>
        {conflictGoals ? <GoalList goals={conflictGoals.goals} /> : <p>HubSpot の今の目標は空です。</p>}
        <button className="jc-button" onClick={() => { write.resolveConflict('theirs'); setEditing(false); }}>HubSpot の今の目標を使う</button>
        <button className="jc-button" onClick={() => { write.resolveConflict('mine'); setEditing(false); }}>今の目標を上書きして、入力した目標で保存</button></div>}
      {write.writesEnabled === false ? <p>{WRITES_OFF_NOTE}</p> : write.schema.phase === 'error' ? <p>編集できるか確認できないため、表示のみです。</p>
        : !targeted ? <p>この取引は納品管理ではないため、表示のみです。</p> : schema && !writable ? <p>HubSpot でこの項目の編集が許可されていないため、表示のみです。</p> : null}
      {!editing && data?.status === 'draft' && <DraftNote data={data} />}
      {!editing && edited && <p className="fg-edited" role="note">HubSpot で目標の文が直されています。直された文を表示しています（確定済みとして扱います）。</p>}
      {!editing && (data ? <><GoalList goals={data.goals} />{goalNotes(data.notes).map((note, i) => <small className="fg-note" key={i}>{note}</small>)}</> : <p>まだ入力されていません。初回MTGで決めたマインド目標とコミット目標を入力してください。</p>)}
      {editing && <div className="fg-edit">
        {goalNotes(data?.notes).map((note, i) => <small className="fg-note" key={i}>{note}</small>)}
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
            {goalNotes(row.notes).map((note, i) => <small className="fg-note" key={i}>{note}</small>)}
          </div>)}
        </div>
        <div className="fg-add">
          <button className="jc-button" disabled={locked} onClick={() => { setRows(prev => [...prev, emptyGoal('ideal')]); }}>マインド目標を足す</button>
          <button className="jc-button" disabled={locked} onClick={() => { setRows(prev => [...prev, emptyGoal('minimum')]); }}>コミット目標を足す</button>
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
