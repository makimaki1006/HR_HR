import { useEffect, useRef, useState } from 'react';
import { InfoTip } from './InfoTip';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { ProposalMemo } from './ProposalMemo';
import { contractWriteApi } from './dealContractApi';
import { NextTodoButton, OverdueBadge } from './NextTodo';
import { ownerParams } from './OwnerTodo';
import { navigateJobCopy, updateJobCopyQuery, useJobCopySearch } from './navigation';
import { formatDateJst } from './format';
import { yen } from './homeModel';
import { expiryText } from './DealHeading';
import { OutLink } from './Pressable';
import './home.css';
export interface RiskFlag { key: string; label: string; new: boolean; since: string | null }
export interface RiskRow { deal_id: string; name: string; company: string | null; consultant: string; flags: string[]; risk_flags: RiskFlag[]; good_flags?: RiskFlag[]; days_left: number | null; days_since_contact: number | null; mtg_last: string | null; actionyotei: string | null; actionnaiyou: string | null; /** 案件一覧から開くときの最後の接触（一覧と同じ値を出す）。 */ list_contact?: { date: string | null; status: 'ready' | 'outside' | 'unavailable' | 'none' } }
export interface RiskResponse { status: string; data_day: string | null; previous_day: string | null; comparison: 'ready' | 'first' | 'unavailable'; rows: RiskRow[]; refreshing?: boolean; refresh_failed?: boolean }
export function riskGroups(data: RiskResponse) {
  if (data.comparison !== 'ready') return [{ title: '危ない案件の一覧', rows: data.rows }];
  return [{ title: '新しくタグが付いた案件', rows: data.rows.filter(r => r.risk_flags.some(f => f.new)) }, { title: '前回から続いている案件', rows: data.rows.filter(r => !r.risk_flags.some(f => f.new)) }];
}
/** タグの日付は行に 1 回だけ。いちばん前から出ているタグの日付を出す。 */
export function earliestSince(flags: RiskFlag[]): string | null {
  return flags.map(f => f.since).filter((d): d is string => !!d).sort()[0] ?? null;
}
/** 危ない案件の既定は、コンサルKPI「今日」と同じタグ 2 つ以上。 */
export const RISK_MIN_FLAGS = 2;
function Flags({ flags, good = false }: { flags: RiskFlag[]; good?: boolean }) {
  const since = earliestSince(flags);
  return <><ul className="cr-flags">{flags.map(f => <li key={f.key} className={good ? 'cr-good' : f.new ? 'cr-new' : ''}><strong>{f.label}</strong>{f.new && <span className="cr-new-mark" title="前回の集計から新しく付いたタグ">新</span>}</li>)}</ul>{since && <small className="cr-since">{formatDateJst(since)}から</small>}</>;
}
/** 満了までの日数の色。案件のページの見出し（expiryText）と同じ判定で、30 日以内と過ぎて 30 日以内は赤い太字。 */
export function expiryClass(days: number): string {
  const tone = expiryText(days).tone;
  return tone === 'urgent' ? 'jc-expiry-urgent' : tone === 'ended' ? 'jc-expiry-ended' : '';
}
/** 一覧の「満了まで」の欄に出す日数（色付き）。 */
export function ExpiryDays({ days, empty }: { days: number | null; empty: string }) {
  if (days === null) return <>{empty}</>;
  return <span className={expiryClass(days)}>{days < 0 ? `満了から${String(-days)}日` : `${String(days)}日`}</span>;
}
/** 小窓の記録は最新の 3 件を出し、残りは押したときだけ出す。 */
export const RECENT_RECORDS = 3;
/** 小窓の詳細を読み込む期限。記録の多い案件でも読み切れるよう、既定（15 秒）より長くする。 */
const DETAIL_TIMEOUT_MS = 30_000;
export function jobsHref(search: string, deal: string, listing?: string) {
  const q = new URLSearchParams(search);
  // 来た元（危ない案件・案件一覧など）を ?from= に残し、案件のページの「← ○○に戻る」とメニューの選択に使う。
  const from = q.get('view') ?? 'today';
  for (const k of ['view', 'risk_deal', 'company', 'deal', 'listing', 'job', 'feature', 'analysis', 'drawer', 'drawerEvent', 'popup', 'popupListing', 'dialog', 'compareLeft', 'compareRight', 'from']) q.delete(k);
  q.set('deal', deal); if (listing) q.set('listing', listing); q.set('from', from);
  return `/app/job-copy?${q.toString()}`;
}
interface Detail { meta: { found: boolean; reason?: string }; deal?: { name: string; start: string; expiration: string; period: number | null; amount: number | null }; events?: { kind: string; date: string; period?: string | null; contact?: boolean; time?: string | null; end_time?: string | null; end_date?: string | null; direction?: string | null; mail_type?: string | null; zoom_held_time?: string | null; subject?: string; source_label: string; fact: boolean; link_certainty?: string; attach?: { state: string }; summary?: { summary?: string; next_action?: string; concern?: string }; extracted?: boolean; todo?: string; concern?: string; positive?: string; risk?: string; risk_reason?: string; next?: string }[] }
export function RiskDetail({ row, onClose, onSaved }: { row: RiskRow; onClose: () => void; onSaved: () => void }) {
  const search = useJobCopySearch();
  const dialog = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<Detail | null>(null); const [error, setError] = useState(''); const [attempt, setAttempt] = useState(0);
  const [allRecords, setAllRecords] = useState(false);
  const [actionOptions, setActionOptions] = useState<{value: string; label: string}[]>([]);
  const [workspaceDeal, setWorkspaceDeal] = useState<Record<string, unknown> | null>(null);
  const [portal, setPortal] = useState(''); const [listing, setListing] = useState<string>(); const [jobsReady, setJobsReady] = useState(false);
  useEffect(() => { dialog.current?.showModal(); }, []);
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(row.deal_id)}?deal_props=actionyotei,actionnaiyou`, { signal: ctl.signal }).then(r => { if (r.ok && !ctl.signal.aborted) { setPortal(r.data.hubspot_portal_id); setWorkspaceDeal(r.data.selected.deal); } });
    void contractWriteApi.editSchema(row.deal_id, ctl.signal).then(r => { if (r.ok && !ctl.signal.aborted) setActionOptions((Array.isArray(r.data.editable) ? r.data.editable : []).find(p => p.name === 'actionnaiyou')?.options ?? []); });
    void apiGet<{ status: string; total: number | null; next_offset: number | null; listings: { id: string }[] }>(`/api/job-copy/listings?deal=${encodeURIComponent(row.deal_id)}&offset=0`, { signal: ctl.signal }).then(r => { if (r.ok && !ctl.signal.aborted && r.data.status === 'ready') { if (r.data.total === 1 && r.data.next_offset === null) setListing(r.data.listings[0]?.id); setJobsReady(true); } });
    return () => { ctl.abort(); };
  }, [row.deal_id]);
  useEffect(() => {
    const ctl = new AbortController();
    void apiGet<Detail>(`/api/consulting/deal-detail?deal_id=${encodeURIComponent(row.deal_id)}`, { signal: ctl.signal, timeoutMs: DETAIL_TIMEOUT_MS }).then(r => { if (!ctl.signal.aborted) { if (r.ok) setData(r.data); else setError('案件の詳細を読み込めませんでした。'); } });
    return () => { ctl.abort(); };
  }, [row.deal_id, attempt]);
  const retry = () => { setError(''); setData(null); setAttempt(n => n + 1); };
  const records = data?.events?.filter(e => ['email', 'call', 'mtg', 'zoom_mtg', 'mail_mtg'].includes(e.kind)) ?? [];
  const shownRecords = allRecords ? records : records.slice(0, RECENT_RECORDS);
  const plan = typeof workspaceDeal?.actionyotei === 'string' ? workspaceDeal.actionyotei : row.actionyotei;
  const action = actionOptions.find(o => o.value === workspaceDeal?.actionnaiyou)?.label ?? row.actionnaiyou;
  const inContract = (e: NonNullable<Detail['events']>[number]) => !e.attach || e.attach.state === 'own' || e.attach.state === 'moved_in';
  const lastContact = data?.events?.filter(e => inContract(e) && (e.kind === 'mtg' || e.kind === 'zoom_mtg' || (e.kind === 'call' && e.contact))).map(e => e.date).sort().at(-1);
  const listContact = row.list_contact;
  const contact = listContact
    ? listContact.status === 'outside' ? '記録なし（対象外）' : listContact.status === 'unavailable' ? '未取得' : formatDateJst(listContact.date, '記録なし')
    : row.days_since_contact === null ? formatDateJst(lastContact, '記録なし') : `${String(row.days_since_contact)}日前`;
  const lastMtg = row.mtg_last ?? data?.events?.filter(e => inContract(e) && (e.kind === 'mtg' || e.kind === 'zoom_mtg')).map(e => e.date).sort().at(-1) ?? null;
  return <dialog className="cr-detail" ref={dialog} aria-labelledby="risk-detail-title" onCancel={onClose}>
    <header><h2 id="risk-detail-title">{row.name}</h2><button className="jc-button" onClick={onClose}>閉じる</button></header>
    <p>{row.company ?? '顧客未登録'} ／ {row.consultant || '担当未登録'}</p><Flags flags={row.good_flags ?? row.risk_flags} good={!!row.good_flags} />
    <nav className="cr-next" aria-label="次へ進む"><a className="jc-button" href={jobsHref(search, row.deal_id)} onClick={e => { e.preventDefault(); navigateJobCopy(new URL(e.currentTarget.href)); }}>案件のページを開く</a>{jobsReady ? <a className="jc-button" href={jobsHref(search, row.deal_id, listing)} onClick={e => { e.preventDefault(); navigateJobCopy(new URL(e.currentTarget.href)); }}>求人を見る</a> : <span className="cr-next-wait">求人を確認しています…</span>}{/^\d+$/.test(portal) && <OutLink href={`https://app.hubspot.com/contacts/${portal}/record/0-3/${row.deal_id}`} name="HubSpotで開く" />}</nav>
    {error && <div className="cr-error" role="alert"><span>{error}</span><button type="button" className="jc-button" onClick={retry}>もう一度読み込む</button></div>}{!data && !error && <p role="status">詳細を読み込んでいます…</p>}
    {data && !data.meta.found && <p>{data.meta.reason ?? '詳細が見つかりませんでした。'}</p>}
    {data?.deal && <section><h3>契約</h3><p>{formatDateJst(data.deal.start)}〜{formatDateJst(data.deal.expiration)} ／ {data.deal.period ?? '—'}か月 ／ {yen(data.deal.amount)}</p></section>}
    <section><h3>最後の接触とMTG</h3><p>接触：{contact} ／ MTG：{formatDateJst(lastMtg, '記録なし')}</p></section>
    <section><h3>次の予定</h3><p>{formatDateJst(plan, '予定なし')} ／ {action ?? '内容未登録'}<OverdueBadge due={plan} /></p><NextTodoButton dealId={row.deal_id} hasPlan={!!plan} onSaved={onSaved} /></section>
    <ProposalMemo dealId={row.deal_id} />
    <section><h3>メール・電話・MTGの記録{records.length > 0 && <small> 新しい順 {records.length}件</small>}</h3>{data && records.length === 0 && <p>記録なし</p>}{shownRecords.map((e, i) => <article key={`${e.date}-${String(i)}`}><h4>{formatDateJst(e.date)}{e.time ? ` ${e.time}` : ''} {e.kind === 'email' ? `メール ${e.subject ?? '件名なし'}` : e.subject ?? (e.kind === 'call' ? '電話' : 'MTG')}</h4><p>{e.source_label}{e.link_certainty && e.link_certainty !== '高' ? ` ／ 案件との結び付き：${e.link_certainty}` : ''}{e.period ? ` ／ 契約期間の外（${e.period}）` : e.attach?.state && e.attach.state !== 'own' ? ' ／ 別の契約との関連あり' : ''}</p>{e.kind === 'email' ? <p>{e.direction} ／ {e.mail_type}{e.zoom_held_time && <span> ／ Zoom で開催 {e.zoom_held_time}</span>}</p> : e.kind === 'zoom_mtg' ? <p>{e.time ?? '開始時刻不明'}〜{e.end_date && e.end_date !== e.date ? `${formatDateJst(e.end_date)} ` : ''}{e.end_time ?? '終了時刻不明'}</p> : e.kind === 'call' ? <><p>{e.summary?.summary ?? '要約なし'}</p>{e.summary?.concern && <p>懸念：{e.summary.concern}</p>}{e.summary?.next_action && <p>次の予定：{e.summary.next_action}</p>}</> : e.extracted === false ? <p>未抽出</p> : <>{([['やること', e.todo], ['懸念', e.concern], ['前向きな話', e.positive], ['リスク判定', e.risk], ['理由', e.risk_reason], ['次回予定', e.next]] as const).filter(([,v]) => v).map(([label,v]) => <p key={label}>{label}：{v}</p>)}</>}</article>)}{records.length > RECENT_RECORDS && <button type="button" className="jc-open cr-more-records" aria-expanded={allRecords} onClick={() => { setAllRecords(value => !value); }}>{allRecords ? `新しい${String(RECENT_RECORDS)}件だけにする ▴` : `残りを見る（${String(records.length - RECENT_RECORDS)}件） ▾`}</button>}</section>
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
  const includeSingle = !good && new URLSearchParams(search).get('risk_all') === '1';
  const shown = ready ? (good || includeSingle ? ready.rows : ready.rows.filter(r => r.flags.length >= RISK_MIN_FLAGS)) : [];
  const singles = ready ? ready.rows.length - ready.rows.filter(r => r.flags.length >= RISK_MIN_FLAGS).length : 0;
  const view = ready ? { ...ready, rows: shown } : null;
  const row = ready?.rows.find(r => r.deal_id === selected);
  return <div className="jc-page"><main className="cr-main" id="job-details"><header><div><h1>{title}{good && ready && <small> {ready.rows.length}件</small>}</h1>{ready && <p>{formatDateJst(ready.data_day, '集計日未取得')}のデータ{ready.previous_day ? ` ／ 前回：${formatDateJst(ready.previous_day)}` : ''}</p>}{ready && !good && (ready.comparison === 'first' || ready.comparison === 'unavailable') && <InfoTip label="前回との比較"><p>{ready.comparison === 'first' ? '前回と比べられるのは明日のデータからです。' : '前回との比較を取得できないため、一覧で表示しています。'}</p></InfoTip>}</div><button className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    {ready && !good && <p className="cr-scope" role="note"><label><input type="checkbox" checked={includeSingle} onChange={e => { updateJobCopyQuery({ risk_all: e.target.checked ? '1' : null }, 'replace'); }} /> タグが1つだけの案件も出す</label> <span>{includeSingle ? `タグが1つだけの案件 ${String(singles)}件を含めて ${String(shown.length)}件を出しています。` : `タグが2つ以上の案件 ${String(shown.length)}件を出しています（コンサルKPIの「今日」と同じ基準）。タグが1つだけの案件 ${String(singles)}件は出していません。`}</span></p>}
    {error && <p role="alert">{title}を読み込めませんでした。再読み込みしてください。</p>}{!error && (!data || data.status === 'preparing') && <p role="status">案件を集計しています…</p>}{data?.status === 'failed' && <p role="alert">案件を取得できませんでした。再読み込みしてください。</p>}
    {ready?.refresh_failed && <p>前に取得した担当と予定を表示しています。</p>}
    {view && (good ? [{ title: 'いい案件の一覧', rows: view.rows }] : riskGroups(view)).map(group => <section key={group.title} aria-label={group.title}>{!good && <h2>{group.title} <small>{group.rows.length}件</small></h2>}<div className="cr-scroll"><table><thead><tr>{['案件・顧客', '担当', good ? '理由' : 'タグ・いつから', '満了まで', '最後の接触', '次の予定'].map(label => <th key={label}>{label}</th>)}<th className="cr-open-col"><span className="jc-visually-hidden">詳細</span></th></tr></thead><tbody>{group.rows.map(r => <tr key={r.deal_id} onClick={() => { updateJobCopyQuery({ risk_deal: r.deal_id }); }}><th scope="row"><button type="button" className="jc-row-name" onClick={e => { e.stopPropagation(); updateJobCopyQuery({ risk_deal: r.deal_id }); }}>{r.name}</button><small>{r.company ?? '顧客未登録'}</small></th><td>{r.consultant || '未登録'}</td><td><Flags flags={r.good_flags ?? r.risk_flags} good={good} />{good && r.flags.length > 0 && <small>危ないタグもあり（{r.flags.length}件）</small>}</td><td><ExpiryDays days={r.days_left} empty="未登録" /></td><td>{r.days_since_contact === null ? '記録なし' : `${String(r.days_since_contact)}日前`}</td><td>{formatDateJst(r.actionyotei, '予定なし')}<small>{r.actionnaiyou ?? '内容未登録'}</small></td><td className="cr-open-cell"><button type="button" className="jc-open" aria-label={`${r.name}の詳細を開く`} onClick={e => { e.stopPropagation(); updateJobCopyQuery({ risk_deal: r.deal_id }); }}>詳細 ▸</button></td></tr>)}{!group.rows.length && <tr><td colSpan={7}>該当する案件はありません。</td></tr>}</tbody></table></div></section>)}
    {row && <RiskDetail key={row.deal_id} row={row} onClose={() => { updateJobCopyQuery({ risk_deal: null }); }} onSaved={() => { setReload(n => n + 1); }} />}
  </main></div>;
}

export function detailRow(id: string, name: string, company: string | null = null): RiskRow {
  return { deal_id: id, name, company, consultant: '担当未取得', flags: [], risk_flags: [], days_left: null, days_since_contact: null, mtg_last: null, actionyotei: null, actionnaiyou: null };
}
