import { useEffect, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { formatDateTimeJst } from './format';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy } from './SnapshotErrorNotice';
import { listingRecord, mediaLabel, publicationLabel } from './hubspotListings';
import type { DealSelection, HubSpotListingPage, HubSpotVersions, HubSpotListing } from './hubspotListings';
import type { JobCopyRecord } from './data';
import { compareJobs, comparisonValue, salaryAmount } from './dealComparisonModel';
import type { ComparedJob } from './dealComparisonModel';
const columns = [['title', '求人名'], ['media', '媒体'], ['publication', '公開状況'], ['occupation', '職種'], ['location', '勤務地'], ['salary_kind', '給与形態'], ['salary_min', '給与の下限'], ['salary_max', '給与の上限'], ['hours', '勤務時間'], ['holidays', '休日'], ['applications', '応募数'], ['body_changed_at', '本文の最終変更日']] as const;
const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '求人を取得できませんでした。時間を置いて再取得してください。';
function cell(row: HubSpotListing, key: typeof columns[number][0]): string {
  if (key === 'title') return row.title ?? '求人名未取得';
  if (key === 'media') return mediaLabel(row.media);
  if (key === 'publication') return publicationLabel(row.publication_status);
  if (key === 'applications') return row.application_count == null ? '未取得' : `${String(row.application_count)}件`;
  if (key === 'salary_min' || key === 'salary_max') return salaryAmount(row.summary?.salary_kind, row.summary?.[key]);
  if (key === 'body_changed_at') return formatDateTimeJst(row.summary?.body_changed_at, '—');
  return row.summary?.[key] ?? '—';
}
export function ParallelJobs({ jobs }: { jobs: ComparedJob[] }) {
  const result = compareJobs(jobs);
  const table = (different: boolean) => <div className="jc-deal-table-scroll"><table className="jc-parallel-table"><thead><tr><th>項目</th>{jobs.map(job => <th key={job.id}>{job.title}<small>{job.media}</small></th>)}</tr></thead><tbody>{result.rows.filter(row => row.different === different).map(row => <tr key={row.key} className={different ? 'jc-comparison-different' : ''}><th scope="row">{row.label}</th>{row.values.map((value, index) => <td key={jobs[index]?.id}>{comparisonValue(row.key, value, jobs[index]?.body ?? null)}</td>)}</tr>)}</tbody></table></div>;
  return <section aria-label="選んだ求人の横並び比較"><h2>選んだ求人を比べる</h2><p>媒体ごとに別の求人として並べています。本文に記録された項目だけを比べます。</p><p>{result.fullDifferent ? '本文には違いがあります。' : '取得した本文は同じです。'} 未取得の項目は「—」で表示します。</p><h3>違う項目（{result.rows.filter(row => row.different).length}項目）</h3>{table(true)}<details><summary>同じ項目（{result.rows.filter(row => !row.different).length}項目）</summary>{table(false)}</details>{result.unstructured.length > 0 && <p>項目に分けられない本文があります。給与や休日を文章から推測して補っていません。求人の詳細で確認してください。</p>}{jobs.some(job => job.body === null) && <p>本文が未取得の求人があります。</p>}</section>;
}
export function DealComparison({ deal, onOpen, onClose }: { deal: DealSelection; onOpen: (job: JobCopyRecord) => void; onClose: () => void }) {
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const [media, setMedia] = useState(deal.media);
  const [publication, setPublication] = useState(deal.publication ?? '');
  const [sort, setSort] = useState('title');
  const [descending, setDescending] = useState(false);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<HubSpotListing[]>([]);
  const [comparison, setComparison] = useState<ComparedJob[] | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const action = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    const query = new URLSearchParams({ deal: deal.id, offset: String(offset), sort, descending: String(descending) });
    if (deal.prefecture) query.set('prefecture', deal.prefecture);
    if (deal.title) query.set('title', deal.title);
    if (deal.draftStatus) query.set('draft_status', deal.draftStatus);
    if (media) query.set('media', media);
    if (publication) query.set('publication', publication);
    void apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => { if (controller.signal.aborted) return; if (result.ok) { setPage(result.data); setError(''); } else setError(failure(result.error)); });
    return () => { controller.abort(); };
  }, [deal, media, publication, sort, descending, offset, reload]);
  useEffect(() => () => { action.current?.abort(); }, []);
  const open = async (row: HubSpotListing) => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError('');
    const result = await apiGet<HubSpotVersions>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/versions`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) onOpen(listingRecord(result.data)); else setError(failure(result.error));
    setBusy(false);
  };
  const compare = async () => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError(''); setComparison(null);
    const result = await apiGet<{ jobs: { listing: HubSpotListing; body: string | null }[] }>(`/api/job-copy/listing-comparison?ids=${selected.map(row => encodeURIComponent(row.id)).join(',')}`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) setComparison(result.data.jobs.map(job => ({ id: job.listing.id, title: job.listing.title ?? '求人名未取得', media: mediaLabel(job.listing.media), body: job.body }))); else setError(failure(result.error));
    setBusy(false);
  };
  return <main className="jc-detail jc-deal-comparison" id="job-details" tabIndex={-1}><header><div><h1>この取引の求人</h1><p>{deal.name}</p></div><button className="jc-button" onClick={onClose}>詳細に戻る</button></header><p>取引に関連する求人を50件ずつ表示します。都道府県・職種・案の状態は左の絞り込み条件を引き継ぎます。</p>
    <div className="jc-deal-controls"><label>表の媒体<select value={media} onChange={event => { setMedia(event.target.value); setOffset(0); }}><option value="">すべて</option><option value="hrh">HRハッカー</option><option value="airwork">AirWork</option></select></label><label>表の公開状況<select value={publication} onChange={event => { setPublication(event.target.value); setOffset(0); }}><option value="">すべて</option><option value="unknown">未取得</option>{['公開', '公開中', '掲載中', '非公開', '公開終了', '公開開始前', '掲載終了', '募集中', '募集終了', '不明'].map(value => <option key={value}>{value}</option>)}</select></label><button className="jc-button" onClick={() => { setReload(value => value + 1); }}>表を再取得</button></div>
    {error && <p role="alert">{error}</p>}{!page && <p role="status">この取引の求人を取得しています…</p>}{busy && <p role="status">選んだ求人を取得しています…</p>}
    {page && <><p>{page.total == null ? '準備中' : `${String(page.total)}件`} · {formatDateTimeJst(page.index_built_at, '日時不明')}時点の関連・条件・応募数。本文の最終変更日は掲載開始日とは異なります。</p>{page.refresh_failed && <p>一覧の更新を取得できませんでした。表示時点の値を利用しています。</p>}
    <div className="jc-deal-table-scroll"><table className="jc-deal-table" aria-label="取引の求人一覧表"><thead><tr><th>比較に選ぶ</th>{columns.map(([key, label]) => <th key={key} aria-sort={sort === key ? descending ? 'descending' : 'ascending' : 'none'}><button onClick={() => { setSort(key); setDescending(sort === key ? !descending : false); setOffset(0); }}>{label}{sort === key ? descending ? ' ▼' : ' ▲' : ''}</button></th>)}</tr></thead><tbody>{page.listings.map(row => <tr key={row.id} onClick={() => { if (!busy) void open(row); }}><td onClick={event => { event.stopPropagation(); }}><input type="checkbox" aria-label={`${row.title ?? '求人'}を比較に選ぶ`} checked={selected.some(item => item.id === row.id)} disabled={!selected.some(item => item.id === row.id) && selected.length >= 5} onChange={event => { setComparison(null); setSelected(items => event.target.checked ? [...items, row] : items.filter(item => item.id !== row.id)); }} /></td>{columns.map(([key]) => <td key={key}>{key === 'title' ? <button className="jc-deal-open" disabled={busy} onClick={event => { event.stopPropagation(); void open(row); }}>{cell(row, key)}</button> : cell(row, key)}</td>)}</tr>)}</tbody></table></div>
    {!page.listings.length && <p>条件に合う求人はありません。</p>}
    <div className="jc-deal-controls"><span>{page.total && page.listings.length ? `${String(offset + 1)}〜${String(offset + page.listings.length)}件` : ''}</span><button className="jc-button" disabled={offset === 0} onClick={() => { setOffset(value => Math.max(0, value - 50)); }}>前の50件</button><button className="jc-button" disabled={page.next_offset === null} onClick={() => { setOffset(page.next_offset ?? 0); }}>次の50件</button></div></>}
    <div className="jc-deal-selection"><p>比較に選択：{selected.length}件（2〜5件）。ページを移動しても選択を保ちます。</p>{selected.map(row => <button className="jc-button" key={row.id} onClick={() => { setComparison(null); setSelected(items => items.filter(item => item.id !== row.id)); }}>{row.title ?? '求人名未取得'}を外す</button>)}<button className="jc-button" disabled={selected.length < 2 || busy} onClick={() => { void compare(); }}>横に並べて比べる</button></div>
    {comparison && <ParallelJobs jobs={comparison} />}
  </main>;
}
