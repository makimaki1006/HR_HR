import { useEffect, useRef, useState } from 'react';
import { AuthRequiredError } from '../../api/client';
import { CompanyListings } from './CompanyListings';
import { loadVersions } from './listingDetailCache';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import type { JobCopyRecord } from './data';
import type { Billing } from './DealSummary';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy } from './SnapshotErrorNotice';
import { loadCompanyDeals } from './companyDeals';
import type { CompanyDeals } from './companyDeals';
import './home.css';
import { ownerParams } from './OwnerTodo';
import { useJobCopySearch } from './navigation';

const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '読み込めませんでした。時間を置いて再取得してください。';

/** 顧客（事業所）の画面。掲載中と過去の求人。 */
export function CompanyView({ company, billingByJob, onName, onOpenDeal, onOpenListing, embedded = false }: {
  embedded?: boolean;
  company: { id: string; name: string };
  billingByJob: Billing;
  /** 名前が分かったら知らせる（URL から直接開いたとき、パンくずの名前になる）。 */
  onName: (name: string) => void;
  onOpenDeal: (deal: { id: string; name: string }) => void;
  onOpenListing: (job: JobCopyRecord) => void;
}) {
  const ownerQuery = ownerParams(useJobCopySearch()) || 'owner=me';
  const [deals, setDeals] = useState<CompanyDeals | null>(null);
  const [error, setError] = useState('');
  const [loadedScope, setLoadedScope] = useState('');
  const scope = `${company.id}|${ownerQuery}`;
  const [busy, setBusy] = useState(false);
  const action = useRef<AbortController | null>(null);
  const report = useRef(onName);
  useEffect(() => { report.current = onName; });
  useEffect(() => {
    const controller = new AbortController();
    void loadCompanyDeals(company.id).then(result => {
      if (controller.signal.aborted) return;
      setLoadedScope(scope);
      if (result.ok) { setDeals(result.data); setError(''); if (result.data.company.name) report.current(result.data.company.name); } else setError(failure(result.error));
    });
    return () => { controller.abort(); };
  }, [company.id, scope]);
  useEffect(() => () => { action.current?.abort(); }, []);
  const open = async (row: HubSpotListing) => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError('');
    const result = await loadVersions(row.id, { signal: controller.signal });
    if (controller.signal.aborted) return;
    if (result.ok) onOpenListing(listingRecord(result.data, row)); else setError(failure(result.error));
    setBusy(false);
  };
  const visibleDeals = loadedScope === scope ? deals : null;
  const Container = embedded ? 'div' : 'main';
  return <Container className="jc-detail jh-home" id={embedded ? undefined : "job-details"} tabIndex={-1}>
    <header className="jh-head"><div>{embedded ? <h3>{company.name || (visibleDeals?.company.name ?? '顧客')}</h3> : <h1>{company.name || (visibleDeals?.company.name ?? '顧客')}</h1>}</div></header>
    {error && loadedScope === scope && <p role="alert">{error}</p>}{busy && <p role="status">選んだ求人を読み込んでいます…</p>}
    {!visibleDeals && (!error || loadedScope !== scope) && <p role="status">求人を読み込んでいます…</p>}
    {visibleDeals && <CompanyListings companyId={company.id} deals={visibleDeals.deals} billingByJob={billingByJob} onOpenDeal={onOpenDeal} onOpenListing={row => { void open(row); }} />}
  </Container>;
}
