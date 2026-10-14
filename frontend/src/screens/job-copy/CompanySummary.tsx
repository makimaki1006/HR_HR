import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { HubSpotListing } from './hubspotListings';
import { ROLLUP_RULES } from './dealSummaryModel';
import { loadDealData } from './dealSummaryLoad';
import type { DealData } from './dealSummaryLoad';
import { SummaryView } from './DealSummary';
import type { Billing, SummarySource } from './DealSummary';

interface CompanyDeals { company: { id: string; name: string | null }; deals: { id: string; name: string | null }[] }

/** 事業所（会社）ひとつ分のまとめ。取引ごとの求人を集めて、同じ求人は1回だけ数える。 */
export function CompanySummary({ company, billingByJob, onOpenListing }: { company: { id: string; name: string }; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void }) {
  const [source, setSource] = useState<SummarySource>({ status: 'loading' });
  useEffect(() => {
    const controller = new AbortController();
    const fail = () => { if (!controller.signal.aborted) setSource({ status: 'error', message: 'この事業所の取引を読み込めませんでした。時間を置いて再取得してください。' }); };
    void (async () => {
      const deals = await apiGet<CompanyDeals>(`/api/job-copy/company-deals?company=${encodeURIComponent(company.id)}`, { signal: controller.signal, timeoutMs: 120_000 });
      if (controller.signal.aborted) return;
      if (!deals.ok) { fail(); return; }
      const wanted = deals.data.deals.slice(0, ROLLUP_RULES.maxDeals);
      const loaded: DealData[] = [];
      let missing = 0;
      for (const deal of wanted) {
        const data = await loadDealData(deal.id, deal.name ?? '取引名未取得', controller.signal);
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- aborted can change while awaiting
        if (controller.signal.aborted) return;
        if (data === null) missing += 1; else if (data.rows.length) loaded.push(data);
      }
      const notes: string[] = [];
      if (deals.data.deals.length > wanted.length) notes.push(`取引が多いため、先頭の${String(wanted.length)}件までで集計しています。`);
      if (missing > 0) notes.push(`読み込めなかった取引が ${String(missing)}件あります。その取引の求人は合計に含まれていません。`);
      setSource(loaded.length === 0 && missing > 0 ? { status: 'error', message: 'この事業所の求人を読み込めませんでした。時間を置いて再取得してください。' } : { status: 'ready', datas: loaded, notes });
    })();
    return () => { controller.abort(); };
  }, [company.id]);
  return <SummaryView scope="company" source={source} billingByJob={billingByJob} onOpenListing={onOpenListing} />;
}
