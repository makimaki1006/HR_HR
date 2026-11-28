import { Fragment, useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { rememberDealName } from './navigation';
import './home.css';

/** 求人の画面の上に出す案件の数（多い求人は先頭だけ）。 */
const MAX_DEALS = 3;
interface Named { id: string; name: string }
interface Loaded { key: string; deals: Named[]; companies: Named[] }

/**
 * 求人の画面の上の「顧客：○○ ・ 案件：○○」。押すと顧客・案件の画面へ移る。
 * 名前は案件の画面と同じ読み取り（案件ごとの CRM の読み取り。サーバーで短く共有される）から取る。
 */
export function ListingParents({ dealIds, companyName, onOpenDeal, onOpenCompany }: { dealIds: readonly string[]; /** 求人から分かっている顧客名（案件から分かるまでの間と、分からないときに出す） */ companyName: string; onOpenDeal: (deal: Named, company?: Named) => void; onOpenCompany: (company: Named) => void }) {
  const ids = dealIds.slice(0, MAX_DEALS);
  const key = ids.join(',');
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  useEffect(() => {
    if (!key) return;
    const controller = new AbortController();
    void Promise.all(key.split(',').map(id => apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(id)}`, { signal: controller.signal, timeoutMs: 35_000 }).then(result => ({ id, result })))).then(results => {
      if (controller.signal.aborted) return;
      const deals: Named[] = []; const companies = new Map<string, string>();
      for (const { id, result } of results) {
        if (!result.ok) continue;
        const name = result.data.deal.name?.trim();
        if (name) { deals.push({ id, name }); rememberDealName(id, name); }
        const company = result.data.companies.find(item => item.is_primary) ?? result.data.companies[0];
        if (company?.name?.trim()) companies.set(company.id, company.name);
      }
      setLoaded({ key, deals, companies: [...companies].map(([id, name]) => ({ id, name })) });
    });
    return () => { controller.abort(); };
  }, [key]);
  const current = loaded?.key === key ? loaded : null;
  const companies = current?.companies ?? [];
  const firstCompany = companies.length === 1 ? companies[0] : undefined;
  if (!key && !companyName) return null;
  return <p className="jc-listing-parents" data-testid="listing-parents">
    <span>顧客：{companies.length ? companies.map((company, index) => <Fragment key={company.id}>{index > 0 && '、'}<button type="button" className="jc-go" onClick={() => { onOpenCompany(company); }}>{company.name}</button></Fragment>) : companyName || (key && !current ? '読み込んでいます…' : '不明')}</span>
    <span aria-hidden="true"> ・ </span>
    <span>案件：{!key ? '結び付いた案件はありません' : !current ? '読み込んでいます…' : current.deals.length ? <>{current.deals.map((deal, index) => <Fragment key={deal.id}>{index > 0 && '、'}<button type="button" className="jc-go" onClick={() => { onOpenDeal(deal, firstCompany); }}>{deal.name}</button></Fragment>)}{dealIds.length > MAX_DEALS && ` ほか${String(dealIds.length - MAX_DEALS)}件`}</> : '読み込めませんでした'}</span>
  </p>;
}
