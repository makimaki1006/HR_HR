import { ReportFreshness } from './ReportFreshness';
import { RecentChangesButton, RecentChangesList, useRecentChanges } from './RecentChangesCard';
import type { ChangedDeal } from './RecentChangesCard';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { ListingPreparationNotice } from './ListingPreparationNotice';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useReportPolling } from './useReportPolling';
import { apiGet, AuthRequiredError } from '../../api/client';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
import { formatDateTimeJst, formatDateJst } from './format';
import { appsChange, customerName, dealCountLabel, filterCustomers, listingCountLabel, mediaName, sortCustomers, yen } from './homeModel';
import type { CardKey, CustomerSort, HomeCard, HomeCustomer, HomeResponse } from './homeModel';
import './home.css';
import { OwnerFilter, OwnerTodo, ownerParams } from './OwnerTodo';

const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '案件管理のホームを読み込めませんでした。時間を置いて再取得してください。';
const cardTone: Record<CardKey, string> = { zero_stay: 'alert', to_zero: 'alert', less: 'plain', more: 'plain', flat: 'plain', costly: 'warn', stale: 'alert' };
const customerColumns: [CustomerSort, string, boolean][] = [['name', '顧客名', false], ['deals', '契約中の案件 ／ 全案件', true], ['latest', '今の取引', false], ['live', '掲載中の求人 ／ 全求人', true], ['apps', '応募（前の 4 週 → 直近 4 週）', true], ['cost', '課金（直近 4 週）', true], ['cpa', '応募 1 件あたり', true]];

function CardList({ card, onOpenListing }: { card: HomeCard; onOpenListing: (id: string) => void }) {
  return <section className="jh-list" aria-label={`${card.title}の一覧`}>
    <h3>{card.title}（{card.count}件）</h3>
    {card.count === 0 ? <p>該当する求人はありません。</p> : <div className="jh-scroll"><table><thead><tr><th scope="col">求人</th><th scope="col">顧客</th><th scope="col">媒体</th><th scope="col" className="num">応募（前 → 直近）</th><th scope="col" className="num">課金（直近 4 週）</th><th scope="col">実績の最新の週</th></tr></thead><tbody>
      {card.listings.map(row => <tr key={row.id}><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenListing(row.id); }}>{row.title ?? '求人名未取得'}</button></th><td>{row.company ?? '—'}</td><td>{mediaName(row.media)}</td><td className="num">{appsChange(row.prev_apps, row.recent_apps)}</td><td className="num">{yen(row.recent_cost)}</td><td>{formatDateJst(row.latest_week_end, '実績なし')}</td></tr>)}
    </tbody></table></div>}
    {card.count > card.listings.length && <p className="jh-note">先頭の{card.listings.length}件だけを表示しています。</p>}
  </section>;
}

function saveHomeCache(cacheRef: { current: HomeResponse | null } | undefined, data: HomeResponse | null) {
  if (cacheRef) cacheRef.current = data;
}

