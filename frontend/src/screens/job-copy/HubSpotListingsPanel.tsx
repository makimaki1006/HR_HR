import { useEffect, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import type { JobCopyRecord } from './data';
import { listingRecord, mediaLabel } from './hubspotListings';
import type { HubSpotListing, HubSpotListingPage, HubSpotVersions } from './hubspotListings';
import { formatDateTimeJst } from './format';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
import { AREA_MASTER } from './areaMaster';

export function HubSpotListingsPanel({ onOpen }: { onOpen: (job: JobCopyRecord) => void }) {
  const [prefecture, setPrefecture] = useState('');
  const [title, setTitle] = useState('');
  const [media, setMedia] = useState('');
  const [titles, setTitles] = useState<string[]>([]);
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [history, setHistory] = useState<HubSpotVersions | null>(null);
  const request = useRef<AbortController | null>(null);
  useEffect(() => () => { request.current?.abort(); }, []);
  function start() {
    request.current?.abort(); const controller = new AbortController(); request.current = controller;
    setBusy(true); setError(''); return controller;
  }
  function failure(error: Error) {
    setError(error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '求人を取得できませんでした。閲覧権限を確認し、時間を置いて再取得してください。');
  }
  async function load(after?: string | null) {
    const controller = start(); setHistory(null);
    const query = new URLSearchParams();
    if (prefecture) query.set('prefecture', prefecture);
    if (title) query.set('title', title);
    if (media) query.set('media', media);
    if (after) query.set('after', after);
    const result = await apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { setPage(result.data); setTitles(result.data.titles); } else failure(result.error);
    setBusy(false);
  }
  function changeFilter(set: (value: string) => void, value: string) {
    request.current?.abort(); setBusy(false); setPage(null); setHistory(null); setError(''); set(value);
  }
  async function open(row: HubSpotListing) {
    const controller = start(); setHistory(null);
    const result = await apiGet<HubSpotVersions>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/versions`, { signal: controller.signal, timeoutMs: 120_000 });
    if (controller.signal.aborted) return;
    if (result.ok) { setHistory(result.data); onOpen(listingRecord(result.data)); } else failure(result.error);
    setBusy(false);
  }
  return <section className="jc-listings-panel" aria-labelledby="hubspot-listings-heading">
    <h2 id="hubspot-listings-heading">HubSpot の求人</h2>
    <p>媒体ごとに別の求人として表示します。応募は求人に関連する件数です。同じ応募が複数の求人に含まれることがあります。</p>
    <div className="jc-filters">
      <label>都道府県<select aria-label="都道府県" value={prefecture} onChange={event => { changeFilter(setPrefecture, event.target.value); }}><option value="">すべて</option>{AREA_MASTER.map(([pref]) => <option key={pref} value={pref}>{pref}</option>)}</select></label>
      <label>職種の分類<select aria-label="職種の分類" value={title} onChange={event => { changeFilter(setTitle, event.target.value); }}><option value="">すべて</option><option value="unknown">不明</option>{titles.map(item => <option key={item} value={item}>{item}</option>)}</select></label>
      <label>媒体<select aria-label="媒体" value={media} onChange={event => { changeFilter(setMedia, event.target.value); }}><option value="">すべて</option><option value="hrh">HRハッカー</option><option value="airwork">AirWork</option></select></label>
      <button type="button" className="jc-button" disabled={busy} onClick={() => { void load(); }}>求人を取得</button>
    </div>
    {busy && <p role="status">求人を取得しています…</p>}{error && <p role="alert">{error}</p>}
    {page && <><p>{page.listings.length}件を表示しています。{page.next_after ? 'まだ確認していない求人があります。「次の求人」で続きを確認できます。' : '一覧の最後まで確認しました。'}職種はIndeedの分類名との一致・含まれる文字で照合しています。近い職種は「不明」とします。</p>
      <div style={{ overflowX: 'auto' }}><table><thead><tr>{['媒体', '媒体の求人ID', '求人名', '都道府県', '市区町村', '職種の分類', '公開状況', '最終CSV検出日', '応募', '文面'].map(label => <th key={label}>{label}</th>)}</tr></thead><tbody>{page.listings.map(row => <tr key={row.id}>
        <td>{mediaLabel(row.media)}</td><td>{row.media === 'airwork' ? `${row.account_id ?? 'アカウント未取得'} / ${row.media_job_id}` : row.media_job_id}</td><td>{row.title ?? '未取得'}</td><td>{row.prefecture ?? '不明'}</td><td>{row.municipality ?? '不明'}</td><td>{row.category ?? '不明'}</td><td>{row.publication_status ?? '未取得'}</td><td>{row.last_csv_detected_at ? formatDateTimeJst(row.last_csv_detected_at, '不明') : '未取得'}</td><td>{row.application_count === null ? '未取得' : `${String(row.application_count)}件`}</td><td><button type="button" disabled={busy} onClick={() => { void open(row); }}>{row.title ?? '求人'}の版を見る</button></td>
      </tr>)}</tbody></table></div>
      {!page.listings.length && <p>今回確認した範囲では、条件に一致する求人はありません。</p>}
      {page.next_after && <button type="button" className="jc-button" disabled={busy} onClick={() => { void load(page.next_after); }}>次の求人</button>}
    </>}
    {history && <p role="status">取得した文面の版は{history.versions.length}件です。本文の履歴：{history.history_counts[history.listing.media === 'hrh' ? 'hrh_kyuujinhyou_honbun' : 'shigotonaiyou'] ?? 0}件{history.listing.media === 'hrh' ? `、画像の履歴：${String(history.history_counts.hrh_kyuujinhyou_gazou ?? 0)}件` : ''}。{history.history_may_be_incomplete && '履歴は項目ごとに20件までの可能性があり、過去の版がすべて含まれているとは限りません。'}日時は保存された日時で、掲載開始日時は不明です。</p>}
  </section>;
}
