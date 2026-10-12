import { ListingPopup } from './ListingPopup';
import { useEffect, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { formatDateTimeJst } from './format';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy } from './SnapshotErrorNotice';
import { listingRecord, mediaLabel, publicationLabel } from './hubspotListings';
import type { DealSelection, HubSpotListingPage, HubSpotVersions, HubSpotListing } from './hubspotListings';
import type { JobCopyRecord } from './data';
import { compareJobs, comparisonValue, salaryAmount } from './dealComparisonModel';
import { composeSalary } from './hrhCopy';
import type { ComparedJob } from './dealComparisonModel';
import { markInlineChanges } from './diff';
const columns = [['title', '求人名'], ['media', '媒体'], ['publication', '公開状況'], ['occupation', '職種'], ['location', '勤務地'], ['salary', '給与'], ['applications', '応募数'], ['body_changed_at', '本文の最終変更日'], ['hours', '勤務時間'], ['holidays', '休日'], ['salary_kind', '給与形態'], ['salary_min', '給与の下限'], ['salary_max', '給与の上限']] as const;
const optionalColumns = columns.slice(8);
type ColumnKey = typeof columns[number][0];
const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '求人を読み込めませんでした。時間を置いて再取得してください。';
function cell(row: HubSpotListing, key: typeof columns[number][0]): string {
  if (key === 'title') return row.title ?? '求人名未取得';
  if (key === 'media') return mediaLabel(row.media);
  if (key === 'publication') return publicationLabel(row.publication_status);
  if (key === 'applications') return row.application_count == null ? '未取得' : `${String(row.application_count)}件`;
  if (key === 'salary') return composeSalary(row.summary?.salary_kind?.trim() ?? '', row.summary?.salary_min?.trim() ?? '', row.summary?.salary_max?.trim() ?? '');
  if (key === 'salary_min' || key === 'salary_max') return salaryAmount(row.summary?.salary_kind, row.summary?.[key]);
  if (key === 'body_changed_at') return formatDateTimeJst(row.summary?.body_changed_at, '未取得');
  const value = row.summary?.[key];
  return value?.trim() ? value : '未取得';
}
function ComparisonCell({ value, reference, different }: { value: string; reference: string; different: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const [clipped, setClipped] = useState(false);
  const preview = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = preview.current;
    if (!element || expanded) return;
    const observer = new ResizeObserver(() => { setClipped(element.scrollHeight > element.clientHeight); });
    observer.observe(element);
    return () => { observer.disconnect(); };
  }, [value, expanded]);
  const segments = different && value !== '未取得' && reference !== '未取得' ? markInlineChanges([{ kind: 'removed', text: reference }, { kind: 'added', text: value }]).find(line => line.kind === 'added')?.segments : undefined;
  const content = segments ? segments.map((segment, i) => segment.changed ? <mark className="jc-mark-added" key={i}>{segment.text}</mark> : segment.text) : value;
  const long = clipped || value.length > 100 || value.split('\n').length > 3;
  return <><div ref={preview} className={expanded ? 'jc-comparison-full' : 'jc-comparison-preview'}>{content}</div>{long && <button className="jc-button" aria-expanded={expanded} onClick={() => { setExpanded(!expanded); }}>{expanded ? '閉じる' : '全文を見る'}</button>}</>;
}
export function ParallelJobs({ jobs }: { jobs: ComparedJob[] }) {
  const result = compareJobs(jobs);
  const table = (different: boolean) => <div className="jc-deal-table-scroll"><table className="jc-parallel-table"><thead><tr><th>項目</th>{jobs.map(job => <th key={job.id}>{job.title}<small>{job.media}</small></th>)}</tr></thead><tbody>{result.rows.filter(row => row.different === different).map(row => <tr key={row.key} className={different ? 'jc-comparison-different' : ''}><th scope="row">{row.label}</th>{row.values.map((value, index) => <td key={jobs[index]?.id}><ComparisonCell value={comparisonValue(row.key, value, jobs[index]?.body ?? null, jobs[index]?.fields)} reference={comparisonValue(row.key, row.values[index === 0 ? 1 : 0] ?? '', jobs[index === 0 ? 1 : 0]?.body ?? null, jobs[index === 0 ? 1 : 0]?.fields)} different={different} /></td>)}</tr>)}</tbody></table></div>;
  return <section aria-label="選んだ求人の横並び比較"><h2>選んだ求人を比べる</h2><p>媒体ごとに別の求人として並べています。取得できた求人の項目を比べます。</p><p>{result.fullDifferent ? '本文には違いがあります。' : '確認した本文は同じです。'} 値がない項目は「未取得」で表示します。</p><h3>違う項目（{result.rows.filter(row => row.different).length}項目）</h3>{table(true)}<details><summary>同じ項目（{result.rows.filter(row => !row.different).length}項目）</summary>{table(false)}</details>{result.unstructured.length > 0 && <p>項目に分けられない本文があります。給与や休日を文章から推測して補っていません。求人の詳細で確認してください。</p>}{jobs.some(job => job.body === null) && <p>本文が未取得の求人があります。</p>}</section>;
}
export function DealComparison({ deal, onOpen, onClose }: { deal: DealSelection; onOpen: (job: JobCopyRecord) => void; onClose: () => void }) {
  const [popup, setPopup] = useState<{ job: JobCopyRecord; opener: HTMLElement } | null>(null);
  const [extraColumns, setExtraColumns] = useState<ColumnKey[]>([]);
  const visibleColumns = columns.filter(([key], index) => index < 8 || extraColumns.includes(key));
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
    void apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => { if (controller.signal.aborted) return; if (result.ok) {
      setPage(result.data); setError('');
      const ids = result.data.listings.map(row => row.id);
      if (ids.length) void apiGet<{ dates?: Record<string, string | null> }>(`/api/job-copy/listing-body-dates?ids=${ids.map(encodeURIComponent).join(',')}`, { signal: controller.signal, timeoutMs: 120_000 }).then(history => {
        if (controller.signal.aborted) return;
        if (history.ok && history.data.dates) { const dates = history.data.dates; setPage(current => current === result.data ? { ...current, listings: current.listings.map(row => ({ ...row, ...(row.summary ? { summary: { ...row.summary, body_changed_at: dates[row.id] ?? null } } : {}) })) } : current); }
      });
    } else setError(failure(result.error)); });
    return () => { controller.abort(); };
  }, [deal, media, publication, sort, descending, offset, reload]);
  useEffect(() => () => { action.current?.abort(); }, []);
  const open = async (row: HubSpotListing, opener?: HTMLElement) => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError('');
    const result = await apiGet<HubSpotVersions>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/versions`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { const job = listingRecord(result.data, row); if (opener) setPopup({ job, opener }); else onOpen(job); } else setError(failure(result.error));
    setBusy(false);
  };
  const compare = async () => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError(''); setComparison(null);
    const result = await apiGet<{ jobs: { listing: HubSpotListing; body: string | null; fields?: Record<string, string> }[] }>(`/api/job-copy/listing-comparison?ids=${selected.map(row => encodeURIComponent(row.id)).join(',')}`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) setComparison(result.data.jobs.map(job => ({ id: job.listing.id, title: job.listing.title ?? '求人名未取得', media: mediaLabel(job.listing.media), body: job.body, ...(job.fields ? { fields: job.fields } : {}) }))); else setError(failure(result.error));
    setBusy(false);
  };
  return <main className="jc-detail jc-deal-comparison" id="job-details" tabIndex={-1}><header><div><h1>この取引の求人</h1><p>{deal.name}</p></div><button className="jc-button" onClick={onClose}>詳細に戻る</button></header><p>取引に関連する求人を50件ずつ表示します。都道府県・職種・案の状態は左の絞り込み条件を引き継ぎます。</p>
    <div className="jc-deal-controls"><label>表の媒体<select value={media} onChange={event => { setMedia(event.target.value); setOffset(0); }}><option value="">すべて</option><option value="hrh">HRハッカー</option><option value="airwork">AirWork</option></select></label><label>表の公開状況<select value={publication} onChange={event => { setPublication(event.target.value); setOffset(0); }}><option value="">すべて</option><option value="unknown">未取得</option>{['公開', '公開中', '掲載中', '非公開', '公開終了', '公開開始前', '掲載終了', '募集中', '募集終了', '不明'].map(value => <option key={value}>{value}</option>)}</select></label><button className="jc-button" onClick={() => { setReload(value => value + 1); }}>表を再読み込み</button></div>
    <details className="jc-deal-column-options"><summary>表示する列を選ぶ</summary><div>{optionalColumns.map(([key, label]) => <label key={key}><input type="checkbox" checked={extraColumns.includes(key)} onChange={event => { setExtraColumns(keys => event.target.checked ? [...keys, key] : keys.filter(item => item !== key)); }} />{label}</label>)}</div></details>
    {error && <p role="alert">{error}</p>}{!page && <p role="status">この取引の求人を読み込みしています…</p>}{busy && <p role="status">選んだ求人を読み込みしています…</p>}
    {page && <><p>{page.total == null ? '準備中' : `${String(page.total)}件`} · {formatDateTimeJst(page.index_built_at, '日時不明')}時点の関連・条件・応募数。本文の最終変更日は掲載開始日とは異なります。</p>{page.refresh_failed && <p>一覧を更新できませんでした。表示時点の値を利用しています。</p>}
    <div className="jc-deal-table-scroll"><table className="jc-deal-table" aria-label="取引の求人一覧表"><thead><tr><th className="jc-deal-select-heading" aria-label="比較に選ぶ">選択</th>{visibleColumns.map(([key, label]) => <th key={key} data-column={key} aria-sort={sort === (key === 'salary' ? 'salary_min' : key) ? descending ? 'descending' : 'ascending' : 'none'}><button onClick={() => { const sortKey = key === 'salary' ? 'salary_min' : key; setSort(sortKey); setDescending(sort === sortKey ? !descending : false); setOffset(0); }}>{label}{sort === (key === 'salary' ? 'salary_min' : key) ? descending ? ' ▼' : ' ▲' : ''}</button></th>)}</tr></thead><tbody>{page.listings.map(row => <tr key={row.id} onClick={() => { if (!busy) void open(row); }}><td onClick={event => { event.stopPropagation(); }}><input type="checkbox" aria-label={`${row.title ?? '求人'}を比較に選ぶ`} checked={selected.some(item => item.id === row.id)} disabled={!selected.some(item => item.id === row.id) && selected.length >= 5} onChange={event => { setComparison(null); setSelected(items => event.target.checked ? [...items, row] : items.filter(item => item.id !== row.id)); }} /></td>{visibleColumns.map(([key]) => <td key={key} data-column={key}>{key === 'title' ? <div className="jc-deal-title-actions"><button className="jc-deal-open" disabled={busy} onClick={event => { event.stopPropagation(); void open(row); }}>{cell(row, key)}</button><button className="jc-button" disabled={busy} onClick={event => { event.stopPropagation(); void open(row, event.currentTarget); }}>内容を見る</button></div> : cell(row, key)}</td>)}</tr>)}</tbody></table></div>
    {!page.listings.length && <p>条件に合う求人はありません。</p>}
    <div className="jc-deal-controls"><span>{page.total && page.listings.length ? `${String(offset + 1)}〜${String(offset + page.listings.length)}件` : ''}</span><button className="jc-button" disabled={offset === 0} onClick={() => { setOffset(value => Math.max(0, value - 50)); }}>前の50件</button><button className="jc-button" disabled={page.next_offset === null} onClick={() => { setOffset(page.next_offset ?? 0); }}>次の50件</button></div></>}
    <div className="jc-deal-selection"><p>比較に選択：{selected.length}件（2〜5件）。ページを移動しても選択を保ちます。</p>{selected.map(row => <button className="jc-button" key={row.id} onClick={() => { setComparison(null); setSelected(items => items.filter(item => item.id !== row.id)); }}>{row.title ?? '求人名未取得'}を外す</button>)}<button className="jc-button" disabled={selected.length < 2 || busy} onClick={() => { void compare(); }}>横に並べて比べる</button></div>
    {popup && <ListingPopup navigation={{ ids: (page?.listings ?? []).map(row => `hubspot-history-${row.id}`), load: async (id, signal) => {
      const row = page?.listings.find(item => `hubspot-history-${item.id}` === id);
      if (!row) throw new Error('求人を取得できません');
      const result = await apiGet<HubSpotVersions>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/versions`, { signal, timeoutMs: 120_000 });
      if (!result.ok) throw result.error;
      return listingRecord(result.data, row);
    } }} job={popup.job} version={popup.job.versions.filter(item => item.kind === 'published').at(-1)} returnFocus={popup.opener} onClose={() => { setPopup(null); }} />}
    {comparison && <ParallelJobs jobs={comparison} />}
  </main>;
}
