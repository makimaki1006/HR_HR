import { useEffect, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { formatDateTimeJst } from './format';

export interface ChangedDeal { id: string; name: string | null; company: string | null; changed_listings: number; fields: string[]; last_changed_at: string }
export type RecentChangesResponse = { status: 'preparing' | 'failed' } | { status: 'ready'; report: { count: number; deals: ChangedDeal[] }; refresh_failed: boolean };
export const recentChangesRule = '今日を含む直近7日（日本時間）に、本文・給与・求人名の変更を確認した求人がある案件。掲載終了の求人も含みます。初回の記録は数えません。納品管理の案件だけを数えます。';

export function useRecentChanges(reload: number) {
  const [data, setData] = useState<RecentChangesResponse>({ status: 'preparing' });
  const [error, setError] = useState('');
  useEffect(() => {
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
        setError(!result.ok && result.error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : '変更のあった案件を読み込めませんでした。時間を置いて再読み込みしてください。');
      }
    };
    void load();
    return () => { controller.abort(); clearTimeout(timer); };
  }, [reload]);
  return { data, error };
}
export function RecentChangesButton({ data, open, onClick }: { data: RecentChangesResponse; open: boolean; onClick: () => void }) {
  return <button type="button" className="jh-card jh-plain" aria-pressed={open} onClick={onClick}>
    <strong className="jh-count">{data.status === 'ready' ? <>{data.report.count}<small>件</small></> : data.status === 'preparing' ? '集計中' : '取得できませんでした'}</strong>
    <span className="jh-card-title">直近7日に変更</span><small className="jh-rule">{recentChangesRule}</small>
  </button>;
}
export function RecentChangesList({ data, error, onOpenDeal }: { data: RecentChangesResponse; error: string; onOpenDeal: (deal: ChangedDeal) => void }) {
  return <section className="jh-list" aria-label="直近7日に変更の一覧"><h3>直近7日に変更{data.status === 'ready' ? `（${String(data.report.count)}件）` : ''}</h3>
    {data.status === 'preparing' && <p role="status">変更のあった案件を集計しています…</p>}
    {data.status === 'failed' && <p role="alert">{error || '変更のあった案件を読み込めませんでした。時間を置いて再読み込みしてください。'}</p>}
    {data.status === 'ready' && <>
      {data.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した件数を表示しています。時間を置いて再読み込みしてください。</p>}
      {data.report.count === 0 ? <p>該当する案件はありません。</p> : <div className="jh-scroll"><table><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">変わった求人の数</th><th scope="col">何が変わったか</th><th scope="col">最後に変わった日（日本時間）</th></tr></thead><tbody>
        {data.report.deals.map(deal => <tr key={deal.id}><td>{deal.company ?? '—'}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenDeal(deal); }}>{deal.name ?? '案件名未取得'}</button></th><td>{deal.changed_listings}件</td><td>{deal.fields.join('・')}</td><td>{formatDateTimeJst(deal.last_changed_at, '—')}</td></tr>)}
      </tbody></table></div>}
    </>}
  </section>;
}
