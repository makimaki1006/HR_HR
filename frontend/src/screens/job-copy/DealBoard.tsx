import { DealApplicationReasons } from './DealApplicationReasons';
import { ExpiryDays, RiskDetail, detailRow, jobsHref, useSideDetail } from './ConsultantRisk';
import { ReportFreshness } from './ReportFreshness';
import { Fragment, useEffect, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import { navigateJobCopy, rememberDealName, updateJobCopyQuery, useJobCopySearch } from './navigation';
import { OwnerFilter, ownerParams, signalCards } from './OwnerTodo';
import { Breadcrumbs } from './Breadcrumbs';
import { InfoTip } from './InfoTip';
import { ReasonChips, UnlinkedMtgLink } from './NextTodo';
import type { DealBoardReason } from '../../generated/DealBoardReason';
import { formatDateJst } from './format';
import { backoff, runWhenDue, usePreparingRetry } from '../../shared/preparingRetry';
import { yen } from './homeModel';
import { SortHeader, PageNav } from './SharedTable';
import { pageCount } from './sharedTableModel';
import { summarizeApplications, applicationChange, countDealReasons, dealSorts, sortDeals, launchGroups, launchLabel, sortLaunch, zeroPeriods, parseWeeks, periodRange, jstToday, isZeroApplicant, periodUnknown, hasNoInterview, interviewMissing } from './dealBoardModel';
import type { DealBoardResponse, DealSort, DealRow, ZeroPeriod } from './dealBoardModel';
import type { RiskRow } from './ConsultantRisk';
import './home.css';
import './deal-board.css';
import { recheckSelfOwner } from './selfOwner';
const PAGE_SIZE = 20;
// 案件一覧だけのタグ。「MTGが60日以上空いている」（60日以上）の手前の30〜59日を分けて数える。今日の画面には出さない。
const boardSignalCards: (readonly [string, string, string])[] = signalCards.flatMap((card): (readonly [string, string, string])[] => card[0] === 'mtg' ? [card, ['mtg_gap', 'MTGが30〜59日空いている', '最後のMTGから30〜59日']] : [card]);
// 一覧のタグは短い名前で出す。日数などの長い文は、指を乗せたときと「数字を見る」で見られる（日数の多くは右の列にもある）。
const shortReasonLabels: Record<DealBoardReason['kind'], string> = {
  expiration: '満了間近', expired: '満了後も稼働', contact: '接触が空いている', stalled: '応募が止まっている', first_contract: '初回の応募が少ない',
  nps: '満足度が低い', target: '採用目標の半分未満', cpa: '採用単価が高い', mtg: 'MTGが60日以上', mtg_gap: 'MTGが30〜59日',
};
// 急ぐタグは赤、それ以外は控えめな色にする。上の絞り込みのボタンも同じタグに赤い印を付ける。
const urgentKinds: ReadonlySet<string> = new Set<DealBoardReason['kind']>(['expired', 'mtg', 'stalled']);
function BoardReasonChips({ reasons }: { reasons: DealBoardReason[] }) {
  // 赤いタグを先に並べる（同じ色の中は元の順）。
  const ordered = [...reasons].sort((a, b) => Number(urgentKinds.has(b.kind)) - Number(urgentKinds.has(a.kind)));
  return <span className="db-reasons">{ordered.map(reason => <span className="db-reason-item" key={reason.kind}><span className={`db-reason${urgentKinds.has(reason.kind) ? ' db-reason-urgent' : ''}`} title={reason.text}>{shortReasonLabels[reason.kind]}</span>{reason.unlinked_mtg && <UnlinkedMtgLink />}</span>)}</span>;
}
function lastContactLabel(row: DealRow): string {
  if (row.contact_status === 'outside') return '記録なし（対象外）';
  if (row.contact_status === 'unavailable') return '不明';
  return formatDateJst(row.last_contact, '記録なし');
}
// 既定の 8 列。採用・採用単価・金額・採用目標は、行の「詳しい数字」を開くと見られる。
const columns: [DealSort, string][] = [['name', '案件・顧客'], ['reasons', 'タグ'], ['recent_applications', '応募（HubSpot・直近4週）'], ['days_left', '満了まで'], ['last_contact', '最後の接触'], ['last_mtg', '最後のMTG'], ['nps', '満足度'], ['consultant', '担当']];
function LaunchCells({ row, preparing }: { row: DealRow; preparing: boolean }) {
  const launch = row.launch;
  const missing = preparing ? '集計中' : '不明';
  const maximum = Math.max(1, ...(launch.weeks ?? []));
  return <>
    <td>{row.consultant ?? '未設定'}</td>
    <td>{formatDateJst(launch.start, '不明')}<small>{launch.elapsed === null ? '不明' : launch.elapsed < 0 ? '開始前' : `開始から${String(launch.elapsed)}日`}</small></td>
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
  const side = useSideDetail();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [moreRight, setMoreRight] = useState(false);
  const checkScroll = () => {
    const el = scrollRef.current;
    setMoreRight(!!el && el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  };
  const zeroOnly = query.get('deals_zero') === 'yes';
  const noInterview = query.get('deals_interview') === 'zero';
  const requestedPeriod = query.get('deals_period');
  const period: ZeroPeriod = zeroOnly && zeroPeriods.some(([key]) => key === requestedPeriod) ? requestedPeriod as ZeroPeriod : 'all';
  const weeks = parseWeeks(query.get('deals_weeks'));
  const periodFrom = query.get('deals_from'); const periodTo = query.get('deals_to');
  const range = periodRange(period, weeks, periodFrom, periodTo, jstToday());
  const rangeQuery = range ? `&period_from=${range.from}&period_to=${range.to}` : '';
  const firstOnly = query.get('deals_first') === 'yes' && !zeroOnly;
  const launchGroup = query.get('deals_launch');
  const sort = (dealSorts.some(([key]) => key === query.get('deals_sort')) ? query.get('deals_sort') : 'priority') as DealSort;
  const descending = query.get('deals_direction') !== 'asc'; const pipeline = query.get('deals_pipeline') ?? 'all'; const ended = query.get('deals_ended') === 'yes';
  useEffect(() => {
    const controller = new AbortController();
    void apiGet<DealBoardResponse>(`/api/job-copy/deals?${ownerQuery}${rangeQuery}`, { signal: controller.signal }).then(result => {
      if (controller.signal.aborted) return;
      if (!result.ok) { setError('案件の一覧を読み込めませんでした。時間を置いて再読み込みしてください。'); return; }
      setData(result.data); setError('');
      if (result.data.status === 'ready') recheckSelfOwner();
      if (result.data.status === 'ready' && !result.data.work.notice && [null, 'me'].includes(new URLSearchParams(ownerQuery).get('owner'))) updateJobCopyQuery({ owner: result.data.work.owner, owner_field: result.data.work.owner_field }, 'replace');
    });
    return () => { controller.abort(); };
  }, [ownerQuery, rangeQuery, reload]);
  usePreparingRetry(data?.status === 'preparing' || (data?.status === 'ready' && data.refreshing), 2_000, data, () => { setReload(n => n + 1); }, `${ownerQuery}${rangeQuery}`);
  const ready = data?.status === 'ready' ? data : null;
  const isReady = !!ready;
  // 「いい」のタグは、いい案件の集計（応募が増えた・採用単価が中央値より低い・MTGで前向きな話）を使う。取れなければタグだけ件数なしにする。
  const [goodState, setGood] = useState<{ key: string; value: Map<string, NonNullable<RiskRow['good_flags']>> | 'failed' } | null>(null);
  const good = goodState?.key === ownerQuery ? goodState.value : null;
  useEffect(() => {
    if (!isReady) return;
    const controller = new AbortController(); const wait = backoff(3_000);
    const load = async () => {
      const result = await apiGet<{ status: string; rows?: RiskRow[] }>(`/api/job-copy/consultant-good?${ownerQuery}`, { signal: controller.signal, timeoutMs: 120_000 });
      if (controller.signal.aborted) return;
      if (!result.ok || result.data.status === 'failed') { setGood({ key: ownerQuery, value: 'failed' }); return; }
      if (result.data.status !== 'ready') { runWhenDue(controller.signal, wait(), () => { void load(); }); return; }
      setGood({ key: ownerQuery, value: new Map((result.data.rows ?? []).map(row => [row.deal_id, row.good_flags ?? []] as const)) });
    };
    void load();
    return () => { controller.abort(); };
  }, [isReady, ownerQuery]);
  const goodIds = good instanceof Map ? good : null;
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
  const scoped = ready?.rows.filter(row => (!firstOnly || row.first_contract) && (!zeroOnly || isZeroApplicant(row, period)) && (!noInterview || hasNoInterview(row)) && (ended || row.active) && (pipeline === 'all' || row.pipeline === pipeline)) ?? [];
  const counts = countDealReasons(scoped);
  const goodCount = goodIds ? scoped.filter(row => goodIds.has(row.id)).length : null;
  const marked = scoped.filter(row => !reason || (reason === 'good' ? !!goodIds?.has(row.id) : row.reasons.some(item => item.kind === reason)));
  const interviewMissingCount = ready?.rows.filter(row => (ended || row.active) && (pipeline === 'all' || row.pipeline === pipeline) && (!firstOnly || row.first_contract) && interviewMissing(row)).length ?? 0;
  const periodUnknownCount = zeroOnly && period !== 'all' ? (ready?.rows.filter(row => (ended || row.active) && (pipeline === 'all' || row.pipeline === pipeline) && periodUnknown(row, period)).length ?? 0) : 0;
  const periodLabel = period === 'contract' ? '初回契約の期間中' : range ? `${formatDateJst(range.from)}〜${formatDateJst(range.to)}` : '';
  const filtered = marked.filter(row => !firstOnly || !launchGroup || row.launch.status === launchGroup);
  const rows = firstOnly && sort === 'priority' ? sortLaunch(filtered) : sortDeals(filtered, sort, descending);
  const applications = summarizeApplications(filtered);
  const selected = ready?.rows.find(row => row.id === query.get('risk_deal'));
  const preparing = ready?.applications_status === 'preparing';
  const summaryPending = preparing ? '集計中' : '不明';
  const pages = pageCount(rows.length, PAGE_SIZE);
  const requested = query.get('deals_page') ?? '1';
  const page = Math.min(pages, /^[1-9]\d{0,5}$/.test(requested) ? Number(requested) : 1);
  const change = (values: Record<string, string | null>) => { updateJobCopyQuery({ ...values, deals_page: null }, 'replace'); };
  const changeSort = (key: DealSort) => { change({ deals_sort: key, deals_direction: sort === key && descending ? 'asc' : 'desc' }); };
  return <div className={selected && side ? 'jc-page cr-split' : 'jc-page'}><main className="jc-detail jh-home db-board" id="job-details">
    <Breadcrumbs crumbs={[{ level: 'home', label: '案件管理', current: false }, { level: 'deal', label: '案件の一覧', current: true }]} onSelect={() => { updateJobCopyQuery({ view: null }); }} />
    <header className="jh-head"><div><h1>案件の一覧</h1>{firstOnly && <p>初回案件の応募の立ち上がりを確認できます。</p>}</div><button className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{error}</p>}
    {(!data || data.status === 'preparing') && <p role="status">案件を集計しています…</p>}
    {data?.status === 'failed' && <p role="alert">案件を読み込めませんでした。再読み込みしてください。</p>}
    {ready && <>
      <ReportFreshness builtAt={ready.facts_built_at} stale={ready.facts_stale} refreshing={ready.refreshing} />
      <div className="db-filters"><OwnerFilter work={ready.work} /><label>パイプライン<select value={pipeline} onChange={event => { change({ deals_pipeline: event.target.value }); }}><option value="all">すべて</option><option value="21596025">リクロジ_納品管理</option><option value="743961896">請負契約_納品管理</option></select></label>
        {zeroOnly && <fieldset className="db-period"><legend>期間を指定</legend>
          <select aria-label="応募ゼロを調べる期間" value={period} onChange={event => { change({ deals_period: event.target.value === 'all' ? null : event.target.value }); }}>{zeroPeriods.map(([key, label]) => <option key={key} value={key}>{label}</option>)}</select>
          {period === 'weeks' && <label>直近<input type="number" min={1} max={104} value={weeks} onChange={event => { change({ deals_weeks: event.target.value }); }} />週</label>}
          {period === 'dates' && <><label>から<input type="date" value={periodFrom ?? ''} onChange={event => { change({ deals_from: event.target.value || null }); }} /></label><label>まで<input type="date" value={periodTo ?? ''} onChange={event => { change({ deals_to: event.target.value || null }); }} /></label></>}
        </fieldset>}
        <label><input type="checkbox" checked={noInterview} onChange={event => { change({ deals_interview: event.target.checked ? 'zero' : null }); }} />面接が1件も無い案件だけ</label>
        <label><input type="checkbox" checked={ended} onChange={event => { change({ deals_ended: event.target.checked ? 'yes' : null }); }} />終わった案件も出す</label>
      </div>
      {/* 表の列が変わる切り替え。押した方に色が付く（今までのチェックの箱と同じ動き）。 */}
      <div className="db-view-switch" role="group" aria-label="表示を切り替える"><span className="db-view-switch-label">表示を切り替える：</span>
        <button type="button" aria-pressed={!firstOnly && !zeroOnly} onClick={() => { if (firstOnly || zeroOnly) change({ deals_first: null, deals_zero: null, deals_period: null, deals_weeks: null, deals_from: null, deals_to: null, deals_launch: null, deals_sort: null, deals_direction: null }); }}>ふつう</button>
        <button type="button" aria-pressed={firstOnly} onClick={() => { if (!firstOnly) change({ deals_first: 'yes', deals_zero: null, deals_period: null, deals_weeks: null, deals_from: null, deals_to: null, deals_launch: null, deals_sort: null, deals_direction: null }); }}>初回案件の立ち上がり</button>
        <button type="button" aria-pressed={zeroOnly} onClick={() => { if (!zeroOnly) change({ deals_zero: 'yes', deals_period: null, deals_weeks: null, deals_from: null, deals_to: null, deals_first: null, deals_launch: null, deals_reason: null, deals_sort: null, deals_direction: null }); }}>応募ゼロの案件</button>
      </div>
      {ready.refresh_failed && !ready.facts_stale && <p role="status">更新できなかったため、前に取得した値を表示しています。</p>}
      <InfoTip className="jc-infotip-left db-rules" iconFirst label="数え方"><p>最後の接触はMTGか60秒を超える電話です（メールは数えません）。最後の接触・最後のMTG・MTGのタグ：{ready.sheet_as_of ? `${formatDateJst(ready.sheet_as_of)} 時点（前日の集計）` : '集計した日が分かりません'}。請負契約は接触とMTGの集計対象外です。</p><p>応募：HubSpot の応募を応募日で数えています。応募日が無いときは作成日を使います。日本時間の今日を含む28日間と、その前の28日間を比べています。同じ応募は1回だけ数え、同じ人が同じ求人にもう一度応募したときは最初の1件だけを数えます。</p><p>タグは記録から自動で付き、条件が解消すると消えます。</p><ul>{boardSignalCards.map(([kind, label, rule]) => <li key={kind}>{label}：{rule}</li>)}</ul></InfoTip>
      {zeroOnly ? <><h2 className="db-zero-heading">応募が1件も無い案件{period !== 'all' && periodLabel && `（${periodLabel}）`} <strong>{scoped.length}件</strong></h2><p className="jh-note">{period === 'all' ? '求人票が案件に付いていて、その求人票への応募が全期間で1件もありません。' : period === 'contract' ? '初回契約の案件で、契約の開始日から満了日までの応募日が1件もありません。' : range ? `求人票が案件に付いていて、${periodLabel}の応募日が1件もありません。` : '期間の開始日と終了日を選んでください。'}{preparing ? '応募を集計しているため、件数は増える場合があります。' : '約60分ごとに更新します。'}求人票が付いていない案件は含みません。{periodUnknownCount > 0 && `応募日も作成日も分からない応募がある${String(periodUnknownCount)}件は、期間の応募数を数えられないため含みません。`}</p></> : <div className="db-reason-filters" role="group" aria-label="タグで絞る">
        <button type="button" aria-pressed={!reason} onClick={() => { change({ deals_reason: null }); }}>すべて <strong>{scoped.length}</strong></button>
        <button type="button" className={goodCount === 0 ? 'db-zero' : ''} aria-pressed={reason === 'good'} disabled={good === 'failed'} title={good === 'failed' ? 'いい案件の集計を読み込めませんでした。' : '応募が増えた・採用単価が同じ区分の中央値より低い・MTGで前向きな話があった案件'} onClick={() => { change({ deals_reason: reason === 'good' ? null : 'good' }); }}>順調 <strong>{goodCount ?? '—'}</strong></button>
        {boardSignalCards.map(([kind, label]) => <button type="button" key={kind} className={[(counts[kind] ?? 0) === 0 ? 'db-zero' : '', urgentKinds.has(kind) ? 'db-filter-urgent' : ''].filter(Boolean).join(' ')} aria-pressed={reason === kind} onClick={() => { change({ deals_reason: reason === kind ? null : kind }); }}>{label} <strong>{counts[kind] ?? 0}</strong></button>)}
      </div>}
      {noInterview && <p className="jh-note">面接の数が0の案件だけを出しています。面接の数が入っていない{String(interviewMissingCount)}件は、0件とは別に数えて含めていません。HubSpotに記録された分だけです。</p>}
      {firstOnly && <><div className="db-reason-filters db-launch-summary" role="group" aria-label="初回案件の応募で絞る"><button type="button" aria-pressed={!launchGroup} onClick={() => { change({ deals_launch: null }); }}>初回案件 <strong>{marked.length}件</strong>：</button>{launchGroups.map(([key, label]) => <button type="button" key={key} aria-pressed={launchGroup === key} onClick={() => { change({ deals_launch: launchGroup === key ? null : key }); }}>{label} <strong>{marked.filter(row => row.launch.status === key).length}</strong></button>)}</div><p className="jh-note">応募は契約開始日から今日まで（日本時間）。約60分ごとに更新します。棒は開始日から7日ごと・最初の8週で、始まっていない週は表示しません。応募日も作成日も分からないときは「不明」と出します（0件とは数えません）。</p></>}
      <section className="db-application-summary" aria-label="絞り込み中の応募数">
        <h2>応募のまとめ</h2>
        <p>直近4週 <strong>{applications.missing ? summaryPending : `${String(applications.recent)}件`}</strong> ／ 前の4週 <strong>{applications.missing ? summaryPending : `${String(applications.previous)}件`}</strong> ／ 差 <strong>{applications.missing ? summaryPending : applicationChange(applications.change)}</strong></p>
        {(() => { const left = [applications.unknown > 0 ? `日付不明の応募 ${String(applications.unknown)}件` : '', applications.outOfScope > 0 ? `満了から8週を過ぎた終了済みの案件 ${String(applications.outOfScope)}件` : ''].filter(text => text !== ''); return left.length > 0 && <p className="jh-note">含めていないもの：{left.join('・')}</p>; })()}
        {applications.missing > 0 && !preparing && <p className="jh-note">応募数が分からない案件が {String(applications.missing)} 件あるため、合計は出していません。</p>}
      </section>
      <div className="db-list-meta"><p aria-live="polite">{rows.length}件・{page} / {pages}ページ</p><p className="jh-note">並び: {sort === 'priority' ? firstOnly ? '目安より少ない → 開始から日数の長い順' : 'タグが多い順' : `${dealSorts.find(([key]) => key === sort)?.[1] ?? ''}の${descending ? '多い・新しい順' : '少ない・古い順'}`} {sort !== 'priority' && <button type="button" className="jh-link" onClick={() => { change({ deals_sort: null, deals_direction: null }); }}>標準の並びに戻す</button>}</p></div>
      <div className={`db-scroll-frame${moreRight ? ' db-more-right' : ''}`}><div className="db-board-scroll" ref={scrollRef} onScroll={checkScroll} tabIndex={0} role="region" aria-label="案件一覧（横スクロール）"><table className={`db-table${firstOnly ? ' db-launch-table' : ''}`}><thead><tr>{(firstOnly || zeroOnly ? columns.slice(0, 1) : columns).map(([key, label]) => <SortHeader key={key} label={label} active={sort === key} descending={descending} onSort={() => { changeSort(key); }} />)}{zeroOnly && ['担当', '契約期間', 'ステージ', '求人票の数', 'HubSpot'].map(label => <th scope="col" key={label}>{label}</th>)}{firstOnly && ['担当', '契約開始日', '開始からの応募数', '目安との比較', '最初の応募', '週ごとの応募', '掲載中の求人数'].map(label => <th scope="col" key={label}>{label}</th>)}<th scope="col" className="db-toggle-column">詳しい数字・詳細</th></tr></thead><tbody>
        {rows.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE).map(row => <Fragment key={row.id}><tr className={row.id === selected?.id ? 'cr-row-open' : undefined} onClick={event => { if (!(event.target as HTMLElement).closest('button, a')) updateJobCopyQuery({ risk_deal: row.id }); }}>
          <th scope="row"><a className="jc-row-name" title={row.name ?? '案件名なし'} href={jobsHref(search, row.id)} onClick={event => { event.preventDefault(); rememberDealName(row.id, row.name); navigateJobCopy(new URL(event.currentTarget.href)); }}>{row.name ?? '案件名なし'}</a><small title={row.company ?? '顧客名なし'}>{row.company ?? '顧客名なし'}</small><DealApplicationReasons value={row.application_reasons} outOfScope={row.applications_out_of_scope} /></th>
          {zeroOnly ? <><td>{row.consultant ?? '未設定'}</td><td>{formatDateJst(row.launch.start, '開始日不明')} 〜 {formatDateJst(row.expiration, '満了日不明')}</td><td>{row.stage ?? '不明'}</td><td>{row.listing_count === null ? '不明' : `${String(row.listing_count)}件`}</td><td><a href={`https://app.hubspot.com/contacts/${ready.hubspot_portal_id}/record/0-3/${row.id}`} target="_blank" rel="noreferrer">HubSpotで開く</a></td></> : firstOnly ? <LaunchCells row={row} preparing={preparing} /> : <><td><BoardReasonChips reasons={row.reasons} />{!row.reasons.length && '—'}</td>
          <td className="db-nowrap">{row.applications_out_of_scope ? '対象外' : <>{row.recent_applications ?? (preparing ? '集計中' : '不明')}<small>前の4週 {row.previous_applications === null ? '不明' : `${String(row.previous_applications)}件`}</small><small>差 {applicationChange(row.applications_change)}</small>{!!row.unknown_application_ids?.length && <small>日付不明 {row.unknown_application_ids.length}件を除外</small>}</>}</td>
          <td className="db-nowrap"><ExpiryDays days={row.days_left} empty="—" /><small>{formatDateJst(row.expiration, '日付不明')}</small></td>
          <td className="db-nowrap">{lastContactLabel(row)}{row.last_email && <small>最後のメール {formatDateJst(row.last_email)}</small>}</td><td className="db-nowrap">{row.mtg_status === 'outside' ? '記録なし（対象外）' : row.mtg_status === 'unavailable' ? '不明' : formatDateJst(row.last_mtg, '記録なし')}</td><td>{row.nps ?? '—'}</td><td>{row.consultant ?? '未設定'}</td></>}<td className="db-toggle-cell"><button type="button" className="jc-open db-expand" aria-label={`${row.name ?? '案件名なし'}の詳しい数字を開く`} aria-expanded={expanded.includes(row.id)} aria-controls={`deal-numbers-${row.id}`} onClick={() => { setExpanded(ids => ids.includes(row.id) ? ids.filter(id => id !== row.id) : [...ids, row.id]); }}>{expanded.includes(row.id) ? '閉じる ▴' : '数字を見る ▾'}</button><button type="button" className="jc-open db-open-detail" aria-label={`${row.name ?? '案件名なし'}の詳細を開く`} onClick={() => { updateJobCopyQuery({ risk_deal: row.id }); }}>詳細 ▸</button></td>
        </tr>{expanded.includes(row.id) && <tr className="db-detail" id={`deal-numbers-${row.id}`}><td colSpan={zeroOnly ? 7 : firstOnly ? 9 : columns.length + 1}>{firstOnly && <p>金額 {yen(row.amount)} ／ 採用目標 {row.hires ?? '—'} / {row.target ?? '—'}人（{row.achievement === null ? '不明' : `${String(Math.round(row.achievement * 100))}%`}） ／ <ReasonChips reasons={row.reasons} />満了日 {formatDateJst(row.expiration)} ／ 最後の接触 {lastContactLabel(row)} ／ 最後のMTG {formatDateJst(row.last_mtg)}</p>}{!firstOnly && !zeroOnly && row.reasons.length > 0 && <p className="db-detail-reasons">タグの内容：<ReasonChips reasons={row.reasons} /></p>}<p>媒体の応募（直近4週） {row.media_recent_applications === null ? '不明' : `${String(row.media_recent_applications)}件`}</p><table aria-label={`${row.name ?? '案件名なし'}の詳しい数字`}><thead><tr>{([['company', '顧客名'], ['stage', 'ステージ'], ['months', '何ヶ月目'], ['nps', '定期の満足度'], ['applications', '案件に記録された応募（累計）'], ['interviews', '面接'], ['hires', '採用'], ['cpa', '採用単価'], ['amount', '金額'], ['achievement', '採用目標に対して']] as [DealSort, string][]).map(([key, label]) => <SortHeader key={key} label={label} active={sort === key} descending={descending} onSort={() => { changeSort(key); }} />)}</tr></thead><tbody><tr><td>{row.company ?? '—'}</td><td>{row.stage ?? '不明'}</td><td>{row.days_left !== null && row.days_left < 0 ? '満了後' : row.months === null ? '—' : `${String(row.months)}ヶ月目`}</td><td>{row.nps ?? '—'}</td><td>{row.applications ?? '—'}</td><td>{row.interviews ?? '—'}</td><td>{row.hires ?? '—'}</td><td>{yen(row.cpa)}</td><td>{yen(row.amount)}</td><td>{row.achievement === null ? '—' : `${String(Math.round(row.achievement * 100))}%`}<small>{row.hires ?? '—'} / {row.target ?? '—'}人</small></td></tr></tbody></table><DealApplicationReasons value={row.application_reasons} outOfScope={row.applications_out_of_scope} details /></td></tr>}</Fragment>)}
        {!rows.length && <tr><td colSpan={zeroOnly ? 7 : firstOnly ? 9 : columns.length + 1}>条件に合う案件はありません。</td></tr>}
      </tbody></table></div></div>
      <PageNav className="db-pages" label="案件のページ送り" size={PAGE_SIZE} hasPrev={page > 1} hasNext={page < pages} onPrev={() => { updateJobCopyQuery({ deals_page: String(page - 1) }); }} onNext={() => { updateJobCopyQuery({ deals_page: String(page + 1) }); }}>{page} / {pages}</PageNav>
    </>}
  </main>{selected && <RiskDetail key={`${selected.id}-${String(side)}`} side={side} row={{ ...detailRow(selected.id, selected.name ?? '案件名未登録', selected.company), consultant: selected.consultant ?? '', days_left: selected.days_left, mtg_last: selected.last_mtg, list_contact: { date: selected.last_contact, status: selected.contact_status }, ...(reason === 'good' && goodIds?.get(selected.id) ? { good_flags: goodIds.get(selected.id) ?? [] } : {}) }} onClose={() => { updateJobCopyQuery({ risk_deal: null }); }} onSaved={() => { setReload(n => n + 1); }} />}</div>;
}
