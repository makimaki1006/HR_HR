import { RiskDetail, detailRow } from './ConsultantRisk';
import { ReportFreshness } from './ReportFreshness';
import { Fragment, useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { OwnerFilter, ownerParams, signalCards } from './OwnerTodo';
import { Breadcrumbs } from './Breadcrumbs';
import { ReasonChips } from './NextTodo';
import { formatDateJst } from './format';
import { yen } from './homeModel';
import { countDealReasons, dealSorts, sortDeals, launchGroups, launchLabel, sortLaunch } from './dealBoardModel';
import type { DealBoardResponse, DealSort, DealRow } from './dealBoardModel';
import './home.css';
import './deal-board.css';
const columns: [DealSort, string][] = [['name', '案件・顧客'], ['reasons', '要対応の印'], ['consultant', '担当'], ['recent_applications', '応募（直近4週）'], ['hires', '採用'], ['cpa', '採用単価'], ['nps', 'NPS'], ['amount', '金額'], ['achievement', '採用目標に対して'], ['days_left', '満了まで'], ['last_contact', '最後の接触'], ['last_mtg', '最後のMTG']];
function LaunchCells({ row, preparing }: { row: DealRow; preparing: boolean }) {
  const launch = row.launch;
  const missing = preparing ? '集計中' : '未取得';
  const maximum = Math.max(1, ...(launch.weeks ?? []));
  return <>
    <td>{row.consultant ?? '未設定'}</td>
    <td>{formatDateJst(launch.start, '未取得')}<small>{launch.elapsed === null ? '未取得' : launch.elapsed < 0 ? '開始前' : `開始から${String(launch.elapsed)}日`}</small></td>
    <td>{launch.total === null ? missing : `${String(launch.total)}件`}</td>
    <td className={`db-launch-${launch.status}`}>{launchLabel(row, preparing)}</td>
    <td>{launch.total === null ? missing : launch.first ? <>{formatDateJst(launch.first)}<small>開始から{launch.first_day}日目</small></> : 'まだ無し'}</td>
    <td>{launch.weeks === null ? missing : <div className="db-week-bars" role="img" aria-label={launch.weeks.map((n, i) => `${String(i + 1)}週目 ${String(n)}件`).join('、') || '開始前'}>
      {launch.weeks.map((n, i) => <span key={i} title={`${String(i + 1)}週目 ${String(n)}件`}><b style={{ height: `${String(n === 0 ? 2 : 4 + n / maximum * 24)}px` }} /><small>{i + 1}</small></span>)}
    </div>}</td>
    <td>{launch.live === null ? missing : `${String(launch.live)}件`}</td>
  </>;
}
export function DealBoard() {
  const search = useJobCopySearch(); const query = new URLSearchParams(search); const ownerQuery = ownerParams(search);
  const [data, setData] = useState<DealBoardResponse | null>(null); const [error, setError] = useState(''); const [reload, setReload] = useState(0);
  const [expanded, setExpanded] = useState<string[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [moreRight, setMoreRight] = useState(false);
  const checkScroll = () => {
    const el = scrollRef.current;
    setMoreRight(!!el && el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  };
  const firstOnly = query.get('deals_first') === 'yes';
  const launchGroup = query.get('deals_launch');
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
  const isReady = !!ready;
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const observer = new ResizeObserver(checkScroll);
    observer.observe(el);
    if (el.firstElementChild) observer.observe(el.firstElementChild);
    checkScroll();
    return () => { observer.disconnect(); };
  }, [isReady, firstOnly]);
  const reason = query.get('deals_reason');
  const scoped = ready?.rows.filter(row => (!firstOnly || row.first_contract) && (ended || row.active) && (pipeline === 'all' || row.pipeline === pipeline)) ?? [];
  const counts = countDealReasons(scoped);
  const marked = scoped.filter(row => !reason || row.reasons.some(item => item.kind === reason));
  const filtered = marked.filter(row => !firstOnly || !launchGroup || row.launch.status === launchGroup);
  const rows = firstOnly && sort === 'priority' ? sortLaunch(filtered) : sortDeals(filtered, sort, descending);
  const selected = ready?.rows.find(row => row.id === query.get('risk_deal'));
  const preparing = ready?.applications_status === 'preparing';
  const pages = Math.max(1, Math.ceil(rows.length / 20));
  const requested = query.get('deals_page') ?? '1';
  const page = Math.min(pages, /^[1-9]\d{0,5}$/.test(requested) ? Number(requested) : 1);
  const change = (values: Record<string, string | null>) => { updateJobCopyQuery({ ...values, deals_page: null }, 'replace'); };
  const changeSort = (key: DealSort) => { change({ deals_sort: key, deals_direction: sort === key && descending ? 'asc' : 'desc' }); };
  return <div className="jc-app"><main className="jc-detail jh-home db-board" id="job-details">
    <Breadcrumbs crumbs={[{ level: 'home', label: '案件管理', current: false }, { level: 'deal', label: '案件の一覧', current: true }]} onSelect={() => { updateJobCopyQuery({ view: null }); }} />
    <header className="jh-head"><div><h1>案件の一覧</h1>{firstOnly && <p>初回案件の応募の立ち上がりを確認できます。</p>}</div><button className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{error}</p>}
    {(!data || data.status === 'preparing') && <p role="status">案件を集計しています…</p>}
    {data?.status === 'failed' && <p role="alert">案件を読み込めませんでした。再読み込みしてください。</p>}
    {ready && <>
      <ReportFreshness builtAt={ready.facts_built_at} stale={ready.facts_stale} refreshing={ready.refreshing} />
      <div className="db-filters"><OwnerFilter work={ready.work} /><label>パイプライン<select value={pipeline} onChange={event => { change({ deals_pipeline: event.target.value }); }}><option value="all">すべて</option><option value="21596025">リクロジ_納品管理</option><option value="743961896">請負契約_納品管理</option></select></label>
        <label>初回案件だけ<input type="checkbox" checked={firstOnly} onChange={event => { change({ deals_first: event.target.checked ? 'yes' : null, deals_launch: null, deals_sort: null, deals_direction: null }); }} /></label>
        <label>終わった案件も出す<input type="checkbox" checked={ended} onChange={event => { change({ deals_ended: event.target.checked ? 'yes' : null }); }} /></label>
      </div>
      {ready.refresh_failed && <p role="status">更新できなかったため、前に取得した値を表示しています。</p>}
      <p className="jh-note">最後のMTG・MTGが途絶えている印：{ready.sheet_as_of ? `${formatDateJst(ready.sheet_as_of)} 時点（前日の集計）` : '集計した日が未取得'}。請負契約はMTGの集計対象外です。</p>
      <div className="db-reason-filters" role="group" aria-label="要対応の印で絞る">
        <button type="button" aria-pressed={!reason} onClick={() => { change({ deals_reason: null }); }}>すべて <strong>{scoped.length}</strong></button>
        {signalCards.map(([kind, label]) => <button type="button" key={kind} className={(counts[kind] ?? 0) === 0 ? 'db-zero' : ''} aria-pressed={reason === kind} onClick={() => { change({ deals_reason: reason === kind ? null : kind }); }}>{label} <strong>{counts[kind] ?? 0}</strong></button>)}
      </div>
      {firstOnly && <><div className="db-reason-filters db-launch-summary" role="group" aria-label="初回案件の応募で絞る"><button type="button" aria-pressed={!launchGroup} onClick={() => { change({ deals_launch: null }); }}>初回案件 <strong>{marked.length}件</strong>：</button>{launchGroups.map(([key, label]) => <button type="button" key={key} aria-pressed={launchGroup === key} onClick={() => { change({ deals_launch: launchGroup === key ? null : key }); }}>{label} <strong>{marked.filter(row => row.launch.status === key).length}</strong></button>)}</div><p className="jh-note">応募は契約開始日から今日まで（日本時間）。約60分ごとに更新します。棒は開始日から7日ごと・最初の8週で、始まっていない週は表示しません。応募日が分からないときは未取得です。</p></>}
      <div className="db-list-meta"><p aria-live="polite">{rows.length}件・{page} / {pages}ページ</p><p className="jh-note">並び: {sort === 'priority' ? firstOnly ? '目安より少ない → 開始から日数の長い順' : '要対応の印が多い順' : `${dealSorts.find(([key]) => key === sort)?.[1] ?? ''}の${descending ? '多い・新しい順' : '少ない・古い順'}`} {sort !== 'priority' && <button type="button" className="jh-link" onClick={() => { change({ deals_sort: null, deals_direction: null }); }}>標準の並びに戻す</button>}</p></div>
      <div className={`db-scroll-frame${moreRight ? ' db-more-right' : ''}`}><div className="db-board-scroll" ref={scrollRef} onScroll={checkScroll} tabIndex={0} role="region" aria-label="案件一覧（横スクロール）"><table className={`db-table${firstOnly ? ' db-launch-table' : ''}`}><thead><tr>{(firstOnly ? columns.slice(0, 1) : columns).map(([key, label]) => <th key={key} scope="col" aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button onClick={() => { changeSort(key); }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ' ▲▼'}</button></th>)}{firstOnly && ['担当', '契約開始日', '開始からの応募数', '目安との比較', '最初の応募', '週ごとの応募', '掲載中の求人数'].map(label => <th scope="col" key={label}>{label}</th>)}<th scope="col" className="db-toggle-column"><span className="jc-visually-hidden">詳しい数字</span></th></tr></thead><tbody>
        {rows.slice((page - 1) * 20, page * 20).map(row => <Fragment key={row.id}><tr onClick={event => { if (!(event.target as HTMLElement).closest('button, a')) updateJobCopyQuery({ risk_deal: row.id }); }}>
          <th scope="row"><button className="jh-link" title={row.name ?? '案件名未取得'} onClick={() => { updateJobCopyQuery({ risk_deal: row.id }); }}>{row.name ?? '案件名未取得'}</button><small title={row.company ?? '顧客名未取得'}>{row.company ?? '顧客名未取得'}</small></th>
          {firstOnly ? <LaunchCells row={row} preparing={preparing} /> : <><td><ReasonChips reasons={row.reasons} />{!row.reasons.length && '—'}</td><td>{row.consultant ?? '未設定'}</td>
          <td>{row.recent_applications ?? '—'}</td><td>{row.hires ?? '—'}</td><td>{yen(row.cpa)}</td><td>{row.nps ?? '—'}</td><td>{yen(row.amount)}</td>
          <td>{row.achievement === null ? '—' : `${String(Math.round(row.achievement * 100))}%`}<small>{row.hires ?? '—'} / {row.target ?? '—'}人</small></td>
          <td>{row.days_left === null ? '—' : row.days_left < 0 ? `満了から${String(-row.days_left)}日` : `${String(row.days_left)}日`}<small>{formatDateJst(row.expiration, '日付未取得')}</small></td>
          <td>{formatDateJst(row.last_contact, '記録なし')}</td><td>{row.mtg_status === 'outside' ? '記録なし（対象外）' : row.mtg_status === 'unavailable' ? '未取得' : formatDateJst(row.last_mtg, '記録なし')}</td></>}<td className="db-toggle-cell"><button type="button" className="db-expand" aria-label={`${row.name ?? '案件名未取得'}の詳しい数字を開く`} aria-expanded={expanded.includes(row.id)} aria-controls={`deal-numbers-${row.id}`} onClick={() => { setExpanded(ids => ids.includes(row.id) ? ids.filter(id => id !== row.id) : [...ids, row.id]); }}><span aria-hidden="true">{expanded.includes(row.id) ? '▾' : '▸'}</span></button></td>
        </tr>{expanded.includes(row.id) && <tr className="db-detail" id={`deal-numbers-${row.id}`}><td colSpan={firstOnly ? 9 : columns.length + 1}>{firstOnly && <p>金額 {yen(row.amount)} ／ 採用目標 {row.hires ?? '—'} / {row.target ?? '—'}人（{row.achievement === null ? '未取得' : `${String(Math.round(row.achievement * 100))}%`}） ／ <ReasonChips reasons={row.reasons} />満了日 {formatDateJst(row.expiration)} ／ 最後の接触 {formatDateJst(row.last_contact)} ／ 最後のMTG {formatDateJst(row.last_mtg)}</p>}<table aria-label={`${row.name ?? '案件名未取得'}の詳しい数字`}><thead><tr>{([['company', '顧客名'], ['stage', 'ステージ'], ['months', '何ヶ月目'], ['nps', '定期NPS'], ['applications', '応募'], ['interviews', '面接'], ['hires', '採用'], ['cpa', '採用単価']] as [DealSort, string][]).map(([key, label]) => <th key={key} scope="col" aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button type="button" onClick={() => { changeSort(key); }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ' ▲▼'}</button></th>)}</tr></thead><tbody><tr><td>{row.company ?? '—'}</td><td>{row.stage ?? '未取得'}</td><td>{row.days_left !== null && row.days_left < 0 ? '満了後' : row.months === null ? '—' : `${String(row.months)}ヶ月目`}</td><td>{row.nps ?? '—'}</td><td>{row.applications ?? '—'}</td><td>{row.interviews ?? '—'}</td><td>{row.hires ?? '—'}</td><td>{yen(row.cpa)}</td></tr></tbody></table></td></tr>}</Fragment>)}
        {!rows.length && <tr><td colSpan={firstOnly ? 9 : columns.length + 1}>条件に合う案件はありません。</td></tr>}
      </tbody></table></div></div>
      <nav className="db-pages" aria-label="案件のページ送り"><button className="jc-button" disabled={page <= 1} onClick={() => { updateJobCopyQuery({ deals_page: String(page - 1) }); }}>前の20件</button><span>{page} / {pages}</span><button className="jc-button" disabled={page >= pages} onClick={() => { updateJobCopyQuery({ deals_page: String(page + 1) }); }}>次の20件</button></nav>
    </>}
    {selected && <RiskDetail key={selected.id} row={{ ...detailRow(selected.id, selected.name ?? '案件名未登録', selected.company), consultant: selected.consultant ?? '', days_left: selected.days_left, mtg_last: selected.last_mtg }} onClose={() => { updateJobCopyQuery({ risk_deal: null }); }} onSaved={() => { setReload(n => n + 1); }} />}
  </main></div>;
}
