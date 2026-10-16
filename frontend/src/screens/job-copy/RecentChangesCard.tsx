import { useEffect, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { formatDateTimeJst } from './format';

export interface ChangedDeal { id: string; name: string | null; company: string | null }
export interface ChangedListing { id: string; title: string | null; media: string | null; publication_status: string | null; deals: ChangedDeal[]; fields: string[]; last_changed_at: string }
export type RecentChangesResponse = { status: 'preparing' | 'failed' } | { status: 'ready'; report: { count: number; listings: ChangedListing[] }; refresh_failed: boolean };
export const recentChangesRule = '今日を含む直近7日（日本時間）に、本文・給与・求人名の変更を確認した求人。掲載終了の求人も含みます。初回の記録は数えません。納品管理の案件に結び付く求人だけを数えます。';

export function useRecentChanges(reload: number, enabled = true) {
  const [data, setData] = useState<RecentChangesResponse>({ status: 'preparing' });
  const [error, setError] = useState('');
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      const result = await apiGet<RecentChangesResponse>('/api/job-copy/recent-changes', { signal: controller.signal });
      if (controller.signal.aborted) return;
      if (result.ok && ['ready', 'preparing', 'failed'].includes(result.data.status)) {
        setData(result.data); setError('');
        if (result.data.status === 'preparing') timer = setTimeout(() => { void load(); }, 2_000);
      } else {
        setData({ status: 'failed' });
        setError(!result.ok && result.error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : '変更のあった求人を読み込めませんでした。時間を置いて再読み込みしてください。');
      }
    };
    void load();
    return () => { controller.abort(); clearTimeout(timer); };
  }, [reload, enabled]);
  return { data, error };
}
export function RecentChangesButton({ data, open, onClick }: { data: RecentChangesResponse; open: boolean; onClick: () => void }) {
  return <button type="button" className="jh-card jh-plain" aria-pressed={open} onClick={onClick}>
    <strong className="jh-count">{data.status === 'ready' ? <>{data.report.count}<small>件</small></> : data.status === 'preparing' ? '集計中' : '取得できませんでした'}</strong>
    <span className="jh-card-title">直近7日に変更</span><small className="jh-rule">{recentChangesRule}</small>
  </button>;
}
export function RecentChangesList({ data, error, onOpenDeal, onOpenListing }: { data: RecentChangesResponse; error: string; onOpenDeal: (deal: ChangedDeal) => void; onOpenListing: (id: string) => void }) {
  return <section className="jh-list" aria-label="直近7日に変更の一覧"><h3>直近7日に変更{data.status === 'ready' ? `（${String(data.report.count)}件）` : ''}</h3>
    {data.status === 'preparing' && <p role="status">変更のあった求人を集計しています…</p>}
    {data.status === 'failed' && <p role="alert">{error || '変更のあった求人を読み込めませんでした。時間を置いて再読み込みしてください。'}</p>}
    {data.status === 'ready' && <>
      {data.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した件数を表示しています。時間を置いて再読み込みしてください。</p>}
      {data.report.count === 0 ? <p>該当する求人はありません。</p> : <div className="jh-scroll"><table><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">求人名</th><th scope="col">媒体</th><th scope="col">状態</th><th scope="col">何が変わったか</th><th scope="col">最後に変わった日時（日本時間）</th></tr></thead><tbody>
        {data.report.listings.map(row => <tr key={row.id}><td>{[...new Set(row.deals.map(deal => deal.company ?? '—'))].join('・')}</td><td>{row.deals.map((deal, index) => <span key={deal.id}>{index > 0 && '・'}<button type="button" className="jh-link" onClick={() => { onOpenDeal(deal); }}>{deal.name ?? '案件名未取得'}</button></span>)}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenListing(row.id); }}>{row.title ?? '求人名未取得'}</button></th><td>{row.media === 'hrh' ? 'HRハッカー' : row.media === 'airwork' ? 'AirWork' : '—'}</td><td>{row.publication_status ?? '—'}</td><td>{row.fields.join('・')}</td><td>{formatDateTimeJst(row.last_changed_at, '—')}</td></tr>)}
      </tbody></table></div>}
    </>}
  </section>;
}
