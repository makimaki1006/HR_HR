import { useCallback, useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { useCrmWrite, WRITES_OFF_NOTE } from '../crm/useCrmWrite';
import type { FieldStatus } from '../crm/useCrmWrite';
import { DealStageMove } from './DealStageMove';
import { fieldKey, stageKey } from '../crm/writeModel';
import type { PropValues } from '../crm/writeTypes';
import { contractWriteApi } from './dealContractApi';
import { CONTRACT_FIELDS, DISPLAY_FIELDS, DELIVERY_PIPELINES, contractRequest, displayValue, inputValue } from './dealContractModel';
import './deal-contract.css';

export function SaveStatus({ statuses }: { statuses: FieldStatus[] }) {
  const status = statuses.find(item => item.phase === 'error' || item.phase === 'invalid')
    ?? statuses.find(item => item.phase === 'saving')
    ?? statuses.find(item => item.phase === 'queued')
    ?? statuses[0];
  if (!status) return null;
  const tone = status.phase === 'saved' ? 'green' : status.phase === 'queued' || status.phase === 'saving' ? 'yellow' : 'red';
  const messages = tone === 'red' ? [...new Set(statuses.flatMap(item => item.phase === 'error' || item.phase === 'invalid' ? [item.message] : []))]
    : [status.phase === 'saved' ? '保存済み' : status.phase === 'queued' ? status.slow ? 'まだHubSpotへの反映を待っています' : '一時保存済み・HubSpotへの反映待ち' : '保存しています…'];
  return <div role={tone === 'red' ? 'alert' : 'status'} className={`dc-status dc-${tone}`}>{messages.map(message => <span key={message}>{message}</span>)}</div>;
}

/** 案件の集計とは別に、その取引自身の HubSpot の契約・成果を表示する。 */
export function DealContract({ dealId, onSaved, onQueued }: { dealId: string; onSaved: () => void; onQueued?: (() => void) | undefined }) {
  const [data, setData] = useState<{ props: CrmCatalogProperty[]; detail: WorkspaceResponse } | null>(null);
  const [error, setError] = useState('');
  const [refreshError, setRefreshError] = useState('');
  const [revision, setRevision] = useState(0);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [base, setBase] = useState<PropValues>({});
  const [savedFields, setSavedFields] = useState<string[]>([]);
  const stageSaving = useRef(false);
  const refresh = useCallback(() => {
    void (async () => {
      if (stageSaving.current) {
        const result = await apiGet(`/api/job-copy/deals/${encodeURIComponent(dealId)}/refresh`, { timeoutMs: 35_000 });
        setRefreshError(result.ok ? '' : '保存済みですが、案件とホームの表示を更新できませんでした。時間を置いてもう一度お試しください。');
      }
      onSaved(); setRevision(value => value + 1);
    })();
  }, [onSaved, dealId]);
  const write = useCrmWrite({ dealId, api: contractWriteApi, onSaved: refresh, onWritten: (_req, kind) => { if (kind === 'queued') onQueued?.(); } });
  useEffect(() => {
    const ctl = new AbortController();
    void (async () => {
      const catalog = await apiGet<CrmPropertyCatalogResponse>('/api/crm/property-catalog', { signal: ctl.signal, timeoutMs: 35_000 });
      if (!catalog.ok || !Array.isArray(catalog.data.objects)) { if (!ctl.signal.aborted) setError('契約・成果の項目を取得できませんでした。再読み込みしてください。'); return; }
      const all = catalog.data.objects.find(object => object.object_type === 'deals')?.groups.flatMap(group => group.properties) ?? [];
      const props = all;
      const query = new URLSearchParams({ deal_props: DISPLAY_FIELDS.filter(name => props.some(prop => prop.name === name)).join(','), fresh: '1' });
      const detail = await apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?${query}`, { signal: ctl.signal, timeoutMs: 35_000 });
      if (ctl.signal.aborted) return;
      if (detail.ok && !detail.data.partial.some(part => part.part === 'selected_properties')) { setData({ props, detail: detail.data }); setError(''); }
      else setError('契約・成果の値を取得できませんでした。再読み込みしてください。');
    })();
    return () => { ctl.abort(); };
  }, [dealId, revision]);
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const targeted = data !== null && DELIVERY_PIPELINES.includes(data.detail.deal.pipeline_id ?? '');
  const writable = data?.props.filter(prop => CONTRACT_FIELDS.some(name => name === prop.name) && schema?.editable.some(item => item.object === 'deal' && item.name === prop.name)) ?? [];
  const locked = Object.values(write.statuses).some(status => status.phase === 'saving' || status.phase === 'queued') || write.conflict !== null;
  const canEdit = targeted && write.writesEnabled === true && writable.length > 0 && !error;
  const values: PropValues = { ...data?.detail.selected.deal };
  for (const prop of data?.props ?? []) {
    const overlay = write.overlay[fieldKey(dealId, 'deal', prop.name)];
    if (overlay && (overlay.pending || !overlay.fetchedAt || !data || data.detail.fetched_at <= overlay.fetchedAt)) values[prop.name] = overlay.value;
  }
  const startEdit = () => {
    stageSaving.current = false;
    setSavedFields([]);
    setBase(values);
    setDraft(Object.fromEntries(writable.map(prop => [prop.name, inputValue(prop, values[prop.name])])));
    setEditing(true);
  };
  const request = { ...contractRequest(dealId, writable, base, draft), fetchedAt: data?.detail.fetched_at };
  const statuses = savedFields.flatMap(name => { const status = write.statuses[name === 'dealstage' ? stageKey(dealId) : fieldKey(dealId, 'deal', name)]; return status ? [status] : []; });
  return <section className="dc-root" aria-label="契約・成果">
    <header><h2>契約・成果</h2>{data && <a href={data.detail.deal.deep_link} target="_blank" rel="noreferrer">HubSpotで取引を見る</a>}{canEdit && !editing && <button className="jc-button" disabled={locked} onClick={startEdit}>編集</button>}</header>
    {!data && !error && <p role="status">契約・成果を読み込んでいます…</p>}
    {error && <p role="alert">{error} <button className="jc-button" onClick={() => { setRevision(value => value + 1); }}>再読み込み</button></p>}
    {refreshError && <p role="alert">{refreshError}</p>}
    {data && <>
      {!write.conflict && <SaveStatus statuses={statuses} />}
      {write.conflict && <div role="alert" className="dc-conflict"><h3>ほかの更新があります。現在の値を確認してください。</h3><p>今回の変更は保存されていません。</p><ul>{write.conflict.req.stage && 'dealstage' in write.conflict.current && <li>ステージ：現在の値「{schema?.stages.find(s => s.id === write.conflict?.current.dealstage)?.label ?? '取得できませんでした'}」／移動先「{write.conflict.req.stage.label}」</li>}{write.conflict.req.changes.map(change => {
        const prop = data.props.find(item => item.name === change.name);
        return prop && <li key={change.name}>{prop.label}：現在の値「{displayValue(prop, write.conflict?.current[change.name])}」／入力した値「{displayValue(prop, change.value)}」</li>;
      })}</ul><button className="jc-button" onClick={() => { write.resolveConflict('theirs'); setEditing(false); }}>現在の値を使う</button><button className="jc-button" onClick={() => { write.resolveConflict('mine'); setEditing(false); }}>確認して入力した値で保存</button></div>}
      {targeted && write.writesEnabled === true && schema && schema.stages.length > 0
        ? <DealStageMove dealId={dealId} write={write} detail={data.detail} props={data.props} request={editing ? request : { dealId, changes: [], fetchedAt: data.detail.fetched_at }} locked={locked}
            onStart={names => { stageSaving.current = true; setSavedFields([...names, 'dealstage']); }} onDone={() => { setEditing(false); }} />
        : <p>ステージ：{data.detail.deal.stage_label ?? '未取得'}（表示のみ）</p>}
      {write.writesEnabled === false ? <p>{WRITES_OFF_NOTE}</p> : write.schema.phase === 'error' ? <p>編集できるか確認できないため、表示のみです。</p> : !targeted ? <p>この取引は契約・成果の編集対象外です。</p> : schema && writable.length < CONTRACT_FIELDS.length ? <p>HubSpotで編集が許可された項目だけ変更できます。</p> : null}
      {DISPLAY_FIELDS.some(name => !data.props.some(prop => prop.name === name)) && <p>一部の項目を取得できないため、確認できた項目だけ表示しています。</p>}
      <div className="dc-fields">{DISPLAY_FIELDS.map(name => {
        const prop = data.props.find(item => item.name === name);
        if (!prop) return null;
        const key = fieldKey(dealId, 'deal', name);
        const editable = editing && canEdit && writable.some(item => item.name === name);
        const value = draft[name] ?? '';
        const change = (next: string) => { setDraft(prev => ({ ...prev, [name]: next })); write.dismiss(key); };
        return <div className={`dc-field${savedFields.includes(name) && write.statuses[key]?.phase === 'saved' ? ' dc-written' : ''}`} key={name}><label htmlFor={`dc-${name}`}>{prop.label}</label>{!editable && <strong>{displayValue(prop, values[name])}</strong>}
          {editable && (prop.property_type === 'enumeration' ? <select id={`dc-${name}`} disabled={locked} value={value} onChange={event => { change(event.target.value); }}><option value="">未入力</option>{value && !prop.options.some(option => option.value === value) && <option value={value}>{displayValue(prop, value)}</option>}{prop.options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}</select> : <input id={`dc-${name}`} disabled={locked} type={prop.property_type === 'date' ? 'date' : prop.property_type === 'number' ? 'number' : 'text'} step="any" value={value} onChange={event => { change(event.target.value); }} />)}
          {!CONTRACT_FIELDS.some(field => field === name) && <small>表示のみ</small>}
          {editable && request.changes.some(change => change.name === name) && <small className="dc-original">元: {displayValue(prop, base[name])}</small>}
        </div>;
      })}</div>
      {editing && canEdit && <div className="dc-actions"><button className="jc-button" disabled={locked || !request.changes.length} onClick={() => { setSavedFields(request.changes.map(change => change.name)); void write.save(request).then(outcome => { if (outcome.kind === 'saved' || outcome.kind === 'queued') setEditing(false); }); }}>保存</button><button className="jc-button" disabled={locked} onClick={() => { setEditing(false); }}>キャンセル</button></div>}

    </>}
  </section>;
}
