import { useEffect, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { usePreparingRetry } from '../../shared/preparingRetry';
import { jobsHref } from './ConsultantRisk';
import { draftStateLabel, mediaLabel, publicationLabel } from './hubspotListings';
import type { HubSpotListingPage } from './hubspotListings';
import { loadCompanies, peekCompany } from './listingDetailCache';
import { navigateJobCopy, updateJobCopyQuery, useJobCopySearch } from './navigation';
import { periodLabel } from './OccupationBulk';
import { ownerParams } from './OwnerTodo';
import { PageNav } from './SharedTable';
import { ChoiceSwitch, StatusFilter } from './StatusSwitch';
import { STATUS_PUBLICATION, countsFromServer, useStatusFilter } from './statusFilter';
import './occupation-bulk.css';
import './listing-board.css';

const PAGE = 50;
type ListingTab = 'listings' | 'occupations' | 'reasons';
const TABS: readonly { key: ListingTab; label: string }[] = [{ key: 'listings', label: '求人の一覧' }, { key: 'occupations', label: '職種の指定' }, { key: 'reasons', label: '応募理由（全社の集計）' }];

/** 「求人」のメニューの中の切り替え。職種の指定と応募理由は、上のメニューから外してここに置く。 */
export function ListingTabs({ current }: { current: ListingTab }) {
  return <div className="lb-tabs"><ChoiceSwitch value={current} label="求人のメニュー" options={TABS} onChange={view => { updateJobCopyQuery({ view, deal: null, risk_deal: null }); }} /></div>;
}

/** 担当の求人の一覧。左の「求人を探す」と同じ一覧を 1 画面で見る。行を押すと求人のページへ移る。 */
export function ListingBoard() {
  const search = useJobCopySearch();
  const ownerQuery = ownerParams(search) || 'owner=me';
  const [status, setStatus] = useStatusFilter();
  const publication = STATUS_PUBLICATION[status];
  const [text, setText] = useState('');
  const [q, setQ] = useState('');
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<HubSpotListingPage | null>(null);
  const [error, setError] = useState('');
  const [reload, setReload] = useState(0);
  const [, setCompanyTick] = useState(0);
  const key = `${ownerQuery}|${publication}|${q}|${String(offset)}`;
  const [loadedKey, setLoadedKey] = useState('');
  const busy = loadedKey !== `${key}|${String(reload)}`;
  useEffect(() => { const timer = window.setTimeout(() => { setQ(text.trim()); setOffset(0); }, 250); return () => { window.clearTimeout(timer); }; }, [text]);
  useEffect(() => {
    const controller = new AbortController();
    const query = new URLSearchParams({ ...Object.fromEntries(new URLSearchParams(ownerQuery)), sort: 'bulk', offset: String(offset) });
    if (publication) query.set('publication', publication);
    if (q) query.set('q', q.slice(0, 100));
    void apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (controller.signal.aborted) return;
      setLoadedKey(`${key}|${String(reload)}`);
      if (!result.ok) { setError(result.error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : '求人を読み込めませんでした。時間を置いて再取得してください。'); return; }
      setError(''); setPage(result.data);
      const ids = result.data.listings.map(row => row.id);
      if (ids.length) void loadCompanies(ids).then(() => { if (!controller.signal.aborted) setCompanyTick(n => n + 1); });
    });
    return () => { controller.abort(); };
  }, [ownerQuery, publication, q, offset, key, reload]);
  usePreparingRetry(page?.status === 'preparing', 5_000, loadedKey, () => { if (!busy) setReload(n => n + 1); }, key);
  const rows = page?.listings ?? [];
  const total = page?.status === 'ready' ? page.total : 0;
  const open = (id: string) => { navigateJobCopy(new URL(jobsHref(search, null, id), window.location.origin)); };
  return <div className="ob-screen lb-screen" id="job-details">
    <header className="lb-head"><h1>求人{page?.status === 'ready' && <small> {total.toLocaleString('ja-JP')}件</small>}</h1><button type="button" className="jc-button" onClick={() => { setReload(n => n + 1); }}>再読み込み</button></header>
    <ListingTabs current="listings" />
    <div className="ob-controls">
      <StatusFilter status={status} counts={countsFromServer(page?.publication_counts)} onChange={next => { setStatus(next); setOffset(0); }} />
      <label className="ob-search"><span className="jc-visually-hidden">求人名・顧客名で探す</span><input type="search" value={text} placeholder="求人名・顧客名で探す" onChange={event => { setText(event.target.value); }} /></label>
    </div>
    {error && <p role="alert" className="ob-error">{error}</p>}
    {page?.status === 'preparing' && <p role="status">求人の一覧を準備しています。しばらくお待ちください。</p>}
    <section className="ob-table-wrap" aria-label="求人の一覧" aria-busy={busy}>
      <table className="ob-table lb-table">
        <thead><tr><th scope="col" className="lb-title">求人 ／ 顧客</th><th scope="col">媒体</th><th scope="col">状況</th><th scope="col">勤務地</th><th scope="col" className="ob-num">応募（HubSpot）</th><th scope="col" className="ob-num">媒体の応募（直近 4 週）</th><th scope="col">比べる職種</th><th scope="col">求人の案</th></tr></thead>
        <tbody>
          {rows.map(row => {
            const recent = row.recent_performance;
            return <tr key={row.id} onClick={event => { if (!(event.target as HTMLElement).closest('button, a')) open(row.id); }}>
              <th scope="row"><button type="button" className="jc-row-name" onClick={() => { open(row.id); }}>{row.title ?? '求人名なし'}</button><div className="ob-sub">{peekCompany(row.id) ?? '顧客名を確認中'}</div></th>
              <td className="lb-nowrap">{mediaLabel(row.media)}</td>
              <td className="lb-nowrap">{publicationLabel(row.publication_status)}</td>
              <td>{row.prefecture ?? '不明'}{row.municipality ?? ''}</td>
              <td className="ob-num">{row.application_count === null ? '不明' : `${String(row.application_count)} 件`}</td>
              <td className="ob-num lb-media">{recent?.applications == null ? '—' : <>{String(recent.applications)} 件<span className="ob-period">（{periodLabel(recent.period_start, recent.period_end)}）</span></>}</td>
              <td className={row.indeed_title_state === 'set' ? undefined : 'lb-muted'}>{row.indeed_title_state === 'set' ? row.indeed_title : row.indeed_title_state === 'none' ? '市場と比べない' : '未指定'}</td>
              <td className={row.draft_status ? 'lb-nowrap' : 'lb-muted'}>{row.draft_status ? draftStateLabel(row.draft_status) : '—'}</td>
            </tr>;
          })}
          {!rows.length && page?.status === 'ready' && !busy && <tr><td colSpan={8} className="ob-empty">{q ? '一致する求人はありません。' : '担当の求人はありません。'}</td></tr>}
          {!page && !error && <tr><td colSpan={8} className="ob-empty" role="status">読み込み中…</td></tr>}
        </tbody>
      </table>
    </section>
    <PageNav className="ob-paging" label="求人のページ送り" size={PAGE} hasPrev={!busy && offset > 0} hasNext={!busy && page?.status === 'ready' && page.next_offset !== null} onPrev={() => { setOffset(Math.max(0, offset - PAGE)); }} onNext={() => { if (page?.status === 'ready' && page.next_offset !== null) setOffset(page.next_offset); }}>{page?.status === 'ready' ? total ? `${String(offset + 1)}〜${String(offset + rows.length)} 件目 ／ ${String(total)} 件` : '0 件' : ''}</PageNav>
    <p className="ob-note">応募（HubSpot）は、求人に記録された応募の全期間の件数です。媒体の応募は Indeed・AirWork の週ごとの実績で、参考に出しています。並びは掲載中の求人を先に、媒体の直近 4 週の応募が多い順です。</p>
  </div>;
}
