import { useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { ProposalMemo } from './ProposalMemo';
import { contractWriteApi } from './dealContractApi';
import { NextTodoButton } from './NextTodo';
import { ownerParams } from './OwnerTodo';
import { navigateJobCopy, updateJobCopyQuery, useJobCopySearch } from './navigation';
import { formatDateJst } from './format';
import { yen } from './homeModel';
import './home.css';
export interface RiskFlag { key: string; label: string; new: boolean; since: string | null }
export interface RiskRow { deal_id: string; name: string; company: string | null; consultant: string; flags: string[]; risk_flags: RiskFlag[]; good_flags?: RiskFlag[]; days_left: number | null; days_since_contact: number | null; mtg_last: string | null; actionyotei: string | null; actionnaiyou: string | null }
export interface RiskResponse { status: string; data_day: string | null; previous_day: string | null; comparison: 'ready' | 'first' | 'unavailable'; rows: RiskRow[]; refreshing?: boolean; refresh_failed?: boolean }
export function riskGroups(data: RiskResponse) {
  if (data.comparison !== 'ready') return [{ title: '危ない案件の一覧', rows: data.rows }];
  return [{ title: '今日わかった', rows: data.rows.filter(r => r.risk_flags.some(f => f.new)) }, { title: '前から出ていて直っていない', rows: data.rows.filter(r => !r.risk_flags.some(f => f.new)) }];
}
function Flags({ flags, good = false }: { flags: RiskFlag[]; good?: boolean }) {
  return <ul className="cr-flags">{flags.map(f => <li key={f.key} className={good ? 'cr-good' : f.new ? 'cr-new' : ''}><strong>{f.label}</strong>{f.new && <span>今日わかった</span>}{f.since && <small>{formatDateJst(f.since)}から</small>}</li>)}</ul>;
}
export function jobsHref(search: string, deal: string, listing?: string) {
  const q = new URLSearchParams(search);
  for (const k of ['view', 'risk_deal', 'company', 'deal', 'listing', 'job', 'feature', 'analysis', 'drawer', 'drawerEvent', 'popup', 'popupListing', 'dialog', 'compareLeft', 'compareRight']) q.delete(k);
  q.set('deal', deal); if (listing) q.set('listing', listing);
  return `/app/job-copy?${q.toString()}`;
}
interface Detail { meta: { found: boolean; reason?: string }; deal?: { name: string; start: string; expiration: string; period: number | null; amount: number | null }; events?: { kind: string; date: string; subject?: string; source_label: string; fact: boolean; link_certainty?: string; attach?: { state: string }; summary?: { summary?: string; next_action?: string; concern?: string }; extracted?: boolean; todo?: string; concern?: string; positive?: string; risk?: string; risk_reason?: string; next?: string }[] }
export function RiskDetail({ row, onClose, onSaved }: { row: RiskRow; onClose: () => void; onSaved: () => void }) {
  const search = useJobCopySearch();
  const dialog = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<Detail | null>(null); const [error, setError] = useState('');
  const [actionOptions, setActionOptions] = useState<{value: string; label: string}[]>([]);
  const [workspaceDeal, setWorkspaceDeal] = useState<Record<string, unknown> | null>(null);
  const [portal, setPortal] = useState(''); const [listing, setListing] = useState<string>(); const [jobsReady, setJobsReady] = useState(false);
  useEffect(() => { dialog.current?.showModal(); }, []);
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<Detail>(`/api/consulting/deal-detail?deal_id=${encodeURIComponent(row.deal_id)}`, { signal: ctl.signal }).then(r => { if (!ctl.signal.aborted) { if (r.ok) setData(r.data); else setError('案件の詳細を読み込めませんでした。'); } });
    void apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(row.deal_id)}?deal_props=actionyotei,actionnaiyou,notes_last_contacted`, { signal: ctl.signal }).then(r => { if (r.ok && !ctl.signal.aborted) { setPortal(r.data.hubspot_portal_id); setWorkspaceDeal(r.data.selected.deal); } });
    void contractWriteApi.editSchema(row.deal_id, ctl.signal).then(r => { if (r.ok && !ctl.signal.aborted) setActionOptions((Array.isArray(r.data.editable) ? r.data.editable : []).find(p => p.name === 'actionnaiyou')?.options ?? []); });
    void apiGet<{ status: string; total: number | null; next_offset: number | null; listings: { id: string }[] }>(`/api/job-copy/listings?deal=${encodeURIComponent(row.deal_id)}&offset=0`, { signal: ctl.signal }).then(r => { if (r.ok && !ctl.signal.aborted && r.data.status === 'ready') { if (r.data.total === 1 && r.data.next_offset === null) setListing(r.data.listings[0]?.id); setJobsReady(true); } });
    return () => { ctl.abort(); };
  }, [row.deal_id]);
  const plan = typeof workspaceDeal?.actionyotei === 'string' ? workspaceDeal.actionyotei : row.actionyotei;
  const action = actionOptions.find(o => o.value === workspaceDeal?.actionnaiyou)?.label ?? row.actionnaiyou;
  const contact = row.days_since_contact === null ? typeof workspaceDeal?.notes_last_contacted === 'string' ? formatDateJst(workspaceDeal.notes_last_contacted) : '記録なし' : `${String(row.days_since_contact)}日前`;
  const lastMtg = row.mtg_last ?? data?.events?.filter(e => e.kind === 'mtg').map(e => e.date).sort().at(-1) ?? null;
  return <dialog className="cr-detail" ref={dialog} aria-labelledby="risk-detail-title" onCancel={onClose}>
    <header><h2 id="risk-detail-title">{row.name}</h2><button className="jc-button" onClick={onClose}>閉じる</button></header>
    <p>{row.company ?? '法人未登録'} ／ {row.consultant || '担当未登録'}</p><Flags flags={row.good_flags ?? row.risk_flags} good={!!row.good_flags} />
    {error && <p role="alert">{error}</p>}{!data && !error && <p role="status">詳細を読み込んでいます…</p>}
    {data && !data.meta.found && <p>{data.meta.reason ?? '詳細が見つかりませんでした。'}</p>}
    {data?.deal && <section><h3>契約</h3><p>{formatDateJst(data.deal.start)}〜{formatDateJst(data.deal.expiration)} ／ {data.deal.period ?? '—'}か月 ／ {yen(data.deal.amount)}</p></section>}
    <section><h3>最後の接触とMTG</h3><p>接触：{contact} ／ MTG：{formatDateJst(lastMtg, '記録なし')}</p></section>
    <section><h3>次の予定</h3><p>{formatDateJst(plan, '予定なし')} ／ {action ?? '内容未登録'}</p><NextTodoButton dealId={row.deal_id} hasPlan={!!plan} onSaved={onSaved} /></section>
    <ProposalMemo dealId={row.deal_id} />
    <section><h3>電話とMTGの記録</h3>{data?.events?.length === 0 && <p>記録なし</p>}{data?.events?.filter(e => ['call', 'mtg', 'mail_mtg'].includes(e.kind)).slice(0, 20).map((e, i) => <article key={`${e.date}-${String(i)}`}><h4>{formatDateJst(e.date)} {e.subject ?? (e.kind === 'call' ? '電話' : 'MTG')}</h4><p>{e.source_label}{e.link_certainty && e.link_certainty !== '高' ? ` ／ 案件との結び付き：${e.link_certainty}` : ''}{e.attach?.state && e.attach.state !== 'own' ? ' ／ 別の契約との関連あり' : ''}</p>{e.kind === 'call' ? <><p>{e.summary?.summary ?? '要約なし'}</p>{e.summary?.concern && <p>懸念：{e.summary.concern}</p>}{e.summary?.next_action && <p>次の予定：{e.summary.next_action}</p>}</> : e.extracted === false ? <p>未抽出</p> : <>{([['やること', e.todo], ['懸念', e.concern], ['前向きな話', e.positive], ['リスク判定', e.risk], ['理由', e.risk_reason], ['次回予定', e.next]] as const).filter(([,v]) => v).map(([label,v]) => <p key={label}>{label}：{v}</p>)}</>}</article>)}</section>
    <footer><a className="jc-button" href={jobsHref(search, row.deal_id)} onClick={e => { e.preventDefault(); navigateJobCopy(new URL(e.currentTarget.href)); }}>取引の画面を開く</a>{jobsReady ? <a className="jc-button" href={jobsHref(search, row.deal_id, listing)} onClick={e => { e.preventDefault(); navigateJobCopy(new URL(e.currentTarget.href)); }}>求人を見る</a> : <p>求人を確認しています…</p>}{/^\d+$/.test(portal) && <a href={`https://app.hubspot.com/contacts/${portal}/record/0-3/${row.deal_id}`} target="_blank" rel="noreferrer">HubSpotで開く</a>}</footer>
  </dialog>;
}
export function ConsultantRisk({ good = false }: { good?: boolean }) {
  const title = good ? 'いい案件' : '危ない案件';
  const search = useJobCopySearch(); const query = ownerParams(search); const selected = new URLSearchParams(search).get('risk_deal');
  const [data, setData] = useState<RiskResponse | null>(null); const [error, setError] = useState(false); const [reload, setReload] = useState(0);
  useEffect(() => {
    const ctl = new AbortController(); let timer: ReturnType<typeof setTimeout>;
    const load = async () => {
      const r = await apiGet<RiskResponse>(`/api/job-copy/consultant-${good ? 'good' : 'risk'}?${query}`, { signal: ctl.signal, timeoutMs: 120_000 });
      if (ctl.signal.aborted) return;
      if (r.ok) { setData(r.data); setError(false); if (r.data.status === 'preparing' || r.data.refreshing) timer = setTimeout(() => { void load(); }, 2000); } else setError(true);
    };
    void load(); return () => { ctl.abort(); clearTimeout(timer); };
  }, [query, reload, good]);
  const ready = data?.status === 'ready' ? data : null;
  const row = ready?.rows.find(r => r.deal_id === selected);
  return <div className="jc-page"><main className="cr-main" id="job-details"><header><div><h1>{title}{good && ready && <small> {ready.rows.length}件</small>}</h1>{ready && <p>{formatDateJst(ready.data_day, '集計日未取得')}のデータ{ready.previous_day ? ` ／ 前回：${formatDateJst(ready.previous_day)}` : ''}</p>}</div><button className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{title}を読み込めませんでした。再読み込みしてください。</p>}{!error && (!data || data.status === 'preparing') && <p role="status">案件を集計しています…</p>}{data?.status === 'failed' && <p role="alert">案件を取得できませんでした。再読み込みしてください。</p>}
    {ready?.refresh_failed && <p>前に取得した担当と予定を表示しています。</p>}{!good && ready?.comparison === 'first' && <p>前回と比べられるのは明日のデータから</p>}{!good && ready?.comparison === 'unavailable' && <p>前回との比較を取得できないため、一覧で表示しています。</p>}
    {ready && (good ? [{ title: 'いい案件の一覧', rows: ready.rows }] : riskGroups(ready)).map(group => <section key={group.title} aria-label={group.title}>{!good && <h2>{group.title} <small>{group.rows.length}件</small></h2>}<div className="cr-scroll"><table><thead><tr>{['案件・法人', '担当', good ? '理由' : '名札・いつから', '満了まで', '最後の接触', '次の予定'].map(label => <th key={label}>{label}</th>)}</tr></thead><tbody>{group.rows.map(r => <tr key={r.deal_id} onClick={() => { updateJobCopyQuery({ risk_deal: r.deal_id }); }}><th scope="row"><button className="jh-link" onClick={e => { e.stopPropagation(); updateJobCopyQuery({ risk_deal: r.deal_id }); }}>{r.name}</button><small>{r.company ?? '法人未登録'}</small></th><td>{r.consultant || '未登録'}</td><td><Flags flags={r.good_flags ?? r.risk_flags} good={good} />{good && r.flags.length > 0 && <small>危ない名札もあり（{r.flags.length}件）</small>}</td><td>{r.days_left === null ? '未登録' : r.days_left < 0 ? `満了から${String(-r.days_left)}日` : `${String(r.days_left)}日`}</td><td>{r.days_since_contact === null ? '記録なし' : `${String(r.days_since_contact)}日前`}</td><td>{formatDateJst(r.actionyotei, '予定なし')}<small>{r.actionnaiyou ?? '内容未登録'}</small></td></tr>)}{!group.rows.length && <tr><td colSpan={6}>該当する案件はありません。</td></tr>}</tbody></table></div></section>)}
    {row && <RiskDetail key={row.deal_id} row={row} onClose={() => { updateJobCopyQuery({ risk_deal: null }); }} onSaved={() => { setReload(n => n + 1); }} />}
  </main></div>;
}

export function detailRow(id: string, name: string, company: string | null = null): RiskRow {
  return { deal_id: id, name, company, consultant: '担当未取得', flags: [], risk_flags: [], days_left: null, days_since_contact: null, mtg_last: null, actionyotei: null, actionnaiyou: null };
}
