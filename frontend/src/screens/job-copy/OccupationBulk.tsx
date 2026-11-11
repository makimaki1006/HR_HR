import { ownerParams } from './OwnerTodo';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { apiGet, ApiHttpError, apiPut, AuthRequiredError } from '../../api/client';
import type { HubSpotListing, HubSpotListingPage } from './hubspotListings';
import { mediaLabel, publicationLabel } from './hubspotListings';
import { invalidateListing, loadCompanies, peekCompany } from './listingDetailCache';
import { errorText } from './MarketOccupationSlot';
import './market-occupation.css';
import './occupation-bulk.css';

/** 「市場と比べない」の選択肢の表示。保存値は title: null, compare: false。 */
export const NONE_LABEL = '市場と比べない';
const PAGE = 50;
type Tab = 'unset' | 'assigned' | 'all';
interface Saved { label: string; fromUnset: boolean; live: boolean }
export interface BulkCounts { set: number; none: number; unset: number }

const tabState = (tab: Tab) => tab === 'unset' ? 'unset' : tab === 'assigned' ? 'assigned' : '';
const listingUrl = (query: Record<string, string>) => `/api/job-copy/listings?${new URLSearchParams(query).toString()}`;
const percent = (done: number, total: number) => total > 0 ? Math.min(100, Math.round(done / total * 1000) / 10) : 0;

/** 直近4週の期間の表示。今年（日本時間）なら月日だけ、そうでなければ年も付ける。 */
export function periodLabel(start: string, end: string, now = new Date()): string {
  const year = String(new Date(now.getTime() + 9 * 3600_000).getUTCFullYear());
  const thisYear = start.startsWith(year) && end.startsWith(year);
  const day = (value: string) => (thisYear ? value.slice(5) : value).replaceAll('-', '/');
  return `${day(start)}〜${day(end)}`;
}

/** 一覧を読めなかったときの案内。400 は理由ごとに分ける。 */
function loadErrorText(error: unknown): string {
  if (error instanceof AuthRequiredError) return 'Googleでログインしてから再取得してください。';
  if (error instanceof ApiHttpError && error.status === 400) {
    const code = typeof error.body === 'object' && error.body !== null && 'code' in error.body ? (error.body as { code?: unknown }).code : undefined;
    if (code === 'listing_deal_not_found') return 'この案件の求人が見つかりませんでした。「すべてに戻す」で全体を表示できます。';
    if (code === 'listing_filter_invalid') return '検索や絞り込みの指定が正しくありません（検索の文字は 100 文字まで）。指定を見直してください。';
    return 'URL の指定が正しくないため読み込めませんでした。メニューから開き直してください。';
  }
  return '求人を読み込めませんでした。時間を置いて再取得してください。';
}

