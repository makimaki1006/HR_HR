import { useCallback, useEffect, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import type { JobCopyRecord } from './data';
import { listingRecord, mediaLabel } from './hubspotListings';
import type { HubSpotListing, HubSpotListingPage, HubSpotVersions } from './hubspotListings';
import { formatDateTimeJst } from './format';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
import { AREA_MASTER } from './areaMaster';

export function HubSpotListingsPanel({ onOpen, active = true, onLoading, onFailure }: { onOpen: (job: JobCopyRecord) => void; active?: boolean; onLoading?: () => void; onFailure?: (message: string) => void }) {
  const [filtersOpen, setFiltersOpen] = useState(() => window.matchMedia('(min-width: 801px)').matches);
  useEffect(() => { const media = window.matchMedia('(min-width: 801px)'); const change = (event: MediaQueryListEvent) => { setFiltersOpen(event.matches); }; media.addEventListener('change', change); return () => { media.removeEventListener('change', change); }; }, []);
  const [prefecture, setPrefecture] = useState('');
  const [title, setTitle] = useState('');
  const [media, setMedia] = useState('');
  const [sort, setSort] = useState('title');
  const [titles, setTitles] = useState<string[]>([]);
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [selected, setSelected] = useState('');
  const [history, setHistory] = useState<HubSpotVersions | null>(null);
  const listRequest = useRef<AbortController | null>(null);
  const detailRequest = useRef<AbortController | null>(null);
  const message = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '求人を取得できませんでした。時間を置いて再取得してください。';
  const load = useCallback(async (offset = 0) => {
    listRequest.current?.abort(); const controller = new AbortController(); listRequest.current = controller;
    setBusy(true); setError('');
    const query = new URLSearchParams({ offset: String(offset), sort: sort === 'applications' ? 'title' : sort });
    if (prefecture) query.set('prefecture', prefecture);
    if (title) query.set('title', title);
    if (media) query.set('media', media);
    const result = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { setPage(result.data); if (result.data.titles.length) setTitles(result.data.titles); } else setError(message(result.error));
    setBusy(false);
  }, [prefecture, title, media, sort]);
  useEffect(() => {
    if (!active) { listRequest.current?.abort(); detailRequest.current?.abort(); return; }
    const timer = window.setTimeout(() => { void load(); }, 0);
    return () => { window.clearTimeout(timer); listRequest.current?.abort(); };
  }, [active, load]);
  useEffect(() => {
    if (!active || page?.status !== 'preparing' || busy || error) return;
    const timer = window.setTimeout(() => { void load(); }, 5_000);
    return () => { window.clearTimeout(timer); };
  }, [active, page, busy, error, load]);
  async function open(row: HubSpotListing) {
    detailRequest.current?.abort(); const controller = new AbortController(); detailRequest.current = controller;
    setSelected(row.id); setHistory(null); setError(''); onLoading?.();
    const result = await apiGet<HubSpotVersions>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/versions`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { setHistory(result.data); onOpen(listingRecord(result.data)); }
    else { const text = message(result.error); setError(text); onFailure?.(text); }
  }
  const rows = sort === 'applications' ? [...(page?.listings ?? [])].sort((a, b) => (b.application_count ?? -1) - (a.application_count ?? -1)) : page?.listings ?? [];
  return <section className="jc-listings-panel" aria-labelledby="hubspot-listings-heading" hidden={!active}>
    <div className="jc-hubspot-list-heading"><h2 id="hubspot-listings-heading">HubSpot の求人</h2><span>{page?.status === 'ready' ? `${String(page.total)}件` : '未取得'}</span></div>
    <details className="jc-hubspot-filter-details" open={filtersOpen} onToggle={event => { setFiltersOpen(event.currentTarget.open); }}><summary>絞り込み・並び順</summary><div className="jc-hubspot-filters">
      <label>都道府県<select aria-label="都道府県" value={prefecture} onChange={event => { setPrefecture(event.target.value); }}><option value="">すべて</option>{AREA_MASTER.map(([pref]) => <option key={pref} value={pref}>{pref}</option>)}</select></label>
      <label>職種の分類<select aria-label="職種の分類" value={title} onChange={event => { setTitle(event.target.value); }}><option value="">すべて</option><option value="unknown">不明</option>{titles.map(item => <option key={item} value={item}>{item}</option>)}</select></label>
      <label>媒体<select aria-label="媒体" value={media} onChange={event => { setMedia(event.target.value); }}><option value="">すべて</option><option value="hrh">HRハッカー</option><option value="airwork">AirWork</option></select></label>
      <label>並び順<select aria-label="並び順" value={sort} onChange={event => { setSort(event.target.value); }}><option value="title">求人名順</option><option value="media">媒体順</option><option value="applications">表示中の応募が多い順</option></select></label>
    </div></details>
    <button type="button" className="jc-button" disabled={busy} onClick={() => { void load(); }}>求人を取得</button>
    {busy && !page && <p role="status">求人の一覧を取得しています…</p>}{error && <p role="alert">{error}</p>}
    {page?.status === 'preparing' && <div className="jc-index-preparing" role="status"><strong>求人の一覧を準備しています</strong><p>初回の準備には約7分が目安です。あと数分かかる場合があります。準備が終わると自動で一覧を表示します。</p>{page.refresh_failed && <p>準備を取得できませんでした。時間を置いて自動で確認します。</p>}<progress aria-label="求人の一覧を準備中" /></div>}
    {page?.status === 'ready' && <><p className="jc-index-date">条件に合う求人は{page.total}件です。<br />{formatDateTimeJst(page.index_built_at, '日時不明')}時点の一覧</p>
      {page.refreshing && <p role="status">一覧を更新中です。表示している時点の一覧を利用できます。</p>}
      {page.refresh_failed && <p role="status">一覧の更新を取得できませんでした。表示している時点の一覧を利用しています。</p>}
      <div className="jc-list-scroll" aria-label="HubSpotの求人一覧">{rows.map(row => <button type="button" className="jc-job jc-hubspot-job" key={row.id} aria-pressed={row.id === selected} aria-label={`${row.title ?? '求人'}の版を見る`} onClick={() => { void open(row); }}>
        <span className="jc-job-company">{mediaLabel(row.media)} <span className="jc-publication">{row.publication_status ?? '未取得'}</span></span>
        <strong>{row.title ?? '求人名未取得'}</strong><span>{row.prefecture ?? '不明'}{row.municipality ?? ''} · {row.category ?? '職種不明'}</span>
        <span className="jc-job-bottom"><small>応募 {row.application_count === null ? '未取得' : `${String(row.application_count)}件`}</small><small>{row.media_job_id}</small></span>
        <span className="jc-visually-hidden">{row.account_id ? `${row.account_id} / ` : ''}{row.media_job_id} 媒体の一覧で最後に確認した日：{formatDateTimeJst(row.last_csv_detected_at, '未取得')}</span>
      </button>)}{!rows.length && <p>条件に合う求人はありません。</p>}</div>
      <div className="jc-list-pagination"><span>{page.total ? `${String(page.offset + 1)}〜${String(page.offset + rows.length)}件を表示` : '条件に合う求人はありません'}</span>{page.offset > 0 && <button className="jc-button" disabled={busy} onClick={() => { void load(Math.max(0, page.offset - 50)); }}>前の求人</button>}{page.next_offset !== null && <button type="button" className="jc-button" disabled={busy} onClick={() => { void load(page.next_offset ?? 0); }}>次の求人</button>}</div>
    </>}
    {history && <p className="jc-history-status" role="status">取得した文面の版は{history.versions.length}件です。本文の履歴：{history.history_counts[history.listing.media === 'hrh' ? 'hrh_kyuujinhyou_honbun' : 'shigotonaiyou'] ?? '未取得'}件。{history.history_may_be_incomplete && '履歴は項目ごとに20件までの可能性があり、過去の版がすべて含まれているとは限りません。'}保存日時と掲載開始日時は異なります。</p>}
  </section>;
}
