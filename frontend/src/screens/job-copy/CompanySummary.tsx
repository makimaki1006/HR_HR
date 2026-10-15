import { useEffect, useState } from 'react';
import type { HubSpotListing } from './hubspotListings';
import { loadCompanyDatas } from './dealSummaryLoad';
import { SummaryView } from './DealSummary';
import type { Billing, SummarySource } from './DealSummary';

/** 事業所（会社）ひとつ分のまとめ。取引ごとの求人を集めて、同じ求人は1回だけ数える。 */
export function CompanySummary({ company, billingByJob, onOpenListing }: { company: { id: string; name: string }; billingByJob: Billing; onOpenListing: (row: HubSpotListing, opener?: HTMLElement) => void }) {
  const [source, setSource] = useState<SummarySource>({ status: 'loading' });
  useEffect(() => {
    const controller = new AbortController();
    void loadCompanyDatas(company.id, controller.signal).then(result => { if (!controller.signal.aborted) setSource(result); });
    return () => { controller.abort(); };
  }, [company.id]);
  return <SummaryView scope="company" source={source} billingByJob={billingByJob} onOpenListing={onOpenListing} />;
}