/** 求人ごとに、市場と比べる Indeed の職種を指定する一括画面。自動では決めない。 */
export function OccupationBulk() {
  const search = useJobCopySearch();
  // `deal=<取引ID>` の指定があれば、その取引の求人だけを出す（担当では絞らない）。
  const deal = new URLSearchParams(search).get('deal')?.trim() ?? '';
  const ownerQuery = deal ? `deal=${encodeURIComponent(deal)}` : ownerParams(search) || 'owner=me';
  const [tab, setTab] = useState<Tab>('unset');
  const [text, setText] = useState('');
  const [q, setQ] = useState('');
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const [live, setLive] = useState<BulkCounts | null>(null);
  const [error, setError] = useState('');
  const [loadedKey, setLoadedKey] = useState('');
  const [reload, setReload] = useState(0);
  const [titles, setTitles] = useState<string[]>([]);
  const [saved, setSaved] = useState<Record<string, Saved>>({});
  const [, setCompanyTick] = useState(0);
  const [dealInfo, setDealInfo] = useState<{ id: string; name: string } | null>(null);
  // 別の案件に変わったら、前の案件名は出さない。
  const dealName = dealInfo?.id === deal ? dealInfo.name : '';
  const request = useRef<AbortController | null>(null);

  useEffect(() => { const timer = window.setTimeout(() => { setQ(text.trim()); setOffset(0); }, 250); return () => { window.clearTimeout(timer); }; }, [text]);

  const key = `${ownerQuery}|${tab}|${q}|${String(offset)}|${String(reload)}`;
  const busy = loadedKey !== key;
  const load = useCallback(async () => {
    request.current?.abort(); const controller = new AbortController(); request.current = controller;
    const query: Record<string, string> = { ...Object.fromEntries(new URLSearchParams(ownerQuery)), sort: 'bulk', offset: String(offset) };
    if (tabState(tab)) query.indeed_state = tabState(tab);
    if (q) query.q = q;
    const [result, liveResult] = await Promise.all([
      apiGet<HubSpotListingPage>(listingUrl(query), { signal: controller.signal, timeoutMs: 120_000 }),
      // 進み具合: 掲載中の求人だけの件数。行は要らないので、空の続きの位置を読む。
      apiGet<HubSpotListingPage>(listingUrl({ ...Object.fromEntries(new URLSearchParams(ownerQuery)), sort: 'bulk', publication: '掲載中', offset: '100000' }), { signal: controller.signal, timeoutMs: 120_000 }),
    ]);
    if (controller.signal.aborted) return;
    setLoadedKey(key);
    if (!result.ok) { setDealInfo(null); setError(loadErrorText(result.error)); return; }
    setError(''); setPage(result.data); setSaved({});
    const selected = result.data.selected_deal;
    setDealInfo(selected ? { id: selected.id, name: selected.name ?? '' } : null);
    if (result.data.titles.length) setTitles(result.data.titles);
    setLive(liveResult.ok ? liveResult.data.indeed_title_counts ?? null : null);
    const ids = result.data.listings.map(row => row.id);
    if (ids.length) void loadCompanies(ids).then(() => { if (!controller.signal.aborted) setCompanyTick(n => n + 1); });
  }, [tab, q, offset, key, ownerQuery]);
  // eslint-disable-next-line react-hooks/set-state-in-effect -- 条件が変わるたびに一覧を取り直す（取得後に状態を入れる）
  useEffect(() => { void load(); return () => { request.current?.abort(); }; }, [load]);
  useEffect(() => {
    if (page?.status !== 'preparing' || busy) return;
    const timer = window.setTimeout(() => { setReload(n => n + 1); }, 5_000);
    return () => { window.clearTimeout(timer); };
  }, [page, busy]);

  const savedRows = Object.values(saved);
  const base = page?.indeed_title_counts;
  const doneNow = savedRows.filter(item => item.fromUnset).length;
  const counts = base ? { unset: base.unset - doneNow, assigned: base.set + base.none + doneNow, all: base.set + base.none + base.unset } : null;
  const liveTotal = live ? live.set + live.none + live.unset : 0;
  const liveDone = live ? live.set + live.none + savedRows.filter(item => item.fromUnset && item.live).length : 0;
  const tabs: [Tab, string, number | null][] = [['unset', '未指定', counts?.unset ?? null], ['assigned', '指定済み', counts?.assigned ?? null], ['all', 'すべて', counts?.all ?? null]];
  const total = page?.status === 'ready' ? page.total : 0;
  const rows = page?.listings ?? [];

  return <div className="ob-screen">
    <nav className="ob-crumb" aria-label="現在地"><a href="/app/job-copy">案件管理</a><span aria-hidden="true">›</span><span aria-current="page"><strong>市場と比べる職種を指定する</strong></span></nav>
    <section className="ob-intro" aria-label="説明と進み具合">
      <p>求人ごとに、Indeed のどの職種と比べるかを指定します。自動では決めません。指定した求人から、市場の数字（競合の求人数・求人を見た人）がつながります。掲載中で、直近 4 週の応募が多い求人から並べています。</p>
      <div className="ob-progress">
        <div><span>職種を指定済みの掲載中の求人</span><strong>{live ? `${String(liveDone)} / ${String(liveTotal)} 件` : '—'}</strong></div>
        <div className="ob-bar" role="progressbar" aria-label="職種を指定済みの掲載中の求人" aria-valuemin={0} aria-valuemax={liveTotal} aria-valuenow={liveDone}><span style={{ width: `${String(percent(liveDone, liveTotal))}%` }} /></div>
      </div>
    </section>
    {deal && <div className="ob-deal" role="status"><span><strong>{dealName || '選んだ案件'}</strong> の求人だけを表示しています</span><button type="button" className="jc-button" onClick={() => { updateJobCopyQuery({ deal: null }); setOffset(0); }}>すべてに戻す</button></div>}
    <div className="ob-controls">
      <div role="tablist" aria-label="状態">{tabs.map(([id, label, count]) => <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => { setTab(id); setOffset(0); }}>{label}（{count ?? '—'}）</button>)}</div>
      <label className="ob-search"><span className="jc-visually-hidden">求人名・顧客名で探す</span><input type="search" value={text} placeholder="求人名・顧客名で探す" onChange={event => { setText(event.target.value); }} /></label>
    </div>
    {error && <p role="alert" className="ob-error">{error} <button type="button" className="jc-button" onClick={() => { setReload(n => n + 1); }}>再取得</button></p>}
    {page?.status === 'preparing' && <p role="status">求人の一覧を準備しています。しばらくお待ちください。</p>}
    <section className="ob-table-wrap" aria-label="職種名の一覧" aria-busy={busy}>
      <table className="ob-table">
        <thead><tr><th scope="col">顧客 ／ 求人</th><th scope="col">手がかり</th><th scope="col" className="ob-num">直近 4 週の応募</th><th scope="col">比べる Indeed の職種</th><th scope="col"><span className="jc-visually-hidden">操作</span></th></tr></thead>
        <tbody>
          {rows.map(row => <BulkRow key={row.id} row={row} titles={titles} done={saved[row.id]}
            onSaved={(next, fromUnset) => { invalidateListing(row.id); setSaved(items => ({ ...items, [row.id]: { label: next, fromUnset, live: publicationLabel(row.publication_status) === '掲載中' } })); }} />)}
          {!rows.length && page?.status === 'ready' && !busy && <tr><td colSpan={5} className="ob-empty">{tab === 'unset' && !q ? '職種が未指定の求人はありません。' : '一致する求人はありません。'}</td></tr>}
          {!page && !error && <tr><td colSpan={5} className="ob-empty" role="status">読み込み中…</td></tr>}
        </tbody>
      </table>
    </section>
    <div className="ob-paging">
      <span aria-live="polite">{page?.status === 'ready' ? total ? `${String(offset + 1)}〜${String(offset + rows.length)} 件目 ／ ${String(total)} 件` : '0 件' : ''}</span>
      <button type="button" className="jc-button" disabled={busy || offset === 0} onClick={() => { setOffset(Math.max(0, offset - PAGE)); }}>前の{PAGE}件</button>
      <button type="button" className="jc-button" disabled={busy || page?.status !== 'ready' || page.next_offset === null} onClick={() => { if (page?.status === 'ready' && page.next_offset !== null) setOffset(page.next_offset); }}>次の{PAGE}件</button>
    </div>
    <p className="ob-note">直近 4 週の応募は、媒体（Indeed・AirWork）の週ごとの実績の、最新の続いた 4 週の合計です。4週分の実績がそろっていない求人は「—」です。</p>
    <p className="ob-note">指定は求人 1 件ずつです。HubSpot の求人の項目「比べる Indeed 職種」に入るので、HubSpot の画面からも直せます。指定していない求人は、市場との比較を出しません。</p>
  </div>;
}

