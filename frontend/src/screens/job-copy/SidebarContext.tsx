import { useEffect, useState } from 'react';
import { contractLabel, dealName, listingsLabel, loadCompanyDeals } from './companyDeals';
import type { CompanyDeal } from './companyDeals';

/** 左の一覧の中身は、いま見ている階層の一つ下だけ。ホームは検索のみ、顧客は案件、案件と求人はその案件の求人。 */
export type SidebarLevel = 'home' | 'company' | 'deal' | 'listing';
export function sidebarMode(level: SidebarLevel, hasDeal: boolean): 'search' | 'deals' | 'listings' {
  if (level === 'company') return 'deals';
  return hasDeal && (level === 'deal' || level === 'listing') ? 'listings' : 'search';
}

/** 顧客を開いているとき、左に出す案件の一覧。案件を選ぶと、その案件の画面へ進む。 */
export function SidebarDeals({ companyId, onOpenDeal }: { companyId: string; onOpenDeal: (deal: { id: string; name: string }) => void }) {
  const [state, setState] = useState<{ deals: CompanyDeal[] } | 'error' | null>(null);
  useEffect(() => {
    let live = true;
    void loadCompanyDeals(companyId).then(result => { if (live) setState(result.ok ? { deals: result.data.deals } : 'error'); });
    return () => { live = false; };
  }, [companyId]);
  return <section className="jc-side-context" aria-label="この顧客の案件"><h3>この顧客の案件</h3>
    {state === null && <p role="status">案件を読み込んでいます…</p>}
    {state === 'error' && <p role="alert">案件を読み込めませんでした。時間を置いて再取得してください。</p>}
    {state && state !== 'error' && (state.deals.length ? <ul>{state.deals.map(deal => <li key={deal.id}><button type="button" onClick={() => { onOpenDeal({ id: deal.id, name: dealName(deal) }); }}><strong>{dealName(deal)}</strong><small>{deal.stage_label ?? 'ステージ未取得'} · {listingsLabel(deal)}</small><small>契約 {contractLabel(deal)}</small></button></li>)}</ul> : <p>納品管理の案件はありません。</p>)}
  </section>;
}
