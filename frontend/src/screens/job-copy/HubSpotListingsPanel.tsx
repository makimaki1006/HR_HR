import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { loadCompanies, loadVersions, prefetchNeighbors } from './listingDetailCache';
import type { JobCopyRecord } from './data';
import { listingRecord, mediaLabel, publicationLabel, draftStateLabel } from './hubspotListings';
import type { HubSpotListingPage, HubSpotListing, DealSelection } from './hubspotListings';
import { formatDateTimeJst } from './format';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
import { AREA_MASTER } from './areaMaster';

export function HubSpotListingsPanel({ onOpen, onVisibleRows, active = true, onLoading, onFailure, initialListingId, onShowDeal, onApplicationsLoaded, revision = 0 }: { onOpen: (job: JobCopyRecord) => void; onVisibleRows?: (rows: HubSpotListing[]) => void; active?: boolean; onLoading?: () => void; onFailure?: (message: string) => void; initialListingId?: string | undefined; onShowDeal?: (deal: DealSelection) => void; onApplicationsLoaded?: (id: string, count: number) => void; revision?: number }) {
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [prefecture, setPrefecture] = useState('');
  const [title, setTitle] = useState('');
  const [media, setMedia] = useState('');
  const [deal, setDeal] = useState('');
  const [dealName, setDealName] = useState('');
  const [dealSearch, setDealSearch] = useState('');
  const [publication, setPublication] = useState('');
  const [draftStatus, setDraftStatus] = useState('');
  const [sort, setSort] = useState('title');
  const [titles, setTitles] = useState<string[]>([]);
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const pageRef = useRef(page);
  const loadedRecord = useRef<JobCopyRecord | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [selected, setSelected] = useState('');
  const listRequest = useRef<AbortController | null>(null);
  const detailRequest = useRef<AbortController | null>(null);
  const countRequest = useRef<{ key: string; controller: AbortController } | null>(null);
  useEffect(() => () => { countRequest.current?.controller.abort(); }, []);
  const message = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '求人を読み込めませんでした。時間を置いて再取得してください。';
  const load = useCallback(async (offset = 0) => {
    listRequest.current?.abort(); const controller = new AbortController(); listRequest.current = controller;
    setBusy(true); setError('');
    const query = new URLSearchParams({ offset: String(offset), sort: sort === 'applications' ? 'title' : sort });
    if (prefecture) query.set('prefecture', prefecture);
    if (title) query.set('title', title);
    if (media) query.set('media', media);
    if (deal) query.set('deal', deal);
    if (dealSearch) query.set('deal_search', dealSearch);
    if (publication) query.set('publication', publication);
    if (draftStatus) query.set('draft_status', draftStatus);
    const result = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { pageRef.current = result.data; setPage(result.data); if (result.data.titles.length) setTitles(result.data.titles); } else setError(message(result.error));
    setBusy(false);
  }, [prefecture, title, media, sort, deal, dealSearch, publication, draftStatus]);
  useEffect(() => {
    if (!active) { listRequest.current?.abort(); detailRequest.current?.abort(); countRequest.current?.controller.abort(); return; }
    const timer = window.setTimeout(() => { void load(); }, dealSearch ? 250 : 0);
    return () => { window.clearTimeout(timer); listRequest.current?.abort(); };
  }, [active, load, dealSearch, revision]);
  useEffect(() => {
    if (!active || page?.status !== 'preparing' || busy || error) return;
    const timer = window.setTimeout(() => { void load(); }, 5_000);
    return () => { window.clearTimeout(timer); };
  }, [active, page, busy, error, load]);
  const orderRef = useRef<string[]>([]);
  const opened = useRef('');
  const restored = useRef('');
  const openId = useCallback(async (id: string, listedRow?: HubSpotListing) => {
    opened.current = id; loadedRecord.current = null;
    countRequest.current?.controller.abort(); countRequest.current = null;
    detailRequest.current?.abort(); const controller = new AbortController(); detailRequest.current = controller;
    setSelected(id); setError(''); onLoading?.();
    const result = await loadVersions(id, { signal: controller.signal });
    if (controller.signal.aborted) return;
    if (result.ok) {
      const record = listingRecord(result.data, listedRow ?? pageRef.current?.listings.find(row => row.id === id));
      loadedRecord.current = record; onOpen(record);
      // 表示が済んだあとで、前後の求人を裏で読んでおく。
      prefetchNeighbors(orderRef.current, id);
    }
    else { const text = message(result.error); setError(text); onFailure?.(text); }
  }, [onOpen, onLoading, onFailure]);
  useEffect(() => {
    const record = loadedRecord.current;
    if (!active || page?.status !== 'ready' || !record?.hubspotId || record.hubspotId !== opened.current || record.listingApplicationCount != null) return;
    const id = record.hubspotId;
    const row = page.listings.find(item => item.id === id);
    if (row?.application_count != null) {
      loadedRecord.current = { ...record, listingApplicationCount: row.application_count };
      onApplicationsLoaded?.(id, row.application_count);
      return;
    }
    // The opened job may be outside the current page or filters. Only re-read
    // the acquired count, once per job/index; retain the selected body and popup.
    const key = JSON.stringify([id, page.index_built_at]);
    if (countRequest.current?.key === key && !countRequest.current.controller.signal.aborted) return;
    countRequest.current?.controller.abort();
    const controller = new AbortController(); countRequest.current = { key, controller };
    void apiGet<{ listing_id: string; application_count: number | null }>(`/api/job-copy/listings/${encodeURIComponent(id)}/applications`, { signal: controller.signal, timeoutMs: 30_000 }).then(result => {
      const current = loadedRecord.current;
      if (controller.signal.aborted || opened.current !== id || current?.hubspotId !== id || current.listingApplicationCount != null) return;
      if (result.ok && result.data.listing_id === id && result.data.application_count != null) {
        loadedRecord.current = { ...current, listingApplicationCount: result.data.application_count };
        onApplicationsLoaded?.(id, result.data.application_count);
      }
    });
  }, [active, page, onApplicationsLoaded]);
  useEffect(() => {
    if (!active) return;
    if (!initialListingId) { restored.current = ''; return; }
    if (!/^\d{1,20}$/.test(initialListingId) || restored.current === initialListingId) return;
    restored.current = initialListingId;
    if (opened.current !== initialListingId) void openId(initialListingId);
  }, [active, initialListingId, openId]);
  const conditions = [deal ? dealName : '', prefecture, title === 'unknown' ? '職種不明' : title, media === 'hrh' ? 'HRハッカー' : media === 'airwork' ? 'AirWork' : '', publication === 'unknown' ? '公開状況は不明' : publication, draftStatus ? draftStatus === 'none' ? '案なし・確認状態は未取得' : ({ pending: '確認待ち', adopted: '採用', rejected: '見送り' } as Record<string, string>)[draftStatus] ?? '不明' : ''].filter(Boolean).join(' ／ ') || 'すべての求人';
  const rows = useMemo(() => sort === 'applications' ? [...(page?.listings ?? [])].sort((a, b) => (b.application_count ?? -1) - (a.application_count ?? -1)) : page?.listings ?? [], [page, sort]);
  useEffect(() => { orderRef.current = rows.map(item => item.id); }, [rows]);
  // 一覧に出ている求人の会社名をまとめて読んでおく。求人を開いたとき会社名がすぐ出る。
  useEffect(() => { if (active && page?.status === 'ready' && rows.length) void loadCompanies(rows.map(item => item.id)); }, [active, page?.status, rows]);
  useEffect(() => { if (active) onVisibleRows?.(rows); }, [active, rows, onVisibleRows]);
  return <section className="jc-listings-panel" aria-labelledby="hubspot-listings-heading" hidden={!active}>
    <div className="jc-hubspot-list-heading"><h2 id="hubspot-listings-heading">HubSpot の求人</h2><span>{page?.status === 'ready' ? `${String(page.total)}件` : page?.status === 'preparing' ? '準備中' : '未取得'}</span></div>
    <details className="jc-hubspot-filter-details" open={filtersOpen} onToggle={event => { setFiltersOpen(event.currentTarget.open); }}><summary>絞り込み・並び順</summary><div className="jc-hubspot-filters">
      <label>取引名を検索<input value={dealSearch} onChange={event => { setDealSearch(event.target.value); }} placeholder="取引名の一部" /></label>
      <label>取引<select aria-label="取引" value={deal} onChange={event => { const id = event.target.value; setDeal(id); setDealName(id === 'none' ? '取引なし' : page?.deals?.find(row => row.id === id)?.name ?? '取引名未取得'); }}><option value="">すべての取引</option><option value="none">取引なし{page?.no_deal_count === undefined ? '' : `（${String(page.no_deal_count)}件）`}</option>{deal && deal !== 'none' && !page?.deals?.some(row => row.id === deal) && <option value={deal}>{dealName}</option>}{page?.deals?.map(row => <option key={row.id} value={row.id}>{row.name ?? '取引名未取得'}（{row.count}件）</option>)}</select></label>
      {dealSearch && <p>候補は取引名順で最大30件です。見つからない場合は取引名を長く入力してください。</p>}
      <label>都道府県<select aria-label="都道府県" value={prefecture} onChange={event => { setPrefecture(event.target.value); }}><option value="">すべて</option>{AREA_MASTER.map(([pref]) => <option key={pref} value={pref}>{pref}</option>)}</select></label>
      <label>職種の分類<select aria-label="職種の分類" value={title} onChange={event => { setTitle(event.target.value); }}><option value="">すべて</option><option value="unknown">不明</option>{titles.map(item => <option key={item} value={item}>{item}</option>)}</select></label>
      <label>媒体<select aria-label="媒体" value={media} onChange={event => { setMedia(event.target.value); }}><option value="">すべて</option><option value="hrh">HRハッカー</option><option value="airwork">AirWork</option></select></label>
      <label>公開状況<select aria-label="公開状況" value={publication} onChange={event => { setPublication(event.target.value); }}><option value="">すべて</option><option value="unknown">不明</option>{(page?.publication_statuses?.filter(value => value !== '不明') ?? ['掲載中', '掲載終了', '掲載前']).map(value => <option key={value} value={value}>{value}</option>)}</select></label>
      <label>案の確認状態<select aria-label="案の確認状態で絞り込む" value={draftStatus} onChange={event => { setDraftStatus(event.target.value); }}><option value="">すべて</option><option value="pending">確認待ち</option><option value="adopted">採用</option><option value="rejected">見送り</option><option value="none">案なし・確認状態は未取得</option></select></label>
      <label>並び順<select aria-label="並び順" value={sort} onChange={event => { setSort(event.target.value); }}><option value="title">求人名順</option><option value="media">媒体順</option><option value="applications">表示中の応募が多い順</option></select></label>
    </div><button type="button" className="jc-button" onClick={() => { setFiltersOpen(false); }}>条件を畳む</button></details>
    {conditions !== 'すべての求人' && <p className="jc-filter-summary" title={conditions} aria-label="選択中の条件">{conditions}</p>}
    <div className="jc-list-actions"><button type="button" className="jc-button" disabled={busy} onClick={() => { void load(); }}>求人を読み込む</button>
    {deal && onShowDeal && <button type="button" className="jc-button" onClick={() => { onShowDeal({ id: deal, name: dealName, prefecture, title, media, ...(publication ? { publication } : {}), ...(draftStatus ? { draftStatus } : {}) }); }}>この取引の求人</button>}
    </div>
    {busy && !page && <p role="status">求人の一覧を読み込んでいます…</p>}{error && <p role="alert">{error}</p>}
    {page?.status === 'preparing' && <div className="jc-index-preparing" role="status"><strong>求人の一覧を準備しています</strong><p>最初の一覧表示には数分以上かかる場合があります。準備が終わると自動で一覧を表示します。</p>{page.refresh_failed && <p>一覧の準備状況を確認できませんでした。時間を置いて自動で確認します。</p>}<progress aria-label="求人の一覧を準備中" /></div>}
    {page?.status === 'ready' && <><p className="jc-index-date">{formatDateTimeJst(page.index_built_at, '日時不明')}時点の求人・応募数</p>
      {page.refreshing && <p role="status">一覧を更新中です。表示している時点の一覧を利用できます。</p>}
      {page.refresh_failed && <p role="status">一覧を更新できませんでした。表示している時点の一覧を利用しています。</p>}
      <div className="jc-list-scroll" aria-label="HubSpotの求人一覧">{rows.map(row => <button type="button" className="jc-job jc-hubspot-job" key={row.id} aria-pressed={row.id === selected} aria-label={`${row.title ?? '求人'}の本文を見る`} onClick={() => { void openId(row.id, row); }}>
        <strong>{row.title ?? '求人名未取得'}</strong><span className="jc-job-company"><span className="jc-publication">{publicationLabel(row.publication_status)}</span><span>案：{row.draft_status ? draftStateLabel(row.draft_status) : '案なし・確認状態は未取得'}</span></span>
        <span className="jc-job-location">{row.prefecture ?? '不明'}{row.municipality ?? ''} · {mediaLabel(row.media)} · {row.category ?? '職種不明'}</span>
        <span className="jc-job-bottom"><small>応募 {row.application_count === null ? '未取得' : `${String(row.application_count)}件`}</small></span>
        <span className="jc-visually-hidden">媒体の一覧で最後に確認した日：{formatDateTimeJst(row.last_csv_detected_at, '未取得')}</span>
      </button>)}{!rows.length && <p>条件に合う求人はありません。</p>}</div>
      <div className="jc-list-pagination"><span>{page.total ? `${String(page.offset + 1)}〜${String(page.offset + rows.length)}件を表示` : '条件に合う求人はありません'}</span>{page.offset > 0 && <button className="jc-button" disabled={busy} onClick={() => { void load(Math.max(0, page.offset - 50)); }}>前の求人</button>}{page.next_offset !== null && <button type="button" className="jc-button" disabled={busy} onClick={() => { void load(page.next_offset ?? 0); }}>次の求人</button>}</div>
    </>}
  </section>;
}
