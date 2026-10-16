import { ownerParams } from './OwnerTodo';
import { useJobCopySearch } from './navigation';
import { useEffect, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { formatDateTimeJst } from './format';

export interface ChangedDeal { id: string; name: string | null; company: string | null }
export interface ChangedListing { id: string; title: string | null; media: string | null; publication_status: string | null; deals: ChangedDeal[]; fields: string[]; last_changed_at: string; classifications?: string[] }
export interface ChangeCategory { key: string; title: string; rule: string; count: number }
export type ChangeBreakdown = { status: 'preparing' } | { status: 'ready'; cards: ChangeCategory[] };
export type RecentChangesResponse = { status: 'preparing' | 'failed' } | { status: 'ready'; report: { count: number; listings: ChangedListing[]; breakdown?: ChangeBreakdown }; refresh_failed: boolean };
export const recentChangesRule = '今日を含む直近7日（日本時間）に、本文・給与・求人名の変更を確認した求人。掲載終了の求人も含みます。初回の記録は数えません。納品管理の案件に結び付く求人だけを数えます。';

export function useRecentChanges(reload: number, enabled = true, defaultToSelf = false) {
  const ownerQuery = ownerParams(useJobCopySearch()) || (defaultToSelf ? 'owner=me' : '');
  const [loadedQuery, setLoadedQuery] = useState<string | null>(null);
  const [data, setData] = useState<RecentChangesResponse>({ status: 'preparing' });
  const [error, setError] = useState('');
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      const result = await apiGet<RecentChangesResponse>(`/api/job-copy/recent-changes${ownerQuery ? `?${ownerQuery}` : ''}`, { signal: controller.signal });
      if (controller.signal.aborted) return;
      if (result.ok && ['ready', 'preparing', 'failed'].includes(result.data.status)) {
        setData(result.data); setLoadedQuery(ownerQuery); setError('');
        if (result.data.status === 'preparing' || (result.data.status === 'ready' && result.data.report.breakdown?.status === 'preparing')) timer = setTimeout(() => { void load(); }, 2_000);
      } else {
        setData({ status: 'failed' }); setLoadedQuery(ownerQuery);
        setError(!result.ok && result.error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : '変更のあった求人を読み込めませんでした。時間を置いて再読み込みしてください。');
      }
    };
    void load();
    return () => { controller.abort(); clearTimeout(timer); };
  }, [reload, enabled, ownerQuery]);
  return { data: loadedQuery === ownerQuery ? data : { status: 'preparing' as const }, error };
}
export function RecentChangesButton({ data, open, onClick }: { data: RecentChangesResponse; open: boolean; onClick: () => void }) {
  return <button type="button" className="jh-card jh-plain" aria-pressed={open} onClick={onClick}>
    <strong className="jh-count">{data.status === 'ready' ? <>{data.report.count}<small>件</small></> : data.status === 'preparing' ? '集計中' : '取得できませんでした'}</strong>
    <span className="jh-card-title">直近7日に変更</span><small className="jh-rule">{recentChangesRule}</small>
  </button>;
}
export function RecentChangesList({ data, error, onOpenDeal, onOpenListing }: { data: RecentChangesResponse; error: string; onOpenDeal: (deal: ChangedDeal) => void; onOpenListing: (id: string) => void }) {
  const [selected, setSelected] = useState<string | null>(null);
  const [hint, setHint] = useState<string | null>(null);
  const breakdown = data.status === 'ready' ? data.report.breakdown : undefined;
  const cards = breakdown?.status === 'ready' ? breakdown.cards : [];
  const active = cards.find(card => card.key === selected);
  const rows = data.status === 'ready' ? data.report.listings.filter(row => !active || row.classifications?.includes(active.key)) : [];
  return <section className="jh-list" aria-label="直近7日に変更の一覧"><h3>直近7日に変更{data.status === 'ready' ? `（${String(data.report.count)}件）` : ''}</h3>
    {data.status === 'preparing' && <p role="status">変更のあった求人を集計しています…</p>}
    {data.status === 'failed' && <p role="alert">{error || '変更のあった求人を読み込めませんでした。時間を置いて再読み込みしてください。'}</p>}
    {data.status === 'ready' && <>
      {data.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した件数を表示しています。時間を置いて再読み込みしてください。</p>}
      {breakdown?.status === 'ready' ? <>
        <div className="jh-change-breakdown" aria-label="変更求人の内訳">
          <button type="button" aria-pressed={!active} onClick={() => { setSelected(null); setHint(null); }}>すべて {data.report.count}件</button>
          {[...cards].sort((a, b) => categoryOrder.indexOf(a.key) - categoryOrder.indexOf(b.key)).map(card => <button key={card.key} type="button" className={card.count === 0 ? 'jh-change-zero' : ''} aria-pressed={active?.key === card.key} title={card.rule} aria-describedby="recent-change-rule" onMouseEnter={() => { setHint(card.key); }} onMouseLeave={() => { setHint(null); }} onFocus={() => { setHint(card.key); }} onBlur={() => { setHint(null); }} onClick={() => { setSelected(active?.key === card.key ? null : card.key); }}>{categoryLabel(card)} {card.count}件</button>)}
        </div>
        <p id="recent-change-rule" className="jh-note">{cards.find(card => card.key === hint)?.rule ?? active?.rule ?? '応募の動きは求人ごとに1つです。課金とデータの遅れの印は重なることがあります。変更後の結果を示すものではありません。'}</p>
      </> : <p className="jh-note">内訳は集計中です。</p>}
      {data.report.count === 0 ? <p>該当する求人はありません。</p> : <div className="jh-scroll"><table><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">求人名</th><th scope="col">媒体</th><th scope="col">状態</th><th scope="col">分類</th><th scope="col">何が変わったか</th><th scope="col">最後に変わった日時（日本時間）</th></tr></thead><tbody>
        {rows.map(row => <tr key={row.id}><td>{[...new Set(row.deals.map(deal => deal.company ?? '—'))].join('・')}</td><td>{row.deals.map((deal, index) => <span key={deal.id}>{index > 0 && '・'}<button type="button" className="jh-link" onClick={() => { onOpenDeal(deal); }}>{deal.name ?? '案件名未取得'}</button></span>)}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenListing(row.id); }}>{row.title ?? '求人名未取得'}</button></th><td>{row.media === 'hrh' ? 'HRハッカー' : row.media === 'airwork' ? 'AirWork' : '—'}</td><td>{row.publication_status ?? '—'}</td><td>{row.classifications?.map(key => { const card = cards.find(c => c.key === key); return card ? <span key={key} className="jh-change-chip" title={card.rule}>{categoryLabel(card)}</span> : null; })}</td><td>{row.fields.join('・')}</td><td>{formatDateTimeJst(row.last_changed_at, '—')}</td></tr>)}
      </tbody></table>{rows.length === 0 && <p>この分類の求人はありません。</p>}</div>}
    </>}
  </section>;
}

const categoryOrder = ['less', 'to_zero', 'zero_stay', 'flat', 'more', 'costly', 'stale', 'out_of_scope', 'unknown'];
const shortLabels: Record<string, string> = { less: '応募が減った', to_zero: 'ゼロになった', zero_stay: 'ゼロのまま', flat: 'ほぼ同じ', more: '応募が増えた', costly: '課金が高いまま応募が少ない', stale: 'データが止まっている', out_of_scope: '対象外（掲載終了など）', unknown: '判定できない' };
function categoryLabel(card: ChangeCategory) { return shortLabels[card.key] ?? card.title; }
