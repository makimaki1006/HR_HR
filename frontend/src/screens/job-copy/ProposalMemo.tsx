import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { appendMemo } from '../crm/callResultModel';
import { useCrmWrite, WRITES_OFF_NOTE } from '../crm/useCrmWrite';
import { fieldKey } from '../crm/writeModel';
import type { SaveRequest } from '../crm/writeModel';
import { contractWriteApi } from './dealContractApi';
import './next-todo.css';

export function proposalMemoRequest(dealId: string, base: string | null, draft: string, now = new Date()): SaveRequest {
  const today = new Date(now.getTime() + 9 * 3600_000).toISOString().slice(0, 10);
  return { dealId, changes: [{ object: 'deal', name: 'teian', base, value: appendMemo(base, draft, 'text', today) }] };
}
export function ProposalMemo({ dealId }: { dealId: string }) {
  const [base, setBase] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const write = useCrmWrite({ dealId, api: contractWriteApi, onSaved: () => { setRevision(n => n + 1); } });
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?deal_props=teian&fresh=1`, { signal: ctl.signal }).then(r => {
      if (ctl.signal.aborted) return;
      if (r.ok && !r.data.partial.some(p => p.part === 'selected_properties')) {
        setBase(r.data.selected.deal.teian ?? null); setLoaded(true); setError('');
      } else setError('メモを取得できませんでした。開き直してください。');
    });
    return () => { ctl.abort(); };
  }, [dealId, revision]);
  const key = fieldKey(dealId, 'deal', 'teian');
  const status = write.statuses[key];
  const busy = status?.phase === 'saving' || status?.phase === 'queued';
  const editable = write.schema.phase === 'ready' && write.schema.schema.editable.some(p => p.object === 'deal' && p.name === 'teian');
  const canWrite = loaded && write.writesEnabled === true && editable;
  const current = write.conflict ? write.conflict.current.teian ?? null : write.overlay[key]?.value ?? base;
  return <section className="pm-memo"><h3>CS提案内容メモ</h3>
    <p className="pm-current">{loaded ? (current === '' ? 'メモなし' : current ?? 'メモなし') : 'メモを確認しています…'}</p>
    {error && <p role="alert">{error}</p>}
    {write.schema.phase === 'error' && <p>編集できるか確認できませんでした。開き直してください。</p>}
    {write.writesEnabled === false && <p>{WRITES_OFF_NOTE}</p>}
    {write.writesEnabled === true && !editable && <p>このメモを編集する権限がありません。</p>}
    {canWrite && <><label>追記するメモ<textarea value={draft} disabled={busy} onChange={e => { setDraft(e.target.value); write.dismiss(key); }} /></label>
      <button className="jc-button" disabled={!draft.trim() || busy || !!write.conflict} onClick={() => {
        const request = status?.phase === 'error' && status.req ? status.req : proposalMemoRequest(dealId, current, draft);
        void write.save(request).then(outcome => { if (outcome.kind === 'saved' || outcome.kind === 'queued') setDraft(''); });
      }}>メモを保存</button></>}
    {status?.phase === 'saved' && <p role="status" className="nt-green">保存済み</p>}
    {busy && <p role="status" className="nt-yellow">{status.phase === 'queued' ? '一時保存済み・HubSpotへの反映待ち' : '保存しています…'}</p>}
    {(status?.phase === 'error' || status?.phase === 'invalid') && <p role="alert" className="nt-red">{status.message}</p>}
    {write.conflict && <div role="alert" className="nt-red"><p>ほかの更新があります。最新のメモを確認してください。</p><button className="jc-button" onClick={() => { setBase(current); write.resolveConflict('theirs'); }}>最新のメモを確認</button></div>}
  </section>;
}
