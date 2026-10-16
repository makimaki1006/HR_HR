import { Fragment, useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import { navigateJobCopy, updateJobCopyQuery, useJobCopySearch } from './navigation';
import { OwnerFilter, ownerParams, signalCards } from './OwnerTodo';
import { Breadcrumbs } from './Breadcrumbs';
import { ReasonChips } from './NextTodo';
import { formatDateJst } from './format';
import { yen } from './homeModel';
import { countDealReasons, dealSorts, sortDeals } from './dealBoardModel';
import type { DealBoardResponse, DealSort } from './dealBoardModel';
import './home.css';
import './deal-board.css';
const columns: [DealSort, string][] = [['name', '案件・顧客'], ['reasons', '要対応の印'], ['consultant', '担当'], ['days_left', '満了まで'], ['last_contact', '最後の接触'], ['last_mtg', '最後のMTG'], ['achievement', '採用目標に対して'], ['amount', '金額']];
export function DealBoard() {
  const search = useJobCopySearch(); const query = new URLSearchParams(search); const ownerQuery = ownerParams(search);
  const [data, setData] = useState<DealBoardResponse | null>(null); const [error, setError] = useState(''); const [reload, setReload] = useState(0);
  const [expanded, setExpanded] = useState<string[]>([]);
  const sort = (dealSorts.some(([key]) => key === query.get('deals_sort')) ? query.get('deals_sort') : 'priority') as DealSort;
  const descending = query.get('deals_direction') !== 'asc'; const pipeline = query.get('deals_pipeline') ?? 'all'; const ended = query.get('deals_ended') === 'yes';
  useEffect(() => {
    const controller = new AbortController();
    void apiGet<DealBoardResponse>(`/api/job-copy/deals?${ownerQuery}`, { signal: controller.signal }).then(result => {
      if (controller.signal.aborted) return;
      if (!result.ok) { setError('案件の一覧を読み込めませんでした。時間を置いて再読み込みしてください。'); return; }
      setData(result.data); setError('');
      if (result.data.status === 'ready' && !result.data.work.notice && [null, 'me'].includes(new URLSearchParams(ownerQuery).get('owner'))) updateJobCopyQuery({ owner: result.data.work.owner, owner_field: result.data.work.owner_field }, 'replace');
    });
    return () => { controller.abort(); };
  }, [ownerQuery, reload]);
  useEffect(() => {
    if (data?.status !== 'preparing' && !(data?.status === 'ready' && data.refreshing)) return;
    const timer = window.setTimeout(() => { setReload(n => n + 1); }, 2_000);
    return () => { window.clearTimeout(timer); };
  }, [data]);
  const ready = data?.status === 'ready' ? data : null;
  const reason = query.get('deals_reason');
  const scoped = ready?.rows.filter(row => (ended || row.active) && (pipeline === 'all' || row.pipeline === pipeline)) ?? [];
  const counts = countDealReasons(scoped);
  const rows = sortDeals(scoped.filter(row => !reason || row.reasons.some(item => item.kind === reason)), sort, descending);
  const pages = Math.max(1, Math.ceil(rows.length / 20));
  const requested = query.get('deals_page') ?? '1';
  const page = Math.min(pages, /^[1-9]\d{0,5}$/.test(requested) ? Number(requested) : 1);
  const change = (values: Record<string, string | null>) => { updateJobCopyQuery({ ...values, deals_page: null }, 'replace'); };
  const changeSort = (key: DealSort) => { change({ deals_sort: key, deals_direction: sort === key && descending ? 'asc' : 'desc' }); };
  return <div className="jc-app"><main className="jc-detail jh-home db-board" id="job-details">
    <Breadcrumbs crumbs={[{ level: 'home', label: '案件管理', current: false }, { level: 'deal', label: '案件の一覧', current: true }]} onSelect={() => { updateJobCopyQuery({ view: null }); }} />
    <header className="jh-head"><div><h1>案件の一覧</h1><p>HubSpotの今の値と、シートにある最後のMTGを確認できます。</p></div><button className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{error}</p>}
    {(!data || data.status === 'preparing') && <p role="status">案件を集計しています…</p>}
    {data?.status === 'failed' && <p role="alert">案件を読み込めませんでした。再読み込みしてください。</p>}
    {ready && <>
      <div className="db-filters"><OwnerFilter work={ready.work} /><label>パイプライン<select value={pipeline} onChange={event => { change({ deals_pipeline: event.target.value }); }}><option value="all">すべて</option><option value="21596025">リクロジ_納品管理</option><option value="743961896">請負契約_納品管理</option></select></label>
        <label>終わった案件も出す<input type="checkbox" checked={ended} onChange={event => { change({ deals_ended: event.target.checked ? 'yes' : null }); }} /></label>
      </div>
      {ready.refresh_failed && <p role="status">更新できなかったため、前に取得した値を表示しています。</p>}
      <p className="jh-note">最後のMTG・MTGが途絶えている印：{ready.sheet_as_of ? `${formatDateJst(ready.sheet_as_of)} 時点（前日の集計）` : '集計した日が未取得'}。請負契約はMTGの集計対象外です。</p>
      <div className="db-reason-filters" role="group" aria-label="要対応の印で絞る">
        <button type="button" aria-pressed={!reason} onClick={() => { change({ deals_reason: null }); }}>すべて <strong>{scoped.length}</strong></button>
        {signalCards.map(([kind, label]) => <button type="button" key={kind} className={(counts[kind] ?? 0) === 0 ? 'db-zero' : ''} aria-pressed={reason === kind} onClick={() => { change({ deals_reason: reason === kind ? null : kind }); }}>{label} <strong>{counts[kind] ?? 0}</strong></button>)}
      </div>
      <div className="db-list-meta"><p aria-live="polite">{rows.length}件・{page} / {pages}ページ</p><p className="jh-note">並び: {sort === 'priority' ? '要対応の印が多い順' : `${dealSorts.find(([key]) => key === sort)?.[1] ?? ''}の${descending ? '多い・新しい順' : '少ない・古い順'}`} {sort !== 'priority' && <button type="button" className="jh-link" onClick={() => { change({ deals_sort: null, deals_direction: null }); }}>標準の並びに戻す</button>}</p></div>
      <table className="db-table"><thead><tr>{columns.map(([key, label]) => <th key={key} scope="col" aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button onClick={() => { changeSort(key); }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ' ▲▼'}</button></th>)}<th scope="col" className="db-toggle-column"><span className="jc-visually-hidden">詳しい数字</span></th></tr></thead><tbody>
        {rows.slice((page - 1) * 20, page * 20).map(row => <Fragment key={row.id}><tr>
          <th scope="row"><button className="jh-link" title={row.name ?? '案件名未取得'} onClick={() => { const url = new URL(window.location.href); for (const key of ['view', 'company', 'listing', 'job']) url.searchParams.delete(key); url.searchParams.set('deal', row.id); navigateJobCopy(url); }}>{row.name ?? '案件名未取得'}</button><small title={row.company ?? '顧客名未取得'}>{row.company ?? '顧客名未取得'}</small></th>
          <td><ReasonChips reasons={row.reasons} />{!row.reasons.length && '—'}</td><td>{row.consultant ?? '未設定'}</td>
          <td>{row.days_left === null ? '—' : row.days_left < 0 ? `満了から${String(-row.days_left)}日` : `${String(row.days_left)}日`}<small>{formatDateJst(row.expiration, '日付未取得')}</small></td>
          <td>{formatDateJst(row.last_contact, '記録なし')}</td><td>{row.mtg_status === 'outside' ? '記録なし（対象外）' : row.mtg_status === 'unavailable' ? '未取得' : formatDateJst(row.last_mtg, '記録なし')}</td>
          <td>{row.achievement === null ? '—' : `${String(Math.round(row.achievement * 100))}%`}<small>{row.hires ?? '—'} / {row.target ?? '—'}人</small></td><td>{yen(row.amount)}</td><td className="db-toggle-cell"><button type="button" className="db-expand" aria-label={`${row.name ?? '案件名未取得'}の詳しい数字を開く`} aria-expanded={expanded.includes(row.id)} aria-controls={`deal-numbers-${row.id}`} onClick={() => { setExpanded(ids => ids.includes(row.id) ? ids.filter(id => id !== row.id) : [...ids, row.id]); }}><span aria-hidden="true">{expanded.includes(row.id) ? '▾' : '▸'}</span></button></td>
        </tr>{expanded.includes(row.id) && <tr className="db-detail" id={`deal-numbers-${row.id}`}><td colSpan={9}><table aria-label={`${row.name ?? '案件名未取得'}の詳しい数字`}><thead><tr>{([['company', '顧客名'], ['stage', 'ステージ'], ['months', '何ヶ月目'], ['nps', '定期NPS'], ['applications', '応募'], ['interviews', '面接'], ['hires', '採用'], ['cpa', '採用単価']] as [DealSort, string][]).map(([key, label]) => <th key={key} scope="col" aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button type="button" onClick={() => { changeSort(key); }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ' ▲▼'}</button></th>)}</tr></thead><tbody><tr><td>{row.company ?? '—'}</td><td>{row.stage ?? '未取得'}</td><td>{row.days_left !== null && row.days_left < 0 ? '満了後' : row.months === null ? '—' : `${String(row.months)}ヶ月目`}</td><td>{row.nps ?? '—'}</td><td>{row.applications ?? '—'}</td><td>{row.interviews ?? '—'}</td><td>{row.hires ?? '—'}</td><td>{yen(row.cpa)}</td></tr></tbody></table></td></tr>}</Fragment>)}
        {!rows.length && <tr><td colSpan={9}>条件に合う案件はありません。</td></tr>}
      </tbody></table>
      <nav className="db-pages" aria-label="案件のページ送り"><button className="jc-button" disabled={page <= 1} onClick={() => { updateJobCopyQuery({ deals_page: String(page - 1) }); }}>前の20件</button><span>{page} / {pages}</span><button className="jc-button" disabled={page >= pages} onClick={() => { updateJobCopyQuery({ deals_page: String(page + 1) }); }}>次の20件</button></nav>
    </>}
  </main></div>;
}
