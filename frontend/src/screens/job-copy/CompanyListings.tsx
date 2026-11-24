import { useEffect, useId, useState } from 'react';
import { apiGet } from '../../api/client';
import type { CompanyDeal } from './companyDeals';
import { dealName } from './companyDeals';
import type { Billing, SummarySource } from './DealSummary';
import { loadCompanyDatas } from './dealSummaryLoad';
import type { DealData } from './dealSummaryLoad';
import { FLAG_LABELS, LISTING_SORTS, companyListingPeriod, companyListingsModel, flagNumbersLabel, perApplicationLabel } from './companyListingsModel';
import type { ListingFlagCard, ListingFlagNumbers, ListingFlags, ListingFlagsResponse, ListingSort } from './companyListingsModel';
import { mediaLabel, publicationLabel } from './hubspotListings';
import type { HubSpotListing, HubSpotListingPage } from './hubspotListings';
import { formatDateTimeJst } from './format';
import { formatDay } from './timelineModel';
import { appsChange, yen } from './homeModel';
import { ownerParams } from './OwnerTodo';
import { useJobCopySearch } from './navigation';
import { PageNav } from './SharedTable';
import { paginate } from './sharedTableModel';
import { InfoTip } from './InfoTip';
import './status-filter.css';
import './company-listings.css';

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
      const result = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${params.toString()}`, { signal, timeoutMs: 60_000 });
      if (!result.ok || result.data.status !== 'ready') throw new Error('unavailable');
      rows.push(...result.data.listings);
      const next: number | null = result.data.next_offset;
      if (next !== null && next <= offset) throw new Error('invalid pagination');
      offset = next;
    }
  }
  return rows;
}

type FlagState = { status: 'loading' | 'preparing' | 'error' } | { status: 'ready'; flags: ListingFlags; numbers: ListingFlagNumbers; cards: ListingFlagCard[] };
/** ホームと同じタグを、掲載中の求人について 200 件ずつ読む。読めなくても求人の表は出す。 */
async function loadFlags(ids: readonly string[], ownerQuery: string, signal: AbortSignal): Promise<FlagState> {
  const flags: Record<string, readonly string[]> = {};
  const numbers: Record<string, ListingFlagNumbers[string]> = {};
  let cards: ListingFlagCard[] = [];
  for (let i = 0; i < ids.length; i += 200) {
    const params = new URLSearchParams(ownerQuery);
    params.set('ids', ids.slice(i, i + 200).join(','));
    const result = await apiGet<ListingFlagsResponse>(`/api/job-copy/listing-flags?${params.toString()}`, { signal });
    if (!result.ok || result.data.status === 'failed') return { status: 'error' };
    if (result.data.status !== 'ready') return { status: 'preparing' };
    // 古い応答や形の違う応答で画面を壊さない。
    const data = result.data as Partial<Extract<ListingFlagsResponse, { status: 'ready' }>>;
    if (!data.flags || typeof data.flags !== 'object' || !Array.isArray(data.cards)) return { status: 'error' };
    Object.assign(flags, data.flags);
    Object.assign(numbers, data.numbers ?? {});
    cards = data.cards;
  }
  return { status: 'ready', flags, numbers, cards };
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
  const [partial, setPartial] = useState<DealData[]>([]);
  const [reload, setReload] = useState(0);
  const [livePage, setLivePage] = useState(0);
  const [pastPage, setPastPage] = useState(0);
  const [sort, setSort] = useState<ListingSort>('attention');
  const [flagState, setFlagState] = useState<FlagState>({ status: 'loading' });
  useEffect(() => {
    const controller = new AbortController();
    setFlagState({ status: 'loading' });
    void loadRows(deals, ownerQuery, controller.signal).then(rows => {
      if (controller.signal.aborted) return;
      setRows(rows); setLoadedQuery(ownerQuery); setError(false); setLivePage(0); setPastPage(0);
      const live = [...new Set(rows.filter(row => publicationLabel(row.publication_status) === '掲載中').map(row => row.id))];
      if (!live.length) { setFlagState({ status: 'ready', flags: {}, numbers: {}, cards: [] }); return; }
      void loadFlags(live, ownerQuery, controller.signal).then(state => { if (!controller.signal.aborted) setFlagState(state); }).catch(() => { if (!controller.signal.aborted) setFlagState({ status: 'error' }); });
    }).catch(() => { if (!controller.signal.aborted) { setError(true); setLoadedQuery(ownerQuery); } });
    setSource({ status: 'loading' }); setPartial([]);
    void loadCompanyDatas(companyId, controller.signal, deals, datas => { if (!controller.signal.aborted) setPartial(datas); }).then(source => { if (!controller.signal.aborted) setSource(source); });
    return () => { controller.abort(); };
  }, [companyId, deals, ownerQuery, reload]);
  if (error && loadedQuery === ownerQuery) return <p role="alert">求人を読み込めませんでした。時間を置いて画面を開き直してください。</p>;
  if (!rows || loadedQuery !== ownerQuery) return <p role="status">求人を読み込んでいます…</p>;
  const model = companyListingsModel(rows, source.status === 'ready' ? source.datas : partial, deals, billingByJob, flagState.status === 'ready' ? flagState.flags : {}, sort);
  const ruleOf = (key: string) => flagState.status === 'ready' ? flagState.cards.find(card => card.key === key)?.rule : undefined;
  /** タグの説明: 基準の文と、この求人の判定に使った数字。 */
  const flagTitle = (key: string, id: string) => [ruleOf(key), flagState.status === 'ready' ? flagNumbersLabel(flagState.numbers[id]) : undefined].filter(Boolean).join('\n');
  const flagCounts = Object.keys(FLAG_LABELS).map(key => ({ key, count: model.live.filter(item => item.flags.includes(key)).length })).filter(entry => entry.count > 0);
  /** 応募・課金が読めていない求人の数字の欄。「—」（応募なし）と見分けがつくようにする。 */
  const missing = source.status === 'loading' ? '読込中' : source.status === 'error' ? '不明' : null;
  const table = (items: typeof model.live, page: number, setPage: (page: number) => void, label: string) => { const view = paginate(items, page, PAGE_SIZE); return <>
    {items.length ? <div className="jh-scroll"><table className="jh-listing-table cl-table" aria-label={`${label}の求人`}><thead><tr>
      <th scope="col" className="cl-title">求人名</th><th scope="col" className="cl-tags">タグ</th><th scope="col">媒体</th><th scope="col">掲載期間</th><th scope="col" className="num">応募（前4週→直近4週）</th><th scope="col" className="num">今の課金（4週）</th><th scope="col" className="num">応募1件あたり</th><th scope="col">最後の変更</th>
    </tr></thead><tbody>{view.items.map(item => <tr key={item.row.id} onClick={event => { if (!(event.target as HTMLElement).closest('button, a')) onOpenListing(item.row); }}>
      <th scope="row"><button type="button" className="jc-go" onClick={() => { onOpenListing(item.row); }}>{item.row.title ?? '求人名なし'}</button><div className="jh-listing-deals">{item.deals.map(deal => <small key={deal.id}><button type="button" className="jc-go" onClick={() => { onOpenDeal({ id: deal.id, name: dealName(deal) }); }}>{dealName(deal)}</button> · {deal.stage_label ?? 'ステージ不明'}</small>)}</div></th>
      <td className="cl-tags">{item.flags.length ? item.flags.map(key => <span key={key} className="jh-change-chip" title={flagTitle(key, item.row.id)}>{FLAG_LABELS[key]}</span>) : '—'}</td>
      <td className="cl-nowrap">{mediaLabel(item.row.media)}</td><td className="cl-nowrap">{companyListingPeriod(item.row)}{!item.live && publicationLabel(item.row.publication_status) !== '掲載終了' && <small>掲載状況：{publicationLabel(item.row.publication_status)}</small>}</td>
      {item.numbers === null && missing ? <><td className="num jh-missing">{missing}</td><td className="num jh-missing">{missing}</td><td className="num jh-missing">{missing}</td></> : <><td className="num">{appsChange(item.numbers?.previous ?? null, item.numbers?.recent ?? null)}</td><td className="num">{yen(item.numbers?.billing ?? null)}</td><td className="num">{perApplicationLabel(item.numbers, yen)}</td></>}
      <td className="cl-nowrap">{item.changedAt ? <>本文を変更<small>{formatDateTimeJst(item.changedAt, '日時不明')}</small></> : '変更日不明'}</td>
    </tr>)}</tbody></table></div> : <p>{label}の求人はありません。</p>}
    {items.length > PAGE_SIZE && <PageNav className="jh-pages" label={`${label}のページ`} size={PAGE_SIZE} hasPrev={view.pageIndex > 0} hasNext={view.pageIndex + 1 < view.pages} onPrev={() => { setPage(page - 1); }} onNext={() => { setPage(page + 1); }}>{String(view.from)}〜{String(view.to)} / {String(items.length)}件</PageNav>}
  </>; };
  return <>
    {source.status === 'error' && <div role="alert" className="jc-load-band"><span>応募・課金を読み込めませんでした。表の「不明」は応募なしではありません。</span><button type="button" className="jc-button" onClick={() => { setReload(count => count + 1); }}>もう一度読み込む</button></div>}
    <div className="cl-toolbar"><div className="jh-note jh-listings-scope">納品管理の求人です。応募・課金は {formatDay(model.asOf)} までの4週間{source.status === 'loading' && <span role="status">（応募・課金を読み込んでいます…）</span>}
      <InfoTip className="jc-infotip-left" iconFirst label="数え方"><p>応募・課金は {formatDay(model.asOf)} までの4週間と、その前の4週間です。取得できなかった数字は「—」です。</p><p>応募1件あたり＝今の課金 ÷ 直近4週の応募。最後の変更は本文の変更だけです。案件名から求人を比べられます。</p>{source.status === 'ready' && source.datas.some(data => data.capped || data.rows.length > 30) && <p>応募・課金は各案件で取得できた求人の分だけ表示しています。</p>}{source.status === 'ready' && source.notes?.map(note => <p key={note}>応募・課金：{note}</p>)}</InfoTip></div>
    <div className="jh-listing-tools">
      <label>並べ替え <select value={sort} onChange={event => { setSort(event.target.value as ListingSort); setLivePage(0); setPastPage(0); }}>{LISTING_SORTS.map(option => <option key={option.key} value={option.key}>{option.label}</option>)}</select></label>
      {flagState.status === 'loading' && <span role="status">タグを読み込んでいます…</span>}
      {flagState.status === 'preparing' && <span className="jh-note">タグはまだ準備中です。少し待って画面を開き直してください。</span>}
      {flagState.status === 'error' && <span role="alert">タグを読み込めませんでした。求人の表はそのまま使えます。</span>}
    </div></div>
    <section className="jh-live-listings" aria-labelledby={liveId}><h2 id={liveId}>掲載中 <span>{String(model.live.length)}件</span></h2>
      {flagState.status === 'ready' && <div className="jh-note jh-flag-summary">{flagCounts.length ? <>タグ：{flagCounts.map(entry => <span key={entry.key} className="jh-change-chip" title={ruleOf(entry.key)}>{FLAG_LABELS[entry.key]} {String(entry.count)}件</span>)}<InfoTip className="jc-infotip-left jh-flag-basis" iconFirst label="表の数字と合わないとき"><p>タグはホームのカードと同じ基準です。タグにマウスを置くと、判定に使った数字が出ます。次の3つの数え方が違うため、表の数字と合わないことがあります。</p><ul>
        <li>応募：タグは週ごとの実績の応募数。表は応募日ごとの件数。</li>
        <li>課金：タグは週ごとの実績の課金だけ。表は週ごとの実績の課金と、取り込んだ課金CSV（HRハッカーの求人）を合わせ、日割りにしたもの。</li>
        <li>期間：タグは週ごとの実績の最新の週までの4週間。表は応募を取得した日までの4週間。</li>
      </ul></InfoTip></> : 'タグが付いた掲載中の求人はありません。'}</div>}
      {table(model.live, livePage, setLivePage, '掲載中')}</section>
    <details className="jh-past-listings"><summary>過去（掲載終了） {String(model.past.length)}件</summary><p className="jh-note">掲載前・掲載状況が分からない求人も含みます。</p>{table(model.past, pastPage, setPastPage, '過去')}</details>
  </>;
}
