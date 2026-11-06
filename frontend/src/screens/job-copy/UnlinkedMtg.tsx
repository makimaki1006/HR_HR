import { useCallback, useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { useCrmWrite, WRITES_OFF_NOTE } from '../crm/useCrmWrite';
import { fieldKey } from '../crm/writeModel';
import { contractWriteApi } from './dealContractApi';
import { ownerParams } from './OwnerTodo';
import { useJobCopySearch } from './navigation';
import { formatDateJst } from './format';
import { MANUAL_LINKS_FIELD, addManualLink, dealChoices, removeManualLink } from './unlinkedMtgModel';
import type { UnlinkedResponse, UnlinkedRow } from './unlinkedMtgModel';
import { demoUnlinkedMtg } from './demoUnlinkedMtg';
import './consultant-risk.css';
import './next-todo.css';
import './unlinked-mtg.css';

type Chosen = { deal_id: string; deal_name: string; pending: boolean } | null;

async function readLinks(dealId: string): Promise<{ ok: true; base: string | null } | { ok: false }> {
  const r = await apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?deal_props=${MANUAL_LINKS_FIELD}&fresh=1`);
  if (!r.ok || r.data.partial.some(p => p.part === 'selected_properties')) return { ok: false };
  const v = r.data.selected.deal[MANUAL_LINKS_FIELD];
  return { ok: true, base: typeof v === 'string' ? v : null };
}

export function UnlinkedMtg() {
  const search = useJobCopySearch(); const query = ownerParams(search);
  const demo = new URLSearchParams(search).get('demo') === '1';
  const [data, setData] = useState<UnlinkedResponse | null>(demo ? demoUnlinkedMtg : null);
  const [error, setError] = useState(false); const [reload, setReload] = useState(0);
  const [fresh, setFresh] = useState(false);
  const [local, setLocal] = useState<Record<string, Chosen>>({});
  const [picked, setPicked] = useState<Record<string, string>>({});
  const [messages, setMessages] = useState<Record<string, string>>({});
  useEffect(() => {
    if (demo) return;
    const ctl = new AbortController(); let timer: ReturnType<typeof setTimeout>;
    const load = async () => {
      const r = await apiGet<UnlinkedResponse>(`/api/job-copy/unlinked-mtg?${query}${fresh ? '&fresh=1' : ''}`, { signal: ctl.signal, timeoutMs: 120_000 });
      if (ctl.signal.aborted) return;
      if (!r.ok) { setError(true); return; }
      setData(r.data); setError(false);
      if (r.data.status === 'preparing') timer = setTimeout(() => { void load(); }, 2000);
    };
    void load(); return () => { ctl.abort(); clearTimeout(timer); };
  }, [query, reload, demo, fresh]);
  const rows = data?.status === 'ready' ? data.rows ?? [] : [];
  // 編集できる項目の確認には、一覧のどれか 1 つの取引を使う（編集できる項目は取引によらず同じ）。
  const schemaDeal = rows.flatMap(r => [r.selected?.deal_id, ...r.candidates.map(c => c.id)]).find((id): id is string => !!id) ?? null;
  const write = useCrmWrite({ dealId: demo ? null : schemaDeal, api: contractWriteApi, onSaved: () => undefined });
  const editable = write.schema.phase === 'ready' && write.schema.schema.editable.some(p => p.object === 'deal' && p.name === MANUAL_LINKS_FIELD);
  const canWrite = !demo && write.writesEnabled === true && editable;
  const email = data?.me_email ?? '';
  const say = (uuid: string, text: string) => { setMessages(m => ({ ...m, [uuid]: text })); };

  const bind = useCallback(async (row: UnlinkedRow, dealId: string, dealName: string) => {
    say(row.uuid, '保存しています…');
    const current = await readLinks(dealId);
    if (!current.ok) { say(row.uuid, '取引の今の内容を読めませんでした。もう一度お試しください。'); return; }
    const next = addManualLink(current.base, row.uuid, email, new Date());
    if (next === null) { setLocal(l => ({ ...l, [row.uuid]: { deal_id: dealId, deal_name: dealName, pending: false } })); say(row.uuid, ''); return; }
    const out = await write.save({ dealId, changes: [{ object: 'deal', name: MANUAL_LINKS_FIELD, base: current.base, value: next }] });
    if (out.kind === 'saved' || out.kind === 'queued') {
      setLocal(l => ({ ...l, [row.uuid]: { deal_id: dealId, deal_name: dealName, pending: out.kind === 'queued' } })); say(row.uuid, ''); setFresh(true);
    } else if (out.kind === 'conflict') say(row.uuid, 'ほかの更新がありました。もう一度結び付けてください。');
    else say(row.uuid, out.kind === 'error' ? out.message : '保存できませんでした。内容を確かめてください。');
  }, [email, write]);
  const cancel = useCallback(async (row: UnlinkedRow, dealId: string) => {
    say(row.uuid, '取り消しています…');
    const current = await readLinks(dealId);
    if (!current.ok) { say(row.uuid, '取引の今の内容を読めませんでした。もう一度お試しください。'); return; }
    const out = await write.save({ dealId, changes: [{ object: 'deal', name: MANUAL_LINKS_FIELD, base: current.base, value: removeManualLink(current.base, row.uuid) }] });
    if (out.kind === 'saved' || out.kind === 'queued') { setLocal(l => ({ ...l, [row.uuid]: null })); say(row.uuid, ''); setFresh(true); }
    else say(row.uuid, out.kind === 'error' ? out.message : '取り消せませんでした。もう一度お試しください。');
  }, [write]);

  return <div className="jc-page"><main className="um-main" id="job-details"><header><div><h1>未連携のMTG</h1><p className="um-sub">夜の更新で取引に結び付けられなかった録画です。取引を選ぶと、次の夜の更新で反映されます。</p></div><button className="jc-button" onClick={() => { setFresh(true); setReload(n => n + 1); }}>再読み込み</button></header>
    {error && <p role="alert">一覧を読み込めませんでした。再読み込みしてください。</p>}
    {!error && (!data || data.status === 'preparing') && <p role="status">一覧を準備しています…</p>}
    {data?.status === 'failed' && <p role="alert">一覧を取得できませんでした。再読み込みしてください。</p>}
    {data?.status === 'unavailable' && <p role="status">未連携の録画の一覧を読み込めません。</p>}
    {data?.status === 'ready' && data.selected_known === false && <p role="status">選択済みかどうかを確認できませんでした。選んだ録画にも印が付かないことがあります。</p>}
    {data?.status === 'ready' && write.writesEnabled === false && <p role="status">{WRITES_OFF_NOTE}。いまは見るだけです。</p>}
    {data?.status === 'ready' && write.writesEnabled === true && !editable && <p role="status">この項目を編集する権限がありません。いまは見るだけです。</p>}
    {data?.status === 'ready' && <ul className="um-list" aria-label="未連携のMTG">
      {rows.map(r => {
        const chosen: Chosen = r.uuid in local ? local[r.uuid] ?? null : r.selected ? { deal_id: r.selected.deal_id, deal_name: r.selected.deal_name, pending: false } : null;
        const choices = dealChoices(r, data.own_deals);
        // 最初は何も選ばない。押し間違いで別の取引に付けないよう、本人が選んでから結び付ける。
        const value = picked[r.uuid] ?? '';
        const key = chosen ? fieldKey(chosen.deal_id, 'deal', MANUAL_LINKS_FIELD) : '';
        const busy = (key !== '' && ['saving', 'queued'].includes(write.statuses[key]?.phase ?? '')) || ['保存しています…', '取り消しています…'].includes(messages[r.uuid] ?? '');
        return <li key={r.uuid} className="um-row">
          <div><span className="um-date">{formatDateJst(r.date, '日付なし')}</span><span className="um-host">{r.host ? `ホスト：${r.host}` : 'ホスト不明'}</span></div>
          <div><p className="um-subject">{r.subject || '件名なし'}</p><span className="um-sub">{r.recording && <a href={r.recording} target="_blank" rel="noreferrer">録画を開く</a>}{r.other_candidates > 0 && <> ／ シートの候補はほかに{r.other_candidates}件</>}</span></div>
          <div className="um-pick">
            {chosen ? <p className="um-done" role="status">選択済み・夜の更新で反映<small>{chosen.deal_name || chosen.deal_id}{chosen.pending ? '（HubSpotへの反映待ち）' : ''}</small></p>
              : choices.length === 0 ? <p className="um-sub">選べる取引がありません。</p>
                : <><label className="um-sub">結び付ける取引<select value={value} disabled={!canWrite || busy} onChange={e => { setPicked(p => ({ ...p, [r.uuid]: e.target.value })); }}>
                  <option value="">取引を選んでください</option>
                  {choices.map(c => <option key={c.id} value={c.id}>{c.candidate ? '候補　' : ''}{c.name || c.id}</option>)}</select></label></>}
            <div className="um-actions">
              {chosen ? <button className="jc-button" disabled={!canWrite || busy} onClick={() => { void cancel(r, chosen.deal_id); }}>取り消す</button>
                : choices.length > 0 && <button className="jc-button" disabled={!canWrite || busy || !value} onClick={() => { void bind(r, value, choices.find(c => c.id === value)?.name ?? ''); }}>結び付ける</button>}
              {messages[r.uuid] && <span className="um-sub" role="status">{messages[r.uuid]}</span>}
            </div>
            {write.statuses[key]?.phase === 'error' && <p role="alert" className="nt-red">保存できませんでした。もう一度お試しください。</p>}
          </div>
        </li>;
      })}
      {rows.length === 0 && <li className="um-sub">未連携の録画はありません。</li>}
    </ul>}
  </main></div>;
}
