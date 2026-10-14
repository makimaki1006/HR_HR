import { useEffect, useMemo, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
import { formatDateTimeJst } from './format';
import { appsChange, customerName, filterCustomers, mediaName, sortCustomers, yen } from './homeModel';
import type { CardKey, CustomerSort, HomeCard, HomeCustomer, HomeResponse } from './homeModel';
import './home.css';

const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '案件管理のホームを読み込めませんでした。時間を置いて再取得してください。';
const cardTone: Record<CardKey, string> = { more: 'plain', same: 'plain', less: 'plain', costly: 'warn', stale: 'alert', occupation: 'plain' };
const customerColumns: [CustomerSort, string, boolean][] = [['name', '顧客', false], ['deals', '案件', true], ['live', '掲載中の求人', true], ['apps', '応募（前の 4 週 → 直近 4 週）', true], ['cost', '課金（直近 4 週）', true], ['cpa', '応募 1 件あたり', true]];

function CardList({ card, onOpenListing }: { card: HomeCard; onOpenListing: (id: string) => void }) {
  return <section className="jh-list" aria-label={`${card.title}の一覧`}>
    <h3>{card.title}（{card.count}件）</h3>
    {card.count === 0 ? <p>該当する求人はありません。</p> : <div className="jh-scroll"><table><thead><tr><th scope="col">求人</th><th scope="col">顧客</th><th scope="col">媒体</th><th scope="col" className="num">応募（前 → 直近）</th><th scope="col" className="num">課金（直近 4 週）</th><th scope="col">実績の最新の週</th></tr></thead><tbody>
      {card.listings.map(row => <tr key={row.id}><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenListing(row.id); }}>{row.title ?? '求人名未取得'}</button></th><td>{row.company ?? '—'}</td><td>{mediaName(row.media)}</td><td className="num">{appsChange(row.prev_apps, row.recent_apps)}</td><td className="num">{yen(row.recent_cost)}</td><td>{row.latest_week_end ?? '実績なし'}</td></tr>)}
    </tbody></table></div>}
    {card.count > card.listings.length && <p className="jh-note">先頭の{card.listings.length}件だけを表示しています。</p>}
  </section>;
}

/** 案件管理のホーム。まず見るものの件数と、顧客ごとの表。 */
export function JobCopyHome({ onOpenCompany, onOpenListing }: { onOpenCompany: (company: { id: string; name: string }) => void; onOpenListing: (id: string) => void }) {
  const [data, setData] = useState<HomeResponse | null>(null);
  const [error, setError] = useState('');
  const [reload, setReload] = useState(0);
  const [open, setOpen] = useState<CardKey | null>(null);
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<CustomerSort>('apps');
  const [descending, setDescending] = useState(true);
  useEffect(() => {
    const controller = new AbortController();
    void apiGet<HomeResponse>('/api/job-copy/home', { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok && (result.data.status === 'preparing' || Array.isArray((result.data as { home?: { cards?: unknown } }).home?.cards))) { setData(result.data); setError(''); } else setError(result.ok ? '案件管理のホームの形式を確認できませんでした。時間を置いて再取得してください。' : failure(result.error));
    });
    return () => { controller.abort(); };
  }, [reload]);
  useEffect(() => {
    if (data?.status !== 'preparing') return;
    const timer = window.setTimeout(() => { setReload(value => value + 1); }, 5_000);
    return () => { window.clearTimeout(timer); };
  }, [data]);
  const customers = useMemo<HomeCustomer[]>(() => data?.status === 'ready' ? sortCustomers(filterCustomers(data.home.customers, query), sort, descending) : [], [data, query, sort, descending]);
  const ready = data?.status === 'ready' ? data : null;
  const card = ready?.home.cards.find(item => item.key === open);
  return <main className="jc-detail jh-home" id="job-details" tabIndex={-1}>
    <header className="jh-head"><div><h1>ホーム</h1><p>顧客から案件、求人へ順にたどれます。最初に、確かめたほうがよい求人の数を並べています。</p></div><button type="button" className="jc-button" onClick={() => { setReload(value => value + 1); }}>再読み込み</button></header>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">ホームを読み込んでいます…</p>}
    {data?.status === 'preparing' && <div className="jc-index-preparing" role="status"><strong>求人の一覧を準備しています</strong><p>準備が終わると自動で表示します。</p><progress aria-label="求人の一覧を準備中" /></div>}
    {ready && <>
      <section aria-labelledby="jh-look"><h2 id="jh-look">まず見るもの</h2>
        <p className="jh-note">掲載中の求人だけを数えています。応募・課金は媒体の週ごとの実績で、実績のない求人は増減に入りません。{formatDateTimeJst(ready.index_built_at, '日時不明')}時点。効果の有無は判定していません。</p>
        <div className="jh-cards">{ready.home.cards.map(item => <button type="button" key={item.key} className={`jh-card jh-${cardTone[item.key]}`} aria-pressed={open === item.key} onClick={() => { setOpen(open === item.key ? null : item.key); }}>
          <strong className="jh-count">{item.count}<small>件</small></strong>
          <span className="jh-card-title">{item.title}</span>
          <small className="jh-rule">{item.rule}</small>
        </button>)}</div>
        {card && <CardList card={card} onOpenListing={onOpenListing} />}
        {card?.key === 'occupation' && <p><a className="jc-button" href="?view=occupations">職種をまとめて指定する</a></p>}
      </section>
      <section aria-labelledby="jh-customers" className="jh-customers"><div className="jh-customers-head"><h2 id="jh-customers">顧客</h2>
        <label>顧客名で探す<input type="search" value={query} onChange={event => { setQuery(event.target.value); }} placeholder="顧客名の一部" /></label></div>
        {!ready.companies_ok && <p role="status">顧客の情報を読み込めなかったため、表を出せません。時間を置いて再読み込みしてください。</p>}
        <p className="jh-note">納品管理のパイプラインの案件だけを数えています（計上・商談などのパイプラインは含めません）。掲載中の求人の合計です。応募は媒体の週ごとの実績で、直近 4 週とその前の 4 週を比べています。{ready.home.live_without_company > 0 ? `納品管理の案件に結び付いていない掲載中の求人が ${String(ready.home.live_without_company)} 件あり、この表には入っていません。` : ''}</p>
        <div className="jh-scroll"><table className="jh-customer-table"><thead><tr>{customerColumns.map(([key, label, numeric]) => <th key={key} scope="col" className={numeric ? 'num' : ''} aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button type="button" onClick={() => { if (sort === key) setDescending(!descending); else { setSort(key); setDescending(key !== 'name'); } }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ''}</button></th>)}<th scope="col"><span className="jc-visually-hidden">開く</span></th></tr></thead><tbody>
          {customers.map(customer => <tr key={customer.company_id}><th scope="row">{customerName(customer)}</th><td className="num">{customer.deals}件</td><td className="num">{customer.live_listings}件</td><td className="num">{appsChange(customer.prev_apps, customer.recent_apps)}</td><td className="num">{yen(customer.recent_cost)}</td><td className="num">{yen(customer.cost_per_app)}</td><td className="num"><button type="button" className="jc-button" aria-label={`${customerName(customer)}を開く`} onClick={() => { onOpenCompany({ id: customer.company_id, name: customerName(customer) }); }}>顧客を開く ›</button></td></tr>)}
          {!customers.length && <tr><td colSpan={7}>{query ? '条件に合う顧客はありません。' : '表示できる顧客がありません。'}</td></tr>}
        </tbody></table></div>
      </section>
    </>}
  </main>;
}
