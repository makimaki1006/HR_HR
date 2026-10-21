import { useEffect, useId, useState } from 'react';
import { apiGet } from '../../api/client';
import type { CompanyDeal } from './companyDeals';
import { dealName } from './companyDeals';
import type { Billing, SummarySource } from './DealSummary';
import { loadCompanyDatas } from './dealSummaryLoad';
import { companyListingPeriod, companyListingsModel } from './companyListingsModel';
import { mediaLabel, publicationLabel } from './hubspotListings';
import type { HubSpotListing, HubSpotListingPage } from './hubspotListings';
import { formatDateTimeJst } from './format';
import { formatDay } from './timelineModel';
import { yen } from './homeModel';
import { ownerParams } from './OwnerTodo';
import { useJobCopySearch } from './navigation';
import { PageNav } from './SharedTable';
import { paginate } from './sharedTableModel';

const PAGE_SIZE = 20;
/** 全求人の一覧は索引だけを読む。応募・課金は従来の読み取りを共用し、追加取得しない。 */
async function loadRows(deals: readonly CompanyDeal[], ownerQuery: string, signal: AbortSignal): Promise<HubSpotListing[]> {
  // 求人がない案件は索引の候補に存在しない。API の案件指定は20件まで。
  const wanted = deals.filter(deal => deal.listings_total !== 0);
  const rows: HubSpotListing[] = [];
  for (let i = 0; i < wanted.length; i += 20) {
    const ids = wanted.slice(i, i + 20).map(deal => deal.id).join(',');
    let offset: number | null = 0;
    while (offset !== null) {
      const params = new URLSearchParams(ownerQuery);
      params.set('deal', ids); params.set('offset', String(offset)); params.set('sort', 'title'); params.set('descending', 'false');
      const result = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${params.toString()}`, { signal, timeoutMs: 120_000 });
      if (!result.ok || result.data.status !== 'ready') throw new Error('unavailable');
      rows.push(...result.data.listings);
      const next: number | null = result.data.next_offset;
      if (next !== null && next <= offset) throw new Error('invalid pagination');
      offset = next;
    }
  }
  return rows;
}

export function CompanyListings({ companyId, deals, billingByJob, onOpenDeal, onOpenListing }: {
  companyId: string; deals: CompanyDeal[]; billingByJob: Billing;
  onOpenDeal: (deal: { id: string; name: string }) => void;
  onOpenListing: (row: HubSpotListing) => void;
}) {
  const liveId = useId();
  const ownerQuery = ownerParams(useJobCopySearch()) || 'owner=me';
  const [rows, setRows] = useState<HubSpotListing[] | null>(null);
  const [error, setError] = useState(false);
  const [loadedQuery, setLoadedQuery] = useState('');
  const [source, setSource] = useState<SummarySource>({ status: 'loading' });
  const [livePage, setLivePage] = useState(0);
  const [pastPage, setPastPage] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    void loadRows(deals, ownerQuery, controller.signal).then(rows => { if (!controller.signal.aborted) { setRows(rows); setLoadedQuery(ownerQuery); setError(false); setLivePage(0); setPastPage(0); } }).catch(() => { if (!controller.signal.aborted) { setError(true); setLoadedQuery(ownerQuery); } });
    void loadCompanyDatas(companyId, controller.signal, deals).then(source => { if (!controller.signal.aborted) setSource(source); });
    return () => { controller.abort(); };
  }, [companyId, deals, ownerQuery]);
  if (error && loadedQuery === ownerQuery) return <p role="alert">求人を読み込めませんでした。時間を置いて画面を開き直してください。</p>;
  if (!rows || loadedQuery !== ownerQuery) return <p role="status">求人を読み込んでいます…</p>;
  const model = companyListingsModel(rows, source.status === 'ready' ? source.datas : [], deals, billingByJob);
  const table = (items: typeof model.live, page: number, setPage: (page: number) => void, label: string) => { const view = paginate(items, page, PAGE_SIZE); return <>
    {items.length ? <div className="jh-scroll"><table className="jh-listing-table" aria-label={`${label}の求人`}><thead><tr>
      <th scope="col">求人名</th><th scope="col">媒体</th><th scope="col">掲載期間</th><th scope="col" className="num">直近4週の応募</th><th scope="col" className="num">今の課金（4週）</th><th scope="col">最後の変更</th>
    </tr></thead><tbody>{view.items.map(item => <tr key={item.row.id} onClick={event => { if (!(event.target as HTMLElement).closest('button, a')) onOpenListing(item.row); }}>
      <th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenListing(item.row); }}>{item.row.title ?? '求人名未取得'}</button><div className="jh-listing-deals">{item.deals.map(deal => <small key={deal.id}><button type="button" className="jh-link" onClick={() => { onOpenDeal({ id: deal.id, name: dealName(deal) }); }}>{dealName(deal)}</button> · {deal.stage_label ?? 'ステージ不明'}</small>)}</div></th>
      <td>{mediaLabel(item.row.media)}</td><td>{companyListingPeriod(item.row)}{!item.live && publicationLabel(item.row.publication_status) !== '掲載終了' && <small>掲載状況：{publicationLabel(item.row.publication_status)}</small>}</td>
      <td className="num">{item.numbers?.recent == null ? '—' : `${String(item.numbers.recent)}件`}</td><td className="num">{yen(item.numbers?.billing ?? null)}</td>
      <td>{item.changedAt ? <>本文を変更<small>{formatDateTimeJst(item.changedAt, '日時不明')}</small></> : '変更日不明'}</td>
    </tr>)}</tbody></table></div> : <p>{label}の求人はありません。</p>}
    {items.length > PAGE_SIZE && <PageNav className="jh-pages" label={`${label}のページ`} size={PAGE_SIZE} hasPrev={view.pageIndex > 0} hasNext={view.pageIndex + 1 < view.pages} onPrev={() => { setPage(page - 1); }} onNext={() => { setPage(page + 1); }}>{String(view.from)}〜{String(view.to)} / {String(items.length)}件</PageNav>}
  </>; };
  return <>
    {source.status === 'loading' && <p role="status">応募・課金を読み込んでいます…</p>}
    {source.status === 'error' && <p role="alert">応募・課金を読み込めませんでした。求人は表示しています。</p>}
    {source.status === 'ready' && source.notes?.map(note => <p key={note} className="jh-note">応募・課金：{note}</p>)}
    {source.status === 'ready' && source.datas.some(data => data.capped || data.rows.length > 30) && <p className="jh-note">応募・課金は各案件で取得できた求人の分だけ表示しています。</p>}
    <p className="jh-note">納品管理の求人を本文の変更が新しい順に表示。案件名から求人を比較できます。<br />応募・課金は {formatDay(model.asOf)} までの4週間（未取得は「—」）。最後の変更は本文のみ。</p>
    <section className="jh-live-listings" aria-labelledby={liveId}><h2 id={liveId}>掲載中 <span>{String(model.live.length)}件</span></h2>{table(model.live, livePage, setLivePage, '掲載中')}</section>
    <details className="jh-past-listings"><summary>過去（掲載終了） {String(model.past.length)}件</summary><p className="jh-note">掲載前・掲載状況不明の求人もこちらに表示しています。</p>{table(model.past, pastPage, setPastPage, '過去')}</details>
  </>;
}