/** 案件管理のホーム。まず見るものの件数と、顧客ごとの表。 */
export function JobCopyHome({ onOpenCompany, onOpenListing, onOpenDeal, cache }: { cache?: { current: HomeResponse | null }; onOpenCompany: (company: { id: string; name: string }) => void; onOpenListing: (id: string) => void; onOpenDeal: (deal: ChangedDeal) => void }) {
  const [data, setData] = useState<HomeResponse | null>(cache?.current ?? null);
  const [error, setError] = useState('');
  const [reload, setReload] = useState(0);
  const search = useJobCopySearch();
  const ownerQuery = ownerParams(search);
  const [resolvedSelf, setResolvedSelf] = useState<{ from: string; to: string } | null>(null);
  const requestOwnerQuery = resolvedSelf?.to === ownerQuery ? resolvedSelf.from : ownerQuery;
  const open = new URLSearchParams(search).get('card');
  const setOpen = (key: CardKey | 'recent_changes' | null) => { updateJobCopyQuery({ card: key, todo_page: null }, 'push'); };
  const recent = useRecentChanges(0, data?.status === 'ready', data?.status === 'ready' && !!data.work);
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<CustomerSort>('default');
  const [descending, setDescending] = useState(true);
  const [onlyActive, setOnlyActive] = useState(true);
  useReportPolling(useCallback(async signal => {
    if (reload === 0 && cache?.current?.status === 'ready' && !cache.current.work && !requestOwnerQuery) return false;
    const result = await apiGet<HomeResponse>(`/api/job-copy/home${requestOwnerQuery ? `?${requestOwnerQuery}` : ''}`, { signal, timeoutMs: 120_000 });
    if (signal.aborted) return false;
    if (!result.ok || !(result.data.status === 'preparing' || Array.isArray((result.data as { home?: { cards?: unknown } }).home?.cards))) {
      setError(result.ok ? '案件管理のホームの形式を確認できませんでした。時間を置いて再取得してください。' : failure(result.error));
      return false;
    }
    saveHomeCache(cache, result.data);
    setData(result.data); setError('');
    const work = result.data.status === 'ready' ? result.data.work : undefined;
    if (work?.status === 'ready' && !work.notice && [null, 'me'].includes(new URLSearchParams(requestOwnerQuery).get('owner'))) {
      const to = new URLSearchParams({ owner_field: work.owner_field, owner: work.owner }).toString();
      setResolvedSelf({ from: requestOwnerQuery, to });
      updateJobCopyQuery({ owner: work.owner, owner_field: work.owner_field }, 'replace');
    }
    return result.data.status === 'preparing' || work?.status === 'preparing' || (work?.status === 'ready' && (work.todos_status === 'preparing' || (work.todos_status !== 'failed' && !!work.refreshing)));
  }, [reload, cache, requestOwnerQuery]));
  useEffect(() => {
    const refresh = () => { saveHomeCache(cache, null); setReload(n => n + 1); };
    window.addEventListener('job-copy-plan-saved', refresh);
    return () => { window.removeEventListener('job-copy-plan-saved', refresh); };
  }, [cache]);
  const customers = useMemo<HomeCustomer[]>(() => data?.status === 'ready' ? sortCustomers(filterCustomers(data.home.customers, query, onlyActive), sort, descending) : [], [data, query, onlyActive, sort, descending]);
  const activeCount = data?.status === 'ready' ? data.home.customers.filter(customer => customer.active_deals > 0).length : 0;
  const ready = data?.status === 'ready' ? data : null;
  const work = ready?.work?.status === 'ready' ? ready.work : null;
  // 古いキャッシュや応答に残るカード、今の画面が知らないカードは表示しない。
  const cards = ready?.home.cards.filter(item => Object.hasOwn(cardTone, item.key)) ?? [];
  const card = cards.find(item => item.key === open);
  return <main className="jc-detail jh-home" id="job-details" tabIndex={-1}>
    <header className="jh-head"><div><h1>ホーム</h1><button type="button" className="jc-button" onClick={() => { updateJobCopyQuery({ view: 'deals', card: null }); }}>案件の一覧</button><p>顧客から案件、求人へ順にたどれます。予定のある案件と、確かめたほうがよい求人を確認できます。</p></div><button type="button" className="jc-button" onClick={() => { setReload(value => value + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">ホームを読み込んでいます…</p>}
    {data?.status === 'preparing' && <ListingPreparationNotice progress={data.progress} />}
    {data?.status === 'ready' && data.work?.status === 'preparing' && <section className="jh-todo"><h2>やること</h2><p role="status">担当者とやることを集計しています…</p></section>}
    {data?.status === 'ready' && data.work?.status === 'failed' && <p role="alert">担当者とやることの情報を読み込めませんでした。再読み込みしてください。</p>}
    {ready && <>
      {work && <><OwnerFilter work={work} /><OwnerTodo work={work} onOpenCompany={onOpenCompany} onOpenDeal={onOpenDeal} onShowList={() => { setReload(value => value + 1); }} /></>}
      <section aria-labelledby="jh-look"><h2 id="jh-look">まず見るもの</h2>
        <p className="jh-note">応募・課金のカードは掲載中の求人だけを数えています。応募・課金は媒体の週ごとの実績で、実績のない求人は増減に入りません。直近4週とその前の4週を比べ、{ready.rules.trend_min_diff}件以上、または{ready.rules.trend_min_percent}%以上の差を増減として数えます。応募がゼロのまま・ゼロになった求人は分けて表示します。{formatDateTimeJst(ready.index_built_at, '日時不明')}時点。効果の有無は判定していません。</p>
        <div className="jh-cards">{cards.map(item => <button type="button" key={item.key} className={`jh-card jh-${cardTone[item.key]}`} aria-pressed={open === item.key} onClick={() => { setOpen(open === item.key ? null : item.key); }}>
          <strong className="jh-count">{item.count}<small>件</small></strong>
          <span className="jh-card-title">{item.title}</span>
          <small className="jh-rule">{item.rule}</small>
        </button>)}<RecentChangesButton data={recent.data} open={open === 'recent_changes'} onClick={() => { setOpen(open === 'recent_changes' ? null : 'recent_changes'); }} /></div>
        {open !== 'recent_changes' && recent.data.status === 'ready' && <ReportFreshness builtAt={recent.data.built_at} stale={recent.data.stale} refreshing={recent.data.refreshing} />}
        {open === 'recent_changes' && <RecentChangesList data={recent.data} error={recent.error} onOpenDeal={onOpenDeal} onOpenListing={onOpenListing} />}
        {card && <CardList card={card} onOpenListing={onOpenListing} />}
      </section>
      <section aria-labelledby="jh-customers" className="jh-customers"><div className="jh-customers-head"><h2 id="jh-customers">顧客</h2>
        <div className="jh-chips" role="group" aria-label="顧客の絞り込み"><button type="button" className="jh-chip" aria-pressed={onlyActive} onClick={() => { setOnlyActive(true); }}>契約中の案件がある顧客<span>{activeCount}</span></button><button type="button" className="jh-chip" aria-pressed={!onlyActive} onClick={() => { setOnlyActive(false); }}>すべて<span>{ready.home.customers.length}</span></button></div>
        <label>顧客名で探す<input type="search" value={query} onChange={event => { setQuery(event.target.value); }} placeholder="顧客名の一部" /></label></div>
        {!ready.companies_ok && <p role="status">顧客の情報を読み込めなかったため、表を出せません。時間を置いて再読み込みしてください。</p>}
        <p className="jh-note">納品管理のパイプラインの案件だけを数えています（計上・商談などのパイプラインは含めません）。「契約中」は、案件のステージが完了（契約終了・失注など）でないものです。求人が結び付いた案件だけを数えています。応募・課金は掲載中の求人の合計です。応募は媒体の週ごとの実績で、直近 4 週とその前の 4 週を比べています。{ready.home.live_without_company > 0 ? `納品管理の案件に結び付いていない掲載中の求人が ${String(ready.home.live_without_company)} 件あり、この表には入っていません。` : ''}</p>
        <div className="jh-scroll"><table className="jh-customer-table"><thead><tr>{customerColumns.map(([key, label, numeric]) => <th key={key} scope="col" className={numeric ? 'num' : ''} aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button type="button" onClick={() => { if (sort === key) setDescending(!descending); else { setSort(key); setDescending(key !== 'name'); } }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ''}</button></th>)}<th scope="col"><span className="jc-visually-hidden">開く</span></th></tr></thead><tbody>
          {customers.map(customer => <tr key={customer.company_id}><th scope="row">{customerName(customer)}</th><td className="num">{dealCountLabel(customer)}</td><td className="jh-latest">{customer.latest_deal ? <><span className="jh-latest-name">{customer.latest_deal.name ?? '案件名未取得'}</span><small>{customer.latest_deal.stage_label ?? 'ステージ未取得'}{!customer.latest_deal.active && <span className="jh-badge">終了</span>} ・ 作成 {formatDateJst(customer.latest_deal.created_at, '—')}</small></> : '—'}</td><td className="num">{listingCountLabel(customer)}</td><td className="num">{appsChange(customer.prev_apps, customer.recent_apps)}</td><td className="num">{yen(customer.recent_cost)}</td><td className="num">{yen(customer.cost_per_app)}</td><td className="num"><button type="button" className="jc-button" aria-label={`${customerName(customer)}を開く`} onClick={() => { onOpenCompany({ id: customer.company_id, name: customerName(customer) }); }}>顧客を開く ›</button></td></tr>)}
          {!customers.length && <tr><td colSpan={8}>{query ? '条件に合う顧客はありません。' : onlyActive ? '契約中の案件がある顧客はありません。「すべて」に切り替えると終了した案件だけの顧客も表示します。' : '表示できる顧客がありません。'}</td></tr>}
        </tbody></table></div>
      </section>
    </>}
  </main>;
}
