import { useEffect, useRef, useState } from 'react';
import { apiGet, AuthRequiredError } from '../../api/client';
import { CompanySummary } from './CompanySummary';
import { loadVersions } from './listingDetailCache';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import type { JobCopyRecord } from './data';
import type { Billing } from './DealSummary';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy } from './SnapshotErrorNotice';
import './home.css';

interface CompanyDeals { company: { id: string; name: string | null }; deals: { id: string; name: string | null }[] }
const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '読み込めませんでした。時間を置いて再取得してください。';

/** 顧客（事業所）の画面。案件の一覧と、求人の合計。 */
export function CompanyView({ company, billingByJob, onName, onOpenDeal, onOpenListing }: {
  company: { id: string; name: string };
  billingByJob: Billing;
  /** 名前が分かったら知らせる（URL から直接開いたとき、パンくずの名前になる）。 */
  onName: (name: string) => void;
  onOpenDeal: (deal: { id: string; name: string }) => void;
  onOpenListing: (job: JobCopyRecord) => void;
}) {
  const [deals, setDeals] = useState<CompanyDeals | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const action = useRef<AbortController | null>(null);
  const report = useRef(onName);
  useEffect(() => { report.current = onName; });
  useEffect(() => {
    const controller = new AbortController();
    void apiGet<CompanyDeals>(`/api/job-copy/company-deals?company=${encodeURIComponent(company.id)}`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok) { setDeals(result.data); setError(''); if (result.data.company.name) report.current(result.data.company.name); } else setError(failure(result.error));
    });
    return () => { controller.abort(); };
  }, [company.id]);
  useEffect(() => () => { action.current?.abort(); }, []);
  const open = async (row: HubSpotListing) => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError('');
    const result = await loadVersions(row.id, { signal: controller.signal });
    if (controller.signal.aborted) return;
    if (result.ok) onOpenListing(listingRecord(result.data, row)); else setError(failure(result.error));
    setBusy(false);
  };
  return <main className="jc-detail jh-home" id="job-details" tabIndex={-1}>
    <header className="jh-head"><div><h1>{company.name || '顧客'}</h1><p>この顧客の案件と求人です。案件を開くと、その案件の求人を比べられます。</p></div></header>
    {error && <p role="alert">{error}</p>}{busy && <p role="status">選んだ求人を読み込んでいます…</p>}
    <section aria-labelledby="jh-deals"><h2 id="jh-deals">案件</h2>
      {!deals && !error && <p role="status">案件を読み込んでいます…</p>}
      {deals && (deals.deals.length ? <ul className="jh-deals">{deals.deals.map(deal => <li key={deal.id}><button type="button" className="jc-button" onClick={() => { onOpenDeal({ id: deal.id, name: deal.name ?? '取引名未取得' }); }}>{deal.name ?? '取引名未取得'} ›</button></li>)}</ul> : <p>この顧客に案件はありません。</p>)}
    </section>
    <CompanySummary company={company} billingByJob={billingByJob} onOpenListing={row => { void open(row); }} />
  </main>;
}