function BulkRow({ row, titles, done, onSaved }: { row: HubSpotListing; titles: readonly string[]; done: Saved | undefined; onSaved: (label: string, fromUnset: boolean) => void }) {
  const listId = useId();
  const [choice, setChoice] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const valid = choice === NONE_LABEL || titles.includes(choice);
  const initial = row.indeed_title_state ?? 'unset';
  const current = initial === 'set' ? row.indeed_title ?? '' : initial === 'none' ? NONE_LABEL : '';
  const company = peekCompany(row.id);
  const name = row.title ?? '求人名未取得';
  const jobKind = row.summary?.job_kind?.trim() ?? '';
  // Indeed 表示職種名は HRハッカーの本文にだけある（AirWork の occupation は「職種」と同じ値）。
  const indeedName = row.media === 'hrh' ? row.summary?.occupation?.trim() ?? '' : '';
  const recent = row.recent_performance;
  const shown = done ? done.label : current;
  const wasUnset = initial === 'unset';

  async function save() {
    if (!valid) return;
    setSaving(true); setError('');
    const body = choice === NONE_LABEL ? { title: null, compare: false } : { title: choice, compare: true };
    const result = await apiPut<{ indeed_title: string | null; indeed_title_state: 'set' | 'none' }>(`/api/job-copy/listings/${encodeURIComponent(row.id)}/indeed-title`, body);
    setSaving(false);
    if (result.ok) onSaved(result.data.indeed_title_state === 'none' ? NONE_LABEL : result.data.indeed_title ?? choice, wasUnset);
    else setError(errorText(result.error));
  }

  return <tr data-state={done || !wasUnset ? 'done' : 'todo'}>
    <td><div className="ob-sub">{company ?? '顧客名を確認中'} ・ {mediaLabel(row.media)}{publicationLabel(row.publication_status) !== '掲載中' && <> ・ {publicationLabel(row.publication_status)}</>}</div><a href={`/app/job-copy?listing=${encodeURIComponent(row.id)}`}>{name}</a></td>
    <td className="ob-hint">{!jobKind && !indeedName ? '—' : <dl className="ob-hints">
      {jobKind && <div><dt>求人票の職種</dt><dd>{jobKind}</dd></div>}
      {indeedName && indeedName !== jobKind && <div><dt>Indeed 表示職種名</dt><dd>{indeedName}</dd></div>}
    </dl>}</td>
    <td className="ob-num">{recent?.applications == null ? '—' : <>{String(recent.applications)} 件<span className="ob-period">（{periodLabel(recent.period_start, recent.period_end)}）</span></>}</td>
    <td>{done || !wasUnset
      ? <><strong>{shown}</strong> <span className="mo-chip mo-chip-set">指定済み</span></>
      : <><input className="ob-select" list={listId} value={choice} aria-label={`${name} と比べる Indeed の職種`} placeholder="職種を選ぶ・探す" onChange={event => { setChoice(event.target.value); }} />
        <datalist id={listId}><option value={NONE_LABEL} />{titles.map(title => <option key={title} value={title} />)}</datalist>
        {choice && !valid && <div className="ob-sub">一覧にある職種を選んでください。</div>}
        {error && <p role="alert" className="ob-error">{error}</p>}</>}</td>
    <td className="ob-action">{done || !wasUnset ? <button type="button" className="jc-button" disabled>指定済み</button> : <button type="button" className="jc-button jc-primary" disabled={!valid || saving} onClick={() => { void save(); }}>{saving ? '保存中…' : '指定する'}</button>}</td>
  </tr>;
}
