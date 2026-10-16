import { useCallback, useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { useCrmWrite, WRITES_OFF_NOTE } from '../crm/useCrmWrite';
import { fieldKey } from '../crm/writeModel';
import type { SaveRequest } from '../crm/writeModel';
import type { PropValues } from '../crm/writeTypes';
import { contractWriteApi } from './dealContractApi';
import { formatDateJst } from './format';
import './next-todo.css';

const FIELDS = ['actionyotei', 'actionnaiyou'] as const;
export function defaultTodoDate(now = new Date()): string {
  return new Date(now.getTime() + (9 + 7 * 24) * 3600_000).toISOString().slice(0, 10);
}
export function validTodoDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value;
}
export function todoRequest(dealId: string, base: PropValues, due: string, content: string): SaveRequest {
  return { dealId, changes: FIELDS.map(name => ({ object: 'deal' as const, name, base: base[name] ?? null, value: name === 'actionyotei' ? due : content })) };
}
export interface TodoReason { kind: 'expiration' | 'contact' | 'stalled' | 'first_contract' | 'nps' | 'target' | 'cpa' | 'mtg'; text: string; strong?: boolean }
export function ReasonChips({ reasons }: { reasons: TodoReason[] }) {
  return <span className="nt-reasons">{reasons.map(reason => <span className={`nt-reason${reason.strong ? ' nt-strong' : ''}`} key={reason.kind}>{reason.text}</span>)}</span>;
}
export function NextTodoButton({ dealId, hasPlan = false, onSaved }: { dealId: string; hasPlan?: boolean; onSaved?: (() => void) | undefined }) {
  const [open, setOpen] = useState(false);
  return <><button className="jc-button" type="button" onClick={() => { setOpen(true); }}>{hasPlan ? '終わった → 次の予定' : '次の予定を入れる'}</button>{open && <NextTodoDialog dealId={dealId} onClose={() => { setOpen(false); }} onSaved={onSaved} />}</>;
}
export function NextTodoDialog({ dealId, onClose, onSaved }: { dealId: string; onClose: () => void; onSaved?: (() => void) | undefined }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [base, setBase] = useState<PropValues | null>(null);
  const [due, setDue] = useState(defaultTodoDate);
  const [content, setContent] = useState('');
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => {
    void apiGet(`/api/job-copy/deals/${encodeURIComponent(dealId)}/refresh`).then(r => {
      if (!r.ok) setError('保存済みですが、一覧を更新できませんでした。再読み込みしてください。');
      onSaved?.(); setRevision(n => n + 1); window.dispatchEvent(new Event('job-copy-plan-saved'));
    });
  }, [onSaved, dealId]);
  const write = useCrmWrite({ dealId, api: contractWriteApi, onSaved: refresh });
  useEffect(() => { const el = dialog.current; el?.showModal(); return () => { el?.close(); }; }, []);
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?deal_props=${FIELDS.join(',')}&fresh=1`, { signal: ctl.signal }).then(r => {
      if (ctl.signal.aborted) return;
      if (r.ok && !r.data.partial.some(p => p.part === 'selected_properties')) { setBase(r.data.selected.deal); setError(''); } else setError('予定を取得できませんでした。開き直してください。');
    });
    return () => { ctl.abort(); };
  }, [dealId, revision]);
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const options = schema?.editable.find(p => p.object === 'deal' && p.name === 'actionnaiyou')?.options ?? [];
  const statuses = Object.values(write.statuses);
  const busy = statuses.some(s => s.phase === 'saving' || s.phase === 'queued');
  const dismiss = () => { for (const name of FIELDS) write.dismiss(fieldKey(dealId, 'deal', name)); };
  const saved = statuses.length > 0 && statuses.every(s => s.phase === 'saved');
  const queued = statuses.some(s => s.phase === 'queued');
  const failures = statuses.flatMap(s => s.phase === 'error' || s.phase === 'invalid' ? [s.message] : []);
  const display = (name: string, value: string | null | undefined) => name === 'actionyotei' ? formatDateJst(value ?? null, '未登録') : options.find(o => o.value === value)?.label ?? '未登録';
  return <dialog ref={dialog} className="nt-dialog" aria-labelledby="nt-title" onCancel={e => { if (statuses.some(s => s.phase === 'saving')) e.preventDefault(); else onClose(); }}><h2 id="nt-title">次の予定を入れる</h2>
    {base && <p className="nt-current">現在の予定：{display('actionyotei', base.actionyotei)} ・ {display('actionnaiyou', base.actionnaiyou)}</p>}
    <label>日付<input type="text" inputMode="numeric" placeholder="YYYY/MM/DD" value={due.replaceAll('-', '/')} disabled={busy} onChange={e => { setDue(e.target.value.replaceAll('/', '-')); dismiss(); }} /></label>
    {due.length >= 10 && !validTodoDate(due) && <p role="alert">存在する日付を YYYY/MM/DD で入力してください。</p>}
    <label>内容<select value={content} disabled={busy} onChange={e => { setContent(e.target.value); dismiss(); }}><option value="">選んでください</option>{options.map(o => <option value={o.value} key={o.value}>{o.label}</option>)}</select></label>
    {(!base || !schema) && !error && <p role="status">予定を確認しています…</p>}
    {(error || write.schema.phase === 'error') && <p role="alert">{error || '編集できるか確認できませんでした。開き直してください。'}</p>}
    {write.writesEnabled === false && <p>{WRITES_OFF_NOTE}</p>}
    {saved && <p role="status" className="nt-green">保存済み</p>}
    {busy && <p role="status" className="nt-yellow">{queued ? '一時保存済み・HubSpotへの反映待ち' : '保存しています…'}</p>}
    {failures.length > 0 && <p role="alert" className="nt-red">{[...new Set(failures)].join(' ')}</p>}
    {write.conflict && <div role="alert" className="nt-red"><p>ほかの更新があります。現在の値を確認してください。</p>{FIELDS.map(name => <p key={name}>{name === 'actionyotei' ? '日付' : '内容'}：現在「{display(name, write.conflict?.current[name])}」</p>)}<button className="jc-button" onClick={() => { write.resolveConflict('theirs'); }}>現在の値を使う</button><button className="jc-button" onClick={() => { write.resolveConflict('mine'); }}>確認して入力した値で保存</button></div>}
    <footer><button className="jc-button" disabled={!base || !validTodoDate(due) || !content || write.writesEnabled !== true || busy || !!write.conflict || !FIELDS.every(name => schema?.editable.some(p => p.object === 'deal' && p.name === name))} onClick={() => { if (base) void write.save(todoRequest(dealId, base, due, content)); }}>保存</button><button className="jc-button" disabled={statuses.some(s => s.phase === 'saving')} onClick={onClose}>閉じる</button></footer>
  </dialog>;
}

export function DealNextTodo({ dealId, onSaved }: { dealId: string; onSaved: () => void }) {
  const [data, setData] = useState<WorkspaceResponse | null>(null);
  const [props, setProps] = useState<{ value: string; label: string }[]>([]);
  const [reasons, setReasons] = useState<TodoReason[]>([]);
  const [state, setState] = useState('preparing');
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => { onSaved(); setRevision(n => n + 1); }, [onSaved]);
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?deal_props=${FIELDS.join(',')}&fresh=1`, { signal: ctl.signal }).then(r => {
      if (!ctl.signal.aborted) { if (r.ok && Object.hasOwn(r.data, 'selected') && !r.data.partial.some(p => p.part === 'selected_properties')) setData(r.data); else setError('予定を取得できませんでした。'); }
    });
    void contractWriteApi.editSchema(dealId, ctl.signal).then(r => { if (r.ok && !ctl.signal.aborted) setProps((Array.isArray(r.data.editable) ? r.data.editable : []).find(p => p.name === 'actionnaiyou')?.options ?? []); });
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const r = await apiGet<{ status: string; reasons?: TodoReason[]; refresh_failed?: boolean; refreshing?: boolean }>(`/api/job-copy/deals/${encodeURIComponent(dealId)}/todo`, { signal: ctl.signal });
      if (ctl.signal.aborted) return;
      if (r.ok) { setState(r.data.refresh_failed ? 'stale' : r.data.status); setReasons(r.data.reasons ?? []); if (r.data.status === 'preparing' || r.data.refreshing) timer = setTimeout(() => { void poll(); }, 2000); } else setState('failed');
    };
    void poll();
    return () => { ctl.abort(); clearTimeout(timer); };
  }, [dealId, revision]);
  const values = data?.selected.deal;
  return <section className="dc-root nt-deal" aria-label="次にやること"><h2>次にやること</h2>{error && <p role="alert">{error}</p>}<p>{formatDateJst(values?.actionyotei ?? null, '予定なし')} ・ {props.find(o => o.value === values?.actionnaiyou)?.label ?? '内容未登録'}</p><NextTodoButton dealId={dealId} hasPlan={!!values?.actionyotei} onSaved={refresh} /><h3>要対応</h3><ReasonChips reasons={reasons} />{state === 'preparing' ? <p role="status">要対応を集計しています…</p> : state === 'failed' ? <p role="alert">要対応を取得できませんでした。</p> : state === 'stale' ? <p role="status">前回取得した要対応を表示しています。</p> : !reasons.length && <p>現在、要対応の条件に該当しません。</p>}</section>;
}
